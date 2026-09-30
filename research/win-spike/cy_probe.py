# Runs in the embedded interpreter (imported from the blob). Imports every rebuilt Ren'Py Cython module by its dotted
# builtin name and reports ok/failure per module. Module list in env CY_MODS.
import sys, os, importlib, importlib.machinery, importlib.util, traceback, time
class BuiltinSubmoduleImporter:   # same as renpy-build runtime/site3.py
    @staticmethod
    def find_spec(fullname, path=None, target=None):
        if path is None or "." not in fullname or fullname not in sys.builtin_module_names:
            return None
        i = importlib.machinery.BuiltinImporter
        return importlib.util.spec_from_loader(fullname, i, origin=i._ORIGIN)
sys.meta_path.append(BuiltinSubmoduleImporter)

# Stand-ins for the modules the route replaces with Rust (pygame layer, GL, fonts, audio, _renpy): any attribute is a
# dummy class, so modules that `import`/`from .. import` them at load time get past that line. Not a claim that they work.
import types
class _Meta(type):
    def __getattr__(cls, k):
        if k.startswith("__"): raise AttributeError(k)
        return cls
    def __iter__(cls): return iter(())
    def __lt__(cls, o): return False
    def __gt__(cls, o): return False
    def __call__(cls, *a, **k): return super().__call__()
class _Dummy(metaclass=_Meta):
    def __getattr__(self, k):
        if k.startswith("__"): raise AttributeError(k)
        return self
    def __iter__(self): return iter(())
    def __call__(self, *a, **k): return self
class _Stub(types.ModuleType):
    __path__ = []
    def __getattr__(self, n):
        if n.startswith("__"): raise AttributeError(n)
        c = type(n, (_Dummy,), {}); setattr(self, n, c); return c
class _StubFinder:
    PREFIX = ("renpy.pygame", "renpy.uguu", "renpy.gl2.gl2draw", "renpy.gl2.gl2texture", "renpy.gl2.gl2shader", "renpy.gl2.gl2uniform",
              "renpy.gl2.assimp", "renpy.gl2.live2dmodel", "renpy.text.ftfont", "renpy.text.hbfont", "renpy.text.bidi",
              "renpy.audio.renpysound", "_renpy")
    @classmethod
    def find_spec(cls, fullname, path=None, target=None):
        if any(fullname == p or fullname.startswith(p + ".") for p in cls.PREFIX):
            return importlib.machinery.ModuleSpec(fullname, cls, is_package=True)
    create_module = staticmethod(lambda spec: _Stub(spec.name))
    exec_module = staticmethod(lambda m: None)
sys.meta_path.insert(0, _StubFinder)
sys.platform_ = sys.platform
os.environ.setdefault("RENPY_PLATFORM", "windows-x86_64")
t = time.perf_counter()
try:
    import renpy
    print("import renpy: ok %.0f ms" % ((time.perf_counter() - t) * 1000), renpy.version_only if hasattr(renpy, "version_only") else "")
except BaseException:
    print("import renpy FAILED"); traceback.print_exc(file=sys.stdout)
try:
    renpy.import_all(); print("renpy.import_all(): OK")
except BaseException as e:
    print("renpy.import_all(): FAIL", type(e).__name__, str(e)[:160])
res = {}
for m in os.environ["CY_MODS"].split():
    t = time.perf_counter()
    try:
        importlib.import_module(m); res[m] = "ok %.1f ms" % ((time.perf_counter() - t) * 1000)
    except BaseException as e:
        tb = traceback.extract_tb(e.__traceback__)[-1]
        res[m] = "FAIL %s: %s (%s:%d)" % (type(e).__name__, str(e)[:110], os.path.basename(tb.filename), tb.lineno)
for m, r in res.items(): print("%-52s %s" % (m, r))
print("builtin renpy modules:", sum(1 for x in sys.builtin_module_names if x.startswith("renpy.")))
