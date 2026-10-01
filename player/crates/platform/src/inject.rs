//! Event injection for headless mode: the stream server calls these from any thread.
//! They build the same SDL-numbered events as the winit path (`evloop`).

use pyo3::prelude::*;
use winit::keyboard::KeyCode;

use crate::event::{self, consts};
use crate::evloop::{self, INPUT, KeyIn};
use crate::{keys, win};

fn run(f: impl FnOnce(Python<'_>) -> PyResult<()>) {
    Python::attach(|py| {
        if let Err(e) = f(py) {
            e.print(py);
        }
    });
    evloop::wake();
}

fn modifier_bit(code: KeyCode) -> u16 {
    match code {
        KeyCode::ShiftLeft => keys::KMOD_LSHIFT,
        KeyCode::ShiftRight => keys::KMOD_RSHIFT,
        KeyCode::ControlLeft => keys::KMOD_LCTRL,
        KeyCode::ControlRight => keys::KMOD_RCTRL,
        KeyCode::AltLeft => keys::KMOD_LALT,
        KeyCode::AltRight => keys::KMOD_RALT,
        KeyCode::SuperLeft => keys::KMOD_LGUI,
        KeyCode::SuperRight => keys::KMOD_RGUI,
        _ => 0,
    }
}

/// A key press or release. `dom_code` is `KeyboardEvent.code`, `dom_key` is `KeyboardEvent.key`.
/// Unknown codes are dropped.
pub fn key(down: bool, dom_code: &str, dom_key: &str, repeat: bool) {
    let Some(code) = keys::code_from_dom(dom_code) else {
        return;
    };
    {
        let mut inp = INPUT.lock();
        let bit = modifier_bit(code);
        if down {
            inp.mods |= bit;
        } else {
            inp.mods &= !bit;
        }
        if code == KeyCode::CapsLock && down && !repeat {
            inp.mods ^= keys::KMOD_CAPS;
        }
    }
    let mut chars = dom_key.chars();
    let single = match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    };
    let k = KeyIn {
        pressed: down,
        physical: Some(code),
        base_char: single,
        text: single.map(String::from),
        repeat,
    };
    run(|py| evloop::key_core(py, &k));
}

/// The pointer moved. Logical pixels of the game window.
pub fn mouse_move(x: f64, y: f64) {
    run(|py| evloop::cursor_moved(py, x, y));
}

/// A mouse button. `dom_button`: 0 left, 1 middle, 2 right, 3 back, 4 forward.
pub fn mouse_button(down: bool, dom_button: u8) {
    let n = match dom_button {
        0 => 1,
        1 => 2,
        2 => 3,
        3 => 4,
        4 => 5,
        other => i64::from(other) + 1,
    };
    run(|py| evloop::mouse_button(py, n, down));
}

/// A wheel movement in lines. Positive `dy` scrolls up (SDL and winit convention).
pub fn wheel(dx: f64, dy: f64) {
    run(|py| evloop::wheel_lines(py, dx, dy));
}

/// A `TEXTINPUT` commit.
pub fn text(s: &str) {
    let s = s.to_string();
    run(|py| event::push_native(py, consts::TEXTINPUT, |d| d.set_item("text", s)));
}

/// The window gained or lost focus (`ACTIVEEVENT`).
pub fn focus(gain: bool) {
    run(|py| evloop::focus_changed(py, gain));
}

/// The logical size of the game window.
pub fn size() -> (u32, u32) {
    win::virtual_size()
}
