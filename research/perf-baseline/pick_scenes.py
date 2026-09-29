#!/usr/bin/env python3
"""Deterministic scene specs from a discover JSON: pick_scenes.py GAME [N=20] [LAYERS=3] [SEED=11].
A scene = 1 base image + LAYERS overlay images from the same game directory, all plain `Image` displayables at the game's
dominant resolution (each shown with its own tag, so all of them are drawn). Writes out/scenes/GAME_L<layers>.json."""
import collections, json, os, random, sys
g = sys.argv[1]; n = int(sys.argv[2]) if len(sys.argv) > 2 else 20
L = int(sys.argv[3]) if len(sys.argv) > 3 else 3; seed = int(sys.argv[4]) if len(sys.argv) > 4 else 11
here = os.path.dirname(os.path.abspath(__file__))
d = json.load(open(f"{here}/out/disc/{g}.json"))
imgs = [r for r in d["images"] if r["type"] == "Image" and r["w"] and r["file"]]
dom = collections.Counter((r["w"], r["h"]) for r in imgs).most_common(1)[0][0]
imgs = [r for r in imgs if (r["w"], r["h"]) == dom]
by = collections.defaultdict(list)
for r in imgs: by[os.path.dirname(r["file"])].append(r)
dirs = sorted(k for k, v in by.items() if len(v) >= L + 1)
rnd = random.Random(seed)
scenes = []
while len(scenes) < n:
    k = rnd.choice(dirs)
    pick = rnd.sample(by[k], L + 1)
    scenes.append({"bg": pick[0]["name"], "layers": [p["name"] for p in pick[1:]], "dir": k, "res": list(dom)})
os.makedirs(f"{here}/out/scenes", exist_ok=True)
out = f"{here}/out/scenes/{g}_L{L}.json"
json.dump(scenes, open(out, "w"))
print(g, "dominant", dom, "dirs", len(dirs), "->", out)
