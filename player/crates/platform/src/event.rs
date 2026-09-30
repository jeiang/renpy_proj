//! `renpy.pygame.event`: the `Event` class, the event queue, timers and the
//! event functions. Event ids are SDL2's.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use pyo3::exceptions::{PyAttributeError, PyKeyError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::{evloop, timemod, util};

/// SDL2 event type values, plus the SDL 1.2 emulation events Ren'Py adds.
#[rustfmt::skip]
pub mod consts {
    pub const FIRSTEVENT: i64 = 0;
    pub const QUIT: i64 = 0x100;
    pub const APP_TERMINATING: i64 = 0x101;
    pub const APP_LOWMEMORY: i64 = 0x102;
    pub const APP_WILLENTERBACKGROUND: i64 = 0x103;
    pub const APP_DIDENTERBACKGROUND: i64 = 0x104;
    pub const APP_WILLENTERFOREGROUND: i64 = 0x105;
    pub const APP_DIDENTERFOREGROUND: i64 = 0x106;
    pub const DISPLAYEVENT: i64 = 0x150;
    pub const WINDOWEVENT: i64 = 0x200;
    pub const SYSWMEVENT: i64 = 0x201;
    pub const KEYDOWN: i64 = 0x300;
    pub const KEYUP: i64 = 0x301;
    pub const TEXTEDITING: i64 = 0x302;
    pub const TEXTINPUT: i64 = 0x303;
    pub const KEYMAPCHANGED: i64 = 0x304;
    pub const MOUSEMOTION: i64 = 0x400;
    pub const MOUSEBUTTONDOWN: i64 = 0x401;
    pub const MOUSEBUTTONUP: i64 = 0x402;
    pub const MOUSEWHEEL: i64 = 0x403;
    pub const JOYAXISMOTION: i64 = 0x600;
    pub const JOYBALLMOTION: i64 = 0x601;
    pub const JOYHATMOTION: i64 = 0x602;
    pub const JOYBUTTONDOWN: i64 = 0x603;
    pub const JOYBUTTONUP: i64 = 0x604;
    pub const JOYDEVICEADDED: i64 = 0x605;
    pub const JOYDEVICEREMOVED: i64 = 0x606;
    pub const CONTROLLERAXISMOTION: i64 = 0x650;
    pub const CONTROLLERBUTTONDOWN: i64 = 0x651;
    pub const CONTROLLERBUTTONUP: i64 = 0x652;
    pub const CONTROLLERDEVICEADDED: i64 = 0x653;
    pub const CONTROLLERDEVICEREMOVED: i64 = 0x654;
    pub const CONTROLLERDEVICEREMAPPED: i64 = 0x655;
    pub const FINGERDOWN: i64 = 0x700;
    pub const FINGERUP: i64 = 0x701;
    pub const FINGERMOTION: i64 = 0x702;
    pub const DOLLARGESTURE: i64 = 0x800;
    pub const DOLLARRECORD: i64 = 0x801;
    pub const MULTIGESTURE: i64 = 0x802;
    pub const CLIPBOARDUPDATE: i64 = 0x900;
    pub const DROPFILE: i64 = 0x1000;
    pub const DROPTEXT: i64 = 0x1001;
    pub const DROPBEGIN: i64 = 0x1002;
    pub const DROPCOMPLETE: i64 = 0x1003;
    pub const AUDIODEVICEADDED: i64 = 0x1100;
    pub const AUDIODEVICEREMOVED: i64 = 0x1101;
    pub const SENSORUPDATE: i64 = 0x1200;
    pub const RENDER_TARGETS_RESET: i64 = 0x2000;
    pub const RENDER_DEVICE_RESET: i64 = 0x2001;
    pub const USEREVENT: i64 = 0x8000;
    pub const LASTEVENT: i64 = 0xFFFF;
    pub const ACTIVEEVENT: i64 = LASTEVENT - 1;
    pub const VIDEORESIZE: i64 = LASTEVENT - 2;
    pub const VIDEOEXPOSE: i64 = LASTEVENT - 3;
    pub const WINDOWMOVED: i64 = LASTEVENT - 4;
    pub const USEREVENT_MAX: i64 = LASTEVENT - 6;
}

#[rustfmt::skip]
const STANDARD_NAMES: &[(i64, &str)] = &[
    (consts::FIRSTEVENT, "NOEVENT"), (consts::QUIT, "QUIT"),
    (consts::APP_TERMINATING, "APP_TERMINATING"), (consts::APP_LOWMEMORY, "APP_LOWMEMORY"),
    (consts::APP_WILLENTERBACKGROUND, "APP_WILLENTERBACKGROUND"),
    (consts::APP_DIDENTERBACKGROUND, "APP_DIDENTERBACKGROUND"),
    (consts::APP_WILLENTERFOREGROUND, "APP_WILLENTERFOREGROUND"),
    (consts::APP_DIDENTERFOREGROUND, "APP_DIDENTERFOREGROUND"),
    (consts::DISPLAYEVENT, "DISPLAYEVENT"), (consts::WINDOWEVENT, "WINDOWEVENT"),
    (consts::SYSWMEVENT, "SYSWMEVENT"), (consts::KEYDOWN, "KEYDOWN"), (consts::KEYUP, "KEYUP"),
    (consts::TEXTEDITING, "TEXTEDITING"), (consts::TEXTINPUT, "TEXTINPUT"),
    (consts::KEYMAPCHANGED, "KEYMAPCHANGED"), (consts::MOUSEMOTION, "MOUSEMOTION"),
    (consts::MOUSEBUTTONDOWN, "MOUSEBUTTONDOWN"), (consts::MOUSEBUTTONUP, "MOUSEBUTTONUP"),
    (consts::MOUSEWHEEL, "MOUSEWHEEL"), (consts::JOYAXISMOTION, "JOYAXISMOTION"),
    (consts::JOYBALLMOTION, "JOYBALLMOTION"), (consts::JOYHATMOTION, "JOYHATMOTION"),
    (consts::JOYBUTTONDOWN, "JOYBUTTONDOWN"), (consts::JOYBUTTONUP, "JOYBUTTONUP"),
    (consts::JOYDEVICEADDED, "JOYDEVICEADDED"), (consts::JOYDEVICEREMOVED, "JOYDEVICEREMOVED"),
    (consts::CONTROLLERAXISMOTION, "CONTROLLERAXISMOTION"),
    (consts::CONTROLLERBUTTONDOWN, "CONTROLLERBUTTONDOWN"),
    (consts::CONTROLLERBUTTONUP, "CONTROLLERBUTTONUP"),
    (consts::CONTROLLERDEVICEADDED, "CONTROLLERDEVICEADDED"),
    (consts::CONTROLLERDEVICEREMOVED, "CONTROLLERDEVICEREMOVED"),
    (consts::CONTROLLERDEVICEREMAPPED, "CONTROLLERDEVICEREMAPPED"),
    (consts::FINGERDOWN, "FINGERDOWN"), (consts::FINGERUP, "FINGERUP"),
    (consts::FINGERMOTION, "FINGERMOTION"), (consts::DOLLARGESTURE, "DOLLARGESTURE"),
    (consts::DOLLARRECORD, "DOLLARRECORD"), (consts::MULTIGESTURE, "MULTIGESTURE"),
    (consts::CLIPBOARDUPDATE, "CLIPBOARDUPDATE"), (consts::DROPFILE, "DROPFILE"),
    (consts::DROPTEXT, "DROPTEXT"), (consts::DROPBEGIN, "DROPBEGIN"),
    (consts::DROPCOMPLETE, "DROPCOMPLETE"), (consts::AUDIODEVICEADDED, "AUDIODEVICEADDED"),
    (consts::AUDIODEVICEREMOVED, "AUDIODEVICEREMOVED"), (consts::SENSORUPDATE, "SENSORUPDATE"),
    (consts::RENDER_TARGETS_RESET, "RENDER_TARGETS_RESET"),
    (consts::RENDER_DEVICE_RESET, "RENDER_DEVICE_RESET"), (consts::USEREVENT, "USEREVENT"),
    (consts::LASTEVENT, "LASTEVENT"), (consts::ACTIVEEVENT, "ACTIVEEVENT"),
    (consts::VIDEORESIZE, "VIDEORESIZE"), (consts::VIDEOEXPOSE, "VIDEOEXPOSE"),
    (consts::WINDOWMOVED, "WINDOWMOVED"),
];

struct Timer {
    next: Instant,
    interval: Duration,
    once: bool,
}

struct State {
    queue: VecDeque<(i64, Py<PyAny>)>,
    blocked: HashSet<i64>,
    timers: HashMap<i64, Timer>,
    next_user: i64,
}

static STATE: std::sync::LazyLock<Mutex<State>> = std::sync::LazyLock::new(|| {
    Mutex::new(State {
        queue: VecDeque::new(),
        blocked: HashSet::new(),
        timers: HashMap::new(),
        next_user: consts::USEREVENT,
    })
});

static NAMES: OnceLock<Py<PyDict>> = OnceLock::new();
static NOEVENT: OnceLock<Py<PyAny>> = OnceLock::new();

fn names(py: Python<'_>) -> &Bound<'_, PyDict> {
    NAMES.get().expect("renpy.pygame.event is initialised").bind(py)
}

/// The Python `Event` (`EventType`) class: a type id plus attributes.
#[pyclass(name = "EventType", module = "renpy.pygame.event", subclass)]
pub struct Event {
    typ: i64,
    dict: Py<PyDict>,
}

#[pymethods]
impl Event {
    #[new]
    #[pyo3(signature = (r#type, dict=None, **kwargs))]
    fn new(
        py: Python<'_>,
        r#type: i64,
        dict: Option<&Bound<'_, PyDict>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let d = PyDict::new(py);
        d.set_item("_type", r#type)?;
        if let Some(src) = dict {
            d.update(src.as_mapping())?;
        }
        if let Some(src) = kwargs {
            d.update(src.as_mapping())?;
        }
        Ok(Self { typ: r#type, dict: d.unbind() })
    }

    #[getter]
    fn r#type(&self) -> i64 {
        self.typ
    }

    #[getter]
    fn dict(&self, py: Python<'_>) -> Py<PyDict> {
        self.dict.clone_ref(py)
    }

    #[getter]
    fn __dict__(&self, py: Python<'_>) -> Py<PyDict> {
        self.dict.clone_ref(py)
    }

    fn __getattr__(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        match self.dict.bind(py).get_item(name)? {
            Some(v) => Ok(v.unbind()),
            None => Err(PyAttributeError::new_err(format!("'EventType' object has no attribute '{name}'"))),
        }
    }

    fn __setattr__(&self, py: Python<'_>, name: &str, value: Py<PyAny>) -> PyResult<()> {
        self.dict.bind(py).set_item(name, value)
    }

    fn __delattr__(&self, py: Python<'_>, name: &str) -> PyResult<()> {
        self.dict.bind(py).del_item(name).map_err(|e| {
            if e.is_instance_of::<PyKeyError>(py) {
                PyAttributeError::new_err(name.to_string())
            } else {
                e
            }
        })
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        match other.cast::<Event>() {
            Ok(o) => {
                let o = o.borrow();
                let same = self.dict.bind(py).eq(o.dict.bind(py))?;
                Ok(same.into_pyobject(py)?.to_owned().into_any().unbind())
            }
            Err(_) => Ok(py.NotImplemented()),
        }
    }

    fn __ne__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        match other.cast::<Event>() {
            Ok(o) => {
                let o = o.borrow();
                let same = self.dict.bind(py).eq(o.dict.bind(py))?;
                Ok((!same).into_pyobject(py)?.to_owned().into_any().unbind())
            }
            Err(_) => Ok(py.NotImplemented()),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let ename = if (consts::USEREVENT..consts::WINDOWMOVED).contains(&self.typ) {
            format!("UserEvent{}", self.typ - consts::USEREVENT)
        } else {
            match names(py).get_item(self.typ)? {
                Some(n) => n.extract::<String>()?,
                None => "UNKNOWN".to_string(),
            }
        };
        let mut items: Vec<(String, String)> = Vec::new();
        for (k, v) in self.dict.bind(py).iter() {
            let k: String = k.extract()?;
            if k == "_type" || k == "timestamp" {
                continue;
            }
            items.push((k, v.repr()?.to_string()));
        }
        items.sort();
        let rest: Vec<String> = items.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
        Ok(format!("<Event({}-{} {})>", self.typ, ename, rest.join(", ")))
    }
}

/// Creates an event, lets `fill` add attributes, stamps it, and queues it unless blocked.
pub fn push_native(
    py: Python<'_>,
    typ: i64,
    fill: impl FnOnce(&Bound<'_, PyDict>) -> PyResult<()>,
) -> PyResult<()> {
    if STATE.lock().blocked.contains(&typ) {
        return Ok(());
    }
    let d = PyDict::new(py);
    d.set_item("_type", typ)?;
    fill(&d)?;
    d.set_item("timestamp", timemod::ticks())?;
    let ev = Bound::new(py, Event { typ, dict: d.unbind() })?;
    STATE.lock().queue.push_back((typ, ev.into_any().unbind()));
    Ok(())
}

/// Earliest timer deadline, if any timer is armed.
pub fn next_timer_due() -> Option<Instant> {
    STATE.lock().timers.values().map(|t| t.next).min()
}

fn fire_timers(py: Python<'_>) -> PyResult<()> {
    let now = Instant::now();
    let due: Vec<i64> = {
        let mut st = STATE.lock();
        let mut due = Vec::new();
        let mut done = Vec::new();
        for (id, t) in st.timers.iter_mut() {
            if t.next <= now {
                due.push(*id);
                if t.once {
                    done.push(*id);
                } else {
                    t.next = now + t.interval;
                }
            }
        }
        for id in done {
            st.timers.remove(&id);
        }
        due
    };
    for id in due {
        push_native(py, id, |d| d.set_item("code", 0))?;
    }
    Ok(())
}

/// Pumps winit without blocking and fires due timers.
pub fn pump_events(py: Python<'_>) -> PyResult<()> {
    evloop::pump(Some(Duration::ZERO));
    fire_timers(py)
}

pub fn set_timer(eventid: i64, ms: i64, once: bool) {
    {
        let mut st = STATE.lock();
        st.timers.remove(&eventid);
        if ms > 0 {
            let interval = Duration::from_millis(ms as u64);
            st.timers.insert(eventid, Timer { next: Instant::now() + interval, interval, once });
        }
    }
    evloop::wake();
}

fn kinds(t: &Bound<'_, PyAny>) -> PyResult<Vec<i64>> {
    if let Ok(i) = t.extract::<i64>() {
        return Ok(vec![i]);
    }
    t.try_iter()?.map(|x| x?.extract::<i64>()).collect()
}

fn pop_front(py: Python<'_>) -> Option<Py<PyAny>> {
    STATE.lock().queue.pop_front().map(|(_, e)| e.clone_ref(py))
}

fn noevent(py: Python<'_>) -> Py<PyAny> {
    NOEVENT.get().expect("renpy.pygame.event is initialised").clone_ref(py)
}

#[pyfunction]
fn pump(py: Python<'_>) -> PyResult<()> {
    pump_events(py)
}

#[pyfunction]
#[pyo3(signature = (t=None))]
fn get(py: Python<'_>, t: Option<&Bound<'_, PyAny>>) -> PyResult<Py<PyList>> {
    pump_events(py)?;
    let want = match t {
        Some(t) if !t.is_none() => Some(kinds(t)?),
        _ => None,
    };
    let taken: Vec<Py<PyAny>> = {
        let mut st = STATE.lock();
        match want {
            None => st.queue.drain(..).map(|(_, e)| e).collect(),
            Some(w) => {
                let mut out = Vec::new();
                let mut keep = VecDeque::with_capacity(st.queue.len());
                for (typ, e) in st.queue.drain(..) {
                    if w.contains(&typ) {
                        out.push(e);
                    } else {
                        keep.push_back((typ, e));
                    }
                }
                st.queue = keep;
                out
            }
        }
    };
    Ok(PyList::new(py, taken)?.unbind())
}

#[pyfunction]
fn poll(py: Python<'_>) -> PyResult<Py<PyAny>> {
    pump_events(py)?;
    Ok(pop_front(py).unwrap_or_else(|| noevent(py)))
}

/// Blocks until an event arrives. `timeout` is in milliseconds; `None` or 0 waits forever.
/// Returns the no-event `Event(0)` on timeout.
#[pyfunction]
#[pyo3(signature = (timeout=None))]
fn wait(py: Python<'_>, timeout: Option<i64>) -> PyResult<Py<PyAny>> {
    let deadline = timeout.filter(|t| *t > 0).map(|t| Instant::now() + Duration::from_millis(t as u64));
    loop {
        pump_events(py)?;
        if let Some(e) = pop_front(py) {
            return Ok(e);
        }
        let now = Instant::now();
        let mut limit = Duration::from_millis(100);
        if let Some(d) = deadline {
            if d <= now {
                return Ok(noevent(py));
            }
            limit = limit.min(d - now);
        }
        if let Some(t) = next_timer_due() {
            limit = limit.min(t.saturating_duration_since(now));
        }
        py.check_signals()?;
        if evloop::loop_ready() {
            py.detach(|| evloop::pump(Some(limit)));
        } else {
            py.detach(|| std::thread::sleep(limit.min(Duration::from_millis(5))));
        }
    }
}

#[pyfunction]
#[pyo3(signature = (t=None))]
fn peek(py: Python<'_>, t: Option<&Bound<'_, PyAny>>) -> PyResult<bool> {
    pump_events(py)?;
    let st = STATE.lock();
    match t {
        Some(t) if !t.is_none() => {
            let w = kinds(t)?;
            Ok(st.queue.iter().any(|(typ, _)| w.contains(typ)))
        }
        _ => Ok(!st.queue.is_empty()),
    }
}

#[pyfunction]
#[pyo3(signature = (t=None))]
fn clear(py: Python<'_>, t: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
    get(py, t).map(|_| ())
}

#[pyfunction]
fn post(py: Python<'_>, e: &Bound<'_, PyAny>) -> PyResult<()> {
    let Ok(ev) = e.cast::<Event>() else {
        return Err(util::pg_error(py, "event.post must be called with an Event."));
    };
    let typ = ev.borrow().typ;
    {
        let mut st = STATE.lock();
        if st.blocked.contains(&typ) {
            return Ok(());
        }
        st.queue.push_back((typ, e.clone().unbind()));
    }
    evloop::wake();
    Ok(())
}

#[pyfunction]
fn register(py: Python<'_>, name: &str) -> PyResult<i64> {
    let id = {
        let mut st = STATE.lock();
        let id = st.next_user;
        st.next_user += 1;
        id
    };
    names(py).set_item(id, name)?;
    Ok(id)
}

#[pyfunction]
fn get_standard_events(py: Python<'_>) -> PyResult<Vec<i64>> {
    let mut out = Vec::new();
    for k in names(py).keys().iter() {
        let i: i64 = k.extract()?;
        if i < consts::USEREVENT || i > consts::USEREVENT_MAX {
            out.push(i);
        }
    }
    Ok(out)
}

#[pyfunction]
fn event_name(py: Python<'_>, t: i64) -> PyResult<String> {
    Ok(match names(py).get_item(t)? {
        Some(n) => n.extract()?,
        None => "UNKNOWN".to_string(),
    })
}

#[pyfunction]
#[pyo3(signature = (t=None))]
fn set_blocked(py: Python<'_>, t: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
    let ids = match t {
        Some(t) if !t.is_none() => Some(kinds(t)?),
        _ => None,
    };
    let mut st = STATE.lock();
    match ids {
        None => st.blocked.clear(),
        Some(ids) => st.blocked.extend(ids),
    }
    let _ = py;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (t=None))]
fn set_allowed(py: Python<'_>, t: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
    let ids = match t {
        Some(t) if !t.is_none() => Some(kinds(t)?),
        _ => None,
    };
    let all: Vec<i64> = if ids.is_none() {
        names(py).keys().iter().map(|k| k.extract()).collect::<PyResult<_>>()?
    } else {
        Vec::new()
    };
    let mut st = STATE.lock();
    match ids {
        None => st.blocked.extend(all),
        Some(ids) => {
            for i in ids {
                st.blocked.remove(&i);
            }
        }
    }
    Ok(())
}

#[pyfunction]
fn get_blocked(t: i64) -> bool {
    STATE.lock().blocked.contains(&t)
}

#[pyfunction]
fn set_grab(py: Python<'_>, on: bool) -> PyResult<()> {
    use winit::window::CursorGrabMode;
    if let Some(w) = evloop::window() {
        let res = if on {
            w.set_cursor_grab(CursorGrabMode::Confined).or_else(|_| w.set_cursor_grab(CursorGrabMode::Locked))
        } else {
            w.set_cursor_grab(CursorGrabMode::None)
        };
        res.map_err(|e| util::pg_error(py, &format!("cannot change the input grab: {e}")))?;
    }
    evloop::INPUT.lock().grab = on;
    Ok(())
}

#[pyfunction]
fn get_grab() -> bool {
    evloop::INPUT.lock().grab
}

#[pyfunction]
fn set_mousewheel_buttons(flag: bool) {
    evloop::INPUT.lock().mousewheel_buttons = flag;
}

#[pyfunction]
fn get_mousewheel_buttons() -> bool {
    evloop::INPUT.lock().mousewheel_buttons
}

#[pyfunction]
fn copy_event_queue(py: Python<'_>) -> PyResult<Py<PyList>> {
    let copy: Vec<Py<PyAny>> = STATE.lock().queue.iter().map(|(_, e)| e.clone_ref(py)).collect();
    Ok(PyList::new(py, copy)?.unbind())
}

#[pyfunction]
fn init() {}

#[pyfunction]
fn quit() {
    let mut st = STATE.lock();
    st.queue.clear();
    st.timers.clear();
}

#[pymodule(gil_used = false)]
pub mod renpy_pygame_event {
    use super::*;

    #[pymodule_init]
    fn module_init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        let py = m.py();
        let d = PyDict::new(py);
        for (id, name) in STANDARD_NAMES {
            d.set_item(*id, *name)?;
        }
        NAMES.set(d.clone().unbind()).ok();
        m.add("event_names", d)?;
        let noev = Bound::new(py, Event { typ: 0, dict: { let e = PyDict::new(py); e.set_item("_type", 0)?; e.unbind() } })?;
        NOEVENT.set(noev.clone().into_any().unbind()).ok();
        m.add("NOEVENT_EVENT", noev)?;
        m.add("ACTIVEEVENT", consts::ACTIVEEVENT)?;
        m.add("VIDEORESIZE", consts::VIDEORESIZE)?;
        m.add("VIDEOEXPOSE", consts::VIDEOEXPOSE)?;
        m.add("WINDOWMOVED", consts::WINDOWMOVED)?;
        m.add("USEREVENT_MAX", consts::USEREVENT_MAX)?;
        m.add("Event", m.getattr("EventType")?)?;
        Ok(())
    }

    #[pymodule_export]
    use super::Event as EventType;
    #[pymodule_export]
    use super::{
        clear, copy_event_queue, event_name, get, get_blocked, get_grab, get_mousewheel_buttons,
        get_standard_events, init, peek, poll, post, pump, quit, register, set_allowed, set_blocked,
        set_grab, set_mousewheel_buttons, wait,
    };
}
