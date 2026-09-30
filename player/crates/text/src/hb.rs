//! `renpy.text.hbfont`: `HBFace`, `HBFont`, `Features`, `Axis`, `Variations`, `FreetypeError`,
//! `init`. Port of `hbfont.pyx`: `rustybuzz` shapes, and the hinted glyph advances that
//! `hb-ft` reads from FreeType replace the font's own advances.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use pyo3::prelude::*;
use pyo3::types::PyList;
use rustybuzz::{BufferClusterLevel, Direction, Feature, UnicodeBuffer};

use crate::bidi::chars_of;
use crate::face::Face;
use crate::glue;
use crate::metrics::{self, HbScale};
use crate::raster::{Hinting, Rasterizer, Rotation, Settings};
use crate::shape::Shaper;
use crate::{freetype_error, load_face};

#[pyfunction]
pub fn init() {}

pub const PY_DEFS: &std::ffi::CStr = c"
class Axis:
    '''
    Represents an axis in a variable font.
    '''

    def __init__(self, index, minimum, default, maximum):
        self.index = index
        self.minimum = minimum
        self.default = default
        self.maximum = maximum

    def __repr__(self):
        return '<Axis index={self.index} minimum={self.minimum} default={self.default} maximum={self.maximum}>'.format(self=self)


class Variations:
    '''
    Represents the variations of a font.
    '''

    # Ensure this isn't shortened by the console.
    _console_always_long = True

    def __init__(self):
        # A map from a named instance name to its index.
        self.instance = { }

        # A map from an axis name to its Axis object.
        self.axis = { }

    def __repr__(self):
        rv = [ ]

        for k in self.instance:
            rv.append('  Named Instance: ' + repr(k))

        for k, v in self.axis.items():
            rv.append('  Axis: ' + repr(k) + ' (minimum={}, default={}, maximum={})'.format(v.minimum, v.default, v.maximum))

        return '\\n'.join(rv)
";

/// The tuple-of-tuples `features` argument, as `(tag, value)` pairs.
#[pyclass(module = "renpy.text.hbfont", name = "Features", frozen)]
pub struct Features {
    features: Vec<([u8; 4], u32)>,
}

static FEATURE_CACHE: Mutex<Option<HashMap<Vec<([u8; 4], u32)>, Py<Features>>>> = Mutex::new(None);

#[pymethods]
impl Features {
    #[new]
    fn new(features: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Features {
            features: parse_features(features)?,
        })
    }

    /// Returns a Features object for the given features.
    #[staticmethod]
    fn get(py: Python<'_>, features: &Bound<'_, PyAny>) -> PyResult<Py<Features>> {
        let key = parse_features(features)?;
        let mut guard = FEATURE_CACHE.lock();
        let map = guard.get_or_insert_with(HashMap::new);
        if let Some(f) = map.get(&key) {
            return Ok(f.clone_ref(py));
        }
        let f = Py::new(
            py,
            Features {
                features: key.clone(),
            },
        )?;
        map.insert(key, f.clone_ref(py));
        Ok(f)
    }
}

fn parse_features(features: &Bound<'_, PyAny>) -> PyResult<Vec<([u8; 4], u32)>> {
    let mut rv = Vec::new();
    for item in features.try_iter()? {
        let item = item?;
        let name: String = item.get_item(0)?.extract()?;
        let value: u32 = item.get_item(1)?.extract()?;
        let b = name.as_bytes();
        if b.len() < 4 {
            return Err(pyo3::exceptions::PyIndexError::new_err(
                "a font feature tag needs four characters",
            ));
        }
        rv.push(([b[0], b[1], b[2], b[3]], value));
    }
    Ok(rv)
}

#[pyclass(module = "renpy.text.hbfont", name = "HBFace")]
pub struct HBFace {
    pub face: Arc<Face>,
    #[pyo3(get, set)]
    pub r#fn: Py<PyAny>,
    #[pyo3(get, set)]
    pub variations: Py<PyAny>,
    #[pyo3(get, set)]
    pub current_instance: Py<PyAny>,
    #[pyo3(get, set)]
    pub current_axis: Py<PyAny>,
}

#[pymethods]
impl HBFace {
    #[new]
    fn new(py: Python<'_>, f: &Bound<'_, PyAny>, index: u32, r#fn: Py<PyAny>) -> PyResult<Self> {
        let face = load_face(py, "renpy.text.hbfont", f, index)?;

        let variations = if face.is_variable() {
            let module = py.import("renpy.text.hbfont")?;
            let v = module.getattr("Variations")?.call0()?;
            let axis_class = module.getattr("Axis")?;
            let axes = v.getattr("axis")?;
            for a in face.axes.iter().filter(|a| a.index < 16) {
                let obj =
                    axis_class.call1((a.index, a.min as f64, a.default as f64, a.max as f64))?;
                axes.set_item(&a.name, obj)?;
            }
            let instances = v.getattr("instance")?;
            // `for 0 < i < num_namedstyles`: the first named instance is skipped, as in stock.
            for inst in face.instances.iter().filter(|i| i.index > 0) {
                instances.set_item(&inst.name, inst.index)?;
            }
            v.unbind()
        } else {
            py.None()
        };
        Ok(HBFace {
            face,
            r#fn,
            variations,
            current_instance: py.None(),
            current_axis: py.None(),
        })
    }
}

#[pyclass(module = "renpy.text.hbfont", name = "HBFont")]
pub struct HBFont {
    face_obj: Py<HBFace>,
    face: Arc<Face>,
    raster: Rasterizer,
    shaper: Shaper,
    features: Option<Py<Features>>,
    vertical: bool,
    expand: i32,
    has_setup: bool,

    #[pyo3(get, set)]
    underline_offset: i32,
    #[pyo3(get, set)]
    underline_height: i32,
    #[pyo3(get, set)]
    ascent: i32,
    #[pyo3(get, set)]
    descent: i32,
    #[pyo3(get, set)]
    height: i32,
    #[pyo3(get, set)]
    lineskip: i32,
}

#[pymethods]
impl HBFont {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        face: Bound<'_, HBFace>,
        size: f32,
        bold: f32,
        italic: glue::Bint,
        outline: i32,
        antialias: glue::Bint,
        vertical: glue::Bint,
        hinting: &Bound<'_, PyAny>,
        instance: &Bound<'_, PyAny>,
        axis: &Bound<'_, PyAny>,
        features: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let f = face.borrow();
        let mut size = size.max(1.0);
        let mut bold = bold;
        let mut instance: Option<String> = instance.extract().ok().filter(|_| !instance.is_none());

        if instance.is_none() && f.variations.bind(py).is_truthy()? {
            let has = |n: &str| f.face.instances.iter().any(|i| i.index > 0 && i.name == n);
            if bold >= 1.0 {
                if has("bold") {
                    bold = 0.0;
                    instance = Some("bold".into());
                } else if has("bold italic") {
                    bold = 0.0;
                    instance = Some("bold italic".into());
                }
            } else if has("regular") {
                instance = Some("regular".into());
            } else if has("italic") {
                instance = Some("italic".into());
            }
        }
        let (italic, antialias, vertical) = (italic.0, antialias.0 || bold != 0.0, vertical.0);

        let features = if features.is_truthy()? {
            Some(Features::get(py, features)?)
        } else {
            None
        };
        size = size
            * glue::config_scale(py, "ftfont_scale", f.r#fn.bind(py))?
            * glue::pref_font_size(py)?;

        let hint_str = hinting.extract::<String>().ok();
        let hinting = Hinting::parse(
            hint_str.as_deref(),
            hinting.is_none() || hint_str.as_deref() == Some("none"),
        );
        let arc = f.face.clone();
        let axis_values = glue::axis_pairs(axis)?;
        let settings_vec = arc.settings(instance.as_deref(), &axis_values);
        let location = arc.location(&settings_vec);
        let shaper = Shaper::new(arc.clone(), &settings_vec)
            .ok_or_else(|| freetype_error(py, "renpy.text.hbfont", 3, "invalid file format"))?;
        let settings = Settings {
            size,
            hinting,
            italic,
            rotation: if vertical {
                Rotation::Hb
            } else {
                Rotation::None
            },
            outline,
            antialias,
            bold: bold != 0.0,
        };
        let raster = Rasterizer::new(arc.clone(), settings, location);
        drop(f);
        Ok(HBFont {
            face_obj: face.unbind(),
            face: arc,
            raster,
            shaper,
            features,
            vertical,
            expand: outline * 2,
            has_setup: false,
            underline_offset: 0,
            underline_height: 0,
            ascent: 0,
            descent: 0,
            height: 0,
            lineskip: 0,
        })
    }

    #[getter]
    fn expand(&self) -> i32 {
        self.expand
    }

    fn glyphs<'py>(
        &mut self,
        py: Python<'py>,
        s: &Bound<'py, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyList>> {
        self.setup(py)?;
        let chars = chars_of(s)?;
        let rv = PyList::empty(py);

        let mut buffer = UnicodeBuffer::new();
        for (i, c) in chars.iter().enumerate() {
            buffer.add(*c, i as u32);
        }
        buffer.set_direction(match (self.vertical, level & 1 != 0) {
            (true, true) => Direction::BottomToTop,
            (true, false) => Direction::TopToBottom,
            (false, true) => Direction::RightToLeft,
            (false, false) => Direction::LeftToRight,
        });
        buffer.guess_segment_properties();
        buffer.set_cluster_level(BufferClusterLevel::MonotoneCharacters);

        let features: Vec<Feature> = match &self.features {
            Some(f) => f
                .get()
                .features
                .iter()
                .map(|(tag, value)| {
                    Feature::new(rustybuzz::ttf_parser::Tag::from_bytes(tag), *value, ..)
                })
                .collect(),
            None => Vec::new(),
        };
        let out = rustybuzz::shape(&self.shaper.buzz, &features, buffer);

        let scale = HbScale::new(&self.face, self.raster.settings.size);
        let hinted = matches!(
            self.raster.settings.hinting,
            Hinting::Auto | Hinting::Bytecode
        );
        let upem_scale = |v: i32| scale.apply(v as i64) as f32 / 64.0;

        for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions().iter()) {
            let gl = glue::new_glyph(py)?;
            let cluster = info.cluster as usize;
            gl.setattr(
                "character",
                chars.get(cluster).map(|c| *c as u32).unwrap_or(0),
            )?;
            gl.setattr("glyph", info.glyph_id)?;
            gl.setattr("ascent", self.ascent)?;
            gl.setattr("descent", -self.descent)?;
            gl.setattr("line_spacing", self.lineskip)?;
            gl.setattr("draw", true)?;

            let (x_offset, y_offset, advance);
            if self.vertical {
                x_offset = -upem_scale(pos.y_offset);
                y_offset = -upem_scale(pos.x_offset);
                advance = -upem_scale(pos.y_advance);
            } else {
                x_offset = upem_scale(pos.x_offset);
                y_offset = -upem_scale(pos.y_offset);
                advance = if hinted && pos.x_advance != 0 {
                    // hb-ft takes the base advance from FreeType (hinted); positioning adjustments
                    // from GPOS come on top of it.
                    let base = self
                        .shaper
                        .buzz
                        .glyph_hor_advance(rustybuzz::ttf_parser::GlyphId(info.glyph_id as u16))
                        .unwrap_or(0) as i64;
                    let delta = scale.apply(pos.x_advance as i64) - scale.apply(base);
                    let hinted_base = self.raster.glyph(info.glyph_id).advance;
                    hinted_base + delta as f32 / 64.0
                } else {
                    upem_scale(pos.x_advance)
                };
            }
            gl.setattr("x_offset", x_offset)?;
            gl.setattr("y_offset", y_offset)?;
            gl.setattr("advance", advance)?;
            gl.setattr("width", advance)?;
            rv.append(gl)?;
        }
        Ok(rv)
    }

    /// The intersection of `bounds` and the area the glyphs draw to: `(x, y, w, h)`.
    fn bounds<'py>(
        &mut self,
        py: Python<'py>,
        glyphs: &Bound<'py, PyAny>,
        bounds: (i32, i32, i32, i32),
    ) -> PyResult<(i32, i32, i32, i32)> {
        let (mut x, mut y, mut w, mut h) = bounds;
        self.setup(py)?;
        for g in glyphs.try_iter()? {
            let g = g?;
            let gi = glue::read_glyph(&g)?;
            if gi.split_instead {
                continue;
            }
            let cache = self.raster.glyph(gi.glyph);
            let b = &cache.bitmap;

            let bmx = (gi.x as f64 + 0.5 + gi.x_offset as f64) as i32 + b.left;
            let bmy = (gi.y as f64 + gi.y_offset as f64) as i32 - b.top;
            x = x.min(bmx);
            y = y.min(bmy);
            w = w.max(bmx + b.width as i32);
            h = h.max(bmy + b.rows as i32);

            glue::set_add(
                &g,
                (-(bmx - gi.x)).max(0),
                (-(bmy - gi.y)).max(0),
                ((bmx + b.width as i32) as f32 - (gi.x as f32 + gi.width)).max(0.0) as i32,
                (bmy + b.rows as i32 - (gi.y + gi.line_spacing)).max(0),
            )?;
        }
        Ok((x, y, w, h))
    }

    #[pyo3(signature = (surface, xo, yo, color, glyphs, underline, strikethrough, black_color=None))]
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        py: Python<'_>,
        surface: &Bound<'_, PyAny>,
        xo: f32,
        yo: i32,
        color: &Bound<'_, PyAny>,
        glyphs: &Bound<'_, PyList>,
        underline: i32,
        strikethrough: glue::Bint,
        black_color: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let _ = black_color;
        let color = glue::color4(color)?;
        if color[3] == 0 {
            return Ok(());
        }
        self.setup(py)?;
        let (target, surf) = glue::Target::of(surface)?;
        let expand = self.expand;

        for g in glyphs.iter() {
            let gi = glue::read_glyph(&g)?;
            if gi.split_instead {
                continue;
            }
            let x = ((gi.x as f32 + xo) + gi.x_offset) as i32;
            let y = ((gi.y + yo) as f32 + gi.y_offset) as i32;

            let underline_x = ((gi.x as f32 + xo) - gi.delta_x_adjustment as f32) as i32;
            let mut underline_end = (gi.x as f32 + xo + gi.advance + expand as f32 + 0.9999) as i32;

            let cache = self.raster.glyph(gi.glyph);
            let b = &cache.bitmap;

            let bmx = (x as f64 + 0.5) as i32 + b.left;
            let bmy = y - b.top;
            underline_end = underline_end.min(target.w - 1);

            if gi.draw {
                target.blit(b, bmx, bmy, color);
            }
            if underline != 0 {
                let ly = y - self.underline_offset - 1;
                let lh = self.underline_height * underline;
                target.fill(underline_x, underline_end, ly, ly + lh, color);
            }
            if strikethrough.0 {
                let ly = y - self.ascent + self.height / 2;
                let lh = (self.height / 10).max(1);
                target.fill(underline_x, underline_end, ly, ly + lh, color);
            }
        }
        surf.get().touch();
        Ok(())
    }
}

impl HBFont {
    fn setup(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.has_setup {
            return Ok(());
        }
        self.has_setup = true;
        let face = &self.face;
        let size = self.raster.settings.size;
        let scale = HbScale::new(face, size);

        let (mut asc, mut desc) = (
            scale.apply(face.hb_ascender as i64),
            scale.apply(face.hb_descender as i64),
        );
        let (v_asc, v_desc) = match face.vhea {
            Some((a, d)) => (scale.apply(a as i64), scale.apply(d as i64)),
            None => {
                let half = scale.apply(face.upem as i64) / 2;
                (half, -half)
            }
        };
        let fn_obj = self.face_obj.bind(py).borrow().r#fn.clone_ref(py);
        let vext = glue::config_scale(py, "ftfont_vertical_extent_scale", fn_obj.bind(py))? as f64;

        // Fonts whose tables give no metrics take FreeType's size metrics.
        if asc == 0 && desc == 0 {
            let (a, d) = metrics::size_ascender_descender(face, size);
            asc = a;
            desc = d;
        }

        if self.vertical {
            self.ascent = metrics::ceil(v_asc) as i32;
            self.descent = metrics::floor(v_desc) as i32;
        } else {
            self.ascent = metrics::ceil((asc as f64 * vext) as i64) as i32;
            self.descent = metrics::floor((desc as f64 * vext) as i64) as i32;
        }
        if self.descent > 0 {
            self.descent = -self.descent;
        }
        self.ascent += self.expand;
        self.descent -= self.expand;
        self.height = self.ascent - self.descent;
        self.lineskip = (self.height as f64 * glue::pref_line_spacing(py)?) as i32;

        let underline_offset = scale.apply(face.post_underline_position as i64);
        let underline_size = scale.apply(face.post_underline_thickness as i64);
        self.underline_offset = if self.vertical {
            metrics::floor((self.ascent - self.descent) as i64 - underline_offset) as i32
        } else {
            metrics::floor(underline_offset) as i32
        };
        self.underline_height = metrics::floor(underline_size) as i32;
        if self.underline_height < 1 {
            self.underline_height = 1;
        }
        self.underline_height += self.expand;
        Ok(())
    }
}
