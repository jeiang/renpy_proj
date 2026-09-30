#!/usr/bin/env python3
"""Stock-engine decode baseline: Ren'Py 8.5.3's own pygame_sdl2 (SDL2_image) on scratch/ images.
Run with the 8.5.3 SDK's python (research/shared-engine-launcher/sdk), headless:
  SDL_VIDEODRIVER=dummy <sdk>/lib/py3-mac-universal/python baseline_sdl.py
Measures what Image.load does before the texture step: pygame.image.load (decode), then
pgrender.copy_surface (approximated by Surface.copy(), one full-frame copy), then Surface.get_bounding_rect
(optimize_texture_bounds). Median of 7 after 1 warm-up, milliseconds."""
import glob, json, os, statistics, sys, time
SDK = "/Users/aidanp/Projects/renpy_proj/research/shared-engine-launcher/sdk/renpy-8.5.3-sdk"
sys.path[:0] = [SDK + "/lib/python3.12", SDK]
import renpy.pygame as pg
here = os.path.dirname(os.path.abspath(__file__))
def med(f, n=7):
    f(); xs = []
    for _ in range(n):
        t = time.perf_counter(); f(); xs.append((time.perf_counter() - t) * 1000)
    return statistics.median(xs)
out = {}
for fn in sorted(glob.glob(here + "/scratch/*.*")):
    data = open(fn, "rb").read()
    import io
    def dec(): return pg.image.load(io.BytesIO(data), os.path.basename(fn))
    s = dec(); w, h = s.get_size()
    def cp():
        return s.copy()  # stand-in for accelerator.nogil_copy (needs an initialised renpy); both are one full-surface copy
    d = cp()
    out[os.path.basename(fn)] = dict(w=w, h=h, bytes=len(data), decode_ms=round(med(dec), 2),
        copy_ms=round(med(cp), 2), bounds_ms=round(med(lambda: d.get_bounding_rect()), 2))
    print(os.path.basename(fn), out[os.path.basename(fn)], flush=True)
json.dump(out, open(here + "/data/baseline_sdl.json", "w"), indent=1)
