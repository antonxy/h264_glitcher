use std::{collections::HashMap, sync::{Arc, Mutex}};

use rosc::{OscMessage, OscType};

use crate::{osc_var::LoopRange, state::{ShortLoop, State, StreamingParams}};

#[derive(Clone)]
pub struct OSCHandler {
    pub video_index: HashMap<String, i32>,
    pub streaming_params: Arc<Mutex<StreamingParams>>,
}

impl OSCHandler {
    pub fn handle_osc_message(&self, msg: &OscMessage) -> Result<(), ()> {
        let mut params = self.streaming_params.lock().unwrap();
        if params.handle_osc_message(&msg) {}
        else {
            match msg.addr.as_str() {
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
                            match self.video_index.get(name) {
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
    }
}
