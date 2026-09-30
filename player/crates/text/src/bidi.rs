//! `renpy.text.bidi`: the two functions Ren'Py takes from GNU fribidi, on top of `unicode-bidi`.
//!
//! The direction constants have fribidi's values, because Ren'Py stores them in style data:
//! `LTR` and `RTL` are strong, `WLTR` and `WRTL` are weak (used when no strong character decides),
//! `ON` means "detect".

use pyo3::prelude::*;
use pyo3::types::PyTuple;
use unicode_bidi::{BidiClass, BidiInfo, Level, bidi_class};

pub const LTR: i32 = 0x110;
pub const RTL: i32 = 0x111;
pub const ON: i32 = 0x40;
pub const WLTR: i32 = 0x20;
pub const WRTL: i32 = 0x21;

/// Resolves the paragraph direction the way `fribidi_get_par_embedding_levels` does. For `LTR` and
/// `RTL` the direction is fixed. For `ON`, `WLTR` and `WRTL` the first strong character outside an
/// isolate decides. When there is none, `ON` becomes `WLTR` and the weak directions stay as given.
fn resolve_direction(classes: &[BidiClass], direction: i32) -> (u8, i32) {
    match direction {
        LTR => return (0, LTR),
        RTL => return (1, RTL),
        _ => {}
    }
    let mut depth = 0usize;
    for &c in classes {
        match c {
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => depth += 1,
            BidiClass::PDI => depth = depth.saturating_sub(1),
            BidiClass::L if depth == 0 => return (0, LTR),
            BidiClass::R | BidiClass::AL if depth == 0 => return (1, RTL),
            _ => {}
        }
    }
    match direction {
        WRTL => (1, WRTL),
        WLTR => (0, WLTR),
        _ => (0, WLTR),
    }
}

struct Resolved {
    chars: Vec<char>,
    classes: Vec<BidiClass>,
    levels: Vec<u8>,
    base_level: u8,
    direction: i32,
}

/// Python strings can hold lone surrogates. They have no bidi class, so they become U+FFFD here.
pub fn chars_of(s: &Bound<'_, PyAny>) -> PyResult<Vec<char>> {
    let s = s.cast::<pyo3::types::PyString>()?;
    match s.to_str() {
        Ok(t) => Ok(t.chars().collect()),
        Err(_) => {
            let raw = s.call_method1("encode", ("utf-32-le", "surrogatepass"))?;
            let bytes = raw.cast::<pyo3::types::PyBytes>()?.as_bytes();
            Ok(bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| {
                    char::from_u32(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .unwrap_or('\u{fffd}')
                })
                .collect())
        }
    }
}

fn resolve(chars: Vec<char>, direction: i32) -> Resolved {
    let classes: Vec<BidiClass> = chars.iter().map(|&c| bidi_class(c)).collect();
    let (base_level, direction) = resolve_direction(&classes, direction);
    // fribidi as Ren'Py ships it has no paired-bracket rule (N0). `unicode-bidi` applies it, so the
    // brackets are swapped for a plain neutral of the same class while the levels are computed.
    let text: String = chars
        .iter()
        .map(|&c| {
            if bidi_class(c) == BidiClass::ON && unicode_bidi_mirroring::is_mirroring(c) {
                '!'
            } else {
                c
            }
        })
        .collect();
    let mut levels = Vec::with_capacity(chars.len());
    if !chars.is_empty() {
        let info = BidiInfo::new(&text, Level::new(base_level).ok());
        for (byte_index, _) in text.char_indices() {
            levels.push(info.levels[byte_index].number());
        }
    }
    Resolved {
        chars,
        classes,
        levels,
        base_level,
        direction,
    }
}

/// `renpy.text.bidi.get_embedding_levels(s, direction=ON) -> (levels, direction)`.
#[pyfunction]
#[pyo3(signature = (s, direction=ON))]
pub fn get_embedding_levels<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    direction: i32,
) -> PyResult<(Bound<'py, PyTuple>, i32)> {
    let r = resolve(chars_of(s)?, direction);
    let levels = PyTuple::new(py, r.levels.iter().map(|&l| l as i32))?;
    Ok((levels, r.direction))
}

/// Rules L1 to L3 with fribidi's default flags (mirroring on, non-spacing marks reordered, no Arabic
/// shaping, nothing removed): the visual string has the same length as the logical one.
fn visual(r: Resolved) -> String {
    let n = r.chars.len();
    let mut levels = r.levels;
    let classes = r.classes;

    // L1: separators, and whitespace or isolate controls before them or at the end of the line,
    // go back to the paragraph level.
    let mut trailing = true;
    for i in (0..n).rev() {
        match classes[i] {
            BidiClass::B | BidiClass::S => {
                levels[i] = r.base_level;
                trailing = true;
            }
            BidiClass::WS
            | BidiClass::FSI
            | BidiClass::LRI
            | BidiClass::RLI
            | BidiClass::PDI
            | BidiClass::BN
            | BidiClass::LRE
            | BidiClass::RLE
            | BidiClass::LRO
            | BidiClass::RLO
            | BidiClass::PDF => {
                if trailing {
                    levels[i] = r.base_level;
                }
            }
            _ => trailing = false,
        }
    }

    // L2: reverse every run at or above each level, from the highest down to the lowest odd one.
    let mut order: Vec<usize> = (0..n).collect();
    if let (Some(&max), Some(min_odd)) = (
        levels.iter().max(),
        levels.iter().copied().filter(|l| l % 2 == 1).min(),
    ) {
        for level in (min_odd..=max).rev() {
            let mut i = 0;
            while i < n {
                if levels[order[i]] >= level {
                    let start = i;
                    while i < n && levels[order[i]] >= level {
                        i += 1;
                    }
                    order[start..i].reverse();
                } else {
                    i += 1;
                }
            }
        }
    }

    // L3: a mark that follows its base in logical order follows it in visual order too, so the
    // reversed "marks, base" groups of right-to-left runs are turned around again.
    let mut i = 0;
    while i < n {
        let idx = order[i];
        if classes[idx] == BidiClass::NSM && levels[idx] % 2 == 1 {
            let start = i;
            while i < n && classes[order[i]] == BidiClass::NSM && levels[order[i]] % 2 == 1 {
                i += 1;
            }
            if i < n && levels[order[i]] % 2 == 1 {
                order[start..=i].reverse();
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    order
        .into_iter()
        .map(|idx| {
            let c = r.chars[idx];
            if levels[idx] % 2 == 1 {
                unicode_bidi_mirroring::get_mirrored(c).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// `renpy.text.bidi.log2vis(s, direction=ON) -> (visual string, direction)`.
#[pyfunction]
#[pyo3(signature = (s, direction=ON))]
pub fn log2vis(s: &Bound<'_, PyAny>, direction: i32) -> PyResult<(String, i32)> {
    let r = resolve(chars_of(s)?, direction);
    let direction = r.direction;
    Ok((visual(r), direction))
}
