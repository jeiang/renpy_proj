# Experiment (ticket #19, question A): where can a Python 2 compat pass be inserted?
# A `python early` block in the first-sorted game file, before any other game file is compiled.
python early:
    import ast, os, collections
    _H = {"log": os.path.join(config.basedir, "hook_log.txt"), "seen": collections.Counter()}
    def _hlog(s):
        with open(_H["log"], "a") as f: f.write(s + "\n")

    # (1) AST choke point: replace renpy.python.wrap_node with a subclass that also rewrites `/` -> __py2div__(a, b)
    _orig_wrap = renpy.python.wrap_node
    class _Py2Node(type(_orig_wrap)):
        def visit_BinOp(self, node):
            node = self.generic_visit(node)
            if isinstance(node.op, ast.Div):
                _H["seen"]["div"] += 1
                _hlog("div rewritten at line %r" % (getattr(node, "lineno", None),))
                return ast.Call(func=ast.Name(id="__py2div__", ctx=ast.Load()), args=[node.left, node.right], keywords=[])
            return node
    _new = _Py2Node()
    class _Tap:
        """Wraps the transformer to log which (filename, mode) reached it via py_compile."""
        def visit(self, tree):
            f = sys._getframe(1)
            _hlog("wrap_node.visit from py_compile file=%r mode=%r ast_node=%r" % (
                f.f_locals.get("filename"), f.f_locals.get("mode"), f.f_locals.get("ast_node")))
            return _new.visit(tree)
        def __getattr__(self, n): return getattr(_new, n)
    import sys
    renpy.python.wrap_node = _Tap()

    # (2) source-level choke point for legacy syntax: renpy.compat.fixes.fix_tokens is called (attribute lookup at
    #     call time) only after the first parse raised SyntaxError.
    _orig_fix_tokens = renpy.compat.fixes.fix_tokens
    def _fix_tokens(source):
        _hlog("fix_tokens called on %d bytes" % len(source))
        source = source.replace("print 'py2print'", "pass")
        return _orig_fix_tokens(source)
    renpy.compat.fixes.fix_tokens = _fix_tokens

    # (3) cache versioning levers
    _hlog("PYC_MAGIC tail=%r ccache.version=%r scache.version=%r" % (
        renpy.script.PYC_MAGIC[-12:], renpy.pyanalysis.ccache.version, renpy.sl2.slast.scache.version))
    _hlog("script.bytecode_oldcache entries at hook time: %d" % len(renpy.game.script.bytecode_oldcache))
    _hlog("early_script_version=%r (config.script_version is not defined yet at python early: %s)" % (
        config.early_script_version, "script_version" not in vars(renpy.config)))

init -999 python:
    def __py2div__(a, b):
        if isinstance(a, int) and isinstance(b, int) and not isinstance(a, bool):
            return a // b
        return a / b
    _hlog("init -999 ran; config.script_version=%r" % (config.script_version,))
    for _k in ("search_prefixes", "quadratic_volumes", "allow_unfull_vpgrids", "atl_function_always_blocks", "narrator_menu", "box_skip", "gl2"):
        _hlog("config.%s=%r" % (_k, getattr(config, _k)))
    _hlog("store has: " + ", ".join("%s=%s" % (n, n in globals()) for n in ("basestring","unicode","xrange","raw_input","long","reduce","cmp","unichr","PY2","file","execfile","__builtin__","round","range","sorted","dict","open")))
