use std::time::{Instant, Duration};
use std::collections::VecDeque;

pub struct BeatPredictor {
    input_beats: VecDeque<Instant>
}

impl BeatPredictor {
    pub fn new() -> Self {
        Self {
            input_beats: VecDeque::new()
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
        let beat_length = self.input_beats[1] - self.input_beats[0];
        let dur_since_last_beat = Duration::from_micros((dur_since_last_input_beat.as_micros() % beat_length.as_micros()) as u64);
        let dur_to_next_beat = beat_length - dur_since_last_beat;
        Some(dur_to_next_beat)
    }
}
