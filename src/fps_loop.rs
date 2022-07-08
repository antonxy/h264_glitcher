use std::time::{Instant, Duration};
use std::sync::{Condvar, Mutex, Arc};

pub struct LoopTimer {
    loop_begin_time: Instant,
    state: Arc<(Mutex<SharedState>, Condvar)>,
}

#[derive(Clone)]
pub struct LoopController {
    state: Arc<(Mutex<SharedState>, Condvar)>,
}

struct SharedState {
    fps: f32,
    wake_up: bool,
}

impl LoopTimer {
    pub fn new() -> (Self, LoopController) {
        let fps_loop = Self {
            loop_begin_time: Instant::now(),
            state: Arc::new((Mutex::new(SharedState {
                fps: 30.0,
                wake_up: false,
            }), Condvar::new())),
        };
        let controller = LoopController {
            state: fps_loop.state.clone(),
        };
        (fps_loop, controller)
    }

    pub fn begin_loop(&mut self) {
        self.loop_begin_time = Instant::now();
    }

    pub fn end_loop(&self) {
        let (mutex, cvar) = &*self.state;
        let loop_time_left = |state: &SharedState| {
            let loop_end_time = Instant::now();
            let loop_time_taken = loop_end_time - self.loop_begin_time;
            let total_loop_time = Duration::from_secs_f32(1.0 / state.fps);
            total_loop_time.checked_sub(loop_time_taken)
        };

        //TODO maybe use spin wait for very short periods
        //Just a bit tricky to do it in an interruptible way
        //Locking mutex in each loop iteration might be a bit much,
        //maybe have to use atomics
        //Or just ignore interrupting for very short times

        let mut guard = mutex.lock().unwrap();
        loop {
            let timeout = match loop_time_left(&mut *guard) {
                Some(timeout) => timeout,
                None => break,
            };
            guard = cvar.wait_timeout(guard, timeout).unwrap().0;
            
        }
        guard.wake_up = false;
    }

}

impl LoopController {
    pub fn fps(&self) -> f32 {
        let (mutex, _) = &*self.state;
        mutex.lock().unwrap().fps
    }

    pub fn set_fps(&mut self, fps: f32) {
        let (mutex, cvar) = &*self.state;
        mutex.lock().unwrap().fps = fps;
        cvar.notify_all();
    }

    pub fn wake_up_now(&mut self) {
        let (mutex, cvar) = &*self.state;
        mutex.lock().unwrap().wake_up = true;
        cvar.notify_all();
    }
}