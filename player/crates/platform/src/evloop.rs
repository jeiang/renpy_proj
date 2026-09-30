//! The winit event loop (pump mode, main thread), the one window, and the
//! translation of winit events into SDL2-style pygame events.

use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, anyhow};
use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersKeyState, ModifiersState, PhysicalKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};
use winit::window::{Fullscreen, Icon, Window, WindowId};

use crate::event::{self, consts};
use crate::keys;

/// Keyboard, mouse and text-input state that `pygame.key` and `pygame.mouse` read back.
pub struct Input {
    pub mouse_pos: (i32, i32),
    pub mouse_mask: u32,
    pub last_cursor: Option<(f64, f64)>,
    pub rel_acc: (i32, i32),
    pub mouse_focus: bool,
    pub mods: u16,
    pub pressed: HashSet<u32>,
    pub text_input: bool,
    pub wheel_acc: (f64, f64),
    pub mousewheel_buttons: bool,
    pub grab: bool,
    pub title: String,
    pub icon: Option<(u32, u32, Vec<u8>)>,
}

pub static INPUT: LazyLock<Mutex<Input>> = LazyLock::new(|| {
    Mutex::new(Input {
        mouse_pos: (0, 0),
        mouse_mask: 0,
        last_cursor: None,
        rel_acc: (0, 0),
        mouse_focus: false,
        mods: 0,
        pressed: HashSet::new(),
        text_input: false,
        wheel_acc: (0.0, 0.0),
        mousewheel_buttons: true,
        grab: false,
        title: String::new(),
        icon: None,
    })
});

static WINDOW: Mutex<Option<Arc<Window>>> = Mutex::new(None);
static PROXY: Mutex<Option<EventLoopProxy<()>>> = Mutex::new(None);

type Job = Box<dyn FnOnce(&ActiveEventLoop)>;

thread_local! {
    static EVENT_LOOP: RefCell<Option<EventLoop<()>>> = const { RefCell::new(None) };
    static LOOP_BUILT: Cell<bool> = const { Cell::new(false) };
    static JOBS: RefCell<VecDeque<Job>> = const { RefCell::new(VecDeque::new()) };
    static RESUMED: Cell<bool> = const { Cell::new(false) };
}

/// The one window, if it exists.
pub fn window() -> Option<Arc<Window>> {
    WINDOW.lock().clone()
}

/// Drops the stored window handle and hides the window. Other holders of the
/// `Arc` (the renderer) keep it alive until they drop it.
pub fn destroy_window() {
    if let Some(w) = WINDOW.lock().take() {
        w.set_visible(false);
    }
}

/// Size of the window in physical pixels; `(0, 0)` without a window.
pub fn drawable_size() -> (u32, u32) {
    window().map_or((0, 0), |w| {
        let s = w.inner_size();
        (s.width, s.height)
    })
}

/// Wakes a blocked `pump` (from any thread).
pub fn wake() {
    if let Some(p) = PROXY.lock().as_ref() {
        let _ = p.send_event(());
    }
}

/// True when the event loop exists and this thread may pump it.
pub fn loop_ready() -> bool {
    EVENT_LOOP.with(|l| l.borrow().is_some())
}

fn ensure_loop() -> anyhow::Result<()> {
    if LOOP_BUILT.get() {
        return Ok(());
    }
    let el = EventLoop::<()>::with_user_event().build().map_err(|e| {
        anyhow!("cannot create the winit event loop (it must run on the main thread): {e}")
    })?;
    *PROXY.lock() = Some(el.create_proxy());
    EVENT_LOOP.with(|l| *l.borrow_mut() = Some(el));
    LOOP_BUILT.set(true);
    Ok(())
}

/// Runs winit for at most `timeout`. Does nothing when no loop exists yet or it is already running.
pub fn pump(timeout: Option<Duration>) {
    let Some(mut el) = EVENT_LOOP.with(|l| l.borrow_mut().take()) else {
        return;
    };
    let status = el.pump_app_events(timeout, &mut App);
    EVENT_LOOP.with(|l| *l.borrow_mut() = Some(el));
    if let PumpStatus::Exit(code) = status {
        log::warn!("winit event loop asked to exit with code {code}");
    }
}

fn run_jobs(el: &ActiveEventLoop) {
    if !RESUMED.get() {
        return;
    }
    loop {
        let job = JOBS.with(|j| j.borrow_mut().pop_front());
        match job {
            Some(job) => job(el),
            None => break,
        }
    }
}

/// Runs `f` on the event loop thread with access to the `ActiveEventLoop` and returns its result.
pub fn on_loop<R: 'static>(f: impl FnOnce(&ActiveEventLoop) -> R + 'static) -> anyhow::Result<R> {
    ensure_loop()?;
    let slot: Rc<RefCell<Option<R>>> = Rc::new(RefCell::new(None));
    let out = slot.clone();
    JOBS.with(|j| {
        j.borrow_mut()
            .push_back(Box::new(move |el| *out.borrow_mut() = Some(f(el))))
    });
    for i in 0..400 {
        pump(Some(if i < 4 {
            Duration::ZERO
        } else {
            Duration::from_millis(10)
        }));
        if let Some(r) = slot.borrow_mut().take() {
            return Ok(r);
        }
    }
    Err(anyhow!("the winit event loop did not start"))
}

/// Creates the one window, or resizes and returns the existing one.
pub fn create_window(
    width: u32,
    height: u32,
    title: &str,
    resizable: bool,
) -> anyhow::Result<Arc<Window>> {
    if let Some(w) = window() {
        w.set_title(title);
        w.set_resizable(resizable);
        if w.fullscreen().is_none() && !w.is_maximized() {
            let _ = w.request_inner_size(LogicalSize::new(width, height));
        }
        pump(Some(Duration::ZERO));
        return Ok(w);
    }
    let title = title.to_string();
    // The window is stored inside the job: events that arrive during creation
    // (`Resized`, `Focused`) need its scale factor.
    let w = on_loop(move |el| {
        let w = el.create_window(
            Window::default_attributes()
                .with_title(title)
                .with_inner_size(LogicalSize::new(width, height))
                .with_resizable(resizable),
        )?;
        let w = Arc::new(w);
        *WINDOW.lock() = Some(w.clone());
        w.focus_window();
        Ok::<_, winit::error::OsError>(w)
    })?
    .context("cannot create the window")?;
    if let Some((iw, ih, rgba)) = INPUT.lock().icon.clone()
        && let Ok(icon) = Icon::from_rgba(rgba, iw, ih) {
            w.set_window_icon(Some(icon));
        }
    pump(Some(Duration::ZERO));
    Ok(w)
}

/// A monitor as SDL reports it: logical points.
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub refresh: i32,
    pub modes: Vec<(i32, i32)>,
}

/// All monitors, primary first.
pub fn monitors() -> anyhow::Result<Vec<Monitor>> {
    on_loop(|el| {
        let primary = el.primary_monitor();
        let mut list: Vec<_> = el.available_monitors().collect();
        if let Some(p) = primary
            && let Some(i) = list.iter().position(|m| *m == p) {
                list.swap(0, i);
            }
        list.into_iter()
            .map(|m| {
                let s = m.scale_factor();
                let pos: LogicalPosition<f64> = m.position().to_logical(s);
                let size = m.size().to_logical::<f64>(s);
                let mut modes: Vec<(i32, i32)> = Vec::new();
                for vm in m.video_modes() {
                    let sz = vm.size().to_logical::<f64>(s);
                    let t = (sz.width.round() as i32, sz.height.round() as i32);
                    if !modes.contains(&t) {
                        modes.push(t);
                    }
                }
                Monitor {
                    x: pos.x.round() as i32,
                    y: pos.y.round() as i32,
                    w: size.width.round() as i32,
                    h: size.height.round() as i32,
                    refresh: m
                        .refresh_rate_millihertz()
                        .map_or(60, |r| (r as f64 / 1000.0).round() as i32),
                    modes,
                }
            })
            .collect()
    })
}

pub fn set_fullscreen(w: &Window, on: bool) {
    w.set_fullscreen(if on {
        Some(Fullscreen::Borderless(None))
    } else {
        None
    });
}

fn scale() -> f64 {
    window().map_or(1.0, |w| w.scale_factor())
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

struct App;

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        RESUMED.set(true);
        run_jobs(el);
    }

    fn new_events(&mut self, el: &ActiveEventLoop, _cause: winit::event::StartCause) {
        run_jobs(el);
    }

    fn user_event(&mut self, el: &ActiveEventLoop, _event: ()) {
        run_jobs(el);
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        run_jobs(el);
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        Python::attach(|py| {
            if let Err(e) = translate(py, ev) {
                e.print(py);
            }
        });
    }
}

fn round(v: f64) -> i64 {
    v.round() as i64
}

fn simple(py: Python<'_>, typ: i64, attrs: &[(&str, i64)]) -> PyResult<()> {
    event::push_native(py, typ, |d: &Bound<'_, PyDict>| {
        for (k, v) in attrs {
            d.set_item(*k, *v)?;
        }
        Ok(())
    })
}

fn button_number(b: MouseButton) -> i64 {
    match b {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
        MouseButton::Back => 4,
        MouseButton::Forward => 5,
        MouseButton::Other(n) => 6 + i64::from(n),
    }
}

fn button_mask(n: i64) -> u32 {
    if (1..=32).contains(&n) {
        1 << (n - 1)
    } else {
        0
    }
}

fn buttons_tuple(mask: u32) -> (i64, i64, i64) {
    (
        i64::from(mask & 1 != 0),
        i64::from(mask & 2 != 0),
        i64::from(mask & 4 != 0),
    )
}

fn mods_from(m: &winit::event::Modifiers) -> u16 {
    let s: ModifiersState = m.state();
    let pair =
        |l: ModifiersKeyState, r: ModifiersKeyState, any: bool, lbit: u16, rbit: u16| -> u16 {
            let lp = l == ModifiersKeyState::Pressed;
            let rp = r == ModifiersKeyState::Pressed;
            let mut v = 0;
            if lp {
                v |= lbit;
            }
            if rp {
                v |= rbit;
            }
            if any && v == 0 {
                v = lbit;
            }
            v
        };
    pair(
        m.lshift_state(),
        m.rshift_state(),
        s.shift_key(),
        keys::KMOD_LSHIFT,
        keys::KMOD_RSHIFT,
    ) | pair(
        m.lcontrol_state(),
        m.rcontrol_state(),
        s.control_key(),
        keys::KMOD_LCTRL,
        keys::KMOD_RCTRL,
    ) | pair(
        m.lalt_state(),
        m.ralt_state(),
        s.alt_key(),
        keys::KMOD_LALT,
        keys::KMOD_RALT,
    ) | pair(
        m.lsuper_state(),
        m.rsuper_state(),
        s.super_key(),
        keys::KMOD_LGUI,
        keys::KMOD_RGUI,
    )
}

fn printable(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| {
            let u = c as u32;
            u >= 0x20 && u != 0x7f && !(0xF700..=0xF8FF).contains(&u)
        })
}

fn key_event(py: Python<'_>, e: &KeyEvent) -> PyResult<()> {
    let pressed = e.state == ElementState::Pressed;
    let (sc, phys) = match e.physical_key {
        PhysicalKey::Code(c) => (keys::scancode(c), keys::keycode_from_physical(c)),
        PhysicalKey::Unidentified(_) => (0, 0),
    };
    let sym = match e.key_without_modifiers() {
        Key::Character(s) => {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(ch), None) => keys::keycode_from_char(ch),
                _ => phys,
            }
        }
        _ => phys,
    };

    let (mods, text_input) = {
        let mut inp = INPUT.lock();
        if pressed {
            inp.pressed.insert(sc);
        } else {
            inp.pressed.remove(&sc);
        }
        (inp.mods, inp.text_input)
    };

    let text = if pressed {
        e.text.as_deref().filter(|t| printable(t))
    } else {
        None
    };
    let shortcut =
        mods & (keys::KMOD_LCTRL | keys::KMOD_RCTRL | keys::KMOD_LGUI | keys::KMOD_RGUI) != 0;

    let unicode: String = if !pressed || text_input {
        String::new()
    } else if sym < 0x20 {
        char::from_u32(sym).map(String::from).unwrap_or_default()
    } else if sym <= 0xFFFF && !shortcut {
        text.unwrap_or("").to_string()
    } else {
        String::new()
    };

    let typ = if pressed {
        consts::KEYDOWN
    } else {
        consts::KEYUP
    };
    event::push_native(py, typ, |d| {
        d.set_item("scancode", sc)?;
        d.set_item("key", sym)?;
        d.set_item("mod", mods)?;
        d.set_item("unicode", unicode)?;
        d.set_item("repeat", i64::from(e.repeat))?;
        Ok(())
    })?;

    if pressed && text_input && !shortcut
        && let Some(t) = text {
            let t = t.to_string();
            event::push_native(py, consts::TEXTINPUT, |d| d.set_item("text", t))?;
        }
    Ok(())
}

fn wheel(py: Python<'_>, delta: MouseScrollDelta) -> PyResult<()> {
    let (lines_x, lines_y) = match delta {
        MouseScrollDelta::LineDelta(x, y) => (f64::from(x), f64::from(y)),
        MouseScrollDelta::PixelDelta(p) => {
            let s = scale();
            (p.x / s * 0.1, p.y / s * 0.1)
        }
    };
    let (ix, iy, pos, as_buttons) = {
        let mut inp = INPUT.lock();
        inp.wheel_acc.0 += lines_x;
        inp.wheel_acc.1 += lines_y;
        let ix = inp.wheel_acc.0.trunc();
        let iy = inp.wheel_acc.1.trunc();
        inp.wheel_acc.0 -= ix;
        inp.wheel_acc.1 -= iy;
        (ix as i64, iy as i64, inp.mouse_pos, inp.mousewheel_buttons)
    };
    if ix == 0 && iy == 0 {
        return Ok(());
    }
    if !as_buttons {
        return event::push_native(py, consts::MOUSEWHEEL, |d| {
            d.set_item("which", 0)?;
            d.set_item("x", ix)?;
            d.set_item("y", iy)?;
            d.set_item("touch", false)
        });
    }
    // SDL 1.x style: the wheel is buttons 4 and 5, a press followed by a release.
    let button = if iy > 0 {
        4
    } else if iy < 0 {
        5
    } else {
        return Ok(());
    };
    for _ in 0..iy.unsigned_abs() {
        for typ in [consts::MOUSEBUTTONDOWN, consts::MOUSEBUTTONUP] {
            event::push_native(py, typ, |d| {
                d.set_item("which", 0)?;
                d.set_item("button", button)?;
                d.set_item("pos", pos)?;
                d.set_item("touch", false)
            })?;
        }
    }
    Ok(())
}

fn translate(py: Python<'_>, ev: WindowEvent) -> PyResult<()> {
    match ev {
        WindowEvent::CloseRequested => simple(py, consts::QUIT, &[]),
        WindowEvent::Destroyed => Ok(()),
        WindowEvent::Resized(size) => {
            let l = size.to_logical::<f64>(scale());
            let (w, h) = (round(l.width), round(l.height));
            event::push_native(py, consts::VIDEORESIZE, |d| {
                d.set_item("size", (w, h))?;
                d.set_item("w", w)?;
                d.set_item("h", h)
            })
        }
        WindowEvent::Moved(p) => {
            let l: LogicalPosition<f64> = p.to_logical(scale());
            let (x, y) = (round(l.x), round(l.y));
            event::push_native(py, consts::WINDOWMOVED, |d| {
                d.set_item("pos", (x, y))?;
                d.set_item("x", x)?;
                d.set_item("y", y)
            })
        }
        WindowEvent::Focused(gain) => {
            if !gain {
                let mut inp = INPUT.lock();
                inp.pressed.clear();
            }
            simple(
                py,
                consts::ACTIVEEVENT,
                &[("state", 2), ("gain", i64::from(gain))],
            )
        }
        WindowEvent::Occluded(hidden) => simple(
            py,
            consts::ACTIVEEVENT,
            &[("state", 4), ("gain", i64::from(!hidden))],
        ),
        WindowEvent::CursorEntered { .. } => {
            INPUT.lock().mouse_focus = true;
            simple(py, consts::ACTIVEEVENT, &[("state", 1), ("gain", 1)])
        }
        WindowEvent::CursorLeft { .. } => {
            INPUT.lock().mouse_focus = false;
            simple(py, consts::ACTIVEEVENT, &[("state", 1), ("gain", 0)])
        }
        WindowEvent::RedrawRequested => simple(py, consts::VIDEOEXPOSE, &[]),
        WindowEvent::ModifiersChanged(m) => {
            let mut inp = INPUT.lock();
            let caps = inp.mods & keys::KMOD_CAPS;
            inp.mods = mods_from(&m) | caps;
            Ok(())
        }
        WindowEvent::KeyboardInput { event, .. } => {
            if event.physical_key == PhysicalKey::Code(winit::keyboard::KeyCode::CapsLock)
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                INPUT.lock().mods ^= keys::KMOD_CAPS;
            }
            key_event(py, &event)
        }
        WindowEvent::Ime(ime) => match ime {
            Ime::Commit(text) => {
                event::push_native(py, consts::TEXTINPUT, |d| d.set_item("text", text))
            }
            Ime::Preedit(text, cursor) => {
                let (start, length) = match cursor {
                    Some((a, b)) => {
                        let s = text.get(..a).map_or(0, |p| p.chars().count());
                        let l = text.get(a..b).map_or(0, |p| p.chars().count());
                        (s as i64, l as i64)
                    }
                    None => (text.chars().count() as i64, 0),
                };
                event::push_native(py, consts::TEXTEDITING, |d| {
                    d.set_item("text", text)?;
                    d.set_item("start", start)?;
                    d.set_item("length", length)
                })
            }
            Ime::Enabled | Ime::Disabled => Ok(()),
        },
        WindowEvent::CursorMoved { position, .. } => {
            let l: LogicalPosition<f64> = position.to_logical(scale());
            let (pos, rel, mask) = {
                let mut inp = INPUT.lock();
                let rel = match inp.last_cursor {
                    Some((px, py_)) => ((l.x - px).round() as i32, (l.y - py_).round() as i32),
                    None => (0, 0),
                };
                inp.last_cursor = Some((l.x, l.y));
                let pos = (l.x as i32, l.y as i32);
                inp.mouse_pos = pos;
                inp.rel_acc.0 += rel.0;
                inp.rel_acc.1 += rel.1;
                (pos, rel, inp.mouse_mask)
            };
            event::push_native(py, consts::MOUSEMOTION, |d| {
                d.set_item("pos", pos)?;
                d.set_item("rel", rel)?;
                d.set_item("which", 0)?;
                d.set_item("buttons", buttons_tuple(mask))?;
                d.set_item("touch", false)
            })
        }
        WindowEvent::MouseInput { state, button, .. } => {
            let n = button_number(button);
            let down = state == ElementState::Pressed;
            let pos = {
                let mut inp = INPUT.lock();
                if down {
                    inp.mouse_mask |= button_mask(n);
                } else {
                    inp.mouse_mask &= !button_mask(n);
                }
                inp.mouse_pos
            };
            let reported = if INPUT.lock().mousewheel_buttons && n >= 4 {
                n + 2
            } else {
                n
            };
            event::push_native(
                py,
                if down {
                    consts::MOUSEBUTTONDOWN
                } else {
                    consts::MOUSEBUTTONUP
                },
                |d| {
                    d.set_item("button", reported)?;
                    d.set_item("pos", pos)?;
                    d.set_item("which", 0)?;
                    d.set_item("touch", false)
                },
            )
        }
        WindowEvent::MouseWheel { delta, .. } => wheel(py, delta),
        WindowEvent::DroppedFile(path) => {
            let file = path.to_string_lossy().into_owned();
            event::push_native(py, consts::DROPFILE, |d| {
                d.set_item("file", file)?;
                d.set_item("window_id", 1)
            })
        }
        _ => Ok(()),
    }
}

/// Warps the cursor, in logical points.
pub fn warp_cursor(x: i32, y: i32) -> Result<(), winit::error::ExternalError> {
    if let Some(w) = window() {
        w.set_cursor_position(LogicalPosition::new(x, y))?;
    }
    let mut inp = INPUT.lock();
    inp.mouse_pos = (x, y);
    inp.last_cursor = Some((f64::from(x), f64::from(y)));
    Ok(())
}
