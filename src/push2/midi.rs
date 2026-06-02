// Push 2 MIDI Constants and Parsing

/// Pad note range: 36-99 (8x8 grid)
pub const PAD_NOTE_MIN: u8 = 36;
pub const PAD_NOTE_MAX: u8 = 99;

/// Get note number for pad at (row, col)
/// Row 0 is at the bottom, col 0 at the left
pub fn pad_note(row: u8, col: u8) -> u8 {
    PAD_NOTE_MIN + (row * 8) + col
}

/// Get (row, col) from pad note number
pub fn pad_position(note: u8) -> Option<(u8, u8)> {
    if note >= PAD_NOTE_MIN && note <= PAD_NOTE_MAX {
        let index = note - PAD_NOTE_MIN;
        Some((index / 8, index % 8))
    } else {
        None
    }
}

/// Decode relative encoder value (7-bit two's complement)
pub fn decode_encoder_delta(value: u8) -> i8 {
    if value < 64 {
        value as i8
    } else {
        (value as i16 - 128) as i8
    }
}

// Encoder CC numbers
pub const CC_ENCODER_1: u8 = 71;
pub const CC_ENCODER_2: u8 = 72;
pub const CC_ENCODER_3: u8 = 73;
pub const CC_ENCODER_4: u8 = 74;
pub const CC_ENCODER_5: u8 = 75;
pub const CC_ENCODER_6: u8 = 76;
pub const CC_ENCODER_7: u8 = 77;
pub const CC_ENCODER_8: u8 = 78;
pub const CC_MASTER_ENCODER: u8 = 79;
pub const CC_TEMPO_ENCODER: u8 = 14;
pub const CC_SWING_ENCODER: u8 = 15;

// Touchstrip
pub const NOTE_TOUCHSTRIP: u8 = 12;
pub const CC_TOUCHSTRIP: u8 = 1; // Mod wheel mode

// Sysex
pub const SYSEX_START: u8 = 0xF0;
pub const SYSEX_END: u8 = 0xF7;
pub const SYSEX_MANUFACTURER: [u8; 3] = [0x00, 0x21, 0x1D];
pub const SYSEX_DEVICE_ID: u8 = 0x01;
pub const SYSEX_MODEL_ID: u8 = 0x01;

// MIDI modes
pub const MIDI_MODE_LIVE: u8 = 0;
pub const MIDI_MODE_USER: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SysexCommand {
    SetLedColorPaletteEntry = 0x03,
    SetLedBrightness = 0x06,
    SetDisplayBrightness = 0x08,
    SetMidiMode = 0x0A,
    SetTouchStripConfig = 0x17,
    SetTouchStripLeds = 0x19,
    SetAftertouchMode = 0x1E,
}

pub fn build_sysex(command: SysexCommand, args: &[u8]) -> Vec<u8> {
    let mut msg = vec![
        SYSEX_START,
        SYSEX_MANUFACTURER[0],
        SYSEX_MANUFACTURER[1],
        SYSEX_MANUFACTURER[2],
        SYSEX_DEVICE_ID,
        SYSEX_MODEL_ID,
        command as u8,
    ];
    msg.extend_from_slice(args);
    msg.push(SYSEX_END);
    msg
}

/// LED Animation types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LedAnimation {
    #[default]
    None,
    OneShot24th,
    OneShot16th,
    OneShot8th,
    OneShotQuarter,
    OneShotHalf,
    Pulsing24th,
    Pulsing16th,
    Pulsing8th,
    PulsingQuarter,
    PulsingHalf,
    Blinking24th,
    Blinking16th,
    Blinking8th,
    BlinkingQuarter,
    BlinkingHalf,
}

impl LedAnimation {
    pub fn channel(self) -> u8 {
        match self {
            LedAnimation::None => 0,
            LedAnimation::OneShot24th => 1,
            LedAnimation::OneShot16th => 2,
            LedAnimation::OneShot8th => 3,
            LedAnimation::OneShotQuarter => 4,
            LedAnimation::OneShotHalf => 5,
            LedAnimation::Pulsing24th => 6,
            LedAnimation::Pulsing16th => 7,
            LedAnimation::Pulsing8th => 8,
            LedAnimation::PulsingQuarter => 9,
            LedAnimation::PulsingHalf => 10,
            LedAnimation::Blinking24th => 11,
            LedAnimation::Blinking16th => 12,
            LedAnimation::Blinking8th => 13,
            LedAnimation::BlinkingQuarter => 14,
            LedAnimation::BlinkingHalf => 15,
        }
    }
}

/// Color palette indices
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Off,
    White,
    LightGray,
    DarkGray,
    Red,
    Green,
    Blue,
    Yellow,
    Orange,
    Purple,
    Cyan,
    Pink,
    /// Custom palette index (0-127)
    Index(u8),
}

impl Color {
    pub fn index(self) -> u8 {
        match self {
            Color::Off => 0,
            Color::White => 122,
            Color::LightGray => 123,
            Color::DarkGray => 124,
            Color::Blue => 125,
            Color::Green => 126,
            Color::Red => 127,
            Color::Yellow => 13,
            Color::Orange => 9,
            Color::Purple => 49,
            Color::Cyan => 33,
            Color::Pink => 57,
            Color::Index(i) => i,
        }
    }
}

/// Parse incoming MIDI message into Push2Event
pub fn parse_midi_message(message: &[u8]) -> Option<super::Push2Event> {
    if message.len() < 2 {
        return None;
    }

    let status = message[0] & 0xF0;
    let _channel = message[0] & 0x0F;

    match status {
        // Note On
        0x90 if message.len() >= 3 => {
            let note = message[1];
            let velocity = message[2];

            if let Some((row, col)) = pad_position(note) {
                Some(super::Push2Event::Pad { row, col, velocity })
            } else if note == NOTE_TOUCHSTRIP {
                if velocity > 0 {
                    Some(super::Push2Event::TouchStripTouch)
                } else {
                    Some(super::Push2Event::TouchStripRelease)
                }
            } else {
                // Encoder touch (velocity > 0) or release (velocity == 0)
                encoder_from_note(note).map(|enc| super::Push2Event::EncoderTouch { encoder: enc, touched: velocity > 0 })
            }
        }
        // Note Off
        0x80 if message.len() >= 3 => {
            let note = message[1];

            if let Some((row, col)) = pad_position(note) {
                Some(super::Push2Event::Pad { row, col, velocity: 0 })
            } else if note == NOTE_TOUCHSTRIP {
                Some(super::Push2Event::TouchStripRelease)
            } else {
                encoder_from_note(note)
                    .map(|enc| super::Push2Event::EncoderTouch { encoder: enc, touched: false })
            }
        }
        // Control Change
        0xB0 if message.len() >= 3 => {
            let cc = message[1];
            let value = message[2];

            // Encoder
            if let Some(encoder) = encoder_from_cc(cc) {
                let delta = decode_encoder_delta(value);
                return Some(super::Push2Event::EncoderTurn { encoder, delta });
            }

            // Button
            if let Some(button) = super::Button::from_cc(cc) {
                return Some(super::Push2Event::Button { button, pressed: value > 0 });
            }

            // Touchstrip as mod wheel
            if cc == CC_TOUCHSTRIP {
                return Some(super::Push2Event::TouchStripMove { value });
            }

            None
        }
        // Pitch Bend (touchstrip)
        0xE0 if message.len() >= 3 => {
            let lsb = message[1] as u16;
            let msb = message[2] as u16;
            let value = ((msb << 7) | lsb) as u16;
            Some(super::Push2Event::TouchStripPitchBend { value })
        }
        _ => None,
    }
}

fn encoder_from_cc(cc: u8) -> Option<super::Encoder> {
    match cc {
        CC_ENCODER_1 => Some(super::Encoder::Track1),
        CC_ENCODER_2 => Some(super::Encoder::Track2),
        CC_ENCODER_3 => Some(super::Encoder::Track3),
        CC_ENCODER_4 => Some(super::Encoder::Track4),
        CC_ENCODER_5 => Some(super::Encoder::Track5),
        CC_ENCODER_6 => Some(super::Encoder::Track6),
        CC_ENCODER_7 => Some(super::Encoder::Track7),
        CC_ENCODER_8 => Some(super::Encoder::Track8),
        CC_MASTER_ENCODER => Some(super::Encoder::Master),
        CC_TEMPO_ENCODER => Some(super::Encoder::Tempo),
        CC_SWING_ENCODER => Some(super::Encoder::Swing),
        _ => None,
    }
}

fn encoder_from_note(note: u8) -> Option<super::Encoder> {
    // Encoder touch notes are 0-10, not the same as CC numbers
    match note {
        0 => Some(super::Encoder::Track1),
        1 => Some(super::Encoder::Track2),
        2 => Some(super::Encoder::Track3),
        3 => Some(super::Encoder::Track4),
        4 => Some(super::Encoder::Track5),
        5 => Some(super::Encoder::Track6),
        6 => Some(super::Encoder::Track7),
        7 => Some(super::Encoder::Track8),
        8 => Some(super::Encoder::Master),
        9 => Some(super::Encoder::Tempo),
        10 => Some(super::Encoder::Swing),
        _ => None,
    }
}
