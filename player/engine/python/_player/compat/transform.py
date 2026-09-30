"""The Python 2 semantics AST pass (`Py2Transformer`) and the `exec`-in-function fixer (prototype items 2 and 3)."""

# Ported from the accepted prototype prototype/py2compat-proto (research/py2compat-proto/module/renpy/py2compat.py).

import ast
import builtins
import collections

def _N(name):
    return ast.Name(id=name, ctx=ast.Load())


def _call(fname, *args):
    return ast.Call(func=_N(fname), args=list(args), keywords=[])


def _is_float_const(n):
    return isinstance(n, ast.Constant) and isinstance(n.value, (float, complex))


def _is_float_expr(n):
    """Operand certainly not an int: float literal, float(...), a `/` result with a float operand, math.* call."""
    if _is_float_const(n):
        return True
    if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == "float":
        return True
    if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and isinstance(n.func.value, ast.Name) \
            and n.func.value.id in ("math", "random") and n.func.attr not in ("floor", "ceil", "factorial", "gcd", "randint", "randrange"):
        return True
    if isinstance(n, ast.BinOp) and isinstance(n.op, (ast.Add, ast.Sub, ast.Mult, ast.Div, ast.Pow)):
        return _is_float_expr(n.left) or _is_float_expr(n.right)
    if isinstance(n, ast.UnaryOp):
        return _is_float_expr(n.operand)
    return False


class _Bound(ast.NodeVisitor):
    """Names bound in one function scope (not descending into nested scopes' bodies)."""

    def __init__(self):
        self.names = set()
        self.declared = set()   # global / nonlocal

    def visit_Name(self, n):
        if isinstance(n.ctx, (ast.Store, ast.Del)):
            self.names.add(n.id)

    def visit_FunctionDef(self, n):
        self.names.add(n.name)
        # decorators and defaults are evaluated here, but never bind

    visit_AsyncFunctionDef = visit_FunctionDef

    def visit_ClassDef(self, n):
        self.names.add(n.name)

    def visit_Lambda(self, n):
        pass

    def visit_ListComp(self, n):
        pass

    visit_SetComp = visit_DictComp = visit_GeneratorExp = visit_ListComp

    def visit_Import(self, n):
        for a in n.names:
            self.names.add((a.asname or a.name).split(".")[0])

    visit_ImportFrom = visit_Import

    def visit_ExceptHandler(self, n):
        if n.name:
            self.names.add(n.name)
        self.generic_visit(n)

    def visit_Global(self, n):
        self.declared.update(n.names)

    visit_Nonlocal = visit_Global


def _scope_names(fn):
    b = _Bound()
    for a in (fn.args.posonlyargs + fn.args.args + fn.args.kwonlyargs):
        b.names.add(a.arg)
    if fn.args.vararg:
        b.names.add(fn.args.vararg.arg)
    if fn.args.kwarg:
        b.names.add(fn.args.kwarg.arg)
    for s in fn.body:
        b.visit(s)
    return b.names, b.declared


def _contains_exec(fn):
    """Does this function (not its nested defs/classes/lambdas) call exec() with a single argument?"""
    found = []

    class V(ast.NodeVisitor):
        def visit_Call(self, n):
            if isinstance(n.func, ast.Name) and n.func.id == "exec" and len(n.args) == 1 and not n.keywords:
                found.append(n)
            self.generic_visit(n)

        def visit_FunctionDef(self, n):
            pass

        visit_AsyncFunctionDef = visit_FunctionDef
        visit_Lambda = visit_FunctionDef
        visit_ClassDef = visit_FunctionDef

    v = V()
    for s in fn.body:
        v.visit(s)
    return bool(found)


class Py2Transformer(ast.NodeTransformer):
    """Rewrites Python 3 semantics back to Python 2 ones.

    `division`: rewrite `/` (off for files that said `rpy python 3`).
    `ordering`: also apply Python 2 mixed-type ordering (only the retry after a TypeError).
    `counts`: Counter of rewrites made (also used by the pre-flight scan).
    """

    def __init__(self, division=True, ordering=False, loose=False):
        self.division = division
        self.ordering = ordering
        self.loose = loose
        self.counts = collections.Counter()
        self.shadowed = [set()]     # names rebound by the snippet itself: leave those calls alone

    # names the snippet defines or assigns anywhere: `map = ...` / `def zip():` must not be rewritten
    def transform(self, tree):
        rebound = set()
        for n in ast.walk(tree):
            if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store):
                rebound.add(n.id)
            elif isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                rebound.add(n.name)
            elif isinstance(n, ast.arg):
                rebound.add(n.arg)
        self.shadowed = [rebound]
        return self.visit(tree)

    def _shadowed(self, name):
        return name in self.shadowed[-1]

    # ---- operators
    def visit_BinOp(self, n):
        n = self.generic_visit(n)
        if self.division and isinstance(n.op, ast.Div) and not (_is_float_expr(n.left) or _is_float_expr(n.right)):
            self.counts["division"] += 1
            return ast.copy_location(_call("_py2c_div", n.left, n.right), n)
        return n

    def visit_AugAssign(self, n):
        n = self.generic_visit(n)
        if self.division and isinstance(n.op, ast.Div) and not _is_float_expr(n.value):
            self.counts["division"] += 1
            tgt = n.target
            load = _copy_ctx(tgt, ast.Load())
            new = ast.Assign(targets=[tgt], value=_call("_py2c_div", load, n.value))
            return ast.copy_location(new, n)
        return n

    def visit_Compare(self, n):
        n = self.generic_visit(n)
        if not self.ordering:
            return n
        if not any(isinstance(o, (ast.Lt, ast.LtE, ast.Gt, ast.GtE)) for o in n.ops):
            return n
        names = {ast.Lt: "<", ast.LtE: "<=", ast.Gt: ">", ast.GtE: ">=", ast.Eq: "==", ast.NotEq: "!=",
                 ast.Is: "is", ast.IsNot: "is not", ast.In: "in", ast.NotIn: "not in"}
        rest = [ast.Tuple(elts=[ast.Constant(names[type(o)]), c], ctx=ast.Load()) for o, c in zip(n.ops, n.comparators)]
        self.counts["ordering-compare"] += 1
        return ast.copy_location(_call("_py2c_compare", n.left, *rest), n)

    # ---- calls
    def visit_Call(self, n):
        n = self.generic_visit(n)
        f = n.func
        if isinstance(f, ast.Name) and not self._shadowed(f.id):
            nm = f.id
            if nm in ("map", "filter", "zip"):
                self.counts["list-" + nm] += 1
                f.id = "_py2c_" + nm
            elif nm == "round":
                self.counts["round"] += 1
                f.id = "_py2c_round"
            elif nm == "sorted" and (any(k.arg == "cmp" for k in n.keywords) or len(n.args) > 1 or self.ordering):
                self.counts["sorted"] += 1
                f.id = "_py2c_sorted"
            elif nm in ("min", "max") and self.ordering:
                self.counts["ordering-" + nm] += 1
                f.id = "_py2c_" + nm
            elif nm == "cmp":
                pass   # builtins.cmp is installed
        elif isinstance(f, ast.Attribute):
            a = f.attr
            if a in ("keys", "values", "items") and not n.args and not n.keywords:
                self.counts["list-" + a] += 1
                return ast.copy_location(_call("_py2c_view", n), n)
            if a in ("iteritems", "iterkeys", "itervalues") and not n.args and not n.keywords:
                self.counts["iter*"] += 1
                return ast.copy_location(_call("_py2c_" + a, f.value), n)
            if a == "has_key" and len(n.args) == 1 and not n.keywords:
                self.counts["has_key"] += 1
                return ast.copy_location(_call("_py2c_has_key", f.value, n.args[0]), n)
            if a == "sort" and (n.args or any(k.arg == "cmp" for k in n.keywords) or self.ordering):
                self.counts["sort"] += 1
                return ast.copy_location(
                    ast.Call(func=_N("_py2c_sort"), args=[f.value] + n.args, keywords=n.keywords), n)
        return n

    # ---- classes: Python 2 special methods and __metaclass__
    def visit_ClassDef(self, n):
        n = self.generic_visit(n)
        names = set()
        for s in n.body:
            if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef)):
                names.add(s.name)
            elif isinstance(s, ast.Assign):
                for t in s.targets:
                    if isinstance(t, ast.Name):
                        names.add(t.id)
        extra = []
        for s in list(n.body):
            if isinstance(s, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "__metaclass__" for t in s.targets):
                n.keywords.append(ast.keyword(arg="metaclass", value=s.value))
                n.body.remove(s)
                self.counts["__metaclass__"] += 1
        alias = (("__nonzero__", "__bool__"), ("next", "__next__"), ("__div__", "__truediv__"),
                 ("__rdiv__", "__rtruediv__"), ("__idiv__", "__itruediv__"))
        for old, new in alias:
            if old in names and new not in names:
                extra.append(ast.Assign(targets=[ast.Name(id=new, ctx=ast.Store())], value=_N(old)))
                self.counts["dunder-alias"] += 1
        if "__eq__" in names and "__hash__" not in names:
            extra.append(ast.Assign(targets=[ast.Name(id="__hash__", ctx=ast.Store())], value=_N("_py2c_hash")))
            self.counts["__eq__-hash"] += 1
        if "__cmp__" in names:
            n.decorator_list.append(_N("_py2c_cmp_class"))
            self.counts["__cmp__"] += 1
        n.body.extend(extra)
        if not n.body:
            n.body.append(ast.Pass())
        return n

    # ---- exec() inside functions: hand the function to ExecFixer, then rewrite the rest as usual
    def _visit_function(self, n):
        if _contains_exec(n):
            n = ExecFixer(n).run()
            self.counts["exec-in-function"] += 1
        return self.generic_visit(n)

    visit_FunctionDef = _visit_function
    visit_AsyncFunctionDef = _visit_function


_BUILTIN_NAMES = frozenset(n for n in dir(builtins) if not n.startswith('_py2c_'))


class ExecFixer(ast.NodeTransformer):
    """Python 2 `exec` inside a function bound the function's locals; Python 3's cannot. The fix, per function that
    calls exec(code): run it as _py2c_exec(code, globals(), locals(), _py2c_ns) (locals visible, new names land in
    the `_py2c_ns` dict); after each exec statement copy the names the function assigns itself back from the dict; and
    look up every other name in `_py2c_ns` first (`_py2c_ns['x'] if 'x' in _py2c_ns else x`)."""

    def __init__(self, fn):
        self.fn = fn
        self.assigned, self.declared = _scope_names(fn)
        self.assigned -= self.declared
        self.inner = []      # names bound by nested lambdas / comprehensions / defs on the way down
        self.depth = 0       # nested-function depth (exec belongs to the nearest function)

    def run(self):
        fn = self.fn
        fn.body = [x for s in fn.body for x in (lambda r: r if isinstance(r, list) else [r])(self.visit(s))]
        init = ast.Assign(targets=[ast.Name(id="_py2c_ns", ctx=ast.Store())], value=ast.Dict(keys=[], values=[]))
        first = fn.body[0] if fn.body else None
        i = 1 if (isinstance(first, ast.Expr) and isinstance(getattr(first, "value", None), ast.Constant)
                  and isinstance(first.value.value, str)) else 0
        fn.body.insert(i, init)
        return fn

    def _is_bound_inner(self, name):
        return any(name in s for s in self.inner)

    def visit_Name(self, n):
        if isinstance(n.ctx, ast.Load):
            name = n.id
            if (name in self.assigned or name in self.declared or name.startswith("_py2c_") or self._is_bound_inner(name)
                    or name in ("True", "False", "None", "locals", "globals", "exec") or name in _BUILTIN_NAMES):
                return n
            test = ast.Compare(left=ast.Constant(name), ops=[ast.In()], comparators=[_N("_py2c_ns")])
            get = ast.Subscript(value=_N("_py2c_ns"), slice=ast.Constant(name), ctx=ast.Load())
            return ast.copy_location(ast.IfExp(test=test, body=get, orelse=ast.Name(id=name, ctx=ast.Load())), n)
        return n

    def visit_Call(self, n):
        n = self.generic_visit(n)
        if self.depth == 0 and isinstance(n.func, ast.Name) and n.func.id == "exec" and len(n.args) == 1 and not n.keywords:
            return ast.copy_location(
                _call("_py2c_exec", n.args[0], _call("globals"), _call("locals"), _N("_py2c_ns")), n)
        return n

    def visit_Expr(self, n):
        n = self.generic_visit(n)
        if _is_py2c_exec(n.value):
            out = [n]
            for name in sorted(self.assigned):
                if name.startswith("_py2c_"):
                    continue
                cond = ast.Compare(left=ast.Constant(name), ops=[ast.In()], comparators=[_N("_py2c_ns")])
                assign = ast.Assign(targets=[ast.Name(id=name, ctx=ast.Store())],
                                    value=ast.Subscript(value=_N("_py2c_ns"), slice=ast.Constant(name), ctx=ast.Load()))
                out.append(ast.If(test=cond, body=[assign], orelse=[]))
            return out
        return n

    def _scoped(self, n, bound):
        self.inner.append(bound)
        try:
            return self.generic_visit(n)
        finally:
            self.inner.pop()

    def visit_Lambda(self, n):
        bound = {a.arg for a in n.args.posonlyargs + n.args.args + n.args.kwonlyargs}
        if n.args.vararg: bound.add(n.args.vararg.arg)
        if n.args.kwarg: bound.add(n.args.kwarg.arg)
        return self._scoped(n, bound)

    def _comp(self, n):
        bound = {t.id for g in n.generators for t in ast.walk(g.target) if isinstance(t, ast.Name)}
        return self._scoped(n, bound)

    visit_ListComp = visit_SetComp = visit_DictComp = visit_GeneratorExp = _comp

    def _nested_def(self, n):
        # defaults / decorators are evaluated in this scope; the body is another scope with its own exec handling
        n.decorator_list = [self.visit(d) for d in n.decorator_list]
        n.args.defaults = [self.visit(d) for d in n.args.defaults]
        n.args.kw_defaults = [self.visit(d) if d is not None else None for d in n.args.kw_defaults]
        b, d = _scope_names(n)
        self.inner.append(b | d)
        self.depth += 1
        try:
            n.body = [x for s in n.body for x in (lambda r: r if isinstance(r, list) else [r])(self.visit(s))]
        finally:
            self.depth -= 1
            self.inner.pop()
        return n

    visit_FunctionDef = visit_AsyncFunctionDef = _nested_def

    def visit_ClassDef(self, n):
        return n     # class bodies are their own namespace: leave alone


def _is_py2c_exec(n):
    return isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == "_py2c_exec"


def _copy_ctx(node, ctx):
    """A Load-context copy of an assignment target (for `x[i] /= 2` -> `x[i] = _py2c_div(x[i], 2)`)."""
    import copy
    node = copy.deepcopy(node)
    for x in ast.walk(node):
        if hasattr(x, "ctx"):
            x.ctx = ast.Load()
    return node
