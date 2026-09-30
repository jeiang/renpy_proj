"""Generate host/src/inittab_gen.rs from a module list. Usage: mkinittab.py out.rs name[:initsym] ...
Default init symbol: PyInit_<name with . -> _>  (the naming setuplib.py's RENPY_STATIC fixup produces)."""
import sys
out, *mods = sys.argv[1:]
rows = []
for m in mods:
    name, _, sym = m.partition(":")
    rows.append((name, sym or "PyInit_" + name.replace(".", "_")))
with open(out, "w") as f:
    f.write("use pyo3_ffi::PyObject;\nextern \"C\" {\n")
    for _, s in rows: f.write(f"    fn {s}() -> *mut PyObject;\n")
    f.write("}\npub static INITTAB: &[(&std::ffi::CStr, unsafe extern \"C\" fn() -> *mut PyObject)] = &[\n")
    for n, s in rows: f.write(f"    (c\"{n}\", {s}),\n")
    f.write("];\n")
print(out, len(rows), "modules")
