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

// Display dimensions
const DISPLAY_WIDTH: i32 = 960;
const LABEL_HEIGHT: i32 = 16;
const TIMELINE_HEIGHT: i32 = 40;
const TIMELINE_Y: i32 = LABEL_HEIGHT;

const BEAT_FLASH_DURATION_MS: u128 = 100;

pub fn run(
    streaming_params: Arc<Mutex<StreamingParams>>,
    uniforms: Arc<Mutex<Uniforms>>,
) -> Result<()> {
    println!("Push2Control - Loop Controller");

    // Initialize Push 2
    let mut push2 = Push2::new()?;
    println!("Connected to Push 2");

    push2.set_user_mode()?;

    let mut needs_redraw = true;
    let mut last_redraw = Instant::now();
    let mut last_beat: Option<Instant> = None;

    // Set initial button color
    {
        let params = streaming_params.lock().unwrap();
        update_repeat_button(&mut push2, *params.active_state().frame_repeat)?;
    }

    println!("Ready!");

    loop {
        // Handle Push 2 events
        while let Some(event) = push2.poll_event() {
            match event {
                Push2Event::EncoderTurn { encoder, delta } => {
                    let mut params = streaming_params.lock().unwrap();
                    let changed = handle_encoder(&mut params, encoder, delta);
                    if changed {
                        match encoder {
                            Encoder::Track4 => {
                                update_repeat_button(&mut push2, *params.active_state().frame_repeat)?;
                            }
                            _ => {}
                        }
                        needs_redraw = true;
                    }
                }
                Push2Event::ButtonPressed { button: Button::TrackAbove4 } => {
                    // Reset frame repeat to 1.0
                    let mut params = streaming_params.lock().unwrap();
                    params.active_state_mut().frame_repeat.set(1.0);
                    update_repeat_button(&mut push2, 1.0)?;
                    needs_redraw = true;
                }
                Push2Event::ButtonPressed { button: Button::TapTempo } => {
                    // Send manual beat - trigger restart_loop if loop_to_beat is enabled
                    let mut params = streaming_params.lock().unwrap();
                    if *params.active_state().loop_to_beat {
                        params.restart_loop = true;
                    }
                    // Flash the LED
                    last_beat = Some(Instant::now());
                    push2.set_button_color(Button::TapTempo, Color::White)?;
                }
                // Top right pad (row 7, col 7) - LED flash for compositor
                Push2Event::PadPressed { row: 7, col: 7, .. } => {
                    let mut u = uniforms.lock().unwrap();
                    u.flash = 1.0;
                    push2.set_pad_color(7, 7, Color::White)?;
                }
                Push2Event::PadReleased { row: 7, col: 7 } => {
                    let mut u = uniforms.lock().unwrap();
                    u.flash = 0.0;
                    push2.set_pad_color(7, 7, Color::Off)?;
                }
                _ => {}
            }
        }

        // Turn off beat LED after flash duration
        if let Some(beat_time) = last_beat {
            if beat_time.elapsed().as_millis() > BEAT_FLASH_DURATION_MS {
                push2.set_button_color(Button::TapTempo, Color::Off)?;
                last_beat = None;
            }
        }

        // Redraw display if needed or periodically
        if needs_redraw || last_redraw.elapsed().as_secs() >= 1 {
            let params = streaming_params.lock().unwrap();
            draw_timeline(&mut push2, &params)?;
            needs_redraw = false;
            last_redraw = Instant::now();
        }

        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn handle_encoder(params: &mut StreamingParams, encoder: Encoder, delta: i8) -> bool {
    let sensitivity = 0.005;
    let change = delta as f32 * sensitivity;
    let state = params.active_state_mut();

    match encoder {
        // Encoder 3: FPS (logarithmic)
        Encoder::Track3 => {
            let current = *state.fps;
            let scale = current.max(1.0);
            let new_val = (current + change * scale * 4.0).clamp(1.0, 120.0);
            state.fps.set(new_val);
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

fn update_repeat_button(push2: &mut Push2, frame_repeat: f32) -> Result<()> {
    let color = if (frame_repeat - 1.0).abs() < 0.01 {
        Color::Blue
    } else {
        Color::Red
    };
    push2.set_button_color(Button::TrackAbove4, color)
}

fn draw_timeline(push2: &mut Push2, params: &StreamingParams) -> Result<()> {
    let state = params.active_state();
    let display = &mut push2.display;

    // Colors (Bgr565: blue, green, red - each 5, 6, 5 bits)
    let bg_color = Bgr565::new(2, 4, 2); // Dark gray
    let timeline_color = Bgr565::new(8, 16, 8); // Slightly lighter
    let loop_color = Bgr565::new(31, 20, 0); // Cyan (high blue)
    let bar_color = Bgr565::new(31, 63, 31); // White
    let playhead_color = Bgr565::new(0, 20, 31); // Orange (high red)
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
    let fps_label = format!("FPS {:.0}", *state.fps);
    let repeat_label = format!("Repeat {:.1}", *state.frame_repeat);

    let labels: [(i32, &str); 6] = [
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

    // Flush to display
    push2.display.flush()?;

    Ok(())
}
