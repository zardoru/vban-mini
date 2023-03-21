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

    let pa = PortAudio::new().expect("couldn't open portaudio");

    if args.transmit_ip.is_none() {
        recv::do_receive(args.bind_ip, args.stream_name, pa);
    } else {
        transmit::do_send(args.bind_ip, args.transmit_ip.unwrap(), args.stream_name, pa);
    }
}
