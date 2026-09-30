//! The `_renpy` module: Python entry points over [`crate::ops`].

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;
use pyo3::types::PyBytes;

use crate::buf::Img;
use crate::image::encode_png;
use crate::ops;
use crate::surface::Surface;

fn surf<'a>(o: &'a Bound<'_, PyAny>, fname: &str, which: &str) -> PyResult<&'a Surface> {
    match o.cast::<Surface>() {
        Ok(s) => Ok(s.get()),
        Err(_) => Err(PyException::new_err(format!(
            "{fname} requires a pygame Surface as its {which} argument."
        ))),
    }
}

fn need(cond: bool, msg: String) -> PyResult<()> {
    if cond {
        Ok(())
    } else {
        Err(PyException::new_err(msg))
    }
}

/// Checks shared by the two-surface operations of `_renpy.pyx`.
fn pair<'a>(
    fname: &str,
    a: &'a Bound<'_, PyAny>,
    b: &'a Bound<'_, PyAny>,
    same_size: bool,
) -> PyResult<(&'a Surface, &'a Surface)> {
    let (a, b) = (surf(a, fname, "first")?, surf(b, fname, "second")?);
    need(
        matches!(a.format().bits(), 24 | 32),
        format!("{fname} requires a 24 or 32 bit surface."),
    )?;
    need(
        a.format().bits() == b.format().bits(),
        format!("{fname} requires both surfaces have the same bitsize."),
    )?;
    if same_size {
        need(
            a.size() == b.size(),
            format!("{fname} requires both surfaces have the same size."),
        )?;
    }
    Ok((a, b))
}

fn tbl(b: &PyBackedBytes, what: &str) -> PyResult<()> {
    need(
        b.len() >= 256,
        format!("{what} must be at least 256 bytes long."),
    )
}

#[pyfunction]
pub fn version() -> (u32, u32, u32) {
    (6, 12, 0)
}

#[pyfunction]
#[pyo3(signature = (surf, file, compress=-1))]
pub fn save_png(
    py: Python<'_>,
    surf: &Bound<'_, PyAny>,
    file: &Bound<'_, PyAny>,
    compress: i32,
) -> PyResult<()> {
    let s = match surf.cast::<Surface>() {
        Ok(s) => s.get(),
        Err(_) => {
            return Err(PyException::new_err(
                "save_png requires a pygame Surface as its first argument.",
            ));
        }
    };
    let bytes = py
        .detach(|| encode_png(s, compress))
        .map_err(|e| PyException::new_err(format!("save_png: {e}")))?;
    file.call_method1("write", (PyBytes::new(py, &bytes),))?;
    Ok(())
}

#[pyfunction]
pub fn pixellate(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    avgwidth: i32,
    avgheight: i32,
    outwidth: i32,
    outheight: i32,
) -> PyResult<()> {
    let (s, d) = pair("pixellate", pysrc, pydst, false)?;
    let (si, di) = (s.img(), d.img());
    py.detach(|| ops::pixellate(&si, &di, avgwidth, avgheight, outwidth, outheight));
    d.touch();
    Ok(())
}

#[pyfunction]
#[pyo3(name = "map")]
pub fn map_(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    r: PyBackedBytes,
    g: PyBackedBytes,
    b: PyBackedBytes,
    a: PyBackedBytes,
) -> PyResult<()> {
    let (s, d) = pair("map", pysrc, pydst, true)?;
    for m in [&r, &g, &b, &a] {
        tbl(m, "map")?;
    }
    let (si, di) = (s.img(), d.img());
    py.detach(|| ops::map(&si, &di, [&r, &g, &b, &a]));
    d.touch();
    Ok(())
}

#[pyfunction]
pub fn linmap(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    r: i32,
    g: i32,
    b: i32,
    a: i32,
) -> PyResult<()> {
    let (s, d) = pair("map", pysrc, pydst, true)?;
    let (si, di) = (s.img(), d.img());
    py.detach(|| ops::linmap(&si, &di, [r, g, b, a]));
    d.touch();
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (pysrc, pywrk, pydst, xrad, yrad=None))]
pub fn blur(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pywrk: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    xrad: f32,
    yrad: Option<f32>,
) -> PyResult<()> {
    let (s, w) = pair("blur", pysrc, pywrk, true)?;
    let (_, d) = pair("blur", pysrc, pydst, true)?;
    let yrad = yrad.unwrap_or(xrad);
    need(
        xrad >= 0.0 && yrad >= 0.0,
        "blur requires a positive radius.".into(),
    )?;
    let (si, wi, di) = (s.img(), w.img(), d.img());
    py.detach(|| ops::blur(&si, &wi, &di, xrad, yrad));
    w.touch();
    d.touch();
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (pysrc, pydst, radius, vertical=0))]
pub fn linblur(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    radius: i32,
    vertical: i32,
) -> PyResult<()> {
    let (s, d) = pair("linblur", pysrc, pydst, true)?;
    need(radius >= 1, "linblur requires a non-zero radius.".into())?;
    let (si, di) = (s.img(), d.img());
    py.detach(|| ops::linblur(&si, &di, radius, vertical != 0));
    d.touch();
    Ok(())
}

#[pyfunction]
pub fn alpha_munge(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    srcchan: i32,
    dstchan: i32,
    amap: PyBackedBytes,
) -> PyResult<()> {
    let (s, d) = pair("alpha_munge", pysrc, pydst, true)?;
    tbl(&amap, "alpha_munge map")?;
    need(
        (0..4).contains(&srcchan) && (0..4).contains(&dstchan),
        "alpha_munge channel out of range.".into(),
    )?;
    let (si, di) = (s.img(), d.img());
    py.detach(|| ops::alpha_munge(&si, &di, srcchan, dstchan, &amap));
    d.touch();
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (pysrc, pydst, source_xoff=0.0, source_yoff=0.0, source_width=None, source_height=None, dest_xoff=0.0, dest_yoff=0.0, dest_width=None, dest_height=None, precise=0))]
#[allow(clippy::too_many_arguments)]
pub fn bilinear(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    source_xoff: f32,
    source_yoff: f32,
    source_width: Option<f32>,
    source_height: Option<f32>,
    dest_xoff: f32,
    dest_yoff: f32,
    dest_width: Option<f32>,
    dest_height: Option<f32>,
    precise: i32,
) -> PyResult<()> {
    let (s, d) = pair("bilinear", pysrc, pydst, false)?;
    let (sw, sh) = match (source_width, source_height) {
        (Some(w), Some(h)) => (w, h),
        _ => (s.size().0 as f32, s.size().1 as f32),
    };
    let (dw, dh) = match (dest_width, dest_height) {
        (Some(w), Some(h)) => (w, h),
        _ => (d.size().0 as f32, d.size().1 as f32),
    };
    let (si, di) = (s.img(), d.img());
    py.detach(|| {
        ops::bilinear(
            &si,
            &di,
            source_xoff,
            source_yoff,
            sw,
            sh,
            dest_xoff,
            dest_yoff,
            dw,
            dh,
            precise != 0,
        )
    });
    d.touch();
    Ok(())
}

#[pyfunction]
pub fn check(surf: &Bound<'_, PyAny>) -> PyResult<()> {
    let s = match surf.cast::<Surface>() {
        Ok(s) => s.get(),
        Err(_) => return Err(PyException::new_err("Surface must be a pygame surface.")),
    };
    need(s.format().bits() == 32, "Surface must be 32-bit.".into())
}

#[pyfunction]
#[pyo3(signature = (pysrc, pydst, corner_x, corner_y, xdx, ydx, xdy, ydy, a=1.0, precise=0))]
#[allow(clippy::too_many_arguments)]
pub fn transform(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    corner_x: f32,
    corner_y: f32,
    xdx: f32,
    ydx: f32,
    xdy: f32,
    ydy: f32,
    a: f32,
    precise: i32,
) -> PyResult<()> {
    check(pysrc)?;
    check(pydst)?;
    let (s, d) = (
        surf(pysrc, "transform", "first")?,
        surf(pydst, "transform", "second")?,
    );
    let (si, di) = (s.img(), d.img());
    let ashift = s.format().shifts[3] as u32;
    py.detach(|| {
        ops::transform(
            &si,
            &di,
            corner_x,
            corner_y,
            xdx,
            ydx,
            xdy,
            ydy,
            ashift,
            a,
            precise != 0,
        )
    });
    d.touch();
    Ok(())
}

fn three(
    a: &Bound<'_, PyAny>,
    b: &Bound<'_, PyAny>,
    c: &Bound<'_, PyAny>,
) -> PyResult<(Img, Img, Img)> {
    for s in [a, b, c] {
        check(s)?;
    }
    Ok((
        surf(a, "blend", "")?.img(),
        surf(b, "blend", "")?.img(),
        surf(c, "blend", "")?.img(),
    ))
}

#[pyfunction]
pub fn blend(
    py: Python<'_>,
    pysrca: &Bound<'_, PyAny>,
    pysrcb: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    alpha: i32,
) -> PyResult<()> {
    let (a, b, d) = three(pysrca, pysrcb, pydst)?;
    py.detach(|| ops::blend(&a, &b, &d, alpha));
    surf(pydst, "blend", "")?.touch();
    Ok(())
}

#[pyfunction]
pub fn imageblend(
    py: Python<'_>,
    pysrca: &Bound<'_, PyAny>,
    pysrcb: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    pyimg: &Bound<'_, PyAny>,
    aoff: i32,
    amap: PyBackedBytes,
) -> PyResult<()> {
    check(pyimg)?;
    let (a, b, d) = three(pysrca, pysrcb, pydst)?;
    tbl(&amap, "imageblend map")?;
    let img = surf(pyimg, "imageblend", "")?.img();
    py.detach(|| ops::imageblend(&a, &b, &d, &img, aoff, &amap));
    surf(pydst, "imageblend", "")?.touch();
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (pysrc, pydst, c00, c01, c02, c03, c04, c10, c11, c12, c13, c14, c20, c21, c22, c23, c24, c30, c31, c32, c33, c34))]
#[allow(clippy::too_many_arguments)]
pub fn colormatrix(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    c00: f32,
    c01: f32,
    c02: f32,
    c03: f32,
    c04: f32,
    c10: f32,
    c11: f32,
    c12: f32,
    c13: f32,
    c14: f32,
    c20: f32,
    c21: f32,
    c22: f32,
    c23: f32,
    c24: f32,
    c30: f32,
    c31: f32,
    c32: f32,
    c33: f32,
    c34: f32,
) -> PyResult<()> {
    check(pysrc)?;
    check(pydst)?;
    let (s, d) = (
        surf(pysrc, "colormatrix", "")?,
        surf(pydst, "colormatrix", "")?,
    );
    let m = [
        [c00, c01, c02, c03, c04],
        [c10, c11, c12, c13, c14],
        [c20, c21, c22, c23, c24],
        [c30, c31, c32, c33, c34],
    ];
    let (si, di) = (s.img(), d.img());
    py.detach(|| ops::colormatrix(&si, &di, m));
    d.touch();
    Ok(())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn staticgray(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    rmul: i32,
    gmul: i32,
    bmul: i32,
    amul: i32,
    shift: u32,
    vmap: PyBackedBytes,
) -> PyResult<()> {
    let (s, d) = (
        surf(pysrc, "staticgray", "first")?,
        surf(pydst, "staticgray", "second")?,
    );
    let (si, di) = (s.img(), d.img());
    py.detach(|| ops::staticgray(&si, &di, [rmul, gmul, bmul, amul], shift.min(31), &vmap));
    d.touch();
    Ok(())
}

#[pyfunction]
pub fn subpixel(
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
    xoffset: f64,
    yoffset: f64,
    shift: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let _ = shift;
    pydst.call_method1("blit", (pysrc, (xoffset as i64, yoffset as i64)))?;
    Ok(())
}

#[pyfunction]
pub fn premultiply_alpha(
    py: Python<'_>,
    pysrc: &Bound<'_, PyAny>,
    pydst: &Bound<'_, PyAny>,
) -> PyResult<()> {
    check(pysrc)?;
    check(pydst)?;
    let (s, d) = (
        surf(pysrc, "premultiply_alpha", "")?,
        surf(pydst, "premultiply_alpha", "")?,
    );
    let (si, di) = (s.img(), d.img());
    py.detach(|| ops::premultiply_alpha(&si, &di));
    d.touch();
    Ok(())
}
