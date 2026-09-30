r"""Approach (b) probe: compile CPython 3.12.8 core (+ the extension modules whose sources are in the tarball) with MSVC /MT and
Py_NO_ENABLE_SHARED into one static lib. The file list is PBS's PYTHON.json object list (same PCbuild pythoncore/extension
membership), mapped by basename onto the python.org source tree. Run in the MSVC env from C:\spike.
Usage: c1_static_crt_build.py <src-root> <outdir>"""
import json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor
src, out = sys.argv[1:3]
os.makedirs(out, exist_ok=True)
info = json.load(open(r"C:\spike\pbs\PYTHON.json"))["build_info"]
by = {}
for root, _, fs in os.walk(src):
    if os.sep + "Lib" in root or os.sep + "Tools" in root or os.sep + "Doc" in root or "Programs" in root: continue
    for f in fs:
        if f.endswith(".c"): by.setdefault(f[:-2], []).append(os.path.join(root, f))
pref = ["Python", "Objects", "Parser", "PC", os.path.join("Modules", "_io"), "Modules"]
def pick(name, grp="core"):
    c = by.get(name, [])
    if grp != "core":   # e.g. libmpdec context.c vs Python/context.c
        g = [p for p in c if os.sep + grp + os.sep in p or (grp == "_decimal" and "_decimal" in p) or (grp == "_bz2" and "bzip2-1.0.8" in p)]
        if g: c = g
    if not c: return None
    z = [p for p in c if "zlib-1.3.1" in p]
    if z and grp == "core": return z[0]
    c.sort(key=lambda p: min((i for i, d in enumerate(pref) if os.sep + d + os.sep in p or p.endswith(os.sep + d)), default=99))
    return c[0]
items = [("core", o) for o in info["core"]["objs"]]
EXT = ["_socket", "select", "unicodedata", "pyexpat", "_elementtree", "_asyncio", "_queue", "_zoneinfo", "_uuid", "_multiprocessing", "_overlapped", "_decimal", "_ctypes", "_bz2", "_ssl", "_hashlib"]
for m in EXT:
    items += [(m, o) for o in info["extensions"][m][0]["objs"]]
seen = set(); jobs = []; missing = []
for grp, o in items:
    n = os.path.basename(o)[:-4]
    key = (grp if grp != "core" else "core", n)
    p = pick(n, grp)
    if p is None: missing.append((grp, n)); continue
    if p in seen: continue
    seen.add(p); jobs.append((grp, n, p))
print("files", len(jobs), "missing", missing)
inc = ["Include", "Include\\internal", "PC", "Python", "Modules\\expat", "Modules", "externals\\bzip2-1.0.8", "Modules\\_hacl\\include", "externals\\zlib-1.3.1"]
flags = ["/nologo", "/c", "/O2", "/MT", "/utf-8", "/w", "/DNDEBUG", "/DWIN32", "/D_WINDOWS", "/DPy_NO_ENABLE_SHARED", "/D_CRT_SECURE_NO_WARNINGS",
         "/DUSE_PYEXPAT_CAPI", "/DXML_STATIC", "/DHAVE_EXPAT_CONFIG_H", "/DPy_BUILD_CORE", "/D_Py_HAVE_ZLIB", "/DUSE_ZLIB_CRC32", '/DMS_DLL_ID="3.12"', "/DPY3_DLLNAME=L\"python3\""] + ["/I" + os.path.join(src, i) for i in inc]
def build(j):
    grp, n, p = j
    o = os.path.join(out, grp + "__" + n + ".obj")
    ex = ["/DPy_BUILD_CORE_BUILTIN"] if grp != "core" else []
    if grp in ("_ssl", "_hashlib"): ex += ["/IC:\\spike\\vcpkg\\installed\\x64-windows-static\\include"]
    if grp == "_ctypes": ex += ["/DFFI_STATIC_BUILD", "/IC:\\spike\\vcpkg\\installed\\x64-windows-static\\include"]
    if grp == "_decimal": ex += ["/DCONFIG_64=1", "/DANSI=1", "/I" + os.path.join(src, "Modules", "_decimal", "libmpdec")]
    if n == "getpath": ex += ["/DPREFIX=NULL", "/DEXEC_PREFIX=NULL", "/DVERSION=NULL", '/DVPATH="..\\\\.."', '/DPYDEBUGEXT=""', '/DPLATLIBDIR="DLLs"']
    if n == "sysmodule": ex += ['/DVPATH="..\\\\.."']
    r = subprocess.run(["cl"] + flags + ex + [p, "/Fo" + o], capture_output=True, text=True)
    return grp, n, p, r.returncode, (r.stdout + r.stderr)
with ThreadPoolExecutor(4) as ex: res = list(ex.map(build, jobs))
bad = [(g, n, [l for l in log.splitlines() if "error" in l][:2]) for g, n, p, rc, log in res if rc]
print("compiled", len(res) - len(bad), "failed", len(bad))
for b in bad: print(" FAIL", *b)
json.dump({"failed": bad, "missing": missing}, open(os.path.join(out, "result.json"), "w"), indent=1)
