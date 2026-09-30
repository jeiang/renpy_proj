//! Decode benchmark: every decoder returns tightly packed RGBA8 (converted if needed, cost included).
//! Usage: imgbench <scratch dir> [rounds]   Prints TSV: file fmt w h decoder median_ms min_ms maxdiff_vs_ref
use std::{fs, io::Cursor, path::Path, time::Instant};

type Img = (u32, u32, Vec<u8>);
type Dec = fn(&[u8]) -> Result<Img, String>;

fn rgb_to_rgba(w: u32, h: u32, rgb: &[u8]) -> Img {
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for p in rgb.chunks_exact(3) {
        out.extend_from_slice(&[p[0], p[1], p[2], 255]);
    }
    (w, h, out)
}
fn gray_to_rgba(w: u32, h: u32, g: &[u8], alpha: bool) -> Img {
    let n = if alpha { 2 } else { 1 };
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for p in g.chunks_exact(n) {
        out.extend_from_slice(&[p[0], p[0], p[0], if alpha { p[1] } else { 255 }]);
    }
    (w, h, out)
}

// ---- PNG ----
fn png_png(d: &[u8]) -> Result<Img, String> {
    let mut dec = png::Decoder::new(Cursor::new(d));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; r.output_buffer_size().ok_or("size")?];
    let i = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let b = &buf[..i.buffer_size()];
    Ok(match i.color_type {
        png::ColorType::Rgba => (i.width, i.height, b.to_vec()),
        png::ColorType::Rgb => rgb_to_rgba(i.width, i.height, b),
        png::ColorType::Grayscale => gray_to_rgba(i.width, i.height, b, false),
        png::ColorType::GrayscaleAlpha => gray_to_rgba(i.width, i.height, b, true),
        c => return Err(format!("{c:?}")),
    })
}
fn png_zune(d: &[u8]) -> Result<Img, String> {
    use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let opts = DecoderOptions::default().png_set_add_alpha_channel(true).png_set_strip_to_8bit(true);
    let mut dec = zune_png::PngDecoder::new_with_options(ZCursor::new(d), opts);
    let px = dec.decode().map_err(|e| format!("{e:?}"))?;
    let (w, h) = dec.dimensions().unwrap();
    let cs = dec.colorspace().unwrap();
    let b = px.u8().ok_or("not u8")?;
    Ok(match cs {
        ColorSpace::RGBA => (w as u32, h as u32, b),
        ColorSpace::RGB => rgb_to_rgba(w as u32, h as u32, &b),
        ColorSpace::Luma => gray_to_rgba(w as u32, h as u32, &b, false),
        ColorSpace::LumaA => gray_to_rgba(w as u32, h as u32, &b, true),
        c => return Err(format!("{c:?}")),
    })
}
fn png_spng(d: &[u8]) -> Result<Img, String> {
    let mut dec = spng::Decoder::new(Cursor::new(d)).with_output_format(spng::Format::Rgba8);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let info = r.info().clone();
    let mut buf = vec![0; r.output_buffer_size()];
    r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    Ok((info.width, info.height, buf))
}
fn png_lodepng(d: &[u8]) -> Result<Img, String> {
    let im = lodepng::decode32(d).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(im.buffer.len() * 4);
    for p in &im.buffer {
        out.extend_from_slice(&[p.r, p.g, p.b, p.a]);
    }
    Ok((im.width as u32, im.height as u32, out))
}

// ---- JPEG ----
fn jpg_zune(d: &[u8]) -> Result<Img, String> {
    use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(d), opts);
    let b = dec.decode().map_err(|e| format!("{e:?}"))?;
    let i = dec.info().unwrap();
    Ok((i.width as u32, i.height as u32, b))
}
fn jpg_jpegdecoder(d: &[u8]) -> Result<Img, String> {
    let mut dec = jpeg_decoder::Decoder::new(Cursor::new(d));
    let b = dec.decode().map_err(|e| e.to_string())?;
    let i = dec.info().unwrap();
    match i.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => Ok(rgb_to_rgba(i.width as u32, i.height as u32, &b)),
        jpeg_decoder::PixelFormat::L8 => Ok(gray_to_rgba(i.width as u32, i.height as u32, &b, false)),
        f => Err(format!("{f:?}")),
    }
}
#[cfg(feature = "turbojpeg")]
fn jpg_turbo(d: &[u8]) -> Result<Img, String> {
    let im = turbojpeg::decompress(d, turbojpeg::PixelFormat::RGBA).map_err(|e| e.to_string())?;
    Ok((im.width as u32, im.height as u32, im.pixels))
}
#[cfg(feature = "mozjpeg")]
fn jpg_moz(d: &[u8]) -> Result<Img, String> {
    let dm = mozjpeg::Decompress::new_mem(d).map_err(|e| e.to_string())?;
    let (w, h) = (dm.width() as u32, dm.height() as u32);
    let mut s = dm.rgba().map_err(|e| e.to_string())?;
    let px: Vec<[u8; 4]> = s.read_scanlines().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(px.len() * 4);
    for p in &px { out.extend_from_slice(p); }
    Ok((w, h, out))
}

// ---- WebP ----
fn webp_imagewebp(d: &[u8]) -> Result<Img, String> {
    let mut dec = image_webp::WebPDecoder::new(Cursor::new(d)).map_err(|e| e.to_string())?;
    let (w, h) = dec.dimensions();
    let mut buf = vec![0; dec.output_buffer_size().ok_or("size")?];
    dec.read_image(&mut buf).map_err(|e| e.to_string())?;
    Ok(if dec.has_alpha() { (w, h, buf) } else { rgb_to_rgba(w, h, &buf) })
}
/// libwebp advanced API with use_threads=1 (one extra thread for the loop filter of lossy images).
fn webp_libwebp_mt(d: &[u8]) -> Result<Img, String> {
    use libwebp_sys::*;
    use libwebp_sys::WEBP_CSP_MODE::MODE_RGBA;
    unsafe {
        let mut cfg: WebPDecoderConfig = std::mem::zeroed();
        if !WebPInitDecoderConfig(&mut cfg) { return Err("init".into()); }
        cfg.options.use_threads = 1;
        cfg.output.colorspace = MODE_RGBA;
        if WebPDecode(d.as_ptr(), d.len(), &mut cfg) != VP8StatusCode::VP8_STATUS_OK { return Err("decode".into()); }
        let o = &cfg.output;
        let (w, h) = (o.width as u32, o.height as u32);
        let buf = std::slice::from_raw_parts(o.u.RGBA.rgba, o.u.RGBA.size).to_vec();
        WebPFreeDecBuffer(&mut cfg.output);
        Ok((w, h, buf))
    }
}
fn webp_libwebp(d: &[u8]) -> Result<Img, String> {
    let im = webp::Decoder::new(d).decode().ok_or("decode failed")?;
    let (w, h) = (im.width(), im.height());
    Ok(if im.is_alpha() { (w, h, im.to_vec()) } else { rgb_to_rgba(w, h, &im) })
}

// ---- generic wrapper ----
fn image_crate(d: &[u8]) -> Result<Img, String> {
    let im = image::load_from_memory(d).map_err(|e| e.to_string())?.into_rgba8();
    Ok((im.width(), im.height(), im.into_raw()))
}

fn decoders(fmt: &str) -> Vec<(&'static str, Dec)> {
    let mut v: Vec<(&'static str, Dec)> = vec![];
    match fmt {
        "png" => v.extend([("png", png_png as Dec), ("zune-png", png_zune), ("spng", png_spng), ("lodepng", png_lodepng)]),
        "jpg" => {
            v.extend([("zune-jpeg", jpg_zune as Dec), ("jpeg-decoder", jpg_jpegdecoder)]);
            #[cfg(feature = "turbojpeg")]
            v.push(("turbojpeg", jpg_turbo));
            #[cfg(feature = "mozjpeg")]
            v.push(("mozjpeg", jpg_moz));
        }
        "webp" => v.extend([("libwebp(webp)", webp_libwebp as Dec), ("image-webp", webp_imagewebp), ("libwebp+threads", webp_libwebp_mt)]),
        _ => {}
    }
    v.push(("image(crate)", image_crate));
    v
}

fn maxdiff(a: &[u8], b: &[u8]) -> (u8, f64) {
    if a.len() != b.len() { return (255, f64::NAN); }
    let (mut m, mut s) = (0u8, 0u64);
    for (x, y) in a.iter().zip(b) {
        let d = x.abs_diff(*y);
        m = m.max(d);
        s += (d as u64) * (d as u64);
    }
    (m, (s as f64 / a.len() as f64).sqrt())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = Path::new(&args[1]);
    let rounds: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(9);
    if args.get(3).map(|s| s.as_str()) == Some("par") { return par(dir); }
    if args.get(3).map(|s| s.as_str()) == Some("post") { return post(dir); }
    println!("file\tfmt\tw\th\tdecoder\tmedian_ms\tmin_ms\tmax_abs_diff\trms_diff");
    let mut files: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    for f in files {
        let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let fmt = if ext == "jpeg" { "jpg".into() } else { ext };
        let data = fs::read(&f).unwrap();
        // reference = the C library that SDL2_image itself uses (libpng-like spng for PNG, libwebp, libjpeg-turbo if built)
        let refname = match fmt.as_str() { "png" => "spng", "webp" => "libwebp(webp)", "jpg" => if cfg!(feature = "turbojpeg") { "turbojpeg" } else { "jpeg-decoder" }, _ => "image(crate)" };
        let decs = decoders(&fmt);
        let refimg = decs.iter().find(|(n, _)| *n == refname).and_then(|(_, d)| d(&data).ok());
        for (name, dec) in &decs {
            let first = match dec(&data) { Ok(i) => i, Err(e) => { println!("{}\t{}\t-\t-\t{}\tERR {}", f.file_name().unwrap().to_string_lossy(), fmt, name, e); continue } };
            let mut ts: Vec<f64> = (0..rounds).map(|_| { let t = Instant::now(); let r = dec(&data).unwrap(); std::hint::black_box(&r); t.elapsed().as_secs_f64() * 1e3 }).collect();
            ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let (md, rms) = refimg.as_ref().map(|r| maxdiff(&r.2, &first.2)).unwrap_or((0, 0.0));
            println!("{}\t{}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{}\t{:.3}", f.file_name().unwrap().to_string_lossy(), fmt, first.0, first.1, name, ts[ts.len() / 2], ts[0], md, rms);
        }
    }
}

/// Parallel scaling: decode the whole directory with the fastest decoder per format on N threads.
fn par(dir: &Path) {
    use rayon::prelude::*;
    let best = |fmt: &str| match fmt { "png" => "spng", "webp" => "libwebp(webp)", _ => if cfg!(feature = "turbojpeg") { "turbojpeg" } else { "zune-jpeg" } };
    let mut items: Vec<(Dec, Vec<u8>)> = vec![];
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let fmt = if ext == "jpeg" { "jpg".to_string() } else { ext };
        let d = decoders(&fmt).into_iter().find(|(n, _)| *n == best(&fmt)).unwrap().1;
        items.push((d, fs::read(&p).unwrap()));
    }
    println!("threads\twall_ms_for_{}_images\tspeedup", items.len());
    let mut base = 0.0;
    for th in [1usize, 2, 4, 6, 8, 12] {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(th).build().unwrap();
        let mut best_t = f64::MAX;
        for _ in 0..5 {
            let t = Instant::now();
            pool.install(|| items.par_iter().for_each(|(d, b)| { std::hint::black_box(d(b).unwrap()); }));
            best_t = best_t.min(t.elapsed().as_secs_f64() * 1e3);
        }
        if th == 1 { base = best_t; }
        println!("{th}\t{best_t:.1}\t{:.2}", base / best_t);
    }
}

/// CPU post-processing after decode, on 1080p/4K straight-alpha RGBA: premultiply and alpha bounding box
/// (what Ren'Py does with the ftl shader pass and Surface.get_bounding_rect).
fn premultiply(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 255 {
            p[0] = ((p[0] as u32 * a + 127) / 255) as u8;
            p[1] = ((p[1] as u32 * a + 127) / 255) as u8;
            p[2] = ((p[2] as u32 * a + 127) / 255) as u8;
        }
    }
}
fn bbox(px: &[u8], w: usize, h: usize) -> (usize, usize, usize, usize) {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        let row = &px[y * w * 4..(y + 1) * w * 4];
        if let Some(first) = row.chunks_exact(4).position(|p| p[3] != 0) {
            let last = row.chunks_exact(4).rposition(|p| p[3] != 0).unwrap();
            x0 = x0.min(first); x1 = x1.max(last + 1); y0 = y0.min(y); y1 = y1.max(y + 1);
        }
    }
    (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}
fn post(dir: &Path) {
    println!("file\tw\th\tpremultiply_ms\tbbox_ms\tmemcpy_ms");
    let mut files: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    for f in files {
        let d = fs::read(&f).unwrap();
        let Ok((w, h, mut px)) = image_crate(&d) else { continue };
        let med = |mut g: Box<dyn FnMut()>| { let mut v: Vec<f64> = (0..9).map(|_| { let t = Instant::now(); g(); t.elapsed().as_secs_f64() * 1e3 }).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[0] };
        let mut cp = px.clone();
        let pm = { let mut v: Vec<f64> = (0..9).map(|_| { let t = Instant::now(); cp.copy_from_slice(&px); premultiply(&mut cp); std::hint::black_box(&cp); t.elapsed().as_secs_f64() * 1e3 }).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[0] };
        let mc = { let mut v: Vec<f64> = (0..9).map(|_| { let t = Instant::now(); cp.copy_from_slice(&px); std::hint::black_box(&cp); t.elapsed().as_secs_f64() * 1e3 }).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[0] };
        let bb = med(Box::new(|| { std::hint::black_box(bbox(&px, w as usize, h as usize)); }));
        px.truncate(0);
        println!("{}\t{w}\t{h}\t{:.2}\t{:.2}\t{:.2}", f.file_name().unwrap().to_string_lossy(), pm - mc, bb, mc);
    }
}
