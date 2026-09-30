//! Pixel algorithms of `_renpy` (`src/core.c` in Ren'Py), on 32-bit surfaces.
//!
//! Channels are "first byte through fourth byte"; the callers in
//! `renpy/display/module.py` map them to colors. A 32-bit surface without alpha
//! is handled as 4 bytes per pixel (stock treats it as 3, which is a bug).

use crate::buf::Img;
use crate::par;

#[inline]
fn lerp_packed(a: u32, b: u32, mul: u32) -> u32 {
    // The `I` macro of core.c: two 8-bit lanes in one word.
    ((b.wrapping_sub(a)).wrapping_mul(mul) >> 8).wrapping_add(a) & 0x00ff_00ff
}

pub fn pixellate(src: &Img, dst: &Img, aw: i32, ah: i32, ow: i32, oh: i32) {
    if aw <= 0 || ah <= 0 {
        return;
    }
    let (srcw, srch, dstw, dsth) = (src.w as i32, src.h as i32, dst.w as i32, dst.h as i32);
    let vw = (srcw + aw - 1) / aw;
    let vh = (srch + ah - 1) / ah;
    let (src, dst) = (*src, *dst);
    par::lines(vh.max(0) as usize, srcw as usize * ah as usize, move |y| {
        let y = y as i32;
        let srcy = ah * y;
        let dsty = oh * y;
        let srcylimit = (srcy + ah).min(srch);
        let dstylimit = (dsty + oh).min(dsth);
        for x in 0..vw {
            let srcx = aw * x;
            let dstx = ow * x;
            let srcxlimit = (srcx + aw).min(srcw);
            // Stock uses the output height here; kept for identical output.
            let dstxlimit = (dstx + oh).min(dstw);
            let mut sum = [0u32; 4];
            let mut n = 0u32;
            for j in srcy..srcylimit {
                for i in srcx..srcxlimit {
                    let p = src.px(i as usize, j as usize).to_le_bytes();
                    for c in 0..4 {
                        sum[c] += p[c] as u32;
                    }
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            let avg = u32::from_le_bytes([
                (sum[0] / n) as u8,
                (sum[1] / n) as u8,
                (sum[2] / n) as u8,
                (sum[3] / n) as u8,
            ]);
            for j in dsty..dstylimit {
                for i in dstx..dstxlimit {
                    dst.put(i as usize, j as usize, avg);
                }
            }
        }
    });
}

pub fn map(src: &Img, dst: &Img, maps: [&[u8]; 4]) {
    let (src, dst) = (*src, *dst);
    let maps = maps.map(|m| {
        let mut t = [0u8; 256];
        t.copy_from_slice(&m[..256]);
        t
    });
    par::rows(src.w, src.h, move |y| {
        for x in 0..src.w {
            let p = src.px(x, y).to_le_bytes();
            let q = [
                maps[0][p[0] as usize],
                maps[1][p[1] as usize],
                maps[2][p[2] as usize],
                maps[3][p[3] as usize],
            ];
            dst.put(x, y, u32::from_le_bytes(q));
        }
    });
}

pub fn linmap(src: &Img, dst: &Img, mul: [i32; 4]) {
    let (src, dst) = (*src, *dst);
    par::rows(src.w, src.h, move |y| {
        for x in 0..src.w {
            let p = src.px(x, y).to_le_bytes();
            let mut q = [0u8; 4];
            for c in 0..4 {
                q[c] = ((p[c] as i32 * mul[c]) >> 8) as u8;
            }
            dst.put(x, y, u32::from_le_bytes(q));
        }
    });
}

fn blur_filters(sigma: f32, n: i32) -> (i32, i32, i32) {
    let nf = n as f32;
    let mut wl = ((12.0 * sigma * sigma / nf + 1.0) as f64).sqrt().floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let wlf = wl as f32;
    let m = ((12.0 * sigma * sigma - nf * wlf * wlf - 4.0 * nf * wlf - 3.0 * nf)
        / (-4.0 * wlf - 4.0))
        .round() as i32;
    (wl, wu, m)
}

/// One-dimensional box blur of radius `radius` with edge pixels repeated.
pub fn linblur(src: &Img, dst: &Img, radius: i32, vertical: bool) {
    let (src, dst) = (*src, *dst);
    let radius = radius.max(0) as i64;
    let (lines, cols) = if vertical {
        (dst.w, dst.h)
    } else {
        (dst.h, dst.w)
    };
    if cols == 0 {
        return;
    }
    let divisor = (radius * 2 + 1) as i32;
    par::lines(lines, cols, move |r| {
        let at =
            |img: &Img, c: usize| -> isize { if vertical { img.at(r, c) } else { img.at(c, r) } };
        let get = |k: i64| -> [i32; 4] {
            let c = k.clamp(0, cols as i64 - 1) as usize;
            let p = src.rd32(at(&src, c)).to_le_bytes();
            [p[0] as i32, p[1] as i32, p[2] as i32, p[3] as i32]
        };
        let mut sum = [0i32; 4];
        for k in -radius..=radius {
            let p = get(k);
            for c in 0..4 {
                sum[c] += p[c];
            }
        }
        for c in 0..cols {
            let out = [
                (sum[0] / divisor) as u8,
                (sum[1] / divisor) as u8,
                (sum[2] / divisor) as u8,
                (sum[3] / divisor) as u8,
            ];
            dst.wr32(at(&dst, c), u32::from_le_bytes(out));
            let add = get(c as i64 + radius + 1);
            let sub = get(c as i64 - radius);
            for ch in 0..4 {
                sum[ch] += add[ch] - sub[ch];
            }
        }
    });
}

pub fn blur(src: &Img, wrk: &Img, dst: &Img, xrad: f32, yrad: f32) {
    let n = 3;
    let (xl, xu, xm) = blur_filters(xrad, n);
    let (yl, yu, ym) = if xrad != yrad {
        blur_filters(yrad, n)
    } else {
        (xl, xu, xm)
    };
    let mut cur = *src;
    for i in 0..n {
        let xr = if i < xm { xl } else { xu };
        linblur(&cur, wrk, xr, false);
        let yr = if i < ym { yl } else { yu };
        linblur(wrk, dst, yr, true);
        cur = *dst;
    }
}

pub fn alpha_munge(src: &Img, dst: &Img, srcchan: i32, dstchan: i32, amap: &[u8]) {
    let (src, dst) = (*src, *dst);
    let mut t = [0u8; 256];
    t.copy_from_slice(&amap[..256]);
    par::rows(dst.w, dst.h, move |y| {
        for x in 0..dst.w {
            let v = src.rd8(src.at(x, y) + srcchan as isize);
            dst.wr8(dst.at(x, y) + dstchan as isize, t[v as usize]);
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub fn bilinear(
    src: &Img,
    dst: &Img,
    sxoff: f32,
    syoff: f32,
    swidth: f32,
    sheight: f32,
    dxoff: f32,
    dyoff: f32,
    dwidth: f32,
    dheight: f32,
    precise: bool,
) {
    let (src, dst) = (*src, *dst);
    let (xdelta, ydelta) = if precise {
        (
            if dwidth > 1.0 {
                256.0 * (swidth - 1.0) / (dwidth - 1.0)
            } else {
                0.0
            },
            if dheight > 1.0 {
                256.0 * (sheight - 1.0) / (dheight - 1.0)
            } else {
                0.0
            },
        )
    } else {
        (
            255.0 * (swidth - 1.0) / dwidth,
            255.0 * (sheight - 1.0) / dheight,
        )
    };
    par::rows(dst.w, dst.h, move |y| {
        let sline = (syoff * 256.0 + (y as f32 + dyoff) * ydelta) as i32;
        let s1frac = (sline & 255);
        let s0frac = 256 - s1frac;
        let row_off = (sline >> 8) as isize * src.pitch as isize;
        let mut scol = sxoff * 256.0 + dxoff * xdelta;
        for x in 0..dst.w {
            let ic = scol as i32;
            let xfrac = 256 - (ic & 255);
            let s0 = row_off + (ic >> 8) as isize * 4;
            let s1 = s0 + src.pitch as isize;
            let mut out = [0u8; 4];
            for c in 0..4isize {
                let left =
                    (((src.rd8(s0 + c) as i32 * s0frac) + (src.rd8(s1 + c) as i32 * s1frac)) >> 8)
                        * xfrac;
                let right = (((src.rd8(s0 + 4 + c) as i32 * s0frac)
                    + (src.rd8(s1 + 4 + c) as i32 * s1frac))
                    >> 8)
                    * (256 - xfrac);
                out[c as usize] = (((left + right) as u32 & 0xffff) >> 8) as u8;
            }
            dst.put(x, y, u32::from_le_bytes(out));
            scol += xdelta;
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub fn transform(
    src: &Img,
    dst: &Img,
    corner_x: f32,
    corner_y: f32,
    mut xdx: f32,
    mut ydx: f32,
    mut xdy: f32,
    mut ydy: f32,
    ashift: u32,
    a: f32,
    precise: bool,
) {
    const EPSILON: f64 = 1.0 / 256.0;
    let (src, dst) = (*src, *dst);
    let amul = (a * 256.0) as u32;
    let mut maxsx = src.w as f64;
    let mut maxsy = src.h as f64;
    if !precise {
        maxsx -= EPSILON;
        maxsy -= EPSILON;
        let fix = |d: &mut f32| {
            if *d != 0.0 && ((((1.0f64 / *d as f64) as f32) % 1.0).abs() as f64) < EPSILON {
                let v = *d as f64;
                *d = (v - (v / v.abs()) * EPSILON) as f32;
            }
        };
        fix(&mut xdx);
        fix(&mut xdy);
        fix(&mut ydx);
        fix(&mut ydy);
    }
    let dstw = dst.w as f64;
    par::rows(dst.w, dst.h, move |y| {
        let leftsx = (corner_x + y as f32 * xdy) as f64;
        let leftsy = (corner_y + y as f32 * ydy) as f64;
        let mut minx = 0.0f64;
        let mut maxx = dstw - 1.0;
        if xdx != 0.0 {
            let x1 = (0.0 - leftsx) / xdx as f64;
            let x2 = (maxsx - leftsx) / xdx as f64;
            if x1 < x2 {
                minx = x1.max(minx);
                maxx = x2.min(maxx);
            } else {
                minx = x2.max(minx);
                maxx = x1.min(maxx);
            }
        } else if leftsx < 0.0 || leftsx > maxsx {
            return;
        }
        if ydx != 0.0 {
            let x1 = (0.0 - leftsy) / ydx as f64;
            let x2 = (maxsy - leftsy) / ydx as f64;
            if x1 < x2 {
                minx = x1.max(minx);
                maxx = x2.min(maxx);
            } else {
                minx = x2.max(minx);
                maxx = x1.min(maxx);
            }
        } else if leftsy < 0.0 || leftsy > maxsy {
            return;
        }
        minx = minx.ceil();
        maxx = maxx.floor();
        if minx >= maxx {
            return;
        }
        let x0 = minx as i32;
        let x1 = maxx as i32;
        let mut sxi = ((leftsx + minx * xdx as f64) * 65536.0) as i32;
        let mut syi = ((leftsy + minx * ydx as f64) * 65536.0) as i32;
        let dsxi = (xdx * 65536.0) as i32;
        let dsyi = (ydx * 65536.0) as i32;
        let pitch = src.pitch as isize;
        for x in x0..=x1 {
            let px = (sxi >> 16) as isize;
            let py = (syi >> 16) as isize;
            let yfrac = ((syi >> 8) & 0xff) as u32;
            let xfrac = ((sxi >> 8) & 0xff) as u32;
            let sp = py * pitch + px * 4;
            let pal = src.rd32(sp);
            let pbl = src.rd32(sp + 4);
            let pcl = src.rd32(sp + pitch);
            let pdl = src.rd32(sp + pitch + 4);
            let (pah, pbh, pch, pdh) = (
                (pal >> 8) & 0x00ff_00ff,
                (pbl >> 8) & 0x00ff_00ff,
                (pcl >> 8) & 0x00ff_00ff,
                (pdl >> 8) & 0x00ff_00ff,
            );
            let (pal, pbl, pcl, pdl) = (
                pal & 0x00ff_00ff,
                pbl & 0x00ff_00ff,
                pcl & 0x00ff_00ff,
                pdl & 0x00ff_00ff,
            );
            let rh = lerp_packed(
                lerp_packed(pah, pch, yfrac),
                lerp_packed(pbh, pdh, yfrac),
                xfrac,
            );
            let rl = lerp_packed(
                lerp_packed(pal, pcl, yfrac),
                lerp_packed(pbl, pdl, yfrac),
                xfrac,
            );
            let mut alpha = (((rh << 8) | rl) >> ashift) & 0xff;
            alpha = (alpha * amul) >> 8;
            let o = dst.at(x as usize, y);
            let dv = dst.rd32(o);
            let mut dh = (dv >> 8) & 0x00ff_00ff;
            let mut dl = dv & 0x00ff_00ff;
            dl = lerp_packed(dl, rl, alpha);
            dh = lerp_packed(dh, rh, alpha);
            dst.wr32(o, (dh << 8) | dl);
            sxi = sxi.wrapping_add(dsxi);
            syi = syi.wrapping_add(dsyi);
        }
    });
}

pub fn blend(a: &Img, b: &Img, dst: &Img, alpha: i32) {
    let (a, b, dst) = (*a, *b, *dst);
    let alpha = alpha as u32;
    par::rows(dst.w, dst.h, move |y| {
        for x in 0..dst.w {
            let sal = a.px(x, y);
            let sbl = b.px(x, y);
            let out = lerp_packed(sal & 0x00ff_00ff, sbl & 0x00ff_00ff, alpha)
                | (lerp_packed((sal >> 8) & 0x00ff_00ff, (sbl >> 8) & 0x00ff_00ff, alpha) << 8);
            dst.put(x, y, out);
        }
    });
}

pub fn imageblend(a: &Img, b: &Img, dst: &Img, img: &Img, alpha_off: i32, amap: &[u8]) {
    let (a, b, dst, img) = (*a, *b, *dst, *img);
    let mut t = [0u8; 256];
    t.copy_from_slice(&amap[..256]);
    par::rows(dst.w, dst.h, move |y| {
        for x in 0..dst.w {
            let alpha = t[img.rd8(img.at(x, y) + alpha_off as isize) as usize] as u32;
            let sal = a.px(x, y);
            let sbl = b.px(x, y);
            let out = lerp_packed(sal & 0x00ff_00ff, sbl & 0x00ff_00ff, alpha)
                | (lerp_packed((sal >> 8) & 0x00ff_00ff, (sbl >> 8) & 0x00ff_00ff, alpha) << 8);
            dst.put(x, y, out);
        }
    });
}

pub fn colormatrix(src: &Img, dst: &Img, c: [[f32; 5]; 4]) {
    let (src, dst) = (*src, *dst);
    let o = [
        (c[0][4] * 255.0) as i32,
        (c[1][4] * 255.0) as i32,
        (c[2][4] * 255.0) as i32,
        (c[3][4] * 255.0) as i32,
    ];
    par::rows(dst.w, dst.h, move |y| {
        for x in 0..dst.w {
            let s = src.px(x, y).to_le_bytes();
            let (s0, s1, s2, s3) = (s[0] as f32, s[1] as f32, s[2] as f32, s[3] as f32);
            let mut q = [0u8; 4];
            for i in 0..4 {
                let r = o[i] + (c[i][0] * s0 + c[i][1] * s1 + c[i][2] * s2 + c[i][3] * s3) as i32;
                q[i] = r.clamp(0, 255) as u8;
            }
            dst.put(x, y, u32::from_le_bytes(q));
        }
    });
}

/// Writes one byte per destination pixel, as stock does.
pub fn staticgray(src: &Img, dst: &Img, mul: [i32; 4], shift: u32, vmap: &[u8]) {
    let (src, dst) = (*src, *dst);
    par::rows(dst.w, dst.h, move |y| {
        for x in 0..dst.w {
            let p = src.px(x, y).to_le_bytes();
            let mut sum = 0i32;
            for c in 0..4 {
                sum += p[c] as i32 * mul[c];
            }
            let idx = (sum >> shift) as usize;
            dst.wr8(
                (y * dst.pitch + x) as isize,
                vmap.get(idx).copied().unwrap_or(0),
            );
        }
    });
}

/// Output byte order is A, B, G, R, as in stock.
pub fn premultiply_alpha(src: &Img, dst: &Img) {
    let (src, dst) = (*src, *dst);
    par::rows(dst.w, dst.h, move |y| {
        for x in 0..dst.w {
            let p = src.px(x, y).to_le_bytes();
            let (r, g, b, a) = (p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32);
            dst.put(
                x,
                y,
                u32::from_le_bytes([
                    a as u8,
                    ((b * a) / 255) as u8,
                    ((g * a) / 255) as u8,
                    ((r * a) / 255) as u8,
                ]),
            );
        }
    });
}
