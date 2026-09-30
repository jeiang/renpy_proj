//! `renpy.pygame.draw`, following `renpy/pygame/draw.pyx`.

use pyo3::prelude::*;

use crate::gfx::Pen;
use crate::pyutil::*;
use crate::surface::Surface;

fn pen(s: &Bound<'_, Surface>, color: &Bound<'_, PyAny>) -> PyResult<Pen> {
    let surf = s.get();
    let c = match parse_color(color)? {
        ColorArg::Rgba(c) => c,
        ColorArg::Pixel(p) => surf.format().unpack(p),
    };
    Ok(Pen {
        img: surf.img(),
        clip: surf.clip_rect(),
        color: c,
    })
}

fn point(o: &Bound<'_, PyAny>) -> PyResult<(i32, i32)> {
    let (x, y): (f64, f64) = o.extract()?;
    Ok((x as i32, y as i32))
}

fn points(o: &Bound<'_, PyAny>) -> PyResult<Vec<(i32, i32)>> {
    o.try_iter()?.map(|p| point(&p?)).collect()
}

/// `Rect(x, y, w, h).clip(surface.get_rect())` as a Python Rect.
fn dirty<'py>(
    py: Python<'py>,
    s: &Surface,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> PyResult<Bound<'py, PyAny>> {
    let (sw, sh) = s.size();
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = x.saturating_add(w).min(sw as i32);
    let y1 = y.saturating_add(h).min(sh as i32);
    if w > 0 && h > 0 && x1 > x0 && y1 > y0 {
        make_rect(py, x0, y0, x1 - x0, y1 - y0)
    } else {
        make_rect(py, 0, 0, 0, 0)
    }
}

/// Bounding box of 1x1 rects at the points, as stock builds with `union_ip`.
fn bounds(pts: &[(i32, i32)], size: i32) -> (i32, i32, i32, i32) {
    let x0 = pts.iter().map(|p| p.0).min().unwrap_or(0);
    let y0 = pts.iter().map(|p| p.1).min().unwrap_or(0);
    let x1 = pts.iter().map(|p| p.0 + size).max().unwrap_or(0);
    let y1 = pts.iter().map(|p| p.1 + size).max().unwrap_or(0);
    (x0, y0, x1 - x0, y1 - y0)
}

fn rect_of(o: &Bound<'_, PyAny>) -> PyResult<(i32, i32, i32, i32)> {
    let r = parse_rect(o, None)?;
    Ok((r.x, r.y, r.w, r.h))
}

#[pyfunction]
#[pyo3(signature = (surface, color, rect, width=0))]
pub fn rect<'py>(
    py: Python<'py>,
    surface: &Bound<'py, Surface>,
    color: &Bound<'py, PyAny>,
    rect: &Bound<'py, PyAny>,
    width: i32,
) -> PyResult<Bound<'py, PyAny>> {
    let p = pen(surface, color)?;
    let (x, y, w, h) = rect_of(rect)?;
    if width == 0 {
        p.box_(x, y, x + w, y + h);
    } else {
        p.rectangle(x, y, x + w, y + h);
        for n in 1..width {
            p.rectangle(x - n, y - n, x - n + w + n * 2, y - n + h + n * 2);
            p.rectangle(x + n, y + n, x + n + w - n * 2, y + n + h - n * 2);
        }
    }
    surface.get().touch();
    dirty(
        py,
        surface.get(),
        x - width,
        y - width,
        w + width * 2,
        h + width * 2,
    )
}

#[pyfunction]
#[pyo3(signature = (surface, color, pointlist, width=0))]
pub fn polygon<'py>(
    py: Python<'py>,
    surface: &Bound<'py, Surface>,
    color: &Bound<'py, PyAny>,
    pointlist: &Bound<'py, PyAny>,
    width: i32,
) -> PyResult<Bound<'py, PyAny>> {
    if width != 0 {
        return lines(py, surface, color, true, pointlist, width);
    }
    let p = pen(surface, color)?;
    let pts = points(pointlist)?;
    p.filled_polygon(&pts);
    surface.get().touch();
    let (x, y, w, h) = bounds(&pts, 1);
    dirty(py, surface.get(), x, y, w, h)
}

#[pyfunction]
#[pyo3(signature = (surface, color, pos, radius, width=0))]
pub fn circle<'py>(
    py: Python<'py>,
    surface: &Bound<'py, Surface>,
    color: &Bound<'py, PyAny>,
    pos: &Bound<'py, PyAny>,
    radius: i32,
    width: i32,
) -> PyResult<Bound<'py, PyAny>> {
    let p = pen(surface, color)?;
    let (x, y) = point(pos)?;
    if width == 0 {
        p.filled_circle(x, y, radius);
        surface.get().touch();
        return dirty(
            py,
            surface.get(),
            x - radius,
            y - radius,
            radius * 2,
            radius * 2,
        );
    }
    p.circle(x, y, radius);
    for n in 1..width {
        p.circle(x, y, radius - n);
        p.circle(x + 1, y, radius - n);
        p.circle(x - 1, y, radius - n);
    }
    surface.get().touch();
    dirty(
        py,
        surface.get(),
        x - radius - width,
        y - radius - width,
        radius * 2 + width,
        radius * 2 + width,
    )
}

#[pyfunction]
#[pyo3(signature = (surface, color, rect, width=0))]
pub fn ellipse<'py>(
    py: Python<'py>,
    surface: &Bound<'py, Surface>,
    color: &Bound<'py, PyAny>,
    rect: &Bound<'py, PyAny>,
    width: i32,
) -> PyResult<Bound<'py, PyAny>> {
    let p = pen(surface, color)?;
    // Stock reads (x, y, rx, ry) with the center at (x, y).
    let (x, y, rx, ry) = rect_of(rect)?;
    if width == 0 {
        p.filled_ellipse(x, y, rx, ry);
        surface.get().touch();
        return dirty(py, surface.get(), x - rx, y - ry, rx * 2, ry * 2);
    }
    p.ellipse(x, y, rx, ry);
    for n in 1..width {
        p.ellipse(x, y, rx - n, ry - n);
        p.ellipse(x + 1, y, rx - n, ry - n);
        p.ellipse(x - 1, y, rx - n, ry - n);
    }
    surface.get().touch();
    dirty(
        py,
        surface.get(),
        x - rx - width,
        y - ry - width,
        rx * 2 + width,
        ry * 2 + width,
    )
}

#[pyfunction]
#[pyo3(signature = (surface, color, rect, start_angle, stop_angle, width=1))]
pub fn arc(
    py: Python<'_>,
    surface: &Bound<'_, Surface>,
    color: &Bound<'_, PyAny>,
    rect: &Bound<'_, PyAny>,
    start_angle: f64,
    stop_angle: f64,
    width: i32,
) -> PyResult<()> {
    let _ = (surface, color, rect, start_angle, stop_angle, width);
    Err(pygame_error(py, "Not implemented."))
}

fn one_line(p: &Pen, a: (i32, i32), b: (i32, i32), width: i32) {
    // Stock skips zero-length lines.
    if a != b {
        p.thick_line(a.0, a.1, b.0, b.1, width);
    }
}

#[pyfunction]
#[pyo3(signature = (surface, color, start_pos, end_pos, width=1))]
pub fn line<'py>(
    py: Python<'py>,
    surface: &Bound<'py, Surface>,
    color: &Bound<'py, PyAny>,
    start_pos: &Bound<'py, PyAny>,
    end_pos: &Bound<'py, PyAny>,
    width: i32,
) -> PyResult<Bound<'py, PyAny>> {
    let p = pen(surface, color)?;
    let (a, b) = (point(start_pos)?, point(end_pos)?);
    one_line(&p, a, b, width);
    surface.get().touch();
    let (x, y, w, h) = bounds(&[a, b], width);
    dirty(py, surface.get(), x, y, w, h)
}

#[pyfunction]
#[pyo3(signature = (surface, color, closed, pointlist, width=1))]
pub fn lines<'py>(
    py: Python<'py>,
    surface: &Bound<'py, Surface>,
    color: &Bound<'py, PyAny>,
    closed: bool,
    pointlist: &Bound<'py, PyAny>,
    width: i32,
) -> PyResult<Bound<'py, PyAny>> {
    let p = pen(surface, color)?;
    let pts = points(pointlist)?;
    if pts.is_empty() {
        return Err(pyo3::exceptions::PyIndexError::new_err(
            "list index out of range",
        ));
    }
    for pair in pts.windows(2) {
        one_line(&p, pair[0], pair[1], width);
    }
    if closed {
        one_line(&p, pts[pts.len() - 1], pts[0], width);
    }
    surface.get().touch();
    let (x, y, w, h) = bounds(&pts, width);
    dirty(py, surface.get(), x, y, w, h)
}

#[pyfunction]
#[pyo3(signature = (surface, color, startpos, endpos, blend=1))]
pub fn aaline<'py>(
    py: Python<'py>,
    surface: &Bound<'py, Surface>,
    color: &Bound<'py, PyAny>,
    startpos: &Bound<'py, PyAny>,
    endpos: &Bound<'py, PyAny>,
    blend: i32,
) -> PyResult<Bound<'py, PyAny>> {
    let _ = blend;
    let p = pen(surface, color)?;
    let (a, b) = (point(startpos)?, point(endpos)?);
    p.aaline(a.0, a.1, b.0, b.1);
    surface.get().touch();
    dirty(py, surface.get(), a.0, a.1, b.0 - a.0, b.1 - a.1)
}

#[pyfunction]
#[pyo3(signature = (surface, color, closed, pointlist, blend=1))]
pub fn aalines<'py>(
    py: Python<'py>,
    surface: &Bound<'py, Surface>,
    color: &Bound<'py, PyAny>,
    closed: bool,
    pointlist: &Bound<'py, PyAny>,
    blend: i32,
) -> PyResult<Bound<'py, PyAny>> {
    let _ = blend;
    let p = pen(surface, color)?;
    let pts = points(pointlist)?;
    if pts.is_empty() {
        return Err(pyo3::exceptions::PyIndexError::new_err(
            "list index out of range",
        ));
    }
    for pair in pts.windows(2) {
        p.aaline(pair[0].0, pair[0].1, pair[1].0, pair[1].1);
    }
    if closed {
        let (a, b) = (pts[pts.len() - 1], pts[0]);
        p.aaline(a.0, a.1, b.0, b.1);
    }
    surface.get().touch();
    let (x, y, w, h) = bounds(&pts, 1);
    dirty(py, surface.get(), x, y, w, h)
}
