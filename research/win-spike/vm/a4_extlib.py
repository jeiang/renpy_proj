r"""Archive PBS's per-extension COFF objects (PYTHON.json build_info.extensions[*].objs) into pbsext.lib and print the
module list for the inittab. Run from C:\spike\pbs with the MSVC env. Usage: a4_extlib.py <outdir> mod..."""
import json, subprocess, sys, os
out, *want = sys.argv[1:]
j = json.load(open("PYTHON.json"))["build_info"]["extensions"]
objs = []; mods = []; skipped = []
for m in want:
    v = j[m][0]
    if v["in_core"]: skipped.append(m + "(in core)"); continue
    dyn = [l["name"] for l in v["links"] if "path_dynamic" in l]
    print(m, "objs", len(v["objs"]), "links", [l["name"] for l in v["links"]], "DYNAMIC:" if dyn else "", dyn)
    # pyexpat and _elementtree each carry their own expat objects (separate .pyd in PBS): keep one copy
    new = [o for o in v["objs"] if os.path.basename(o) not in {os.path.basename(x) for x in objs}]
    objs += new; mods.append(m)
open(os.path.join(out, "ext.rsp"), "w").write("\n".join(objs))
subprocess.check_call(["lib", "/nologo", "/OUT:" + os.path.join(out, "pbsext.lib"), "@" + os.path.join(out, "ext.rsp")])
open(os.path.join(out, "ext_mods.txt"), "w").write(" ".join(mods))
print("MODS", " ".join(mods))
