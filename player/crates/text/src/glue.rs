//! What `ftfont` and `hbfont` share: `textsupport.Glyph` access, Ren'Py settings, and the pixel
//! writer that `draw` uses on a `surface::Surface`.

use pyo3::exceptions::PyValueError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyDict;

use crate::raster::Bitmap;

/// `textsupport.SPLIT_INSTEAD`.
const SPLIT_INSTEAD: i32 = 2;

static GLYPH_CLASS: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

/// A new, empty `renpy.text.textsupport.Glyph`.
pub fn new_glyph<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    let class = GLYPH_CLASS.get_or_try_init(py, || {
        Ok::<_, PyErr>(
            py.import("renpy.text.textsupport")?
                .getattr("Glyph")?
                .unbind(),
        )
    })?;
    class.bind(py).call0()
}

/// The fields of a `Glyph` that `bounds` and `draw` read.
pub struct GlyphIn {
    pub x: i32,
    pub y: i32,
    pub delta_x_adjustment: i32,
    pub character: u32,
    pub variation: u32,
    pub glyph: u32,
    pub split_instead: bool,
    pub x_offset: f32,
    pub y_offset: f32,
    pub advance: f32,
    pub width: f32,
    pub line_spacing: i32,
    pub draw: bool,
}

pub fn read_glyph(g: &Bound<'_, PyAny>) -> PyResult<GlyphIn> {
    Ok(GlyphIn {
        x: g.getattr(intern!(g.py(), "x"))?.extract()?,
        y: g.getattr(intern!(g.py(), "y"))?.extract()?,
        delta_x_adjustment: g
            .getattr(intern!(g.py(), "delta_x_adjustment"))?
            .extract()?,
        character: g.getattr(intern!(g.py(), "character"))?.extract()?,
        variation: g.getattr(intern!(g.py(), "variation"))?.extract()?,
        glyph: g.getattr(intern!(g.py(), "glyph"))?.extract()?,
        split_instead: g.getattr(intern!(g.py(), "split"))?.extract::<i32>()? == SPLIT_INSTEAD,
        x_offset: g.getattr(intern!(g.py(), "x_offset"))?.extract()?,
        y_offset: g.getattr(intern!(g.py(), "y_offset"))?.extract()?,
        advance: g.getattr(intern!(g.py(), "advance"))?.extract()?,
        width: g.getattr(intern!(g.py(), "width"))?.extract()?,
        line_spacing: g.getattr(intern!(g.py(), "line_spacing"))?.extract()?,
        draw: g.getattr(intern!(g.py(), "draw"))?.extract()?,
    })
}

/// Writes the four `add_*` fields `bounds` sets.
pub fn set_add(g: &Bound<'_, PyAny>, left: i32, top: i32, right: i32, bottom: i32) -> PyResult<()> {
    let py = g.py();
    g.setattr(intern!(py, "add_left"), left)?;
    g.setattr(intern!(py, "add_right"), right)?;
    g.setattr(intern!(py, "add_top"), top)?;
    g.setattr(intern!(py, "add_bottom"), bottom)
}

fn renpy_attr<'py>(py: Python<'py>, path: &[&str]) -> PyResult<Bound<'py, PyAny>> {
    let mut obj = py.import("renpy")?.into_any();
    for name in path {
        obj = obj.getattr(*name)?;
    }
    Ok(obj)
}

/// `renpy.config.<dict>.get(fn, 1.0)`.
pub fn config_scale(py: Python<'_>, name: &str, face_fn: &Bound<'_, PyAny>) -> PyResult<f32> {
    let d = renpy_attr(py, &["config", name])?;
    d.call_method1("get", (face_fn, 1.0f64))?.extract()
}

pub fn pref_font_size(py: Python<'_>) -> PyResult<f32> {
    renpy_attr(py, &["game", "preferences", "font_size"])?.extract()
}

pub fn pref_line_spacing(py: Python<'_>) -> PyResult<f64> {
    renpy_attr(py, &["game", "preferences", "font_line_spacing"])?.extract()
}

/// The pixels of a RGBA `Surface`.
pub struct Target {
    ptr: *mut u8,
    pub pitch: usize,
    pub w: i32,
    pub h: i32,
}

impl Target {
    pub fn of<'py>(
        surface: &Bound<'py, PyAny>,
    ) -> PyResult<(Target, Bound<'py, surface::Surface>)> {
        let s = surface.cast::<surface::Surface>()?.clone();
        let img = s.get().img();
        if !img.fmt.rgba || !s.get().has_alpha_channel() {
            return Err(PyValueError::new_err(
                "text can only be drawn to a surface with the RGBA byte layout and an alpha channel",
            ));
        }
        let t = Target {
            ptr: img.pixels_ptr(),
            pitch: img.pitch,
            w: img.w as i32,
            h: img.h as i32,
        };
        Ok((t, s))
    }

    #[inline]
    fn px(&self, x: i32, y: i32) -> Option<*mut u8> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        // SAFETY: the pixel is inside the surface, whose allocation `Surface::img` vouches for and
        // the caller keeps alive.
        Some(unsafe { self.ptr.add(y as usize * self.pitch + x as usize * 4) })
    }

    /// Draws a coverage bitmap with its top-left at (`bmx`, `bmy`). A pixel is written only when it
    /// raises the alpha there, so overlapping glyphs do not darken each other.
    pub fn blit(&self, b: &Bitmap, bmx: i32, bmy: i32, color: [u32; 4]) {
        let [sr, sg, sb, sa] = color;
        for py in 0..b.rows as i32 {
            let row = &b.data[(py as usize * b.width as usize)..][..b.width as usize];
            for (px, &cov) in row.iter().enumerate() {
                let Some(p) = self.px(bmx + px as i32, bmy + py) else {
                    continue;
                };
                let alpha = (cov as u32 * sa + sa) >> 8;
                // SAFETY: `px` checked the position; a pixel is four bytes.
                unsafe {
                    if alpha == 255 {
                        *p = sr as u8;
                        *p.add(1) = sg as u8;
                        *p.add(2) = sb as u8;
                        *p.add(3) = 255;
                    } else if alpha > *p.add(3) as u32 {
                        *p = (sr * alpha / 255) as u8;
                        *p.add(1) = (sg * alpha / 255) as u8;
                        *p.add(2) = (sb * alpha / 255) as u8;
                        *p.add(3) = alpha as u8;
                    }
                }
            }
        }
    }

    /// Fills `x0..x1` by `y0..y1` with the colour at its own alpha (underline and strikethrough).
    pub fn fill(&self, x0: i32, x1: i32, y0: i32, y1: i32, color: [u32; 4]) {
        let [sr, sg, sb, sa] = color;
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(p) = self.px(x, y) {
                    // SAFETY: `px` checked the position.
                    unsafe {
                        *p = (sr * sa / 255) as u8;
                        *p.add(1) = (sg * sa / 255) as u8;
                        *p.add(2) = (sb * sa / 255) as u8;
                        *p.add(3) = sa as u8;
                    }
                }
            }
        }
    }
}

/// The `(r, g, b, a)` colour argument of `draw`.
pub fn color4(color: &Bound<'_, PyAny>) -> PyResult<[u32; 4]> {
    let (r, g, b, a): (u32, u32, u32, u32) = color.extract()?;
    Ok([r, g, b, a])
}

pub fn is_vs(c: u32) -> bool {
    (0xfe00..=0xfe0f).contains(&c)
        || (0xe0100..=0xe01ef).contains(&c)
        || (0x180b..=0x180d).contains(&c)
}

pub fn is_zerowidth(c: u32) -> bool {
    matches!(c, 0x200b | 0x200c | 0x200d | 0x2060 | 0xfeff) || is_vs(c)
}

/// A dict argument as `(key, float)` pairs.
pub fn axis_pairs(axis: &Bound<'_, PyAny>) -> PyResult<Vec<(String, f32)>> {
    if axis.is_none() {
        return Ok(Vec::new());
    }
    let d = axis.cast::<PyDict>()?;
    d.iter()
        .map(|(k, v)| Ok((k.extract::<String>()?, v.extract::<f32>()?)))
        .collect()
}

/// A Cython `bint` argument: any Python object, by truth value.
#[derive(Clone, Copy)]
pub struct Bint(pub bool);

impl<'a, 'py> FromPyObject<'a, 'py> for Bint {
    type Error = PyErr;

    fn extract(obj: pyo3::Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        Ok(Bint(obj.is_truthy()?))
    }
}
