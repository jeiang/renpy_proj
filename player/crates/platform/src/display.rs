//! `renpy.pygame.display`: the one window, display queries and hints.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::{Icon, Window as WinitWindow};

use crate::{evloop, util};

const WINDOW_FULLSCREEN: i64 = 0x1;
const WINDOW_OPENGL: i64 = 0x2;
const WINDOW_SHOWN: i64 = 0x4;
const WINDOW_BORDERLESS: i64 = 0x10;
const WINDOW_RESIZABLE: i64 = 0x20;
const WINDOW_MINIMIZED: i64 = 0x40;
const WINDOW_MAXIMIZED: i64 = 0x80;
const WINDOW_INPUT_FOCUS: i64 = 0x200;
const WINDOW_MOUSE_FOCUS: i64 = 0x400;
const WINDOW_FULLSCREEN_DESKTOP: i64 = 0x1001;
const SRCALPHA: u32 = 0x8000_0000;
const WINDOWPOS_UNDEFINED: i64 = 0x1FFF_0000;
const WINDOWPOS_CENTERED: i64 = 0x2FFF_0000;

const DEFAULT_TITLE: &str = "pygame window";

static MAIN_WINDOW: Mutex<Option<Py<PyWindow>>> = Mutex::new(None);
static INIT_DONE: AtomicBool = AtomicBool::new(false);
static SCREENSAVER: AtomicBool = AtomicBool::new(true);
static HINTS: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
static GL_ATTRIBUTES: std::sync::LazyLock<Mutex<HashMap<i64, i64>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn title_or_default() -> String {
    let t = evloop::INPUT.lock().title.clone();
    if t.is_empty() {
        DEFAULT_TITLE.to_string()
    } else {
        t
    }
}

fn os_err(py: Python<'_>, what: &str, e: impl std::fmt::Display) -> PyErr {
    util::pg_error(py, &format!("{what}: {e}"))
}

fn need_window(py: Python<'_>) -> PyResult<std::sync::Arc<WinitWindow>> {
    evloop::window()
        .ok_or_else(|| util::pg_error(py, "No window exists. Call display.set_mode first."))
}

fn logical_size(w: &WinitWindow) -> (i64, i64) {
    let l = w.inner_size().to_logical::<f64>(w.scale_factor());
    (l.width.round() as i64, l.height.round() as i64)
}

fn apply_pos(w: &WinitWindow, pos: (i64, i64)) {
    let kind = |v: i64| v & 0xFFFF_0000;
    if kind(pos.0) == WINDOWPOS_UNDEFINED && kind(pos.1) == WINDOWPOS_UNDEFINED {
        return;
    }
    if kind(pos.0) == WINDOWPOS_CENTERED || kind(pos.1) == WINDOWPOS_CENTERED {
        if let Some(m) = w.current_monitor().or_else(|| w.primary_monitor()) {
            let s = m.scale_factor();
            let mp = m.position().to_logical::<f64>(s);
            let ms = m.size().to_logical::<f64>(s);
            let os = w.outer_size().to_logical::<f64>(w.scale_factor());
            w.set_outer_position(LogicalPosition::new(
                mp.x + (ms.width - os.width) / 2.0,
                mp.y + (ms.height - os.height) / 2.0,
            ));
        }
        return;
    }
    w.set_outer_position(LogicalPosition::new(pos.0 as f64, pos.1 as f64));
}

fn monitor_size(py: Python<'_>) -> PyResult<(i64, i64)> {
    let m = evloop::monitors().map_err(|e| os_err(py, "cannot query monitors", e))?;
    let m = m
        .first()
        .ok_or_else(|| util::pg_error(py, "no video display found"))?;
    Ok((i64::from(m.w), i64::from(m.h)))
}

/// A display window handle. All instances control the one winit window.
#[pyclass(name = "Window", module = "renpy.pygame.display")]
pub struct PyWindow {
    create_flags: i64,
    surface: Mutex<Option<Py<PyAny>>>,
}

fn open(
    py: Python<'_>,
    title: &str,
    resolution: (i64, i64),
    flags: i64,
    pos: (i64, i64),
) -> PyResult<PyWindow> {
    let (mut w, mut h) = resolution;
    if w <= 0 || h <= 0 {
        (w, h) = monitor_size(py)?;
    }
    let win = evloop::create_window(w as u32, h as u32, title, flags & WINDOW_RESIZABLE != 0)
        .map_err(|e| os_err(py, "cannot create the window", e))?;
    win.set_decorations(flags & WINDOW_BORDERLESS == 0);
    win.set_visible(true);
    if flags & WINDOW_FULLSCREEN != 0 {
        evloop::set_fullscreen(&win, true);
    } else if flags & WINDOW_MAXIMIZED != 0 {
        win.set_maximized(true);
    } else {
        apply_pos(&win, pos);
    }
    evloop::pump(Some(Duration::ZERO));
    Ok(PyWindow {
        create_flags: flags,
        surface: Mutex::new(None),
    })
}

#[pymethods]
impl PyWindow {
    #[new]
    #[pyo3(signature = (title, resolution=(0, 0), flags=0, depth=0, pos=(WINDOWPOS_UNDEFINED, WINDOWPOS_UNDEFINED), shape=None))]
    fn new(
        py: Python<'_>,
        title: &Bound<'_, PyAny>,
        resolution: (i64, i64),
        flags: i64,
        depth: i64,
        pos: (i64, i64),
        shape: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let _ = depth;
        if shape.is_some() {
            return Err(util::pg_error(py, "Shaped windows are not supported."));
        }
        open(py, &text_arg(title)?, resolution, flags, pos)
    }

    /// The window surface. It has the window size but is never presented: drawing
    /// goes through the wgpu renderer.
    fn get_surface(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let mut slot = self.surface.lock();
        if let Some(s) = slot.as_ref() {
            return Ok(s.clone_ref(py));
        }
        let win = need_window(py)?;
        let (w, h) = logical_size(&win);
        let cls = py.import("renpy.pygame.surface")?.getattr("Surface")?;
        let s = cls.call1(((w, h), SRCALPHA, 32))?.unbind();
        *slot = Some(s.clone_ref(py));
        Ok(s)
    }

    fn create_surface(&self, py: Python<'_>) -> PyResult<()> {
        *self.surface.lock() = None;
        self.get_surface(py).map(|_| ())
    }

    fn destroy(&self) {
        *self.surface.lock() = None;
        evloop::destroy_window();
    }

    #[pyo3(signature = (size, opengl=false, fullscreen=None, maximized=None))]
    fn resize(
        &self,
        py: Python<'_>,
        size: (i64, i64),
        opengl: bool,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
    ) -> PyResult<()> {
        let _ = opengl;
        let win = need_window(py)?;
        let fullscreen = fullscreen.unwrap_or_else(|| win.fullscreen().is_some());
        let maximized = !fullscreen && maximized.unwrap_or_else(|| win.is_maximized());
        if fullscreen {
            evloop::set_fullscreen(&win, true);
        } else {
            evloop::set_fullscreen(&win, false);
            if maximized {
                win.set_maximized(true);
            } else {
                win.set_maximized(false);
                let (mut w, mut h) = size;
                if w <= 0 || h <= 0 {
                    (w, h) = monitor_size(py)?;
                }
                if logical_size(&win) != (w, h) {
                    let _ = win.request_inner_size(LogicalSize::new(w as f64, h as f64));
                }
            }
        }
        evloop::pump(Some(Duration::ZERO));
        *self.surface.lock() = None;
        self.get_surface(py).map(|_| ())
    }

    #[pyo3(signature = (always=false))]
    fn recreate_gl_context(&self, always: bool) -> bool {
        let _ = always;
        false
    }

    fn get_window_flags(&self, py: Python<'_>) -> PyResult<i64> {
        let win = need_window(py)?;
        let mut f = 0;
        if win.is_visible() != Some(false) {
            f |= WINDOW_SHOWN;
        }
        if win.is_resizable() {
            f |= WINDOW_RESIZABLE;
        }
        if !win.is_decorated() {
            f |= WINDOW_BORDERLESS;
        }
        if win.fullscreen().is_some() {
            f |= WINDOW_FULLSCREEN_DESKTOP;
        }
        if win.is_maximized() {
            f |= WINDOW_MAXIMIZED;
        }
        if win.is_minimized() == Some(true) {
            f |= WINDOW_MINIMIZED;
        }
        if win.has_focus() {
            f |= WINDOW_INPUT_FOCUS;
        }
        if evloop::INPUT.lock().mouse_focus {
            f |= WINDOW_MOUSE_FOCUS;
        }
        Ok(f)
    }

    fn proxy_window_surface(&self, py: Python<'_>) -> PyResult<()> {
        Err(software_present_error(py))
    }

    fn flip(&self, py: Python<'_>) -> PyResult<()> {
        Err(software_present_error(py))
    }

    #[pyo3(signature = (rectangles=None))]
    fn update(&self, py: Python<'_>, rectangles: Option<Py<PyAny>>) -> PyResult<()> {
        let _ = rectangles;
        Err(software_present_error(py))
    }

    fn get_wm_info<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        PyDict::new(py)
    }

    fn get_active(&self) -> bool {
        active()
    }

    fn iconify(&self) -> bool {
        do_iconify()
    }

    fn toggle_fullscreen(&self) -> bool {
        do_toggle_fullscreen()
    }

    #[pyo3(signature = (red, green=None, blue=None))]
    fn set_gamma(&self, red: f64, green: Option<f64>, blue: Option<f64>) -> bool {
        let _ = (red, green, blue);
        false
    }

    fn set_gamma_ramp(&self, red: Py<PyAny>, green: Py<PyAny>, blue: Py<PyAny>) -> bool {
        let _ = (red, green, blue);
        false
    }

    fn set_icon(&self, py: Python<'_>, surface: &Bound<'_, PyAny>) -> PyResult<()> {
        apply_icon(py, surface)
    }

    fn set_caption(&self, title: &Bound<'_, PyAny>) -> PyResult<()> {
        let t = text_arg(title)?;
        if let Some(w) = evloop::window() {
            w.set_title(&t);
        }
        Ok(())
    }

    fn get_drawable_size(&self) -> (u32, u32) {
        evloop::drawable_size()
    }

    fn get_size(&self, py: Python<'_>) -> PyResult<(i64, i64)> {
        let w = need_window(py)?;
        Ok(logical_size(&w))
    }

    fn restore(&self) {
        if let Some(w) = evloop::window() {
            w.set_minimized(false);
            w.set_maximized(false);
        }
    }

    fn maximize(&self) {
        if let Some(w) = evloop::window() {
            w.set_maximized(true);
        }
    }

    fn minimize(&self) {
        do_iconify();
    }

    fn get_sdl_window_pointer(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Err(util::pg_error(py, "This player has no SDL window pointer."))
    }

    fn get_position(&self, py: Python<'_>) -> PyResult<(i64, i64)> {
        position(py)
    }

    fn set_position(&self, pos: (i64, i64)) {
        if let Some(w) = evloop::window() {
            apply_pos(&w, pos);
        }
    }
}

fn software_present_error(py: Python<'_>) -> PyErr {
    util::pg_error(
        py,
        "The window surface cannot be presented: this player draws with the wgpu renderer only.",
    )
}

/// Accepts `str` or UTF-8 `bytes`, as Ren'Py passes either.
fn text_arg(v: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(b) = v.cast::<PyBytes>() {
        return Ok(String::from_utf8_lossy(b.as_bytes()).into_owned());
    }
    v.extract()
}

fn apply_icon(py: Python<'_>, surface: &Bound<'_, PyAny>) -> PyResult<()> {
    let (w, h, rgba) = util::read_rgba(surface)?;
    if let Some(win) = evloop::window() {
        match Icon::from_rgba(rgba.clone(), w, h) {
            Ok(icon) => win.set_window_icon(Some(icon)),
            Err(e) => return Err(os_err(py, "bad window icon", e)),
        }
    }
    evloop::INPUT.lock().icon = Some((w, h, rgba));
    Ok(())
}

fn active() -> bool {
    match evloop::window() {
        Some(w) => w.is_visible() != Some(false) && w.is_minimized() != Some(true),
        None => false,
    }
}

fn do_iconify() -> bool {
    if let Some(w) = evloop::window() {
        w.set_minimized(true);
    }
    true
}

fn do_toggle_fullscreen() -> bool {
    if let Some(w) = evloop::window() {
        let on = w.fullscreen().is_none();
        evloop::set_fullscreen(&w, on);
    }
    true
}

fn position(py: Python<'_>) -> PyResult<(i64, i64)> {
    let w = need_window(py)?;
    let p = w
        .outer_position()
        .map_err(|e| os_err(py, "cannot read the window position", e))?;
    let l = p.to_logical::<f64>(w.scale_factor());
    Ok((l.x.round() as i64, l.y.round() as i64))
}

#[pyfunction]
fn sdl_main_init() {}

#[pyfunction]
fn init() {
    INIT_DONE.store(true, Ordering::Relaxed);
}

#[pyfunction]
fn quit() {
    destroy();
    INIT_DONE.store(false, Ordering::Relaxed);
}

#[pyfunction]
fn get_init() -> bool {
    INIT_DONE.load(Ordering::Relaxed)
}

#[pyfunction]
#[pyo3(signature = (hint, value, priority=1))]
fn hint(hint: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>, priority: i64) -> PyResult<()> {
    let _ = priority;
    HINTS.lock().insert(text_arg(hint)?, text_arg(value)?);
    Ok(())
}

#[pyfunction]
fn _get_hint(hint: &str, default: Py<PyAny>, py: Python<'_>) -> Py<PyAny> {
    if let Some(v) = HINTS.lock().get(hint) {
        return v
            .clone()
            .into_pyobject(py)
            .map_or_else(|_| default, |o| o.into_any().unbind());
    }
    if let Ok(v) = std::env::var(hint) {
        return v
            .into_pyobject(py)
            .map_or_else(|_| default, |o| o.into_any().unbind());
    }
    default
}

#[pyfunction]
#[pyo3(signature = (resolution=(0, 0), flags=0, depth=0, pos=(WINDOWPOS_UNDEFINED, WINDOWPOS_UNDEFINED)))]
fn set_mode(
    py: Python<'_>,
    resolution: (i64, i64),
    flags: i64,
    depth: i64,
    pos: (i64, i64),
) -> PyResult<Py<PyAny>> {
    let _ = depth;
    let resize_flags = WINDOW_OPENGL | WINDOW_FULLSCREEN_DESKTOP;
    let existing = MAIN_WINDOW.lock().as_ref().map(|w| w.clone_ref(py));
    if let Some(win) = existing {
        let win = win.bind(py);
        if (flags & !resize_flags) == (win.borrow().create_flags & !resize_flags)
            && evloop::window().is_some()
        {
            win.borrow().resize(
                py,
                resolution,
                flags & WINDOW_OPENGL != 0,
                Some(flags & WINDOW_FULLSCREEN != 0),
                None,
            )?;
            return win.borrow().get_surface(py);
        }
        win.borrow().destroy();
        *MAIN_WINDOW.lock() = None;
    }
    let win = Bound::new(py, open(py, &title_or_default(), resolution, flags, pos)?)?;
    let surface = win.borrow().get_surface(py)?;
    *MAIN_WINDOW.lock() = Some(win.unbind());
    Ok(surface)
}

#[pyfunction]
fn destroy() {
    if let Some(w) = MAIN_WINDOW.lock().take() {
        Python::attach(|py| w.bind(py).borrow().destroy());
    }
}

#[pyfunction]
fn get_surface(py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
    let w = MAIN_WINDOW.lock().as_ref().map(|w| w.clone_ref(py));
    w.map(|w| w.bind(py).borrow().get_surface(py)).transpose()
}

#[pyfunction]
fn get_window(py: Python<'_>) -> Option<Py<PyWindow>> {
    MAIN_WINDOW.lock().as_ref().map(|w| w.clone_ref(py))
}

#[pyfunction]
fn flip(py: Python<'_>) -> PyResult<()> {
    if MAIN_WINDOW.lock().is_some() {
        Err(software_present_error(py))
    } else {
        Ok(())
    }
}

#[pyfunction]
#[pyo3(signature = (rectangles=None))]
fn update(py: Python<'_>, rectangles: Option<Py<PyAny>>) -> PyResult<()> {
    let _ = rectangles;
    flip(py)
}

#[pyfunction]
fn get_driver() -> &'static str {
    "winit"
}

#[pyfunction]
fn get_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "Mac OS X"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

#[pyfunction]
fn get_wm_info(py: Python<'_>) -> Bound<'_, PyDict> {
    PyDict::new(py)
}

#[pyfunction]
fn get_num_video_displays(py: Python<'_>) -> PyResult<usize> {
    Ok(evloop::monitors()
        .map_err(|e| os_err(py, "cannot query monitors", e))?
        .len())
}

#[pyfunction]
#[pyo3(signature = (index))]
fn get_display_bounds(py: Python<'_>, index: usize) -> PyResult<(i32, i32, i32, i32)> {
    let m = evloop::monitors().map_err(|e| os_err(py, "cannot query monitors", e))?;
    let m = m
        .get(index)
        .ok_or_else(|| util::pg_error(py, "Display index out of range."))?;
    Ok((m.x, m.y, m.w, m.h))
}

#[pyfunction]
#[pyo3(signature = (depth=0, flags=0, display=0))]
fn list_modes(py: Python<'_>, depth: i64, flags: i64, display: usize) -> PyResult<Vec<(i32, i32)>> {
    let _ = (depth, flags);
    let m = evloop::monitors().map_err(|e| os_err(py, "cannot query monitors", e))?;
    let m = m
        .get(display)
        .ok_or_else(|| util::pg_error(py, "Display index out of range."))?;
    Ok(m.modes.clone())
}

#[pyfunction]
#[pyo3(signature = (size, flags=0, depth=0))]
fn mode_ok(py: Python<'_>, size: (i32, i32), flags: i64, depth: i64) -> PyResult<bool> {
    Ok(list_modes(py, depth, flags, 0)?.contains(&size))
}

#[pyfunction]
fn _info(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let monitors = evloop::monitors().map_err(|e| os_err(py, "cannot query monitors", e))?;
    let m = monitors
        .first()
        .ok_or_else(|| util::pg_error(py, "no video display found"))?;
    let d = PyDict::new(py);
    d.set_item("bitsize", 32)?;
    d.set_item("bytesize", 4)?;
    d.set_item(
        "masks",
        (
            0x00ff_0000u32,
            0x0000_ff00u32,
            0x0000_00ffu32,
            0xff00_0000u32,
        ),
    )?;
    d.set_item("shifts", (16, 8, 0, 24))?;
    d.set_item("losses", (0, 0, 0, 0))?;
    let (cw, ch) = match evloop::window() {
        Some(w) => logical_size(&w),
        None => (i64::from(m.w), i64::from(m.h)),
    };
    d.set_item("current_w", cw)?;
    d.set_item("current_h", ch)?;
    d.set_item("refresh_rate", m.refresh)?;
    d.set_item("hw", false)?;
    d.set_item("wm", true)?;
    d.set_item("video_mem", 256 * 1024 * 1024)?;
    for k in [
        "blit_hw",
        "blit_hw_CC",
        "blit_hw_A",
        "blit_sw",
        "blit_sw_CC",
        "blit_sw_A",
    ] {
        d.set_item(k, false)?;
    }
    Ok(d)
}

#[pyfunction]
fn gl_reset_attributes() {
    GL_ATTRIBUTES.lock().clear();
}

#[pyfunction]
fn gl_set_attribute(flag: i64, value: i64) {
    GL_ATTRIBUTES.lock().insert(flag, value);
}

#[pyfunction]
fn gl_get_attribute(py: Python<'_>, flag: i64) -> PyResult<i64> {
    GL_ATTRIBUTES.lock().get(&flag).copied().ok_or_else(|| {
        util::pg_error(
            py,
            "OpenGL attribute was not set: this player has no OpenGL context.",
        )
    })
}

#[pyfunction]
#[pyo3(signature = (path=None))]
fn gl_load_library(py: Python<'_>, path: Option<Py<PyAny>>) -> PyResult<()> {
    let _ = path;
    Err(util::pg_error(py, "This player has no OpenGL library."))
}

#[pyfunction]
fn gl_unload_library() {}

#[pyfunction]
fn get_active() -> bool {
    active()
}

#[pyfunction]
fn iconify() -> bool {
    do_iconify()
}

#[pyfunction]
fn toggle_fullscreen() -> bool {
    do_toggle_fullscreen()
}

#[pyfunction]
#[pyo3(signature = (red, green=None, blue=None))]
fn set_gamma(red: f64, green: Option<f64>, blue: Option<f64>) -> bool {
    let _ = (red, green, blue);
    false
}

#[pyfunction]
fn set_gamma_ramp(red: Py<PyAny>, green: Py<PyAny>, blue: Py<PyAny>) -> bool {
    let _ = (red, green, blue);
    false
}

#[pyfunction]
fn set_icon(py: Python<'_>, surface: &Bound<'_, PyAny>) -> PyResult<()> {
    apply_icon(py, surface)
}

#[pyfunction]
#[pyo3(signature = (title, icontitle=None))]
fn set_caption(title: &Bound<'_, PyAny>, icontitle: Option<Py<PyAny>>) -> PyResult<()> {
    let _ = icontitle;
    let t = text_arg(title)?;
    if let Some(w) = evloop::window() {
        w.set_title(&t);
    }
    evloop::INPUT.lock().title = t;
    Ok(())
}

#[pyfunction]
fn get_caption() -> String {
    title_or_default()
}

#[pyfunction]
fn get_drawable_size() -> Option<(u32, u32)> {
    evloop::window().map(|_| evloop::drawable_size())
}

#[pyfunction]
fn get_size() -> Option<(i64, i64)> {
    evloop::window().map(|w| logical_size(&w))
}

#[pyfunction]
fn get_position(py: Python<'_>) -> PyResult<Option<(i64, i64)>> {
    if evloop::window().is_none() {
        return Ok(None);
    }
    position(py).map(Some)
}

#[pyfunction]
fn set_position(pos: (i64, i64)) -> bool {
    match evloop::window() {
        Some(w) => {
            apply_pos(&w, pos);
            true
        }
        None => false,
    }
}

/// Records the request. The player holds no power-management assertion.
#[pyfunction]
fn set_screensaver(state: bool) {
    SCREENSAVER.store(state, Ordering::Relaxed);
}

#[pymodule(gil_used = false)]
pub mod renpy_pygame_display {
    use super::*;

    #[pymodule_init]
    fn module_init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        let py = m.py();
        py.run(
            c"class Info(object):
    def __init__(self):
        self.__dict__.update(_info())

    def __repr__(self):
        return '<Info({!r})>'.format(self.__dict__)
",
            Some(&m.dict()),
            None,
        )?;
        util::register_hook(m, "register_init", "init")?;
        util::register_hook(m, "register_quit", "quit")
    }

    #[pymodule_export]
    use super::PyWindow as Window;
    #[pymodule_export]
    use super::{
        _get_hint, _info, destroy, flip, get_active, get_caption, get_display_bounds,
        get_drawable_size, get_driver, get_init, get_num_video_displays, get_platform,
        get_position, get_size, get_surface, get_window, get_wm_info, gl_get_attribute,
        gl_load_library, gl_reset_attributes, gl_set_attribute, gl_unload_library, hint, iconify,
        init, list_modes, mode_ok, quit, sdl_main_init, set_caption, set_gamma, set_gamma_ramp,
        set_icon, set_mode, set_position, set_screensaver, toggle_fullscreen, update,
    };
}
