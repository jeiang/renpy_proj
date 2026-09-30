#!/usr/bin/env python3
"""L1 census over every save copied to scratch/. Writes out/census.jsonl (gitignored) + prints aggregates."""
import glob, json, os, sys, zlib
from collections import Counter
from multiprocessing import Pool
import savescan as S

def one(p):
    if p.endswith(".save"):
        r = S.summarize_save(p)
        r["kind"] = "save"
    else:
        r = {"path": p, "kind": "persistent"}
        try:
            data, sig = S.load_persistent(p)
            sc = S.scan_opcodes(data)
            r.update(protocol=sc["protocol"], error=sc["error"], py2_str_ops=sc["py2_str_ops"],
                     py2_markers=sorted(sc["py2_markers"]), has_signature=bool(sig),
                     globals={"%s %s" % k: v for k, v in sc["globals"].items()})
        except Exception as e:
            r["error"] = "%s: %s" % (type(e).__name__, e)
    r["path"] = os.path.relpath(p, "scratch")
    return r

if __name__ == "__main__":
    # Save dirs are <save_directory>, which may itself contain a slash (Company/id): cover 1 and 2 levels.
    # scratch/run*, scratch/x-* hold probe output, not real saves.
    files = [p for p in sorted(glob.glob("scratch/*/*.save") + glob.glob("scratch/*/*/*.save") + glob.glob("scratch/*/persistent") + glob.glob("scratch/*/*/persistent"))
             if not p.split("/")[1].startswith(("run", "x-"))]
    os.makedirs("out", exist_ok=True)
    done = set()
    if os.path.exists("out/census.jsonl"):  # resumable: a killed run keeps its finished rows
        for l in open("out/census.jsonl"):
            try: done.add(json.loads(l)["path"])
            except Exception: pass  # torn last line
    files = [p for p in files if os.path.relpath(p, "scratch") not in done]
    with Pool() as pool, open("out/census.jsonl", "a") as f:
        for r in pool.imap_unordered(one, files, chunksize=20):
            f.write(json.dumps(r, default=list) + "\n")
    print(len(files), "files")
