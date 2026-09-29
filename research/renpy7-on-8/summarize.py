#!/usr/bin/env python3
"""Summarise out/: per game+engine+mode the first exception line (or lint statistics) from the captured files."""
import pathlib, re
out = pathlib.Path(__file__).parent / "out"
runs = sorted({p.name[:-4] for p in out.glob("*__*.*.out")})
for r in runs:
    game_eng, mode = r.rsplit(".", 1)
    tb = out / f"{game_eng}.{mode}.traceback.txt"
    o = (out / f"{game_eng}.{mode}.out").read_text(errors="replace")
    if tb.exists():
        lines = [l for l in tb.read_text(errors="replace").splitlines() if l.strip()]
        err = next((l for l in lines if re.match(r"^\w*(Error|Exception|Warning)\b|^\w+\.\w+Error|^\w*Error:", l) or "Error" in l.split(":")[0]), lines[3] if len(lines) > 3 else "")
        where = next((l.strip() for l in lines if l.strip().startswith("File \"game")), "")
        print(f"{game_eng:55s} {mode:6s} TRACEBACK {err[:110]} @ {where[:90]}")
    elif "Statistics" in o:
        print(f"{game_eng:55s} {mode:6s} lint-ok ({len(re.findall(r'^game/', o, re.M))} game/ msgs)")
    else:
        print(f"{game_eng:55s} {mode:6s} no traceback; out tail: {o.strip().splitlines()[-1][:100] if o.strip() else ''}")
