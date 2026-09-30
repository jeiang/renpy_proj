r"""Compile the Cython-generated C (tmp/gen3-static/*.c, from gen_cython.py) with MSVC against the PBS 3.12 headers.
Run in C:\spike\renpy-src inside the MSVC env. Writes out\cy\<mod>.obj + <mod>.log and results.json (per-module rc).
Usage: b1_compile.py <pbs-include-dir> <outdir> [mod ...]"""
import subprocess, sys, os, json, glob
from concurrent.futures import ThreadPoolExecutor
inc, out, *only = sys.argv[1:]
os.makedirs(out, exist_ok=True)
extra = {"renpy.tfd": ["src/tinyfiledialogs/tinyfiledialogs.c"]}
CRT = os.environ.get("CRT", "/MD")   # /MT for the static-CRT variant (then also set PY_STATIC=1)
STATIC = ["/DPy_NO_ENABLE_SHARED"] if os.environ.get("PY_STATIC") else []
cflags = ["/nologo", "/c", "/O2", CRT, "/utf-8", "/w"] + STATIC + [x for i in inc.split(",") for x in ("/I", i)] + [ "/Isrc", "/Itmp/gen3-static", "/Isrc/pygame/include", "/I."]
def one(cfile):
    mod = os.path.basename(cfile)[:-2]
    obj = os.path.join(out, mod + ".obj")
    cmd = ["cl"] + cflags + [cfile, "/Fo" + obj]
    r = subprocess.run(cmd, capture_output=True, text=True)
    log = r.stdout + r.stderr; rc = r.returncode
    for i, x in enumerate(extra.get(mod, [])):
        o2 = os.path.join(out, mod + f".x{i}.obj")
        r2 = subprocess.run(["cl"] + cflags + [x, "/Fo" + o2], capture_output=True, text=True)
        log += r2.stdout + r2.stderr; rc = rc or r2.returncode
    open(os.path.join(out, mod + ".log"), "w").write(log)
    return mod, rc, [l for l in log.splitlines() if "error" in l][:3]
files = sorted(glob.glob("tmp/gen3-static/*.c"))
if only: files = [f for f in files if os.path.basename(f)[:-2] in only]
with ThreadPoolExecutor(4) as ex: res = list(ex.map(one, files))
json.dump({m: {"rc": rc, "errors": e} for m, rc, e in res}, open(os.path.join(out, "results.json"), "w"), indent=1)
for m, rc, e in res: print("OK  " if rc == 0 else "FAIL", m, *e)
