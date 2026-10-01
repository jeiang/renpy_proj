//! The `Surface` class of `renpy.pygame.surface`.

use std::ffi::{c_int, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;

use pyo3::exceptions::{PyBufferError, PyIndexError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;
use pyo3::types::{PyDict, PyTuple};

use crate::blit::{self, SrcState};
use crate::buf::{Buf, Img, Shared};
use crate::pyutil::*;
use crate::sdl::*;

/// `SRCALPHA` as `renpy.pygame.locals` defines it.
pub const SRCALPHA: u64 = 0x8000_0000;

/// Bytes held by live surfaces that own their memory (`total_size` in stock).
static TOTAL_SIZE: AtomicUsize = AtomicUsize::new(0);

struct BlockBox(Box<SdlBlock>);
// The block holds raw pointers into itself and into the shared allocation.
// It is only written while the GIL is held.
unsafe impl Send for BlockBox {}
unsafe impl Sync for BlockBox {}

#[derive(Clone, Copy)]
struct State {
    clip: SdlRect,
    colorkey: Option<u32>,
    alpha_mod: u8,
    has_alpha: bool,
}

#[pyclass(
    frozen,
    subclass,
    weakref,
    dict,
    module = "renpy.pygame.surface",
    name = "Surface"
)]
pub struct Surface {
    shared: Arc<Shared>,
    owns: bool,
    off: usize,
    pitch: usize,
    w: u32,
    h: u32,
    fmt: Format,
    block: BlockBox,
    parent: Option<Py<Surface>>,
    root: Option<Py<Surface>>,
    offset: (i32, i32),
    locks: Mutex<Vec<Py<PyAny>>>,
    state: Mutex<State>,
}

impl Drop for Surface {
    fn drop(&mut self) {
        if self.owns {
            TOTAL_SIZE.fetch_sub(self.pitch * self.h as usize, Ordering::Relaxed);
        }
    }
}

fn masks_from(flags: u64, masks: Option<[u32; 4]>, sample: Option<Format>) -> Format {
    if let Some(m) = masks {
        return Format::new(m);
    }
    if let Some(f) = sample {
        return f;
    }
    if flags & SRCALPHA != 0 {
        Format::RGBA
    } else {
        Format::RGBX
    }
}

impl Surface {
    // One argument per field of the block layout.
    #[allow(clippy::too_many_arguments)]
    fn build(
        shared: Arc<Shared>,
        owns: bool,
        off: usize,
        pitch: usize,
        w: u32,
        h: u32,
        fmt: Format,
        parent: Option<Py<Surface>>,
        root: Option<Py<Surface>>,
        offset: (i32, i32),
    ) -> Surface {
        let pixels = unsafe { shared.buf.ptr().add(off) } as *mut c_void;
        let mut block = Box::new(SdlBlock {
            surf: SdlSurface {
                flags: 0,
                format: std::ptr::null_mut(),
                w: w as i32,
                h: h as i32,
                pitch: pitch as i32,
                pixels,
                userdata: std::ptr::null_mut(),
                locked: 0,
                list_blitmap: std::ptr::null_mut(),
                clip_rect: SdlRect {
                    x: 0,
                    y: 0,
                    w: w as i32,
                    h: h as i32,
                },
                map: std::ptr::null_mut(),
                refcount: 1,
            },
            fmt: fmt.to_sdl(),
        });
        block.surf.format = &mut block.fmt;
        if owns {
            TOTAL_SIZE.fetch_add(pitch * h as usize, Ordering::Relaxed);
        }
        Surface {
            shared,
            owns,
            off,
            pitch,
            w,
            h,
            fmt,
            block: BlockBox(block),
            parent,
            root,
            offset,
            locks: Mutex::new(Vec::new()),
            state: Mutex::new(State {
                clip: SdlRect {
                    x: 0,
                    y: 0,
                    w: w as i32,
                    h: h as i32,
                },
                colorkey: None,
                alpha_mod: 255,
                has_alpha: false,
            }),
        }
    }

    /// A new zero-filled surface that owns its memory.
    pub fn alloc(w: u32, h: u32, fmt: Format) -> Result<Surface, String> {
        let pitch = (w as usize).checked_mul(4).ok_or("Surface too large.")?;
        let len = pitch.checked_mul(h as usize).ok_or("Surface too large.")?;
        if len > isize::MAX as usize / 2 {
            return Err("Surface too large.".to_string());
        }
        let shared = Arc::new(Shared {
            buf: Buf::zeroed(len),
            generation: AtomicU64::new(0),
        });
        Ok(Surface::build(
            shared,
            true,
            0,
            pitch,
            w,
            h,
            fmt,
            None,
            None,
            (0, 0),
        ))
    }

    /// Rust API: a surface with RGBA masks that takes `rgba` as its pixels.
    /// `rgba` must hold exactly `width * height * 4` bytes.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Surface {
        let pitch = width as usize * 4;
        assert_eq!(
            rgba.len(),
            pitch * height as usize,
            "from_rgba: buffer length does not match width * height * 4"
        );
        let shared = Arc::new(Shared {
            buf: Buf::from_vec(rgba),
            generation: AtomicU64::new(0),
        });
        Surface::build(
            shared,
            true,
            0,
            pitch,
            width,
            height,
            Format::RGBA,
            None,
            None,
            (0, 0),
        )
    }

    /// A raw view of the pixels, for operations in this crate.
    pub fn img(&self) -> Img {
        // SAFETY: `off`, `pitch`, `w` and `h` were validated against the buffer
        // when the surface was built, and `shared` keeps it alive as long as
        // `self`. Callers that release the GIL hold the Python object alive.
        unsafe {
            Img::new(
                self.shared.buf.ptr(),
                self.shared.buf.len(),
                self.off,
                self.pitch,
                self.w as usize,
                self.h as usize,
                self.fmt,
            )
        }
    }

    pub fn format(&self) -> Format {
        self.fmt
    }

    /// Calls `f` with the pixels: `pitch` bytes per row, `bytes` from pixel
    /// (0, 0) to the end of the last row.
    pub fn with_pixels<R>(&self, f: impl FnOnce(PixelView<'_>) -> R) -> R {
        let img = self.img();
        // SAFETY: `span` bytes from the first pixel are inside the allocation.
        let bytes = unsafe { std::slice::from_raw_parts(img.pixels_ptr(), img.span()) };
        f(PixelView {
            width: self.w,
            height: self.h,
            pitch: self.pitch,
            bytes,
        })
    }

    /// Changes on every write made through this crate.
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Relaxed)
    }

    pub fn touch(&self) {
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    pub fn has_alpha_channel(&self) -> bool {
        self.fmt.has_alpha()
    }

    fn state(&self) -> State {
        *self.state.lock()
    }

    pub fn clip_rect(&self) -> SdlRect {
        self.state().clip
    }

    pub fn src_state(&self) -> SrcState {
        let s = self.state();
        SrcState {
            alpha_mod: s.alpha_mod,
            colorkey: s.colorkey,
        }
    }

    pub fn sdl_ptr(&self) -> *mut SdlSurface {
        &self.block.0.surf as *const SdlSurface as *mut SdlSurface
    }

    fn map_color(&self, c: ColorArg) -> u32 {
        match c {
            ColorArg::Pixel(p) => p,
            ColorArg::Rgba(c) => self.fmt.pack(c),
        }
    }

    fn root_of<'py>(slf: &Bound<'py, Surface>) -> Bound<'py, Surface> {
        match &slf.get().root {
            Some(r) => r.bind(slf.py()).clone(),
            None => slf.clone(),
        }
    }
}

/// Borrowed pixels of a surface.
pub struct PixelView<'a> {
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
    pub bytes: &'a [u8],
}

fn sample_format(depth: &Bound<'_, PyAny>) -> Option<Format> {
    depth.cast::<Surface>().ok().map(|s| s.get().fmt)
}

fn extract_masks(obj: &Bound<'_, PyAny>) -> PyResult<[u32; 4]> {
    let (r, g, b, a): (u32, u32, u32, u32) = obj.extract()?;
    Ok([r, g, b, a])
}

#[pymethods]
impl Surface {
    #[new]
    #[pyo3(signature = (size, flags=0, depth=None, masks=None))]
    fn py_new(
        py: Python<'_>,
        size: &Bound<'_, PyAny>,
        flags: u64,
        depth: Option<&Bound<'_, PyAny>>,
        masks: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Surface> {
        let (w, h) = parse_size(size)?;
        let masks = match masks {
            Some(m) if !m.is_none() => Some(extract_masks(m)?),
            _ => None,
        };
        let sample = depth.and_then(sample_format);
        if let Some(d) = depth
            && sample.is_none()
        {
            let bits: i64 = d.extract()?;
            if masks.is_some() && bits != 32 && bits != 24 {
                return Err(pygame_error(py, "Only 32-bit surfaces are supported."));
            }
            if masks.is_none() && bits != 32 {
                return Err(pygame_error(py, "Only 32-bit surfaces are supported."));
            }
        }
        let mut fmt = masks_from(flags, masks, sample);
        if masks.is_none() && sample.is_none() && flags & SRCALPHA == 0 {
            fmt = Format::RGBX;
        }
        Surface::alloc(w, h, fmt).map_err(|e| pygame_error(py, e))
    }

    fn __repr__(&self) -> String {
        format!("<Surface({}x{}x{})>", self.w, self.h, self.fmt.bits())
    }

    fn __sizeof__(&self) -> usize {
        if self.owns {
            self.pitch * self.h as usize
        } else {
            0
        }
    }

    #[pyo3(signature = (source, dest, area=None, special_flags=0))]
    fn blit<'py>(
        &self,
        py: Python<'py>,
        source: &Bound<'py, Surface>,
        dest: &Bound<'py, PyAny>,
        area: Option<&Bound<'py, PyAny>>,
        special_flags: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let src = source.get();
        let dest_rect = parse_rect(dest, Some("dest"))?;
        let area_rect = match area {
            Some(a) if !a.is_none() => Some(parse_rect(a, Some("area"))?),
            _ => None,
        };
        let sst = src.src_state();
        let clip = self.clip_rect();
        let region = blit::clip_blit(
            src.w as i32,
            src.h as i32,
            area_rect,
            clip,
            dest_rect.x,
            dest_rect.y,
        );
        if let Some(c) = region {
            let (s, d) = (src.img(), self.img());
            let (dst_keep, src_keep) = (self.shared.clone(), src.shared.clone());
            let r = py.detach(move || {
                let r = blit::blit_region(
                    &s,
                    sst,
                    c.sx as usize,
                    c.sy as usize,
                    c.w as usize,
                    c.h as usize,
                    &d,
                    c.dx as usize,
                    c.dy as usize,
                    special_flags,
                );
                drop((dst_keep, src_keep));
                r
            });
            r.map_err(|e| pygame_error(py, e))?;
            self.touch();
        } else if !matches!(special_flags, 0 | 1..=9 | 0x10 | 0x11) {
            return Err(pygame_error(py, "Invalid argument passed to blit."));
        }
        // dirty = Rect(dest[0], dest[1], source.w, source.h).clip(self.get_rect())
        let (x, y) = (dest_rect.x, dest_rect.y);
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = x.saturating_add(src.w as i32).min(self.w as i32);
        let y1 = y.saturating_add(src.h as i32).min(self.h as i32);
        if x1 > x0 && y1 > y0 {
            make_rect(py, x0, y0, x1 - x0, y1 - y0)
        } else {
            make_rect(py, 0, 0, 0, 0)
        }
    }

    /// Raw copy of `src` to the top left of this surface, with no blending.
    /// Pixel formats are converted.
    pub fn copy_from(&self, py: Python<'_>, src: &Bound<'_, Surface>) -> PyResult<()> {
        let (s, d) = (src.get().img(), self.img());
        let keep = (src.get().shared.clone(), self.shared.clone());
        py.detach(move || {
            blit::copy_all(&s, &d);
            drop(keep);
        });
        self.touch();
        Ok(())
    }

    #[pyo3(signature = (surface=None))]
    fn convert(&self, py: Python<'_>, surface: Option<&Bound<'_, PyAny>>) -> PyResult<Surface> {
        let fmt = match surface.and_then(sample_format) {
            Some(f) if f.has_alpha() => Format::new([f.masks[0], f.masks[1], f.masks[2], 0]),
            Some(f) => f,
            None => Format::RGBX,
        };
        self.converted(py, fmt)
    }

    #[pyo3(signature = (surface=None))]
    fn convert_alpha(
        &self,
        py: Python<'_>,
        surface: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Surface> {
        let fmt = match surface.and_then(sample_format) {
            Some(f) if f.has_alpha() => f,
            Some(f) => {
                let a = !(f.masks[0] | f.masks[1] | f.masks[2]);
                Format::new([f.masks[0], f.masks[1], f.masks[2], a])
            }
            None => Format::RGBA,
        };
        self.converted(py, fmt)
    }

    fn copy(&self, py: Python<'_>) -> PyResult<Surface> {
        self.converted(py, self.fmt)
    }

    #[pyo3(signature = (color, rect=None, special_flags=0))]
    fn fill<'py>(
        &self,
        py: Python<'py>,
        color: &Bound<'py, PyAny>,
        rect: Option<&Bound<'py, PyAny>>,
        special_flags: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = special_flags;
        let pixel = self.map_color(parse_color(color)?);
        let clip = self.clip_rect();
        match rect {
            Some(r) if !r.is_none() => {
                let mut sr = parse_rect(r, None)?;
                if sr.x < 0 {
                    sr.w += sr.x;
                    sr.x = 0;
                }
                if sr.y < 0 {
                    sr.h += sr.y;
                    sr.y = 0;
                }
                if sr.w <= 0 || sr.h <= 0 {
                    return make_rect(py, 0, 0, 0, 0);
                }
                blit::fill_rect(&self.img(), clip, Some(sr), pixel);
                self.touch();
                make_rect(py, sr.x, sr.y, sr.w, sr.h)
            }
            _ => {
                blit::fill_rect(&self.img(), clip, None, pixel);
                self.touch();
                make_rect(py, 0, 0, self.w as i32, self.h as i32)
            }
        }
    }

    #[pyo3(signature = (dx=0, dy=0))]
    fn scroll(&self, dx: i32, dy: i32) {
        let (w, h) = (self.w as i32, self.h as i32);
        let (sx, ddx, mw) = if dx >= 0 {
            (0, dx, w - dx)
        } else {
            (-dx, 0, w + dx)
        };
        let (sy, ddy, mh) = if dy >= 0 {
            (0, dy, h - dy)
        } else {
            (-dy, 0, h + dy)
        };
        if mw <= 0 || mh <= 0 {
            return;
        }
        let img = self.img();
        let rows: Box<dyn Iterator<Item = i32>> = if dy > 0 {
            Box::new((0..mh).rev())
        } else {
            Box::new(0..mh)
        };
        for r in rows {
            // SAFETY: both rows are inside the surface; `copy` handles overlap.
            unsafe {
                let s = img.row_ptr((sy + r) as usize).add(sx as usize * 4);
                let d = img.row_ptr((ddy + r) as usize).add(ddx as usize * 4);
                std::ptr::copy(s, d, mw as usize * 4);
            }
        }
        self.touch();
    }

    #[pyo3(signature = (color, flags=0))]
    fn set_colorkey(&self, color: Option<&Bound<'_, PyAny>>, flags: i64) -> PyResult<()> {
        let _ = flags;
        let key = match color {
            Some(c) if !c.is_none() => Some(self.map_color(parse_color(c)?)),
            _ => None,
        };
        self.state.lock().colorkey = key;
        Ok(())
    }

    fn get_colorkey<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        match self.state().colorkey {
            Some(k) => Ok(Some(make_color(py, self.fmt.unpack(k))?)),
            None => Ok(None),
        }
    }

    #[pyo3(signature = (value, flags=0))]
    fn set_alpha(&self, value: Option<u8>, flags: i64) {
        let _ = flags;
        let mut st = self.state.lock();
        match value {
            Some(v) => {
                st.alpha_mod = v;
                st.has_alpha = true;
            }
            None => {
                st.alpha_mod = 255;
                st.has_alpha = false;
            }
        }
    }

    fn get_alpha(&self) -> Option<u8> {
        let st = self.state();
        if st.has_alpha || self.fmt.has_alpha() {
            Some(st.alpha_mod)
        } else {
            None
        }
    }

    #[pyo3(signature = (lock=None))]
    fn lock(slf: &Bound<'_, Self>, lock: Option<Py<PyAny>>) -> PyResult<()> {
        let py = slf.py();
        let root = Surface::root_of(slf);
        let item = lock.unwrap_or_else(|| slf.clone().into_any().unbind());
        root.get().locks.lock().push(item);
        // SAFETY: `locked` is a plain int in the heap block; the GIL is held.
        unsafe { (*root.get().sdl_ptr()).locked += 1 };
        let _ = py;
        Ok(())
    }

    #[pyo3(signature = (lock=None))]
    fn unlock(slf: &Bound<'_, Self>, lock: Option<Py<PyAny>>) -> PyResult<()> {
        let py = slf.py();
        let root = Surface::root_of(slf);
        let item = lock.unwrap_or_else(|| slf.clone().into_any().unbind());
        let mut list = root.get().locks.lock();
        let pos = list
            .iter()
            .position(|o| o.bind(py).eq(item.bind(py)).unwrap_or(false));
        match pos {
            Some(i) => {
                list.remove(i);
            }
            None => return Err(PyValueError::new_err("list.remove(x): x not in list")),
        }
        // SAFETY: see `lock`.
        unsafe {
            let s = root.get().sdl_ptr();
            if (*s).locked > 0 {
                (*s).locked -= 1;
            }
        }
        Ok(())
    }

    fn mustlock(&self) -> bool {
        false
    }

    fn get_locked(&self) -> bool {
        !self.locks.lock().is_empty()
    }

    fn get_locks(slf: &Bound<'_, Self>) -> Vec<Py<PyAny>> {
        let py = slf.py();
        Surface::root_of(slf)
            .get()
            .locks
            .lock()
            .iter()
            .map(|o| o.clone_ref(py))
            .collect()
    }

    fn get_at<'py>(&self, py: Python<'py>, pos: (i64, i64)) -> PyResult<Bound<'py, PyAny>> {
        let (x, y) = pos;
        if !(0 <= x && x < self.w as i64) || !(0 <= y && y < self.h as i64) {
            return Err(PyIndexError::new_err("Position outside surface."));
        }
        let px = self.img().px(x as usize, y as usize);
        make_color(py, self.fmt.unpack(px))
    }

    fn set_at(&self, pos: (i64, i64), color: &Bound<'_, PyAny>) -> PyResult<()> {
        let (x, y) = pos;
        if !(0 <= x && x < self.w as i64) || !(0 <= y && y < self.h as i64) {
            return Err(PyValueError::new_err("Position outside surface."));
        }
        let pixel = self.map_color(parse_color(color)?);
        self.img().put(x as usize, y as usize, pixel);
        self.touch();
        Ok(())
    }

    fn get_at_mapped(&self, pos: (i64, i64)) -> PyResult<u32> {
        let (x, y) = pos;
        if !(0 <= x && x < self.w as i64) || !(0 <= y && y < self.h as i64) {
            return Err(PyValueError::new_err("Position outside surface."));
        }
        Ok(self.img().px(x as usize, y as usize))
    }

    fn map_rgb(&self, color: &Bound<'_, PyAny>) -> PyResult<u32> {
        Ok(self.map_color(parse_color(color)?))
    }

    fn unmap_rgb<'py>(&self, py: Python<'py>, pixel: u32) -> PyResult<Bound<'py, PyAny>> {
        make_color(py, self.fmt.unpack(pixel))
    }

    #[pyo3(signature = (rect))]
    fn set_clip(&self, rect: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let full = SdlRect {
            x: 0,
            y: 0,
            w: self.w as i32,
            h: self.h as i32,
        };
        let clip = match rect {
            Some(r) if !r.is_none() => {
                let r = parse_rect(r, None)?;
                // SDL_SetClipRect: intersect with the surface.
                let x0 = r.x.max(0);
                let y0 = r.y.max(0);
                let x1 = r.x.saturating_add(r.w).min(full.w);
                let y1 = r.y.saturating_add(r.h).min(full.h);
                SdlRect {
                    x: x0,
                    y: y0,
                    w: (x1 - x0).max(0),
                    h: (y1 - y0).max(0),
                }
            }
            _ => full,
        };
        self.state.lock().clip = clip;
        // SAFETY: plain struct write in the heap block, GIL held.
        unsafe { (*self.sdl_ptr()).clip_rect = clip };
        Ok(())
    }

    fn get_clip(&self) -> (i32, i32, i32, i32) {
        let c = self.clip_rect();
        (c.x, c.y, c.w, c.h)
    }

    #[pyo3(signature = (*args))]
    fn subsurface(slf: &Bound<'_, Self>, args: &Bound<'_, PyTuple>) -> PyResult<Py<Surface>> {
        let py = slf.py();
        let this = slf.get();
        let r = if args.len() == 1 {
            parse_rect(&args.get_item(0)?, None)?
        } else {
            parse_rect(args.as_any(), None)?
        };
        if r.w < 0 || r.h < 0 {
            return Err(pygame_error(py, "subsurface size must be non-negative."));
        }
        if r.x < 0
            || r.y < 0
            || r.x as i64 + r.w as i64 > this.w as i64
            || r.y as i64 + r.h as i64 > this.h as i64
        {
            return Err(pygame_error(
                py,
                "subsurface rectangle outside surface area.",
            ));
        }
        let off = this.off + r.y as usize * this.pitch + r.x as usize * 4;
        let root = Surface::root_of(slf).unbind();
        let mut sub = Surface::build(
            this.shared.clone(),
            false,
            off,
            this.pitch,
            r.w as u32,
            r.h as u32,
            this.fmt,
            Some(slf.clone().unbind()),
            Some(root),
            (r.x, r.y),
        );
        {
            let st = this.state();
            if st.has_alpha {
                let s = sub.state.get_mut();
                s.has_alpha = true;
                s.alpha_mod = st.alpha_mod;
            }
        }
        Py::new(py, sub)
    }

    fn get_parent(&self, py: Python<'_>) -> Option<Py<Surface>> {
        self.parent.as_ref().map(|p| p.clone_ref(py))
    }

    fn get_abs_parent(slf: &Bound<'_, Self>) -> Py<Surface> {
        Surface::root_of(slf).unbind()
    }

    fn get_offset(&self) -> (i32, i32) {
        self.offset
    }

    fn get_abs_offset(slf: &Bound<'_, Self>) -> (i32, i32) {
        let py = slf.py();
        let (mut x, mut y) = (0, 0);
        let mut cur = slf.clone();
        loop {
            x += cur.get().offset.0;
            y += cur.get().offset.1;
            let next = cur.get().parent.as_ref().map(|p| p.bind(py).clone());
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
        (x, y)
    }

    fn get_size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    fn get_width(&self) -> u32 {
        self.w
    }

    fn get_height(&self) -> u32 {
        self.h
    }

    #[pyo3(signature = (**kwargs))]
    fn get_rect<'py>(
        &self,
        py: Python<'py>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let rv = make_rect(py, 0, 0, self.w as i32, self.h as i32)?;
        if let Some(kw) = kwargs {
            for (k, v) in kw.iter() {
                rv.setattr(k.cast_into::<pyo3::types::PyString>()?, v)?;
            }
        }
        Ok(rv)
    }

    fn get_bitsize(&self) -> u8 {
        self.fmt.bits()
    }

    fn get_bytesize(&self) -> u8 {
        4
    }

    fn get_flags(&self) -> u64 {
        if self.fmt.has_alpha() || self.state().has_alpha {
            SRCALPHA
        } else {
            0
        }
    }

    fn get_pitch(&self) -> usize {
        self.pitch
    }

    fn get_masks(&self) -> (u32, u32, u32, u32) {
        let m = self.fmt.masks;
        (m[0], m[1], m[2], m[3])
    }

    fn set_masks(&self, py: Python<'_>, masks: &Bound<'_, PyAny>) -> PyResult<()> {
        let _ = masks;
        py.import("warnings")?
            .call_method1("warn", ("Surface.set_masks is not supported.",))?;
        Ok(())
    }

    fn get_shifts(&self) -> (u8, u8, u8, u8) {
        let s = self.fmt.shifts;
        (s[0], s[1], s[2], s[3])
    }

    fn set_shifts(&self, py: Python<'_>, shifts: &Bound<'_, PyAny>) -> PyResult<()> {
        let _ = shifts;
        py.import("warnings")?
            .call_method1("warn", ("Surface.set_shifts is not supported.",))?;
        Ok(())
    }

    fn get_losses(&self) -> (u8, u8, u8, u8) {
        let l = self.fmt.losses;
        (l[0], l[1], l[2], l[3])
    }

    #[pyo3(signature = (min_alpha=1))]
    fn get_bounding_rect<'py>(
        &self,
        py: Python<'py>,
        min_alpha: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (w, h) = (self.w as usize, self.h as usize);
        let amask = self.fmt.masks[3];
        if amask == 0 || w == 0 || h == 0 {
            return make_rect(py, 0, 0, w as i32, h as i32);
        }
        let amin = (0x0101_0101u32.wrapping_mul(min_alpha as u32)) & amask;
        let img = self.img();
        let (mut minx, mut maxx, mut miny, mut maxy) = (w - 1, 0usize, h - 1, 0usize);
        let topleft = img.px(0, 0);
        let botright = img.px(w - 1, h - 1);
        if (topleft & amask) > amin && (botright & amask) > amin {
            minx = 0;
            miny = 0;
            maxx = w - 1;
            maxy = h - 1;
        } else {
            for y in 0..h {
                for x in 0..w {
                    if (img.px(x, y) & amask) >= amin {
                        minx = minx.min(x);
                        miny = miny.min(y);
                        maxx = maxx.max(x);
                        maxy = maxy.max(y);
                    }
                }
            }
        }
        if minx > maxx {
            return make_rect(py, 0, 0, 0, 0);
        }
        let rw = (maxx - minx + 1).min(w - minx);
        let rh = (maxy - miny + 1).min(h - miny);
        make_rect(py, minx as i32, miny as i32, rw as i32, rh as i32)
    }

    #[pyo3(signature = (kind="2"))]
    fn get_view(&self, py: Python<'_>, kind: &str) -> PyResult<()> {
        let _ = kind;
        Err(pygame_error(py, "Surface.get_view is not supported."))
    }

    #[getter]
    fn _pixels_address(&self) -> usize {
        self.img().pixels_ptr() as usize
    }

    /// Changes on every write made through the API of this crate.
    #[getter]
    fn _generation(&self) -> u64 {
        self.generation()
    }

    // Python API name (pygame_sdl2 Surface.from_data) takes self.
    #[allow(clippy::wrong_self_convention)]
    fn from_data(&self, data: PyBackedBytes) -> PyResult<()> {
        let row = self.w as usize * 4;
        if data.len() != row * self.h as usize {
            return Err(PyValueError::new_err("The data must fill the surface."));
        }
        let img = self.img();
        for y in 0..self.h as usize {
            // SAFETY: rows are inside the surface and `data` holds `row` bytes per row.
            unsafe {
                std::ptr::copy_nonoverlapping(data[y * row..].as_ptr(), img.row_ptr(y), row);
            }
        }
        self.touch();
        Ok(())
    }

    /// Buffer protocol: 2-D, shape `(height, width * 4)`, one byte per item
    /// (`B`), strides `(pitch, 1)`. Writable.
    unsafe fn __getbuffer__(
        slf: Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        if view.is_null() {
            return Err(PyBufferError::new_err("View is null"));
        }
        let this = slf.get();
        let row = this.w as usize * 4;
        let strided = (flags & ffi::PyBUF_STRIDES) == ffi::PyBUF_STRIDES;
        if !strided && this.h > 1 && this.pitch != row {
            return Err(PyBufferError::new_err(
                "Surface is not contiguous; request strides.",
            ));
        }
        if (flags & ffi::PyBUF_WRITABLE) == ffi::PyBUF_WRITABLE {
            this.touch();
        }
        let dims: Box<[isize; 4]> =
            Box::new([this.h as isize, row as isize, this.pitch as isize, 1]);
        let dims = Box::into_raw(dims);
        unsafe {
            (*view).buf = this.img().pixels_ptr() as *mut c_void;
            (*view).obj = slf.clone().into_any().into_ptr();
            (*view).len = (row * this.h as usize) as isize;
            (*view).readonly = 0;
            (*view).itemsize = 1;
            (*view).format = if (flags & ffi::PyBUF_FORMAT) == ffi::PyBUF_FORMAT {
                c"B".as_ptr() as *mut _
            } else {
                std::ptr::null_mut()
            };
            (*view).ndim = 2;
            (*view).shape = if (flags & ffi::PyBUF_ND) == ffi::PyBUF_ND {
                (dims as *mut isize).cast()
            } else {
                std::ptr::null_mut()
            };
            (*view).strides = if strided {
                (dims as *mut isize).add(2)
            } else {
                std::ptr::null_mut()
            };
            (*view).suboffsets = std::ptr::null_mut();
            (*view).internal = dims as *mut c_void;
        }
        Ok(())
    }

    unsafe fn __releasebuffer__(&self, view: *mut ffi::Py_buffer) {
        self.touch();
        unsafe {
            let p = (*view).internal as *mut [isize; 4];
            if !p.is_null() {
                drop(Box::from_raw(p));
                (*view).internal = std::ptr::null_mut();
            }
        }
    }
}

impl Surface {
    fn converted(&self, py: Python<'_>, fmt: Format) -> PyResult<Surface> {
        let rv = Surface::alloc(self.w, self.h, fmt).map_err(|e| pygame_error(py, e))?;
        let (s, d) = (self.img(), rv.img());
        let keep = self.shared.clone();
        py.detach(move || {
            blit::copy_all(&s, &d);
            drop(keep);
        });
        let st = self.state();
        if st.has_alpha && !fmt.has_alpha() {
            let mut s = rv.state.lock();
            s.has_alpha = true;
            s.alpha_mod = st.alpha_mod;
        }
        Ok(rv)
    }
}

/// The value of stock `renpy.pygame.surface.total_size`.
pub fn total_size() -> usize {
    TOTAL_SIZE.load(Ordering::Relaxed)
}

/// `PySurface_AsSurface`: the `SDL_Surface`-layout block inside a `Surface`.
/// Returns NULL with a `TypeError` set for other objects.
pub unsafe extern "C" fn py_surface_as_surface(obj: *mut ffi::PyObject) -> *mut SdlSurface {
    // SAFETY: Cython calls this with the GIL held.
    let py = unsafe { Python::assume_attached() };
    let any = unsafe { Bound::from_borrowed_ptr(py, obj) };
    match any.cast::<Surface>() {
        Ok(s) => s.get().sdl_ptr(),
        Err(_) => {
            pyo3::exceptions::PyTypeError::new_err("not a Surface").restore(py);
            std::ptr::null_mut()
        }
    }
}
