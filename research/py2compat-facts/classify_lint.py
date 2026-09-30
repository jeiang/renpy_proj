#!/usr/bin/env python3
"""Bucket the lint output of the new games (out/<game>.lint.out) by message class; print a markdown table."""
import re, glob, os, collections
CLASSES = [
 ("file not loadable (image/audio path)", r"which is not loadable|not loadable"),
 ("image file case/spelling", r"case"),
 ("could not evaluate say `who`", r"Could not evaluate '.*' in the who part"),
 ("unknown/undefined name", r"is not defined|not a known|Unknown|unknown"),
 ("unused/other lint warning", r"^\S+\.rpy:\d+ "),
 ("Python-related message", r"Traceback|NameError|TypeError|AttributeError|SyntaxError|ModuleNotFound|Exception"),
]
print("| game | lint rc | lines | " + " | ".join(c[0] for c in CLASSES) + " |")
print("|---|---|---|" + "---|" * len(CLASSES))
for f in sorted(glob.glob("out/*.lint.out")):
    g = os.path.basename(f).split(".")[0]
    lines = [l for l in open(f, errors="replace").read().splitlines() if l.strip()]
    c = collections.Counter()
    for l in lines:
        for name, rx in CLASSES:
            if re.search(rx, l): c[name] += 1; break
    print("| %s | | %d | " % (g, len(lines)) + " | ".join(str(c[n]) if c[n] else "." for n, _ in CLASSES) + " |")
