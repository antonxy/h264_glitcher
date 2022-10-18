use h264_glitcher::h264::*;
use h264_glitcher::beat_predictor::BeatPredictor;
use h264_glitcher::fps_loop::{LoopTimer, LoopController};

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
use websocket::sync::Server;
use websocket::OwnedMessage;
use walkdir::WalkDir;


#[derive(Debug, StructOpt)]
#[structopt(name = "h264_glitcher", about = "Live controllable h264 glitcher.",
            long_about = "Pipe output into mpv.")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), required=true, help="Input video file(s). Directories will be ignored.")]
    input_dir: PathBuf,

    #[structopt(short = "l", long, default_value = "0.0.0.0:8000", help="OSC listen address")]
    listen_addr: String,
    #[structopt(short = "s", long, default_value = "0.0.0.0:0", help="OSC send address")]
    send_addr: String,

    #[structopt(long, help="Rewrite frame_num fields for potentially smoother playback")]
    rewrite_frame_nums: bool,

    #[structopt(long, default_value = "1", help="Slow down input beat")]
    external_beat_divider: u32,
}


// TODO rename this as State
// There should also be events
// Design question: Should the state only be changed through events?
// Or should it be possible to send a complete new state?
// Sending complete new state would make implementation of state stack in ui easier
// If there should be multiple clients, maybe sending only events causes smaller mess
#[derive(Clone)]
struct StreamingParams {
    record_loop: bool,
    play_loop: bool,
    loop_ping_pong: bool,
    cut_loop: Option<f32>,
    restart_loop: bool,
    pass_iframe: bool,
    drop_frames: bool,
    skip_frames: Option<usize>,
    video_num: usize,
    client_addr: Option<SocketAddr>,
    save_loop_num: Option<usize>,
    recall_loop_num: Option<usize>,
    auto_skip: bool,
    auto_switch_n: usize,
    loop_to_beat: bool,
    use_external_beat: bool,
    beat_offset: Duration,
    beat_divider: u32,
}

impl Default for StreamingParams {
    fn default() -> Self {
        Self {
            record_loop: false,
            play_loop: false,
            loop_ping_pong: false,
            cut_loop: None,
            restart_loop: false,
            pass_iframe: false,
            drop_frames: false,
            skip_frames: None,
            video_num: 0,
            client_addr: None,
            save_loop_num: None,
            recall_loop_num: None,
            auto_skip: false,
            auto_switch_n: 0,
            loop_to_beat: false,
            use_external_beat: false,
            beat_offset: Duration::from_millis(0),
            beat_divider: 1,
        }
    }
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();

    let encoded_path = opt.input_dir.join("encoded");
    let thumbnail_path = opt.input_dir.join("thumbnails");
    let relative_paths : Vec<PathBuf> = WalkDir::new(&encoded_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| p.extension().unwrap_or(std::ffi::OsStr::new("")) == "h264")
        .map(|p| p.strip_prefix(&encoded_path).unwrap().with_extension("").to_path_buf())
        .collect();

    let paths : Vec<PathBuf> = relative_paths.iter().map(|p| encoded_path.join(p).with_extension("h264")).collect();

    // Check if all video files can be opened
    for path in &paths {
        File::open(path)?;
    }


    let videos : Vec<h264_glitcher_protocol::Video> = relative_paths.iter().enumerate().map(|(i, p)| {
        let mut file = File::open(thumbnail_path.join(p).with_extension("png")).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        h264_glitcher_protocol::Video {
            id: i,
            name: p.to_str().unwrap().to_owned(),
            thumbnail_png: data,
        }
    }).take(20).collect();

    let streaming_params = Arc::new(Mutex::new(StreamingParams::default()));
    let (mut loop_timer, loop_controller) = LoopTimer::new();

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

    // Start websocket server
    // The server should send on connection:
    // - video thumbnails
    // - current state
    // Whenever the state changes the new state should be sent
    // The server should apply state changes sent to the websocket
    // The server should accept events sent to the websocket
    let websock_server = Server::bind("127.0.0.1:2794").unwrap();
    thread::spawn({
        let streaming_params = streaming_params.clone();
        move || {
            for request in websock_server.filter_map(Result::ok) {
                // Spawn a new thread for each connection.
                let videos = videos.clone();
                let streaming_params = streaming_params.clone();
                thread::spawn(move || {
                    let mut client = request.use_protocol("rust-websocket").accept().unwrap();

                    let ip = client.peer_addr().unwrap();

                    eprintln!("Connection from {}", ip);

                    let mut message = Vec::new();
                    ciborium::ser::into_writer(&h264_glitcher_protocol::Message::Videos(videos), &mut message);
                    client.send_message(&OwnedMessage::Binary(message)).unwrap();

                    let (mut receiver, mut sender) = client.split().unwrap();

                    for message in receiver.incoming_messages() {
                        let message = message.unwrap();

                        match message {
                            OwnedMessage::Close(_) => {
                                let message = OwnedMessage::Close(None);
                                sender.send_message(&message).unwrap();
                                eprintln!("Client {} disconnected", ip);
                                return;
                            }
                            OwnedMessage::Ping(ping) => {
                                let message = OwnedMessage::Pong(ping);
                                sender.send_message(&message).unwrap();
                            }
                            OwnedMessage::Binary(data) => {
                                let msg : h264_glitcher_protocol::Message = ciborium::de::from_reader(data.as_slice()).unwrap();
                                match msg {
                                    h264_glitcher_protocol::Message::Event(h264_glitcher_protocol::Event::SetVideo(video_id)) => {
                                        eprintln!("Video num");
                                        streaming_params.lock().unwrap().video_num = video_id;
                                    },
                                    _ => { eprintln!("Unhandled message"); },
                                }
                            }
                            _ => eprintln!("Unhandled message"),
                        }
                    }
                });
            }
        }
    });

    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let loop_controller = loop_controller.clone();
        let paths = paths.clone();
        move || {
        video_name_sender(send_sock, streaming_params, loop_controller, paths);
    }});

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    handle.write_all(&[0x00, 0x00, 0x00, 0x01])?;


    let mut last_frame_num = 0;
    let rewrite_frame_nums = true;
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
        let it = it.map(|data| NalUnit::from_bytes(&data));
        Ok(it)
    };

    let mut h264_iter = open_h264_file(&paths[0])?;
    let mut current_video_num = 0;

    // Write out at least one I-frame
    loop {
        let nal_unit = h264_iter.next().unwrap();
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

    let beat_predictor = BeatPredictor::new();
    let beat_predictor = Arc::new(Mutex::new(beat_predictor));
    let switch_history: VecDeque<usize> = VecDeque::with_capacity(5);
    let switch_history = Arc::new(Mutex::new(switch_history));
    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let fps_controller = loop_controller.clone();
        let beat_predictor = beat_predictor.clone();
        let switch_history = switch_history.clone();
        move || {
        beat_thread(beat_predictor, switch_history, send_sock, &addr, streaming_params, fps_controller);
    }});

    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let fps_controller = loop_controller.clone();
        let beat_predictor = beat_predictor.clone();
        let switch_history = switch_history.clone();
        let external_beat_divider = opt.external_beat_divider;
        move || {
        osc_listener(beat_predictor, external_beat_divider, switch_history, send_sock, &addr, streaming_params, fps_controller);
    }});

    loop {
        loop_timer.begin_loop();
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

        if params.restart_loop {
            loop_i = 0;
            streaming_params.lock().unwrap().restart_loop = false;
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
            let nal_unit = frame.unwrap();
            if let Ok(nal_unit) = nal_unit {
                // If pass_iframe is not activated, send only CodedSliceNonIdr
                // Sending a new SPS without an Idr Slice seems to cause problems when switching between some videos
                if nal_unit.nal_unit_type == NALUnitType::CodedSliceNonIdr || params.pass_iframe {
                    write_frame(&nal_unit)?;
                    let is_picture_data = nal_unit.nal_unit_type.is_picture_data();
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
        loop_timer.end_loop();
    }
}

const PALETTE : &'static [&'static str] = &["EF476FFF", "FFD166FF", "06D6A0FF", "118AB2FF", "aa1d97ff"];

fn video_name_sender(send_sock: Arc<Mutex<UdpSocket>>, streaming_params: Arc<Mutex<StreamingParams>>, loop_controller: LoopController, paths: Vec<PathBuf>) {

    loop {
        let params = streaming_params.lock().unwrap().clone();
        if let Some(client_addr) = params.client_addr {
            // Send FPS
            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                addr: "/fps".to_string(),
                args: vec![loop_controller.fps().into()],
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

fn beat_thread(beat_predictor: Arc<Mutex<BeatPredictor>>, switch_history: Arc<Mutex<VecDeque<usize>>>, send_sock: Arc<Mutex<UdpSocket>>, addr: &SocketAddr, streaming_params: Arc<Mutex<StreamingParams>>, mut fps_controller: LoopController) {
    let mut auto_switch_num = 0;
    let mut beat_num = 0;

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

                beat_num += 1;

                if beat_num >= params.beat_divider {
                    beat_num = 0;

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
                        fps_controller.wake_up_now();
                    }

                    if params.auto_switch_n > 0 {
                        let switch_history = switch_history.lock().unwrap();
                        auto_switch_num += 1;
                        if auto_switch_num >= switch_history.len() || auto_switch_num > params.auto_switch_n {
                            auto_switch_num = 0;
                        }
                        if switch_history.len() > 0 {
                            params.video_num = switch_history[auto_switch_num];
                            fps_controller.wake_up_now();
                        }
                    }

                    if params.loop_to_beat {
                        params.restart_loop = true;
                    }
                }

            }
        } else {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn osc_listener(beat_predictor: Arc<Mutex<BeatPredictor>>, external_beat_divider: u32, switch_history: Arc<Mutex<VecDeque<usize>>>, send_sock: Arc<Mutex<UdpSocket>>, addr: &SocketAddr, streaming_params: Arc<Mutex<StreamingParams>>, mut fps_controller: LoopController) {
    let sock = UdpSocket::bind(addr).unwrap();
    eprintln!("OSC: Listening to {}", addr);

    let mut buf = [0u8; rosc::decoder::MTU];

    let mut beat_i = 0;

    let mut parse_message = |msg: &OscMessage, params: &mut StreamingParams, client_addr: SocketAddr, switch_history: &mut VecDeque<usize>| -> Result<(), ()> {
        match msg.addr.as_str() {
            "/set_client_address" => {
                params.client_addr = Some(client_addr);
                eprintln!("updated client_addr: {:?}", params.client_addr);
            },
            "/fps" => {
                fps_controller.set_fps(msg.args[0].clone().float().ok_or(())?);
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
                fps_controller.wake_up_now();
            },
            "/video_num" => {
                params.video_num = msg.args[0].clone().int().ok_or(())? as usize;
                fps_controller.wake_up_now();
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
            "/loop_to_beat" => {
                params.loop_to_beat = msg.args[0].clone().bool().ok_or(())?;
            },
            "/use_external_beat" => {
                params.use_external_beat = msg.args[0].clone().bool().ok_or(())?;
            },
            "/manual_beat" => {
                if !params.use_external_beat {
                    beat_predictor.lock().unwrap().put_input_beat();
                }
            },
            "/traktor/beat" => {
                if params.use_external_beat {
                    beat_i += 1;
                    if beat_i >= external_beat_divider {
                        beat_i = 0;
                        beat_predictor.lock().unwrap().put_input_beat();
                        if let Some(client_addr) = params.client_addr {
                            // Send beat
                            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                                addr: "/traktor/beat".to_string(),
                                args: vec![OscType::Int(1)],
                            })).unwrap();
                            send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
                        }
                    }
                } else {
                    beat_i = 0;
                }
            },
            "/beat_offset" => {
                params.beat_offset = Duration::from_secs_f32(msg.args[0].clone().float().ok_or(())?);
            },
            "/beat_multiplicator" => {
                let step = -msg.args[0].clone().int().ok_or(())? + 2;
                if step > 0 {
                    beat_predictor.lock().unwrap().multiplicator = 1.0;
                    params.beat_divider = 1 << step;
                } else {
                    let mult = f32::powi(2.0, step);
                    beat_predictor.lock().unwrap().multiplicator = mult;
                    params.beat_divider = 1;
                }
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
                let mut switch_history = switch_history.lock().unwrap();
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
