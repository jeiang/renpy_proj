#!/usr/bin/env python3
"""Survey one Ren'Py archive by streaming it. Nothing from the game is extracted to disk.

usage: archive_survey.py ARCHIVE OUTDIR [SCRATCH]

ARCHIVE is a .tar.zst (decoded with `zstd -dc`) or a plain .tar. The archive is read twice in one process:

  pass 1  every member name and size. Loose files under <base>/game/: RPA headers and indexes
          (only the index bytes are kept), .rpyc and .rpy/.py text (read in memory, reduced to
          regex counts), movie prefixes (ffprobe on the first 8 MiB). Engine files under
          <base>/renpy/: sha256 per file, plus the version files.
  pass 2  RPA entries only: script text and .rpyc (full), movie prefixes (ffprobe), and the largest
          movie in full (ffprobe on a short-lived file in SCRATCH).

OUTDIR gets <stem>.json (summary), <stem>.renpy.json (engine file hashes), <stem>.rpa.json (every RPA
index entry) and <stem>.movies.json (one ffprobe record per movie). Nothing else is written.
Needs zstd, ffprobe and python3 >= 3.9.
"""
import collections
import hashlib
import io
import json
import os
import pickle
import re
import struct
import subprocess
import sys
import tarfile
import zlib

PROBE_BYTES = 8 << 20
TEXT_MAX = 64 << 20
INDEX_MAX = 512 << 20
MOVIE_EXT = (".webm", ".mkv", ".ogv", ".mp4", ".avi", ".mov", ".mpg", ".mpeg")
TEXT_EXT = (".rpy", ".py", ".rpe", ".rpe.py")
SCRIPT_EXT = (".rpy", ".rpyc", ".rpymc", ".py", ".rpe", ".rpe.py")
GAME_RE = re.compile(r"^((?:[^/]+/)*?)game/(.+)$")
RENPY_RE = re.compile(r"^((?:[^/]+/)*?)renpy/(.+)$")
LIB_RE = re.compile(r"(?:^|/)lib/([^/]+)/")
# Obfuscation and custom-loader signals, matched on game text and on the source strings inside rpyc.
SIGNALS = {
    "archive_handlers": rb"archive_handlers",
    "renpy_loader": rb"renpy\.loader\b",
    "marshal": rb"\bmarshal\.loads?\b",
    "pickle_load": rb"pickle\.loads?\(|Unpickler\(",
    "zlib": rb"zlib\.(?:de)?compress",
    "base64": rb"base64\.|b64decode",
    "exec_call": rb"\bexec\(|\beval\(\s*compile",
    "xor_key": rb"\^\s*0x[0-9A-Fa-f]{2,}|\bXOR\b|\bxor\b",
    "rpa_literal": rb"RPA-\d\.\d|ALT-1\.0|ZiX-1",
    "rpe_literal": rb"\.rpe\b",
    "crypto": rb"\bAES\b|\bChaCha|\bXTEA\b|\bcryptography\b|Crypto\.",
    "rpyc_header_literal": rb"RENPY RPC2",
}
GUI_INIT = re.compile(rb"gui\.init\(\s*(\d{3,5})\s*,\s*(\d{3,5})")
CFG_SIZE = re.compile(rb"config\.screen_(width|height)\s*=\s*(\d{3,5})")


class Safe(pickle.Unpickler):
    """Index loader: any global lookup is refused (same guard as research/test-corpus/rpa_scan.py)."""

    def find_class(self, module, name):
        raise pickle.UnpicklingError(f"forbidden {module}.{name}")


def ext_of(rel):
    low = rel.lower()
    return ".rpe.py" if low.endswith(".rpe.py") else os.path.splitext(low)[1] or "(none)"


def skip(f, n):
    while n > 0:
        b = f.read(min(n, 1 << 24))
        if not b:
            raise EOFError("short read while skipping")
        n -= len(b)


def read_upto(f, n):
    out = bytearray()
    while len(out) < n:
        b = f.read(min(n - len(out), 1 << 24))
        if not b:
            break
        out += b
    return bytes(out)


def read_exact(f, n):
    data = read_upto(f, n)
    if len(data) != n:
        raise EOFError(f"wanted {n} bytes, got {len(data)}")
    return data


def open_tar(archive):
    if archive.endswith(".zst"):
        proc = subprocess.Popen(["zstd", "-dc", "-q", "--", archive], stdout=subprocess.PIPE)
        fobj = proc.stdout
    else:
        proc, fobj = None, open(archive, "rb", buffering=1 << 20)
    return proc, tarfile.open(fileobj=fobj, mode="r|", bufsize=1 << 22)


def close_tar(proc):
    if proc is not None:
        rc = proc.wait()
        if rc != 0:
            raise RuntimeError(f"zstd exited {rc}")


def ffprobe(target, data=None):
    cmd = ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
           "stream=codec_name,width,height,avg_frame_rate,pix_fmt", "-of", "json", target]
    r = subprocess.run(cmd, input=data, capture_output=True, timeout=600)
    try:
        s = json.loads(r.stdout)["streams"][0]
        return {"codec": s.get("codec_name"), "width": s.get("width"), "height": s.get("height"),
                "fps": s.get("avg_frame_rate"), "pix_fmt": s.get("pix_fmt")}
    except (ValueError, KeyError, IndexError):
        err = r.stderr.decode("utf-8", "replace").strip().splitlines()
        return {"error": (err[-1] if err else f"rc {r.returncode}")[:160]}


class Survey:
    def __init__(self, archive, scratch):
        self.archive = archive
        self.stem = re.sub(r"\.tar(\.zst)?$", "", os.path.basename(archive))
        self.scratch = scratch
        os.makedirs(scratch, exist_ok=True)
        self.members = 0
        self.total_bytes = 0
        self.links = 0
        self.bases = collections.Counter()
        self.game_ext = collections.Counter()
        self.game_ext_bytes = collections.Counter()
        self.lib = set()
        self.rpa_list = []
        self.rpyc_kinds = collections.Counter()
        self.rpyc_other = []
        self.signals = collections.Counter()
        self.signal_files = collections.defaultdict(set)
        self.screen = []
        self.python_modules = []
        self.renpy_files = {}
        self.renpy_text = {}
        self.script_version = None
        self.movies = []
        self.rep = None
        self.errors = []

    # ---- text and rpyc analysis (in memory, reduced to counts) ----
    def text(self, rel, data):
        for key, rx in SIGNALS.items():
            hits = re.findall(rx, data)
            if hits:
                self.signals[key] += len(hits)
                self.signal_files[key].add(rel)
        g = GUI_INIT.findall(data)
        c = CFG_SIZE.findall(data)
        if g or c:
            self.screen.append({"file": rel, "gui_init": [[int(a), int(b)] for a, b in g[:5]],
                                "config": [[k.decode(), int(v)] for k, v in c[:5]]})

    def rpyc(self, rel, data):
        if data[:10] == b"RENPY RPC2":
            kind, blobs, pos = "RPC2", [], 10
            try:
                while True:
                    s, st, ln = struct.unpack("<III", data[pos:pos + 12])
                    pos += 12
                    if s == 0:
                        break
                    blobs.append((s, data[st:st + ln]))
            except struct.error:
                kind += "+badtable"
        elif data[:1] == b"\x78":
            kind, blobs = "zlib-v1", [(1, data)]
        else:
            kind, blobs = "other", []
            self.rpyc_other.append([rel, data[:12].hex()])
        for s, blob in blobs:
            try:
                pk = zlib.decompressobj().decompress(blob)
            except zlib.error:
                kind += "+badzlib"
                continue
            self.text(f"{rel}#slot{s}", pk)
        self.rpyc_kinds[kind] += 1

    def script(self, rel, data):
        low = rel.lower()
        if low.endswith((".rpyc", ".rpymc")):
            self.rpyc(rel, data)
        else:
            self.text(rel, data)

    # ---- pass 1 ----
    def pass1(self):
        proc, tf = open_tar(self.archive)
        for m in tf:
            if m.issym() or m.islnk():
                self.links += 1
                continue
            if not m.isreg():
                continue
            self.members += 1
            self.total_bytes += m.size
            g = GAME_RE.match(m.name)
            if g:
                self.bases[g.group(1)] += 1
                self.loose(m.name, g.group(2), m.size, tf.extractfile(m))
                continue
            r = RENPY_RE.match(m.name)
            if r:
                self.engine(m.name, r.group(1), r.group(2), m.size, tf.extractfile(m))
                continue
            lib = LIB_RE.search(m.name)
            if lib:
                self.lib.add(lib.group(1))
        close_tar(proc)

    def loose(self, name, rel, size, f):
        low = rel.lower()
        ext = ext_of(rel)
        self.game_ext[ext] += 1
        self.game_ext_bytes[ext] += size
        if low.endswith(".rpa"):
            self.rpa_index(name, rel, size, f)
        elif low.endswith((".rpyc", ".rpymc")) and size <= TEXT_MAX:
            self.rpyc(rel, read_exact(f, size))
        elif low.endswith(TEXT_EXT) and size <= TEXT_MAX:
            self.text(rel, read_exact(f, size))
        elif low.endswith(MOVIE_EXT):
            prefix = read_upto(f, min(size, PROBE_BYTES))
            rec = {"where": "loose", "full": name, "name": rel, "size": size,
                   "complete": size <= PROBE_BYTES}
            rec.update(ffprobe("pipe:0", prefix))
            self.movies.append(rec)
        elif low == "game/script_version.txt" or rel == "script_version.txt":
            self.script_version = read_exact(f, size).decode("utf-8", "replace").strip()
        if low.endswith(".py") and ("python-packages/" in rel or "/" not in rel):
            self.python_modules.append([rel, size])

    def engine(self, name, base, rel, size, f):
        h = hashlib.sha256()
        keep = rel in ("__init__.py", "vc_version.py") and size < (1 << 20)
        parts, left = [], size
        while left:
            b = f.read(min(left, 1 << 22))
            if not b:
                break
            h.update(b)
            if keep:
                parts.append(b)
            left -= len(b)
        self.renpy_files[name] = {"size": size, "sha256": h.hexdigest()}
        if keep:
            self.renpy_text[name] = b"".join(parts).decode("utf-8", "replace")

    def rpa_index(self, name, rel, size, f):
        line = f.readline(256)
        entry = {"name": name, "rel": rel, "size": size, "header": line.strip().decode("latin-1")[:80],
                 "parsed": False, "entries": []}
        self.rpa_list.append(entry)
        parts = line.split()
        if len(parts) < 2 or parts[0] not in (b"RPA-3.0", b"RPA-2.0"):
            return
        try:
            off = int(parts[1], 16)
            key = int(parts[2], 16) if parts[0] == b"RPA-3.0" else 0
        except (ValueError, IndexError):
            entry["error"] = "bad header fields"
            return
        if off < len(line) or off >= size or size - off > INDEX_MAX:
            entry["error"] = "index offset out of range"
            return
        skip(f, off - len(line))
        raw = read_exact(f, size - off)
        try:
            idx = Safe(io.BytesIO(zlib.decompressobj().decompress(raw)), encoding="utf-8").load()
        except Exception as e:  # noqa: BLE001 - record and continue
            entry["error"] = f"index: {type(e).__name__}: {e}"[:200]
            return
        for k, v in idx.items():
            seg = v[0]
            o, ln = seg[0] ^ key, seg[1] ^ key
            pre = seg[2] if len(seg) > 2 and seg[2] else b""
            if isinstance(pre, str):
                pre = pre.encode("latin-1")
            name_k = k.decode("utf-8", "replace") if isinstance(k, bytes) else k
            entry["entries"].append([name_k, o, ln, pre.hex(), len(v)])
        entry["parsed"] = True

    # ---- pass 2 ----
    def plan(self):
        cands = [(mv["size"], 0, mv["full"], "loose", None, mv["name"]) for mv in self.movies]
        for e in self.rpa_list:
            for k, o, ln, pre, nseg in e["entries"]:
                if k.lower().endswith(MOVIE_EXT):
                    cands.append((ln, 1, k, "rpa", e["name"], k))
        if cands:
            size, _, full, where, rpa, name = max(cands, key=lambda c: (c[0], c[1]))
            self.rep = {"size": size, "where": where, "full": full, "rpa": rpa, "name": name}

    def wants(self):
        out = {}
        for e in self.rpa_list:
            if not e["parsed"]:
                continue
            ws = []
            for k, o, ln, pre_hex, nseg in e["entries"]:
                low = k.lower()
                pre = bytes.fromhex(pre_hex)
                if low.endswith(MOVIE_EXT):
                    is_rep = bool(self.rep) and self.rep["where"] == "rpa" and \
                        self.rep["rpa"] == e["name"] and self.rep["name"] == k
                    kind = "rep" if is_rep else "movie"
                elif low.endswith(SCRIPT_EXT) and ln <= TEXT_MAX:
                    kind = "script"
                else:
                    continue
                ws.append((o, ln, pre, kind, k))
            ws.sort(key=lambda w: w[0])
            if ws:
                out[e["name"]] = ws
        return out

    def pass2(self):
        wants = self.wants()
        loose_rep = self.rep["full"] if self.rep and self.rep["where"] == "loose" else None
        proc, tf = open_tar(self.archive)
        for m in tf:
            if not m.isreg():
                continue
            if m.name in wants:
                try:
                    self.walk(m.name, tf.extractfile(m), wants[m.name])
                except EOFError as e:
                    self.errors.append(f"{m.name}: {e}")
            elif loose_rep and m.name == loose_rep:
                self.loose_rep_probe(m.size, tf.extractfile(m))
        close_tar(proc)

    def walk(self, rpa_name, f, ws):
        pos = len(f.readline(256))
        for o, ln, pre, kind, k in ws:
            want = ln - len(pre)
            if o < pos or want < 0:
                self.errors.append(f"{rpa_name}::{k}: unexpected offset {o} at {pos}")
                continue
            skip(f, o - pos)
            if kind == "script":
                data = pre + read_exact(f, want)
                pos = o + want
                self.script(f"{rpa_name}::{k}", data)
                continue
            rec = {"where": f"rpa:{rpa_name}", "full": f"{rpa_name}::{k}", "name": k, "size": ln,
                   "complete": want <= PROBE_BYTES}
            if kind == "rep":
                # Read the entry once: this stream pass is the only consumer of these bytes.
                rec["representative"] = True
                rec["complete"] = True
                path = os.path.join(self.scratch, "rep-rpa.bin")
                with open(path, "wb") as w:
                    w.write(pre)
                    left = want
                    while left:
                        b = f.read(min(left, 1 << 24))
                        if not b:
                            raise EOFError("short read on representative movie")
                        w.write(b)
                        left -= len(b)
                pos = o + want
                try:
                    rec.update(ffprobe(path))
                finally:
                    os.remove(path)
            else:
                take = min(want, PROBE_BYTES)
                prefix = pre + read_exact(f, take)
                skip(f, want - take)
                pos = o + want
                rec.update(ffprobe("pipe:0", prefix[:PROBE_BYTES]))
            self.movies.append(rec)

    def loose_rep_probe(self, size, f):
        path = os.path.join(self.scratch, "rep-loose.bin")
        with open(path, "wb") as w:
            left = size
            while left:
                b = f.read(min(left, 1 << 24))
                if not b:
                    raise EOFError("short read on representative movie")
                w.write(b)
                left -= len(b)
        try:
            rec = {"where": "loose", "full": self.rep["full"], "name": self.rep["name"], "size": size,
                   "complete": True, "representative": True}
            rec.update(ffprobe(path))
        finally:
            os.remove(path)
        self.movies.append(rec)

    # ---- summary ----
    def summary(self):
        base = self.bases.most_common(1)[0][0] if self.bases else ""
        renpy_base = base + "renpy/"
        engine = {n[len(base):]: v for n, v in self.renpy_files.items() if n.startswith(renpy_base)}
        texts = {n[len(base):]: t for n, t in self.renpy_text.items() if n.startswith(renpy_base)}
        init = texts.get("renpy/__init__.py", "")
        vt = re.findall(r"version_tuple\s*=\s*\(([0-9, ]+)", init)
        vc = texts.get("renpy/vc_version.py", "")
        vcm = re.findall(r"^version\s*=\s*u?['\"]([^'\"]+)", vc, re.M)
        rpa_hdr = collections.Counter(e["header"].split(" ")[0] if e["header"] else "(empty)" for e in self.rpa_list)
        odd_hdr = [[e["rel"], e["header"], e.get("error", "")] for e in self.rpa_list
                   if not e["parsed"] or e.get("error")]
        in_rpa = collections.Counter()
        in_rpa_bytes = collections.Counter()
        for e in self.rpa_list:
            for k, o, ln, pre, nseg in e["entries"]:
                ext = ext_of(k)
                in_rpa[ext] += 1
                in_rpa_bytes[ext] += ln
        movies_all = collections.Counter()
        movies_all_b = collections.Counter()
        for e in self.rpa_list:
            for k, o, ln, pre, nseg in e["entries"]:
                if k.lower().endswith(MOVIE_EXT):
                    movies_all[ext_of(k)] += 1
                    movies_all_b[ext_of(k)] += ln
        for ext in MOVIE_EXT:
            if ext in self.game_ext:
                movies_all[ext] += self.game_ext[ext]
                movies_all_b[ext] += self.game_ext_bytes[ext]
        probes = [m for m in self.movies if "codec" in m]
        codecs = collections.Counter(f"{m['codec']} {m['width']}x{m['height']} {m['fps']}" for m in probes)
        widest = max(probes, key=lambda m: m["width"] or 0) if probes else None
        py2 = any(c.startswith(("py2-", "python2")) for c in self.lib)
        py3 = any(c.startswith(("py3-", "python3")) for c in self.lib)
        pkgs = collections.Counter()
        for rel, _size in self.python_modules:
            if rel.startswith("python-packages/"):
                pkgs[rel.split("/")[1]] += 1
            elif "/" not in rel:
                pkgs[f"(module) {rel[:-3]}"] += 1
            else:
                pkgs[f"(loose) {rel.split('/')[0]}"] += 1
        screens = [s for s in self.screen if s["gui_init"] or s["config"]]
        engine_diff = {"files": len(engine), "pyc": sum(1 for n in engine if n.endswith((".pyc", ".pyo")))}
        return {
            "stem": self.stem,
            "base": base,
            "members": self.members,
            "total_bytes": self.total_bytes,
            "symlinks": self.links,
            "bases_seen": dict(self.bases),
            "renpy_version_tuple": vt[-1].strip() if vt else None,
            "renpy_vc_version": vcm[0] if vcm else None,
            "script_version": self.script_version,
            "lib_dirs": sorted(self.lib),
            "python_lib": {"py2": py2, "py3": py3},
            "game_loose_ext": dict(self.game_ext),
            "game_loose_ext_bytes": dict(self.game_ext_bytes),
            "rpa_count": len(self.rpa_list),
            "rpa_headers": dict(rpa_hdr),
            "rpa_odd": odd_hdr,
            "rpa_entries_ext": dict(in_rpa),
            "rpa_entries_ext_bytes": dict(in_rpa_bytes),
            "rpyc_kinds": dict(self.rpyc_kinds),
            "rpyc_other": self.rpyc_other[:20],
            "signals": dict(self.signals),
            "signal_files": {k: sorted(v)[:12] for k, v in self.signal_files.items()},
            "screen_hits": screens[:20],
            "python_modules": pkgs,
            "python_module_count": len(self.python_modules),
            "engine_files": len(engine),
            "engine_pyc": engine_diff["pyc"],
            "movies_ext": {k: [movies_all[k], movies_all_b[k]] for k in movies_all},
            "movie_codecs": dict(codecs),
            "movies_probed": len(probes),
            "movies_probe_failed": len(self.movies) - len(probes),
            "widest_movie": widest,
            "rep_movie": self.rep,
            "rep_probe": next((m for m in self.movies if m.get("representative")), None),
            "errors": self.errors[:30],
        }


def main(argv):
    archive, outdir = argv[0], argv[1]
    scratch = argv[2] if len(argv) > 2 else os.path.join(outdir, "tmp")
    os.makedirs(outdir, exist_ok=True)
    s = Survey(archive, scratch)
    s.pass1()
    s.plan()
    s.pass2()
    summ = s.summary()
    summ["archive_bytes"] = os.path.getsize(archive)
    with open(os.path.join(outdir, s.stem + ".json"), "w") as f:
        json.dump(summ, f, indent=1, sort_keys=True)
    with open(os.path.join(outdir, s.stem + ".renpy.json"), "w") as f:
        json.dump(s.renpy_files, f, sort_keys=True)
    with open(os.path.join(outdir, s.stem + ".rpa.json"), "w") as f:
        json.dump(s.rpa_list, f)
    with open(os.path.join(outdir, s.stem + ".movies.json"), "w") as f:
        json.dump(s.movies, f)
    if os.path.isdir(scratch) and not os.listdir(scratch):
        os.rmdir(scratch)
    print(s.stem, "ok")


if __name__ == "__main__":
    main(sys.argv[1:])
