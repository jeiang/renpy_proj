//! Glyph loading and rasterizing with FreeType's semantics: hinting modes, synthetic italic and bold,
//! vertical rotation, round-stroked outlines, anti-aliased or 1-bit bitmaps.

use std::collections::HashMap;
use std::sync::Arc;

use skrifa::MetadataProvider;
use skrifa::instance::{Location, LocationRef, Size};
use skrifa::outline::{
    DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen, SmoothMode, Target,
};
use skrifa::raw::types::GlyphId;
use zeno::{Cap, Command, Fill, Join, Mask, Point, Stroke};

use crate::face::Face;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hinting {
    None,
    Bytecode,
    Auto,
    AutoLight,
}

impl Hinting {
    /// Maps the `hinting` argument of `FTFont` and `HBFont`: `None` and "none" are unhinted,
    /// every unknown value is "auto".
    pub fn parse(s: Option<&str>, is_none: bool) -> Hinting {
        if is_none {
            return Hinting::None;
        }
        match s {
            Some("none") => Hinting::None,
            Some("bytecode") => Hinting::Bytecode,
            Some("auto-light") => Hinting::AutoLight,
            _ => Hinting::Auto,
        }
    }
}

/// How glyphs are turned into the vertical layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rotation {
    None,
    /// `ftfont`, font with vertical metrics: move the origin to the vertical origin, then rotate.
    FtVertical,
    /// `ftfont`, font without vertical metrics: put the origin at the top centre, then rotate.
    FtSimulated,
    /// `hbfont`: only rotate; the shaper supplies the positions.
    Hb,
}

pub struct Settings {
    pub size: f32,
    pub hinting: Hinting,
    pub italic: bool,
    pub rotation: Rotation,
    /// Outline radius in pixels, 0 for none.
    pub outline: i32,
    pub antialias: bool,
    pub bold: bool,
}

pub struct Bitmap {
    /// Distance from the origin to the left edge.
    pub left: i32,
    /// Distance from the baseline up to the top edge.
    pub top: i32,
    pub width: u32,
    pub rows: u32,
    /// `width * rows` coverage values.
    pub data: Vec<u8>,
}

pub struct Loaded {
    /// The hinted horizontal advance in pixels (rounded to 1/64).
    pub advance: f32,
    /// The vertical advance in pixels; 0 when the font has no vertical metrics.
    pub vert_advance: f32,
    pub bitmap: Bitmap,
}

pub struct Rasterizer {
    pub face: Arc<Face>,
    pub settings: Settings,
    pub location: Location,
    /// The size in pixels FreeType would use: `size * 64` truncated, back in pixels.
    pub ppem: f32,
    hinter: Option<HintingInstance>,
    cache: HashMap<u32, Arc<Loaded>>,
}

struct Pen(Vec<Command>);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::MoveTo(Point::new(x, y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::LineTo(Point::new(x, y)));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0
            .push(Command::QuadTo(Point::new(cx0, cy0), Point::new(x, y)));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(Command::CurveTo(
            Point::new(cx0, cy0),
            Point::new(cx1, cy1),
            Point::new(x, y),
        ));
    }
    fn close(&mut self) {
        self.0.push(Command::Close);
    }
}

fn map_points(cmds: &mut [Command], f: impl Fn(Point) -> Point) {
    for c in cmds {
        *c = match *c {
            Command::MoveTo(a) => Command::MoveTo(f(a)),
            Command::LineTo(a) => Command::LineTo(f(a)),
            Command::QuadTo(a, b) => Command::QuadTo(f(a), f(b)),
            Command::CurveTo(a, b, d) => Command::CurveTo(f(a), f(b), f(d)),
            Command::Close => Command::Close,
        };
    }
}

/// The control box: the extent of every point, control points included, like `FT_Outline_Get_CBox`.
fn cbox(cmds: &[Command]) -> Option<(f32, f32, f32, f32)> {
    let mut b: Option<(f32, f32, f32, f32)> = None;
    let mut add = |p: Point| {
        b = Some(match b {
            None => (p.x, p.y, p.x, p.y),
            Some((x0, y0, x1, y1)) => (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
        });
    };
    for c in cmds {
        match *c {
            Command::MoveTo(a) | Command::LineTo(a) => add(a),
            Command::QuadTo(a, d) => {
                add(a);
                add(d)
            }
            Command::CurveTo(a, d, e) => {
                add(a);
                add(d);
                add(e)
            }
            Command::Close => {}
        }
    }
    b
}

fn round64(v: f32) -> f32 {
    (v * 64.0).round() / 64.0
}

impl Rasterizer {
    pub fn new(face: Arc<Face>, settings: Settings, location: Location) -> Rasterizer {
        let ppem = ((settings.size * 64.0) as i32) as f32 / 64.0;
        let hinter = match settings.hinting {
            Hinting::None => None,
            h => {
                let (engine, mode) = match h {
                    Hinting::Bytecode => (Engine::Interpreter, SmoothMode::Normal),
                    Hinting::AutoLight => (Engine::Auto(None), SmoothMode::Light),
                    _ => (Engine::Auto(None), SmoothMode::Normal),
                };
                HintingInstance::new(
                    &face.outlines,
                    Size::new(ppem),
                    &location,
                    HintingOptions {
                        engine,
                        target: Target::from(mode),
                    },
                )
                .ok()
            }
        };
        Rasterizer {
            face,
            settings,
            location,
            ppem,
            hinter,
            cache: HashMap::new(),
        }
    }

    /// Pixels per font unit at the current size.
    pub fn scale(&self) -> f32 {
        self.ppem / self.face.upem as f32
    }

    /// The loaded glyph (bitmap and advances), cached.
    pub fn glyph(&mut self, gid: u32) -> Arc<Loaded> {
        if let Some(g) = self.cache.get(&gid) {
            return g.clone();
        }
        if self.cache.len() > 4096 {
            self.cache.clear();
        }
        let g = Arc::new(self.load(gid));
        self.cache.insert(gid, g.clone());
        g
    }

    fn outline(&self, gid: u32) -> (Vec<Command>, f32) {
        let linear = || {
            self.face
                .font()
                .glyph_metrics(Size::new(self.ppem), &self.location)
                .advance_width(GlyphId::new(gid))
                .unwrap_or(0.0)
        };
        let Some(glyph) = self.face.outlines.get(GlyphId::new(gid)) else {
            return (Vec::new(), round64(linear()));
        };
        let mut pen = Pen(Vec::new());
        let settings = match &self.hinter {
            Some(h) => DrawSettings::hinted(h, false),
            None => DrawSettings::unhinted(Size::new(self.ppem), &self.location),
        };
        let advance = match glyph.draw(settings, &mut pen) {
            Ok(m) => m.advance_width.unwrap_or_else(linear),
            Err(_) => {
                // A hinting failure: FreeType would fail the load; draw the outline unhinted.
                pen.0.clear();
                let _ = glyph.draw(
                    DrawSettings::unhinted(Size::new(self.ppem), LocationRef::from(&self.location)),
                    &mut pen,
                );
                linear().round()
            }
        };
        (pen.0, round64(advance))
    }

    fn load(&self, gid: u32) -> Loaded {
        let face = &self.face;
        let s = &self.settings;
        let (mut cmds, advance) = self.outline(gid);

        let mut vert_advance = 0.0;
        let mut vert_bearing_y = 0.0f32;
        if let Ok(vmtx) = read_fonts::TableProvider::vmtx(face.font()) {
            if face.has_vertical {
                let scale = self.scale();
                let long = vmtx.v_metrics();
                let (adv, tsb) = match long.get(gid as usize) {
                    Some(m) => (m.advance.get() as f32, m.side_bearing.get() as f32),
                    None => {
                        let adv = long.last().map(|m| m.advance.get() as f32).unwrap_or(0.0);
                        let tsb = vmtx
                            .top_side_bearings()
                            .get((gid as usize).saturating_sub(long.len()))
                            .map(|v| v.get() as f32)
                            .unwrap_or(0.0);
                        (adv, tsb)
                    }
                };
                vert_advance = round64(adv * scale);
                vert_bearing_y = round64(tsb * scale);
            }
        }

        if s.italic {
            // 207/1000 shear, as SDL_ttf.
            map_points(&mut cmds, |p| Point::new(p.x + p.y * 0.207, p.y));
        }

        match s.rotation {
            Rotation::None => {}
            Rotation::FtVertical | Rotation::FtSimulated | Rotation::Hb => {
                if s.rotation == Rotation::FtVertical || s.rotation == Rotation::FtSimulated {
                    let (dx, dy) = if s.rotation == Rotation::FtVertical {
                        let y_max = cbox(&cmds).map(|b| b.3).unwrap_or(0.0);
                        (-((advance * 64.0) / 2.0).ceil() / 64.0, -vert_bearing_y - y_max)
                    } else {
                        // FreeType's face bbox is in font units, and the translation adds it as 26.6.
                        (
                            -((advance * 64.0) / 2.0).ceil() / 64.0,
                            -(face.bbox_y_max as f32) / 64.0,
                        )
                    };
                    map_points(&mut cmds, |p| Point::new(p.x + dx, p.y + dy));
                }
                map_points(&mut cmds, |p| Point::new(-p.y, p.x));
                if s.rotation != Rotation::Hb {
                    let dy = ((face.bbox_y_max as i32 + face.bbox_y_min as i32).div_euclid(2)) as f32 / 64.0;
                    map_points(&mut cmds, |p| Point::new(p.x, p.y + dy));
                }
            }
        }

        let mut bitmap = self.rasterize(&cmds);
        if s.outline > 0 {
            bitmap.left += s.outline;
            bitmap.top -= s.outline;
        }
        if s.bold {
            embolden(&mut bitmap, (self.ppem.round() as i32) / 10);
        }
        Loaded {
            advance,
            vert_advance,
            bitmap,
        }
    }

    fn rasterize(&self, cmds: &[Command]) -> Bitmap {
        let empty = Bitmap {
            left: 0,
            top: 0,
            width: 0,
            rows: 0,
            data: Vec::new(),
        };
        if cmds.is_empty() {
            return empty;
        }
        // y grows down in the bitmap.
        let mut down = cmds.to_vec();
        // FreeType's outlines hold 26.6 integers.
        map_points(&mut down, |p| Point::new(round64(p.x), -round64(p.y)));

        if self.settings.outline > 0 {
            // FT_Glyph_Stroke: both borders of a round-joined stroke, one contour set.
            let mut stroke = Stroke::new(self.settings.outline as f32 * 2.0);
            stroke.join(Join::Round).cap(Cap::Round);
            let mut stroked: Vec<Command> = Vec::new();
            zeno::apply(&down, stroke, None, &mut stroked);
            map_points(&mut stroked, |p| Point::new(round64(p.x), round64(p.y)));
            down = stroked;
        }
        let Some((x0, y0, x1, y1)) = cbox(&down) else {
            return empty;
        };
        let (x0, y0) = (x0.floor(), y0.floor());
        let (w, h) = ((x1.ceil() - x0) as i64, (y1.ceil() - y0) as i64);
        if w <= 0 || h <= 0 || w > 16384 || h > 16384 {
            return empty;
        }
        let mut data = vec![0u8; (w * h) as usize];
        Mask::new(&down)
            .style(Fill::NonZero)
            .size(w as u32, h as u32)
            .offset((-x0, -y0))
            .render_into(&mut data, None);
        let (left, top, width, rows) = (x0 as i32, -(y0 as i32), w as u32, h as u32);
        if !self.settings.antialias {
            for v in data.iter_mut() {
                *v = if *v >= 128 { 255 } else { 0 };
            }
        }
        Bitmap {
            left,
            top,
            width,
            rows,
            data,
        }
    }
}

/// `FT_Bitmap_Embolden(bitmap, xstr << 6, 0)` on a gray bitmap: every pixel becomes the saturated
/// sum of itself and the `xstr` pixels before it, and the bitmap grows by `xstr` columns.
fn embolden(b: &mut Bitmap, xstr: i32) {
    if xstr <= 0 || b.width == 0 {
        return;
    }
    let xstr = xstr as usize;
    let w = b.width as usize;
    let nw = w + xstr;
    let mut out = vec![0u8; nw * b.rows as usize];
    for (src, row) in b.data.chunks(w).zip(out.chunks_mut(nw)) {
        row[..w].copy_from_slice(src);
        for x in (0..nw).rev() {
            let mut v = row[x] as u32;
            for i in 1..=xstr {
                if x < i {
                    break;
                }
                let sum = v + row[x - i] as u32;
                if sum > 255 {
                    v = 255;
                    break;
                }
                v = sum;
                if v == 255 {
                    break;
                }
            }
            row[x] = v as u8;
        }
    }
    b.width = nw as u32;
    b.data = out;
}
