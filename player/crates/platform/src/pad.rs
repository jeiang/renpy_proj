//! `renpy.pygame.joystick` and `renpy.pygame.controller` over gilrs.
//!
//! The device list is gilrs's connected pads in connect order. A device index is
//! its position in that list, as in SDL. Each device gets a monotonic instance id.
//! Controller semantics follow SDL2 GameController: button and axis numbering, axis
//! range -32768..32767, triggers 0..32767, up on a stick is negative.
//! Events are produced while the event queue is pumped (`poll`). With no pad attached
//! the count is 0 and no events occur.
//!
//! Gaps: gilrs does not expose the raw button and axis lists of a device, so
//! `Joystick.get_numaxes/get_numbuttons` count the elements seen so far, and the raw
//! indexes of joystick events follow first-seen order. `get_numhats` and `get_numballs`
//! are 0 (gilrs reports hats as axes). The raw joystick indexes are therefore not
//! usable for SDL mapping strings; the calibration screen (`Gamepad.calibrate`) for
//! unknown pads does not produce a correct mapping.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs, GilrsBuilder, MappingSource};
use pyo3::prelude::*;

use crate::event::{self, consts};
use crate::util;

static JOY_INIT: AtomicBool = AtomicBool::new(false);
static PAD_INIT: AtomicBool = AtomicBool::new(false);

const AXES: [&str; 6] = [
    "leftx",
    "lefty",
    "rightx",
    "righty",
    "lefttrigger",
    "righttrigger",
];
const BUTTONS: [&str; 21] = [
    "a",
    "b",
    "x",
    "y",
    "back",
    "guide",
    "start",
    "leftstick",
    "rightstick",
    "leftshoulder",
    "rightshoulder",
    "dpup",
    "dpdown",
    "dpleft",
    "dpright",
    "misc1",
    "paddle1",
    "paddle2",
    "paddle3",
    "paddle4",
    "touchpad",
];

struct Dev {
    id: GamepadId,
    instance: i64,
    name: String,
    uuid: [u8; 16],
    ctl_open: bool,
    joy_open: bool,
    buttons: [bool; 21],
    axes: [i16; 6],
    dpad: [i8; 2],
    raw_buttons: Vec<(u32, bool)>,
    raw_axes: Vec<(u32, f32)>,
}

#[derive(Default)]
struct Pads {
    gilrs: Option<Gilrs>,
    failed: bool,
    mappings: String,
    devs: Vec<Dev>,
    next_instance: i64,
}

thread_local! {
    static PADS: RefCell<Pads> = RefCell::new(Pads::default());
}

/// Events found while polling gilrs, pushed to the Python queue after the borrow ends.
enum Out {
    Added(i64),
    Removed(i64),
    CAxis(i64, i64, i64),
    CButton(i64, i64, bool),
    JAxis(i64, usize, f32),
    JButton(i64, usize, bool),
}

fn to_i16(v: f32) -> i16 {
    let v = v.clamp(-1.0, 1.0);
    if v < 0.0 {
        (v * 32768.0) as i16
    } else {
        (v * 32767.0) as i16
    }
}

fn sdl_button(b: Button) -> Option<usize> {
    Some(match b {
        Button::South => 0,
        Button::East => 1,
        Button::West => 2,
        Button::North => 3,
        Button::Select => 4,
        Button::Mode => 5,
        Button::Start => 6,
        Button::LeftThumb => 7,
        Button::RightThumb => 8,
        Button::LeftTrigger => 9,
        Button::RightTrigger => 10,
        Button::DPadUp => 11,
        Button::DPadDown => 12,
        Button::DPadLeft => 13,
        Button::DPadRight => 14,
        _ => return None,
    })
}

impl Dev {
    fn new(id: GamepadId, instance: i64, name: String, uuid: [u8; 16]) -> Self {
        Dev {
            id,
            instance,
            name,
            uuid,
            ctl_open: false,
            joy_open: false,
            buttons: [false; 21],
            axes: [0; 6],
            dpad: [0; 2],
            raw_buttons: Vec::new(),
            raw_axes: Vec::new(),
        }
    }

    fn set_button(&mut self, idx: usize, down: bool, out: &mut Vec<Out>) {
        if self.buttons[idx] != down {
            self.buttons[idx] = down;
            if self.ctl_open {
                out.push(Out::CButton(self.instance, idx as i64, down));
            }
        }
    }

    fn set_axis(&mut self, idx: usize, value: i16, out: &mut Vec<Out>) {
        if self.axes[idx] != value {
            self.axes[idx] = value;
            if self.ctl_open {
                out.push(Out::CAxis(self.instance, idx as i64, i64::from(value)));
            }
        }
    }

    fn raw_button_index(&mut self, code: u32, down: bool) -> usize {
        match self.raw_buttons.iter().position(|(c, _)| *c == code) {
            Some(i) => {
                self.raw_buttons[i].1 = down;
                i
            }
            None => {
                self.raw_buttons.push((code, down));
                self.raw_buttons.len() - 1
            }
        }
    }

    fn raw_axis_index(&mut self, code: u32, v: f32) -> usize {
        match self.raw_axes.iter().position(|(c, _)| *c == code) {
            Some(i) => {
                self.raw_axes[i].1 = v;
                i
            }
            None => {
                self.raw_axes.push((code, v));
                self.raw_axes.len() - 1
            }
        }
    }
}

impl Pads {
    fn build(&self) -> Result<Gilrs, gilrs::Error> {
        GilrsBuilder::new().add_mappings(&self.mappings).build()
    }

    /// Creates the gilrs context on first use and lists the devices already present.
    fn ensure(&mut self, out: &mut Vec<Out>) {
        if self.gilrs.is_some() || self.failed {
            return;
        }
        match self.build() {
            Ok(g) => {
                self.gilrs = Some(g);
                self.sync_devices(out);
            }
            Err(e) => {
                log::warn!("gamepad support unavailable: {e}");
                self.failed = true;
            }
        }
    }

    /// Makes `devs` match the connected pads of gilrs. Keeps instance ids of devices
    /// that are still present (same uuid and name) and reports the differences.
    fn sync_devices(&mut self, out: &mut Vec<Out>) {
        let Some(g) = self.gilrs.as_ref() else { return };
        let present: Vec<(GamepadId, String, [u8; 16])> = g
            .gamepads()
            .map(|(id, gp)| (id, gp.name().to_string(), gp.uuid()))
            .collect();
        let mut old = std::mem::take(&mut self.devs);
        let mut devs = Vec::new();
        let mut added = Vec::new();
        for (id, name, uuid) in present {
            if let Some(p) = old.iter().position(|d| d.uuid == uuid && d.name == name) {
                let mut d = old.remove(p);
                d.id = id;
                devs.push(d);
            } else {
                self.next_instance += 1;
                added.push(devs.len());
                devs.push(Dev::new(id, self.next_instance, name, uuid));
            }
        }
        for d in old {
            out.push(Out::Removed(d.instance));
        }
        for i in added {
            out.push(Out::Added(i as i64));
        }
        self.devs = devs;
    }

    fn handle(&mut self, ev: gilrs::Event, out: &mut Vec<Out>) {
        match ev.event {
            EventType::Connected => {
                if self.devs.iter().any(|d| d.id == ev.id) {
                    return;
                }
                let Some(g) = self.gilrs.as_ref() else { return };
                let gp = g.gamepad(ev.id);
                let (name, uuid) = (gp.name().to_string(), gp.uuid());
                self.next_instance += 1;
                self.devs
                    .push(Dev::new(ev.id, self.next_instance, name, uuid));
                out.push(Out::Added(self.devs.len() as i64 - 1));
            }
            EventType::Disconnected => {
                if let Some(p) = self.devs.iter().position(|d| d.id == ev.id) {
                    let d = self.devs.remove(p);
                    out.push(Out::Removed(d.instance));
                }
            }
            EventType::ButtonPressed(b, code) | EventType::ButtonReleased(b, code) => {
                let down = matches!(ev.event, EventType::ButtonPressed(..));
                let Some(d) = self.devs.iter_mut().find(|d| d.id == ev.id) else {
                    return;
                };
                if d.joy_open {
                    let i = d.raw_button_index(code.into_u32(), down);
                    out.push(Out::JButton(d.instance, i, down));
                } else {
                    d.raw_button_index(code.into_u32(), down);
                }
                if let Some(idx) = sdl_button(b) {
                    d.set_button(idx, down, out);
                }
            }
            EventType::ButtonChanged(b, v, _) => {
                let Some(d) = self.devs.iter_mut().find(|d| d.id == ev.id) else {
                    return;
                };
                match b {
                    Button::LeftTrigger2 => d.set_axis(4, to_i16(v.max(0.0)), out),
                    Button::RightTrigger2 => d.set_axis(5, to_i16(v.max(0.0)), out),
                    _ => {}
                }
            }
            EventType::AxisChanged(a, v, code) => {
                let Some(d) = self.devs.iter_mut().find(|d| d.id == ev.id) else {
                    return;
                };
                let i = d.raw_axis_index(code.into_u32(), v);
                if d.joy_open {
                    out.push(Out::JAxis(d.instance, i, v));
                }
                match a {
                    Axis::LeftStickX => d.set_axis(0, to_i16(v), out),
                    Axis::LeftStickY => d.set_axis(1, to_i16(-v), out),
                    Axis::RightStickX => d.set_axis(2, to_i16(v), out),
                    Axis::RightStickY => d.set_axis(3, to_i16(-v), out),
                    Axis::DPadX => {
                        let s = if v > 0.5 {
                            1
                        } else if v < -0.5 {
                            -1
                        } else {
                            0
                        };
                        let old = std::mem::replace(&mut d.dpad[0], s);
                        if old != s {
                            d.set_button(13, s < 0, out);
                            d.set_button(14, s > 0, out);
                        }
                    }
                    Axis::DPadY => {
                        // gilrs reports up as positive.
                        let s = if v > 0.5 {
                            1
                        } else if v < -0.5 {
                            -1
                        } else {
                            0
                        };
                        let old = std::mem::replace(&mut d.dpad[1], s);
                        if old != s {
                            d.set_button(11, s > 0, out);
                            d.set_button(12, s < 0, out);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn pads_used() -> bool {
    PAD_INIT.load(Ordering::Relaxed) || JOY_INIT.load(Ordering::Relaxed)
}

fn flush(py: Python<'_>, out: Vec<Out>) -> PyResult<()> {
    for o in out {
        match o {
            Out::Added(index) => {
                event::push_native(py, consts::CONTROLLERDEVICEADDED, |d| {
                    d.set_item("which", index)
                })?;
                event::push_native(py, consts::JOYDEVICEADDED, |d| d.set_item("which", index))?;
            }
            Out::Removed(inst) => {
                event::push_native(py, consts::CONTROLLERDEVICEREMOVED, |d| {
                    d.set_item("which", inst)
                })?;
                event::push_native(py, consts::JOYDEVICEREMOVED, |d| d.set_item("which", inst))?;
            }
            Out::CAxis(which, axis, value) => {
                event::push_native(py, consts::CONTROLLERAXISMOTION, |d| {
                    d.set_item("which", which)?;
                    d.set_item("axis", axis)?;
                    d.set_item("value", value)
                })?;
            }
            Out::CButton(which, button, down) => {
                let t = if down {
                    consts::CONTROLLERBUTTONDOWN
                } else {
                    consts::CONTROLLERBUTTONUP
                };
                event::push_native(py, t, |d| {
                    d.set_item("which", which)?;
                    d.set_item("button", button)?;
                    d.set_item("state", i64::from(down))
                })?;
            }
            Out::JAxis(which, axis, value) => {
                event::push_native(py, consts::JOYAXISMOTION, |d| {
                    d.set_item("which", which)?;
                    d.set_item("joy", which)?;
                    d.set_item("axis", axis)?;
                    d.set_item("value", f64::from(value))
                })?;
            }
            Out::JButton(which, button, down) => {
                let t = if down {
                    consts::JOYBUTTONDOWN
                } else {
                    consts::JOYBUTTONUP
                };
                event::push_native(py, t, |d| {
                    d.set_item("which", which)?;
                    d.set_item("joy", which)?;
                    d.set_item("button", button)?;
                    d.set_item("state", i64::from(down))
                })?;
            }
        }
    }
    Ok(())
}

/// Reads gilrs events into the pygame event queue. Called by the event pump.
pub fn poll(py: Python<'_>) -> PyResult<()> {
    if !pads_used() {
        return Ok(());
    }
    let mut out = Vec::new();
    PADS.with_borrow_mut(|p| {
        p.ensure(&mut out);
        while let Some(ev) = p.gilrs.as_mut().and_then(|g| g.next_event()) {
            p.handle(ev, &mut out);
        }
    });
    flush(py, out)
}

/// Runs `f` on the pads, creating the gilrs context first. Events that creation finds
/// are queued.
fn with_pads<R>(py: Python<'_>, f: impl FnOnce(&mut Pads) -> R) -> PyResult<R> {
    let mut out = Vec::new();
    let r = PADS.with_borrow_mut(|p| {
        p.ensure(&mut out);
        f(p)
    });
    flush(py, out)?;
    Ok(r)
}

fn count(py: Python<'_>) -> PyResult<i64> {
    if !pads_used() {
        return Ok(0);
    }
    with_pads(py, |p| p.devs.len() as i64)
}

fn is_mapped(p: &Pads, d: &Dev) -> bool {
    p.gilrs
        .as_ref()
        .and_then(|g| g.connected_gamepad(d.id))
        .is_some_and(|gp| gp.mapping_source() != MappingSource::None)
}

fn dev_at(p: &mut Pads, index: i64) -> Option<&mut Dev> {
    usize::try_from(index).ok().and_then(|i| p.devs.get_mut(i))
}

fn guid_string(uuid: &[u8; 16]) -> String {
    uuid.iter().map(|b| format!("{b:02x}")).collect()
}

fn no_device(py: Python<'_>, index: i64) -> PyErr {
    util::pg_error(py, &format!("Invalid joystick device number {index}"))
}

fn not_init(py: Python<'_>) -> PyErr {
    util::pg_error(py, "joystick not initialized")
}

pub mod joystick {
    use super::*;

    #[pyfunction]
    fn init() {
        JOY_INIT.store(true, Ordering::Relaxed);
    }

    #[pyfunction]
    fn quit() {
        JOY_INIT.store(false, Ordering::Relaxed);
    }

    #[pyfunction]
    fn get_init() -> bool {
        JOY_INIT.load(Ordering::Relaxed)
    }

    #[pyfunction]
    fn get_count(py: Python<'_>) -> PyResult<i64> {
        count(py)
    }

    /// A joystick handle. Queries fail until `init` opens it.
    #[pyclass(module = "renpy.pygame.joystick", weakref)]
    pub struct Joystick {
        joyid: i64,
        instance: Option<i64>,
    }

    impl Joystick {
        fn with_dev<R>(&self, py: Python<'_>, f: impl FnOnce(&Dev) -> R) -> PyResult<R> {
            let inst = self.instance.ok_or_else(|| not_init(py))?;
            with_pads(py, |p| p.devs.iter().find(|d| d.instance == inst).map(f))?
                .ok_or_else(|| util::pg_error(py, "Joystick was disconnected"))
        }
    }

    #[pymethods]
    impl Joystick {
        #[new]
        fn new(id: i64) -> Self {
            Self {
                joyid: id,
                instance: None,
            }
        }

        fn init(&mut self, py: Python<'_>) -> PyResult<()> {
            let id = self.joyid;
            let inst = with_pads(py, |p| {
                dev_at(p, id).map(|d| {
                    d.joy_open = true;
                    d.instance
                })
            })?
            .ok_or_else(|| no_device(py, id))?;
            self.instance = Some(inst);
            Ok(())
        }

        fn quit(&mut self, py: Python<'_>) -> PyResult<()> {
            if let Some(inst) = self.instance.take() {
                with_pads(py, |p| {
                    if let Some(d) = p.devs.iter_mut().find(|d| d.instance == inst) {
                        d.joy_open = false;
                    }
                })?;
            }
            Ok(())
        }

        fn get_init(&self) -> bool {
            self.instance.is_some()
        }

        fn get_id(&self, py: Python<'_>) -> PyResult<i64> {
            self.instance.ok_or_else(|| not_init(py))
        }

        fn get_name(&self, py: Python<'_>) -> PyResult<Option<String>> {
            self.with_dev(py, |d| Some(d.name.clone()))
        }

        fn get_numaxes(&self, py: Python<'_>) -> PyResult<i64> {
            self.with_dev(py, |d| d.raw_axes.len() as i64)
        }

        fn get_numballs(&self, py: Python<'_>) -> PyResult<i64> {
            self.with_dev(py, |_| 0)
        }

        fn get_numbuttons(&self, py: Python<'_>) -> PyResult<i64> {
            self.with_dev(py, |d| d.raw_buttons.len() as i64)
        }

        fn get_numhats(&self, py: Python<'_>) -> PyResult<i64> {
            self.with_dev(py, |_| 0)
        }

        fn get_axis(&self, py: Python<'_>, axis_number: i64) -> PyResult<f64> {
            self.with_dev(py, |d| {
                usize::try_from(axis_number)
                    .ok()
                    .and_then(|i| d.raw_axes.get(i))
                    .map_or(0.0, |(_, v)| f64::from(*v))
            })
        }

        fn get_ball(&self, py: Python<'_>, ball_number: i64) -> PyResult<(i64, i64)> {
            self.with_dev(py, |_| ())?;
            Err(util::pg_error(
                py,
                &format!("Joystick ball index {ball_number} out of range"),
            ))
        }

        fn get_button(&self, py: Python<'_>, button: i64) -> PyResult<bool> {
            self.with_dev(py, |d| {
                usize::try_from(button)
                    .ok()
                    .and_then(|i| d.raw_buttons.get(i))
                    .is_some_and(|(_, down)| *down)
            })
        }

        fn get_hat(&self, py: Python<'_>, hat_number: i64) -> PyResult<(i64, i64)> {
            self.with_dev(py, |_| ())?;
            Err(util::pg_error(
                py,
                &format!("Joystick hat index {hat_number} out of range"),
            ))
        }
    }

    #[pymodule(gil_used = false)]
    pub mod renpy_pygame_joystick {
        use crate::util;
        use pyo3::prelude::*;

        #[pymodule_export]
        use super::{Joystick, get_count, get_init, init, quit};

        #[pymodule_init]
        fn module_init(m: &Bound<'_, PyModule>) -> PyResult<()> {
            util::register_hook(m, "register_init", "init")?;
            util::register_hook(m, "register_quit", "quit")
        }
    }
}

pub mod controller {
    use super::*;

    #[pyfunction]
    fn init() {
        PAD_INIT.store(true, Ordering::Relaxed);
    }

    #[pyfunction]
    fn quit() {
        PAD_INIT.store(false, Ordering::Relaxed);
    }

    #[pyfunction]
    fn get_init() -> bool {
        PAD_INIT.load(Ordering::Relaxed)
    }

    #[pyfunction]
    fn get_count(py: Python<'_>) -> PyResult<i64> {
        count(py)
    }

    /// Adds SDL mapping lines. gilrs takes mappings only when its context is built, so
    /// the context is rebuilt; devices that stay keep their instance ids.
    fn add_text(py: Python<'_>, text: &str) -> PyResult<()> {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.split(',').count() < 3 || line.split(',').next().is_none_or(|g| g.len() != 32) {
                return Err(util::pg_error(
                    py,
                    &format!("Invalid controller mapping: {line}"),
                ));
            }
        }
        let mut out = Vec::new();
        PADS.with_borrow_mut(|p| {
            p.mappings.push_str(text);
            if !text.ends_with('\n') {
                p.mappings.push('\n');
            }
            if p.gilrs.is_some() {
                match p.build() {
                    Ok(g) => {
                        p.gilrs = Some(g);
                        p.sync_devices(&mut out);
                    }
                    Err(e) => log::warn!("gamepad mappings could not be applied: {e}"),
                }
            }
        });
        flush(py, out)
    }

    #[pyfunction]
    fn add_mapping(py: Python<'_>, mapping: &Bound<'_, PyAny>) -> PyResult<()> {
        add_text(py, &text_of(mapping)?)
    }

    fn text_of(o: &Bound<'_, PyAny>) -> PyResult<String> {
        if let Ok(b) = o.cast::<pyo3::types::PyBytes>() {
            return Ok(String::from_utf8_lossy(b.as_bytes()).into_owned());
        }
        o.extract()
    }

    /// `mapping_file` is a path or a file-like object opened for reading.
    #[pyfunction]
    fn add_mappings(py: Python<'_>, mapping_file: &Bound<'_, PyAny>) -> PyResult<()> {
        let text = if mapping_file.hasattr("read")? {
            text_of(&mapping_file.call_method0("read")?)?
        } else {
            let path = text_of(mapping_file)?;
            String::from_utf8_lossy(
                &std::fs::read(&path)
                    .map_err(|e| util::pg_error(py, &format!("Could not read {path}: {e}")))?,
            )
            .into_owned()
        };
        add_text(py, &text)
    }

    fn name_bytes(name: &Bound<'_, PyAny>) -> PyResult<String> {
        text_of(name)
    }

    #[pyfunction]
    fn get_axis_from_string(name: &Bound<'_, PyAny>) -> PyResult<i64> {
        let n = name_bytes(name)?;
        Ok(AXES.iter().position(|a| *a == n).map_or(-1, |i| i as i64))
    }

    #[pyfunction]
    fn get_button_from_string(name: &Bound<'_, PyAny>) -> PyResult<i64> {
        let n = name_bytes(name)?;
        Ok(BUTTONS
            .iter()
            .position(|a| *a == n)
            .map_or(-1, |i| i as i64))
    }

    #[pyfunction]
    fn get_string_for_axis(axis: i64) -> Option<&'static str> {
        usize::try_from(axis)
            .ok()
            .and_then(|i| AXES.get(i).copied())
    }

    #[pyfunction]
    fn get_string_for_button(button: i64) -> Option<&'static str> {
        usize::try_from(button)
            .ok()
            .and_then(|i| BUTTONS.get(i).copied())
    }

    fn pad_not_init(py: Python<'_>) -> PyErr {
        util::pg_error(py, "controller not initialized.")
    }

    /// A game controller handle.
    #[pyclass(module = "renpy.pygame.controller", weakref)]
    pub struct Controller {
        index: i64,
        #[pyo3(get, set)]
        instance_id: i64,
        open: bool,
    }

    impl Controller {
        /// Runs `f` on the device at this index, whether opened or not.
        fn at_index<R>(
            &self,
            py: Python<'_>,
            f: impl FnOnce(&mut Pads, usize) -> R,
        ) -> PyResult<Option<R>> {
            let idx = usize::try_from(self.index).ok();
            with_pads(py, |p| idx.filter(|i| *i < p.devs.len()).map(|i| f(p, i)))
        }
    }

    #[pymethods]
    impl Controller {
        #[new]
        fn new(index: i64) -> Self {
            Self {
                index,
                instance_id: 0,
                open: false,
            }
        }

        fn init(&mut self, py: Python<'_>) -> PyResult<()> {
            let inst = self
                .at_index(py, |p, i| {
                    p.devs[i].ctl_open = true;
                    p.devs[i].instance
                })?
                .ok_or_else(|| no_device(py, self.index))?;
            self.instance_id = inst;
            self.open = true;
            Ok(())
        }

        fn quit(&mut self, py: Python<'_>) -> PyResult<()> {
            if self.open {
                let inst = self.instance_id;
                with_pads(py, |p| {
                    if let Some(d) = p.devs.iter_mut().find(|d| d.instance == inst) {
                        d.ctl_open = false;
                    }
                })?;
                self.open = false;
            }
            Ok(())
        }

        fn get_init(&self) -> bool {
            self.open
        }

        fn get_axis(&self, py: Python<'_>, axis: i64) -> PyResult<i64> {
            if !self.open {
                return Err(pad_not_init(py));
            }
            let inst = self.instance_id;
            let v = with_pads(py, |p| {
                p.devs.iter().find(|d| d.instance == inst).map(|d| {
                    usize::try_from(axis)
                        .ok()
                        .and_then(|i| d.axes.get(i))
                        .map_or(0, |v| i64::from(*v))
                })
            })?;
            Ok(v.unwrap_or(0))
        }

        fn get_button(&self, py: Python<'_>, button: i64) -> PyResult<i64> {
            if !self.open {
                return Err(pad_not_init(py));
            }
            let inst = self.instance_id;
            let v = with_pads(py, |p| {
                p.devs.iter().find(|d| d.instance == inst).map(|d| {
                    usize::try_from(button)
                        .ok()
                        .and_then(|i| d.buttons.get(i))
                        .map_or(0, |v| i64::from(*v))
                })
            })?;
            Ok(v.unwrap_or(0))
        }

        fn get_name(&self, py: Python<'_>) -> PyResult<Option<String>> {
            self.at_index(py, |p, i| p.devs[i].name.clone())
        }

        fn is_controller(&self, py: Python<'_>) -> PyResult<bool> {
            Ok(self
                .at_index(py, |p, i| is_mapped(p, &p.devs[i]))?
                .unwrap_or(false))
        }

        fn get_guid_string(&self, py: Python<'_>) -> PyResult<String> {
            self.at_index(py, |p, i| guid_string(&p.devs[i].uuid))?
                .ok_or_else(|| no_device(py, self.index))
        }
    }

    #[pymodule(gil_used = false)]
    pub mod renpy_pygame_controller {
        use crate::util;
        use pyo3::prelude::*;

        #[pymodule_export]
        use super::{
            Controller, add_mapping, add_mappings, get_axis_from_string, get_button_from_string,
            get_count, get_init, get_string_for_axis, get_string_for_button, init, quit,
        };

        #[pymodule_init]
        fn module_init(m: &Bound<'_, PyModule>) -> PyResult<()> {
            util::register_hook(m, "register_init", "init")?;
            util::register_hook(m, "register_quit", "quit")
        }
    }
}
