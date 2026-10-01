#![allow(dead_code)]
//! Synthetic game-like scene shared by the examples.

use encode::{PixelFormat, RawFrame};

/// A visual-novel-like UI: sky gradient, a dark textbox with text-like bars, and a button row.
pub fn base_scene(w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut d = vec![255u8; w * h * 4];
    let mut rng = 0x1234_5678u32;
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 4;
            // Gradient plus a little texture so the picture is not trivially flat.
            rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = (rng >> 28) as u8;
            d[o] = (40 + y * 120 / h) as u8 + n;
            d[o + 1] = (80 + x * 100 / w) as u8 + n;
            d[o + 2] = 200u8.saturating_sub((y * 60 / h) as u8);
        }
    }
    let rect = |d: &mut Vec<u8>, x0: usize, y0: usize, x1: usize, y1: usize, c: [u8; 3]| {
        for y in y0..y1.min(h) {
            for x in x0..x1.min(w) {
                let o = (y * w + x) * 4;
                d[o..o + 3].copy_from_slice(&c);
            }
        }
    };
    rect(
        &mut d,
        w / 20,
        h * 3 / 4,
        w * 19 / 20,
        h * 19 / 20,
        [20, 20, 30],
    );
    let mut y = h * 3 / 4 + h / 30;
    while y + h / 60 < h * 19 / 20 - h / 30 {
        for k in 0..3 {
            let x = w / 16 + k * 0;
            rect(
                &mut d,
                x,
                y,
                x + w * 7 / 8 - (y * 13 % (w / 6)),
                y + h / 60,
                [235, 235, 235],
            );
        }
        y += h / 25;
    }
    for i in 0..4 {
        rect(
            &mut d,
            w / 3 + i * w / 8,
            h / 3,
            w / 3 + i * w / 8 + w / 10,
            h / 3 + h / 12,
            [250, 200, 80],
        );
    }
    d
}

pub fn frame_from(
    base: &[u8],
    w: u32,
    h: u32,
    fmt: PixelFormat,
    t: usize,
    moving: bool,
) -> RawFrame {
    let mut rgba = base.to_vec();
    if moving {
        // A 120x120 sprite sliding across the screen.
        let (wu, hu) = (w as usize, h as usize);
        let x0 = (t * 17) % (wu - 120);
        let y0 = hu / 8 + (t * 5) % (hu / 4);
        for y in y0..y0 + 120 {
            for x in x0..x0 + 120 {
                let o = (y * wu + x) * 4;
                rgba[o..o + 4].copy_from_slice(&[230, 40 + (t % 200) as u8, 90, 255]);
            }
        }
    }
    if fmt == PixelFormat::Bgra {
        for p in rgba.chunks_exact_mut(4) {
            p.swap(0, 2);
        }
    }
    RawFrame {
        width: w,
        height: h,
        format: fmt,
        data: rgba,
        capture_ms: t as u32 * 16,
    }
}

/// Plain integer BT.709 limited-range RGBA -> NV12 for tests of the `Nv12` input path.
pub fn to_nv12(f: &RawFrame) -> RawFrame {
    let (w, h) = (f.width as usize, f.height as usize);
    let mut out = vec![0u8; w * h * 3 / 2];
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 4;
            let (r, g, b) = (f.data[o] as f32, f.data[o + 1] as f32, f.data[o + 2] as f32);
            out[y * w + x] = (16.0 + 0.1826 * r + 0.6142 * g + 0.0620 * b) as u8;
        }
    }
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            let o = (2 * y * w + 2 * x) * 4;
            let (r, g, b) = (f.data[o] as f32, f.data[o + 1] as f32, f.data[o + 2] as f32);
            out[w * h + y * w + 2 * x] = (128.0 - 0.1006 * r - 0.3386 * g + 0.4392 * b) as u8;
            out[w * h + y * w + 2 * x + 1] = (128.0 + 0.4392 * r - 0.3989 * g - 0.0403 * b) as u8;
        }
    }
    RawFrame {
        width: f.width,
        height: f.height,
        format: PixelFormat::Nv12,
        data: out,
        capture_ms: f.capture_ms,
    }
}
