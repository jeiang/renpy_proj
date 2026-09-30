# pyhost start-up module. Executed once, right after Py_InitializeFromConfig, as module `_pyhost_blob`.
# Uses only builtin/frozen modules: sys, _imp, _blob, zlib, marshal, _frozen_importlib.
# Port of research/pypack/bootstrap_blob.py, extended to several zips, package resources and dotted builtins.
import sys, _imp, _blob, zlib, marshal

_bs = sys.modules["_frozen_importlib"]
ModuleSpec = _bs.ModuleSpec


class _Zip:
    """Read-only index over one embedded zip (memoryview, no copy)."""

    def __init__(self, name, buf):
        self.name = name
        self.buf = buf
        b = buf
        u16 = lambda o: int.from_bytes(b[o:o + 2], "little")
        u32 = lambda o: int.from_bytes(b[o:o + 4], "little")
        eocd = bytes(b[max(0, len(b) - 65557):]).rfind(b"PK\x05\x06") + max(0, len(b) - 65557)
        if eocd < max(0, len(b) - 65557):
            raise ValueError("pyhost: %s is not a zip file" % name)
        n, pos = u16(eocd + 10), u32(eocd + 16)
        idx = {}
        for _ in range(n):
            m, csz, nl, el, cl, lo = u16(pos + 10), u32(pos + 20), u16(pos + 28), u16(pos + 30), u16(pos + 32), u32(pos + 42)
            nm = str(bytes(b[pos + 46:pos + 46 + nl]), "utf-8")
            idx[nm] = (lo, csz, m)
            pos += 46 + nl + el + cl
        self.idx = idx
        self.u16 = u16

    def read(self, entry):
        lo, csz, m = self.idx[entry]
        o = lo + 30 + self.u16(lo + 26) + self.u16(lo + 28)
        raw = bytes(self.buf[o:o + csz])
        return zlib.decompress(raw, -15) if m == 8 else raw


class _Loader:
    """Loader and resource provider for modules stored as .pyc in one zip."""

    def __init__(self, z):
        self.z = z

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        code = self.get_code(module.__name__)
        exec(code, module.__dict__)

    def _entry(self, fullname):
        base = fullname.replace(".", "/")
        for fn in (base + ".pyc", base + "/__init__.pyc"):
            if fn in self.z.idx:
                return fn
        raise ImportError("no module %r in %s" % (fullname, self.z.name), name=fullname)

    def get_code(self, fullname):
        return marshal.loads(self.z.read(self._entry(fullname))[16:])

    def get_source(self, fullname):
        self._entry(fullname)
        return None

    def is_package(self, fullname):
        return self._entry(fullname).endswith("/__init__.pyc")

    def get_filename(self, fullname):
        return self.z.name + "/" + self._entry(fullname)[:-1]

    def get_data(self, path):
        p = str(path).replace("\\", "/")
        prefix = self.z.name + "/"
        if p.startswith(prefix):
            p = p[len(prefix):]
        try:
            return self.z.read(p)
        except KeyError:
            raise OSError(2, "No such resource in %s" % self.z.name, p) from None


class _Finder:
    """meta_path finder for one zip. Package `__path__` is `<zip name>/<dir>`."""

    def __init__(self, z):
        self.z = z
        self.loader = _Loader(z)

    def find_spec(self, fullname, path=None, target=None):
        idx = self.z.idx
        base = fullname.replace(".", "/")
        if base + ".pyc" in idx:
            fn, pkg = base + ".pyc", False
        elif base + "/__init__.pyc" in idx:
            fn, pkg = base + "/__init__.pyc", True
        else:
            return None
        origin = self.z.name + "/" + fn[:-1]  # logical source name, like __file__ of a .py module
        s = ModuleSpec(fullname, self.loader, origin=origin, is_package=pkg)
        s.has_location = True
        s.cached = None
        if pkg:
            s.submodule_search_locations = [self.z.name + "/" + base]
        return s


class BuiltinSubmoduleImporter:
    """Dotted names registered with PyImport_AppendInittab (for example `renpy.display.render`) win over
    same-named modules inside zip packages. Port of renpy-build runtime/site3.py."""

    @staticmethod
    def find_spec(fullname, path=None, target=None):
        if "." in fullname and _imp.is_builtin(fullname):
            return ModuleSpec(fullname, _bs.BuiltinImporter, origin="built-in")
        return None


def install(zips):
    mp = sys.meta_path
    # Before PathFinder (last entry) so embedded modules win over anything found on sys.path.
    at = len(mp)
    for i, f in enumerate(mp):
        if getattr(f, "__name__", "") == "PathFinder":
            at = i
            break
    for name, buf in zips:
        mp.insert(at, _Finder(_Zip(name, buf)))
        at += 1
    mp.insert(0, BuiltinSubmoduleImporter)


def run(modname, funcname):
    """Import `modname`, call `funcname()`, return the process exit code. Prints the traceback on failure."""
    try:
        try:
            mod = __import__(modname, fromlist=["_"])
            result = getattr(mod, funcname)()
        finally:
            pass
        if result is None:
            code = 0
        elif isinstance(result, int):
            code = result
        else:
            code = 0
    except SystemExit as e:
        c = e.code
        if c is None:
            code = 0
        elif isinstance(c, int):
            code = c
        else:
            try:
                sys.stderr.write(str(c) + "\n")
            except BaseException:
                pass
            code = 1
    except BaseException:
        try:
            sys.excepthook(*sys.exc_info())
        except BaseException:
            pass
        code = 1
    for s in (sys.stdout, sys.stderr):
        try:
            s.flush()
        except BaseException:
            pass
    return code
