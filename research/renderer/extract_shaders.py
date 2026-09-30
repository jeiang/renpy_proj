#!/usr/bin/env python3
"""Extract every shader part Ren'Py 8.5.3 registers itself (renpy/common/_shaders.rpym,
00textshader_ren.py, gl2/live2d.py) via `ast` (no Ren'Py import), assemble complete
GLSL 120 / ESSL 100 programs exactly like renpy/gl2/gl2shadercache.py:source(), and write
them to battery/generated/ (gitignored: derived from Ren'Py text).

usage: extract_shaders.py <renpy-src-dir> [outdir]
"""
import ast, os, re, sys, json

SRC = sys.argv[1]
OUT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(__file__), "battery", "generated")
os.makedirs(OUT, exist_ok=True)

FILES = ["renpy/common/_shaders.rpym", "renpy/common/00textshader_ren.py", "renpy/gl2/live2d.py"]
parts = {}   # name -> dict(kwargs)

def expand(name, s):
    n = name.replace(".", "_")
    s = re.sub(r"\b[uavl]__\w+", lambda m: {"u": "u_", "a": "a_", "v": "v_", "l": "l_"}[m.group(0)[0]] + n + "_" + m.group(0)[3:], s)
    return re.sub(r"\bu_(\w+)__(\w+)", lambda m: "u_%s_OP_%s" % (m.group(1), m.group(2)), s)

for f in FILES:
    text = open(os.path.join(SRC, f), encoding="utf-8-sig").read()
    if f.endswith(".rpym"):
        text = re.sub(r"^init python[^\n]*:", "if True:", text, flags=re.M)
    text = re.sub(r'^"""renpy\n.*?\n"""', "", text, flags=re.M | re.S)
    tree = ast.parse(text)
    for node in ast.walk(tree):
        if isinstance(node, ast.Call):
            fn = node.func
            fname = fn.attr if isinstance(fn, ast.Attribute) else getattr(fn, "id", None)
            if fname not in ("register_shader", "register_textshader"):
                continue
            kw = {}
            for k in node.keywords:
                if k.arg is None: continue
                try: kw[k.arg] = ast.literal_eval(k.value)
                except Exception: pass
            if fname == "register_shader":
                name = ast.literal_eval(node.args[0]); kw2 = kw
            else:
                name = "textshader." + ast.literal_eval(node.args[0])
                kw2 = {k: v for k, v in kw.items() if k == "variables" or k.startswith(("vertex_", "fragment_"))}
            if not any(k.startswith(("vertex_", "fragment_")) for k in kw2) and "variables" not in kw2:
                continue
            parts[name] = dict(kw2, _file=f)

UNI = {"float","vec2","vec3","vec4","int","ivec2","ivec3","ivec4","bool","bvec2","bvec3","bvec4","mat2","mat3","mat4","sampler2D"}
def parse_var(line):
    l = line.strip().rstrip("; ")
    w = re.findall(r"\w+|\[\s*\d+\s*\]", l)
    if w and w[0] == "invariant": w = w[1:]
    if not w or w[0] not in ("uniform", "attribute", "varying"): return None
    return l

class Part:
    def __init__(s, name, kw):
        s.name = name
        s.vf = expand(name, kw.get("vertex_functions", ""))
        s.ff = expand(name, kw.get("fragment_functions", ""))
        s.vp, s.fp = [], []
        vused, fused = set(), set()
        for k, v in kw.items():
            if k.startswith(("vertex_", "fragment_")) and k.split("_")[1].isdigit():
                v = expand(name, v)
                (s.vp if k.startswith("vertex_") else s.fp).append((int(k.split("_")[1]), name, v))
                (vused if k.startswith("vertex_") else fused).update(re.findall(r"\b\w+\b", v))
        s.vv, s.fv = set(), set()
        for l in expand(name, kw.get("variables", "")).split("\n"):
            l = l.partition("//")[0].strip()
            v = parse_var(l) if l else None
            if not v: continue
            nm = re.findall(r"\w+", re.sub(r"\[.*?\]", "", v))[-1]
            if nm in vused: s.vv.add(v)
            if nm in fused: s.fv.add(v)

def source(variables, ps, functions, fragment, gles):
    rv = []
    if gles:
        rv.append("#version 100\n")
        if fragment:
            rv.append("#ifdef GL_FRAGMENT_PRECISION_HIGH\n    precision highp float;\n    precision highp int;\n#else\n    precision mediump float;\n    precision mediump int;\n#endif\n")
    else:
        rv.append("#version 120\n")
    for v in sorted(variables, key=lambda x: re.findall(r"\w+", re.sub(r"\[.*?\]", "", x))[-1]):
        rv.append(v + ";\n")
    rv.extend(functions)
    rv.append("\nvoid main() {\n")
    for _, _, p in sorted(ps): rv.append(p)
    rv.append("}\n")
    return "".join(rv)

def build(names, gles):
    ps = [Part(n, parts[n]) for n in sorted(set(names))]
    vv, fv, vp, fp, vf, ff = set(), set(), [], [], [], []
    for p in ps:
        vv |= p.vv; fv |= p.fv; vp += p.vp; fp += p.fp; vf.append(p.vf); ff.append(p.ff)
    return source(vv, vp, vf, False, gles), source(fv, fp, ff, True, gles)

combos = {}
base = ["renpy.geometry", "renpy.texture"]
for n in parts:
    if n in ("renpy.geometry", "renpy.ftl"): continue
    if n.startswith("textshader."):
        combos["text_" + n[11:]] = base + [n]
    elif n in ("renpy.texture",):
        combos["texture"] = base
    elif n == "live2d.flip_texture":
        combos["live2d_flip_texture"] = base + [n]
    elif n.startswith("renpy.") and n in ("renpy.matrixcolor", "renpy.alpha"):
        combos[n[6:]] = base + [n]
    elif n in ("renpy.solid",):
        combos["solid"] = ["renpy.geometry", n]
    else:
        combos[n.replace(".", "_")] = ["renpy.geometry", n]
combos["ftl"] = ["renpy.ftl"]
combos["texture_matrixcolor_alpha"] = base + ["renpy.matrixcolor", "renpy.alpha"]
manifest = {}
for cname, names in sorted(combos.items()):
    for gles, tag in ((False, "120"), (True, "100")):
        v, f = build(names, gles)
        open(f"{OUT}/{cname}.{tag}.vert", "w").write(v)
        open(f"{OUT}/{cname}.{tag}.frag", "w").write(f)
    manifest[cname] = names
json.dump(manifest, open(f"{OUT}/manifest.json", "w"), indent=1)
print(len(parts), "parts;", len(combos), "combos ->", OUT)
