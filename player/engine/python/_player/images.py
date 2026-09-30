"""Image loader pool glue (engine patches 0100-0103, surface crate `loader`).

`prefetch(im, priority)` queues the files an image will read on the Rust pool.
`renpy.pygame.image.load` then takes the decoded pixels instead of decoding.
"""

import atexit
import functools

import renpy
from renpy.pygame import image as _image

# Formats the pool decodes. Other formats load on the calling thread as before.
_FORMATS = (".png", ".jpg", ".jpeg", ".webp")

# One third of GPU memory goes to the image cache, at most this many MB.
_CACHE_DIVISOR = 3
_CACHE_MAX_MB = 4096

_STOCK_CACHE_MB = 400


def _read(filename):
    """Runs on a pool thread: the bytes of one game image."""
    with renpy.loader.load(filename, directory="images") as f:
        return f.read()


def prefetch(im, priority):
    """Queues every file that loading `im` reads. Never blocks."""

    ce = renpy.display.im.cache.cache.get(im)
    if ce is not None and ce.texture is not None:
        return

    try:
        files = im.predict_files()
    except Exception:
        # A missing image is reported when the game draws it, not here.
        return

    for filename in files:
        if filename.lower().endswith(_FORMATS):
            _image.prefetch(filename, functools.partial(_read, filename), priority)


def cache_size_mb(stock_mb):
    """
    The image cache size in MB. A game that set `config.image_cache_size_mb`
    keeps its value. With the stock default, use a third of the GPU memory that
    `renpy.gl2.wgpudraw.gpu_memory_bytes()` reports, never less than stock.
    """

    if stock_mb != _STOCK_CACHE_MB:
        return stock_mb

    try:
        from renpy.gl2 import wgpudraw
    except ImportError:
        return stock_mb

    gpu_memory_bytes = getattr(wgpudraw, "gpu_memory_bytes", None)
    if gpu_memory_bytes is None:
        return stock_mb

    total = gpu_memory_bytes()
    if not total:
        return stock_mb

    return max(stock_mb, min(_CACHE_MAX_MB, total // _CACHE_DIVISOR >> 20))


def stats():
    """(workers, submitted, ready hits, waited hits, misses, wait ms) since start."""
    return _image.prefetch_stats()


atexit.register(_image.prefetch_shutdown)
