use crate::osc_var::{OscVar, LoopRange};
use rosc::OscMessage;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;
use crate::beat_predictor::BeatGrid;


// State of the stream_loop, should rename
#[derive(Clone)]
pub struct State {
    pub video_num: OscVar<i32>,
    pub pass_iframe: OscVar<bool>,
    pub playhead: OscVar<f32>,
    pub loop_range: OscVar<LoopRange>,
    pub frame_repeat: OscVar<f32>, // Values < 1 drop frames, non integer values drop / repeat frames sometimes
    pub fps: OscVar<f32>, // TODO setting invalid fps can crash the program
}

impl Default for State {
    fn default() -> Self {
        Self {
            video_num: OscVar::new("/video_num", 0),
            pass_iframe: OscVar::new("/pass_iframe", false),
            playhead: OscVar::new("/playhead", 0.0),
            loop_range: OscVar::new("/loop_range", LoopRange(None)),
            frame_repeat: OscVar::new("/frame_repeat", 1.0),
            fps: OscVar::new("/fps", 30.0),
        }
    }
}

impl State {
    pub fn send_changed(&mut self, socket: &UdpSocket, client_addr: &SocketAddr) {
        self.video_num.send_if_changed(socket, client_addr);
        self.pass_iframe.send_if_changed(socket, client_addr);
        self.playhead.send_if_changed(socket, client_addr);
        self.loop_range.send_if_changed(socket, client_addr);
        self.frame_repeat.send_if_changed(socket, client_addr);
        self.fps.send_if_changed(socket, client_addr);
    }

    pub fn set_changed(&mut self) {
        self.video_num.set_changed();
        self.pass_iframe.set_changed();
        self.playhead.set_changed();
        self.loop_range.set_changed();
        self.frame_repeat.set_changed();
        self.fps.set_changed();
    }

    pub fn handle_osc_message(&mut self, msg: &OscMessage) -> bool {
        self.video_num.handle_osc_message(msg) ||
        self.pass_iframe.handle_osc_message(msg) ||
        //self.playhead.handle_osc_message(msg) ||
        self.loop_range.handle_osc_message(msg) ||
        self.frame_repeat.handle_osc_message(msg) ||
        self.fps.handle_osc_message(msg)
    }
}

#[derive(Clone, Debug)]
pub struct ShortLoop {
    pub first_frame: Option<usize>,
    pub len: usize,
}

#[derive(Clone)]
pub struct StreamingParams {
    pub skip_frames: Option<usize>,
    pub short_loop: Option<ShortLoop>,
    pub restart_loop: bool, // request to jump back to start of loop. TODO would be better if we can set playhead directly

    pub beat_grid: BeatGrid,
    pub beat_offset: OscVar<Duration>,
    pub beat_divider: u32,

    pub video_gain: OscVar<f32>,
    pub video_blackout: OscVar<bool>,
    pub led_gain: OscVar<f32>,
    pub led_flash: OscVar<bool>,

    pub led_flash_beats: Option<u32>,
    pub video_to_led: Option<f32>,

    pub state: State,
}

impl Default for StreamingParams {
    fn default() -> Self {
        Self {
            skip_frames: None,
            short_loop: None,
            restart_loop: false,

            beat_grid: BeatGrid::default(),
            beat_offset: OscVar::new("/beat_offset", Duration::from_millis(0)),
            beat_divider: 1,

            video_gain: OscVar::new("/video_gain", 1.0),
            video_blackout: OscVar::new("/video_blackout", false),
            led_gain: OscVar::new("/led_gain", 1.0),
            led_flash: OscVar::new("/led_flash", false),

            led_flash_beats: None,
            video_to_led: None,

            state: State::default(),
        }
    }
}

impl StreamingParams {
    pub fn send_changed(&mut self, socket: &UdpSocket, client_addr: &SocketAddr) {
        self.beat_offset.send_if_changed(socket, client_addr);
        self.state.send_changed(socket, client_addr);
    }

    pub fn handle_osc_message(&mut self, msg: &OscMessage) -> bool {
        self.beat_offset.handle_osc_message(msg) ||
        self.state.handle_osc_message(msg)
    }

    pub fn active_state(&self) -> &State {
        &self.state
    }

    pub fn active_state_mut(&mut self) -> &mut State {
        &mut self.state
    }

    pub fn edit_state(&self) -> &State {
        &self.state
    }

    pub fn edit_state_mut(&mut self) -> &mut State {
        &mut self.state
    }
}
