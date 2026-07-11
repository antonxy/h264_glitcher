use std::time::{Instant, Duration};
use std::collections::VecDeque;

#[derive(Clone)]
pub struct BeatGrid {
    pub bar_start_time: Instant,
    pub bar_duration: Duration,
}

impl Default for BeatGrid {
    fn default() -> Self {
        Self {
            bar_start_time: Instant::now(),
            bar_duration: Duration::from_secs_f32(1.0/30.0),
        }
    }
}


impl BeatGrid {
    /// Has a beat occurred betwen two time instances?
    ///
    /// `now` is inclusive, `last` is exclusive
    ///
    /// `division_level` defines if we are interested in beats/bars/subbeats
    /// -2 = bar
    /// -1 = half bar
    /// 0 = beat
    /// 1 = half beat
    /// Everything is assuming 4 beats per bar for now
    pub fn has_beat_occurred(&self, now: Instant, last: Instant, division_level: i32) -> bool {
        let division_length = self.bar_duration; //TODO division_level
        // TODO something seems imprecise, the beat is off pretty quickly
        let divisions_start_to_last = (last - self.bar_start_time).as_nanos() / division_length.as_nanos();
        let divisions_start_to_now  = (now  - self.bar_start_time).as_nanos() / division_length.as_nanos();
        return divisions_start_to_now > divisions_start_to_last;
    }
}

pub struct BeatTap {
    taps: VecDeque<Instant>,
}

impl BeatTap {
    pub fn new() -> Self {
        Self {
            taps: VecDeque::new(),
        }
    }

    /// Enter a user tap. Will wait until four taps are received, then returns a new BeatGrid
    pub fn tap(&mut self) -> Option<BeatGrid> {
        self.taps.push_back(Instant::now());

        // TODO Check if any of the intervals between taps is much larger than the last two
        // If so there was a break where the user abandoned tapping a bar
        if self.taps.len() >= 2 {
            let last_interval = self.taps[self.taps.len() - 1] - self.taps[self.taps.len() - 2];
            
        }

        if self.taps.len() >= 4 {
            let res = Some(BeatGrid {
                bar_start_time: self.taps[0],
                bar_duration: Duration::from_secs_f32((self.taps[3] - self.taps[0]).as_secs_f32() * (4.0/3.0))
            });
            self.taps.clear();
            res
        } else {
            None
        }    
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
