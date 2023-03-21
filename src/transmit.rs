use std::io;
use portaudio::{Blocking, Input, PortAudio, Stream};
use portaudio::stream::{Available, Buffer, InputSettings};

use std::net::UdpSocket;
use crate::vban::{VBanBitFormat, VBanPacket};

type BlockingStream<T> = Stream<Blocking<Buffer>, Input<T>>;

pub struct TransmitStream {
    stream_name: String,
    socket: UdpSocket
}

impl TransmitStream {
    pub fn new(stream_name: String, bind_ip: String) -> Result<TransmitStream, io::Error> {
        let new = TransmitStream {
            stream_name,
            socket: UdpSocket::bind(bind_ip)?
        };

        Ok(new)
    }

    pub fn do_send(&mut self, pa: PortAudio, transmit_ip: String) {
        println!("connecting to {transmit_ip}");
        self.socket.connect(transmit_ip)
            .expect("couldn't connect");

        println!("ok");

        let def_out = pa.default_input_device().expect("no default audio input device");
        let out_info = pa.device_info(def_out).unwrap();

        let stream: BlockingStream<i16> = Self::create_stream(
            &pa,
            out_info.default_sample_rate,
            out_info.max_input_channels as u8
        );

        /* make the stream name */
        let mut seq: u32 = 1;
        let mut stream_name_bytes = [0u8; 16];
        let max = 14.min(self.stream_name.len());
        for (index, c) in self.stream_name[0..max].bytes().enumerate() {
            stream_name_bytes[index] = c;
        }

        loop {
            let packet;
            let read_frames;
            if let Ok(Available::Frames(available_frames)) = stream.read_available() {
                read_frames = available_frames;
            } else { continue }

            let frames_to_write = 256.min(read_frames as usize);

            if frames_to_write == 0 {
                continue
            }

            let res = stream.read(frames_to_write.try_into().unwrap());
            let audio_data;

            if let Ok(data) = res {
                audio_data = data;
            } else {
                continue
            }

            /* convert i16 sample data to array of bytes. */
            let datau8: Vec<u8> = audio_data.iter()
                .map(|x| x.to_le_bytes())
                .flatten()
                .collect();

            packet = VBanPacket::make_audio_packet(
                stream.info().sample_rate as i32,
                stream_name_bytes,
                frames_to_write as u16 * out_info.max_input_channels as u16,
                out_info.max_input_channels as u8,
                datau8.as_slice(),
                VBanBitFormat::I16,
                seq
            );

            seq += 1;

            /* push the data */
            self.socket.send(&packet).expect("couldn't send audio packet");
        }
    }

    fn create_stream(pa: &PortAudio, sample_rate: f64, chans: u8) -> BlockingStream<i16> {
        let in_sett: InputSettings<i16> = pa.default_input_stream_settings(chans as i32, sample_rate, 256)
            .expect("it was not possible to initialize stream settings");
        let mut s = pa.open_blocking_stream(in_sett)
            .expect("couldn't open the stream");
        s.start()
            .expect("couldn't start the stream");
        s
    }
}