"""Probe: does a candidate Python runtime execute Ren'Py's *unmodified* rollback/AST-rewrite code?

Loads real renpy/revertable.py and the real WrapNode AST transformer (renpy/python.py L374-wrap_node)
against stub `renpy` modules. Run with: <python> probe_semantics.py <path-to-renpy-checkout>
"""
import sys, types, ast, marshal, pickle, copyreg, weakref, io, platform

R = sys.argv[1]
results = []
def check(name, fn):
    try:
        fn(); results.append((name, "ok"))
    except BaseException as e:
        results.append((name, "FAIL %s: %s" % (type(e).__name__, str(e)[:120])))

# --- stubs
renpy = types.ModuleType("renpy"); renpy.__path__ = []
sys.modules["renpy"] = renpy
compat = types.ModuleType("renpy.compat")
import builtins
for n in "PY2 basestring bchr bord chr open pystr range round str tobytes unicode".split():
    setattr(compat, n, getattr(builtins, n, None))
sys.modules["renpy.compat"] = renpy.compat = compat
game = types.ModuleType("renpy.game")
class Log: mutated = {}
game.log = Log()
sys.modules["renpy.game"] = renpy.game = game
config = types.ModuleType("renpy.config"); config.list_compression_length = 25; config.developer = False
sys.modules["renpy.config"] = renpy.config = config

# --- load real revertable.py
src = open(R + "/renpy/revertable.py").read()
rv = types.ModuleType("renpy.revertable"); rv.__file__ = "revertable.py"
sys.modules["renpy.revertable"] = renpy.revertable = rv
def load_rv():
    exec(compile(src, "renpy/revertable.py", "exec"), rv.__dict__)
check("exec real revertable.py", load_rv)
rv_ok = hasattr(rv, "RevertableList")

if rv_ok:
    def t_list():
        Log.mutated.clear()
        l = rv.RevertableList([1,2,3]); Log.mutated.clear()
        clean = l._clean(); l.append(4)
        assert id(l) in Log.mutated and Log.mutated[id(l)][1] == [1,2,3]
        l._rollback(clean); assert list(l) == [1,2,3]
    check("RevertableList mutate/rollback", t_list)
    def t_dict():
        Log.mutated.clear()
        d = rv.RevertableDict(a=1); Log.mutated.clear()
        d["b"] = 2; assert id(d) in Log.mutated
        d._rollback(Log.mutated[id(d)][1]); assert dict(d) == {"a": 1}
    check("RevertableDict mutate/rollback", t_dict)
    def t_set():
        Log.mutated.clear()
        s = rv.RevertableSet([1]); Log.mutated.clear()
        s.add(2); assert id(s) in Log.mutated
    check("RevertableSet mutate", t_set)
    def t_obj():
        Log.mutated.clear()
        class Foo(rv.RevertableObject): pass
        o = Foo(); o.x = 1; assert id(o) in Log.mutated
        w = weakref.ref(o)  # mutator relies on weakrefs to RevertableObject
        assert w() is o
    check("RevertableObject __setattr__ mutator + weakref", t_obj)
    def t_pickle():
        class Foo(rv.RevertableObject): pass
        globals()["Foo"] = Foo; Foo.__module__ = "__main__"; Foo.__qualname__ = "Foo"
        for o in (rv.RevertableList([1,2]), rv.RevertableDict(a=1), rv.RevertableSet([1]), Foo()):
            for proto in (2, pickle.HIGHEST_PROTOCOL):
                o2 = pickle.loads(pickle.dumps(o, proto)); assert type(o2) is type(o)
    check("pickle roundtrip of Revertable* (copyreg._reconstructor patch)", t_pickle)

# --- real WrapNode
psrc = open(R + "/renpy/python.py").read().splitlines()
start = next(i for i,l in enumerate(psrc) if l.startswith("class LoadedVariables"))
end = next(i for i,l in enumerate(psrc) if l.startswith("wrap_node = WrapNode()"))
ns = {"ast": ast, "sys": sys, "renpy": renpy, "Any": object}
def load_wrap():
    exec(compile("\n".join(psrc[start:end+1]), "python.py-excerpt", "exec"), ns)
check("exec real WrapNode/LoadedVariables", load_wrap)
if "wrap_node" in ns:
    SRC = '''
x = [i*2 for i in range(3)]
d = {"a": 1, **{"b": 2}}
s = {1, 2}
a, *b = [1, 2, 3]
match d:
    case {"a": 1, **rest}:
        r = rest
    case _:
        r = None
class C: pass
def f[T](y: T) -> T:
    return y
t = f"{x!r:>{len(x)}} {'nested'!r}"
'''
    def t_wrap():
        flags = 0
        tree = compile(SRC, "<t>", "exec", ast.PyCF_ONLY_AST | flags, True)
        tree = ns["wrap_node"].visit(tree)
        ast.fix_missing_locations(tree)
        code = compile(tree, "<t>", "exec", flags, True)
        seen = []
        g = {"__renpy__list__": lambda v=(): (seen.append("list"), list(v))[1],
             "__renpy__dict__": lambda *a, **k: (seen.append("dict"), dict(*a, **k))[1],
             "__renpy__set__": lambda v=(): (seen.append("set"), set(v))[1],
             "__name__": "store"}
        exec(code, g)
        assert g["x"] == [0, 2, 4] and "list" in seen and "dict" in seen and "set" in seen, seen
        assert g["r"] == {"b": 2}, g["r"]
        assert g["f"](3) == 3
        return code
    check("WrapNode rewrite -> compile(AST) -> exec (3.12 syntax: PEP 695, match, nested f-string)", t_wrap)
    def t_marshal():
        code = compile("y = 1 + 2", "<t>", "exec")
        c2 = marshal.loads(marshal.dumps(code)); g = {}; exec(c2, g); assert g["y"] == 3
    check("marshal.dumps/loads of code object (Ren'Py bytecode cache)", t_marshal)

def t_frame():
    def inner(): return sys._getframe(1).f_globals["__name__"]
    assert inner() == __name__
check("sys._getframe(1).f_globals", t_frame)
def t_dictnext():
    # pydict.pyx equivalent behaviour: dict mutation-during-iteration semantics & ordering
    d = {1: 1, 2: 2}; d[3] = 3; del d[1]; assert list(d) == [2, 3]
check("dict insertion ordering", t_dictnext)
def t_pickle_ast():
    tree = ast.parse("a = 1"); t2 = pickle.loads(pickle.dumps(tree)); assert ast.dump(t2) == ast.dump(tree)
check("pickle of ast nodes (rpyc-style)", t_pickle_ast)

print("impl:", platform.python_implementation(), sys.version.split()[0])
for n, r in results: print("  %-90s %s" % (n, r))
