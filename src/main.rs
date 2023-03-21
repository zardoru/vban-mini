mod vban;
mod recv;
mod transmit;

extern crate portaudio;

use clap::Parser;
use portaudio::PortAudio;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Stream name
    #[arg(short, long, default_value = "Stream1")]
    stream_name: String,

    #[arg(short, long, default_value = "0.0.0.0:6980")]
    bind_ip: String,

    #[arg(short, long)]
    transmit_ip: Option<String>
}


// windows (portaudio specifically) needs user32 for some reason.
#[cfg_attr(target_os = "windows", link(name = "user32"))]
extern {}

fn main() {
    let args = Args::parse();

    println!("vban-mini started");
    println!("bind ip: {}", args.bind_ip);

    let pa = PortAudio::new().expect("couldn't open portaudio");

    if args.transmit_ip.is_none() {
        let def_out = pa.default_output_device().expect("no default audio output device");
        let out_info = pa.device_info(def_out).unwrap();

        println!("receive mode: here's the device we're opening up: {out_info:#?}");

        let mut s = recv::ReceiveStream::new(args.stream_name, args.bind_ip);
        match s {
            Ok(ref mut rs) => {
                rs.do_receive(&pa);
            }
            Err(ref e) => {
                panic!("{}", e)
            }
        }
    } else {
        let def_out = pa.default_input_device().expect("no default audio input device");
        let out_info = pa.device_info(def_out).unwrap();

        println!("transmit mode: here's the device we're opening up: {out_info:#?}");

        let mut s = transmit::TransmitStream::new(args.stream_name, args.bind_ip);
        match s {
            Ok(ref mut ts) => {
                ts.do_send(pa, args.transmit_ip.unwrap());
            }
            Err(ref e) => {
                panic!("{}", e)
            }
        }
    }
}
