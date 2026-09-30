//! Row-parallel loop helper.

use rayon::prelude::*;

/// Pixel count above which a per-row loop is worth splitting across threads.
const PAR_MIN_PIXELS: usize = 1 << 16;

/// Runs `f(y)` for `y` in `0..h`. Rows run in parallel when the image is
/// large. `f` must only touch memory that belongs to its own row.
pub fn rows<F>(w: usize, h: usize, f: F)
where
    F: Fn(usize) + Sync + Send,
{
    if w.saturating_mul(h) >= PAR_MIN_PIXELS && h > 1 {
        (0..h).into_par_iter().for_each(f);
    } else {
        (0..h).for_each(f);
    }
}

/// Same as [`rows`], for a loop over `n` independent lines of `len` pixels.
pub fn lines<F>(n: usize, len: usize, f: F)
where
    F: Fn(usize) + Sync + Send,
{
    rows(len, n, f)
}
