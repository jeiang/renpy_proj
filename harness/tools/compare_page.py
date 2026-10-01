#!/usr/bin/env python3
"""Self-contained HTML page that compares two engines' screenshots of the anisotropic-filtering test game.

    python3 harness/tools/compare_page.py --out harness/out/aniso/index.html \
        --set "macOS: Ren'Py 8.5.3 (OpenGL) vs player (Metal)" harness/out/aniso/mac-stock/route-1/shots harness/out/aniso/mac-player/route-1/shots \
        --set "Linux: ..." <stock shots> <player shots>

One section per `--set`, one row per screenshot name found in both folders: stock, player, difference (|a-b| x4, clamped) and
numbers. Every shot is cropped to the game area (title bar and the 84 px caption strip removed). Left half of the game area
draws with gl_anisotropic True, right half with False. Numbers are on a 0..255 scale unless noted:
  mean_abs   mean absolute difference over all channels of the whole crop, stock against player
  on / off   the same for the left (aniso True) and the right (aniso False) half
  >24        share of pixels where any channel differs by more than 24
  own on/off mean_abs between the left and the right half of ONE engine (how much the flag changes that engine's picture);
             the halves are mirrored scenes only for the tilt cases, so this is a plain pixel difference of the two halves
Python 3, standard library only. The page embeds the PNGs (data URIs), so it can be moved or mailed alone.
"""
import argparse
import base64
import html
import operator
import pathlib
import struct
import sys
import zlib

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from gatelib.pngdiff import decode_png  # noqa: E402

VIRT_W, VIRT_H, CAPTION = 1280, 720, 84   # the game's virtual size and the caption strip height


def crop_game(w, h, rgb):
    """-> (w, h, rows) of the game area: no title bar, no caption strip. Scale from the width."""
    s = w / VIRT_W
    content_h = round(VIRT_H * s)
    top = max(0, h - content_h) + round(CAPTION * s)
    row = w * 3
    return w, h - top, rgb[top * row:h * row]


def png_bytes(w, h, rgb):
    row = w * 3
    raw = b"".join(b"\x00" + rgb[y * row:(y + 1) * row] for y in range(h))

    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


def halves(w, h, rgb):
    """Left and right half as flat byte strings."""
    hw = (w // 2) * 3
    row = w * 3
    left = b"".join(rgb[y * row:y * row + hw] for y in range(h))
    right = b"".join(rgb[y * row + row - hw:(y + 1) * row] for y in range(h))
    return left, right


def absdiff(a, b):
    return bytes(map(abs, map(operator.sub, a, b)))


def mean(d):
    return sum(d) / len(d) if d else 0.0


def changed_share(d, level=24):
    """Share of pixels (3 bytes each) with any channel over `level`."""
    n = len(d) // 3
    bad = sum(1 for i in range(0, len(d), 3) if d[i] > level or d[i + 1] > level or d[i + 2] > level)
    return 100.0 * bad / n if n else 0.0


def uri(png):
    return "data:image/png;base64," + base64.b64encode(png).decode()


def measure(a, b):
    """Numbers for two cropped shots of equal size."""
    w, h = a[:2]
    d = absdiff(a[2], b[2])
    dl, dr = halves(w, h, d)
    row = {"mean": mean(d), "changed": changed_share(d), "on": mean(dl), "off": mean(dr)}
    for key, (pw, ph, prgb) in (("own_stock", a), ("own_player", b)):
        l, r = halves(pw, ph, prgb)
        row[key] = mean(absdiff(l, r))
    return row, d


def compare_set(title, sdir, pdir, adir=None):
    names = sorted(p.stem for p in pathlib.Path(sdir).glob("*.png") if (pathlib.Path(pdir) / p.name).exists())
    rows = []
    for n in names:
        if adir and not (pathlib.Path(adir) / (n + ".png")).exists():
            continue
        a = crop_game(*decode_png(str(pathlib.Path(sdir) / (n + ".png"))))
        b = crop_game(*decode_png(str(pathlib.Path(pdir) / (n + ".png"))))
        row = {"name": n, "sa": a[:2], "sb": b[:2]}
        if a[:2] != b[:2]:
            row["error"] = "size mismatch: stock %dx%d, player %dx%d" % (a[0], a[1], b[0], b[1])
            row["imgs"] = [uri(png_bytes(*a)), uri(png_bytes(*b)), None]
            rows.append(row)
            continue
        w, h = a[:2]
        m, d = measure(a, b)
        row.update(m)
        imgs = [uri(png_bytes(*a)), uri(png_bytes(*b))]
        if adir:
            c = crop_game(*decode_png(str(pathlib.Path(adir) / (n + ".png"))))
            if c[:2] != a[:2]:
                row["error"] = "size mismatch: after %dx%d" % c[:2]
            else:
                m2, d = measure(a, c)
                row["after"] = m2
                imgs.append(uri(png_bytes(*c)))
        amp = bytes(map(lambda v: 255 if v > 63 else v * 4, d))
        imgs.append(uri(png_bytes(w, h, amp)))
        row["imgs"] = imgs
        rows.append(row)
        print("%s / %s: mean_abs %.3f (on %.3f, off %.3f)" % (title, n, row["mean"], row["on"], row["off"]), flush=True)
    return {"title": title, "rows": rows}


CSS = """body{font:14px/1.4 system-ui,sans-serif;margin:16px;background:#fafafa;color:#111}
h2{margin-top:40px}.row{border-top:2px solid #444;padding:8px 0}.imgs{display:flex;gap:6px}
.imgs figure{margin:0;flex:1 1 0;min-width:0}.imgs img{width:100%;image-rendering:pixelated;cursor:zoom-in;background:#000}
.imgs img.full{width:auto;max-width:none}.imgs figure.big{flex:0 0 auto}figcaption{font-size:12px;color:#555}
table.n{border-collapse:collapse;font-variant-numeric:tabular-nums}table.n td,table.n th{padding:1px 8px;text-align:right;border-bottom:1px solid #ddd}
.err{color:#b00}code{background:#eee;padding:0 3px}"""

JS = "document.addEventListener('click',e=>{if(e.target.tagName=='IMG'){e.target.classList.toggle('full');e.target.parentElement.classList.toggle('big')}})"


def render(sets, intro):
    o = ["<!doctype html><meta charset=utf-8><title>Anisotropic filtering: stock vs player</title><style>%s</style>" % CSS,
         "<h1>Anisotropic filtering: stock Ren'Py 8.5.3 against the player</h1>", intro]
    for s in sets:
        o.append("<h2>%s</h2>" % html.escape(s["title"]))
        for r in s["rows"]:
            o.append('<div class="row"><b>%s</b>' % html.escape(r["name"]))
            if r.get("error"):
                o.append(' <span class=err>%s</span>' % html.escape(r["error"]))
            o.append('<div class=imgs>')
            caps = ["stock (left: aniso True, right: False)", "player", "difference x4"]
            if len(r["imgs"]) == 4:
                caps = ["stock (left: aniso True, right: False)", "player before fix", "player after fix", "difference stock vs after, x4"]
            for cap, im in zip(caps, r["imgs"]):
                if im:
                    o.append('<figure><img src="%s" loading=lazy><figcaption>%s</figcaption></figure>' % (im, cap))
            o.append("</div>")
            if "after" in r:
                x = r["after"]
                o.append("<table class=n><tr><th><th>mean_abs<th>on (left)<th>off (right)<th>&gt;24 %%<th>own on/off, stock<th>own on/off, player"
                         "<tr><th>before<td>%.3f<td>%.3f<td>%.3f<td>%.2f<td>%.3f<td>%.3f"
                         "<tr><th>after<td>%.3f<td>%.3f<td>%.3f<td>%.2f<td>%.3f<td>%.3f</table>"
                         % (r["mean"], r["on"], r["off"], r["changed"], r["own_stock"], r["own_player"],
                            x["mean"], x["on"], x["off"], x["changed"], x["own_stock"], x["own_player"]))
            elif "mean" in r:
                o.append("<table class=n><tr><th>mean_abs<th>on (left)<th>off (right)<th>&gt;24 %%<th>own on/off, stock<th>own on/off, player"
                         "<tr><td>%.3f<td>%.3f<td>%.3f<td>%.2f<td>%.3f<td>%.3f</table>"
                         % (r["mean"], r["on"], r["off"], r["changed"], r["own_stock"], r["own_player"]))
            o.append("</div>")
    o.append("<script>%s</script>" % JS)
    return "\n".join(o)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--set", nargs="+", action="append", metavar="TITLE STOCK_SHOTS PLAYER_SHOTS [PLAYER_AFTER_SHOTS]", required=True,
                    help="a 4th folder adds an 'after' player column and before/after numbers")
    ap.add_argument("--json", help="also write the numbers here")
    a = ap.parse_args()
    sets = [compare_set(*x) for x in a.set]
    intro = ("<p>One row per case. Left half of each picture: <code>gl_anisotropic True</code>; right half: <code>False</code>. "
             "Click a picture for full size. See harness/testgames/aniso/README.md for what to look at.</p>")
    pathlib.Path(a.out).write_text(render(sets, intro))
    if a.json:
        import json
        pathlib.Path(a.json).write_text(json.dumps([{**s, "rows": [{k: v for k, v in r.items() if k != "imgs"} for r in s["rows"]]} for s in sets], indent=1))
    print("wrote", a.out)


if __name__ == "__main__":
    main()
