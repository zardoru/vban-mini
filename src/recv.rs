use std::collections::HashSet;
use std::{io, thread};
use portaudio::{Blocking, Output, PortAudio, Stream};
use portaudio::stream::{Buffer, OutputSettings};

use std::io::Cursor;
use std::net::UdpSocket;
use std::time::Duration;
use byteorder::{ReadBytesExt, LittleEndian};
use crate::vban;
use crate::vban::VBAN_MAX_PACKET_SIZE;

type BlockingStream<T> = Stream<Blocking<Buffer>, Output<T>>;

const MAX_CACHED_PACKETS: usize = 32;

pub struct ReceiveStream {
    stream_name: String,
    pending_frames: Vec<(vban::VBanPacket, Vec<u8>)>,
    seen_stream_names: HashSet<String>,
    sequential_last_received_seq: u32,
    last_played_seq: u32,
    socket: UdpSocket,
    stream: Option<BlockingStream<i16>>,
    last_frames_pushed: u8
}

impl ReceiveStream {
    pub fn new(stream_name: String, bind_ip: String) -> Result<ReceiveStream, io::Error> {
        let new = ReceiveStream {
            stream_name,
            pending_frames: Vec::with_capacity(MAX_CACHED_PACKETS),
            seen_stream_names: HashSet::<String>::new(),
            sequential_last_received_seq: 0,
            last_played_seq: 0,
            socket:  UdpSocket::bind(bind_ip)?,
            stream: None,
            last_frames_pushed: 0
        };

        new.socket.set_nonblocking(true)?;
        Ok(new)
    }

    fn get_network_data(&mut self) {
        let mut buffer = [0u8; VBAN_MAX_PACKET_SIZE];
        let (byte_cnt, _) = match self.socket.recv_from(&mut buffer) {
            Ok((b, f)) => (b, f),
            Err(ref _e) => {
                /* rest for a bit before pushing new data */
                if let Some(s) = &self.stream {
                    let secs = (self.last_frames_pushed as f32) / s.info().sample_rate as f32 * 0.5;
                    let d = Duration::from_secs_f32(secs);
                    thread::sleep(d);
                }

                return
            }
        };

        /* packet is not vban -- too small */
        if byte_cnt < vban::HEADER_SIZE {
            return
        }

        let (bytes_header, bytes_data) = (&buffer[..byte_cnt]).split_at(vban::HEADER_SIZE);
        let header = vban::VBanPacket::from_bytes(&bytes_header.try_into().unwrap());

        // skip this not vban package
        if !header.is_vban() || !header.is_pcm() || !header.is_audio() {
            return
        }

        // skip if it's not a stream we expect
        let packet_stream_name = header.get_stream_name();
        if self.stream_name != packet_stream_name {
            if !self.seen_stream_names.contains(&packet_stream_name) {
                eprintln!("received data from stream with different name from ours: '{packet_stream_name}' != '{0}'", self.stream_name);
                self.seen_stream_names.insert(packet_stream_name);
            }
            return
        }

        /* is it the right format? */
        assert_eq!(header.get_bit_format(), vban::VBanBitFormat::I16, "non 16 bit signed audio NOT SUPPORTED");

        /* do the frame ordering bookkeeping */
        let this_seq = header.get_seq_num();

        self.sequential_last_received_seq = this_seq.max(self.sequential_last_received_seq);
        self.pending_frames.push((header, bytes_data.to_vec()));
    }

    pub fn do_receive(&mut self, pa: &PortAudio) {
        loop {
            self.get_network_data();

            /* make the packet sequentially consistent... */
            let mut next_frame_found = false;
            let mut audio_data: Option<&Vec<u8>> = None;
            let mut header: Option<&vban::VBanPacket> = None;
            for (head, data) in self.pending_frames.iter() {
                let delta = head.get_seq_num() as i32 - self.last_played_seq as i32;
                if delta == 1 || self.last_played_seq == 0 {
                    header = Some(head);
                    audio_data = Some(data);
                    next_frame_found = true;
                    break;
                }
            }

            if !next_frame_found {
                /* dropped packet heruistic */
                let mut delta_min = i32::MAX;
                if self.pending_frames.len() >= MAX_CACHED_PACKETS {
                    /* use packet closest to last played */

                    for (head, data) in self.pending_frames.iter() {
                        let delta = head.get_seq_num() as i32 - self.last_played_seq as i32;
                        if delta > 0 && /* packet comes afterwards and */
                            delta < delta_min /* is the closest to the last played */ {
                            header = Some(head);
                            audio_data = Some(data);
                            delta_min = delta
                        }
                    }
                } else {
                    /* we can still wait for a little longer */
                    continue;
                }
            }

            if header.is_none() || audio_data.is_none() {
                /* should never happen */
                continue;
            }

            let uheader = header.unwrap();
            self.last_played_seq = uheader.get_seq_num();

            /* create a stream if necessary */
            if self.stream.is_none()
            {
                let sr_opt = uheader.get_sr();
                let chans = uheader.get_channels();
                if sr_opt.is_ok() {
                    let sr = sr_opt.unwrap();
                    self.stream = Self::create_stream(&pa, sr as f64, chans)
                }
            }

            /* recreate stream if necessary */
            if let Some(s) = &mut self.stream {
                let sr = uheader.get_sr().unwrap();
                if s.info().sample_rate != sr as f64 {
                    self.stream = Self::create_stream(&pa, sr as f64, uheader.get_channels())
                }
            }

            /* push the data */
            let sr = uheader.get_sr().unwrap() as f32;
            let frames = uheader.get_frame_count();
            let max_smp = frames as usize * uheader.get_channels() as usize;
            let max_bytes = max_smp * std::mem::size_of::<i16>();
            let aud = audio_data.unwrap();

            let recv_len = aud.len();
            if max_bytes < recv_len {
                eprintln!("received bytes ({recv_len}) greater than number of samples specified by header ({max_bytes})")
            }

            if let Some(s) = &mut self.stream {
                if let Err(e) = Self::push_audio_buffer(&aud[0..max_bytes], s) {
                    eprintln!("error in output stream: '{}' carrying on.", e)
                }
            }

            /* discard all that came before */
            self.drop_old_packets();
            self.last_frames_pushed = frames;
        };
    }

    fn drop_old_packets(&mut self) {
        let new_pending_frames: Vec<(vban::VBanPacket, Vec<u8>)> = self.pending_frames
            .iter()
            .filter(|p| p.0.get_seq_num() > self.last_played_seq)
            .cloned()
            .collect();

        self.pending_frames = new_pending_frames;
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
}