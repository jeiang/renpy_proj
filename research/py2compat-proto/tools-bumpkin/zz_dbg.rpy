# Debug aid (throwaway): dump the AST nodes whose line range is invalid when slast.compile_expr fails.
python early hide:
    import renpy.sl2.slast as _s, ast, os
    _orig = _s.compile_expr
    def _dbg(loc, node):
        try:
            return _orig(loc, node)
        except ValueError as e:
            bad = []
            for n in ast.walk(node):
                if hasattr(n, "lineno") and (getattr(n, "end_lineno", None) is None or n.lineno > n.end_lineno):
                    bad.append((type(n).__name__, n.lineno, getattr(n, "end_lineno", "MISSING"), ast.dump(n)[:300]))
            with open(os.path.join(renpy.config.basedir, "dbg_linerange.txt"), "a") as f:
                f.write("loc=%r err=%s\nroot=%s\n" % (loc, e, type(node).__name__))
                for b in bad: f.write("  %r\n" % (b,))
                f.write("  whole: %s\n" % ast.dump(node)[:1500])
            raise
    _s.compile_expr = _dbg
