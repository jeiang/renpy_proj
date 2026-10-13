init 999 python:
    import os as _zp_os, collections as _zp_c, json as _zp_json
    _zp_out = _zp_os.environ.get("PURE_OUT")
    if _zp_out:
        ast = renpy.ast
        reasons = _zp_c.Counter(); nodes_by = _zp_c.Counter(); small = 0; total = 0; ok = 0; oknodes = 0
        skip = (ast.Init, ast.EarlyPython, ast.UserStatement, ast.Testcase)
        for L in renpy.game.script.all_stmts:
            if type(L) is not ast.Label or not L.block or not isinstance(L.name, str):
                continue
            total += 1
            nodes = []
            for i in L.block:
                i.get_children(nodes.append)
            why = None
            for n in nodes:
                if isinstance(n, skip):
                    why = "type " + type(n).__name__; break
                if n.get_init() is not None:
                    why = "get_init " + type(n).__name__; break
                if getattr(n, "store", "store") != "store":
                    why = "store " + type(n).__name__; break
            if why:
                reasons[why] += 1; nodes_by[why] += len(nodes)
            elif len(nodes) < 8:
                small += 1
            else:
                ok += 1; oknodes += len(nodes)
        with open(_zp_out, "w") as f:
            _zp_json.dump(dict(total=total, ok=ok, oknodes=oknodes, small=small, reasons=reasons, nodes_by=nodes_by), f)
        def _zp_q():
            renpy.quit(save=False)
        config.periodic_callbacks.append(_zp_q)
