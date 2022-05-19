pub(crate) mod h264;
pub(crate) mod nal_iterator;
pub(crate) mod parse_nal;
pub(crate) mod beat_predictor;

use crate::parse_nal::*;
use crate::h264::NALUnitType;
use crate::nal_iterator::NalIterator;
use crate::beat_predictor::BeatPredictor;

extern crate structopt;

use std::fs::File;
use std::io::{Read, Write};
use std::ops::Add;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};
use std::vec::Vec;
use std::collections::VecDeque;
use structopt::StructOpt;
use std::thread;
use std::sync::{Mutex, Arc};
use std::net::{SocketAddr, UdpSocket};
use rosc::{OscPacket, OscMessage, encoder, OscType};


#[derive(Debug, StructOpt)]
#[structopt(name = "h264_glitcher", about = "Live controllable h264 glitcher.",
            long_about = "Pipe output into mpv.")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), required=true, help="Input video file(s). Directories will be ignored.")]
    input: Vec<PathBuf>,

    #[structopt(short = "l", long, default_value = "0.0.0.0:8000", help="OSC listen address")]
    listen_addr: String,
    #[structopt(short = "s", long, default_value = "0.0.0.0:0", help="OSC send address")]
    send_addr: String,

    #[structopt(long, help="Rewrite frame_num fields for potentially smoother playback")]
    rewrite_frame_nums: bool,
}


#[derive(Clone)]
struct StreamingParams {
    fps: f32,
    record_loop: bool,
    play_loop: bool,
    loop_ping_pong: bool,
    cut_loop: Option<f32>,
    pass_iframe: bool,
    drop_frames: bool,
    skip_frames: Option<usize>,
    video_num: usize,
    client_addr: Option<SocketAddr>,
    save_loop_num: Option<usize>,
    recall_loop_num: Option<usize>,
    auto_skip: bool,
    auto_switch_n: usize,
    auto_switch: bool,
    beat_offset: Duration,
}

impl Default for StreamingParams {
    fn default() -> Self {
        Self {
            fps: 30.0,
            record_loop: false,
            play_loop: false,
            loop_ping_pong: false,
            cut_loop: None,
            pass_iframe: false,
            drop_frames: false,
            skip_frames: None,
            video_num: 0,
            client_addr: None,
            save_loop_num: None,
            recall_loop_num: None,
            auto_skip: false,
            auto_switch_n: 0,
            auto_switch: false,
            beat_offset: Duration::from_millis(0),
        }
    }
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();

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
        Err(_) => panic!("Invalid send_addr"),
    };

    let send_sock = UdpSocket::bind(send_from_addr).unwrap();
    let send_sock = Arc::new(Mutex::new(send_sock));

    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let paths = paths.clone();
        move || {
        video_name_sender(send_sock, streaming_params, paths);
    }});

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    handle.write_all(&[0x00, 0x00, 0x00, 0x01])?;


    let mut last_frame_num = 0;
    let rewrite_frame_nums = opt.rewrite_frame_nums;
    let mut write_frame = move |nal_unit: &NalUnit| -> std::io::Result<()> {
        let mut nal_unit = nal_unit.clone();
        let has_frame_num = match nal_unit.nal_unit_type {
            NALUnitType::CodedSliceIdr | NALUnitType::CodedSliceNonIdr => { true },
            _ => { false },
        };
        if rewrite_frame_nums && has_frame_num {
            let mut header = SliceHeader::from_bytes(&nal_unit.rbsp).unwrap();
            // Just setting all frame nums to zero also seems to work.
            // Maybe mpv even crashes a bit less with just zero
            // I haven't observed a crash for a while though, maybe it was something else also
            //header.frame_num = 0;

            header.frame_num = last_frame_num;
            last_frame_num += 1;
            last_frame_num %= 16; //This just assumes frame nums are encoded with 4 bits. That doesn't have to be the case though.
            nal_unit.rbsp = header.to_bytes();

        }
        handle.write_all(&nal_unit.to_bytes())?;
        handle.write_all(&[0x00, 0x00, 0x00, 0x01])?;
        handle.flush()?;
        Ok(())
    };

    let open_h264_file = |path| -> std::io::Result<_> {
        eprintln!("Open file {:?}", path);
        let input_file = File::open(path)?;
        let file = std::io::BufReader::with_capacity(1<<20, input_file);
        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
        let it = it.map(move |data| {
            let nal_unit = NalUnit::from_bytes(&data);
            (data, nal_unit)
        });
        Ok(it)
    };

    let mut h264_iter = open_h264_file(&paths[0])?;
    let mut current_video_num = 0;

    // Write out at least one I-frame
    loop {
        let (data, nal_unit) = h264_iter.next().unwrap();
        if let Ok(nal_unit) = nal_unit {
            write_frame(&nal_unit)?;
            if nal_unit.nal_unit_type == NALUnitType::CodedSliceIdr {
                eprintln!("Got first I frame");
                break;
            }
        }
    }


    let mut loop_mem = std::collections::HashMap::<usize, Vec<NalUnit>>::new();
    let mut loop_buf = Vec::<NalUnit>::new();
    let mut loop_i = 0;
    let mut loop_backwards = false;
    let mut recording = false; // for params.record_loop edge detection

    let (sender, receiver) = std::sync::mpsc::sync_channel(0);

    std::thread::spawn({
        let streaming_params = Arc::clone(&streaming_params);
        let sender = sender.clone();
        move || {

        let max_supported_fps = 240.0;

        let mut last_frame_at = Instant::now();
        loop {
            let target_fps = streaming_params.lock().unwrap().fps;
            let now = Instant::now();
            if last_frame_at.add(Duration::from_secs_f32(1.0 / target_fps)) >  now {
                // Sleep very briefly, then re-evaluate.
                // This lowers response time to fps / video_num changes.
                // XXX: interruptible sleep would be better
                spin_sleep::sleep(Duration::from_secs_f32( 1.0 / max_supported_fps));
                continue;
            }
            last_frame_at = now;
            // wake the main loop for one frame
            sender.send(()).unwrap();
        }
    }});

    let beat_predictor = BeatPredictor::new();
    let beat_predictor = Arc::new(Mutex::new(beat_predictor));
    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let sender = sender.clone();
        let beat_predictor = beat_predictor.clone();
        move || {
        beat_thread(beat_predictor, send_sock, &addr, streaming_params, sender);
    }});

    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let sender = sender.clone();
        move || {
        osc_listener(beat_predictor, send_sock, &addr, streaming_params, sender);
    }});

    loop {
        let mut params = streaming_params.lock().unwrap().clone();

        // Process all "requests"

        // Switch video if requested
        if current_video_num != params.video_num && params.video_num < paths.len() {
            h264_iter = open_h264_file(&paths[params.video_num])?;
            current_video_num = params.video_num;
        }

        if let Some(save_loop_num) = params.save_loop_num {
            loop_mem.insert(save_loop_num, loop_buf.clone());
            streaming_params.lock().unwrap().save_loop_num = None;
        }

        if let Some(recall_loop_num) = params.recall_loop_num {
            loop_mem.get(&recall_loop_num).map(|x| loop_buf = x.clone());
            let mut params_mut = streaming_params.lock().unwrap();
            params_mut.recall_loop_num = None;
            params_mut.play_loop = true;
            if let Some(client_addr) = &params_mut.client_addr {
                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage{
                    addr: "/play_loop".to_owned(),
                    args: vec![params_mut.play_loop.into()],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            }
        }

        if let Some(cut_loop) = params.cut_loop {
            let new_len = (loop_buf.len() as f32 * cut_loop) as usize + 1;
            loop_buf.truncate(new_len);
            streaming_params.lock().unwrap().cut_loop = None;
        }

        // Now the state based stuff

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
                    args: vec![params_mut.play_loop.into()],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            }
        }
        recording = params.record_loop;

        if params.play_loop && loop_buf.len() > 0 && !recording {
            // Play from loop
            if params.loop_ping_pong {
                if loop_backwards {
                    if loop_i > 0 {
                        loop_i -= 1;
                    }
                    if loop_i == 0 {
                        loop_backwards = false;
                    }
                } else {
                    loop_i += 1;
                    if loop_i >= loop_buf.len() - 1 {
                        loop_backwards = true;
                        loop_i = loop_buf.len() - 1;
                    }
                }
            } else {
                loop_i += 1;
                if loop_i >= loop_buf.len() {
                    loop_i = 0;
                }
            }

            write_frame(&loop_buf[loop_i])?;

        } else {
            // Play from file
            if params.drop_frames {
                h264_iter.next();
            }

            if let Some(skip) = params.skip_frames {
                for _ in 0..skip {
                    h264_iter.next();
                }
                streaming_params.lock().unwrap().skip_frames = None;
            }

            let mut frame = h264_iter.next();
            // Restart video if at end
            if frame.is_none() {
                h264_iter = open_h264_file(&paths[current_video_num])?;
                frame = h264_iter.next();
            }
            let (data, nal_unit) = frame.unwrap();
            if let Ok(mut nal_unit) = nal_unit {
                if nal_unit.nal_unit_type != NALUnitType::CodedSliceIdr || params.pass_iframe {
                    write_frame(&nal_unit)?;
                    let is_picture_data = match nal_unit.nal_unit_type {
                        NALUnitType::CodedSliceIdr | NALUnitType::CodedSliceNonIdr => { true },
                        _ => { false },
                    };
                    if params.record_loop {
                        loop_buf.push(nal_unit);
                    }
                    if !is_picture_data {
                        continue; //Only sleep if the nal_unit is a video frame
                    }
                } else {
                    continue; //If we didn't send out frame don't sleep
                }
            } else {
                continue; //If we didn't send out frame don't sleep
            }
        }

        receiver.recv().unwrap();
    }
}

const PALETTE : &'static [&'static str] = &["EF476FFF", "FFD166FF", "06D6A0FF", "118AB2FF", "aa1d97ff"];

fn video_name_sender(send_sock: Arc<Mutex<UdpSocket>>, streaming_params: Arc<Mutex<StreamingParams>>, paths: Vec<PathBuf>) {

    loop {
        let params = streaming_params.lock().unwrap().clone();
        if let Some(client_addr) = params.client_addr {
            // Send FPS
            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                addr: "/fps".to_string(),
                args: vec![params.fps.into()],
            })).unwrap();
            send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            // Send auto-states
            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                addr: "/auto_skip".to_string(),
                args: vec![params.auto_skip.into()],
            })).unwrap();
            send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                addr: "/auto_switch".to_string(),
                args: vec![(params.auto_switch_n as i32).into()],
            })).unwrap();
            send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            // Send beat_offset
            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                addr: "/beat_offset".to_string(),
                args: vec![params.beat_offset.as_secs_f32().into()],
            })).unwrap();

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
                    args: vec![filename.into(), color.into()],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
                i += 1;
            }
            for j in i..54+(54-5)*2+1 {
                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                    addr: format!("/label{}", j).to_string(),
                    args: vec!["N/A".into(), "#000000".into()],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }

}

fn beat_thread(beat_predictor: Arc<Mutex<BeatPredictor>>, send_sock: Arc<Mutex<UdpSocket>>, addr: &SocketAddr, streaming_params: Arc<Mutex<StreamingParams>>, wakeup_main_loop: SyncSender<()>) {
    let wake_main_loop = move || {
        match wakeup_main_loop.try_send(()) {
            Ok(_) => (),
            Err(TrySendError::Full(_)) => (),
            e @ Err(_) => panic!("{:?}", e),
        }
    };

    loop {
        let params = streaming_params.lock().unwrap().clone();
        let next_beat_dur = beat_predictor.lock().unwrap().duration_to_next_beat(params.beat_offset);
        if let Some(next_beat_dur) = next_beat_dur {
            // Sleep max 100ms so that we don't miss if the beat speed changes from a very low
            // one to a high one
            if next_beat_dur > Duration::from_millis(100) {
                std::thread::sleep(Duration::from_millis(90));
            } else {
                std::thread::sleep(next_beat_dur);

                if let Some(client_addr) = params.client_addr {
                    // Send beat
                    let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                        addr: "/beat_delayed".to_string(),
                        args: vec![OscType::Int(1)],
                    })).unwrap();
                    send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
                }

                // Do the beat stuff here
                let mut params = streaming_params.lock().unwrap();
                if params.auto_skip {
                    params.skip_frames = Some(20);
                    wake_main_loop();
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn osc_listener(beat_predictor: Arc<Mutex<BeatPredictor>>, send_sock: Arc<Mutex<UdpSocket>>, addr: &SocketAddr, streaming_params: Arc<Mutex<StreamingParams>>, wakeup_main_loop: SyncSender<()>) {
    let sock = UdpSocket::bind(addr).unwrap();
    eprintln!("OSC: Listening to {}", addr);

    let wake_main_loop = move || {
        match wakeup_main_loop.try_send(()) {
            Ok(_) => (),
            Err(TrySendError::Full(_)) => (),
            e @ Err(_) => panic!("{:?}", e),
        }
    };
    let mut switch_history: VecDeque<usize> = VecDeque::with_capacity(5);

    let mut buf = [0u8; rosc::decoder::MTU];

    let parse_message = |msg: &OscMessage, params: &mut StreamingParams, client_addr: SocketAddr, switch_history: &mut VecDeque<usize>| -> Result<(), ()> {
        match msg.addr.as_str() {
            "/set_client_address" => {
                params.client_addr = Some(client_addr);
                eprintln!("updated client_addr: {:?}", params.client_addr);
            },
            "/fps" => {
                params.fps = msg.args[0].clone().float().ok_or(())?;
                wake_main_loop();
            },
            "/record_loop" => {
                params.record_loop = msg.args[0].clone().bool().ok_or(())?;
            },
            "/play_loop" => {
                params.play_loop = msg.args[0].clone().bool().ok_or(())?;
            },
            "/loop_ping_pong" => {
                params.loop_ping_pong = msg.args[0].clone().bool().ok_or(())?;
            },
            "/cut_loop" => {
                params.cut_loop = Some(msg.args[0].clone().float().ok_or(())?);
            },
            "/pass_iframe" => {
                params.pass_iframe = msg.args[0].clone().bool().ok_or(())?;
            },
            "/drop_frames" => {
                params.drop_frames = msg.args[0].clone().bool().ok_or(())?;
            },
            "/skip_frames" => {
                params.skip_frames = Some(msg.args[0].clone().int().ok_or(())? as usize);
                wake_main_loop();
            },
            "/video_num" => {
                params.video_num = msg.args[0].clone().int().ok_or(())? as usize;
                wake_main_loop();
                if switch_history.len() == 5 {
                    switch_history.pop_back();
                }
                switch_history.push_front(params.video_num);
            },
            "/save_loop" => {
                params.save_loop_num = Some(msg.args[0].clone().int().ok_or(())? as usize);
            },
            "/recall_loop" => {
                params.recall_loop_num = Some(msg.args[0].clone().int().ok_or(())? as usize);
            },
            "/auto_skip" => {
                params.auto_skip = msg.args[0].clone().bool().ok_or(())?;
            },
            "/auto_switch" => {
                params.auto_switch_n = msg.args[0].clone().int().ok_or(())? as usize;
            },
            "/beat" => {
                beat_predictor.lock().unwrap().put_input_beat();
                if let Some(client_addr) = params.client_addr {
                    // Send beat
                    let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                        addr: "/beat".to_string(),
                        args: vec![OscType::Int(1)],
                    })).unwrap();
                    send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
                }
            },
            "/beat_offset" => {
                params.beat_offset = Duration::from_secs_f32(msg.args[0].clone().float().ok_or(())?);
            },
            _ => {
                eprintln!("Unhandled OSC address: {}", msg.addr);
                eprintln!("Unhandled OSC arguments: {:?}", msg.args);
                return Err(());
            }
        }
        Ok(())
    };

    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, client_addr)) => {
                let packet = rosc::decoder::decode(&buf[..size]).unwrap();
                let mut params = streaming_params.lock().unwrap();
                let parse_result = match packet {
                    OscPacket::Message(ref msg) => {
                        parse_message(&msg, &mut params, client_addr, &mut switch_history)
                    }
                    OscPacket::Bundle(_) => {
                        eprintln!("Received bundle but they are currently not handled");
                        Err(())
                    }
                };

                if parse_result.is_err() {
                    eprintln!("Failed to parse OSC Packet: {:?}", packet);
                }
            }
            Err(e) => {
                eprintln!("Error receiving from socket: {}", e);
                break;
            }
        }
    }

}
