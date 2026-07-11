use std::time::{Instant, Duration};
use std::collections::VecDeque;

#[derive(Clone)]
pub struct BeatGrid {
    pub beats_per_bar: u32,
    pub bar_start_time: f64,
    pub bar_duration: f64,
}

impl Default for BeatGrid {
    fn default() -> Self {
        Self {
            beats_per_bar: 4,
            bar_start_time: 0.0,
            bar_duration: 1.0/30.0,
        }
    }
}

impl BeatGrid {
}

pub struct BeatTap {
    input_beats: VecDeque<Instant>,
}

impl BeatTap {
    /// Enter a user tap. Will return a new BeatGrid after enough taps have been received.
// When do we have enough taps? Do we know the new grid immediately or do we have to wait a bit until no taps are coming?
    pub fn tap(&mut self) -> Option<BeatGrid> {
        unimplemented!();
    }
}

pub struct BeatPredictor {
    input_beats: VecDeque<Instant>,
    pub multiplier: f32,
}

impl BeatPredictor {
    pub fn new() -> Self {
        Self {
            input_beats: VecDeque::new(),
            multiplier: 1.0,
        }
    }

    pub fn put_input_beat(&mut self) {
        let time = Instant::now();
        self.input_beats.push_back(time);
        if self.input_beats.len() > 2 {
            self.input_beats.pop_front();
        }
    }

    pub fn duration_to_next_beat(&self, offset: Duration) -> Option<Duration> {
        if self.input_beats.len() < 2 {
            return None
        }

        let time = Instant::now() + offset;
        let dur_since_last_input_beat = time - self.input_beats[1];
        let beat_length = (self.input_beats[1] - self.input_beats[0]).mul_f32(self.multiplier);
        let dur_since_last_beat = Duration::from_micros((dur_since_last_input_beat.as_micros() % beat_length.as_micros()) as u64);
        let dur_to_next_beat = beat_length - dur_since_last_beat;
        Some(dur_to_next_beat)
    }

    pub fn duration_since_last_beat(&self, offset: Duration) -> Option<Duration> {
        if self.input_beats.len() < 2 {
            return None
        }

        let time = Instant::now() + offset;
        let dur_since_last_input_beat = time - self.input_beats[1];
        let beat_length = (self.input_beats[1] - self.input_beats[0]).mul_f32(self.multiplier);
        let dur_since_last_beat = Duration::from_micros((dur_since_last_input_beat.as_micros() % beat_length.as_micros()) as u64);
        Some(dur_since_last_beat)
    }

    pub fn duration_since_last_beat_mul(&self, offset: Duration, mul: f32) -> Option<Duration> {
        if self.input_beats.len() < 2 {
            return None
        }

        let time = Instant::now() + offset;
        let dur_since_last_input_beat = time - self.input_beats[1];
        let beat_length = (self.input_beats[1] - self.input_beats[0]).mul_f32(self.multiplier * mul);
        let dur_since_last_beat = Duration::from_micros((dur_since_last_input_beat.as_micros() % beat_length.as_micros()) as u64);
        Some(dur_since_last_beat)
    }
}
