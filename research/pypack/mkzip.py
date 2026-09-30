"""Build the stdlib zip: legacy-layout .pyc (mod.pyc, no __pycache__), deflated, unchecked-hash pycs.
Usage: mkzip.py <libdir> <out.zip> [extra dir/file ...]   (run with the SAME minor version as the target: 3.12)"""
import sys, os, zipfile, marshal, importlib.util, pathlib
lib, out, *extra = sys.argv[1:]
SKIP_DIRS = {"test", "tests", "idlelib", "tkinter", "turtledemo", "lib2to3", "ensurepip", "site-packages",
             "lib-dynload", "distutils", "pydoc_data", "__pycache__"}
def add(z, src, arc):
    code = compile(pathlib.Path(src).read_bytes(), "<embedded>/" + arc, "exec", dont_inherit=True)
    # flags=1: unchecked hash-based pyc, so zipimport never compares source mtime.
    z.writestr(arc[:-3] + ".pyc", importlib.util.MAGIC_NUMBER + (1).to_bytes(4, "little") + b"\0" * 8 + marshal.dumps(code))
n = 0
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for root, dirs, files in os.walk(lib):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS and not d.startswith("config-")]
        rel = os.path.relpath(root, lib).replace(os.sep, "/")
        rel = "" if rel == "." else rel + "/"
        for f in files:
            if f.endswith(".py") and f != "turtle.py":
                add(z, os.path.join(root, f), rel + f); n += 1
    for e in extra:
        p = pathlib.Path(e)
        if p.is_file(): add(z, p, p.name); n += 1
        else:
            for f in p.rglob("*.py"):
                add(z, f, str(f.relative_to(p.parent))); n += 1
print(out, n, "modules", os.path.getsize(out), "bytes")
