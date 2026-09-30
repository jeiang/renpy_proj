//! `renpy.pygame.transform`.

use pyo3::prelude::*;

use crate::buf::Img;
use crate::pyutil::{parse_size, pygame_error};
use crate::surface::Surface;

fn new_like(py: Python<'_>, s: &Surface, w: u32, h: u32) -> PyResult<Surface> {
    Surface::alloc(w, h, s.format()).map_err(|e| pygame_error(py, e))
}

#[pyfunction]
pub fn flip(py: Python<'_>, surface: &Bound<'_, Surface>, xbool: bool, ybool: bool) -> PyResult<Surface> {
    let s = surface.get();
    let (w, h) = s.size();
    let rv = new_like(py, s, w, h)?;
    let (src, dst) = (s.img(), rv.img());
    for y in 0..h as usize {
        let dy = if ybool { h as usize - 1 - y } else { y };
        for x in 0..w as usize {
            let dx = if xbool { w as usize - 1 - x } else { x };
            dst.put(dx, dy, src.px(x, y));
        }
    }
    Ok(rv)
}

/// Nearest-neighbor stretch of all of `src` onto all of `dst`.
fn stretch(src: &Img, dst: &Img) {
    if src.w == 0 || src.h == 0 || dst.w == 0 || dst.h == 0 {
        return;
    }
    let same = src.fmt == dst.fmt;
    for y in 0..dst.h {
        let sy = y * src.h / dst.h;
        for x in 0..dst.w {
            let sx = x * src.w / dst.w;
            let p = src.px(sx, sy);
            dst.put(x, y, if same { p } else { dst.fmt.pack(src.fmt.unpack(p)) });
        }
    }
}

#[pyfunction]
#[pyo3(signature = (surface, size, DestSurface=None))]
#[allow(non_snake_case)]
pub fn scale(
    py: Python<'_>,
    surface: &Bound<'_, Surface>,
    size: &Bound<'_, PyAny>,
    DestSurface: Option<Bound<'_, Surface>>,
) -> PyResult<Py<Surface>> {
    let s = surface.get();
    let out = match DestSurface {
        Some(d) => d.unbind(),
        None => {
            let (w, h) = parse_size(size)?;
            Py::new(py, new_like(py, s, w, h)?)?
        }
    };
    let (src, dst) = (s.img(), out.bind(py).get().img());
    py.detach(|| stretch(&src, &dst));
    out.bind(py).get().touch();
    Ok(out)
}

/// Bilinear resample of `src` onto a `dw` x `dh` destination.
fn smooth(src: &Img, dst: &Img) {
    let (sw, sh, dw, dh) = (src.w, src.h, dst.w, dst.h);
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return;
    }
    let fmt = src.fmt;
    for y in 0..dh {
        let fy = ((y as f64 + 0.5) * sh as f64 / dh as f64 - 0.5).clamp(0.0, (sh - 1) as f64);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let ty = fy - y0 as f64;
        for x in 0..dw {
            let fx = ((x as f64 + 0.5) * sw as f64 / dw as f64 - 0.5).clamp(0.0, (sw - 1) as f64);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let tx = fx - x0 as f64;
            let p = [fmt.unpack(src.px(x0, y0)), fmt.unpack(src.px(x1, y0)), fmt.unpack(src.px(x0, y1)), fmt.unpack(src.px(x1, y1))];
            let mut out = [0u8; 4];
            for c in 0..4 {
                let top = p[0][c] as f64 * (1.0 - tx) + p[1][c] as f64 * tx;
                let bot = p[2][c] as f64 * (1.0 - tx) + p[3][c] as f64 * tx;
                out[c] = (top * (1.0 - ty) + bot * ty).round() as u8;
            }
            dst.put(x, y, dst.fmt.pack(out));
        }
    }
}

#[pyfunction]
#[pyo3(signature = (surface, size, DestSurface=None))]
#[allow(non_snake_case)]
pub fn smoothscale(
    py: Python<'_>,
    surface: &Bound<'_, Surface>,
    size: &Bound<'_, PyAny>,
    DestSurface: Option<Bound<'_, Surface>>,
) -> PyResult<Py<Surface>> {
    let s = surface.get();
    let (w, h) = parse_size(size)?;
    let rv = new_like(py, s, w, h)?;
    let (src, dst) = (s.img(), rv.img());
    py.detach(|| smooth(&src, &dst));
    let rv = Py::new(py, rv)?;
    if let Some(d) = DestSurface {
        d.get().copy_from(py, rv.bind(py))?;
        // Stock returns the scaled surface, not the destination.
    }
    Ok(rv)
}

#[pyfunction]
#[pyo3(signature = (surface, angle, scale, smooth=1))]
pub fn rotozoom(py: Python<'_>, surface: &Bound<'_, Surface>, angle: f64, scale: f64, smooth: i32) -> PyResult<Surface> {
    let s = surface.get();
    let (w, h) = s.size();
    if scale <= 0.0 {
        return Err(pygame_error(py, "rotozoom: scale must be positive."));
    }
    let rad = angle.to_radians();
    let (sn, cs) = (rad.sin(), rad.cos());
    let (x, y) = (w as f64 / 2.0, h as f64 / 2.0);
    let (cx, cy, sx, sy) = (cs * x * scale, cs * y * scale, sn * x * scale, sn * y * scale);
    let hw = [cx + sy, cx - sy, -cx + sy, -cx - sy].iter().fold(0.0f64, |m, v| m.max(v.abs())).ceil().max(1.0);
    let hh = [sx + cy, sx - cy, -sx + cy, -sx - cy].iter().fold(0.0f64, |m, v| m.max(v.abs())).ceil().max(1.0);
    let (dw, dh) = if angle == 0.0 {
        (((w as f64 * scale).floor() as u32).max(1), ((h as f64 * scale).floor() as u32).max(1))
    } else {
        (hw as u32 * 2, hh as u32 * 2)
    };
    let rv = Surface::alloc(dw, dh, s.format()).map_err(|e| pygame_error(py, e))?;
    if w == 0 || h == 0 {
        return Ok(rv);
    }
    let (src, dst) = (s.img(), rv.img());
    let fmt = src.fmt;
    let (dcx, dcy) = (dw as f64 / 2.0, dh as f64 / 2.0);
    let (scx, scy) = (w as f64 / 2.0, h as f64 / 2.0);
    py.detach(|| {
        for dy in 0..dh as usize {
            for dx in 0..dw as usize {
                // Inverse map: rotate the destination offset by +angle back to the source.
                let (ox, oy) = (dx as f64 + 0.5 - dcx, dy as f64 + 0.5 - dcy);
                let (rx, ry) = (ox * cs - oy * sn, ox * sn + oy * cs);
                let (fx, fy) = (rx / scale + scx - 0.5, ry / scale + scy - 0.5);
                if smooth != 0 {
                    if fx < -0.5 || fy < -0.5 || fx > w as f64 - 0.5 || fy > h as f64 - 0.5 {
                        continue;
                    }
                    let (cx0, cy0) = (fx.clamp(0.0, (w - 1) as f64), fy.clamp(0.0, (h - 1) as f64));
                    let (x0, y0) = (cx0.floor() as usize, cy0.floor() as usize);
                    let (x1, y1) = ((x0 + 1).min(w as usize - 1), (y0 + 1).min(h as usize - 1));
                    let (tx, ty) = (cx0 - x0 as f64, cy0 - y0 as f64);
                    let p = [fmt.unpack(src.px(x0, y0)), fmt.unpack(src.px(x1, y0)), fmt.unpack(src.px(x0, y1)), fmt.unpack(src.px(x1, y1))];
                    let mut out = [0u8; 4];
                    for c in 0..4 {
                        let top = p[0][c] as f64 * (1.0 - tx) + p[1][c] as f64 * tx;
                        let bot = p[2][c] as f64 * (1.0 - tx) + p[3][c] as f64 * tx;
                        out[c] = (top * (1.0 - ty) + bot * ty).round() as u8;
                    }
                    dst.put(dx, dy, fmt.pack(out));
                } else {
                    let (ix, iy) = ((fx + 0.5).floor(), (fy + 0.5).floor());
                    if ix < 0.0 || iy < 0.0 || ix >= w as f64 || iy >= h as f64 {
                        continue;
                    }
                    dst.put(dx, dy, src.px(ix as usize, iy as usize));
                }
            }
        }
    });
    Ok(rv)
}

#[pyfunction]
pub fn rotate(py: Python<'_>, surface: &Bound<'_, Surface>, angle: f64) -> PyResult<Surface> {
    rotozoom(py, surface, angle, 1.0, 0)
}

/// Ren'Py test helper: counts pixels that differ, writing `same` or `different` colors to `dest`.
#[pyfunction]
pub fn _diff(
    py: Python<'_>,
    dest: &Bound<'_, Surface>,
    a: &Bound<'_, Surface>,
    b: &Bound<'_, Surface>,
    same_color: &Bound<'_, PyAny>,
    different_color: &Bound<'_, PyAny>,
) -> PyResult<usize> {
    let (d, a, b) = (dest.get(), a.get(), b.get());
    if d.size() != a.size() || d.size() != b.size() {
        return Err(pygame_error(py, "Surface sizes do not match."));
    }
    let map = |c: &Bound<'_, PyAny>| -> PyResult<u32> {
        Ok(match crate::pyutil::parse_color(c)? {
            crate::pyutil::ColorArg::Pixel(p) => p,
            crate::pyutil::ColorArg::Rgba(c) => d.format().pack(c),
        })
    };
    let (same, diff) = (map(same_color)?, map(different_color)?);
    let (di, ai, bi) = (d.img(), a.img(), b.img());
    let mut count = 0;
    for y in 0..di.h {
        for x in 0..di.w {
            if ai.px(x, y) == bi.px(x, y) {
                di.put(x, y, same);
            } else {
                di.put(x, y, diff);
                count += 1;
            }
        }
    }
    d.touch();
    Ok(count)
}
