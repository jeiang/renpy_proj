//! `renpy.pygame.mouse`: pointer state, cursor shape and visibility.

use pyo3::prelude::*;
use winit::window::{CursorIcon, CustomCursor};

use crate::{evloop, util};

static ACTIVE_CURSOR: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);

#[pyfunction]
fn init() {
    *ACTIVE_CURSOR.lock() = None;
}

#[pyfunction]
fn quit() {
    *ACTIVE_CURSOR.lock() = None;
}

#[pyfunction]
fn reset() {
    if let Some(w) = evloop::window() {
        w.set_cursor(CursorIcon::Default);
    }
    *ACTIVE_CURSOR.lock() = None;
}

#[pyfunction]
fn get_pressed() -> (u8, u8, u8) {
    let m = evloop::INPUT.lock().mouse_mask;
    (u8::from(m & 1 != 0), u8::from(m & 2 != 0), u8::from(m & 4 != 0))
}

#[pyfunction]
fn get_pos() -> (i32, i32) {
    evloop::INPUT.lock().mouse_pos
}

#[pyfunction]
fn get_rel() -> (i32, i32) {
    std::mem::take(&mut evloop::INPUT.lock().rel_acc)
}

#[pyfunction]
fn set_pos(py: Python<'_>, pos: (i32, i32)) -> PyResult<()> {
    evloop::warp_cursor(pos.0, pos.1).map_err(|e| util::pg_error(py, &format!("cannot move the cursor: {e}")))
}

#[pyfunction]
fn set_visible(visible: bool) {
    if let Some(w) = evloop::window() {
        w.set_cursor_visible(visible);
    }
}

#[pyfunction]
fn get_focused() -> bool {
    evloop::INPUT.lock().mouse_focus
}

/// SDL1 bitmap cursors are not supported. `ColorCursor` covers the Ren'Py use.
#[pyfunction]
fn set_cursor(py: Python<'_>, size: Py<PyAny>, hotspot: Py<PyAny>, xormasks: Py<PyAny>, andmasks: Py<PyAny>) -> PyResult<()> {
    let _ = (size, hotspot, xormasks, andmasks);
    Err(util::pg_error(py, "Bitmap cursors are not supported; use mouse.ColorCursor."))
}

#[pyfunction]
fn get_cursor() -> Option<Py<PyAny>> {
    None
}

/// A cursor built from a surface. `activate` makes it the window cursor.
#[pyclass(module = "renpy.pygame.mouse", unsendable)]
struct ColorCursor {
    cursor: CustomCursor,
    id: usize,
}

#[pymethods]
impl ColorCursor {
    #[new]
    fn new(py: Python<'_>, surface: &Bound<'_, PyAny>, x: u16, y: u16) -> PyResult<Self> {
        static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
        let (w, h, rgba) = util::read_rgba(surface)?;
        let (w16, h16) = (u16::try_from(w), u16::try_from(h));
        let (Ok(w16), Ok(h16)) = (w16, h16) else {
            return Err(util::pg_error(py, "The cursor image is too large."));
        };
        let source = CustomCursor::from_rgba(rgba, w16, h16, x, y)
            .map_err(|e| util::pg_error(py, &format!("bad cursor image: {e}")))?;
        let cursor = evloop::on_loop(move |el| el.create_custom_cursor(source))
            .map_err(|e| util::pg_error(py, &format!("cannot create the cursor: {e}")))?;
        Ok(Self { cursor, id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) })
    }

    fn activate(&self) {
        let mut active = ACTIVE_CURSOR.lock();
        if *active != Some(self.id) {
            *active = Some(self.id);
            if let Some(w) = evloop::window() {
                w.set_cursor(self.cursor.clone());
            }
        }
    }
}

#[pymodule(gil_used = false)]
pub mod renpy_pygame_mouse {
    #[pymodule_export]
    use super::ColorCursor;
    #[pymodule_export]
    use super::{
        get_cursor, get_focused, get_pos, get_pressed, get_rel, init, quit, reset, set_cursor, set_pos,
        set_visible,
    };
}
