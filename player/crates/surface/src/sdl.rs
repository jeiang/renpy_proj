//! SDL2-layout structs for the `PySurface_AsSurface` capsule, and the pixel
//! format helper shared by every operation in this crate.
//!
//! The struct layout follows SDL 2.x (`SDL_surface.h`, `SDL_pixels.h`), which
//! stock `ftfont` and `hbfont` are compiled against.

use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SdlRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[repr(C)]
pub struct SdlPixelFormat {
    pub format: u32,
    pub palette: *mut c_void,
    pub bits_per_pixel: u8,
    pub bytes_per_pixel: u8,
    pub padding: [u8; 2],
    pub rmask: u32,
    pub gmask: u32,
    pub bmask: u32,
    pub amask: u32,
    pub rloss: u8,
    pub gloss: u8,
    pub bloss: u8,
    pub aloss: u8,
    pub rshift: u8,
    pub gshift: u8,
    pub bshift: u8,
    pub ashift: u8,
    pub refcount: i32,
    pub next: *mut SdlPixelFormat,
}

#[repr(C)]
pub struct SdlSurface {
    pub flags: u32,
    pub format: *mut SdlPixelFormat,
    pub w: i32,
    pub h: i32,
    pub pitch: i32,
    pub pixels: *mut c_void,
    pub userdata: *mut c_void,
    pub locked: i32,
    pub list_blitmap: *mut c_void,
    pub clip_rect: SdlRect,
    pub map: *mut c_void,
    pub refcount: i32,
}

/// The heap block that a `Surface` owns. `surf.format` points at `fmt` in the
/// same block, so the pointer is stable while the block lives.
#[repr(C)]
pub struct SdlBlock {
    pub surf: SdlSurface,
    pub fmt: SdlPixelFormat,
}

/// Channel layout of a 32-bit pixel: masks in R, G, B, A order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub masks: [u32; 4],
    pub shifts: [u8; 4],
    pub losses: [u8; 4],
    /// True for the little-endian RGBA byte layout (the native layout).
    pub rgba: bool,
}

pub const RGBA_MASKS: [u32; 4] = [0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0xff00_0000];
pub const RGBX_MASKS: [u32; 4] = [0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0];

fn shift_loss(mask: u32) -> (u8, u8) {
    if mask == 0 {
        return (0, 8);
    }
    let shift = mask.trailing_zeros() as u8;
    let bits = (mask >> shift).trailing_ones() as u8;
    (shift, 8u8.saturating_sub(bits))
}

impl Format {
    pub const RGBA: Format = Format {
        masks: RGBA_MASKS,
        shifts: [0, 8, 16, 24],
        losses: [0, 0, 0, 0],
        rgba: true,
    };
    pub const RGBX: Format = Format {
        masks: RGBX_MASKS,
        shifts: [0, 8, 16, 0],
        losses: [0, 0, 0, 8],
        rgba: false,
    };

    pub fn new(masks: [u32; 4]) -> Format {
        let mut shifts = [0u8; 4];
        let mut losses = [0u8; 4];
        for i in 0..4 {
            let (s, l) = shift_loss(masks[i]);
            shifts[i] = s;
            losses[i] = l;
        }
        Format {
            masks,
            shifts,
            losses,
            rgba: masks == RGBA_MASKS,
        }
    }

    pub fn has_alpha(&self) -> bool {
        self.masks[3] != 0
    }

    /// `BitsPerPixel` as SDL reports it: 32 with alpha, 24 for a 32-bit
    /// pixel without alpha (`XBGR8888` and friends).
    pub fn bits(&self) -> u8 {
        if self.has_alpha() { 32 } else { 24 }
    }

    #[inline]
    fn channel(&self, px: u32, i: usize) -> u8 {
        let mask = self.masks[i];
        if mask == 0 {
            return 255;
        }
        let v = (px & mask) >> self.shifts[i];
        let loss = self.losses[i] as u32;
        if loss == 0 {
            v as u8
        } else {
            ((v << loss) + (v >> 8u32.saturating_sub(loss << 1))) as u8
        }
    }

    /// Splits a pixel value into R, G, B, A. A is 255 when there is no alpha.
    #[inline]
    pub fn unpack(&self, px: u32) -> [u8; 4] {
        if self.rgba {
            return px.to_le_bytes();
        }
        [
            self.channel(px, 0),
            self.channel(px, 1),
            self.channel(px, 2),
            self.channel(px, 3),
        ]
    }

    /// Like `SDL_MapRGBA`.
    #[inline]
    pub fn pack(&self, c: [u8; 4]) -> u32 {
        if self.rgba {
            return u32::from_le_bytes(c);
        }
        let mut px = 0u32;
        for i in 0..4 {
            if self.masks[i] != 0 {
                px |= (((c[i] as u32) >> self.losses[i]) << self.shifts[i]) & self.masks[i];
            }
        }
        px
    }

    /// The SDL pixel format enum for the masks, or 0 (unknown).
    pub fn sdl_enum(&self) -> u32 {
        const ABGR8888: u32 = 0x1676_2004;
        const ARGB8888: u32 = 0x1636_2004;
        const XBGR8888: u32 = 0x1656_1804;
        const XRGB8888: u32 = 0x1616_1804;
        const RGBA8888: u32 = 0x1646_2004;
        const BGRA8888: u32 = 0x1686_2004;
        match self.masks {
            [0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0xff00_0000] => ABGR8888,
            [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000] => ARGB8888,
            [0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0] => XBGR8888,
            [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0] => XRGB8888,
            [0xff00_0000, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff] => RGBA8888,
            [0x0000_ff00, 0x00ff_0000, 0xff00_0000, 0x0000_00ff] => BGRA8888,
            _ => 0,
        }
    }

    pub fn to_sdl(&self) -> SdlPixelFormat {
        SdlPixelFormat {
            format: self.sdl_enum(),
            palette: std::ptr::null_mut(),
            bits_per_pixel: self.bits(),
            bytes_per_pixel: 4,
            padding: [0; 2],
            rmask: self.masks[0],
            gmask: self.masks[1],
            bmask: self.masks[2],
            amask: self.masks[3],
            rloss: self.losses[0],
            gloss: self.losses[1],
            bloss: self.losses[2],
            aloss: self.losses[3],
            rshift: self.shifts[0],
            gshift: self.shifts[1],
            bshift: self.shifts[2],
            ashift: self.shifts[3],
            refcount: 1,
            next: std::ptr::null_mut(),
        }
    }
}
