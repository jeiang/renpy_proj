#!/usr/bin/env python3
"""Compile every .py under DIRs with the running python3 (use 3.12); report SyntaxError kinds (no source printed)."""
import sys, os, collections, warnings
warnings.simplefilter("ignore")
for d in sys.argv[1:]:
    ok = 0; bad = collections.Counter(); pkgs = collections.Counter()
    for root, _, fs in os.walk(d):
        for fn in fs:
            if fn.endswith(".py"):
                p = os.path.join(root, fn)
                try: compile(open(p, "rb").read(), p, "exec"); ok += 1
                except SyntaxError as e:
                    bad[str(e).split(" (")[0]] += 1; pkgs[os.path.relpath(p, d).split(os.sep)[0]] += 1
    print(d, "compile ok:", ok, "SyntaxError:", dict(bad), "by top-level pkg:", dict(pkgs))
