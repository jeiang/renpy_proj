# Node census (investigation only). Env NODES_OUT: JSON file. Runs at init 999, writes, then quits.
init 999 python:
    import os as _zn_os, json as _zn_json, collections as _zn_c
    _zn_out = _zn_os.environ.get("NODES_OUT")
    if _zn_out:
        _zn_ast = renpy.ast
        _zn_all = renpy.game.script.all_stmts
        _zn_under_init = set()
        for _n in _zn_all:
            if isinstance(_n, _zn_ast.Init):
                def _add(x):
                    _zn_under_init.add(id(x))
                _n.get_children(_add)
        _zn_files = _zn_c.OrderedDict()
        def _F(fn):
            if fn not in _zn_files:
                _zn_files[fn] = dict(nodes=0, init_nodes=0, label_pure_nodes=0, label_init_nodes=0, label_pure=0, label_impure=0,
                                   top_other=0, translate=0, labels=0, say=0, py=0)
            return _zn_files[fn]
        for _n in _zn_all:
            f = _F(_n.filename)
            f["nodes"] += 1
            if isinstance(_n, (_zn_ast.Translate, _zn_ast.TranslateBlock, _zn_ast.TranslateEarlyBlock, _zn_ast.TranslatePython, _zn_ast.TranslateSay)):
                f["translate"] += 1
            if isinstance(_n, _zn_ast.Say):
                f["say"] += 1
            if isinstance(_n, (_zn_ast.Python, _zn_ast.EarlyPython)):
                f["py"] += 1
        _zn_in_label = set()
        for _n in _zn_all:
            if isinstance(_n, _zn_ast.Label) and id(_n) not in _zn_in_label:
                f = _F(_n.filename)
                f["labels"] += 1
                kids = []
                _n.get_children(kids.append)
                has_init = any(id(k) in _zn_under_init for k in kids)
                for k in kids:
                    _zn_in_label.add(id(k))
                if has_init:
                    f["label_impure"] += 1
                    f["label_init_nodes"] += len(kids)
                else:
                    f["label_pure"] += 1
                    f["label_pure_nodes"] += len(kids)
        for _n in _zn_all:
            if id(_n) not in _zn_in_label:
                f = _F(_n.filename)
                if id(_n) in _zn_under_init:
                    f["init_nodes"] += 1
                else:
                    f["top_other"] += 1
        _zn_tot = _zn_c.Counter()
        for fn, d in _zn_files.items():
            _zn_tot.update(d)
        with open(_zn_out, "w") as _zf:
            _zn_json.dump(dict(total=_zn_tot, files=_zn_files, initcode=len(renpy.game.script.initcode)), _zf)
        import time as _zn_t
        _zn_t0 = _zn_t.time()
        def _zn_q():
            renpy.quit(save=False)
        config.periodic_callbacks.append(_zn_q)
