//! `renpy.pygame.pygame_time`: millisecond clock, delays, timers, `Clock`.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use pyo3::prelude::*;

use crate::{event, util};

static START: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Monotonic milliseconds since the module was initialised.
pub fn ticks() -> i64 {
    START.elapsed().as_millis() as i64
}

#[pyfunction]
fn get_ticks() -> i64 {
    ticks()
}

#[pyfunction]
fn wait(py: Python<'_>, milliseconds: i64) -> i64 {
    let start = ticks();
    let d = Duration::from_millis(milliseconds.max(0) as u64);
    py.detach(|| std::thread::sleep(d));
    ticks() - start
}

#[pyfunction]
fn delay(py: Python<'_>, milliseconds: i64) -> i64 {
    wait(py, milliseconds)
}

#[pyfunction]
#[pyo3(signature = (eventid, milliseconds, once=false))]
fn set_timer(eventid: i64, milliseconds: i64, once: bool) {
    event::set_timer(eventid, milliseconds, once);
}

#[pyfunction]
fn init() {}

#[pyfunction]
fn quit() {}

/// `pygame.time.Clock`.
#[pyclass(module = "renpy.pygame.pygame_time")]
struct Clock {
    last: i64,
    last_frames: Vec<i64>,
    frametime: i64,
    raw_frametime: i64,
}

#[pymethods]
impl Clock {
    #[new]
    fn new() -> Self {
        Self { last: ticks(), last_frames: Vec::new(), frametime: 0, raw_frametime: 0 }
    }

    #[pyo3(signature = (framerate=0.0))]
    fn tick(&mut self, py: Python<'_>, framerate: f64) -> i64 {
        let now = ticks();
        self.raw_frametime = now - self.last;
        while self.last_frames.len() > 9 {
            self.last_frames.remove(0);
        }
        if framerate == 0.0 {
            self.last = now;
            self.last_frames.push(self.raw_frametime);
            return self.raw_frametime;
        }
        let duration = (1.0 / framerate * 1000.0) as i64;
        if self.raw_frametime < duration {
            wait(py, duration - self.raw_frametime);
        }
        let now = ticks();
        self.frametime = now - self.last;
        self.last = now;
        self.last_frames.push(self.frametime);
        self.frametime
    }

    #[pyo3(signature = (framerate=0.0))]
    fn tick_busy_loop(&mut self, py: Python<'_>, framerate: f64) -> i64 {
        self.tick(py, framerate)
    }

    fn get_time(&self) -> i64 {
        self.frametime
    }

    fn get_rawtime(&self) -> i64 {
        self.raw_frametime
    }

    fn get_fps(&self) -> f64 {
        if self.last_frames.is_empty() {
            return 0.0;
        }
        let total: i64 = self.last_frames.iter().sum();
        let avg = total as f64 / 1000.0 / self.last_frames.len() as f64;
        if avg == 0.0 { 0.0 } else { 1.0 / avg }
    }
}

#[pymodule(gil_used = false)]
pub mod renpy_pygame_time {
    use super::*;

    #[pymodule_init]
    fn module_init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        LazyLock::force(&START);
        util::register_hook(m, "register_init", "init")?;
        util::register_hook(m, "register_quit", "quit")
    }

    #[pymodule_export]
    use super::{Clock, delay, get_ticks, init, quit, set_timer, wait};
}
