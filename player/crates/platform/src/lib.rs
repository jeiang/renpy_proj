//! Window, events and input: renpy.pygame.display/event/key/mouse/time/...
//! Contract: player/CONTRACTS.md.
//!
//! The winit event loop runs on the main thread in pump mode. `pygame.event.get`,
//! `poll`, `pump`, `wait` and `peek` pump it, so Ren'Py keeps its own main loop.

mod display;
mod evloop;
mod event;
mod keys;
mod key;
mod mouse;
mod pad;
mod power;
mod scrap;
mod timemod;
mod util;

use std::ffi::CStr;

pub use evloop::{create_window, drawable_size, window};

/// Every dotted builtin module of this crate with its init function.
pub fn inittab() -> Vec<(&'static CStr, unsafe extern "C" fn() -> *mut pyo3_ffi::PyObject)> {
    vec![
        (c"renpy.pygame.display", display::renpy_pygame_display::__pyo3_init),
        (c"renpy.pygame.event", event::renpy_pygame_event::__pyo3_init),
        (c"renpy.pygame.key", key::renpy_pygame_key::__pyo3_init),
        (c"renpy.pygame.mouse", mouse::renpy_pygame_mouse::__pyo3_init),
        (c"renpy.pygame.pygame_time", timemod::renpy_pygame_time::__pyo3_init),
        (c"renpy.pygame.joystick", pad::joystick::renpy_pygame_joystick::__pyo3_init),
        (c"renpy.pygame.controller", pad::controller::renpy_pygame_controller::__pyo3_init),
        (c"renpy.pygame.power", power::renpy_pygame_power::__pyo3_init),
        (c"renpy.pygame.scrap", scrap::renpy_pygame_scrap::__pyo3_init),
    ]
}
