//! Software blits, following `pygame_Blit` (`alphablit.c`) and `SDL_UpperBlit`
//! for 32-bit surfaces, plus fills and raw format-converting copies.

use crate::buf::Img;
use crate::par;
use crate::sdl::SdlRect;

pub const BLEND_ADD: i32 = 0x1;
pub const BLEND_SUB: i32 = 0x2;
pub const BLEND_MULT: i32 = 0x3;
pub const BLEND_MIN: i32 = 0x4;
pub const BLEND_MAX: i32 = 0x5;
pub const BLEND_RGBA_ADD: i32 = 0x6;
pub const BLEND_RGBA_SUB: i32 = 0x7;
pub const BLEND_RGBA_MULT: i32 = 0x8;
pub const BLEND_RGBA_MIN: i32 = 0x9;
pub const BLEND_RGBA_MAX: i32 = 0x10;
pub const BLEND_PREMULTIPLIED: i32 = 0x11;

/// Source-surface state that changes how a blit combines pixels.
#[derive(Clone, Copy, Debug)]
pub struct SrcState {
    /// `SDL_SetSurfaceAlphaMod`, used only for sources without per-pixel alpha.
    pub alpha_mod: u8,
    /// Mapped color key; matching source pixels are skipped.
    pub colorkey: Option<u32>,
}

impl Default for SrcState {
    fn default() -> Self {
        SrcState {
            alpha_mod: 255,
            colorkey: None,
        }
    }
}

#[inline]
fn comp(sc: i32, dc: i32, sa: i32) -> i32 {
    (((sc - dc) * sa + sc) >> 8) + dc
}

/// `ALPHA_BLEND` from `pygame/surface.h`.
#[inline]
pub fn alpha_blend(s: [u8; 4], d: [u8; 4]) -> [u8; 4] {
    let sa = s[3] as i32;
    let da = d[3] as i32;
    if da != 0 {
        [
            comp(s[0] as i32, d[0] as i32, sa) as u8,
            comp(s[1] as i32, d[1] as i32, sa) as u8,
            comp(s[2] as i32, d[2] as i32, sa) as u8,
            (sa + da - (sa * da) / 255) as u8,
        ]
    } else {
        s
    }
}

#[inline]
fn special(flags: i32, s: [u8; 4], d: [u8; 4], dst_alpha: bool) -> [u8; 4] {
    // The RGBA modes fall back to the RGB modes when the destination has no
    // alpha channel.
    let (mode, with_alpha) = match flags {
        BLEND_ADD | BLEND_SUB | BLEND_MULT | BLEND_MIN | BLEND_MAX => (flags, false),
        BLEND_RGBA_ADD => (BLEND_ADD, dst_alpha),
        BLEND_RGBA_SUB => (BLEND_SUB, dst_alpha),
        BLEND_RGBA_MULT => (BLEND_MULT, dst_alpha),
        BLEND_RGBA_MIN => (BLEND_MIN, dst_alpha),
        BLEND_RGBA_MAX => (BLEND_MAX, dst_alpha),
        _ => (flags, false),
    };
    let mut out = d;
    let n = if with_alpha { 4 } else { 3 };
    for i in 0..n {
        let (sc, dc) = (s[i] as i32, d[i] as i32);
        out[i] = match mode {
            BLEND_ADD => (dc + sc).min(255),
            BLEND_SUB => (dc - sc).max(0),
            BLEND_MULT => {
                if dc != 0 && sc != 0 {
                    (dc * sc) >> 8
                } else {
                    0
                }
            }
            BLEND_MIN => dc.min(sc),
            _ => dc.max(sc),
        } as u8;
    }
    out
}

#[inline]
fn premultiplied(s: [u8; 4], d: [u8; 4]) -> [u8; 4] {
    let sa = s[3] as i32;
    let ch =
        |sc: u8, dc: u8| -> u8 { (sc as i32 + dc as i32 - ((dc as i32 * sa) >> 8)).min(255) as u8 };
    [
        ch(s[0], d[0]),
        ch(s[1], d[1]),
        ch(s[2], d[2]),
        (sa + d[3] as i32 - (sa * d[3] as i32) / 255) as u8,
    ]
}

/// Clip result: the source origin and the destination origin of a region of
/// `w` by `h` pixels. `None` when nothing is left to draw.
pub struct Clipped {
    pub sx: i32,
    pub sy: i32,
    pub dx: i32,
    pub dy: i32,
    pub w: i32,
    pub h: i32,
}

/// The clipping of `pygame_Blit`: clip the source area to the source, then
/// the destination area to the destination clip rectangle.
pub fn clip_blit(
    src_w: i32,
    src_h: i32,
    area: Option<SdlRect>,
    dst_clip: SdlRect,
    dstx: i32,
    dsty: i32,
) -> Option<Clipped> {
    let (mut dx, mut dy) = (dstx, dsty);
    let (mut sx, mut sy, mut w, mut h);
    if let Some(a) = area {
        sx = a.x;
        w = a.w;
        if sx < 0 {
            w += sx;
            dx -= sx;
            sx = 0;
        }
        w = w.min(src_w - sx);

        sy = a.y;
        h = a.h;
        if sy < 0 {
            h += sy;
            dy -= sy;
            sy = 0;
        }
        h = h.min(src_h - sy);
    } else {
        sx = 0;
        sy = 0;
        w = src_w;
        h = src_h;
    }

    let c = dst_clip;
    let d = c.x - dx;
    if d > 0 {
        w -= d;
        dx += d;
        sx += d;
    }
    let d = dx + w - c.x - c.w;
    if d > 0 {
        w -= d;
    }
    let d = c.y - dy;
    if d > 0 {
        h -= d;
        dy += d;
        sy += d;
    }
    let d = dy + h - c.y - c.h;
    if d > 0 {
        h -= d;
    }

    if w > 0 && h > 0 {
        Some(Clipped {
            sx,
            sy,
            dx,
            dy,
            w,
            h,
        })
    } else {
        None
    }
}

/// Snapshot of a source region, used when source and destination overlap.
struct Snapshot {
    data: Vec<u8>,
}

impl Snapshot {
    fn take(src: &Img, sx: usize, sy: usize, w: usize, h: usize) -> Snapshot {
        let mut data = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let row = src.row_bytes(sy + y);
            data.extend_from_slice(&row[sx * 4..(sx + w) * 4]);
        }
        Snapshot { data }
    }

    fn img(&mut self, w: usize, h: usize, fmt: crate::sdl::Format) -> Img {
        // SAFETY: `data` holds exactly `w * h * 4` bytes and outlives the Img
        // (both live in the caller's frame).
        unsafe { Img::new(self.data.as_mut_ptr(), self.data.len(), 0, w * 4, w, h, fmt) }
    }
}

/// Blits `w` x `h` pixels from (`sx`, `sy`) of `src` to (`dx`, `dy`) of `dst`.
/// The region must already be clipped to both surfaces.
pub fn blit_region(
    src: &Img,
    st: SrcState,
    sx: usize,
    sy: usize,
    w: usize,
    h: usize,
    dst: &Img,
    dx: usize,
    dy: usize,
    flags: i32,
) -> Result<(), String> {
    if !matches!(
        flags,
        0 | BLEND_ADD
            | BLEND_SUB
            | BLEND_MULT
            | BLEND_MIN
            | BLEND_MAX
            | BLEND_RGBA_ADD
            | BLEND_RGBA_SUB
            | BLEND_RGBA_MULT
            | BLEND_RGBA_MIN
            | BLEND_RGBA_MAX
            | BLEND_PREMULTIPLIED
    ) {
        return Err("Invalid argument passed to blit.".to_string());
    }

    let mut snap;
    let (src, sx, sy) = if src.overlaps(dst) {
        snap = Snapshot::take(src, sx, sy, w, h);
        (snap.img(w, h, src.fmt), 0, 0)
    } else {
        (*src, sx, sy)
    };

    let sfmt = src.fmt;
    let dfmt = dst.fmt;
    let src_alpha = sfmt.has_alpha();
    let dst_alpha = dfmt.has_alpha();
    let rgb_mask = sfmt.masks[0] | sfmt.masks[1] | sfmt.masks[2];

    par::rows(w, h, move |y| {
        // SAFETY: the region is clipped to both surfaces, so every pixel
        // address below is inside the allocation of its surface.
        let sp = unsafe { src.row_ptr(sy + y).add(sx * 4) };
        let dp = unsafe { dst.row_ptr(dy + y).add(dx * 4) };
        for x in 0..w {
            unsafe {
                let spx = (sp.add(x * 4) as *const u32).read_unaligned();
                let dpx = (dp.add(x * 4) as *const u32).read_unaligned();
                if flags == 0
                    && st
                        .colorkey
                        .is_some_and(|k| (k & rgb_mask) == (spx & rgb_mask))
                    && !src_alpha
                {
                    continue;
                }
                let mut s = sfmt.unpack(spx);
                let d = dfmt.unpack(dpx);
                let out = if flags == 0 {
                    if src_alpha {
                        alpha_blend(s, d)
                    } else if st.alpha_mod == 255 {
                        s[3] = 255;
                        s
                    } else {
                        s[3] = st.alpha_mod;
                        alpha_blend(s, d)
                    }
                } else if flags == BLEND_PREMULTIPLIED {
                    premultiplied(s, d)
                } else {
                    special(flags, s, d, dst_alpha)
                };
                (dp.add(x * 4) as *mut u32).write_unaligned(dfmt.pack(out));
            }
        }
    });
    Ok(())
}

/// Copies a clipped region without blending, converting pixel formats.
/// This is `SDL_UpperBlit` with `SDL_BLENDMODE_NONE`.
pub fn copy_region(
    src: &Img,
    sx: usize,
    sy: usize,
    w: usize,
    h: usize,
    dst: &Img,
    dx: usize,
    dy: usize,
) {
    let mut snap;
    let (src, sx, sy) = if src.overlaps(dst) {
        snap = Snapshot::take(src, sx, sy, w, h);
        (snap.img(w, h, src.fmt), 0, 0)
    } else {
        (*src, sx, sy)
    };
    let same = src.fmt == dst.fmt;
    let (sfmt, dfmt) = (src.fmt, dst.fmt);
    par::rows(w, h, move |y| {
        // SAFETY: clipped region, see `blit_region`.
        let sp = unsafe { src.row_ptr(sy + y).add(sx * 4) };
        let dp = unsafe { dst.row_ptr(dy + y).add(dx * 4) };
        if same {
            unsafe { std::ptr::copy_nonoverlapping(sp, dp, w * 4) };
        } else {
            for x in 0..w {
                unsafe {
                    let spx = (sp.add(x * 4) as *const u32).read_unaligned();
                    let out = dfmt.pack(sfmt.unpack(spx));
                    (dp.add(x * 4) as *mut u32).write_unaligned(out);
                }
            }
        }
    });
}

/// Copies all of `src` to the top left of `dst` with format conversion, clipped
/// to `dst`. Returns the copied size.
pub fn copy_all(src: &Img, dst: &Img) {
    let w = src.w.min(dst.w);
    let h = src.h.min(dst.h);
    if w > 0 && h > 0 {
        copy_region(src, 0, 0, w, h, dst, 0, 0);
    }
}

/// `SDL_FillRect`: writes `pixel` to `rect`, clipped to the surface and `clip`.
pub fn fill_rect(dst: &Img, clip: SdlRect, rect: Option<SdlRect>, pixel: u32) {
    let full = SdlRect {
        x: 0,
        y: 0,
        w: dst.w as i32,
        h: dst.h as i32,
    };
    let r = rect.unwrap_or(full);
    let x0 = r.x.max(clip.x).max(0);
    let y0 = r.y.max(clip.y).max(0);
    let x1 = (r.x.saturating_add(r.w))
        .min(clip.x.saturating_add(clip.w))
        .min(dst.w as i32);
    let y1 = (r.y.saturating_add(r.h))
        .min(clip.y.saturating_add(clip.h))
        .min(dst.h as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (x0, y0, w, h) = (
        x0 as usize,
        y0 as usize,
        (x1 - x0) as usize,
        (y1 - y0) as usize,
    );
    let dst = *dst;
    par::rows(w, h, move |y| {
        // SAFETY: clipped to the surface.
        let dp = unsafe { dst.row_ptr(y0 + y).add(x0 * 4) as *mut u32 };
        for x in 0..w {
            unsafe { dp.add(x).write_unaligned(pixel) };
        }
    });
}
