//! `renpy.pygame.key`: modifier state, pressed keys, text input.

use std::collections::HashSet;

use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use winit::dpi::{LogicalPosition, LogicalSize};

use crate::{evloop, keys};

const NUM_SCANCODES: u32 = 512;

/// Snapshot of the pressed keys, indexed by SDL keycode.
#[pyclass(module = "renpy.pygame.key", weakref)]
struct KeyboardState {
    pressed: HashSet<u32>,
}

#[pymethods]
impl KeyboardState {
    fn __getitem__(&self, key: i64) -> PyResult<u8> {
        // SDLK_DELETE < key < SDLK_CAPSLOCK has no scancode.
        if 127 < key && key < 0x4000_0039 {
            return Err(PyIndexError::new_err("Out of range."));
        }
        let sc = u32::try_from(key).map_or(0, keys::scancode_from_keycode);
        if sc > NUM_SCANCODES {
            return Err(PyIndexError::new_err("Out of range."));
        }
        Ok(u8::from(self.pressed.contains(&sc)))
    }
}

#[pyfunction]
fn get_focused() -> bool {
    evloop::window().is_some_and(|w| w.has_focus())
}

#[pyfunction]
fn get_pressed() -> KeyboardState {
    KeyboardState {
        pressed: evloop::INPUT.lock().pressed.clone(),
    }
}

#[pyfunction]
fn get_mods() -> u16 {
    evloop::INPUT.lock().mods
}

#[pyfunction]
fn set_mods(state: u16) {
    evloop::INPUT.lock().mods = state;
}

#[pyfunction]
#[pyo3(signature = (delay=0, interval=0))]
fn set_repeat(delay: i64, interval: i64) {
    let _ = (delay, interval);
}

#[pyfunction]
fn get_repeat() -> (i64, i64) {
    (0, 0)
}

#[pyfunction]
fn name(py: Python<'_>, key: i64) -> Bound<'_, pyo3::types::PyBytes> {
    let n = u32::try_from(key).map(keys::key_name).unwrap_or_default();
    pyo3::types::PyBytes::new(py, n.as_bytes())
}

#[pyfunction]
fn start_text_input() {
    evloop::INPUT.lock().text_input = true;
    if let Some(w) = evloop::window() {
        w.set_ime_allowed(true);
    }
}

#[pyfunction]
fn stop_text_input() {
    evloop::INPUT.lock().text_input = false;
    if let Some(w) = evloop::window() {
        w.set_ime_allowed(false);
    }
}

fn rect_of(r: &Bound<'_, PyAny>) -> PyResult<(f64, f64, f64, f64)> {
    if let Ok(t) = r.extract::<(f64, f64, f64, f64)>() {
        return Ok(t);
    }
    Ok((
        r.getattr("x")?.extract()?,
        r.getattr("y")?.extract()?,
        r.getattr("w")?.extract()?,
        r.getattr("h")?.extract()?,
    ))
}

/// Places the IME candidate window. `None` leaves the last area in place.
#[pyfunction]
fn set_text_input_rect(rect: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
    let (Some(rect), Some(w)) = (rect.filter(|r| !r.is_none()), evloop::window()) else {
        return Ok(());
    };
    let (x, y, rw, rh) = rect_of(rect)?;
    w.set_ime_cursor_area(LogicalPosition::new(x, y), LogicalSize::new(rw, rh));
    Ok(())
}

#[pyfunction]
fn has_screen_keyboard_support() -> bool {
    false
}

#[pyfunction]
#[pyo3(signature = (window=None))]
fn is_screen_keyboard_shown(window: Option<Py<PyAny>>) -> bool {
    let _ = window;
    false
}

#[pymodule(gil_used = false)]
pub mod renpy_pygame_key {
    #[pymodule_export]
    use super::KeyboardState;
    #[pymodule_export]
    use super::{
        get_focused, get_mods, get_pressed, get_repeat, has_screen_keyboard_support,
        is_screen_keyboard_shown, name, set_mods, set_repeat, set_text_input_rect,
        start_text_input, stop_text_input,
    };
}
