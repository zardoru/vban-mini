use std::collections::HashSet;
use portaudio::{Blocking, Output, PortAudio, Stream};
use portaudio::stream::{Buffer, OutputSettings};

use std::io::Cursor;
use std::net::UdpSocket;
use byteorder::{ReadBytesExt, LittleEndian};
use crate::vban;
use crate::vban::VBAN_MAX_PACKET_SIZE;

type BlockingStream<T> = Stream<Blocking<Buffer>, Output<T>>;

const MAX_CACHED_PACKETS: usize = 32;

pub fn do_receive(ip: String, stream_name: String, pa: PortAudio) {
    println!("(receive mode) attempt to bind to {ip}");
    let socket = UdpSocket::bind(ip)
        .expect("couldn't bind to ip and port");

    println!("ok");

    let mut buffer = [0u8; VBAN_MAX_PACKET_SIZE];

    let def_out = pa.default_output_device().expect("no default audio output device");
    let out_info = pa.device_info(def_out).unwrap();

    println!("here's the device we're opening up: {out_info:#?}");

    let mut stream: Option<BlockingStream<i16>> = None;

    let mut sequential_last_received_seq = 0u32;
    let mut last_played_seq = 0u32;
    let mut seen_stream_names = HashSet::<String>::new();
    let mut pending_frames:Vec<(vban::VBanPacket, Vec<u8>)> = Vec::with_capacity(MAX_CACHED_PACKETS);
    loop {
        let (byte_cnt, _) = socket.recv_from(&mut buffer)
            .expect("couldn't receive");

        if byte_cnt < vban::HEADER_SIZE {
            continue
        }

        let (bytes_header, mut bytes_data) = (&buffer[..byte_cnt]).split_at(vban::HEADER_SIZE);
        let mut header = vban::VBanPacket::from_bytes(&bytes_header.try_into().unwrap());

        // skip this not vban package
        if !header.is_vban() || !header.is_pcm() || !header.is_audio() { continue }

        // skip if it's not a stream we expect
        let packet_stream_name = header.get_stream_name();
        if stream_name.ne(&packet_stream_name) {
            if !seen_stream_names.contains(&packet_stream_name) {
                eprintln!("received data from stream with different name from ours: '{packet_stream_name}' != '{stream_name}'");
                seen_stream_names.insert(packet_stream_name);
            }
            continue
        }

        /* is it the right format? */
        assert_eq!(header.get_bit_format(), vban::VBanBitFormat::I16, "non 16 bit signed audio NOT SUPPORTED");

        /* do the frame ordering logic */
        let this_seq = header.get_seq_num();

        sequential_last_received_seq = this_seq.max(sequential_last_received_seq);
        pending_frames.push((header, bytes_data.to_vec()));

        /* make the packet sequentially consistent... */
        let mut next_frame_found = false;
        for (head, data) in pending_frames.iter() {
            let delta = head.get_seq_num() as i32 - last_played_seq as i32;
            if delta == 1 || last_played_seq == 0 {
                header = *head;
                bytes_data = data;
                next_frame_found = true;
                break
            }
        }

        if !next_frame_found {
            /* dropped packet heruistic */
            let mut delta_min = i32::MAX;
            if pending_frames.len() >= MAX_CACHED_PACKETS {
                /* use packet closest to last played */
                for (head, data) in pending_frames.iter() {
                    let delta = head.get_seq_num() as i32 - last_played_seq as i32;
                    if delta > 0 && /* packet comes afterwards and */
                        delta < delta_min /* is the closest to the last played */ {
                        header = *head;
                        bytes_data = data;
                        delta_min = delta
                    }
                }
            } else {
                /* we can still wait for a little longer */
                continue
            }
        }

        last_played_seq = header.get_seq_num();

        /* create a stream if necessary */
        if stream.is_none()
        {
            let sr_opt = header.get_sr();
            let chans = header.get_channels();
            if sr_opt.is_ok() {
                let sr = sr_opt.unwrap();
                stream = create_stream(&pa, sr as f64, chans)
            }
        }

        /* recreate stream if necessary */
        if let Some(s) = &mut stream {
            let sr = header.get_sr().unwrap();
            if s.info().sample_rate != sr as f64 {
                stream = create_stream(&pa, sr as f64, header.get_channels())
            }
        }

        /* push the data */
        if let Some(s) = &mut stream {
            let max_smp = header.get_frame_count() as usize * header.get_channels() as usize;
            let max_bytes = max_smp * std::mem::size_of::<i16>();

            if max_bytes < bytes_data.len() {
                let recv_len = bytes_data.len();
                eprintln!("received bytes ({recv_len}) greater than number of samples specified by header ({max_bytes})")
            }

            if let Err(e) = push_audio_buffer(&bytes_data[0..max_bytes], s) {
                eprintln!("error in output stream: '{}' carrying on.", e)
            }
        }

        /* discard all that came before */
        let new_pending_frames: Vec<(vban::VBanPacket, Vec<u8>)> = pending_frames
            .iter()
            .filter(|p| p.0.get_seq_num() > last_played_seq)
            .cloned()
            .collect();

        pending_frames = new_pending_frames;
    };
}

fn push_audio_buffer(data: &[u8], s: &mut BlockingStream<i16>) -> Result<(), portaudio::Error> {
    let samp_cnt = data.len() as u32 / 2; // i16 frames,
    let frame_cnt = samp_cnt / 2;
    s.write(frame_cnt, |out| {
        let mut cursor = Cursor::new(data);
        for i in 0..out.len() {
            out[i as usize] = cursor.read_i16::<LittleEndian>().unwrap();
        }
    })
}

fn create_stream(pa: &PortAudio, sample_rate: f64, chans: u8) -> Option<Stream<Blocking<Buffer>, Output<i16>>> {
    let out_sett: OutputSettings<i16> = pa.default_output_stream_settings(chans as i32, sample_rate, 1024)
        .expect("it was not possible to initialize stream settings");
    let mut s = pa.open_blocking_stream(out_sett)
        .expect("couldn't open the stream");
    s.start()
        .expect("couldn't start the stream");
    Some(s)
}