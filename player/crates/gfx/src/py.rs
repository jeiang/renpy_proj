//! The Python face of the crate: module `renpy.gl2.wgpudraw`.
//!
//! Rust classes: `Gpu` (device, pass recording), `GpuTexture`, `GpuProgram`. The `WgpuDraw` class itself, the texture
//! wrapper and the frame logic are Python (`wgpudraw.py`, run into this module at import), because they walk Ren'Py's
//! Cython objects. The tree walk that needs `cdef` access lives in `renpy/gl2/gl2meshbridge.pyx`.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Arc;

use media::PyVideoFrame;
use parking_lot::Mutex;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};
use surface::Surface;

use crate::gpu::{Renderer, SamplerKey, Target, TexInner, mip_count};
use crate::program::{PipeKey, ProgramInner, VLayout};

const PY_SOURCE: &str = include_str!("wgpudraw.py");

fn rt(e: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

/// Flattens numbers, nested sequences and float32 byte strings into one list of numbers.
fn flatten(obj: &Bound<'_, PyAny>, out: &mut Vec<f64>) -> PyResult<()> {
    if let Ok(b) = obj.cast::<PyBytes>() {
        for c in b.as_bytes().as_chunks::<4>().0 {
            out.push(f32::from_ne_bytes([c[0], c[1], c[2], c[3]]) as f64);
        }
        return Ok(());
    }
    if let Ok(x) = obj.extract::<f64>() {
        out.push(x);
        return Ok(());
    }
    for item in obj.try_iter()? {
        flatten(&item?, out)?;
    }
    Ok(())
}

#[pyclass(module = "renpy.gl2.wgpudraw")]
pub struct GpuProgram {
    inner: Arc<ProgramInner>,
}

#[pymethods]
impl GpuProgram {
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// (name, type, array length or None) of every live non-sampler uniform, in packing order.
    fn uniforms(&self) -> Vec<(String, String, Option<u32>)> {
        self.inner
            .slots
            .iter()
            .map(|s| (s.name.clone(), s.ty.clone(), s.array))
            .collect()
    }

    /// Names of the live `sampler2D` uniforms, in binding order.
    fn samplers(&self) -> Vec<String> {
        self.inner.tr.samplers.clone()
    }

    fn attributes(&self) -> Vec<(String, String)> {
        self.inner
            .tr
            .attributes
            .iter()
            .map(|(n, d, _)| (n.clone(), d.ty.clone()))
            .collect()
    }

    /// Builds the common pipelines on a worker thread, so the first draw does not stall.
    fn warm(&self, gpu: &Gpu) {
        let prog = self.inner.clone();
        let sh = gpu.r.lock().sh.clone();
        let formats = {
            let r = gpu.r.lock();
            vec![crate::gpu::COLOR_FORMAT, r.screen_format]
        };
        let _ = std::thread::Builder::new()
            .name("gfx-warm".into())
            .spawn(move || {
                let text = prog
                    .tr
                    .attributes
                    .iter()
                    .any(|(n, _, _)| n.starts_with("a_text_"));
                for (stride, offs, is_text) in crate::gpu::standard_layouts() {
                    if is_text && !text {
                        continue;
                    }
                    let offs: HashMap<String, u32> =
                        offs.into_iter().map(|(n, o)| (n.to_string(), o)).collect();
                    let Ok(vl) = prog.resolve(2, stride, &offs) else {
                        continue;
                    };
                    for f in formats.iter().copied() {
                        let key = PipeKey {
                            format: f,
                            blend: None,
                            mask: 15,
                            cull: 0,
                            depth: false,
                            vlayout: vl.clone(),
                        };
                        if let Err(e) = prog.pipeline(&sh, &key) {
                            log::warn!("pipeline warm-up failed: {e}");
                        }
                    }
                }
            });
    }
}

#[pyclass(module = "renpy.gl2.wgpudraw")]
pub struct GpuTexture {
    inner: Mutex<Arc<TexInner>>,
}

impl GpuTexture {
    fn get(&self) -> Arc<TexInner> {
        self.inner.lock().clone()
    }
}

#[pymethods]
impl GpuTexture {
    #[getter]
    fn width(&self) -> u32 {
        self.get().width
    }
    #[getter]
    fn height(&self) -> u32 {
        self.get().height
    }
    #[getter]
    fn mip_levels(&self) -> u32 {
        self.get().mips
    }
}

#[pyclass(module = "renpy.gl2.wgpudraw")]
pub struct Gpu {
    r: Mutex<Renderer>,
}

fn premultiply_rows(bytes: &[u8], pitch: usize, w: usize, h: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for p in bytes[y * pitch..y * pitch + w * 4].as_chunks::<4>().0 {
            let a = p[3] as u32;
            out.push(((p[0] as u32 * a + 127) / 255) as u8);
            out.push(((p[1] as u32 * a + 127) / 255) as u8);
            out.push(((p[2] as u32 * a + 127) / 255) as u8);
            out.push(p[3]);
        }
    }
    out
}

#[pymethods]
impl Gpu {
    fn info(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let r = self.r.lock();
        let d = PyDict::new(py);
        d.set_item("gpu_name", r.sh.adapter_name.clone())?;
        d.set_item("gpu_backend", r.sh.backend.clone())?;
        d.set_item("max_texture_size", r.sh.max_texture_size)?;
        Ok(d.unbind())
    }

    fn screen_size(&self) -> (u32, u32) {
        self.r.lock().screen_size
    }

    fn resize(&self, w: u32, h: u32) {
        self.r.lock().resize(w, h);
    }

    /// (bytes, count) of live textures.
    fn texture_size(&self) -> (usize, usize) {
        let r = self.r.lock();
        (
            r.sh.tex_bytes.load(std::sync::atomic::Ordering::Relaxed),
            r.sh.tex_count.load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// A blank texture that can be a render target. `mipmap` allocates the full chain.
    fn new_texture(&self, w: u32, h: u32, mipmap: bool) -> PyResult<GpuTexture> {
        let r = self.r.lock();
        let mips = if mipmap { mip_count(w, h) } else { 1 };
        Ok(GpuTexture {
            inner: Mutex::new(r.sh.new_texture(w, h, mips).map_err(rt)?),
        })
    }

    /// Uploads an RGBA surface. Non-premultiplied pixels are premultiplied on the way in.
    fn texture_from_surface(
        &self,
        surf: PyRef<'_, Surface>,
        premultiplied: bool,
        mipmap: bool,
    ) -> PyResult<GpuTexture> {
        let mut r = self.r.lock();
        let (w, h) = surf.with_pixels(|pv| (pv.width, pv.height));
        let mips = if mipmap { mip_count(w, h) } else { 1 };
        let tex = r.sh.new_texture(w, h, mips).map_err(rt)?;
        surf.with_pixels(|pv| {
            if premultiplied {
                tex.write_level0(pv.bytes, pv.pitch, pv.width, pv.height);
            } else {
                let px =
                    premultiply_rows(pv.bytes, pv.pitch, pv.width as usize, pv.height as usize);
                tex.write_level0(&px, pv.width as usize * 4, pv.width, pv.height);
            }
        });
        r.queue_mips(&tex);
        Ok(GpuTexture {
            inner: Mutex::new(tex),
        })
    }

    /// Gives `tex` a full mip chain (a new allocation, since wgpu fixes the level count).
    fn add_mipmap(&self, tex: &GpuTexture) -> PyResult<()> {
        let mut r = self.r.lock();
        let old = tex.get();
        if old.mips > 1 {
            return Ok(());
        }
        let new =
            r.sh.new_texture(old.width, old.height, mip_count(old.width, old.height))
                .map_err(rt)?;
        r.flush().map_err(rt)?;
        let mut enc = r.sh.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &old.tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &new.tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: old.width,
                height: old.height,
                depth_or_array_layers: 1,
            },
        );
        r.sh.encode_mips(&mut enc, &new);
        r.sh.queue.submit([enc.finish()]);
        *tex.inner.lock() = new;
        Ok(())
    }

    fn load_video_frame(
        &self,
        frame: PyRef<'_, PyVideoFrame>,
        mipmap: bool,
    ) -> PyResult<GpuTexture> {
        let mut guard = self.r.lock();
        let r = &mut *guard;
        let sh = r.sh.clone();
        if r.yuv.is_none() {
            r.yuv = Some(crate::yuv::Yuv::new(&sh));
        }
        let t = r
            .yuv
            .as_mut()
            .unwrap()
            .convert(&sh, &frame.0, mipmap)
            .map_err(rt)?;
        Ok(GpuTexture {
            inner: Mutex::new(t),
        })
    }

    /// Starts a pass. `target` None means the window. `viewport` is (x, y, w, h) in target pixels, top-left origin.
    #[pyo3(signature = (target, viewport, clear, screen_like=false))]
    fn begin_pass(
        &self,
        target: Option<&GpuTexture>,
        viewport: (f32, f32, f32, f32),
        clear: Option<(f64, f64, f64, f64)>,
        screen_like: bool,
    ) {
        let mut r = self.r.lock();
        let vp = [viewport.0, viewport.1, viewport.2, viewport.3];
        let c = clear.map(|c| [c.0, c.1, c.2, c.3]);
        let t = match target {
            None => Target::Screen,
            Some(t) => Target::Tex(t.get()),
        };
        if screen_like {
            r.begin_screen_like_pass(t, vp, c);
        } else {
            r.begin_pass(t, vp, c);
        }
    }

    fn end_pass(&self) -> PyResult<()> {
        self.r.lock().end_pass().map_err(rt)
    }

    fn queue_mips(&self, tex: &GpuTexture) {
        let t = tex.get();
        self.r.lock().queue_mips(&t);
    }

    /// Records one indexed draw. Mesh pointers stay valid for the call; the data is copied.
    ///
    /// `values` holds one entry per `GpuProgram.uniforms()` item; `textures` one
    /// `(GpuTexture, wrap_s, wrap_t, mag_linear, min_linear, mip, aniso)` per `GpuProgram.samplers()` item.
    #[pyo3(signature = (prog, point_size, pos_addr, pos_len, attr_addr, attr_len, stride, offsets, tri_addr, tri_len, values, textures, blend, mask, cull, depth, clear_depth))]
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        prog: PyRef<'_, GpuProgram>,
        point_size: u32,
        pos_addr: usize,
        pos_len: usize,
        attr_addr: usize,
        attr_len: usize,
        stride: u32,
        offsets: &Bound<'_, PyDict>,
        tri_addr: usize,
        tri_len: usize,
        values: &Bound<'_, PyList>,
        textures: &Bound<'_, PyList>,
        blend: Option<[i32; 6]>,
        mask: Option<[bool; 4]>,
        cull: u8,
        depth: bool,
        clear_depth: bool,
    ) -> PyResult<()> {
        let p = &prog.inner;
        let mut offs: HashMap<String, u32> = HashMap::with_capacity(offsets.len());
        for (k, v) in offsets.iter() {
            offs.insert(k.extract()?, v.extract()?);
        }
        let vl: VLayout = p
            .resolve(point_size, stride, &offs)
            .map_err(PyValueError::new_err)?;
        if values.len() != p.slots.len() {
            return Err(rt(format!(
                "shader {} takes {} uniforms, got {}",
                p.name,
                p.slots.len(),
                values.len()
            )));
        }
        let mut flat = Vec::with_capacity(values.len());
        for v in values.iter() {
            let mut f = vec![];
            flatten(&v, &mut f)?;
            flat.push(f);
        }
        let mut ub = vec![0u8; p.block_size as usize];
        p.pack(&flat, &mut ub).map_err(rt)?;
        let mut texs = Vec::with_capacity(textures.len());
        for t in textures.iter() {
            let t = t.cast::<PyTuple>()?;
            let g: PyRef<'_, GpuTexture> = t.get_item(0)?.extract()?;
            let key = SamplerKey {
                wrap_s: t.get_item(1)?.extract()?,
                wrap_t: t.get_item(2)?.extract()?,
                mag_linear: t.get_item(3)?.extract()?,
                min_linear: t.get_item(4)?.extract()?,
                mip: t.get_item(5)?.extract()?,
                aniso: t.get_item(6)?.extract()?,
            };
            texs.push((g.get(), key));
        }
        if texs.len() != p.tr.samplers.len() {
            return Err(rt(format!(
                "shader {} takes {} textures, got {}",
                p.name,
                p.tr.samplers.len(),
                texs.len()
            )));
        }
        // SAFETY: the caller passes addresses and lengths of live `Mesh` arrays that outlive this call.
        let (pos, attr, idx) = unsafe {
            (
                std::slice::from_raw_parts(pos_addr as *const u8, pos_len * 4),
                std::slice::from_raw_parts(attr_addr as *const u8, attr_len * 4),
                std::slice::from_raw_parts(tri_addr as *const u8, tri_len * 4),
            )
        };
        let m = mask.unwrap_or([true; 4]);
        let mask_bits = (m[0] as u8) | (m[1] as u8) << 1 | (m[2] as u8) << 2 | (m[3] as u8) << 3;
        let mut r = self.r.lock();
        let format = match r.current_target_is_screen() {
            true => r.screen_format,
            false => crate::gpu::COLOR_FORMAT,
        };
        let key = PipeKey {
            format,
            blend,
            mask: mask_bits,
            cull,
            depth,
            vlayout: vl.clone(),
        };
        r.draw(&prog.inner, &vl, key, pos, attr, idx, ub, texs, clear_depth)
            .map_err(rt)
    }

    fn flush(&self) -> PyResult<()> {
        self.r.lock().flush().map_err(rt)
    }

    fn present(&self) -> PyResult<()> {
        self.r.lock().present().map_err(rt)
    }

    /// Reads a texture (or the headless screen) into a new `Surface`. Pixels are un-premultiplied when asked.
    #[pyo3(signature = (tex, unpremultiply=false))]
    fn read_surface(
        &self,
        py: Python<'_>,
        tex: Option<&GpuTexture>,
        unpremultiply: bool,
    ) -> PyResult<Py<Surface>> {
        let t = tex.map(|t| t.get());
        let (w, h, mut px, _) = self.r.lock().read_pixels(t.as_ref(), None).map_err(rt)?;
        if unpremultiply {
            for p in px.as_chunks_mut::<4>().0 {
                let a = p[3] as u32;
                if 0 < a && a < 255 {
                    p[0] = (p[0] as u32 * 255 / a).min(255) as u8;
                    p[1] = (p[1] as u32 * 255 / a).min(255) as u8;
                    p[2] = (p[2] as u32 * 255 / a).min(255) as u8;
                }
            }
        }
        Py::new(py, Surface::from_rgba(w, h, px))
    }

    /// Returns the RGBA bytes of a texture, for tests.
    fn read_bytes<'py>(
        &self,
        py: Python<'py>,
        tex: Option<&GpuTexture>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let t = tex.map(|t| t.get());
        let (_, _, px, _) = self.r.lock().read_pixels(t.as_ref(), None).map_err(rt)?;
        Ok(PyBytes::new(py, &px))
    }

    /// The alpha byte of pixel (0, 0) of a texture.
    fn read_alpha(&self, tex: &GpuTexture) -> PyResult<u8> {
        let t = tex.get();
        let (_, _, px, _) = self
            .r
            .lock()
            .read_pixels(Some(&t), Some([0, 0, 1, 1]))
            .map_err(rt)?;
        Ok(px[3])
    }
}

/// Translates and validates a program (`gl2shader.Program.load`). Raises `ValueError` on a compile or link error.
#[pyfunction]
fn compile_program(name: &str, vertex: &str, fragment: &str) -> PyResult<GpuProgram> {
    let inner = ProgramInner::compile(name, vertex, fragment).map_err(PyValueError::new_err)?;
    Ok(GpuProgram {
        inner: Arc::new(inner),
    })
}

/// Creates the window through `platform` and the wgpu device on it.
#[pyfunction]
fn create(width: u32, height: u32, title: &str, resizable: bool) -> PyResult<Gpu> {
    let win = platform::create_window(width, height, title, resizable).map_err(rt)?;
    let r = Renderer::new_window(win).map_err(rt)?;
    Ok(Gpu { r: Mutex::new(r) })
}

/// A device with no window (tests, screenshots without a display).
#[pyfunction]
fn create_headless(width: u32, height: u32) -> PyResult<Gpu> {
    Ok(Gpu {
        r: Mutex::new(Renderer::new_headless(width, height).map_err(rt)?),
    })
}

/// The physical size of the window, from `platform`.
#[pyfunction]
fn drawable_size() -> (u32, u32) {
    platform::drawable_size()
}

#[pymodule]
#[pyo3(name = "wgpudraw")]
pub fn wgpudraw(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    init_module(py, m)
}

/// Fills module `renpy.gl2.wgpudraw`. Public so that a test build can wrap it in its own extension module.
pub fn init_module(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Gpu>()?;
    m.add_class::<GpuTexture>()?;
    m.add_class::<GpuProgram>()?;
    m.add_function(wrap_pyfunction!(compile_program, m)?)?;
    m.add_function(wrap_pyfunction!(create, m)?)?;
    m.add_function(wrap_pyfunction!(create_headless, m)?)?;
    m.add_function(wrap_pyfunction!(drawable_size, m)?)?;
    let builtins = py.import("builtins")?;
    let src = CString::new(PY_SOURCE).map_err(rt)?;
    let code = builtins.call_method1(
        "compile",
        (src.to_str().map_err(rt)?, "<renpy.gl2.wgpudraw>", "exec"),
    )?;
    builtins.call_method1("exec", (code, m.dict()))?;
    Ok(())
}
