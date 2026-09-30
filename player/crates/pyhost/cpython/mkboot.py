"""Marshal the 3 stdlib modules Python needs *during* Py_InitializeFromConfig (encodings) so they can be frozen
into the executable. Everything else comes from the in-memory zip. Usage: mkboot.py <libdir> <outdir> (3.12)"""
import sys, marshal, pathlib
lib, out = map(pathlib.Path, sys.argv[1:3]); out.mkdir(parents=True, exist_ok=True)
for name, rel in [("encodings", "encodings/__init__.py"), ("encodings_aliases", "encodings/aliases.py"), ("encodings_utf_8", "encodings/utf_8.py")]:
    (out / (name + ".bin")).write_bytes(marshal.dumps(compile((lib / rel).read_bytes(), "<frozen " + rel + ">", "exec", dont_inherit=True)))
