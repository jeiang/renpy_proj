//! `renpy.pygame.joystick` and `renpy.pygame.controller`.
//!
//! M1 gap: the player reports zero joysticks and zero controllers and never
//! produces joystick or controller events. Opening a device raises
//! `pygame.error`, as SDL does for an index without a device.

use std::sync::atomic::{AtomicBool, Ordering};

use pyo3::prelude::*;

use crate::util;

static JOY_INIT: AtomicBool = AtomicBool::new(false);
static PAD_INIT: AtomicBool = AtomicBool::new(false);

fn no_device(py: Python<'_>, index: i64) -> PyErr {
    util::pg_error(py, &format!("Invalid joystick device number {index}: this player has no joystick or controller support yet."))
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
    fn get_count() -> i64 {
        0
    }

    /// A joystick handle. No device can be opened, so every query after `init` fails.
    #[pyclass(module = "renpy.pygame.joystick", weakref)]
    pub struct Joystick {
        joyid: i64,
    }

    #[pymethods]
    impl Joystick {
        #[new]
        fn new(id: i64) -> Self {
            Self { joyid: id }
        }

        fn init(&self, py: Python<'_>) -> PyResult<()> {
            Err(no_device(py, self.joyid))
        }

        fn quit(&self) {}

        fn get_init(&self) -> bool {
            false
        }

        fn get_id(&self, py: Python<'_>) -> PyResult<i64> {
            Err(not_init(py))
        }

        fn get_name(&self, py: Python<'_>) -> PyResult<Option<String>> {
            Err(not_init(py))
        }

        fn get_numaxes(&self, py: Python<'_>) -> PyResult<i64> {
            Err(not_init(py))
        }

        fn get_numballs(&self, py: Python<'_>) -> PyResult<i64> {
            Err(not_init(py))
        }

        fn get_numbuttons(&self, py: Python<'_>) -> PyResult<i64> {
            Err(not_init(py))
        }

        fn get_numhats(&self, py: Python<'_>) -> PyResult<i64> {
            Err(not_init(py))
        }

        fn get_axis(&self, py: Python<'_>, axis_number: i64) -> PyResult<f64> {
            let _ = axis_number;
            Err(not_init(py))
        }

        fn get_ball(&self, py: Python<'_>, ball_number: i64) -> PyResult<(i64, i64)> {
            let _ = ball_number;
            Err(not_init(py))
        }

        fn get_button(&self, py: Python<'_>, button: i64) -> PyResult<bool> {
            let _ = button;
            Err(not_init(py))
        }

        fn get_hat(&self, py: Python<'_>, hat_number: i64) -> PyResult<(i64, i64)> {
            let _ = hat_number;
            Err(not_init(py))
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

    const AXES: [&str; 6] = ["leftx", "lefty", "rightx", "righty", "lefttrigger", "righttrigger"];
    const BUTTONS: [&str; 21] = [
        "a", "b", "x", "y", "back", "guide", "start", "leftstick", "rightstick", "leftshoulder",
        "rightshoulder", "dpup", "dpdown", "dpleft", "dpright", "misc1", "paddle1", "paddle2",
        "paddle3", "paddle4", "touchpad",
    ];

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
    fn get_count() -> i64 {
        0
    }

    /// With zero devices there is nothing to map. The mapping is accepted and dropped.
    #[pyfunction]
    fn add_mapping(mapping: Py<PyAny>) {
        let _ = mapping;
    }

    /// With zero devices there is nothing to map. The file is accepted and not read.
    #[pyfunction]
    fn add_mappings(mapping_file: Py<PyAny>) {
        let _ = mapping_file;
    }

    fn name_bytes(name: &Bound<'_, PyAny>) -> PyResult<String> {
        if let Ok(b) = name.cast::<pyo3::types::PyBytes>() {
            return Ok(String::from_utf8_lossy(b.as_bytes()).into_owned());
        }
        name.extract()
    }

    #[pyfunction]
    fn get_axis_from_string(name: &Bound<'_, PyAny>) -> PyResult<i64> {
        let n = name_bytes(name)?;
        Ok(AXES.iter().position(|a| *a == n).map_or(-1, |i| i as i64))
    }

    #[pyfunction]
    fn get_button_from_string(name: &Bound<'_, PyAny>) -> PyResult<i64> {
        let n = name_bytes(name)?;
        Ok(BUTTONS.iter().position(|a| *a == n).map_or(-1, |i| i as i64))
    }

    #[pyfunction]
    fn get_string_for_axis(axis: i64) -> Option<&'static str> {
        usize::try_from(axis).ok().and_then(|i| AXES.get(i).copied())
    }

    #[pyfunction]
    fn get_string_for_button(button: i64) -> Option<&'static str> {
        usize::try_from(button).ok().and_then(|i| BUTTONS.get(i).copied())
    }

    /// A game controller handle. No device can be opened.
    #[pyclass(module = "renpy.pygame.controller", weakref)]
    pub struct Controller {
        index: i64,
        #[pyo3(get, set)]
        instance_id: i64,
    }

    fn pad_not_init(py: Python<'_>) -> PyErr {
        util::pg_error(py, "controller not initialized.")
    }

    #[pymethods]
    impl Controller {
        #[new]
        fn new(index: i64) -> Self {
            Self { index, instance_id: 0 }
        }

        fn init(&self, py: Python<'_>) -> PyResult<()> {
            Err(no_device(py, self.index))
        }

        fn quit(&self) {}

        fn get_init(&self) -> bool {
            false
        }

        fn get_axis(&self, py: Python<'_>, axis: i64) -> PyResult<i64> {
            let _ = axis;
            Err(pad_not_init(py))
        }

        fn get_button(&self, py: Python<'_>, button: i64) -> PyResult<i64> {
            let _ = button;
            Err(pad_not_init(py))
        }

        fn get_name(&self) -> Option<String> {
            None
        }

        fn is_controller(&self) -> bool {
            false
        }

        fn get_guid_string(&self, py: Python<'_>) -> PyResult<String> {
            Err(no_device(py, self.index))
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
