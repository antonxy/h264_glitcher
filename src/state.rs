use crate::osc_var::{OscVar, LoopRange, OscValue};
use rosc::OscMessage;
use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

#[derive(Clone)]
pub struct State {
    pub video_num: OscVar<i32>,
    pub beat_multiplier: OscVar<i32>,
    pub pass_iframe: OscVar<bool>,
    pub playhead: OscVar<f32>,
    pub loop_range: OscVar<LoopRange>,
    pub auto_skip: OscVar<bool>,
    pub frame_repeat: OscVar<f32>, // Values < 1 drop frames, non integer values drop / repeat frames sometimes
    pub loop_to_beat: OscVar<bool>,
    pub fps: OscVar<f32>,
    pub auto_switch_n: OscVar<i32>,
    pub switch_history: VecDeque<usize>,
    pub byte_errors: OscVar<f32>,
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
    pub fn send_changed(&mut self, socket: &UdpSocket, client_addr: &SocketAddr) {
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

    pub fn set_changed(&mut self) {
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

    pub fn handle_osc_message(&mut self, msg: &OscMessage) -> bool {
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
pub struct ShortLoop {
    pub first_frame: Option<usize>,
    pub len: usize,
}

#[derive(Clone)]
pub struct StreamingParams {
    pub restart_loop: bool,
    pub skip_frames: Option<usize>,
    pub client_addr: Option<SocketAddr>,
    pub short_loop: Option<ShortLoop>,

    pub use_external_beat: OscVar<bool>,
    pub beat_offset: OscVar<Duration>,
    pub beat_divider: u32,

    pub video_gain: OscVar<f32>,
    pub video_blackout: OscVar<bool>,
    pub led_gain: OscVar<f32>,
    pub led_flash: OscVar<bool>,

    pub state_slots: Vec<State>,
    pub active_slot: OscVar<usize>,
    pub edit_slot: OscVar<usize>,

    pub is_live: OscVar<bool>,
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

            video_gain: OscVar::new("/video_gain", 1.0),
            video_blackout: OscVar::new("/video_blackout", false),
            led_gain: OscVar::new("/led_gain", 1.0),
            led_flash: OscVar::new("/led_flash", false),

            state_slots: vec![State::default(); 6],
            active_slot: OscVar::new("/active_slot", 0),
            edit_slot: OscVar::new("/edit_slot", 0),

            is_live: OscVar::new("/is_live", true),
        }
    }
}

impl StreamingParams {
    pub fn set_active_slot(&mut self, slot: usize) {
        assert!(slot < 6);
        self.active_slot.set(slot);
        self.active_state_mut().set_changed();

        self.is_live.set(*self.active_slot == *self.edit_slot);
    }

    pub fn set_edit_slot(&mut self, slot: usize) {
        assert!(slot < 6);
        self.edit_slot.set(slot);
        self.edit_state_mut().set_changed();

        self.is_live.set(*self.active_slot == *self.edit_slot);
    }

    pub fn active_state(&self) -> &State {
        &self.state_slots[*self.active_slot]
    }

    pub fn active_state_mut(&mut self) -> &mut State {
        &mut self.state_slots[*self.active_slot]
    }

    pub fn edit_state(&self) -> &State {
        &self.state_slots[*self.edit_slot]
    }

    pub fn edit_state_mut(&mut self) -> &mut State {
        &mut self.state_slots[*self.edit_slot]
    }

    pub fn send_changed(&mut self, socket: &UdpSocket, client_addr: &SocketAddr) {
        self.use_external_beat.send_if_changed(socket, client_addr);
        self.beat_offset.send_if_changed(socket, client_addr);
        self.active_slot.send_if_changed(socket, client_addr);
        self.edit_slot.send_if_changed(socket, client_addr);
        self.edit_state_mut().send_changed(socket, client_addr);
        self.is_live.send_if_changed(socket, client_addr);
    }

    pub fn handle_osc_message(&mut self, msg: &OscMessage) -> bool {
        self.use_external_beat.handle_osc_message(msg) ||
        self.beat_offset.handle_osc_message(msg) ||
        self.active_slot.handle_osc_message(msg) ||
        self.edit_slot.handle_osc_message(msg) ||
        self.edit_state_mut().handle_osc_message(msg)
    }
}
