#!/usr/bin/env python3
"""Turn out/probes-<mode>.txt into a markdown table: tag, engine, alive-at-45s, traceback (first exception line), progress."""
import re, sys
mode = sys.argv[1]
txt = open(f"{__file__.rsplit('/',1)[0]}/out/probes-{mode}.txt").read()
print("| run | engine | process at 45 s | traceback.txt | script progress (dialogue lines / labels) | window screenshot |")
print("|---|---|---|---|---|---|")
rows = {}
for blk in re.split(r"^== ", txt, flags=re.M)[1:]:
    head = blk.splitlines()[0]
    m = re.match(r"(\S+) on (\S+)", head)
    tag, eng = m.groups()
    alive = "alive" if "alive after" in blk else "exited"
    tb = "none"
    if "TRACEBACK" in blk:
        lines = blk.split("TRACEBACK", 1)[1].splitlines()
        tb = next((l.strip() for l in lines if re.search(r"(Error|Exception)\b", l) and "File" not in l), "yes")[:90]
    pr = re.search(r"progress: (\d*) say lines, (\d*) labels", blk)
    prog = f"{pr.group(1) or 0} / {pr.group(2) or 0}" if pr else "-"
    shot = "yes" if "screenshot: /" in blk else "no"
    rows[(tag, eng)] = f"| {tag} | {eng} | {alive} | {tb} | {prog} | {shot} |"   # later runs of the same probe replace earlier ones
print("\n".join(rows.values()))
