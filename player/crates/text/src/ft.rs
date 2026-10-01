//! `renpy.text.ftfont`: `FTFace`, `FTFont`, `init`, `FreetypeError`. Port of `ftfont.pyx`; the
//! shaping is FreeType's: one glyph per character, kerning from the `kern` table only.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::bidi::chars_of;
use crate::face::Face;
use crate::glue::{self, GlyphIn};
use crate::load_face;
use crate::metrics;
use crate::raster::{Hinting, Rasterizer, Rotation, Settings};
use crate::shape::VertSubst;

#[pyfunction]
pub fn init() {}

#[pyclass(module = "renpy.text.ftfont", name = "FTFace")]
pub struct FTFace {
    pub face: Arc<Face>,
    #[pyo3(get, set)]
    pub r#fn: Py<PyAny>,
}

#[pymethods]
impl FTFace {
    #[new]
    fn new(py: Python<'_>, f: &Bound<'_, PyAny>, index: u32, r#fn: Py<PyAny>) -> PyResult<Self> {
        Ok(FTFace {
            face: load_face(py, "renpy.text.ftfont", f, index)?,
            r#fn,
        })
    }
}

struct Cached {
    advance: f32,
    width: i32,
}

#[pyclass(module = "renpy.text.ftfont", name = "FTFont")]
pub struct FTFont {
    face_obj: Py<FTFace>,
    face: Arc<Face>,
    raster: Rasterizer,
    vert: Option<VertSubst>,
    vertical: bool,
    overhang: i32,
    has_setup: bool,

    #[pyo3(get, set)]
    underline_offset: i32,
    #[pyo3(get, set)]
    underline_height: i32,
    #[pyo3(get, set)]
    expand: i32,
    #[pyo3(get, set)]
    ascent: i32,
    #[pyo3(get, set)]
    descent: i32,
    #[pyo3(get, set)]
    height: i32,
    #[pyo3(get, set)]
    lineskip: i32,
    /// Ren'Py 7 `ftfont.pyx` composites overlapping glyph bitmaps over each other; Ren'Py 8 keeps
    /// the larger alpha. The engine patch sets this for a Ren'Py 7 game.
    #[pyo3(get, set)]
    over_blend: bool,
}

#[pymethods]
impl FTFont {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        face: Bound<'_, FTFace>,
        size: f32,
        bold: f32,
        italic: glue::Bint,
        outline: i32,
        antialias: glue::Bint,
        vertical: glue::Bint,
        hinting: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let mut size = size.max(1.0);
        let (italic, antialias, vertical) = (italic.0, antialias.0 || bold != 0.0, vertical.0);
        let f = face.borrow();
        size = size
            * glue::config_scale(py, "ftfont_scale", f.r#fn.bind(py))?
            * glue::pref_font_size(py)?;

        let hint_str = hinting.extract::<String>().ok();
        let hinting = Hinting::parse(
            hint_str.as_deref(),
            hinting.is_none() || hint_str.as_deref() == Some("none"),
        );
        let rotation = if !vertical {
            Rotation::None
        } else if f.face.has_vertical {
            Rotation::FtVertical
        } else {
            Rotation::FtSimulated
        };
        let settings = Settings {
            size,
            hinting,
            italic,
            rotation,
            outline,
            antialias,
            bold: bold != 0.0,
        };
        let arc = f.face.clone();
        let location = arc.location(&[]);
        let raster = Rasterizer::new(arc.clone(), settings, location);
        let overhang = if bold != 0.0 {
            raster.ppem.round() as i32 / 10
        } else {
            0
        };
        drop(f);
        Ok(FTFont {
            face_obj: face.unbind(),
            face: arc,
            raster,
            vert: None,
            vertical,
            overhang,
            has_setup: false,
            underline_offset: 0,
            underline_height: 0,
            expand: outline * 2,
            ascent: 0,
            descent: 0,
            height: 0,
            lineskip: 0,
            over_blend: false,
        })
    }

    fn glyphs<'py>(
        &mut self,
        py: Python<'py>,
        s: &Bound<'py, PyAny>,
        _level: i32,
    ) -> PyResult<Bound<'py, PyList>> {
        self.setup(py)?;
        let chars: Vec<u32> = chars_of(s)?.into_iter().map(|c| c as u32).collect();
        let rv = PyList::empty(py);
        let len = chars.len();
        if len == 0 {
            return Ok(rv);
        }

        // The character and variation selector that start at `i`, and the glyph they map to.
        let lookup = |face: &Face, i: usize| -> (u32, u32, u32) {
            let c = chars[i];
            let vs_at = i + 1;
            if vs_at < len && glue::is_vs(chars[vs_at]) {
                let vs = chars[vs_at];
                let index = face.glyph_index(c, vs);
                if index != 0 {
                    return (c, vs, index);
                }
            }
            (c, 0, face.nominal(c))
        };

        let (mut next_c, mut vs, mut next_index) = lookup(&self.face, 0);
        let mut next_min_advance = 0.0f32;

        for i in 0..len {
            let c = next_c;
            let index = next_index;
            let vs_now = vs;
            let min_advance = next_min_advance;

            let cache = self.cached(index);

            let gl = glue::new_glyph(py)?;
            gl.setattr("character", c)?;
            gl.setattr("variation", vs_now)?;
            gl.setattr("ascent", self.ascent)?;
            gl.setattr("descent", -self.descent)?;
            gl.setattr("width", cache.width as f32)?;
            gl.setattr("line_spacing", self.lineskip)?;
            gl.setattr("draw", true)?;

            let advance;
            if i < len - 1 {
                let (nc, nvs, nindex) = lookup(&self.face, i + 1);
                next_c = nc;
                vs = nvs;
                next_index = nindex;

                let kern = self.kerning(index, next_index) as f32;
                advance = if cache.advance + kern > min_advance {
                    cache.advance + kern
                } else {
                    min_advance
                };
                next_min_advance = cache.advance - advance;
            } else {
                advance = cache.advance;
            }
            gl.setattr("advance", advance)?;

            if glue::is_zerowidth(c) {
                gl.setattr("width", 0f32)?;
                gl.setattr("advance", 0f32)?;
                gl.setattr("draw", false)?;
            }
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
            if gi.split_instead || gi.character == 0x200b {
                continue;
            }
            let index = self.face.glyph_index(gi.character, gi.variation);
            let gid = self.subst(index);
            let cache = self.raster.glyph(gid);
            let b = &cache.bitmap;

            let bmx = (gi.x as f32 + 0.5) as i32 + b.left;
            let bmy = gi.y - b.top;
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
            let gi: GlyphIn = glue::read_glyph(&g)?;
            if gi.split_instead {
                continue;
            }
            let x = (gi.x as f32 + xo) as i32;
            let y = (gi.y + yo) as i32;
            let underline_x = x - gi.delta_x_adjustment;
            let mut underline_end = x + (gi.advance + expand as f32 + 0.999) as i32;

            let index = self.face.glyph_index(gi.character, gi.variation);
            let gid = self.subst(index);
            let cache = self.raster.glyph(gid);
            let b = &cache.bitmap;

            let bmx = (x as f64 + 0.5) as i32 + b.left;
            let bmy = y - b.top;
            underline_end = underline_end.min(target.w - 1);

            if gi.draw {
                target.blit(b, bmx, bmy, color, self.over_blend);
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

impl FTFont {
    fn cached(&mut self, index: u32) -> Cached {
        let gid = self.subst(index);
        let g = self.raster.glyph(gid);
        let advance = if self.vertical {
            if self.face.has_vertical {
                g.vert_advance + self.expand as f32 + self.overhang as f32
            } else {
                self.lineskip as f32 + self.overhang as f32
            }
        } else {
            g.advance + self.expand as f32 + self.overhang as f32
        };
        Cached {
            advance,
            width: g.bitmap.width as i32 + g.bitmap.left,
        }
    }

    /// `FT_Get_Kerning(FT_KERNING_DEFAULT)` followed by `FT_ROUND`, in pixels: kerning is scaled
    /// down below 25 ppem, then fitted to the pixel grid.
    fn kerning(&self, left: u32, right: u32) -> i32 {
        let units = self.face.kerning(left, right) as i64;
        if units == 0 {
            return 0;
        }
        let mut k = metrics::mul_fix(
            units,
            metrics::y_scale(&self.face, self.raster.settings.size),
        );
        let ppem = self.raster.ppem.round() as i64;
        if ppem < 25 {
            k = metrics::mul_div(k, ppem, 25);
        }
        metrics::round(k) as i32
    }

    /// The vertical form of a glyph when the font has one (`GetVerticalGlyph`), else the glyph.
    fn subst(&mut self, index: u32) -> u32 {
        if !self.vertical {
            return index;
        }
        let v = self
            .vert
            .get_or_insert_with(|| VertSubst::new(self.face.clone()));
        v.map(index)
    }

    fn setup(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.has_setup {
            return Ok(());
        }
        self.has_setup = true;
        let face = &self.face;
        let size = self.raster.settings.size;
        let scale = metrics::y_scale(face, size);
        let (asc, desc) = metrics::size_ascender_descender(face, size);
        let fn_obj = self.face_obj.bind(py).borrow().r#fn.clone_ref(py);
        let vext = glue::config_scale(py, "ftfont_vertical_extent_scale", fn_obj.bind(py))? as f64;

        self.ascent = metrics::ceil((asc as f64 * vext) as i64) as i32;
        self.descent = metrics::floor((desc as f64 * vext) as i64) as i32;
        if self.descent > 0 {
            self.descent = -self.descent;
        }
        self.ascent += self.expand;
        self.descent -= self.expand;
        self.height = self.ascent - self.descent;
        self.lineskip = (self.height as f64 * glue::pref_line_spacing(py)?) as i32;

        self.underline_offset = if self.vertical {
            metrics::floor(metrics::mul_fix(
                (face.ft_ascender + face.ft_descender - face.underline_position) as i64,
                scale,
            )) as i32
        } else {
            metrics::floor(metrics::mul_fix(face.underline_position as i64, scale)) as i32
        };
        self.underline_height =
            metrics::floor(metrics::mul_fix(face.underline_thickness as i64, scale)) as i32;
        if self.underline_height < 1 {
            self.underline_height = 1;
        }
        self.underline_height += self.expand;
        Ok(())
    }
}
