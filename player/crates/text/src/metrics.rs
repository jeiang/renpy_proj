//! FreeType's fixed-point size metrics, so `ascent`, `descent` and the underline match the pixel.

use crate::face::Face;

/// `FT_MulFix`: `a * b / 65536`, rounded half away from zero.
pub fn mul_fix(a: i64, b: i64) -> i64 {
    let p = a * b;
    if p >= 0 {
        (p + 0x8000) >> 16
    } else {
        -((-p + 0x8000) >> 16)
    }
}

/// `FT_MulDiv`: `a * b / c`, rounded half away from zero.
pub fn mul_div(a: i64, b: i64, c: i64) -> i64 {
    let p = a * b;
    if p >= 0 {
        (p + c / 2) / c
    } else {
        -((-p + c / 2) / c)
    }
}

/// `FT_CEIL`, `FT_FLOOR` and `FT_ROUND` on a 26.6 value, giving pixels.
pub fn ceil(x: i64) -> i64 {
    (x + 63) >> 6
}
pub fn floor(x: i64) -> i64 {
    x >> 6
}
pub fn round(x: i64) -> i64 {
    (x + 32) >> 6
}

/// The 16.16 scale FreeType uses for `size` pixels per em (`FT_Set_Char_Size(size * 64)`).
pub fn y_scale(face: &Face, size: f32) -> i64 {
    let ppem26 = (size * 64.0) as i64;
    ((ppem26 << 16) + face.upem as i64 / 2) / face.upem as i64
}

/// `face->size->metrics.ascender` and `.descender`, in 26.6.
pub fn size_ascender_descender(face: &Face, size: f32) -> (i64, i64) {
    let scale = y_scale(face, size);
    let a = mul_fix(face.ft_ascender as i64, scale);
    let d = mul_fix(face.ft_descender as i64, scale);
    (ceil(a) << 6, floor(d) << 6)
}

/// HarfBuzz's font scale: font units to 26.6 with 16.16 rounding.
pub struct HbScale {
    mult: i64,
}

impl HbScale {
    pub fn new(face: &Face, size: f32) -> HbScale {
        let scale = (size * 64.0) as i64;
        HbScale {
            mult: (scale << 16) / face.upem as i64,
        }
    }

    pub fn apply(&self, units: i64) -> i64 {
        (units * self.mult + 0x8000) >> 16
    }
}
