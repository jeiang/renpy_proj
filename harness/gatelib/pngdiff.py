"""Frame diff for two PNG screenshots, stdlib plus macOS `sips` (used to decode and resample).

Every PNG goes through `sips` to an uncompressed BMP: once at full size (exact equality test) and once resampled to a
common grid (COARSE_W x COARSE_H) for the metrics, so two runs at different window sizes still compare.
"""
import os
import struct
import subprocess
import tempfile

COARSE_W, COARSE_H = 640, 360
CHANGED_LEVEL = 24   # a pixel counts as changed when any channel differs by more than this (of 255)


def _sips_bmp(png, dest, size=None):
    cmd = ["/usr/bin/sips", "-s", "format", "bmp"]
    if size:
        cmd += ["--resampleHeightWidth", str(size[1]), str(size[0])]
    r = subprocess.run(cmd + [png, "--out", dest], capture_output=True, text=True)
    if r.returncode != 0 or not os.path.exists(dest):
        raise RuntimeError("sips failed on %s: %s" % (png, r.stderr.strip() or r.stdout.strip()))


def _read_bmp(path):
    """-> (width, height, bytes-per-pixel, rows as list of bytes, top-down). BI_RGB 24/32 bit or BI_BITFIELDS 32 bit."""
    d = open(path, "rb").read()
    if d[:2] != b"BM":
        raise ValueError("not a BMP")
    off = struct.unpack_from("<I", d, 10)[0]
    w, h, _planes, bpp = struct.unpack_from("<iiHH", d, 18)
    if bpp not in (24, 32):
        raise ValueError("unsupported BMP depth %d" % bpp)
    bottom_up = h > 0
    h = abs(h)
    bp = bpp // 8
    stride = (w * bp + 3) & ~3
    rows = [d[off + y * stride: off + y * stride + w * bp] for y in range(h)]
    if bottom_up:
        rows.reverse()
    return w, h, bp, rows


def _channels(rows, bp):
    """Flatten to BGR bytes (drop alpha)."""
    if bp == 3:
        return b"".join(rows)
    out = bytearray()
    for r in rows:
        b = bytearray(r)
        del b[3::4]
        out += b
    return bytes(out)


def _crop_top(png, rows, dest):
    """Copy of `png` without its top `rows` pixel rows (the window title bar: its focus state and title differ per run).
    sips crops to the centre, so the same number of rows also goes from the bottom: both images lose the same band."""
    r = subprocess.run(["/usr/bin/sips", "-g", "pixelWidth", "-g", "pixelHeight", png], capture_output=True, text=True).stdout.split()
    w, h = int(r[-3]), int(r[-1])
    c = subprocess.run(["/usr/bin/sips", "-c", str(h - 2 * rows), str(w), png, "--out", dest],
                       capture_output=True, text=True)
    if c.returncode != 0:
        raise RuntimeError("sips crop failed: " + c.stderr.strip())
    return dest


def compare(png_a, png_b, changed_level=CHANGED_LEVEL, crop_top=0):
    """-> dict: exact_equal, size_a, size_b, mean_abs (0..1 of full scale), pct_changed (0..100).
    crop_top: pixel rows to drop from the top (and bottom) of both images first."""
    with tempfile.TemporaryDirectory(prefix="pngdiff-") as td:
        if crop_top:
            png_a = _crop_top(png_a, crop_top, os.path.join(td, "a.png"))
            png_b = _crop_top(png_b, crop_top, os.path.join(td, "b.png"))
        fa, fb = os.path.join(td, "a.bmp"), os.path.join(td, "b.bmp")
        _sips_bmp(png_a, fa)
        _sips_bmp(png_b, fb)
        wa, ha, bpa, ra = _read_bmp(fa)
        wb, hb, bpb, rb = _read_bmp(fb)
        rec = {"size_a": [wa, ha], "size_b": [wb, hb]}
        ca, cb = _channels(ra, bpa), _channels(rb, bpb)
        rec["exact_equal"] = (wa, ha) == (wb, hb) and ca == cb
        if rec["exact_equal"]:
            rec.update(mean_abs=0.0, pct_changed=0.0)
            return rec
        for name, png in (("a", png_a), ("b", png_b)):
            _sips_bmp(png, os.path.join(td, name + "c.bmp"), (COARSE_W, COARSE_H))
        _, _, bpa, ra = _read_bmp(os.path.join(td, "ac.bmp"))
        _, _, bpb, rb = _read_bmp(os.path.join(td, "bc.bmp"))
        ca, cb = _channels(ra, bpa), _channels(rb, bpb)
    n = len(ca) // 3
    diffs = [abs(x - y) for x, y in zip(ca, cb)]
    rec["mean_abs"] = sum(diffs) / (len(diffs) * 255.0)
    changed = 0
    for i in range(0, len(diffs), 3):
        if max(diffs[i:i + 3]) > changed_level:
            changed += 1
    rec["pct_changed"] = 100.0 * changed / n
    rec["coarse"] = [COARSE_W, COARSE_H]
    return rec
