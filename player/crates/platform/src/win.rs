//! One handle for "the window": the native winit window, or the virtual window of
//! headless mode. `display`, `key` and `event` use this instead of winit types.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Condvar, Mutex};
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::Window;

use crate::{evloop, headless};

const WINDOWPOS_UNDEFINED: i64 = 0x1FFF_0000;
const WINDOWPOS_CENTERED: i64 = 0x2FFF_0000;

/// The state of the virtual window.
struct VState {
    exists: bool,
    size: (u32, u32),
    pos: (i64, i64),
    resizable: bool,
    decorated: bool,
    visible: bool,
    fullscreen: bool,
    maximized: bool,
    minimized: bool,
    focus: bool,
}

static VWIN: Mutex<VState> = Mutex::new(VState {
    exists: false,
    size: (1280, 720),
    pos: (0, 0),
    resizable: false,
    decorated: true,
    visible: true,
    fullscreen: false,
    maximized: false,
    minimized: false,
    focus: true,
});

/// Wake flag of the headless `pump`.
static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

pub fn set_default_size(w: u32, h: u32) {
    let mut v = VWIN.lock();
    if !v.exists {
        v.size = (w.max(1), h.max(1));
    }
}

/// Logical size of the virtual window (also before it exists: the default size).
pub fn virtual_size() -> (u32, u32) {
    VWIN.lock().size
}

/// Headless `pump`: blocks up to `timeout` or until `wake`. A zero timeout returns at once
/// and keeps the wake flag.
pub fn block(timeout: Duration) {
    if timeout.is_zero() {
        return;
    }
    let mut flag = WAKE.0.lock();
    if !*flag {
        WAKE.1.wait_for(&mut flag, timeout);
    }
    *flag = false;
}

pub fn wake() {
    *WAKE.0.lock() = true;
    WAKE.1.notify_all();
}

/// The one window.
pub enum Win {
    Native(Arc<Window>),
    Virtual,
}

impl Win {
    /// The window if it exists.
    pub fn get() -> Option<Win> {
        if headless::is_headless() {
            VWIN.lock().exists.then_some(Win::Virtual)
        } else {
            evloop::window().map(Win::Native)
        }
    }

    /// Creates the window, or resizes and returns the existing one.
    pub fn create(w: u32, h: u32, title: &str, resizable: bool) -> anyhow::Result<Win> {
        if !headless::is_headless() {
            return evloop::create_window(w, h, title, resizable).map(Win::Native);
        }
        let mut v = VWIN.lock();
        v.exists = true;
        v.resizable = resizable;
        if !v.fullscreen && !v.maximized {
            v.size = (w.max(1), h.max(1));
        }
        Ok(Win::Virtual)
    }

    pub fn logical_size(&self) -> (i64, i64) {
        match self {
            Win::Native(w) => {
                let l = w.inner_size().to_logical::<f64>(w.scale_factor());
                (l.width.round() as i64, l.height.round() as i64)
            }
            Win::Virtual => {
                let s = VWIN.lock().size;
                (i64::from(s.0), i64::from(s.1))
            }
        }
    }

    pub fn drawable_size(&self) -> (u32, u32) {
        match self {
            Win::Native(w) => {
                let s = w.inner_size();
                (s.width, s.height)
            }
            Win::Virtual => VWIN.lock().size,
        }
    }

    pub fn set_title(&self, t: &str) {
        if let Win::Native(w) = self {
            w.set_title(t);
        }
    }

    pub fn set_decorations(&self, on: bool) {
        match self {
            Win::Native(w) => w.set_decorations(on),
            Win::Virtual => VWIN.lock().decorated = on,
        }
    }

    pub fn set_visible(&self, on: bool) {
        match self {
            Win::Native(w) => w.set_visible(on),
            Win::Virtual => VWIN.lock().visible = on,
        }
    }

    pub fn set_fullscreen(&self, on: bool) {
        match self {
            Win::Native(w) => evloop::set_fullscreen(w, on),
            Win::Virtual => VWIN.lock().fullscreen = on,
        }
    }

    pub fn set_maximized(&self, on: bool) {
        match self {
            Win::Native(w) => w.set_maximized(on),
            Win::Virtual => VWIN.lock().maximized = on,
        }
    }

    pub fn set_minimized(&self, on: bool) {
        match self {
            Win::Native(w) => w.set_minimized(on),
            Win::Virtual => VWIN.lock().minimized = on,
        }
    }

    pub fn request_logical_size(&self, w: i64, h: i64) {
        match self {
            Win::Native(win) => {
                let _ = win.request_inner_size(LogicalSize::new(w as f64, h as f64));
            }
            Win::Virtual => VWIN.lock().size = (w.max(1) as u32, h.max(1) as u32),
        }
    }

    pub fn is_fullscreen(&self) -> bool {
        match self {
            Win::Native(w) => w.fullscreen().is_some(),
            Win::Virtual => VWIN.lock().fullscreen,
        }
    }

    pub fn is_maximized(&self) -> bool {
        match self {
            Win::Native(w) => w.is_maximized(),
            Win::Virtual => VWIN.lock().maximized,
        }
    }

    pub fn is_visible(&self) -> bool {
        match self {
            Win::Native(w) => w.is_visible() != Some(false),
            Win::Virtual => VWIN.lock().visible,
        }
    }

    pub fn is_resizable(&self) -> bool {
        match self {
            Win::Native(w) => w.is_resizable(),
            Win::Virtual => VWIN.lock().resizable,
        }
    }

    pub fn is_decorated(&self) -> bool {
        match self {
            Win::Native(w) => w.is_decorated(),
            Win::Virtual => VWIN.lock().decorated,
        }
    }

    pub fn is_minimized(&self) -> bool {
        match self {
            Win::Native(w) => w.is_minimized() == Some(true),
            Win::Virtual => VWIN.lock().minimized,
        }
    }

    pub fn has_focus(&self) -> bool {
        match self {
            Win::Native(w) => w.has_focus(),
            Win::Virtual => VWIN.lock().focus,
        }
    }

    pub fn apply_pos(&self, pos: (i64, i64)) {
        match self {
            Win::Native(w) => crate::display::apply_native_pos(w, pos),
            Win::Virtual => {
                let kind = |v: i64| v & 0xFFFF_0000;
                let mut v = VWIN.lock();
                if kind(pos.0) == WINDOWPOS_UNDEFINED && kind(pos.1) == WINDOWPOS_UNDEFINED {
                    return;
                }
                let (mw, mh) = headless::DISPLAY;
                let centered = |p: i64, outer: i64, screen: i64| {
                    if kind(p) == WINDOWPOS_CENTERED {
                        (screen - outer) / 2
                    } else if kind(p) == WINDOWPOS_UNDEFINED {
                        0
                    } else {
                        p
                    }
                };
                v.pos = (
                    centered(pos.0, i64::from(v.size.0), i64::from(mw)),
                    centered(pos.1, i64::from(v.size.1), i64::from(mh)),
                );
            }
        }
    }

    pub fn position(&self) -> Result<(i64, i64), String> {
        match self {
            Win::Native(w) => {
                let p = w.outer_position().map_err(|e| e.to_string())?;
                let l: LogicalPosition<f64> = p.to_logical(w.scale_factor());
                Ok((l.x.round() as i64, l.y.round() as i64))
            }
            Win::Virtual => Ok(VWIN.lock().pos),
        }
    }

    /// Drops the window (the native one is hidden; the virtual one stops existing).
    pub fn destroy() {
        if headless::is_headless() {
            VWIN.lock().exists = false;
        } else {
            evloop::destroy_window();
        }
    }
}
