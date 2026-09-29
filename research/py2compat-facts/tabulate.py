#!/usr/bin/env python3
"""Print census.json as markdown tables (grouped constructs)."""
import json, sys, collections, re
d = json.load(open(sys.argv[1] if len(sys.argv) > 1 else "census.json"))
games = list(d)
short = {g: re.sub(r"[-_]?(v?\d.*)$", "", g.split(".")[0])[:14] for g in games}
short["Bumpkin_Boy's_Bizarre_Adventures-0.14-pc"] = "Bumpkin"
def total(g, pred):
    c = d[g]["counts"]; return sum(v for k, v in c.items() if pred(k))
def row(label, pred):
    vals = [total(g, pred) for g in games]
    return "| %s | %s |" % (label, " | ".join(str(v) if v else "." for v in vals))
print("| construct | " + " | ".join(short[g] for g in games) + " |")
print("|---|" + "---:|" * len(games))
rows = [
 ("snippets (unique)", None),
 ("`/` possibly-int (any)", lambda k: k == "div.possibly-int"),
 ("  of which in expr (screen/ATL/args)", lambda k: k == "div.possibly-int@expr"),
 ("`/` int literal both sides", lambda k: k == "div.int-certain"),
 ("`/` float-safe", lambda k: k == "div.float-safe"),
 ("dict `.keys()/.values()/.items()` stored", lambda k: re.match(r"view\.stored:(keys|values|items)", k)),
 ("map/filter/zip stored", lambda k: re.match(r"view\.stored:(map|filter|zip)", k)),
 ("view/iterator indexed (direct or via name)", lambda k: k.startswith("view.indexed") or k.startswith("view.flow-indexed")),
 ("view/iterator + list method (direct or via name)", lambda k: k.startswith("view.list-method") or k.startswith("view.flow-list")),
 ("view/iterator concatenated with `+`", lambda k: k.startswith("view.concat")),
 ("len() of map/filter/zip", lambda k: k.startswith("view.len")),
 ("random.choice(view)", lambda k: k.startswith("view.random")),
 ("view/iterator returned from function", lambda k: k.startswith("view.returned")),
 ("`round()` 1-arg (py2 float, py3 int)", lambda k: k == "round.1arg"),
 ("`round()` 2-arg", lambda k: k == "round.2arg"),
 ("`sorted/.sort` without key (mixed-type risk)", lambda k: k.startswith("sort.no-key")),
 ("`min/max` without key", lambda k: k == "minmax.no-key"),
 ("`cmp=` / positional cmp / `cmp()`", lambda k: k.startswith("sort.cmp") or k == "cmp.call"),
 ("`.encode(` / `.decode(`", lambda k: k in ("str.encode", "str.decode")),
 ("`str(x.encode())` (silent b'..')", lambda k: k.startswith("str-of-encode")),
 ("`bytes()/bytearray()`", lambda k: k == "bytes-call"),
 ("`open()` text mode", lambda k: k == "open.text-default-encoding"),
 ("`open()` binary", lambda k: k == "open.binary"),
 ("isinstance vs str/unicode/basestring/bytes", lambda k: k == "isinstance-string-type"),
 ("`unicode/basestring/xrange/raw_input` (store provides)", lambda k: k.startswith("py2name.provided")),
 ("`long/unichr/reduce/cmp/file/execfile/reload/..` (NameError)", lambda k: k.startswith("py2name.NameError")),
 ("`exec` statement (parse fail)", lambda k: k.startswith("exec-statement")),
 ("`exec(...)` call inside function", lambda k: k == "exec-call-in-function"),
 ("`.has_key(`", lambda k: k == "has_key"),
 ("`iteritems/iterkeys/itervalues`", lambda k: k.startswith("iter-methods")),
 ("`__metaclass__ =`", lambda k: k.startswith("__metaclass__")),
 ("`__nonzero__/__cmp__/__unicode__/__getslice__` (silent)", lambda k: re.match(r"dunder\.\w+\(silent\)", k)),
 ("`__div__/next` (loud)", lambda k: re.match(r"dunder\.\w+\(loud\)", k)),
 ("class with `__eq__` no `__hash__`", lambda k: k.startswith("eq-without")),
 ("old-style class (no base)", lambda k: k == "old-style-class"),
]
for label, pred in rows:
    if pred is None:
        print("| %s | %s |" % (label, " | ".join(str(d[g]["unique_snippets"]) for g in games)))
    else:
        print(row(label, pred))
print("| `rpy python 3` files | " + " | ".join(str(d[g]["py3file_count"]) or "." for g in games) + " |")
print("| loose .py files (SyntaxError) | " + " | ".join("%d (%d)" % (d[g]["loose_py_files"], d[g]["loose_py_syntax_errors"]) for g in games) + " |")
print()
print("compile stages / failures")
for g in games:
    print(g, d[g]["compile_stage"], d[g]["fail_reasons"])
