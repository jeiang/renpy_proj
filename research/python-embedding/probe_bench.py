"""Micro-benchmark of Ren'Py-shaped Python workloads (not the engine). Usage: <python> probe_bench.py <renpy-checkout>
Reuses probe_semantics stubs by importing it (prints its results first)."""
import sys, time, pickle, io, ast, platform
sys.argv = [sys.argv[0], sys.argv[1]]
import probe_semantics as P
rv = P.rv; ns = P.ns
def t(name, fn, n=1):
    s = time.perf_counter(); fn(); e = time.perf_counter() - s
    print("BENCH %-46s %8.1f ms" % (name, e * 1000))
def b_rlist():
    l = rv.RevertableList()
    for i in range(200000): l.append(i)
def b_rdict():
    d = rv.RevertableDict()
    for i in range(200000): d[i] = i
def b_plain():
    l = []
    for i in range(200000): l.append(i)
    d = {}
    for i in range(200000): d[i] = i
def b_pickle():
    data = {i: [i, str(i), (i, i+1)] for i in range(50000)}
    b = pickle.dumps(data, 2); pickle.loads(b)
SRC = "\n".join("x%d = [a for a in range(%d)]\nif x%d: y%d = {'k': x%d}" % (i,i%7,i,i,i) for i in range(1500))
def b_compile():
    tree = compile(SRC, "<t>", "exec", ast.PyCF_ONLY_AST, True)
    tree = ns["wrap_node"].visit(tree); ast.fix_missing_locations(tree)
    compile(tree, "<t>", "exec", 0, True)
def b_loop():
    s = 0
    for i in range(3000000): s += i * 2 % 7
print("impl:", platform.python_implementation(), sys.version.split()[0])
for n, f in [("RevertableList.append x200k", b_rlist), ("RevertableDict.__setitem__ x200k", b_rdict),
             ("plain list/dict x200k each", b_plain), ("pickle proto2 dump+load 50k records", b_pickle),
             ("WrapNode+compile 3000 stmts", b_compile), ("int loop 3M", b_loop)]:
    t(n, f)
