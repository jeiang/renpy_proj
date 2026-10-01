"""Frame diff for two PNG screenshots, stdlib only; macOS uses `sips` (decode and resample), other hosts a pure Python PNG decoder.

macOS: every PNG goes through `sips` to an uncompressed BMP: once at full size (exact equality test) and once resampled to a
common grid (COARSE_W x COARSE_H) for the metrics, so two runs at different window sizes still compare.
Linux: the PNGs are decoded with `zlib` (8-bit gray, RGB, palette, with or without alpha, non-interlaced), compared at full
size, then point-sampled onto the same grid. The gate's own Linux captures are written with filter type 0 (`write_png`),
which decodes fast; other filter types work and are slower.
"""
import os
import struct
import subprocess
import sys
import tempfile
import zlib

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


def read_ppm(data):
    """-> (width, height, rgb bytes) of a binary P6 PPM (what `grim -t ppm` writes)."""
    toks, i = [], 0
    while len(toks) < 4:
        while data[i:i + 1].isspace():
            i += 1
        if data[i:i + 1] == b"#":
            i = data.index(b"\n", i)
            continue
        j = i
        while not data[j:j + 1].isspace():
            j += 1
        toks.append(data[i:j])
        i = j
    i += 1   # one whitespace byte after maxval
    if toks[0] != b"P6" or int(toks[3]) != 255:
        raise ValueError("not an 8-bit P6 PPM")
    w, h = int(toks[1]), int(toks[2])
    if len(data) - i < w * h * 3:
        raise ValueError("short PPM: %d bytes for %dx%d" % (len(data) - i, w, h))
    return w, h, data[i:i + w * h * 3]


def write_png(path, w, h, rgb):
    """Write 8-bit RGB as a PNG with filter type 0 on every row (fast to decode) and zlib level 1."""
    row = w * 3
    raw = b"".join(b"\x00" + rgb[y * row:(y + 1) * row] for y in range(h))

    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw, 1)) + chunk(b"IEND", b""))
    with open(path, "wb") as f:
        f.write(png)


def decode_png(path):
    """-> (width, height, rgb bytes). 8-bit gray, gray+alpha, RGB, RGBA and palette images, not interlaced."""
    d = open(path, "rb").read()
    if d[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG: %s" % path)
    i, idat, plte, hdr = 8, [], b"", None
    while i < len(d):
        n, tag = struct.unpack_from(">I4s", d, i)
        body = d[i + 8:i + 8 + n]
        i += 12 + n
        if tag == b"IHDR":
            hdr = struct.unpack(">IIBBBBB", body)
        elif tag == b"PLTE":
            plte = body
        elif tag == b"IDAT":
            idat.append(body)
        elif tag == b"IEND":
            break
    w, h, depth, ctype, _c, _f, interlace = hdr
    if depth != 8 or interlace or ctype not in (0, 2, 3, 4, 6):
        raise ValueError("unsupported PNG (depth %d, colour type %d, interlace %d): %s" % (depth, ctype, interlace, path))
    bpp = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    stride = w * bpp
    raw = zlib.decompress(b"".join(idat))
    if len(raw) != (stride + 1) * h:
        raise ValueError("PNG data size mismatch: %s" % path)
    rows, prev = [], bytes(stride)
    for y in range(h):
        ft = raw[y * (stride + 1)]
        line = raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)]
        if ft == 0:
            cur = line
        else:
            cur = bytearray(line)
            if ft == 1:
                for k in range(bpp, stride):
                    cur[k] = (cur[k] + cur[k - bpp]) & 255
            elif ft == 2:
                cur = bytearray((a + b) & 255 for a, b in zip(line, prev))
            elif ft == 3:
                for k in range(stride):
                    left = cur[k - bpp] if k >= bpp else 0
                    cur[k] = (cur[k] + ((left + prev[k]) >> 1)) & 255
            elif ft == 4:
                for k in range(stride):
                    a = cur[k - bpp] if k >= bpp else 0
                    b = prev[k]
                    c = prev[k - bpp] if k >= bpp else 0
                    pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                    pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                    cur[k] = (cur[k] + pred) & 255
            else:
                raise ValueError("bad PNG filter type %d: %s" % (ft, path))
            cur = bytes(cur)
        rows.append(cur)
        prev = cur
    if ctype == 2:
        return w, h, b"".join(rows)
    out = bytearray()
    for r in rows:
        if ctype == 6:
            b = bytearray(r)
            del b[3::4]
            out += b
        elif ctype == 0:
            out += b"".join(bytes((v, v, v)) for v in r)
        elif ctype == 4:
            out += b"".join(bytes((v, v, v)) for v in r[0::2])
        else:
            for v in r:
                out += plte[3 * v:3 * v + 3]
    return w, h, bytes(out)


def _sample(w, h, rgb, tw, th):
    """Point-sample an RGB image onto a tw x th grid (the Linux stand-in for sips' resample)."""
    out = bytearray()
    xs = [min(w - 1, int((x + 0.5) * w / tw)) for x in range(tw)]
    for y in range(th):
        sy = min(h - 1, int((y + 0.5) * h / th))
        row = rgb[sy * w * 3:(sy + 1) * w * 3]
        out += b"".join(row[3 * x:3 * x + 3] for x in xs)
    return bytes(out)


def _compare_py(png_a, png_b, changed_level, crop_top):
    wa, ha, ca = decode_png(png_a)
    wb, hb, cb = decode_png(png_b)
    if crop_top:
        ca = ca[crop_top * wa * 3:(ha - crop_top) * wa * 3]
        cb = cb[crop_top * wb * 3:(hb - crop_top) * wb * 3]
        ha, hb = ha - 2 * crop_top, hb - 2 * crop_top
    rec = {"size_a": [wa, ha], "size_b": [wb, hb], "exact_equal": (wa, ha) == (wb, hb) and ca == cb}
    if rec["exact_equal"]:
        rec.update(mean_abs=0.0, pct_changed=0.0)
        return rec
    ca = _sample(wa, ha, ca, COARSE_W, COARSE_H)
    cb = _sample(wb, hb, cb, COARSE_W, COARSE_H)
    diffs = [abs(x - y) for x, y in zip(ca, cb)]
    rec["mean_abs"] = sum(diffs) / (len(diffs) * 255.0)
    changed = sum(1 for i in range(0, len(diffs), 3) if max(diffs[i:i + 3]) > changed_level)
    rec["pct_changed"] = 100.0 * changed / (len(diffs) // 3)
    rec["coarse"] = [COARSE_W, COARSE_H]
    return rec


def compare(png_a, png_b, changed_level=CHANGED_LEVEL, crop_top=0):
    """-> dict: exact_equal, size_a, size_b, mean_abs (0..1 of full scale), pct_changed (0..100).
    crop_top: pixel rows to drop from the top (and bottom) of both images first."""
    if sys.platform != "darwin":
        return _compare_py(png_a, png_b, changed_level, crop_top)
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
