use h264_glitcher::h264::*;
use h264_glitcher::beat_predictor::BeatPredictor;
use h264_glitcher::fps_loop::{LoopTimer, LoopController};
use h264_glitcher::osc_var::{OscVar, LoopRange, OscValue};
use h264_glitcher::sigma_delta::SigmaDelta;

extern crate structopt;

use std::convert::TryInto;
use std::fs::File;
use std::io::{Read, Write};
use std::ops::{Add, Deref};
use std::path::PathBuf;
use std::str::FromStr;
use std::rc::Rc;
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
use rand::Rng;


#[derive(Debug, StructOpt)]
#[structopt(name = "h264_glitcher", about = "Live controllable h264 glitcher.",
            long_about = "Pipe output into mpv (or any other capable player).")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), required=true, help="Input video directory. Expects a subdirectory \"encoded\" with the raw h264 streams and a subdirectory \"thumbnails\" with a thumbnail for each stream.")]
    input_dir: PathBuf,

    #[structopt(short = "l", long, default_value = "127.0.0.1:8000", help="OSC listen address")]
    listen_addr: String,
    #[structopt(short = "s", long, default_value = "0.0.0.0:0", help="OSC send address (port = 0 -> choose port automatically)")]
    send_addr: String,

    #[structopt(long, help="Do not rewrite frame_num fields for potentially smoother playback")]
    no_rewrite_frame_nums: bool,

    #[structopt(long, help="Load and parse all videos into memory")]
    prefetch: bool,

    #[structopt(long, default_value = "1", help="Slow down input beat")]
    external_beat_divider: u32,

    #[structopt(long, default_value = "http://[::1]:3000/", help="Thumbnail server base url")]
    thumbnail_server_base_url: String,

    #[structopt(short = "l", long, default_value = "[::]:3000", help="Thumbnail server listen address")]
    thumbnail_server_listen_addr: String,
}




#[derive(Clone)]
struct State {
    video_num: OscVar<i32>,
    beat_multiplier: OscVar<i32>,
    pass_iframe: OscVar<bool>,
    playhead: OscVar<f32>,
    loop_range: OscVar<LoopRange>,
    auto_skip: OscVar<bool>,
    frame_repeat: OscVar<f32>, // Values < 1 drop frames, non integer values drop / repeat frames sometimes
    loop_to_beat: OscVar<bool>,
    fps: OscVar<f32>,
    auto_switch_n: OscVar<i32>,
    switch_history: VecDeque<usize>,
    byte_errors: OscVar<f32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            video_num: OscVar::new("/video_num", 0),
            beat_multiplier: OscVar::new("/beat_multiplier", 0),
            pass_iframe: OscVar::new("/pass_iframe", false),
            playhead: OscVar::new("/playhead", 0.0),
            loop_range: OscVar::new("/loop_range", LoopRange(None)),
            auto_skip: OscVar::new("/auto_skip", false),
            frame_repeat: OscVar::new("/frame_repeat", 1.0),
            loop_to_beat: OscVar::new("/loop_to_beat", false),
            fps: OscVar::new("/fps", 30.0),
            auto_switch_n: OscVar::new("/auto_switch", 0),
            switch_history: VecDeque::with_capacity(5),
            byte_errors: OscVar::new("/byte_errors", 0.0),
        }
    }
}

impl State {
    fn send_changed(&mut self, socket: &UdpSocket, client_addr: &SocketAddr) {
        self.video_num.send_if_changed(socket, client_addr);
        self.beat_multiplier.send_if_changed(socket, client_addr);
        self.pass_iframe.send_if_changed(socket, client_addr);
        self.playhead.send_if_changed(socket, client_addr);
        self.loop_range.send_if_changed(socket, client_addr);
        self.auto_skip.send_if_changed(socket, client_addr);
        self.frame_repeat.send_if_changed(socket, client_addr);
        self.loop_to_beat.send_if_changed(socket, client_addr);
        self.fps.send_if_changed(socket, client_addr);
        self.auto_switch_n.send_if_changed(socket, client_addr);
        self.byte_errors.send_if_changed(socket, client_addr);
    }

    fn set_changed(&mut self) {
        self.video_num.set_changed();
        self.beat_multiplier.set_changed();
        self.pass_iframe.set_changed();
        self.playhead.set_changed();
        self.loop_range.set_changed();
        self.auto_skip.set_changed();
        self.frame_repeat.set_changed();
        self.loop_to_beat.set_changed();
        self.fps.set_changed();
        self.auto_switch_n.set_changed();
        self.byte_errors.set_changed();
    }

    fn handle_osc_message(&mut self, msg: &OscMessage) -> bool {
        if self.video_num.handle_osc_message(msg) {
            if self.switch_history.len() == 5 {
                self.switch_history.pop_back();
            }
            self.switch_history.push_front(*self.video_num as usize);
        }
        self.beat_multiplier.handle_osc_message(msg) ||
        self.pass_iframe.handle_osc_message(msg) ||
        //self.playhead.handle_osc_message(msg) ||
        self.loop_range.handle_osc_message(msg) ||
        self.auto_skip.handle_osc_message(msg) ||
        self.frame_repeat.handle_osc_message(msg) ||
        self.loop_to_beat.handle_osc_message(msg) ||
        self.fps.handle_osc_message(msg) ||
        self.auto_switch_n.handle_osc_message(msg) ||
        self.byte_errors.handle_osc_message(msg)
    }
}

#[derive(Clone, Debug)]
struct ShortLoop {
    first_frame: Option<usize>,
    len: usize,
}


#[derive(Clone)]
struct StreamingParams {
    restart_loop: bool,

    skip_frames: Option<usize>,
    client_addr: Option<SocketAddr>,

    short_loop: Option<ShortLoop>,

    use_external_beat: OscVar<bool>,
    beat_offset: OscVar<Duration>,
    beat_divider: u32,

    state_slots: Vec<State>,
    active_slot: OscVar<usize>,
    edit_slot: OscVar<usize>,

    is_live: OscVar<bool>,
}

impl Default for StreamingParams {
    fn default() -> Self {
        Self {
            restart_loop: false,
            skip_frames: None,
            client_addr: None,
            short_loop: None,
            use_external_beat: OscVar::new("/use_external_beat", false),
            beat_offset: OscVar::new("/beat_offset", Duration::from_millis(0)),
            beat_divider: 1,

            state_slots: vec![State::default(); 6],
            active_slot: OscVar::new("/active_slot", 0),
            edit_slot: OscVar::new("/edit_slot", 0),

            is_live: OscVar::new("/is_live", true),
        }
    }
}

impl StreamingParams {
    fn set_active_slot(&mut self, slot: usize) {
        assert!(slot < 6);
        self.active_slot.set(slot);
        self.active_state_mut().set_changed();

        self.is_live.set(*self.active_slot == *self.edit_slot);
    }

    fn set_edit_slot(&mut self, slot: usize) {
        assert!(slot < 6);
        self.edit_slot.set(slot);
        self.edit_state_mut().set_changed();

        self.is_live.set(*self.active_slot == *self.edit_slot);
    }

    fn active_state(&self) -> &State {
        &self.state_slots[*self.active_slot]
    }

    fn active_state_mut(&mut self) -> &mut State {
        &mut self.state_slots[*self.active_slot]
    }

    fn edit_state(&self) -> &State {
        &self.state_slots[*self.edit_slot]
    }

    fn edit_state_mut(&mut self) -> &mut State {
        &mut self.state_slots[*self.edit_slot]
    }

    fn send_changed(&mut self, socket: &UdpSocket, client_addr: &SocketAddr) {
        self.use_external_beat.send_if_changed(socket, client_addr);
        self.beat_offset.send_if_changed(socket, client_addr);
        self.active_slot.send_if_changed(socket, client_addr);
        self.edit_slot.send_if_changed(socket, client_addr);
        self.edit_state_mut().send_changed(socket, client_addr);
        self.is_live.send_if_changed(socket, client_addr);
    }

    fn handle_osc_message(&mut self, msg: &OscMessage) -> bool {
        self.use_external_beat.handle_osc_message(msg) ||
        self.beat_offset.handle_osc_message(msg) ||
        self.active_slot.handle_osc_message(msg) ||
        self.edit_slot.handle_osc_message(msg) ||
        self.edit_state_mut().handle_osc_message(msg)
    }
}

struct Video {
    file: File,
    loadedVideo : Option<Arc<Mutex<LoadedVideo>>>,
}

#[derive(Clone)]
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
    let mut relative_paths : Vec<PathBuf> = WalkDir::new(&encoded_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| p.extension().unwrap_or(std::ffi::OsStr::new("")) == "h264")
        .map(|p| p.strip_prefix(&encoded_path).unwrap().with_extension("").to_path_buf())
        .collect();

    relative_paths.sort();

    let paths : Vec<PathBuf> = relative_paths.iter().map(|p| append_extension(&encoded_path.join(p), "h264")).collect();

    let mut loaded_videos : Vec<Rc<LoadedVideo>> = Vec::new();
    if opt.prefetch {
        loaded_videos = paths.iter().map(|p| Rc::new(LoadedVideo::load(p).unwrap())).collect();
    }

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



    let mut rng = rand::thread_rng();
    let mut last_frame_num = 0;
    let rewrite_frame_nums = !opt.no_rewrite_frame_nums;
    let mut write_frame = move |nal_unit: &NalUnit, byte_errors: f32| -> std::io::Result<()> {
        let mut nal_unit = nal_unit.clone();
        let has_frame_num = match nal_unit.nal_unit_type {
            NALUnitType::CodedSliceIdr | NALUnitType::CodedSliceNonIdr => { true },
            _ => { false },
        };
        if (rewrite_frame_nums || byte_errors > 0.0) && has_frame_num {
            let mut header = SliceHeader::from_bytes(&nal_unit.rbsp).unwrap();
            // Just setting all frame nums to zero also seems to work.
            // Maybe mpv even crashes a bit less with just zero
            // I haven't observed a crash for a while though, maybe it was something else also
            //header.frame_num = 0;

            if rewrite_frame_nums {
                header.frame_num = last_frame_num;
                last_frame_num += 1;
                last_frame_num %= 16; //This just assumes frame nums are encoded with 4 bits. That doesn't have to be the case though.
            }
            if byte_errors > 0.0 {
                // Introduce random errors. Start some bytes into buffer so that we hopefully only
                // hit data, not the header
                let offset = 50;
                // let offset = 0; // Or maybe thats fun also?

                for b in header.data[offset..].iter_mut() {
                    if rng.gen::<f32>() < byte_errors {
                        *b = rng.gen()
                    }
                }

                // Reasonable probability of errors seems to be around 0.0001
                // Weirder behaviour at higher error probabilities:
                // Why does the motion always go down ?
                // Why is the playhead not smooth anymore?
                // Does mpv hang and slow down the glitcher also (pipe gets full)?
                // Probably have to parse further down and destroy more controlled regions
                // Would be cool to destroy whole blocks, would look more glitchy maybe
            }
            nal_unit.rbsp = header.to_bytes();

        }
        handle.write_all(&nal_unit.to_bytes())?;
        handle.write_all(&[0x00, 0x00, 0x00, 0x01])?;
        handle.flush()?;
        Ok(())
    };

    let mut current_video_num: usize = 0;
    let mut current_video: Rc<LoadedVideo> = Rc::new(LoadedVideo::load(&paths[0])?);
    let mut current_frame: usize = 0;

    let advance_frame = |current_frame: &mut usize, total_frames: usize| {
        let (mut from_incl, mut to_excl) = (0, total_frames);

        //TODO don't lock every fucking time
        let mut params = streaming_params.lock().unwrap();

        if let Some(short_loop) = &mut params.short_loop {
            if short_loop.first_frame.is_none() {
                short_loop.first_frame = Some(*current_frame);
            }

            let loop_from = short_loop.first_frame.unwrap();
            let loop_to = short_loop.first_frame.unwrap() + short_loop.len;

            from_incl = usize::min(loop_from, total_frames - 2);
            to_excl = usize::min(usize::max(from_incl + 1, loop_to), total_frames);
        } else if let Some((loop_from, loop_to)) = params.active_state().loop_range.0 {
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
        write_frame(&nal_unit, 0.0)?;
        if nal_unit.nal_unit_type == NALUnitType::CodedSliceIdr {
            eprintln!("Got first I frame");
            break;
        }
    }

    let beat_predictor = BeatPredictor::new();
    let beat_predictor = Arc::new(Mutex::new(beat_predictor));
    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let fps_controller = loop_controller.clone();
        let beat_predictor = beat_predictor.clone();
        move || {
        beat_thread(beat_predictor, send_sock, &addr, streaming_params, fps_controller);
    }});

    thread::spawn({
        let send_sock = Arc::clone(&send_sock);
        let streaming_params = streaming_params.clone();
        let fps_controller = loop_controller.clone();
        let beat_predictor = beat_predictor.clone();
        let external_beat_divider = opt.external_beat_divider;
        move || {
        osc_listener(beat_predictor, external_beat_divider, send_sock, &addr, streaming_params, fps_controller);
    }});

    let mut sd = SigmaDelta::new();

    loop {
        loop_timer.begin_loop();
        let mut params = streaming_params.lock().unwrap().clone();
        let state = params.active_state().clone();

        // Process all "requests"

        // Switch video if requested
        if current_video_num as i32 != *state.video_num && *state.video_num < paths.len() as i32 {
            current_video_num = *state.video_num as usize;
            current_video =
                if opt.prefetch {
                    loaded_videos[current_video_num].clone() //TODO reference
                } else {
                    Rc::new(LoadedVideo::load(&paths[current_video_num])?)
                };
            current_frame = 0;
        }

        if params.restart_loop {
            current_frame = 0; // Will be set to loop start by advance_frame(...)
            streaming_params.lock().unwrap().restart_loop = false;
        }

        if let Some(skip) = params.skip_frames {
            for _ in 0..skip {
                advance_frame(&mut current_frame, current_video.frames.len()); //TODO advance n
            }
            streaming_params.lock().unwrap().skip_frames = None;
        }

        // Now the state based stuff

        let frame_repeat = sd.put(*state.frame_repeat);

        // Restart video if at end
        advance_frame(&mut current_frame, current_video.frames.len());
        let mut nal_unit = &current_video.frames[current_frame];

        for _ in 0..frame_repeat {

            // If pass_iframe is not activated, send only CodedSliceNonIdr
            // Sending a new SPS without an Idr Slice seems to cause problems when switching between some videos
            if nal_unit.nal_unit_type == NALUnitType::CodedSliceNonIdr || *state.pass_iframe {
                //TODO use sigma delta encoder to decide whether to drop/repeat the frame
                //How do I repeat a frame nicely in this loop? Do I want to sleep here? Maybe better
                //restart the loop. But how does it interact with the redt
                // Maybe use a counter somewhere and only `advance_frame` if the counter is reached

                write_frame(&nal_unit, *state.byte_errors)?;
                let is_picture_data = nal_unit.nal_unit_type.is_picture_data();
                if !is_picture_data {
                    continue; //Only sleep if the nal_unit is a video frame
                }
            } else {
                continue; //If we didn't send out frame don't sleep
            }

            let playhead = current_frame as f32 / current_video.frames.len() as f32;
            {
                let mut streaming_params = streaming_params.lock().unwrap();
                streaming_params.active_state_mut().playhead.set(playhead);
                if let Some(addr) = params.client_addr {
                    streaming_params.send_changed(&send_sock.lock().unwrap(), &addr);
                }
            }

            loop_timer.end_loop();

        }

    }
}

const PALETTE : &'static [&'static str] = &["#EF476F", "#FFD166", "#06D6A0", "#118AB2", "#aa1d97"];

fn video_name_sender(send_sock: Arc<Mutex<UdpSocket>>, streaming_params: Arc<Mutex<StreamingParams>>, loop_controller: LoopController, paths: Vec<PathBuf>, thumbnails: Vec<String>) {

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

fn beat_thread(beat_predictor: Arc<Mutex<BeatPredictor>>, send_sock: Arc<Mutex<UdpSocket>>, addr: &SocketAddr, streaming_params: Arc<Mutex<StreamingParams>>, mut fps_controller: LoopController) {
    let mut auto_switch_num = 0;
    let mut beat_num = 0;

    loop {
        let params = streaming_params.lock().unwrap().clone();
        let next_beat_dur = beat_predictor.lock().unwrap().duration_to_next_beat(*params.beat_offset);
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
                    if *params.active_state().auto_skip {
                        params.skip_frames = Some(20);
                        fps_controller.wake_up_now();
                    }

                    if *params.active_state().auto_switch_n > 0 {
                        let switch_history = params.active_state().switch_history.clone();
                        auto_switch_num += 1;
                        if auto_switch_num >= switch_history.len() || auto_switch_num > *params.active_state().auto_switch_n as usize {
                            auto_switch_num = 0;
                        }
                        if switch_history.len() > 0 {
                            params.active_state_mut().video_num.set(switch_history[auto_switch_num] as i32);
                            fps_controller.wake_up_now();
                        }
                    }

                    if *params.active_state().loop_to_beat {
                        params.restart_loop = true;
                    }
                }

            }
        } else {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn osc_listener(beat_predictor: Arc<Mutex<BeatPredictor>>, external_beat_divider: u32, send_sock: Arc<Mutex<UdpSocket>>, addr: &SocketAddr, streaming_params: Arc<Mutex<StreamingParams>>, mut fps_controller: LoopController) {
    let sock = UdpSocket::bind(addr).unwrap();
    eprintln!("OSC: Listening to {}", addr);

    let mut buf = [0u8; rosc::decoder::MTU];

    let mut beat_i = 0;

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
                    fps_controller.wake_up_now();
                },
                "/video_num" => {
                    params.edit_state_mut().video_num.set(msg.args[0].clone().int().ok_or(())?);
                    fps_controller.wake_up_now();
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
                    if !*params.use_external_beat {
                        beat_predictor.lock().unwrap().put_input_beat();
                    }
                },
                "/traktor/beat" => {
                    if *params.use_external_beat {
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
                "/reset" => {
                    *params.edit_state_mut() = State::default();
                    params.edit_state_mut().set_changed();
                },
                "/copy_active" => {
                    if *params.active_slot == *params.edit_slot {
                        params.edit_slot.set((*params.edit_slot + 1) % 6);
                    }
                    *params.edit_state_mut() = params.active_state().clone();
                    params.edit_state_mut().set_changed();
                }
                _ => {
                    eprintln!("Unhandled OSC address: {}", msg.addr);
                    eprintln!("Unhandled OSC arguments: {:?}", msg.args);
                    return Err(());
                }
            }
        }
        if params.active_state().fps.changed_incoming {
            fps_controller.set_fps(*params.active_state().fps);
            params.active_state_mut().fps.set_handled();
        }
        if params.active_state().beat_multiplier.changed_incoming {
            beat_predictor.lock().unwrap().multiplier = 0.5_f32.powi(*params.active_state().beat_multiplier);
            params.active_state_mut().beat_multiplier.set_handled();
        }
        if params.active_slot.changed_incoming {
            params.set_active_slot(*params.active_slot);
            params.active_slot.set_handled()
        }
        if params.edit_slot.changed_incoming {
            params.set_edit_slot(*params.edit_slot);
            params.edit_slot.set_handled()
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
