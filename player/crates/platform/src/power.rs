//! `renpy.pygame.power`: battery state. The player does not query the OS:
//! it reports `POWERSTATE_UNKNOWN` with unknown seconds and percent, as SDL does
//! when it cannot read the power source.

use pyo3::prelude::*;

const POWERSTATE_UNKNOWN: i64 = 0;

#[pyclass(module = "renpy.pygame.power", get_all)]
struct PowerInfo {
    state: i64,
    seconds: i64,
    percent: i64,
}

#[pymethods]
impl PowerInfo {
    fn __repr__(&self) -> String {
        let name = match self.state {
            0 => "POWERSTATE_UNKNOWN",
            1 => "POWERSTATE_ON_BATTERY",
            2 => "POWERSTATE_NO_BATTERY",
            3 => "POWERSTATE_CHARGING",
            4 => "POWERSTATE_CHARGED",
            _ => "INVALID",
        };
        format!("<PowerInfo state={name} seconds={} percent={}>", self.seconds, self.percent)
    }
}

#[pyfunction]
fn get_power_info() -> PowerInfo {
    PowerInfo { state: POWERSTATE_UNKNOWN, seconds: -1, percent: -1 }
}

#[pymodule(gil_used = false)]
pub mod renpy_pygame_power {
    #[pymodule_export]
    use super::{PowerInfo, get_power_info};
}
