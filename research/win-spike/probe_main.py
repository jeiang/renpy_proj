# Runs inside the embedded interpreter, imported from the zip appended to the exe.
import sys, importlib, importlib.machinery, importlib.util, time, json

class BuiltinSubmoduleImporter:   # same idea as renpy-build runtime/site3.py L73-90
    @staticmethod
    def find_spec(fullname, path=None, target=None):
        if path is None or "." not in fullname or fullname not in sys.builtin_module_names:
            return None
        i = importlib.machinery.BuiltinImporter
        return importlib.util.spec_from_loader(fullname, i, origin=i._ORIGIN)
sys.meta_path.append(BuiltinSubmoduleImporter)

print("version", sys.version.split()[0], "executable", sys.executable)
print("sys.path", sys.path, "flags.isolated", sys.flags.isolated, "utf8_mode", sys.flags.utf8_mode)
print("prefix", sys.prefix, "meta_path", [getattr(x,"__name__",repr(x)) for x in sys.meta_path])

# Stdlib modules the 8.5.3 census saw at runtime (mods.txt, passed via PROBE_MODS).
import probe_pkg                       # a package inside the zip
import probe_pkg.fast                  # Rust builtin submodule of a zip-loaded package
print("probe_pkg loader", probe_pkg.__loader__.__name__, "fast.ANSWER", probe_pkg.fast.ANSWER,
      "fast.__spec__.origin", probe_pkg.fast.__spec__.origin)
mods = __import__('os').environ['PROBE_MODS'].split()
t=time.perf_counter(); ok=[]; bad=[]
for m in mods:
    try: importlib.import_module(m); ok.append(m)
    except Exception as e: bad.append((m, type(e).__name__, str(e)[:80]))
print("imported", len(ok), "of", len(mods), "in %.1f ms" % ((time.perf_counter()-t)*1000))
print("failed", bad)
import zlib, pickle, zipfile, socket, hashlib, json as _j, ast
print("loader of json:", _j.__spec__.loader.__name__, _j.__spec__.origin)
print("json is blob-loaded:", _j.__spec__.loader.__name__ == "_Loader", "; math builtin:", "math" in sys.builtin_module_names)
import ctypes; print("ctypes ok", ctypes.sizeof(ctypes.c_void_p))
