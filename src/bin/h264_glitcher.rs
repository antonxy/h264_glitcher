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
use walkdir::WalkDir;


#[derive(Debug, StructOpt)]
#[structopt(name = "h264_glitcher", about = "Live controllable h264 glitcher.",
            long_about = "Pipe output into mpv.")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), required=true, help="Input video directory. Expects a subdirectory \"encoded\" with the raw h264 streams and a subdirectory \"thumbnails\" with a thumbnail for each stream.")]
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

#[derive(Clone, Default)]
struct State {
    video_num: usize,
    beat_multiplier: f32,
    pass_iframe: bool,
    playhead: f32,
    loop_range: Option<(f32, f32)>,
    auto_skip: bool,
    drop_frames: bool,
}


#[derive(Clone)]
struct StreamingParams {
    record_loop: bool,

    cut_loop: Option<f32>,
    restart_loop: bool,

    skip_frames: Option<usize>,
    client_addr: Option<SocketAddr>,
    
    auto_switch_n: usize,
    loop_to_beat: bool,
    use_external_beat: bool,
    beat_offset: Duration,
    beat_divider: u32,

    state_slots: Vec<State>,
    active_slot: usize,
    edit_slot: usize,
}

impl Default for StreamingParams {
    fn default() -> Self {
        Self {
            record_loop: false,
            cut_loop: None,
            restart_loop: false,
            skip_frames: None,
            client_addr: None,
            auto_switch_n: 0,
            loop_to_beat: false,
            use_external_beat: false,
            beat_offset: Duration::from_millis(0),
            beat_divider: 1,

            state_slots: vec![State::default(); 6],
            active_slot: 0,
            edit_slot: 0,
        }
    }
}

impl StreamingParams {
    fn active_state(&self) -> &State {
        &self.state_slots[self.active_slot]
    }

    fn active_state_mut(&mut self) -> &mut State {
        &mut self.state_slots[self.active_slot]
    }

    fn edit_state(&self) -> &State {
        &self.state_slots[self.edit_slot]
    }
    
    fn edit_state_mut(&mut self) -> &mut State {
        &mut self.state_slots[self.edit_slot]
    }
}

struct Video {
    file: File,
    loadedVideo : Option<Arc<Mutex<LoadedVideo>>>,
}

struct LoadedVideo {
    frames: Vec<NalUnit>,
}
impl LoadedVideo {
    fn load(path: &std::path::Path) -> std::io::Result<LoadedVideo> {
        eprintln!("Open file {:?}", path);
        let input_file = File::open(path)?;
        let file = std::io::BufReader::with_capacity(1<<20, input_file);
        let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
        let it = it.map(|data| NalUnit::from_bytes(&data)).filter_map(|r| {
            match r {
                Ok(v) => Some(v),
                Err(err) => {
                    eprintln!("Failed to parse frame: {:?}", err);
                    None
                }
            }
        });
        Ok(LoadedVideo { frames: it.collect() })
    }
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
    let relative_paths : Vec<PathBuf> = WalkDir::new(&encoded_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| p.extension().unwrap_or(std::ffi::OsStr::new("")) == "h264")
        .map(|p| p.strip_prefix(&encoded_path).unwrap().with_extension("").to_path_buf())
        .collect();

    let paths : Vec<PathBuf> = relative_paths.iter().map(|p| append_extension(&encoded_path.join(p), "h264")).collect();

    // Check if all video files can be opened
    for path in &paths {
        File::open(path)?;
    }

    let base_url = PathBuf::from("http://127.0.0.1:3000/");
    let thumbnail_urls : Vec<String> = relative_paths.iter().map(|p| {
         append_extension(&base_url.join(p), "png").to_str().unwrap().to_string()
    }).collect();

    thread::spawn(
        move || {
            h264_glitcher::thumbnail_server::serve(&thumbnail_path);
        }
    );


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

    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let loop_controller = loop_controller.clone();
        let paths = paths.clone();
        move || {
        video_name_sender(send_sock, streaming_params, loop_controller, paths, thumbnail_urls);
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

    // let open_h264_file = |path| -> std::io::Result<_> {
    //     eprintln!("Open file {:?}", path);
    //     let input_file = File::open(path)?;
    //     let file = std::io::BufReader::with_capacity(1<<20, input_file);
    //     let it = NalIterator::new(file.bytes().map(|x| x.unwrap()));
    //     let it = it.map(|data| NalUnit::from_bytes(&data));
    //     Ok(it)
    // };



    //let mut h264_iter = open_h264_file(&paths[0])?;
    let mut current_video_num = 0;
    let mut current_video: LoadedVideo = LoadedVideo::load(&paths[0])?;
    let mut current_frame: usize = 0;

    let advance_frame = |current_frame: &mut usize, total_frames: usize| {
        let (mut from_incl, mut to_excl) = (0, total_frames);

        //TODO don't lock every fucking time
        if let Some((loop_from, loop_to)) = streaming_params.lock().unwrap().active_state().loop_range {
            let loop_from = (total_frames as f32 * loop_from) as usize;
            let loop_to = (total_frames as f32 * loop_to) as usize;

            from_incl = usize::min(loop_from, total_frames - 2);
            to_excl = usize::min(usize::max(from_incl + 1, loop_to), total_frames);
        }

        assert!(from_incl >= 0);
        assert!(from_incl < to_excl);
        assert!(to_excl <= total_frames);

        if *current_frame < from_incl {
            *current_frame = from_incl;
        } else {
            *current_frame += 1;
            if *current_frame >= to_excl {
                *current_frame = from_incl;
            }
        }
    };

    // Write out at least one I-frame
    loop {
        let nal_unit = &current_video.frames[current_frame];
        advance_frame(&mut current_frame, current_video.frames.len());
        write_frame(&nal_unit)?;
        if nal_unit.nal_unit_type == NALUnitType::CodedSliceIdr {
            eprintln!("Got first I frame");
            break;
        }
    }

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
        let state = params.active_state().clone();

        // Process all "requests"

        // Switch video if requested
        if current_video_num != state.video_num && state.video_num < paths.len() {
            current_video_num = state.video_num;
            current_video = LoadedVideo::load(&paths[state.video_num])?;
            current_frame = 0;
        }

        // Now the state based stuff


        
        // Play from file
        if state.drop_frames {
            advance_frame(&mut current_frame, current_video.frames.len());
        }

        if let Some(skip) = params.skip_frames {
            for _ in 0..skip {
                advance_frame(&mut current_frame, current_video.frames.len()); //TODO advance n
            }
            streaming_params.lock().unwrap().skip_frames = None;
        }

        // Restart video if at end
        advance_frame(&mut current_frame, current_video.frames.len());
        let mut nal_unit = &current_video.frames[current_frame];
        
        // If pass_iframe is not activated, send only CodedSliceNonIdr
        // Sending a new SPS without an Idr Slice seems to cause problems when switching between some videos
        if nal_unit.nal_unit_type == NALUnitType::CodedSliceNonIdr || state.pass_iframe {
            write_frame(&nal_unit)?;
            let is_picture_data = nal_unit.nal_unit_type.is_picture_data();
            if !is_picture_data {
                continue; //Only sleep if the nal_unit is a video frame
            }
        } else {
            continue; //If we didn't send out frame don't sleep
        }

        let playhead = current_frame as f32 / current_video.frames.len() as f32;
        streaming_params.lock().unwrap().active_state_mut().playhead = playhead;
        if let Some(client_addr) = params.client_addr {
            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                addr: "/playhead".to_string(),
                args: vec![playhead.into()],
            })).unwrap();
            send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
        }

        loop_timer.end_loop();
    }
}

const PALETTE : &'static [&'static str] = &["#EF476F", "#FFD166", "#06D6A0", "#118AB2", "#aa1d97"];

fn video_name_sender(send_sock: Arc<Mutex<UdpSocket>>, streaming_params: Arc<Mutex<StreamingParams>>, loop_controller: LoopController, paths: Vec<PathBuf>, thumbnails: Vec<String>) {

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
                args: vec![params.edit_state().auto_skip.into()],
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
            send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();

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
                    args: vec![filename.into()/*, color.into()*/],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();

                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                    addr: format!("/label_{}/color", i).to_string(),
                    args: vec![color.into()],
                })).unwrap();
                send_sock.lock().unwrap().send_to(&msg_buf, client_addr).unwrap();
            }

            for (j, thumbnail) in thumbnails.iter().enumerate() {
                let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                    addr: format!("/thumbnail_{}", j).to_string(),
                    args: vec![thumbnail.to_string().into()],
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
                    if params.active_state().auto_skip {
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
                            params.active_state_mut().video_num = switch_history[auto_switch_num];
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
                let record_loop = msg.args[0].clone().bool().ok_or(())?;
                if record_loop {
                    let (_, to) = params.active_state().loop_range.unwrap_or((0.0, 1.0));
                    params.active_state_mut().loop_range = Some((params.active_state().playhead, to));
                } else {
                    let (from, _) = params.active_state().loop_range.unwrap_or((0.0, 1.0));
                    params.active_state_mut().loop_range = Some((from, params.active_state().playhead));
                }
            },
            "/play_loop" => {
                //params.play_loop = msg.args[0].clone().bool().ok_or(())?;
            },
            "/cut_loop" => {
                params.cut_loop = Some(msg.args[0].clone().float().ok_or(())?);
            },
            "/pass_iframe" => {
                params.edit_state_mut().pass_iframe = msg.args[0].clone().bool().ok_or(())?;
            },
            "/drop_frames" => {
                params.edit_state_mut().drop_frames = msg.args[0].clone().bool().ok_or(())?;
            },
            "/skip_frames" => {
                params.skip_frames = Some(msg.args[0].clone().int().ok_or(())? as usize);
                fps_controller.wake_up_now();
            },
            "/video_num" => {
                params.edit_state_mut().video_num = msg.args[0].clone().int().ok_or(())? as usize;
                fps_controller.wake_up_now();
                if switch_history.len() == 5 {
                    switch_history.pop_back();
                }
                switch_history.push_front(params.edit_state().video_num);
            },
            "/auto_skip" => {
                params.edit_state_mut().auto_skip = msg.args[0].clone().bool().ok_or(())?;
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
            "/loop_range" => {
                let from = msg.args[0].clone().float().ok_or(())?;
                let to = msg.args[1].clone().float().ok_or(())?;
                params.edit_state_mut().loop_range = Some((from, to));
            },
            "/active_slot" => {
                params.active_slot = (msg.args[0].clone().int().ok_or(())?) as usize;
            },
            "/edit_slot" => {
                params.edit_slot = (msg.args[0].clone().int().ok_or(())?) as usize;
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
