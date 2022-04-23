pub(crate) mod h264;
pub(crate) mod nal_iterator;
pub(crate) mod parse_nal;
pub mod libh264bitstream;

use crate::parse_nal::H264Parser;
use crate::nal_iterator::NalIterator;

extern crate structopt;

use h264::FrameType;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::vec::Vec;
use structopt::StructOpt;
use std::thread;
use std::sync::{Mutex, Arc};
use std::net::{SocketAddr, UdpSocket};
use rosc::{OscPacket, OscMessage, encoder, OscType};


#[derive(Debug, StructOpt)]
#[structopt(name = "h264_glitcher", about = "Live controllable h264 glitcher.",
            long_about = "Pipe output into 'mpv --untimed --no-cache -'.")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), required=true, help="Input video file(s). Directories will be ignored.")]
    input: Vec<PathBuf>,

    #[structopt(short = "l", long, default_value = "0.0.0.0:8000", help="OSC listen address")]
    listen_addr: String,
    #[structopt(short = "l", long, default_value = "0.0.0.0:8001", help="OSC send address")]
    send_addr: String,
}


#[derive(Clone)]
struct StreamingParams {
    fps: f32,
    record_loop: bool,
    play_loop: bool,
    pass_iframe: bool,
    video_num: usize,
    client_addr: Option<SocketAddr>,
}

impl Default for StreamingParams {
    fn default() -> Self {
        Self {
            fps: 30.0,
            record_loop: false,
            play_loop: false,
            pass_iframe: false,
            video_num: 0,
            client_addr: None,
        }
    }
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();
    eprintln!("{:?}", opt);

    let paths : Vec<PathBuf> = opt.input.into_iter().filter(|p| p.is_file()).collect();
    // Check if all files can be opened
    for path in &paths {
        File::open(path)?;
    }

    let streaming_params = Arc::new(Mutex::new(StreamingParams::default()));

    // Run OSC listener
    let addr = match SocketAddr::from_str(&opt.listen_addr) {
        Ok(addr) => addr,
        Err(_) => panic!("Invalid listen_addr"),
    };
    let send_from_addr = match SocketAddr::from_str(&opt.send_addr) {
        Ok(addr) => addr,
        Err(_) => panic!("Invalid listen_addr"),
    };
    let streaming_params_cpy = streaming_params.clone();
    thread::spawn(move || {
        osc_listener(&addr, streaming_params_cpy);
    });

    let send_sock = UdpSocket::bind(send_from_addr).unwrap();
    let send_sock = Arc::new(Mutex::new(send_sock));

    let streaming_params_cpy = streaming_params.clone();
    let paths_cpy = paths.clone();
    thread::spawn({
        let send_sock  = Arc::clone(&send_sock);
        move || {
        video_name_sender(send_sock, streaming_params_cpy, paths_cpy);
    }});

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    let mut write_frame = move |data : &[u8]| -> std::io::Result<()> {
        handle.write_all(&[0x00, 0x00, 0x00, 0x01])?;
        handle.write_all(data)?;
        handle.flush()?;
        Ok(())
    };

    let open_h264_file = |path| -> std::io::Result<_> {
        eprintln!("Open file {:?}", path);
        let input_file = File::open(path)?;
        let file = std::io::BufReader::new(input_file);
        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
        let mut parser = H264Parser::new();
        let it = it.map(move |data| {
            let info = parser.parse_nal(&data);
            (data, info)
        });
        Ok(it)
    };

    let mut h264_iter = open_h264_file(&paths[0])?;
    let mut current_video_num = 0;

    // Write out at least one I-frame
    loop {
        let (data, info) = h264_iter.next().unwrap();
        write_frame(&data)?;
        if info.map_or(false, |x| x.frame_type == FrameType::IOnly) {
            break;
        }
    }


    let mut loop_buf = Vec::<Vec<u8>>::new();
    let mut loop_i = 0;
    let mut recording = false; // for params.record_loop edge detection

    loop {
        let mut params = streaming_params.lock().unwrap().clone();

        // Switch video if requested
        if current_video_num != params.video_num && params.video_num < paths.len() {
            h264_iter = open_h264_file(&paths[params.video_num])?;
            current_video_num = params.video_num;
        }

        if params.record_loop && !recording {
            // clear loop buffer when starting a new recording
            loop_buf.clear();
            loop_i = 0;
        } else if !params.record_loop && recording {
            params.play_loop = true;
            let mut params_mut = streaming_params.lock().unwrap();
            params_mut.play_loop = true;
            if let Some(client_addr) = &params_mut.client_addr {
                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage{
                    addr: "/play_loop".to_owned(),
                    args: vec![OscType::Bool(params_mut.play_loop)],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            }
        }
        recording = params.record_loop;

        if params.play_loop && loop_buf.len() > 0 {
            // Play from loop
            if loop_i >= loop_buf.len() {
                loop_i = 0;
            }
            write_frame(&loop_buf[loop_i])?;
            loop_i += 1;

        } else {
            // Play from file
            let mut frame = h264_iter.next();
            // Restart video if at end
            if frame.is_none() {
                h264_iter = open_h264_file(&paths[current_video_num])?;
                frame = h264_iter.next();
            }
            let (data, info) = frame.unwrap();
            if info.map_or(false, |x| x.frame_type != FrameType::IOnly || params.pass_iframe) {
                write_frame(&data)?;
                if params.record_loop {
                    loop_buf.push(data);
                }
            }
        }
        std::thread::sleep_ms((1000.0 / params.fps) as u32);
    }
    Ok(())
}

const PALETTE : &'static [&'static str] = &["EF476FFF", "FFD166FF", "06D6A0FF", "118AB2FF", "aa1d97ff"];

fn video_name_sender(send_sock: Arc<Mutex<UdpSocket>>, streaming_params: Arc<Mutex<StreamingParams>>, paths: Vec<PathBuf>) {

    loop {
        let params = streaming_params.lock().unwrap().clone();
        if let Some(client_addr) = params.client_addr {
            // Send FPS
            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                addr: "/fps".to_string(),
                args: vec![OscType::Float(params.fps)],
            })).unwrap();
            send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();

            // Send video labels
            let mut i = 5;
            let mut last_dir = None;
            let mut color_idx = 0;
            for path in &paths {
                let dir = path.parent();
                let filename = path.file_stem().unwrap().to_str().unwrap().to_string();

                // Select new color per directory
                if last_dir != dir {
                    last_dir = dir;
                    color_idx = (color_idx + 1) % PALETTE.len();
                }
                let color = PALETTE[color_idx];

                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                    addr: format!("/label{}", i).to_string(),
                    args: vec![OscType::String(filename), OscType::String(color.to_string())],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
                i += 1;
            }
            for j in i..54+(54-5)*2+1 {
                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                    addr: format!("/label{}", j).to_string(),
                    args: vec![OscType::String("N/A".to_string()), OscType::String("#000000".to_string())],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            }
        }
        std::thread::sleep_ms(1000);
    }

}

fn osc_listener(addr: &SocketAddr, streaming_params: Arc<Mutex<StreamingParams>>) {
    let sock = UdpSocket::bind(addr).unwrap();
    eprintln!("OSC: Listening to {}", addr);

    let mut buf = [0u8; rosc::decoder::MTU];

    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, client_addr)) => {
                let packet = rosc::decoder::decode(&buf[..size]).unwrap();
                let mut params = streaming_params.lock().unwrap();
                params.client_addr = Some(client_addr);
                match packet {
                    OscPacket::Message(msg) => {
                        match msg.addr.as_str() {
                            "/fps" => {
                                params.fps = msg.args[0].clone().float().unwrap();
                            },
                            "/record_loop" => {
                                params.record_loop = msg.args[0].clone().bool().unwrap();
                            },
                            "/play_loop" => {
                                params.play_loop = msg.args[0].clone().bool().unwrap();
                            },
                            "/pass_iframe" => {
                                params.pass_iframe = msg.args[0].clone().bool().unwrap();
                            },
                            "/video_num" => {
                                params.video_num = msg.args[0].clone().int().unwrap() as usize;
                            },
                            _ => {
                                eprintln!("Unhandled OSC address: {}", msg.addr);
                                eprintln!("Unhandled OSC arguments: {:?}", msg.args);
                            }
                        }
                    }
                    OscPacket::Bundle(bundle) => {
                        eprintln!("OSC Bundle: {:?}", bundle);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error receiving from socket: {}", e);
                break;
            }
        }
    }

}
