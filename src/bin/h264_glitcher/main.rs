use h264_glitcher::h264::*;
use h264_glitcher::beat_predictor::BeatTap;
use h264_glitcher::osc_handler::OSCHandler;
use h264_glitcher::osc_var::LoopRange;
use h264_glitcher::sigma_delta::SigmaDelta;
use h264_glitcher::player;
use h264_glitcher::state::{State, ShortLoop, StreamingParams};
// use h264_glitcher::push2;

extern crate structopt;
mod stream_loop;

use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use std::vec::Vec;
use structopt::StructOpt;
use std::thread;
use std::sync::{Mutex, Arc};
use std::net::{SocketAddr, UdpSocket};
use rosc::{OscPacket, OscMessage, OscType};
use walkdir::WalkDir;

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "h264_glitcher", about = "Live controllable h264 glitcher.",
            long_about = "Pipe output into mpv (or any other capable player).")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), required=true, help="Input video directory. Expects a subdirectory \"encoded\" with the raw h264 streams and a subdirectory \"thumbnails\" with a thumbnail for each stream.")]
    input_dir: PathBuf,

    #[structopt(short = "l", long, default_value = "127.0.0.1:8000", help="OSC listen address")]
    listen_addr: String,
    #[structopt(short = "s", long, default_value = "0.0.0.0:0", help="OSC send address (port = 0 -> choose port automatically)")]
    send_addr: String,

    #[structopt(long, help="Load and parse all videos into memory")]
    prefetch: bool,

    #[cfg(feature="web_server")]
    #[structopt(long, default_value = "[::]:3000", help="Web server listen address")]
    web_server_listen_addr: String,

    #[structopt(long, parse(from_os_str), help="Mask image for embedded player (indexed or grayscale PNG)")]
    mask: Option<PathBuf>,

    #[cfg(feature="push2")]
    #[structopt(long, help="Enable Push 2 controller")]
    push2: bool,
}


fn append_extension<S: AsRef<std::ffi::OsStr>>(path: &std::path::Path, extension: S) -> PathBuf {
    let mut full_extension = std::ffi::OsString::new();
    if let Some(ext) = path.extension() {
        full_extension.push(ext);
        full_extension.push(".");
    }
    full_extension.push(extension);
    path.with_extension(full_extension)
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();

    let encoded_path = opt.input_dir.join("encoded");
    let thumbnail_path = opt.input_dir.join("thumbnails");
    let mut relative_paths : Vec<PathBuf> = WalkDir::new(&encoded_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| p.extension().unwrap_or(std::ffi::OsStr::new("")) == "h264")
        .map(|p| p.strip_prefix(&encoded_path).unwrap().with_extension("").to_path_buf())
        .collect();

    relative_paths.sort();

    let paths : Vec<PathBuf> = relative_paths.iter().map(|p| append_extension(&encoded_path.join(p), "h264")).collect();

    // Check if all video files can be opened
    for path in &paths {
        File::open(path)?;
    }

    let streaming_params = Arc::new(Mutex::new(StreamingParams::default()));

    let video_index: HashMap<String, i32> = paths.iter().enumerate().map(|(i, p)| (p.file_stem().unwrap().to_string_lossy().into_owned(), i as i32)).collect();
    let osc_handler = OSCHandler {
        video_index,
        streaming_params: streaming_params.clone(),
    };

    #[cfg(feature="web_server")]
    {
        thread::spawn({
            let listen_addr = opt.web_server_listen_addr.clone();
            let paths = paths.clone();
            let osc_handler = osc_handler.clone();
            move || {
                h264_glitcher::web_server::serve(&thumbnail_path, &listen_addr, paths, osc_handler);
            }
        });
    }

    // Run OSC listener
    let addr = match SocketAddr::from_str(&opt.listen_addr) {
        Ok(addr) => addr,
        Err(_) => panic!("Invalid listen_addr"),
    };
    let send_from_addr = match SocketAddr::from_str(&opt.send_addr) {
        Ok(addr) => addr,
        Err(_) => panic!("Invalid send_addr"),
    };

    let send_sock = UdpSocket::bind(send_from_addr).unwrap();
    let send_sock = Arc::new(Mutex::new(send_sock));


    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let paths = paths.clone();
        let osc_handler = osc_handler.clone();
        move || {
        osc_listener(send_sock, &addr, osc_handler);
    }});

    // Create pipe to connect stream loop to player
    let (pipe_reader, pipe_writer) = pipe::pipe_buffered();

    stream_loop::start_stream_thread(
        paths,
        opt.clone(),
        &streaming_params,
        pipe_writer,
    );

    // if opt.push2 {
    //     let streaming_params = streaming_params.clone();
    //     thread::spawn(move || {
    //         if let Err(e) = push2::run(streaming_params) {
    //             eprintln!("Push 2 error: {}", e);
    //         }
    //     });
    // }

    // thread::sleep(Duration::from_hours(100));


    // Run the player (blocks)
    let mask_path = opt.mask.clone();
    player::run(pipe_reader, mask_path.as_deref(), streaming_params);

    Ok(())
}

fn osc_listener(send_sock: Arc<Mutex<UdpSocket>>, addr: &SocketAddr, osc_handler: OSCHandler) {
    let sock = UdpSocket::bind(addr).unwrap();
    eprintln!("OSC: Listening to {}", addr);

    let mut buf = [0u8; rosc::decoder::MTU];

    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, client_addr)) => {
                let (_, packet) = rosc::decoder::decode_udp(&buf[..size]).unwrap();
                let parse_result = match packet {
                    OscPacket::Message(ref msg) => {
                        osc_handler.handle_osc_message(&msg)
                    }
                    OscPacket::Bundle(_) => {
                        eprintln!("Received bundle but they are currently not handled");
                        Err(())
                    }
                };

                if parse_result.is_err() {
                    eprintln!("Failed to parse OSC Packet: {:?}", packet);
                }


                // if let Some(addr) = params.client_addr {
                //     params.send_changed(&send_sock.lock().unwrap(), &addr);
                // }

            }
            Err(e) => {
                eprintln!("Error receiving from socket: {}", e);
                break;
            }
        }
    }

}
