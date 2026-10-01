//! RawFrame -> FFmpeg video frame in the encoder's pixel format (NV12 or YUV420P).

use crate::{PixelFormat, RawFrame};
use anyhow::{Context as _, Result};
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::Video;
use ffmpeg_next::software::scaling::{Context, Flags};

pub struct Converter {
    dst: Pixel,
    /// (src width, src height, src format) the scaler was built for.
    key: Option<(u32, u32, PixelFormat)>,
    scaler: Option<Context>,
    src: Option<Video>,
}

// The swscale context and frames are owned here and used through `&mut self` only.
unsafe impl Send for Converter {}

fn pix(f: PixelFormat) -> Pixel {
    match f {
        PixelFormat::Rgba => Pixel::RGBA,
        PixelFormat::Bgra => Pixel::BGRA,
        PixelFormat::Nv12 => Pixel::NV12,
    }
}

/// Copies tight rows into a strided plane.
fn copy_plane(
    dst: &mut [u8],
    stride: usize,
    src: &[u8],
    src_stride: usize,
    row_bytes: usize,
    rows: usize,
) {
    for r in 0..rows {
        dst[r * stride..r * stride + row_bytes]
            .copy_from_slice(&src[r * src_stride..r * src_stride + row_bytes]);
    }
}

impl Converter {
    pub fn new(dst: Pixel) -> Self {
        Converter {
            dst,
            key: None,
            scaler: None,
            src: None,
        }
    }

    /// Returns a fresh frame of size `(w & !1, h & !1)` in the destination format.
    pub fn convert(&mut self, f: &RawFrame) -> Result<Video> {
        f.check()?;
        let (ow, oh) = (f.width & !1, f.height & !1);
        let mut out = Video::new(self.dst, ow, oh);
        // Direct copy: NV12 in, NV12 out.
        if f.format == PixelFormat::Nv12 && self.dst == Pixel::NV12 {
            let w = f.width as usize;
            let (ys, uvs) = (out.stride(0), out.stride(1));
            copy_plane(out.data_mut(0), ys, &f.data, w, ow as usize, oh as usize);
            copy_plane(
                out.data_mut(1),
                uvs,
                &f.data[w * f.height as usize..],
                w,
                ow as usize,
                oh as usize / 2,
            );
            return Ok(out);
        }
        let key = (f.width, f.height, f.format);
        if self.key != Some(key) {
            let mut sc = Context::get(
                pix(f.format),
                f.width,
                f.height,
                self.dst,
                ow,
                oh,
                Flags::FAST_BILINEAR,
            )
            .context("create swscale context")?;
            // RGB (full range) -> YUV with BT.709 coefficients, limited range, matching the
            // VUI the encoders write (bt709, tv range). The default would be BT.601.
            unsafe {
                use ffmpeg_sys_next as sys;
                let tab = sys::sws_getCoefficients(sys::SWS_CS_ITU709 as i32);
                sys::sws_setColorspaceDetails(sc.as_mut_ptr(), tab, 1, tab, 0, 0, 1 << 16, 1 << 16);
            }
            self.scaler = Some(sc);
            self.src = Some(Video::new(pix(f.format), f.width, f.height));
            self.key = Some(key);
        }
        let src = self.src.as_mut().unwrap();
        let w = f.width as usize;
        let h = f.height as usize;
        if f.format == PixelFormat::Nv12 {
            let (ys, uvs) = (src.stride(0), src.stride(1));
            copy_plane(src.data_mut(0), ys, &f.data, w, w, h);
            copy_plane(src.data_mut(1), uvs, &f.data[w * h..], w, w, h / 2);
        } else {
            let s = src.stride(0);
            copy_plane(src.data_mut(0), s, &f.data, w * 4, w * 4, h);
        }
        self.scaler
            .as_mut()
            .unwrap()
            .run(src, &mut out)
            .context("swscale")?;
        Ok(out)
    }
}
