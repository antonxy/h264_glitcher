use h264_glitcher::h264::*;
use h264_glitcher::beat_predictor::BeatTap;
use h264_glitcher::osc_var::{LoopRange, OscValue};
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
use rosc::{OscPacket, OscMessage, encoder, OscType};
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

    #[structopt(long, default_value = "http://[::1]:3000/", help="Thumbnail server base url")]
    thumbnail_server_base_url: String,

    #[structopt(long, default_value = "[::]:3000", help="Thumbnail server listen address")]
    thumbnail_server_listen_addr: String,

    #[structopt(long, parse(from_os_str), help="Mask image for embedded player (indexed or grayscale PNG)")]
    mask: Option<PathBuf>,

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

    let base_url = PathBuf::from(&opt.thumbnail_server_base_url);
    let thumbnail_urls : Vec<String> = relative_paths.iter().map(|p| {
         append_extension(&base_url.join(p), "png").to_str().unwrap().to_string()
    }).collect();

    thread::spawn({
        let listen_addr = opt.thumbnail_server_listen_addr.clone();
        move || {
            h264_glitcher::thumbnail_server::serve(&thumbnail_path, &listen_addr);
        }
    });


    let streaming_params = Arc::new(Mutex::new(StreamingParams::default()));

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
        move || {
        video_name_sender(send_sock, streaming_params, paths, thumbnail_urls);
    }});


    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let paths = paths.clone();
        move || {
        osc_listener(send_sock, &addr, streaming_params, paths);
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

    // Run the player (blocks)
    let mask_path = opt.mask.clone();
    player::run(pipe_reader, mask_path.as_deref(), streaming_params);

    Ok(())
}


const PALETTE : &'static [&'static str] = &["#EF476F", "#FFD166", "#06D6A0", "#118AB2", "#aa1d97"];

fn video_name_sender(send_sock: Arc<Mutex<UdpSocket>>, streaming_params: Arc<Mutex<StreamingParams>>, paths: Vec<PathBuf>, thumbnails: Vec<String>) {

    loop {
        let params = streaming_params.lock().unwrap().clone();
        if let Some(client_addr) = params.client_addr {
            // Send video labels
            let mut last_dir = None;
            let mut color_idx = 0;
            for (i, path) in paths.iter().enumerate() {
                let dir = path.parent();
                let filename = path.file_stem().unwrap().to_str().unwrap().to_string();

                // Select new color per directory
                if last_dir != dir {
                    last_dir = dir;
                    color_idx = (color_idx + 1) % PALETTE.len();
                }
                let color = PALETTE[color_idx];

                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                    addr: format!("/label_{}", i).to_string(),
                    args: filename.to_args(),
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();

                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                    addr: format!("/label_{}/color", i).to_string(),
                    args: color.to_string().to_args(),
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            }

            for (j, thumbnail) in thumbnails.iter().enumerate() {
                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                    addr: format!("/thumbnail_{}", j).to_string(),
                    args: thumbnail.to_string().to_args(),
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            }

            streaming_params.lock().unwrap().send_changed(&send_sock.lock().unwrap(), &client_addr);
        }
        std::thread::sleep(Duration::from_millis(1000));
    }

}

fn osc_listener(send_sock: Arc<Mutex<UdpSocket>>, addr: &SocketAddr, streaming_params: Arc<Mutex<StreamingParams>>, paths: Vec<PathBuf>) {
    let sock = UdpSocket::bind(addr).unwrap();
    eprintln!("OSC: Listening to {}", addr);

    let video_index: HashMap<String, i32> = paths.iter().enumerate().map(|(i, p)| (p.file_stem().unwrap().to_string_lossy().into_owned(), i as i32)).collect();

    let mut buf = [0u8; rosc::decoder::MTU];

    let mut beat_tap = BeatTap::new();

    let mut parse_message = |msg: &OscMessage, params: &mut StreamingParams, client_addr: SocketAddr| -> Result<(), ()> {
        if params.handle_osc_message(&msg) {}
        else {
            match msg.addr.as_str() {
                "/set_client_address" => {
                    params.client_addr = Some(client_addr);
                    eprintln!("updated client_addr: {:?}", params.client_addr);
                },
                "/record_loop" => {
                    let record_loop = msg.args[0].clone().bool().ok_or(())?;
                    if record_loop {
                        let from = *params.active_state().playhead;
                        let (_, to) = params.active_state().loop_range.0.unwrap_or((0.0, 1.0));
                        params.active_state_mut().loop_range.set(LoopRange(Some((from, to))));
                    } else {
                        let (from, _) = params.active_state().loop_range.0.unwrap_or((0.0, 1.0));
                        let to = *params.active_state().playhead;
                        params.active_state_mut().loop_range.set(LoopRange(Some((from, to))));
                    }
                },
                "/clear_loop" => {
                    params.edit_state_mut().loop_range.set(LoopRange(None));
                }
                "/cut_loop" => {
                    let loop_range = &mut params.edit_state_mut().loop_range;
                    let range = loop_range.0.unwrap_or((0.0, 1.0));
                    let new_range = (range.0, range.0 + (range.1 - range.0) * msg.args[0].clone().float().ok_or(())?);
                    loop_range.set(LoopRange(Some(new_range)));
                },
                "/skip_frames" => {
                    params.skip_frames = Some(msg.args[0].clone().int().ok_or(())? as usize);
                },
                "/video" => {
                    match &msg.args.as_slice() {
                        &[OscType::Int(idx)] => params.state.video_num.set(*idx),
                        &[OscType::String(name)] => {
                            match video_index.get(name) {
                                Some(idx) => params.state.video_num.set(*idx),
                                None => eprintln!("/video received unknown video name"),
                            }
                        },
                        _ => eprintln!("/video received unknown message type")
                    }
                    params.restart_loop = true;
                },
                "/short_loop" => {
                    let loop_len = msg.args[0].clone().int().ok_or(())?;
                    if loop_len > 0 {
                        params.short_loop = Some(ShortLoop {
                            first_frame: None,
                            len: loop_len as usize,
                        })
                    } else {
                        params.short_loop = None;
                    }
                }
                "/manual_beat" => {
                    if let Some(beat_grid) = beat_tap.tap() {
                        params.beat_grid = beat_grid;
                    }
                },
                "/reset" => {
                    *params.edit_state_mut() = State::default();
                    params.edit_state_mut().set_changed();
                },
                _ => {
                    eprintln!("Unhandled OSC address: {}", msg.addr);
                    eprintln!("Unhandled OSC arguments: {:?}", msg.args);
                    return Err(());
                }
            }
        }
        Ok(())
    };

    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, client_addr)) => {
                let (_, packet) = rosc::decoder::decode_udp(&buf[..size]).unwrap();
                let mut params = streaming_params.lock().unwrap();
                let parse_result = match packet {
                    OscPacket::Message(ref msg) => {
                        parse_message(&msg, &mut params, client_addr)
                    }
                    OscPacket::Bundle(_) => {
                        eprintln!("Received bundle but they are currently not handled");
                        Err(())
                    }
                };

                if parse_result.is_err() {
                    eprintln!("Failed to parse OSC Packet: {:?}", packet);
                }


                if let Some(addr) = params.client_addr {
                    params.send_changed(&send_sock.lock().unwrap(), &addr);
                }

            }
            Err(e) => {
                eprintln!("Error receiving from socket: {}", e);
                break;
            }
        }
    }

}
