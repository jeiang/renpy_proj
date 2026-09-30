#!/usr/bin/env python3
"""Per-format mean ms/MP (min over rounds, mean over files) from data/decode.tsv and data/baseline_sdl.json."""
import csv, collections, json
rows = list(csv.DictReader(open("data/decode.tsv"), delimiter="\t"))
mp = collections.defaultdict(list); mx = collections.defaultdict(int)
for r in rows:
    if r["min_ms"].startswith("ERR") or r["w"] == "-": continue
    mp[(r["fmt"], r["decoder"])].append(float(r["min_ms"]) / (int(r["w"]) * int(r["h"]) / 1e6))
    mx[(r["fmt"], r["decoder"])] = max(mx[(r["fmt"], r["decoder"])], int(r["max_abs_diff"]))
base = collections.defaultdict(list)
for k, v in json.load(open("data/baseline_sdl.json")).items():
    base[k.rsplit(".", 1)[1].replace("jpeg", "jpg")].append(v["decode_ms"] / (v["w"] * v["h"] / 1e6))
print("fmt\tdecoder\tms_per_MP\tfiles\tmax_abs_diff_vs_ref")
for f in ("png", "jpg", "webp"):
    print(f"{f}\tSDL2_image via pygame_sdl2 (stock 8.5.3)\t{sum(base[f])/len(base[f]):.2f}\t{len(base[f])}\t-")
    for (fmt, d), v in sorted(mp.items(), key=lambda kv: sum(kv[1]) / len(kv[1])):
        if fmt == f: print(f"{f}\t{d}\t{sum(v)/len(v):.2f}\t{len(v)}\t{mx[(fmt,d)]}")
