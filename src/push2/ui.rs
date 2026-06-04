use anyhow::Result;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::Bgr565,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use super::{Button, Color, Encoder, Push2, Push2Event};
use crate::state::StreamingParams;
use crate::player::Uniforms;
use crate::osc_var::LoopRange;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::beat_predictor::BeatPredictor;
use crate::fps_loop::LoopController;

// Display dimensions
const DISPLAY_WIDTH: i32 = 960;
const LABEL_HEIGHT: i32 = 16;
const TIMELINE_HEIGHT: i32 = 40;
const TIMELINE_Y: i32 = LABEL_HEIGHT;

fn toggle<T: PartialEq + Copy>(var: &mut T, val: T, reset: T) {
    if var == &val {
        *var = reset;
    } else {
        *var = val;
    }
}

pub fn run(
    streaming_params: Arc<Mutex<StreamingParams>>,
    beat_predictor: Arc<Mutex<BeatPredictor>>,
    fps_controller: LoopController,
) -> Result<()> {
    loop {
        match run_connected(&streaming_params, &beat_predictor, &fps_controller) {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("Push 2 disconnected: {}. Reconnecting...", e);
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
}

fn run_connected(
    streaming_params: &Arc<Mutex<StreamingParams>>,
    beat_predictor: &Arc<Mutex<BeatPredictor>>,
    fps_controller: &LoopController,
) -> Result<()> {
    // Initialize Push 2
    let mut push2 = Push2::new()?;
    println!("Connected to Push 2");

    push2.set_user_mode()?;
    let mut fps_controller = fps_controller.clone();

    let mut last_redraw = Instant::now();

    loop {
        // Handle Push 2 events
        while let Some(event) = push2.poll_event() {
            match event {
                Push2Event::EncoderTurn { encoder, delta } => {
                    let mut params = streaming_params.lock().unwrap();
                    let _changed = handle_encoder(&mut params, &mut fps_controller, encoder, delta);
                }
                Push2Event::Button { button: Button::TrackAbove4, pressed: true } => {
                    // Reset frame repeat to 1.0
                    let mut params = streaming_params.lock().unwrap();
                    params.edit_state_mut().frame_repeat.set(1.0);
                }
                Push2Event::Button { button: Button::TapTempo, pressed: true } => {
                    // Send manual beat - trigger restart_loop if loop_to_beat is enabled
                    let mut params = streaming_params.lock().unwrap();
                    if !*params.use_external_beat {
                        beat_predictor.lock().unwrap().put_input_beat();
                    }
                }
                // Single strobe pad
                Push2Event::Pad { row: 5, col: 0, velocity } => {
                    let mut params = streaming_params.lock().unwrap();
                    let on = velocity > 0;
                    params.led_flash.set(on);
                    push2.set_pad_color(5, 0, if on {Color::White} else {Color::LightGray})?;
                }
                // LED flash beat toggles (row 6, col 0-3)
                Push2Event::Pad { row: 6, col: 0, velocity } if velocity > 0 => {
                    let mut params = streaming_params.lock().unwrap();
                    toggle(&mut params.led_flash_beats, Some(1), None);
                }
                Push2Event::Pad { row: 6, col: 1, velocity } if velocity > 0 => {
                    let mut params = streaming_params.lock().unwrap();
                    toggle(&mut params.led_flash_beats, Some(2), None);
                }
                Push2Event::Pad { row: 6, col: 2, velocity } if velocity > 0 => {
                    let mut params = streaming_params.lock().unwrap();
                    toggle(&mut params.led_flash_beats, Some(4), None);
                }
                Push2Event::Pad { row: 6, col: 3, velocity } if velocity > 0 => {
                    let mut params = streaming_params.lock().unwrap();
                    toggle(&mut params.led_flash_beats, Some(8), None);
                }
                // Record loop
                Push2Event::Pad { row: 7, col: 6, velocity } => {
                    let mut params = streaming_params.lock().unwrap();
                    let playhead = *params.edit_state().playhead;
                    let (start, end) = params.edit_state().loop_range.0.unwrap_or((0.0, 1.0));
                    if velocity > 0 {
                        params.edit_state_mut().loop_range.set(LoopRange(Some((playhead, 1.0))));
                    } else {
                        params.edit_state_mut().loop_range.set(LoopRange(Some((start, playhead))));
                    }
                }
                // Clear loop
                Push2Event::Pad { row: 7, col: 7, velocity } if velocity > 0 => {
                    let mut params = streaming_params.lock().unwrap();
                    params.edit_state_mut().loop_range.set(LoopRange(Some((0.0, 1.0))));
                }
                // Video blackout toggle
                Push2Event::Pad { row: 7, col: 0, velocity } if velocity > 0 => {
                    let mut params = streaming_params.lock().unwrap();
                    let last_val = *params.video_blackout;
                    params.video_blackout.set(!last_val);
                }
                // Video to LED weak
                Push2Event::Pad { row: 7, col: 1, velocity } if velocity > 0 => {
                    let mut params = streaming_params.lock().unwrap();
                    toggle(&mut params.video_to_led, Some(0.6), None);
                }
                // Video to LED strong
                Push2Event::Pad { row: 7, col: 2, velocity } if velocity > 0 => {
                    let mut params = streaming_params.lock().unwrap();
                    toggle(&mut params.video_to_led, Some(1.0), None);
                }
                // Loop to beat toggle (button under End encoder)
                Push2Event::Button { button: Button::TrackAbove8, pressed: true } => {
                    let mut params = streaming_params.lock().unwrap();
                    let current = *params.edit_state().loop_to_beat;
                    params.edit_state_mut().loop_to_beat.set(!current);
                }
                _ => {}
            }
        }

        // Update display and button state periodically
        if last_redraw.elapsed().as_millis() >= 30 {
            // Read only copy of params
            let params = { streaming_params.lock().unwrap().clone() };

            // Blackout button
            push2.set_pad_color(7, 0, if *params.video_blackout {Color::Red} else {Color::Blue})?;
            // Video to LED passthrough buttons (green off, red on)
            push2.set_pad_color(7, 1, if params.video_to_led == Some(0.6) {Color::Red} else {Color::Green})?;
            push2.set_pad_color(7, 2, if params.video_to_led == Some(1.0) {Color::Red} else {Color::Green})?;

            // LED flash beat buttons (row 6, col 0-3)
            push2.set_pad_color(6, 0, if params.led_flash_beats == Some(1) {Color::White} else {Color::LightGray})?;
            push2.set_pad_color(6, 1, if params.led_flash_beats == Some(2) {Color::White} else {Color::LightGray})?;
            push2.set_pad_color(6, 2, if params.led_flash_beats == Some(4) {Color::White} else {Color::LightGray})?;
            push2.set_pad_color(6, 3, if params.led_flash_beats == Some(8) {Color::White} else {Color::LightGray})?;
            // Single strobe pad
            push2.set_pad_color(5, 0, Color::LightGray)?;
            // Record loop and Clear loop
            push2.set_pad_color(7, 6, Color::Red)?;
            push2.set_pad_color(7, 7, Color::Orange)?;

            // Loop to beat button (under End encoder)
            push2.set_button_color(Button::TrackAbove8, if *params.edit_state().loop_to_beat {Color::Green} else {Color::Index(17)})?;

            // Flash Tap button with beat
            if let Some(dur) = beat_predictor.lock().unwrap().duration_since_last_beat(*params.beat_offset) {
                if dur.as_millis() < 100 {
                    push2.set_button_color(Button::TapTempo, Color::White)?;
                } else {
                    push2.set_button_color(Button::TapTempo, Color::Off)?;
                }
            }

            draw_timeline(&mut push2, &params)?;
            draw_encoders(&mut push2, &params)?;
            push2.display.flush()?;
            last_redraw = Instant::now();
        }

        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn handle_encoder(params: &mut StreamingParams, fps_controller: &mut LoopController, encoder: Encoder, delta: i8) -> bool {
    let sensitivity = 0.005;
    let change = delta as f32 * sensitivity;
    let state = params.edit_state_mut();

    match encoder {
        // Encoder 1: LED Gain (log-ish)
        Encoder::Track1 => {
            let current = *params.led_gain;
            let scale = current.max(0.01);
            let new_val = (current + change * scale * 4.0).clamp(0.0, 1.0);
            params.led_gain.set(new_val);
            true
        }
        // Encoder 2: Video Gain (log-ish)
        Encoder::Track2 => {
            let current = *params.video_gain;
            let scale = current.max(0.01);
            let new_val = (current + change * scale * 4.0).clamp(0.0, 1.0);
            params.video_gain.set(new_val);
            true
        }
        // Encoder 3: FPS (logarithmic)
        Encoder::Track3 => {
            let current = *state.fps;
            let scale = current.max(1.0);
            let new_val = (current + change * scale * 4.0).clamp(1.0, 120.0);
            state.fps.set(new_val);
            fps_controller.set_fps(new_val);
            true
        }
        // Encoder 4: Frame repeat (logarithmic)
        Encoder::Track4 => {
            let current = *state.frame_repeat;
            let scale = current.max(0.1);
            let new_val = (current + change * scale * 4.0).clamp(0.1, 10.0);
            state.frame_repeat.set(new_val);
            true
        }
        // Encoder 5: Shift loop left/right
        Encoder::Track5 => {
            let (start, end) = state.loop_range.0.unwrap_or((0.0, 1.0));
            let length = end - start;
            let scale = length.max(0.01);
            let shift = change * scale * 4.0;
            let new_start = (start + shift).clamp(0.0, 1.0 - length);
            state.loop_range.set(LoopRange(Some((new_start, new_start + length))));
            true
        }
        // Encoder 6: Change loop length (scales around center, logarithmic)
        Encoder::Track6 => {
            let (start, end) = state.loop_range.0.unwrap_or((0.0, 1.0));
            let center = (start + end) / 2.0;
            let half_length = (end - start) / 2.0;
            let scale = half_length.max(0.005);
            let new_half_length = (half_length + change * scale * 4.0).max(0.0);
            let new_start = (center - new_half_length).clamp(0.0, 1.0);
            let new_end = (center + new_half_length).clamp(0.0, 1.0);
            state.loop_range.set(LoopRange(Some((new_start, new_end))));
            true
        }
        // Encoder 7: Loop start point
        Encoder::Track7 => {
            let (start, end) = state.loop_range.0.unwrap_or((0.0, 1.0));
            let new_start = (start + change).clamp(0.0, end);
            state.loop_range.set(LoopRange(Some((new_start, end))));
            true
        }
        // Encoder 8: Loop end point
        Encoder::Track8 => {
            let (start, end) = state.loop_range.0.unwrap_or((0.0, 1.0));
            let new_end = (end + change).clamp(start, 1.0);
            state.loop_range.set(LoopRange(Some((start, new_end))));
            true
        }
        _ => false,
    }
}

fn draw_encoders(push2: &mut Push2, params: &StreamingParams) -> Result<()> {
    let state = params.edit_state();
    let display = &mut push2.display;

    let bg_color = Bgr565::new(0, 0, 0);
    let text_color = Bgr565::new(20, 40, 20); // Light gray

    // Clear label area
    Rectangle::new(
        Point::new(0, 0),
        Size::new(DISPLAY_WIDTH as u32, LABEL_HEIGHT as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(bg_color))
    .draw(display)?;

    // Draw encoder labels with values
    let text_style = MonoTextStyle::new(&FONT_6X10, text_color);
    let encoder_width = DISPLAY_WIDTH / 8;

    // Format labels with current values
    let led_gain_label = format!("LED {:.2}", *params.led_gain);
    let video_gain_label = format!("Video {:.2}", *params.video_gain);
    let fps_label = format!("FPS {:.0}", *state.fps);
    let repeat_label = format!("Repeat {:.1}", *state.frame_repeat);

    let labels: [(i32, &str); 8] = [
        (1, &led_gain_label),
        (2, &video_gain_label),
        (3, &fps_label),
        (4, &repeat_label),
        (5, "Shift"),
        (6, "Length"),
        (7, "Start"),
        (8, "End"),
    ];
    for (encoder_num, label) in labels {
        let x = (encoder_num - 1) * encoder_width + encoder_width / 2;
        Text::with_alignment(label, Point::new(x, 10), text_style, Alignment::Center)
            .draw(display)?;
    }
    Ok(())
}


fn draw_timeline(push2: &mut Push2, params: &StreamingParams) -> Result<()> {
    let state = params.edit_state();
    let display = &mut push2.display;

    let bg_color = Bgr565::new(0, 0, 0);
    let playhead_color = Bgr565::new(0, 20, 31);
    let timeline_color = Bgr565::new(8, 16, 8);
    let loop_color = Bgr565::new(31, 20, 0);
    let bar_color = Bgr565::new(31, 63, 31);

    // Clear timeline area
    Rectangle::new(
        Point::new(0, TIMELINE_Y),
        Size::new(DISPLAY_WIDTH as u32, TIMELINE_HEIGHT as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(bg_color))
    .draw(display)?;

    // Draw timeline background bar
    let bar_y = TIMELINE_Y + TIMELINE_HEIGHT / 2;
    let bar_height = 4;
    Rectangle::new(
        Point::new(0, bar_y - bar_height / 2),
        Size::new(DISPLAY_WIDTH as u32, bar_height as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(timeline_color))
    .draw(display)?;

    // Get loop range and playhead
    let (loop_start, loop_end) = state.loop_range.0.unwrap_or((0.0, 1.0));
    let playhead = *state.playhead;

    // Calculate pixel positions
    let start_x = (loop_start * DISPLAY_WIDTH as f32) as i32;
    let end_x = (loop_end * DISPLAY_WIDTH as f32) as i32;
    let playhead_x = (playhead * DISPLAY_WIDTH as f32) as i32;

    // Draw loop region (filled area between bars)
    if end_x > start_x {
        Rectangle::new(
            Point::new(start_x, bar_y - bar_height / 2),
            Size::new((end_x - start_x) as u32, bar_height as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(loop_color))
        .draw(display)?;
    }

    // Draw start bar (vertical line)
    Line::new(
        Point::new(start_x, TIMELINE_Y + 4),
        Point::new(start_x, TIMELINE_Y + TIMELINE_HEIGHT - 4),
    )
    .into_styled(PrimitiveStyle::with_stroke(bar_color, 3))
    .draw(display)?;

    // Draw end bar (vertical line)
    Line::new(
        Point::new(end_x, TIMELINE_Y + 4),
        Point::new(end_x, TIMELINE_Y + TIMELINE_HEIGHT - 4),
    )
    .into_styled(PrimitiveStyle::with_stroke(bar_color, 3))
    .draw(display)?;

    // Draw playhead
    Line::new(
        Point::new(playhead_x, TIMELINE_Y + 2),
        Point::new(playhead_x, TIMELINE_Y + TIMELINE_HEIGHT - 2),
    )
    .into_styled(PrimitiveStyle::with_stroke(playhead_color, 2))
    .draw(display)?;

    Ok(())
}
