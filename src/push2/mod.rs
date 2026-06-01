mod midi;
mod controls;
pub mod ui;

pub use midi::*;
pub use controls::*;
pub use ui::run;

use anyhow::{Context, Result};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use push2_display::Push2Display;
use std::sync::mpsc::{self, Receiver, Sender};

pub struct Push2 {
    pub display: Push2Display,
    midi_out: MidiOutputConnection,
    _midi_in: MidiInputConnection<()>,
    event_rx: Receiver<Push2Event>,
}

impl Push2 {
    pub fn new() -> Result<Self> {
        // Initialize display
        let display = Push2Display::new().context("Failed to connect to Push 2 display")?;

        // Initialize MIDI
        let midi_in = MidiInput::new("push2control-in")?;
        let midi_out = MidiOutput::new("push2control-out")?;

        let in_ports = midi_in.ports();
        let out_ports = midi_out.ports();

        // Print available ports
        println!("Available MIDI input ports:");
        for (i, p) in in_ports.iter().enumerate() {
            println!("  {}: {}", i, midi_in.port_name(p).unwrap_or_default());
        }
        println!("Available MIDI output ports:");
        for (i, p) in out_ports.iter().enumerate() {
            println!("  {}: {}", i, midi_out.port_name(p).unwrap_or_default());
        }

        // Find Push 2 User Port (for User mode) - fall back to any Push 2 port
        let push2_in = in_ports
            .iter()
            .find(|p| {
                midi_in
                    .port_name(p)
                    .map(|n| n.contains("Ableton Push 2") && n.contains("User"))
                    .unwrap_or(false)
            })
            .or_else(|| {
                in_ports.iter().find(|p| {
                    midi_in
                        .port_name(p)
                        .map(|n| n.contains("Ableton Push 2"))
                        .unwrap_or(false)
                })
            })
            .context("Push 2 MIDI input not found")?
            .clone();

        let push2_out = out_ports
            .iter()
            .find(|p| {
                midi_out
                    .port_name(p)
                    .map(|n| n.contains("Ableton Push 2") && n.contains("User"))
                    .unwrap_or(false)
            })
            .or_else(|| {
                out_ports.iter().find(|p| {
                    midi_out
                        .port_name(p)
                        .map(|n| n.contains("Ableton Push 2"))
                        .unwrap_or(false)
                })
            })
            .context("Push 2 MIDI output not found")?
            .clone();

        println!("Using MIDI input: {}", midi_in.port_name(&push2_in).unwrap_or_default());
        println!("Using MIDI output: {}", midi_out.port_name(&push2_out).unwrap_or_default());

        // Setup event channel
        let (event_tx, event_rx): (Sender<Push2Event>, Receiver<Push2Event>) = mpsc::channel();

        // Connect MIDI output
        let midi_out = midi_out
            .connect(&push2_out, "push2control-out")
            .map_err(|e| anyhow::anyhow!("MIDI output connection failed: {}", e))?;

        // Connect MIDI input with callback
        let _midi_in = midi_in
            .connect(
                &push2_in,
                "push2control-in",
                move |_stamp, message, _| {
                    if let Some(event) = parse_midi_message(message) {
                        let _ = event_tx.send(event);
                    }
                },
                (),
            )
            .map_err(|e| anyhow::anyhow!("MIDI input connection failed: {}", e))?;

        Ok(Self {
            display,
            midi_out,
            _midi_in,
            event_rx,
        })
    }

    /// Poll for the next event (non-blocking)
    pub fn poll_event(&self) -> Option<Push2Event> {
        self.event_rx.try_recv().ok()
    }

    /// Wait for the next event (blocking)
    pub fn wait_event(&self) -> Option<Push2Event> {
        self.event_rx.recv().ok()
    }

    /// Set pad LED color
    pub fn set_pad_color(&mut self, row: u8, col: u8, color: Color) -> Result<()> {
        let note = pad_note(row, col);
        self.send_led(note, color, LedAnimation::None)
    }

    /// Set pad LED color with animation
    pub fn set_pad_color_animated(
        &mut self,
        row: u8,
        col: u8,
        color: Color,
        animation: LedAnimation,
    ) -> Result<()> {
        let note = pad_note(row, col);
        self.send_led(note, color, animation)
    }

    /// Set button LED color
    pub fn set_button_color(&mut self, button: Button, color: Color) -> Result<()> {
        self.send_cc_led(button.cc(), color, LedAnimation::None)
    }

    /// Set button LED color with animation
    pub fn set_button_color_animated(
        &mut self,
        button: Button,
        color: Color,
        animation: LedAnimation,
    ) -> Result<()> {
        self.send_cc_led(button.cc(), color, animation)
    }

    /// Clear all pad LEDs
    pub fn clear_pads(&mut self) -> Result<()> {
        for row in 0..8 {
            for col in 0..8 {
                self.set_pad_color(row, col, Color::Off)?;
            }
        }
        Ok(())
    }

    fn send_led(&mut self, note: u8, color: Color, animation: LedAnimation) -> Result<()> {
        let status = 0x90 | animation.channel();
        self.midi_out.send(&[status, note, color.index()])?;
        Ok(())
    }

    fn send_cc_led(&mut self, cc: u8, color: Color, animation: LedAnimation) -> Result<()> {
        let status = 0xB0 | animation.channel();
        self.midi_out.send(&[status, cc, color.index()])?;
        Ok(())
    }

    /// Send raw MIDI message
    pub fn send_midi(&mut self, message: &[u8]) -> Result<()> {
        self.midi_out.send(message)?;
        Ok(())
    }

    /// Send sysex to set Push 2 to User mode
    pub fn set_user_mode(&mut self) -> Result<()> {
        let sysex = build_sysex(SysexCommand::SetMidiMode, &[MIDI_MODE_USER]);
        self.midi_out.send(&sysex)?;
        Ok(())
    }

    /// Send sysex to set Push 2 to Live mode
    pub fn set_live_mode(&mut self) -> Result<()> {
        let sysex = build_sysex(SysexCommand::SetMidiMode, &[MIDI_MODE_LIVE]);
        self.midi_out.send(&sysex)?;
        Ok(())
    }
}
