import re, sys, pathlib, json
root = pathlib.Path(sys.argv[1])
games = sys.argv[2:]
def rows(p):
    out = {}
    for l in p.read_text().splitlines():
        m = re.match(r"\| (lint|probe|route|saveresume|video) \| (\w+) \| [\d.]+ \| (.*) \|$", l)
        if m: out[m.group(1)] = (m.group(2), m.group(3))
    return out
for kind in ("stock", "full"):
    print("##", kind)
    print("| Game | lint | probe | route | saveresume | video |"); print("|---|---|---|---|---|---|")
    for g in games:
        p = root / f"{g}-{kind}" / "summary.md"
        if not p.exists(): print(f"| {g} | not run |||||"); continue
        r = rows(p)
        def cell(k):
            if k not in r: return "-"
            s, e = r[k]; e = e.split(";")[0][:70]
            return f"{s}: {e}"
        print(f"| {g} | " + " | ".join(cell(k) for k in ("lint","probe","route","saveresume","video")) + " |")
