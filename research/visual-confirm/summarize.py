#!/usr/bin/env python3
"""Digest evidence/*.run.txt|progress.txt|traceback.txt into results.tsv (no game text: exception class/message line only, say counts, image tag names)."""
import pathlib, re
E = pathlib.Path(__file__).parent / "evidence"
rows = []
for run in sorted(E.glob("*.run.txt")):
    tag = run.name[:-8]
    t = run.read_text(errors="replace")
    prog = (E / f"{tag}.progress.txt")
    p = prog.read_text(errors="replace") if prog.exists() else ""
    shots = len(re.findall(r"^shot [^:]+: /", t, re.M))
    noshot = len(re.findall(r"^shot [^:]+: NO", t, re.M))
    menu = "menu True" in p
    says = len(re.findall(r"^say \d+$", p, re.M))
    tb = "traceback.txt: PRESENT" in t
    exc = ""
    tbf = E / f"{tag}.traceback.txt"
    if tb and tbf.exists():
        lines = [l for l in tbf.read_text(errors="replace").splitlines() if re.match(r"^\w*(Error|Exception)\b|^\w+Error:", l)]
        exc = lines[-1][:120] if lines else "(see traceback)"
    rows.append((tag, shots, noshot, "yes" if menu else "no", says, "PRESENT" if tb else "absent", exc))
out = ["tag\tshots\tshots_missing\tmain_menu\tsay_lines\ttraceback\texception"] + ["\t".join(map(str, r)) for r in rows]
(pathlib.Path(__file__).parent / "results.tsv").write_text("\n".join(out) + "\n")
print("\n".join(out))
