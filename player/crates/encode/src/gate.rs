use crate::{PixelFormat, RawFrame};

/// Width in pixels (and rows in height) of the timecode band: 32 cells of 16x16 px.
pub const TC_CELLS: usize = 32;
pub const TC_CELL: usize = 16;
const TC_W: usize = TC_CELLS * TC_CELL; // 512
const TC_H: usize = TC_CELL; // 16

/// Damage test. Rule: a frame is "changed" when it is the first frame, when its width, height
/// or pixel format differ from the previous gated frame, or when any byte outside the timecode
/// region differs from the previous gated frame (exact comparison, early exit at the first
/// differing row). The timecode region is pixels x in 0..512, y in 0..16 for RGBA/BGRA and the
/// Y plane of NV12, and the matching UV samples (x in 0..512, rows 0..8) of NV12. The stored
/// reference is the last frame that returned `true` (equal to the last gated frame otherwise,
/// outside the timecode region).
pub struct FrameGate {
    prev: Vec<u8>,
    dims: (u32, u32, Option<PixelFormat>),
}

impl Default for FrameGate {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameGate {
    pub fn new() -> Self {
        FrameGate {
            prev: Vec::new(),
            dims: (0, 0, None),
        }
    }

    pub fn changed(&mut self, f: &RawFrame) -> bool {
        let same_shape =
            self.dims == (f.width, f.height, Some(f.format)) && self.prev.len() == f.data.len();
        if same_shape && !differs(&self.prev, f) {
            return false;
        }
        self.dims = (f.width, f.height, Some(f.format));
        self.prev.clear();
        self.prev.extend_from_slice(&f.data);
        true
    }
}

/// Row geometry: (offset, length, skip bytes at the row start) for every plane row.
fn differs(prev: &[u8], f: &RawFrame) -> bool {
    let (w, h) = (f.width as usize, f.height as usize);
    let rows = |start: usize, n: usize, stride: usize, tc_rows: usize, tc_bytes: usize| -> bool {
        for r in 0..n {
            let o = start + r * stride;
            let skip = if r < tc_rows { tc_bytes.min(stride) } else { 0 };
            if prev[o + skip..o + stride] != f.data[o + skip..o + stride] {
                return true;
            }
        }
        false
    };
    match f.format {
        PixelFormat::Rgba | PixelFormat::Bgra => rows(0, h, w * 4, TC_H, TC_W * 4),
        PixelFormat::Nv12 => rows(0, h, w, TC_H, TC_W) || rows(w * h, h / 2, w, TC_H / 2, TC_W),
    }
}

/// Burns `f.capture_ms` into the top-left: 32 cells of 16x16 px in a row (x = 16 i, y = 0),
/// cell 0 is the most significant bit, white for 1 and black for 0. RGBA/BGRA: 255 or 0 in the
/// colour channels with alpha 255. NV12: Y = 235 or 16, UV = 128. Cells beyond the frame width
/// or height are clipped.
pub fn burn_timecode(f: &mut RawFrame) {
    let (w, h) = (f.width as usize, f.height as usize);
    let cols = w.min(TC_W);
    let rows = h.min(TC_H);
    for i in 0..TC_CELLS {
        let x0 = i * TC_CELL;
        if x0 >= cols {
            break;
        }
        let x1 = (x0 + TC_CELL).min(cols);
        let one = (f.capture_ms >> (31 - i)) & 1 == 1;
        match f.format {
            PixelFormat::Rgba | PixelFormat::Bgra => {
                let v = if one { 255 } else { 0 };
                for y in 0..rows {
                    for px in f.data[(y * w + x0) * 4..(y * w + x1) * 4].chunks_exact_mut(4) {
                        px.copy_from_slice(&[v, v, v, 255]);
                    }
                }
            }
            PixelFormat::Nv12 => {
                let v = if one { 235 } else { 16 };
                for y in 0..rows {
                    f.data[y * w + x0..y * w + x1].fill(v);
                }
                for y in 0..rows / 2 {
                    let o = w * h + y * w;
                    f.data[o + x0..o + x1].fill(128);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(format: PixelFormat, ms: u32) -> RawFrame {
        let (w, h) = (640u32, 360u32);
        RawFrame {
            width: w,
            height: h,
            format,
            data: vec![50; RawFrame::expected_len(w, h, format)],
            capture_ms: ms,
        }
    }

    #[test]
    fn gate_first_same_changed() {
        for fmt in [PixelFormat::Rgba, PixelFormat::Bgra, PixelFormat::Nv12] {
            let mut g = FrameGate::new();
            let mut f = frame(fmt, 0);
            assert!(g.changed(&f), "first frame");
            assert!(!g.changed(&f), "identical frame");
            f.data[640 * 100 + 7] ^= 1; // y=100 in all formats (RGBA: y=25)
            assert!(g.changed(&f), "pixel changed");
            assert!(!g.changed(&f));
            let last = f.data.len() - 1; // last UV / alpha byte, far from the timecode
            f.data[last] ^= 1;
            assert!(g.changed(&f), "last byte changed");
        }
    }

    #[test]
    fn gate_ignores_timecode_but_not_below_it() {
        for fmt in [PixelFormat::Rgba, PixelFormat::Bgra, PixelFormat::Nv12] {
            let mut g = FrameGate::new();
            let mut f = frame(fmt, 0);
            assert!(g.changed(&f));
            f.capture_ms = 0xdead_beef;
            burn_timecode(&mut f);
            assert!(!g.changed(&f), "{fmt:?}: timecode only");
            f.capture_ms = 0x1234_5678;
            burn_timecode(&mut f);
            assert!(!g.changed(&f));
            // x = 512 is the first pixel right of the band, y = 16 the first row under it.
            let bpp = if fmt == PixelFormat::Nv12 { 1 } else { 4 };
            let mut a = frame(fmt, 0);
            a.data[512 * bpp] ^= 1;
            assert!(g.changed(&a), "{fmt:?}: right of band");
            let mut b = frame(fmt, 0);
            b.data[16 * 640 * bpp] ^= 1;
            assert!(g.changed(&b), "{fmt:?}: below band");
        }
    }

    #[test]
    fn gate_size_change() {
        let mut g = FrameGate::new();
        assert!(g.changed(&frame(PixelFormat::Rgba, 0)));
        let mut f = frame(PixelFormat::Rgba, 0);
        f.height = 90;
        f.data.truncate(640 * 90 * 4);
        assert!(g.changed(&f));
        assert!(g.changed(&frame(PixelFormat::Rgba, 0)));
    }

    fn decode(f: &RawFrame) -> u32 {
        let w = f.width as usize;
        let mut v = 0u32;
        for i in 0..32 {
            let (x, y) = (16 * i + 8, 8);
            let luma = match f.format {
                PixelFormat::Nv12 => f.data[y * w + x] as u32,
                PixelFormat::Rgba | PixelFormat::Bgra => f.data[(y * w + x) * 4] as u32,
            };
            v = (v << 1) | (luma >= 128) as u32;
        }
        v
    }

    #[test]
    fn timecode_roundtrip_and_bytes() {
        for fmt in [PixelFormat::Rgba, PixelFormat::Bgra, PixelFormat::Nv12] {
            for ms in [0u32, 1, 0x8000_0000, 0xa5a5_5a5a, u32::MAX] {
                let mut f = frame(fmt, ms);
                burn_timecode(&mut f);
                assert_eq!(decode(&f), ms, "{fmt:?} {ms:#x}");
            }
            let mut f = frame(fmt, 0x8000_0000);
            burn_timecode(&mut f);
            let w = 640;
            match fmt {
                PixelFormat::Nv12 => {
                    assert_eq!((f.data[0], f.data[16], f.data[15 * w + 511]), (235, 16, 16));
                    assert_eq!(f.data[w * 360], 128); // UV
                    assert_eq!(f.data[16 * w], 50); // row 16 untouched
                }
                _ => {
                    assert_eq!(&f.data[0..4], &[255, 255, 255, 255]);
                    assert_eq!(&f.data[16 * 4..16 * 4 + 4], &[0, 0, 0, 255]);
                    assert_eq!(f.data[16 * w * 4], 50);
                }
            }
        }
    }
}
