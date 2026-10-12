#!/usr/bin/env python3
"""Aggregate per-archive survey JSON into survey-downloads.tsv.

Usage: aggregate.py OUTDIR ARCHIVE_LIST TSV_OUT [CORRUPT_LIST]

OUTDIR holds <stem>.json from archive_survey.py.
ARCHIVE_LIST holds lines of "Played|<archive>.tar.zst" or "Unplayed|<archive>.tar.zst".
CORRUPT_LIST, optional, holds archive names that failed to read, one per line.
Cells for rpy, rpyc and images use loose+archived form, for example "48+125".
"""
import glob
import json
import os
import re
import sys

COLUMNS = [
    "game", "status", "archive", "renpy", "script_version", "python",
    "rpy", "rpyc", "images", "rpa_count", "rpa_versions", "videos",
    "video_codecs", "largest_video", "gui_init_resolution", "signals",
    "archive_bytes", "notes",
]

IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".webp", ".gif", ".avif", ".bmp"}


def load(path):
    with open(path) as f:
        return json.load(f)


def version_from_tuple(t, parts_max=None):
    """'(8, 4, 2)', '8, 0, 3,' or [8, 0, 3] to '8.4.2'. Empty when absent."""
    if not t:
        return ""
    if isinstance(t, str):
        parts = re.findall(r"\d+", t)
    else:
        parts = [str(p) for p in t]
    if parts_max:
        parts = parts[:parts_max]
    return ".".join(parts)


def python_summary(lib):
    if not isinstance(lib, dict):
        return ""
    return ",".join(sorted(k for k, v in lib.items() if v))


def rpa_versions(headers):
    if not isinstance(headers, dict):
        return ""
    return ";".join(f"{k}x{v}" for k, v in sorted(headers.items()))


def codec_summary(codecs):
    if not isinstance(codecs, dict):
        return ""
    ordered = sorted(codecs.items(), key=lambda kv: -kv[1])
    return ";".join(f"{k} ({v})" for k, v in ordered)


def gui_summary(hits):
    """Distinct gui.init resolutions across screen_hits, with hit counts."""
    if not isinstance(hits, list):
        return ""
    counts = {}
    for h in hits:
        for w, hh in h.get("gui_init", []) or []:
            key = f"{w}x{hh}"
            counts[key] = counts.get(key, 0) + 1
    ordered = sorted(counts.items(), key=lambda kv: -kv[1])
    return ";".join(f"{k} ({v})" for k, v in ordered)


def signal_summary(signals):
    if not isinstance(signals, dict):
        return ""
    return ";".join(f"{k}={v}" for k, v in sorted(signals.items()) if v)


def cell(loose, archived):
    return f"{loose}+{archived}"


def read_archive_list(path):
    entries = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            source, archive = line.split("|", 1)
            entries.append((source, archive))
    return entries


def match_archive(stem, entries):
    """Pick the archive whose name (without .tar.zst) starts with the JSON stem."""
    hits = [(s, a) for s, a in entries if a[: -len(".tar.zst")].startswith(stem)]
    if len(hits) != 1:
        return None
    return hits[0]


def row_for(outdir, stem, source, archive, corrupt):
    d = load(os.path.join(outdir, f"{stem}.json"))
    ext_loose = d.get("game_loose_ext") or {}
    ext_arch = d.get("rpa_entries_ext") or {}

    rpy = cell(ext_loose.get(".rpy", 0), ext_arch.get(".rpy", 0))
    rpyc = cell(ext_loose.get(".rpyc", 0), ext_arch.get(".rpyc", 0))
    images = cell(
        sum(v for k, v in ext_loose.items() if k.lower() in IMAGE_EXTS),
        sum(v for k, v in ext_arch.items() if k.lower() in IMAGE_EXTS),
    )

    widest = d.get("widest_movie") or {}
    largest = ""
    if isinstance(widest, dict) and widest.get("width"):
        largest = f"{widest['width']}x{widest['height']}"
        if not widest.get("complete", True):
            largest += " (partial probe)"

    notes = []
    if d.get("errors"):
        notes.append(f"errors={len(d['errors'])}")
    if d.get("movies_probe_failed"):
        notes.append(f"movie_probe_failed={d['movies_probe_failed']}")
    bases = d.get("bases_seen") or {}
    if len(bases) > 1:
        notes.append(f"bases={len(bases)}")
    if d.get("symlinks"):
        notes.append(f"symlinks={d['symlinks']}")
    if corrupt:
        notes.append("archive corrupt, re-download")

    return {
        "game": stem,
        "status": source,
        "archive": archive,
        "renpy": version_from_tuple(d.get("renpy_version_tuple"), parts_max=3)
        or version_from_tuple(d.get("renpy_vc_version"), parts_max=3),
        "script_version": version_from_tuple(d.get("script_version")),
        "python": python_summary(d.get("python_lib")),
        "rpy": rpy,
        "rpyc": rpyc,
        "images": images,
        "rpa_count": str(d.get("rpa_count", "")),
        "rpa_versions": rpa_versions(d.get("rpa_headers")),
        "videos": f"{d.get('movies_probed', '')}/{d.get('movies_probe_failed', '')}",
        "video_codecs": codec_summary(d.get("movie_codecs")),
        "largest_video": largest,
        "gui_init_resolution": gui_summary(d.get("screen_hits")),
        "signals": signal_summary(d.get("signals")),
        "archive_bytes": str(d.get("archive_bytes", "")),
        "notes": "; ".join(notes),
    }


def main():
    outdir, list_path, tsv_out = sys.argv[1], sys.argv[2], sys.argv[3]
    corrupt = set()
    if len(sys.argv) > 4:
        with open(sys.argv[4]) as f:
            corrupt = {line.strip() for line in f if line.strip()}
    entries = read_archive_list(list_path)

    stems = sorted(
        os.path.basename(p)[: -len(".json")]
        for p in glob.glob(os.path.join(outdir, "*.json"))
        if not p.endswith((".renpy.json", ".rpa.json", ".movies.json"))
    )
    rows, unmatched = [], []
    for stem in stems:
        hit = match_archive(stem, entries)
        if hit is None:
            unmatched.append(stem)
            continue
        source, archive = hit
        rows.append(row_for(outdir, stem, source, archive, archive in corrupt))
    # Archives with no JSON (for example the corrupt MatrixHearts) still get a row.
    matched = {r["archive"] for r in rows}
    for source, archive in entries:
        if archive not in matched:
            rows.append({
                "game": archive[: -len(".tar.zst")],
                "status": source,
                "archive": archive,
                "notes": "archive corrupt, re-download" if archive in corrupt else "no survey output",
            })

    rows.sort(key=lambda r: r["game"].lower())
    with open(tsv_out, "w") as f:
        f.write("\t".join(COLUMNS) + "\n")
        for r in rows:
            f.write("\t".join(str(r.get(c, "")).replace("\t", " ") for c in COLUMNS) + "\n")
    print(f"wrote {len(rows)} rows to {tsv_out}; unmatched json stems: {unmatched}")


if __name__ == "__main__":
    main()
