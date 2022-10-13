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

    println!("cool we're bound");

    // max size of udp contents given an MTU of 1500
    let mut buffer: [u8; 1472] = [0; 1472];

    let pa = PortAudio::new().expect("couldn't open portaudio");
    let def_out = pa.default_output_device().expect("we need an output device");
    let out_info = pa.device_info(def_out).unwrap();

    println!("here's the device we're opening up: {out_info:#?}");

    let mut stream: Option<BlockingStream<i16>> = None;

    loop {
        let (byte_cnt, _) = socket.recv_from(&mut buffer)
            .expect("couldn't receive");

        if byte_cnt < HEADER_SIZE {
            continue
        }

        let (bytes_header, bytes_data) = (&buffer[..byte_cnt]).split_at(HEADER_SIZE);
        let header = vban::VBanPacket::from_bytes(&bytes_header.try_into().unwrap());

        // skip this not vban package
        if !header.is_vban() || !header.is_pcm() || !header.is_audio() { continue; }

        assert_eq!(header.get_bit_format(), vban::VBanBitFormat::I16, "non 16 bit signed audio NOT SUPPORTED");
        assert_eq!(header.get_sr(), Ok(48000), "ONLY 48KHZ");

        if stream.is_none() {
            stream = create_stream(&pa);
        }

        if stream.is_some() {
            let mut s = stream.unwrap();

            if let Err(e) = push_audio_buffer(bytes_data, &mut s) {
                println!("error in output stream: '{}' carrying on.", e)
            }

            // rust is funny
            stream = Some(s);
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

fn create_stream(pa: &PortAudio) -> Option<Stream<Blocking<Buffer>, Output<i16>>> {
    let out_sett: OutputSettings<i16> = pa.default_output_stream_settings(2, 48000.0, 1024)
        .expect("okay what the fuck");
    let mut s = pa.open_blocking_stream(out_sett).expect("woohooo come on");
    s.start().expect("we NEED a stream??? COME ON WHY NOT");
    Some(s)
}


// why. rust. you were the chosen one.
#[link(name = "user32")]
extern {}

fn main() {
    println!("Hello, world!");

    let port: String = var("BIND_IP")
        // .unwrap_or(String::from("127.0.0.1:6980"))
        .unwrap_or(String::from("0.0.0.0:6980"));

    do_network(port);
}
