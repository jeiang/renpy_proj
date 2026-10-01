//! `renpy.pygame.image`: PNG, JPEG and WebP decoding, PNG/JPEG/BMP encoding.

use std::io::Cursor;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;
use pyo3::types::{PyBytes, PyString};

use crate::pyutil::pygame_error;
use crate::sdl::Format;
use crate::surface::Surface;

pub const INIT_JPG: u32 = 1;
pub const INIT_PNG: u32 = 2;
pub const INIT_TIF: u32 = 4;
pub const INIT_WEBP: u32 = 8;
pub const INIT_JXL: u32 = 16;
pub const INIT_AVIF: u32 = 32;

/// Formats this build decodes.
const SUPPORTED: u32 = INIT_JPG | INIT_PNG | INIT_WEBP | INIT_AVIF;

pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn decode_png(data: &[u8]) -> Result<Decoded, String> {
    let mut dec = png::Decoder::new(Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| format!("PNG: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size().ok_or("PNG: image too large")?];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG: {e}"))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let src = &buf[..info.buffer_size()];
    let mut rgba = Vec::with_capacity(w * h * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(src),
        png::ColorType::Rgb => {
            for p in src.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &g in src {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for p in src.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }
        png::ColorType::Indexed => return Err("PNG: palette was not expanded".to_string()),
    }
    Ok(Decoded {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn decode_jpeg(data: &[u8]) -> Result<Decoded, String> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), opts);
    let rgba = dec.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let info = dec.info().ok_or("JPEG: no image information")?;
    Ok(Decoded {
        width: info.width as u32,
        height: info.height as u32,
        rgba,
    })
}

fn decode_webp(data: &[u8]) -> Result<Decoded, String> {
    let mut dec =
        image_webp::WebPDecoder::new(Cursor::new(data)).map_err(|e| format!("WebP: {e}"))?;
    let (w, h) = dec.dimensions();
    let alpha = dec.has_alpha();
    let size = dec.output_buffer_size().ok_or("WebP: image too large")?;
    let mut buf = vec![0u8; size];
    if dec.is_animated() {
        dec.read_frame(&mut buf).map_err(|e| format!("WebP: {e}"))?;
    } else {
        dec.read_image(&mut buf).map_err(|e| format!("WebP: {e}"))?;
    }
    let rgba = if alpha {
        buf
    } else {
        let mut v = Vec::with_capacity(w as usize * h as usize * 4);
        for p in buf.as_chunks::<3>().0 {
            v.extend_from_slice(&[p[0], p[1], p[2], 255]);
        }
        v
    };
    Ok(Decoded {
        width: w,
        height: h,
        rgba,
    })
}

/// Decodes by content, not by file name.
pub fn decode(data: &[u8]) -> Result<Decoded, String> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(data)
    } else if data.starts_with(&[0xff, 0xd8]) {
        decode_jpeg(data)
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        decode_webp(data)
    } else if is_ffmpeg_image(data) {
        crate::avdec::decode(data)
    } else {
        let kind = if data.len() >= 12 && &data[4..8] == b"ftyp" {
            "HEIF"
        } else if data.starts_with(b"<?xml") || data.starts_with(b"<svg") {
            "SVG"
        } else {
            "unknown"
        };
        Err(format!(
            "Unsupported image format ({kind}); this player decodes PNG, JPEG, WebP, AVIF, GIF, BMP and TGA."
        ))
    }
}

/// Formats FFmpeg decodes here. TGA has no header magic, so only files with
/// the TGA 2.0 footer count.
fn is_ffmpeg_image(data: &[u8]) -> bool {
    data.starts_with(b"GIF8")
        || data.starts_with(b"BM")
        || (data.len() >= 12 && &data[4..8] == b"ftyp" && matches!(&data[8..12], b"avif" | b"avis"))
        || data.ends_with(b"TRUEVISION-XFILE.\0")
}

#[pyfunction]
pub fn init() {}

#[pyfunction]
pub fn has_init(flags: u32) -> bool {
    flags & SUPPORTED == flags
}

#[pyfunction]
#[pyo3(name = "quit")]
pub fn quit_() {}

#[pyfunction]
pub fn get_extended() -> bool {
    true
}

/// The name the game used, from the `namehint` argument (bytes or str).
fn namehint_str(n: &Bound<'_, PyAny>) -> Option<String> {
    if let Ok(s) = n.extract::<String>() {
        Some(s)
    } else {
        n.extract::<Vec<u8>>()
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

/// `prefetch(name, opener, priority=2)`: decode `name` on the loader pool. `opener()`
/// returns the file bytes and runs on a pool thread with the GIL held.
/// Priorities: 0 a caller waits, 1 scene now, 2 script prediction, 3 warming.
#[pyfunction]
#[pyo3(signature = (name, opener, priority=2))]
pub fn prefetch(name: String, opener: Py<PyAny>, priority: u8) {
    crate::loader::prefetch(name, opener, priority);
}

/// Drops unclaimed prefetch results.
#[pyfunction]
pub fn prefetch_clear() {
    crate::loader::clear();
}

/// Stops the loader pool (call at exit, before the interpreter finalizes).
#[pyfunction]
pub fn prefetch_shutdown() {
    crate::loader::shutdown();
}

/// `(workers, submitted, ready_hits, waited_hits, misses, wait_ms)`.
#[pyfunction]
pub fn prefetch_stats() -> (usize, u64, u64, u64, u64, f64) {
    crate::loader::stats()
}

#[pyfunction]
#[pyo3(signature = (fi, namehint=None, size=None))]
pub fn load(
    py: Python<'_>,
    fi: &Bound<'_, PyAny>,
    namehint: Option<&Bound<'_, PyAny>>,
    size: Option<&Bound<'_, PyAny>>,
) -> PyResult<Surface> {
    // A prefetched image (see `loader`) skips the read and the decode.
    if size.is_none_or(|s| s.is_none())
        && let Some(name) = namehint.and_then(|n| namehint_str(n))
        && let Some(img) = crate::loader::take(py, &name)
    {
        return Ok(Surface::from_rgba(img.width, img.height, img.rgba));
    }
    let data: Vec<u8> = if fi.is_instance_of::<PyString>() || fi.is_instance_of::<PyBytes>() {
        let path: std::path::PathBuf = if let Ok(s) = fi.extract::<String>() {
            s.into()
        } else {
            let b = fi.extract::<Vec<u8>>()?;
            String::from_utf8_lossy(&b).into_owned().into()
        };
        std::fs::read(&path)
            .map_err(|e| pygame_error(py, format!("Couldn't open {}: {e}", path.display())))?
    } else if fi.hasattr("read")? {
        let r = fi.call_method0("read")?;
        r.extract::<PyBackedBytes>()?.to_vec()
    } else {
        return Err(pygame_error(py, "Expected a file name or a file object."));
    };
    let img = py
        .detach(|| decode(&data))
        .map_err(|e| pygame_error(py, e))?;
    Ok(Surface::from_rgba(img.width, img.height, img.rgba))
}

/// RGBA rows of a surface, and whether it has alpha.
fn rgba_rows(s: &Surface) -> (u32, u32, Vec<u8>, bool) {
    let (w, h) = s.size();
    let img = s.img();
    let fmt: Format = s.format();
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h as usize {
        for x in 0..w as usize {
            out.extend_from_slice(&fmt.unpack(img.px(x, y)));
        }
    }
    (w, h, out, fmt.has_alpha())
}

pub fn encode_png(s: &Surface, compress: i32) -> Result<Vec<u8>, String> {
    let (w, h, rgba, alpha) = rgba_rows(s);
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(match compress {
        0 | 1 => png::Compression::Fast,
        2..=6 => png::Compression::Balanced,
        7..=9 => png::Compression::High,
        _ => png::Compression::Balanced,
    });
    let raw: Vec<u8> = if alpha {
        enc.set_color(png::ColorType::Rgba);
        rgba
    } else {
        enc.set_color(png::ColorType::Rgb);
        rgba.as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect()
    };
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(&raw).map_err(|e| e.to_string())?;
    wr.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

fn encode_jpeg(s: &Surface, quality: i32) -> Result<Vec<u8>, String> {
    let (w, h, rgba, _) = rgba_rows(s);
    let rgb: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let q = if quality < 1 { 90 } else { quality.min(100) } as u8;
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, q);
    let (w16, h16) = (
        u16::try_from(w).map_err(|e| e.to_string())?,
        u16::try_from(h).map_err(|e| e.to_string())?,
    );
    enc.encode(&rgb, w16, h16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

fn encode_bmp(s: &Surface) -> Vec<u8> {
    let (w, h, rgba, _) = rgba_rows(s);
    let row = (w as usize * 3).div_ceil(4) * 4;
    let size = 54 + row * h as usize;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..h as usize).rev() {
        let start = out.len();
        for x in 0..w as usize {
            let p = &rgba[(y * w as usize + x) * 4..][..4];
            out.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        out.resize(start + row, 0);
    }
    out
}

#[pyfunction]
#[pyo3(signature = (surface, filename, compression=-1))]
pub fn save(
    py: Python<'_>,
    surface: &Bound<'_, Surface>,
    filename: &Bound<'_, PyAny>,
    compression: i32,
) -> PyResult<()> {
    let name: String = if let Ok(s) = filename.extract::<String>() {
        s
    } else {
        String::from_utf8_lossy(&filename.extract::<Vec<u8>>()?).into_owned()
    };
    let ext = std::path::Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_uppercase())
        .unwrap_or_default();
    let s = surface.get();
    let bytes = match ext.as_str() {
        "PNG" => encode_png(s, compression),
        "BMP" => Ok(encode_bmp(s)),
        "JPG" | "JPEG" => encode_jpeg(s, compression),
        _ => return Err(PyValueError::new_err(format!("Unsupported format: .{ext}"))),
    }
    .map_err(|e| pygame_error(py, e))?;
    std::fs::write(&name, bytes)
        .map_err(|e| pygame_error(py, format!("Couldn't write {name}: {e}")))?;
    Ok(())
}
