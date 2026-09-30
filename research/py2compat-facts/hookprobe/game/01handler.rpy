# Experiment (question A2): exception handler that identifies the failing node, swaps its bytecode,
# rolls back to the previous checkpoint and lets the game re-execute.
init -998 python:
    import os
    _HD = {"n": 0}
    _FIXES = {'hp_first = {1: "a"}.keys()[0]': 'hp_first = list({1: "a"}.keys())[0]'}
    def _py2_handler(te):
        _HD["n"] += 1
        ctx = renpy.game.context()
        node = renpy.game.script.lookup(ctx.current)
        code = getattr(node, "code", None)
        _hlog("handler call %d: exc=%s | %s" % (_HD["n"], te.exc_type_str, te._str))
        _hlog("  node=%s name=%r file=%s line=%s init_phase=%s next_node=%s" % (
            type(node).__name__, node.name, node.filename, node.linenumber, ctx.init_phase, ctx.next_node is not None))
        _hlog("  deepest frame: %r" % (te.stack[-1],))
        _hlog("  PyCode: py=%r mode=%r file=%r line=%r hash=%r source=%r" % (
            code.py, code.mode, code.filename, code.linenumber, code.hashcode, code.source) if code else "  no PyCode")
        _hlog("  rollback: enabled=%r can_rollback=%r log_len=%d" % (config.rollback_enabled, renpy.can_rollback(), len(renpy.game.log.log)))
        if code is not None and _HD["n"] == 1:
            fixed = code.source
            for a, b in _FIXES.items():
                fixed = fixed.replace(a, b)
            if fixed != code.source:
                code.source = fixed
                code.bytecode = renpy.python.py_compile(fixed, code.mode, filename=code.filename, lineno=code.linenumber, column=code.col_offset)
                _hlog("  swapped bytecode in place (node name unchanged: %r); rolling back" % (node.name,))
                renpy.rollback(force=True)      # raises RollbackException (BaseException): must not return
        return False
    config.exception_handler = _py2_handler
    def _lbl(name, abnormal): _hlog("label %s" % name)
    config.label_callback = _lbl
    def _say(event, interact=True, **kw):
        if event == "begin": _hlog("say begin")
    config.all_character_callbacks.append(_say)
    config.rollback_enabled = True

init 999 python:
    config.default_afm_enable = True
    config.default_afm_time = 1
