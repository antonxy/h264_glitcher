// Push 2 Control Definitions

/// Push 2 events
#[derive(Debug, Clone, PartialEq)]
pub enum Push2Event {
    Pad { row: u8, col: u8, velocity: u8 },
    Button { button: Button, pressed: bool },
    EncoderTurn { encoder: Encoder, delta: i8 },
    EncoderTouch { encoder: Encoder, touched: bool },
    TouchStripTouch,
    TouchStripRelease,
    TouchStripMove { value: u8 },
    TouchStripPitchBend { value: u16 },
}

/// Encoders on Push 2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoder {
    Track1,
    Track2,
    Track3,
    Track4,
    Track5,
    Track6,
    Track7,
    Track8,
    Master,
    Tempo,
    Swing,
}

impl Encoder {
    pub fn index(self) -> usize {
        match self {
            Encoder::Track1 => 0,
            Encoder::Track2 => 1,
            Encoder::Track3 => 2,
            Encoder::Track4 => 3,
            Encoder::Track5 => 4,
            Encoder::Track6 => 5,
            Encoder::Track7 => 6,
            Encoder::Track8 => 7,
            Encoder::Master => 8,
            Encoder::Tempo => 9,
            Encoder::Swing => 10,
        }
    }

    pub fn is_track(self) -> bool {
        matches!(
            self,
            Encoder::Track1
                | Encoder::Track2
                | Encoder::Track3
                | Encoder::Track4
                | Encoder::Track5
                | Encoder::Track6
                | Encoder::Track7
                | Encoder::Track8
        )
    }
}

/// All buttons on Push 2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    // Transport
    Play,
    Record,
    New,
    Duplicate,
    Automate,
    FixedLength,

    // Navigation
    Left,
    Right,
    Up,
    Down,

    // Track buttons below display
    TrackBelow1,
    TrackBelow2,
    TrackBelow3,
    TrackBelow4,
    TrackBelow5,
    TrackBelow6,
    TrackBelow7,
    TrackBelow8,

    // Track buttons above display
    TrackAbove1,
    TrackAbove2,
    TrackAbove3,
    TrackAbove4,
    TrackAbove5,
    TrackAbove6,
    TrackAbove7,
    TrackAbove8,

    // Mode
    Master,
    StopClip,
    Setup,
    Layout,
    Device,
    Browse,
    Mix,
    Clip,

    // Edit
    Quantize,
    DoubleLoop,
    Delete,
    Undo,

    // Performance
    Repeat,
    Accent,
    Scale,
    User,
    Mute,
    Solo,
    PageLeft,
    PageRight,

    // Utility
    Select,
    Shift,
    Note,
    Session,
    AddDevice,
    AddTrack,
    OctaveDown,
    OctaveUp,
    Convert,
    TapTempo,
    Metronome,
}

impl Button {
    /// Get CC number for this button
    pub fn cc(self) -> u8 {
        match self {
            // Transport
            Button::Play => 85,
            Button::Record => 86,
            Button::New => 87,
            Button::Duplicate => 88,
            Button::Automate => 89,
            Button::FixedLength => 90,

            // Navigation
            Button::Left => 44,
            Button::Right => 45,
            Button::Up => 46,
            Button::Down => 47,

            // Track buttons below display
            Button::TrackBelow1 => 20,
            Button::TrackBelow2 => 21,
            Button::TrackBelow3 => 22,
            Button::TrackBelow4 => 23,
            Button::TrackBelow5 => 24,
            Button::TrackBelow6 => 25,
            Button::TrackBelow7 => 26,
            Button::TrackBelow8 => 27,

            // Track buttons above display
            Button::TrackAbove1 => 102,
            Button::TrackAbove2 => 103,
            Button::TrackAbove3 => 104,
            Button::TrackAbove4 => 105,
            Button::TrackAbove5 => 106,
            Button::TrackAbove6 => 107,
            Button::TrackAbove7 => 108,
            Button::TrackAbove8 => 109,

            // Mode
            Button::Master => 28,
            Button::StopClip => 29,
            Button::Setup => 30,
            Button::Layout => 31,
            Button::Device => 110,
            Button::Browse => 111,
            Button::Mix => 112,
            Button::Clip => 113,

            // Edit
            Button::Quantize => 116,
            Button::DoubleLoop => 117,
            Button::Delete => 118,
            Button::Undo => 119,

            // Performance
            Button::Repeat => 56,
            Button::Accent => 57,
            Button::Scale => 58,
            Button::User => 59,
            Button::Mute => 60,
            Button::Solo => 61,
            Button::PageLeft => 62,
            Button::PageRight => 63,

            // Utility
            Button::Select => 48,
            Button::Shift => 49,
            Button::Note => 50,
            Button::Session => 51,
            Button::AddDevice => 52,
            Button::AddTrack => 53,
            Button::OctaveDown => 54,
            Button::OctaveUp => 55,
            Button::Convert => 35,
            Button::TapTempo => 3,
            Button::Metronome => 9,
        }
    }

    /// Get button from CC number
    pub fn from_cc(cc: u8) -> Option<Self> {
        match cc {
            // Transport
            85 => Some(Button::Play),
            86 => Some(Button::Record),
            87 => Some(Button::New),
            88 => Some(Button::Duplicate),
            89 => Some(Button::Automate),
            90 => Some(Button::FixedLength),

            // Navigation
            44 => Some(Button::Left),
            45 => Some(Button::Right),
            46 => Some(Button::Up),
            47 => Some(Button::Down),

            // Track buttons below display
            20 => Some(Button::TrackBelow1),
            21 => Some(Button::TrackBelow2),
            22 => Some(Button::TrackBelow3),
            23 => Some(Button::TrackBelow4),
            24 => Some(Button::TrackBelow5),
            25 => Some(Button::TrackBelow6),
            26 => Some(Button::TrackBelow7),
            27 => Some(Button::TrackBelow8),

            // Track buttons above display
            102 => Some(Button::TrackAbove1),
            103 => Some(Button::TrackAbove2),
            104 => Some(Button::TrackAbove3),
            105 => Some(Button::TrackAbove4),
            106 => Some(Button::TrackAbove5),
            107 => Some(Button::TrackAbove6),
            108 => Some(Button::TrackAbove7),
            109 => Some(Button::TrackAbove8),

            // Mode
            28 => Some(Button::Master),
            29 => Some(Button::StopClip),
            30 => Some(Button::Setup),
            31 => Some(Button::Layout),
            110 => Some(Button::Device),
            111 => Some(Button::Browse),
            112 => Some(Button::Mix),
            113 => Some(Button::Clip),

            // Edit
            116 => Some(Button::Quantize),
            117 => Some(Button::DoubleLoop),
            118 => Some(Button::Delete),
            119 => Some(Button::Undo),

            // Performance
            56 => Some(Button::Repeat),
            57 => Some(Button::Accent),
            58 => Some(Button::Scale),
            59 => Some(Button::User),
            60 => Some(Button::Mute),
            61 => Some(Button::Solo),
            62 => Some(Button::PageLeft),
            63 => Some(Button::PageRight),

            // Utility
            48 => Some(Button::Select),
            49 => Some(Button::Shift),
            50 => Some(Button::Note),
            51 => Some(Button::Session),
            52 => Some(Button::AddDevice),
            53 => Some(Button::AddTrack),
            54 => Some(Button::OctaveDown),
            55 => Some(Button::OctaveUp),
            35 => Some(Button::Convert),
            3 => Some(Button::TapTempo),
            9 => Some(Button::Metronome),

            _ => None,
        }
    }

    /// Check if button has RGB LED (vs monochrome)
    pub fn is_rgb(self) -> bool {
        matches!(
            self,
            Button::TrackAbove1
                | Button::TrackAbove2
                | Button::TrackAbove3
                | Button::TrackAbove4
                | Button::TrackAbove5
                | Button::TrackAbove6
                | Button::TrackAbove7
                | Button::TrackAbove8
                | Button::TrackBelow1
                | Button::TrackBelow2
                | Button::TrackBelow3
                | Button::TrackBelow4
                | Button::TrackBelow5
                | Button::TrackBelow6
                | Button::TrackBelow7
                | Button::TrackBelow8
                | Button::Play
                | Button::Record
                | Button::Automate
                | Button::Mute
                | Button::Solo
        )
    }
}
