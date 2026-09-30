//! Pixel storage and the bounds-checked view that pixel operations use.

use std::sync::atomic::AtomicU64;

use crate::sdl::Format;

/// An owned byte allocation that is shared between a surface and its
/// subsurfaces. Writes go through raw pointers, so `&self` methods can write
/// (the same aliasing model as the C `SDL_Surface`).
pub struct Buf {
    ptr: *mut u8,
    len: usize,
}

// The buffer is plain bytes. Racing writers can tear pixels, as in C; they
// cannot cause memory unsafety because every access is bounds checked.
unsafe impl Send for Buf {}
unsafe impl Sync for Buf {}

impl Buf {
    pub fn from_vec(v: Vec<u8>) -> Buf {
        let b = v.into_boxed_slice();
        let len = b.len();
        let ptr = Box::into_raw(b) as *mut u8;
        Buf { ptr, len }
    }

    pub fn zeroed(len: usize) -> Buf {
        Buf::from_vec(vec![0u8; len])
    }

    #[inline]
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        // SAFETY: `ptr` and `len` come from `Box::<[u8]>::into_raw` in `from_vec`.
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                self.ptr, self.len,
            )));
        }
    }
}

pub struct Shared {
    pub buf: Buf,
    /// Bumped on every write through this crate. Subsurfaces share it.
    pub generation: AtomicU64,
}

/// A copyable window on a surface's pixels: the root allocation, the offset of
/// pixel (0, 0), the pitch and the size. Reads outside the root allocation
/// return 0 and writes there are dropped, so the C-style "read one pixel past
/// the edge" of the bilinear scalers stays memory safe.
#[derive(Clone, Copy)]
pub struct Img {
    root: *mut u8,
    root_len: usize,
    off: usize,
    pub pitch: usize,
    pub w: usize,
    pub h: usize,
    pub fmt: Format,
}

unsafe impl Send for Img {}
unsafe impl Sync for Img {}

impl Img {
    /// # Safety
    /// `off + (h - 1) * pitch + w * 4 <= root_len` must hold when `w, h > 0`,
    /// and the allocation must outlive every use of the returned value.
    pub unsafe fn new(
        root: *mut u8,
        root_len: usize,
        off: usize,
        pitch: usize,
        w: usize,
        h: usize,
        fmt: Format,
    ) -> Img {
        Img {
            root,
            root_len,
            off,
            pitch,
            w,
            h,
            fmt,
        }
    }

    #[inline]
    fn abs(&self, o: isize, n: usize) -> Option<usize> {
        let a = self.off as isize + o;
        if a >= 0 && (a as usize).checked_add(n)? <= self.root_len {
            Some(a as usize)
        } else {
            None
        }
    }

    /// Byte at `o` bytes from pixel (0, 0); 0 when outside the allocation.
    #[inline]
    pub fn rd8(&self, o: isize) -> u8 {
        match self.abs(o, 1) {
            // SAFETY: `abs` checked the range.
            Some(a) => unsafe { *self.root.add(a) },
            None => 0,
        }
    }

    #[inline]
    pub fn wr8(&self, o: isize, v: u8) {
        if let Some(a) = self.abs(o, 1) {
            // SAFETY: `abs` checked the range.
            unsafe { *self.root.add(a) = v }
        }
    }

    #[inline]
    pub fn rd32(&self, o: isize) -> u32 {
        match self.abs(o, 4) {
            // SAFETY: `abs` checked the range.
            Some(a) => unsafe { (self.root.add(a) as *const u32).read_unaligned() },
            None => 0,
        }
    }

    #[inline]
    pub fn wr32(&self, o: isize, v: u32) {
        if let Some(a) = self.abs(o, 4) {
            // SAFETY: `abs` checked the range.
            unsafe { (self.root.add(a) as *mut u32).write_unaligned(v) }
        }
    }

    /// Pixel value at (x, y). `x < w` and `y < h` must hold.
    #[inline]
    pub fn px(&self, x: usize, y: usize) -> u32 {
        self.rd32((y * self.pitch + x * 4) as isize)
    }

    #[inline]
    pub fn put(&self, x: usize, y: usize, v: u32) {
        self.wr32((y * self.pitch + x * 4) as isize, v)
    }

    /// Byte offset of (x, y) from pixel (0, 0).
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> isize {
        (y * self.pitch + x * 4) as isize
    }

    /// Start of row `y` (`y < h`). Valid for `w * 4` bytes.
    #[inline]
    pub fn row_ptr(&self, y: usize) -> *mut u8 {
        debug_assert!(y < self.h);
        // SAFETY: the constructor contract keeps rows inside the allocation.
        unsafe { self.root.add(self.off + y * self.pitch) }
    }

    pub fn pixels_ptr(&self) -> *mut u8 {
        // SAFETY: `off <= root_len` by the constructor contract.
        unsafe { self.root.add(self.off) }
    }

    /// Number of bytes from pixel (0, 0) to the end of the last row.
    pub fn span(&self) -> usize {
        if self.w == 0 || self.h == 0 {
            0
        } else {
            (self.h - 1) * self.pitch + self.w * 4
        }
    }

    pub fn shares_memory(&self, other: &Img) -> bool {
        self.root == other.root
    }

    /// True if the byte ranges of the two views intersect.
    pub fn overlaps(&self, other: &Img) -> bool {
        if !self.shares_memory(other) || self.span() == 0 || other.span() == 0 {
            return false;
        }
        let a = self.off..self.off + self.span();
        let b = other.off..other.off + other.span();
        a.start < b.end && b.start < a.end
    }

    /// Copies out the used bytes of row `y`.
    pub fn row_bytes(&self, y: usize) -> &[u8] {
        // SAFETY: rows are inside the allocation; callers do not write the row
        // while the slice lives (the slice is used within a single operation).
        unsafe { std::slice::from_raw_parts(self.row_ptr(y), self.w * 4) }
    }
}
