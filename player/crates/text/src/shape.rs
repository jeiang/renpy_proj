//! Text shaping (`rustybuzz`) and the GSUB vertical-glyph table that `ftfont` uses.

use std::collections::HashMap;
use std::sync::Arc;

use read_fonts::TableProvider;
use read_fonts::tables::gsub::{SingleSubst, SubstitutionLookup};
use skrifa::raw::types::Tag;

use crate::face::Face;

/// A `rustybuzz` face over the bytes of a [`Face`], with one font's variation settings applied.
pub struct Shaper {
    // Drops before the bytes it borrows.
    pub buzz: rustybuzz::Face<'static>,
    _face: Arc<Face>,
}

impl Shaper {
    pub fn new(face: Arc<Face>, variations: &[(Tag, f32)]) -> Option<Shaper> {
        // SAFETY: `_face` keeps the byte array alive for as long as `buzz`, and `buzz` drops first.
        let bytes: &'static [u8] = unsafe { &*(face.data.as_ref() as *const [u8]) };
        let mut buzz = rustybuzz::Face::from_slice(bytes, face.index)?;
        let vars: Vec<rustybuzz::Variation> = variations
            .iter()
            .map(|(tag, value)| rustybuzz::Variation {
                tag: rustybuzz::ttf_parser::Tag::from_bytes(&tag.to_be_bytes()),
                value: *value,
            })
            .collect();
        if !vars.is_empty() {
            buzz.set_variations(&vars);
        }
        Some(Shaper { buzz, _face: face })
    }
}

/// Glyph to vertical-form glyph, from the single substitutions of the `vrt2` and `vert` features.
pub struct VertSubst {
    map: HashMap<u32, u32>,
}

impl VertSubst {
    pub fn new(face: Arc<Face>) -> VertSubst {
        let mut map = HashMap::new();
        if let Ok(gsub) = face.font().gsub()
            && let (Ok(features), Ok(lookups)) = (gsub.feature_list(), gsub.lookup_list())
        {
            // `vert` first, so `vrt2` wins where both exist.
            for wanted in [b"vert", b"vrt2"] {
                for rec in features.feature_records() {
                    if rec.feature_tag() != Tag::new(wanted) {
                        continue;
                    }
                    let Ok(feature) = rec.feature(features.offset_data()) else {
                        continue;
                    };
                    for idx in feature.lookup_list_indices() {
                        let Ok(lookup) = lookups.lookups().get(idx.get() as usize) else {
                            continue;
                        };
                        collect_single(&lookup, &mut map);
                    }
                }
            }
        }
        VertSubst { map }
    }

    pub fn map(&self, gid: u32) -> u32 {
        self.map.get(&gid).copied().unwrap_or(gid)
    }
}

fn collect_single(lookup: &SubstitutionLookup<'_>, map: &mut HashMap<u32, u32>) {
    let SubstitutionLookup::Single(l) = lookup else {
        return;
    };
    for sub in l.subtables().iter().flatten() {
        match sub {
            SingleSubst::Format1(t) => {
                let Ok(cov) = t.coverage() else { continue };
                for g in cov.iter() {
                    let out = (g.to_u32() as i32 + t.delta_glyph_id() as i32) & 0xffff;
                    map.insert(g.to_u32(), out as u32);
                }
            }
            SingleSubst::Format2(t) => {
                let Ok(cov) = t.coverage() else { continue };
                for (g, sub) in cov.iter().zip(t.substitute_glyph_ids().iter()) {
                    map.insert(g.to_u32(), sub.get().to_u32());
                }
            }
        }
    }
}
