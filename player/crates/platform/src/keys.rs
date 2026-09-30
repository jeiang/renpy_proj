//! SDL2 scancode, keycode and modifier values for winit keys.

use winit::keyboard::KeyCode;

/// Set on an SDL keycode that is derived from a scancode (`SDLK_SCANCODE_MASK`).
pub const SCANCODE_MASK: u32 = 1 << 30;

pub const KMOD_LSHIFT: u16 = 0x0001;
pub const KMOD_RSHIFT: u16 = 0x0002;
pub const KMOD_LCTRL: u16 = 0x0040;
pub const KMOD_RCTRL: u16 = 0x0080;
pub const KMOD_LALT: u16 = 0x0100;
pub const KMOD_RALT: u16 = 0x0200;
pub const KMOD_LGUI: u16 = 0x0400;
pub const KMOD_RGUI: u16 = 0x0800;
pub const KMOD_CAPS: u16 = 0x2000;

/// One key: winit physical code, SDL scancode, SDL keycode (0 means
/// `scancode | SCANCODE_MASK`) and the SDL key name.
type Row = (KeyCode, u32, u32, &'static str);

const fn c(ch: char) -> u32 {
    ch as u32
}

#[rustfmt::skip]
const TABLE: &[Row] = &[
    (KeyCode::KeyA, 4, c('a'), "A"), (KeyCode::KeyB, 5, c('b'), "B"), (KeyCode::KeyC, 6, c('c'), "C"),
    (KeyCode::KeyD, 7, c('d'), "D"), (KeyCode::KeyE, 8, c('e'), "E"), (KeyCode::KeyF, 9, c('f'), "F"),
    (KeyCode::KeyG, 10, c('g'), "G"), (KeyCode::KeyH, 11, c('h'), "H"), (KeyCode::KeyI, 12, c('i'), "I"),
    (KeyCode::KeyJ, 13, c('j'), "J"), (KeyCode::KeyK, 14, c('k'), "K"), (KeyCode::KeyL, 15, c('l'), "L"),
    (KeyCode::KeyM, 16, c('m'), "M"), (KeyCode::KeyN, 17, c('n'), "N"), (KeyCode::KeyO, 18, c('o'), "O"),
    (KeyCode::KeyP, 19, c('p'), "P"), (KeyCode::KeyQ, 20, c('q'), "Q"), (KeyCode::KeyR, 21, c('r'), "R"),
    (KeyCode::KeyS, 22, c('s'), "S"), (KeyCode::KeyT, 23, c('t'), "T"), (KeyCode::KeyU, 24, c('u'), "U"),
    (KeyCode::KeyV, 25, c('v'), "V"), (KeyCode::KeyW, 26, c('w'), "W"), (KeyCode::KeyX, 27, c('x'), "X"),
    (KeyCode::KeyY, 28, c('y'), "Y"), (KeyCode::KeyZ, 29, c('z'), "Z"),
    (KeyCode::Digit1, 30, c('1'), "1"), (KeyCode::Digit2, 31, c('2'), "2"), (KeyCode::Digit3, 32, c('3'), "3"),
    (KeyCode::Digit4, 33, c('4'), "4"), (KeyCode::Digit5, 34, c('5'), "5"), (KeyCode::Digit6, 35, c('6'), "6"),
    (KeyCode::Digit7, 36, c('7'), "7"), (KeyCode::Digit8, 37, c('8'), "8"), (KeyCode::Digit9, 38, c('9'), "9"),
    (KeyCode::Digit0, 39, c('0'), "0"),
    (KeyCode::Enter, 40, 13, "Return"), (KeyCode::Escape, 41, 27, "Escape"),
    (KeyCode::Backspace, 42, 8, "Backspace"), (KeyCode::Tab, 43, 9, "Tab"), (KeyCode::Space, 44, 32, "Space"),
    (KeyCode::Minus, 45, c('-'), "-"), (KeyCode::Equal, 46, c('='), "="),
    (KeyCode::BracketLeft, 47, c('['), "["), (KeyCode::BracketRight, 48, c(']'), "]"),
    (KeyCode::Backslash, 49, c('\\'), "\\"), (KeyCode::Semicolon, 51, c(';'), ";"),
    (KeyCode::Quote, 52, c('\''), "'"), (KeyCode::Backquote, 53, c('`'), "`"),
    (KeyCode::Comma, 54, c(','), ","), (KeyCode::Period, 55, c('.'), "."), (KeyCode::Slash, 56, c('/'), "/"),
    (KeyCode::CapsLock, 57, 0, "CapsLock"),
    (KeyCode::F1, 58, 0, "F1"), (KeyCode::F2, 59, 0, "F2"), (KeyCode::F3, 60, 0, "F3"),
    (KeyCode::F4, 61, 0, "F4"), (KeyCode::F5, 62, 0, "F5"), (KeyCode::F6, 63, 0, "F6"),
    (KeyCode::F7, 64, 0, "F7"), (KeyCode::F8, 65, 0, "F8"), (KeyCode::F9, 66, 0, "F9"),
    (KeyCode::F10, 67, 0, "F10"), (KeyCode::F11, 68, 0, "F11"), (KeyCode::F12, 69, 0, "F12"),
    (KeyCode::PrintScreen, 70, 0, "PrintScreen"), (KeyCode::ScrollLock, 71, 0, "ScrollLock"),
    (KeyCode::Pause, 72, 0, "Pause"), (KeyCode::Insert, 73, 0, "Insert"), (KeyCode::Home, 74, 0, "Home"),
    (KeyCode::PageUp, 75, 0, "PageUp"), (KeyCode::Delete, 76, 127, "Delete"), (KeyCode::End, 77, 0, "End"),
    (KeyCode::PageDown, 78, 0, "PageDown"), (KeyCode::ArrowRight, 79, 0, "Right"),
    (KeyCode::ArrowLeft, 80, 0, "Left"), (KeyCode::ArrowDown, 81, 0, "Down"), (KeyCode::ArrowUp, 82, 0, "Up"),
    (KeyCode::NumLock, 83, 0, "Numlock"), (KeyCode::NumpadDivide, 84, 0, "Keypad /"),
    (KeyCode::NumpadMultiply, 85, 0, "Keypad *"), (KeyCode::NumpadSubtract, 86, 0, "Keypad -"),
    (KeyCode::NumpadAdd, 87, 0, "Keypad +"), (KeyCode::NumpadEnter, 88, 0, "Keypad Enter"),
    (KeyCode::Numpad1, 89, 0, "Keypad 1"), (KeyCode::Numpad2, 90, 0, "Keypad 2"),
    (KeyCode::Numpad3, 91, 0, "Keypad 3"), (KeyCode::Numpad4, 92, 0, "Keypad 4"),
    (KeyCode::Numpad5, 93, 0, "Keypad 5"), (KeyCode::Numpad6, 94, 0, "Keypad 6"),
    (KeyCode::Numpad7, 95, 0, "Keypad 7"), (KeyCode::Numpad8, 96, 0, "Keypad 8"),
    (KeyCode::Numpad9, 97, 0, "Keypad 9"), (KeyCode::Numpad0, 98, 0, "Keypad 0"),
    (KeyCode::NumpadDecimal, 99, 0, "Keypad ."), (KeyCode::IntlBackslash, 100, 0, "NonUSBackslash"),
    (KeyCode::ContextMenu, 101, 0, "Application"), (KeyCode::NumpadEqual, 103, 0, "Keypad ="),
    (KeyCode::F13, 104, 0, "F13"), (KeyCode::F14, 105, 0, "F14"), (KeyCode::F15, 106, 0, "F15"),
    (KeyCode::F16, 107, 0, "F16"), (KeyCode::F17, 108, 0, "F17"), (KeyCode::F18, 109, 0, "F18"),
    (KeyCode::F19, 110, 0, "F19"), (KeyCode::F20, 111, 0, "F20"), (KeyCode::F21, 112, 0, "F21"),
    (KeyCode::F22, 113, 0, "F22"), (KeyCode::F23, 114, 0, "F23"), (KeyCode::F24, 115, 0, "F24"),
    (KeyCode::Help, 117, 0, "Help"),
    (KeyCode::NumpadComma, 133, 0, "Keypad ,"),
    (KeyCode::IntlRo, 135, 0, "International1"), (KeyCode::IntlYen, 137, 0, "International3"),
    (KeyCode::ControlLeft, 224, 0, "Left Ctrl"), (KeyCode::ShiftLeft, 225, 0, "Left Shift"),
    (KeyCode::AltLeft, 226, 0, "Left Alt"), (KeyCode::SuperLeft, 227, 0, "Left GUI"),
    (KeyCode::ControlRight, 228, 0, "Right Ctrl"), (KeyCode::ShiftRight, 229, 0, "Right Shift"),
    (KeyCode::AltRight, 230, 0, "Right Alt"), (KeyCode::SuperRight, 231, 0, "Right GUI"),
];

/// SDL scancode of a winit physical key code (0 when SDL has no equivalent).
pub fn scancode(code: KeyCode) -> u32 {
    TABLE.iter().find(|r| r.0 == code).map_or(0, |r| r.1)
}

fn keycode_of(row: &Row) -> u32 {
    if row.2 != 0 { row.2 } else { row.1 | SCANCODE_MASK }
}

/// SDL keycode of a physical key on a US layout (used when the layout gives no character).
pub fn keycode_from_physical(code: KeyCode) -> u32 {
    TABLE.iter().find(|r| r.0 == code).map_or(0, keycode_of)
}

/// SDL keycode for a character key: the unshifted character, as SDL reports it.
pub fn keycode_from_char(ch: char) -> u32 {
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l as u32,
        _ => ch as u32,
    }
}

/// `SDL_GetScancodeFromKey`: the scancode that produces `keycode` on the layouts we know.
pub fn scancode_from_keycode(keycode: u32) -> u32 {
    if keycode & SCANCODE_MASK != 0 {
        return keycode & !SCANCODE_MASK;
    }
    TABLE.iter().find(|r| keycode_of(r) == keycode).map_or(0, |r| r.1)
}

/// `SDL_GetKeyName`.
pub fn key_name(keycode: u32) -> String {
    if let Some(r) = TABLE.iter().find(|r| keycode_of(r) == keycode) {
        return r.3.to_string();
    }
    match char::from_u32(keycode) {
        Some(ch) if keycode > 32 => ch.to_uppercase().collect(),
        _ => String::new(),
    }
}
