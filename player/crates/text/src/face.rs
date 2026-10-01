//! A parsed font file: the bytes, the tables Ren'Py's metrics read, and the variable-font tables.

use std::collections::HashMap;
use std::sync::Arc;

use read_fonts::TableProvider;
use skrifa::instance::Location;
use skrifa::raw::types::Tag;
use skrifa::{FontRef, MetadataProvider, charmap::Charmap, outline::OutlineGlyphCollection};

/// One axis of a variable font, as `hbfont.Axis` shows it.
pub struct AxisInfo {
    pub index: usize,
    /// The lower-case name the `axis` style property uses.
    pub name: String,
    pub tag: Tag,
    pub min: f32,
    pub default: f32,
    pub max: f32,
}

/// A named instance and its user-space coordinates, one per axis.
pub struct InstanceInfo {
    pub name: String,
    pub index: usize,
    pub coords: Vec<f32>,
}

pub struct Face {
    // The borrowed fields come first: they must drop before `data`.
    font: FontRef<'static>,
    pub outlines: OutlineGlyphCollection<'static>,
    pub charmap: Charmap<'static>,
    pub data: Arc<[u8]>,
    pub index: u32,

    pub upem: u16,
    pub bbox_y_max: i16,
    pub bbox_y_min: i16,

    /// hhea values, with FreeType's fallback to OS/2 when both are zero.
    pub ft_ascender: i32,
    pub ft_descender: i32,
    /// HarfBuzz's horizontal metrics: OS/2 typo when USE_TYPO_METRICS is set, else hhea.
    pub hb_ascender: i32,
    pub hb_descender: i32,
    pub vhea: Option<(i32, i32)>,
    pub has_vertical: bool,

    /// FreeType's underline: the `post` position moved to the middle of the line.
    pub underline_position: i32,
    pub underline_thickness: i32,
    /// HarfBuzz's underline: always the `post` table.
    pub post_underline_position: i32,
    pub post_underline_thickness: i32,

    pub axes: Vec<AxisInfo>,
    pub instances: Vec<InstanceInfo>,

    /// Pairs from format-0 horizontal `kern` subtables, `(left << 16 | right) -> value`.
    pub kern: Vec<KernTable>,
}

fn axis_name(tag: Tag, fallback: Option<String>) -> String {
    // FreeType names the registered axes itself.
    let n = match &tag.to_be_bytes() {
        b"wght" => "Weight".to_string(),
        b"wdth" => "Width".to_string(),
        b"opsz" => "Optical Size".to_string(),
        b"slnt" => "Slant".to_string(),
        b"ital" => "Italic".to_string(),
        _ => fallback.unwrap_or_else(|| tag.to_string()),
    };
    n.to_lowercase()
}

/// One usable format-0 horizontal `kern` subtable.
pub struct KernTable {
    /// `(left << 16 | right) -> value`; the first of equal keys wins, as in FreeType's linear search.
    pairs: HashMap<u32, i16>,
    /// Coverage bit 3: the value replaces the sum so far instead of adding to it.
    replace: bool,
}

/// The subtables FreeType's `tt_face_load_kern` keeps (at most 32; formats other than 0 and
/// non-horizontal ones are skipped), with the pair count cut to what the subtable `length` holds.
/// A font whose `length` field is too short for its pair count (a `kern` table of more than 64 KiB)
/// so loses the pairs beyond it, and FreeType never finds them.
fn parse_kern(data: &[u8]) -> Vec<KernTable> {
    let mut tables = Vec::new();
    let be16 = |o: usize| -> Option<usize> {
        Some(u16::from_be_bytes([*data.get(o)?, *data.get(o + 1)?]) as usize)
    };
    let Some(count) = be16(2) else { return tables };
    let mut at = 4;
    for _ in 0..count.min(32) {
        if at + 6 > data.len() {
            break;
        }
        let (Some(length), Some(coverage)) = (be16(at + 2), be16(at + 4)) else {
            break;
        };
        if length <= 6 + 8 {
            break;
        }
        let next = (at + length).min(data.len());
        if coverage >> 8 == 0 && coverage & 3 == 1 && at + 6 + 8 <= next {
            let declared = be16(at + 6).unwrap_or(0);
            let start = at + 14;
            let n = declared.min(next.saturating_sub(start) / 6);
            let mut pairs = HashMap::new();
            let mut o = start;
            for _ in 0..n {
                let (Some(l), Some(r), Some(v)) = (be16(o), be16(o + 2), be16(o + 4)) else {
                    break;
                };
                pairs
                    .entry(((l as u32) << 16) | r as u32)
                    .or_insert(v as u16 as i16);
                o += 6;
            }
            tables.push(KernTable {
                pairs,
                replace: coverage & 8 != 0,
            });
        }
        at = next;
    }
    tables
}

impl Face {
    pub fn load(data: Arc<[u8]>, index: u32) -> Result<Face, String> {
        // SAFETY: `data` is kept in the struct and never mutated, so the bytes outlive every borrow
        // below; the borrowed fields are declared first and drop first.
        let bytes: &'static [u8] = unsafe { &*(data.as_ref() as *const [u8]) };
        let font = FontRef::from_index(bytes, index).map_err(|e| format!("{e}"))?;
        let head = font.head().map_err(|e| format!("head table: {e}"))?;
        let upem = head.units_per_em();
        if upem == 0 {
            return Err("units per em is zero".into());
        }

        let (mut ft_ascender, mut ft_descender) = (0, 0);
        let (mut hb_ascender, mut hb_descender) = (0, 0);
        if let Ok(hhea) = font.hhea() {
            ft_ascender = hhea.ascender().to_i16() as i32;
            ft_descender = hhea.descender().to_i16() as i32;
            hb_ascender = ft_ascender;
            hb_descender = ft_descender;
        }
        if let Ok(os2) = font.os2() {
            if ft_ascender == 0 && ft_descender == 0 {
                if os2.s_typo_ascender() != 0 || os2.s_typo_descender() != 0 {
                    ft_ascender = os2.s_typo_ascender() as i32;
                    ft_descender = os2.s_typo_descender() as i32;
                } else {
                    ft_ascender = os2.us_win_ascent() as i32;
                    ft_descender = -(os2.us_win_descent() as i32);
                }
            }
            if os2.fs_selection().bits() & (1 << 7) != 0 {
                hb_ascender = os2.s_typo_ascender() as i32;
                hb_descender = os2.s_typo_descender() as i32;
            }
        }

        let vhea = font
            .vhea()
            .ok()
            .map(|v| (v.ascender().to_i16() as i32, v.descender().to_i16() as i32));
        let has_vertical = vhea.is_some() && font.table_data(Tag::new(b"vmtx")).is_some();

        let post_underline = match font.post() {
            Ok(p) => (
                p.underline_position().to_i16() as i32,
                p.underline_thickness().to_i16() as i32,
            ),
            Err(_) => (-(upem as i32) / 10, upem as i32 / 20),
        };
        // sfnt: FreeType takes `post`'s position as the top of the line and moves it to the middle.
        let (underline_position, underline_thickness) =
            (post_underline.0 - post_underline.1 / 2, post_underline.1);

        let mut axes = Vec::new();
        let mut instances = Vec::new();
        let axis_collection = font.axes();
        for a in axis_collection.iter() {
            let fallback = font
                .localized_strings(a.name_id())
                .english_or_first()
                .map(|s| s.to_string());
            axes.push(AxisInfo {
                index: a.index(),
                name: axis_name(a.tag(), fallback),
                tag: a.tag(),
                min: a.min_value(),
                default: a.default_value(),
                max: a.max_value(),
            });
        }
        for (i, inst) in font.named_instances().iter().enumerate() {
            if let Some(name) = font
                .localized_strings(inst.subfamily_name_id())
                .english_or_first()
                .map(|s| s.to_string())
            {
                instances.push(InstanceInfo {
                    name: name.to_lowercase(),
                    index: i,
                    coords: inst.user_coords().collect(),
                });
            }
        }

        let kern = font
            .table_data(Tag::new(b"kern"))
            .map(|d| parse_kern(d.as_bytes()))
            .unwrap_or_default();

        Ok(Face {
            outlines: font.outline_glyphs(),
            charmap: font.charmap(),
            font,
            data,
            index,
            upem,
            bbox_y_max: head.y_max(),
            bbox_y_min: head.y_min(),
            ft_ascender,
            ft_descender,
            hb_ascender,
            hb_descender,
            vhea,
            has_vertical,
            underline_position,
            underline_thickness,
            post_underline_position: post_underline.0,
            post_underline_thickness: post_underline.1,
            axes,
            instances,
            kern,
        })
    }

    pub fn font(&self) -> &FontRef<'static> {
        &self.font
    }

    pub fn is_variable(&self) -> bool {
        !self.axes.is_empty()
    }

    /// The glyph for `ch`, or for `ch` with variation selector `vs`; 0 when there is none.
    pub fn glyph_index(&self, ch: u32, vs: u32) -> u32 {
        if vs != 0 {
            return match self.charmap.map_variant(ch, vs) {
                Some(skrifa::charmap::MapVariant::Variant(g)) => g.to_u32(),
                Some(skrifa::charmap::MapVariant::UseDefault) => self.nominal(ch),
                None => 0,
            };
        }
        self.nominal(ch)
    }

    pub fn nominal(&self, ch: u32) -> u32 {
        self.charmap.map(ch).map(|g| g.to_u32()).unwrap_or(0)
    }

    /// The user-space setting of every axis for a named instance and per-axis overrides, as
    /// `HBFont.setup_variations` picks them.
    pub fn settings(&self, instance: Option<&str>, axis: &[(String, f32)]) -> Vec<(Tag, f32)> {
        let mut user: Vec<f32> = self.axes.iter().map(|a| a.default).collect();
        if let Some(name) = instance {
            let name = name.to_lowercase();
            if let Some(inst) = self
                .instances
                .iter()
                .find(|i| i.index > 0 && i.name == name)
            {
                for (slot, v) in user.iter_mut().zip(inst.coords.iter()) {
                    *slot = *v;
                }
            }
        }
        for (name, value) in axis {
            let name = name.to_lowercase();
            if let Some(a) = self.axes.iter().find(|a| a.name == name && a.index < 16) {
                user[a.index] = value.clamp(a.min, a.max);
            }
        }
        self.axes.iter().map(|a| (a.tag, user[a.index])).collect()
    }

    pub fn location(&self, settings: &[(Tag, f32)]) -> Location {
        self.font.axes().location(settings.iter().copied())
    }

    /// The FreeType kerning value for a pair, in font units.
    pub fn kerning(&self, left: u32, right: u32) -> i32 {
        let key = (left << 16) | (right & 0xffff);
        let mut result = 0i32;
        for t in &self.kern {
            if let Some(&v) = t.pairs.get(&key) {
                result = if t.replace {
                    v as i32
                } else {
                    result + v as i32
                };
            }
        }
        result
    }
}
