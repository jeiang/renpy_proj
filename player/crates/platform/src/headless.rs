//! Headless mode: no winit loop and no OS window. The pygame API runs against a virtual
//! window, and events come from `inject`.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::win;

/// The one virtual display, in logical pixels.
pub const DISPLAY: (i32, i32) = (1920, 1080);

static ON: LazyLock<AtomicBool> =
    LazyLock::new(|| AtomicBool::new(std::env::var("PLAYER_HEADLESS").is_ok_and(|v| v == "1")));

/// Turns headless mode on. Call it before Python starts. `width` and `height` are the
/// size of the virtual window until the game asks for another size.
pub fn set_headless(width: u32, height: u32) {
    ON.store(true, Ordering::SeqCst);
    win::set_default_size(width, height);
}

/// True in headless mode (`set_headless`, or env `PLAYER_HEADLESS=1`).
pub fn is_headless() -> bool {
    ON.load(Ordering::Relaxed)
}
