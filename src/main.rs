mod vban;

extern crate portaudio;

use std::env::var;
use std::io::Cursor;
use std::net::UdpSocket;
use portaudio::{Blocking, Output, PortAudio, Stream};
use portaudio::stream::{Buffer, OutputSettings};
use byteorder::{ReadBytesExt, LittleEndian};
use crate::vban::HEADER_SIZE;

type BlockingStream<T> = Stream<Blocking<Buffer>, Output<T>>;

fn do_network(ip: String) {
    println!("attempt to bind to {ip}");
    let socket = UdpSocket::bind(ip)
        .expect("couldn't bind to ip and port");

    println!("ok");

    // max size of udp contents given an MTU of 1500
    let mut buffer: [u8; 1472] = [0; 1472];

    let pa = PortAudio::new().expect("couldn't open portaudio");
    let def_out = pa.default_output_device().expect("no default audio output device");
    let out_info = pa.device_info(def_out).unwrap();

    println!("here's the device we're opening up: {out_info:#?}");

    let mut stream: Option<BlockingStream<i16>> = None;

    let mut seq: u32 = 0;
    loop {
        let (byte_cnt, _) = socket.recv_from(&mut buffer)
            .expect("couldn't receive");

        if byte_cnt < HEADER_SIZE {
            continue
        }

        let (bytes_header, bytes_data) = (&buffer[..byte_cnt]).split_at(HEADER_SIZE);
        let header = vban::VBanPacket::from_bytes(&bytes_header.try_into().unwrap());

        // skip this not vban package
        if !header.is_vban() || !header.is_pcm() || !header.is_audio() { continue }

        let this_seq = header.get_seq_num();

        if this_seq < seq {
            eprintln!("out of order packet received");
            continue
        } else {
            let old_seq = seq;
            let gap = this_seq - seq;
            seq = this_seq;

            if gap > 1 && old_seq != 0 {
                eprintln!("frame was dropped")
            }
        }

        assert_eq!(header.get_bit_format(), vban::VBanBitFormat::I16, "non 16 bit signed audio NOT SUPPORTED");

        if stream.is_none() {
            let sr_opt = header.get_sr();
            let chans = header.get_channels();
            if sr_opt.is_ok() {
                let sr = sr_opt.unwrap();
                stream = create_stream(&pa, sr as f64, chans)
            }
        }

        if stream.is_some() {
            let mut s = stream.unwrap();

            if let Err(e) = push_audio_buffer(bytes_data, &mut s) {
                eprintln!("error in output stream: '{}' carrying on.", e)
            }

            // rust is funny
            stream = Some(s)
        }
    }
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


// windows needs user32 for some reason.
#[cfg_attr(target_os = "windows", link(name = "user32"))]
extern {}

fn main() {
    println!("vban-recv (wyrmin) started.");

    let port: String = var("BIND_IP")
        // .unwrap_or(String::from("127.0.0.1:6980"))
        .unwrap_or(String::from("0.0.0.0:6980"));

    do_network(port);
}
