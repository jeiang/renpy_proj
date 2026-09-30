#!/usr/bin/env python3
"""Copy a seeded sample of corpus images into gitignored scratch/ (never committed).
Sources: SecretIsland (loose), Astral (loose 4K webp), WaifuAcademy + Ripples (RPA, via perf-baseline's rpa_get.py)."""
import json, pathlib, random, shutil, subprocess, sys
R = pathlib.Path("/Users/aidanp/Projects/renpy_proj")
B = R / ".worktrees/perf-baseline/corpus"
out = pathlib.Path(__file__).parent / "scratch"
random.seed(24)
def loose(root, ext, n, lo, hi, tag):
    fs = [p for p in root.rglob("*") if p.suffix.lower() == ext and lo <= p.stat().st_size <= hi]
    for i, p in enumerate(random.sample(sorted(fs), n)):
        shutil.copy(p, out / f"{tag}_{i}{ext}")
loose(R / "corpus/SecretIsland-0.18.8.0-pc-released/game/images", ".png", 6, 300_000, 4_000_000, "si_png")
loose(R / "corpus/SecretIsland-0.18.8.0-pc-released/game/images", ".webp", 4, 100_000, 4_000_000, "si_webp")
loose(B / "astral-853/game/images", ".webp", 4, 400_000, 6_000_000, "astral_webp")
rows = [l.rstrip("\n").split("\t") for l in open("/tmp/rpa_list.tsv")]
def rpa(arc, ext, n, lo, hi, tag):
    c = [r[1] for r in rows if r[0] == arc and r[1].lower().endswith(ext) and lo <= int(r[2]) <= hi]
    names = random.sample(sorted(c), n)
    json.dump(names, open("/tmp/want.json", "w"))
    tmp = out / "_tmp"; shutil.rmtree(tmp, ignore_errors=True)
    subprocess.run([sys.executable, str(R / "research/perf-baseline/rpa_get.py"), str(tmp), "/tmp/want.json",
                    str(B / ("wa-853/game/archive.rpa" if arc == "archive.rpa" else "rip-853/game/images.rpa"))], check=True, stdout=subprocess.DEVNULL)
    for i, nm in enumerate(names): shutil.move(tmp / nm, out / f"{tag}_{i}{ext}")
    shutil.rmtree(tmp)
rpa("archive.rpa", ".jpg", 5, 150_000, 2_000_000, "wa_jpg")
rpa("archive.rpa", ".png", 3, 300_000, 4_000_000, "wa_png")
rpa("images.rpa", ".webp", 4, 100_000, 3_000_000, "rip_webp")
rpa("images.rpa", ".jpg", 3, 150_000, 3_000_000, "rip_jpg")
rpa("images.rpa", ".png", 3, 300_000, 6_000_000, "rip_png")
