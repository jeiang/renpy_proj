r"""Run in C:\spike\renpy-src with RENPY_STATIC=1 RENPY_REGENERATE_CYTHON=1 RENPY_CYTHON=<cython.exe>.
Generates tmp/gen3-static/*.c for the boundary research's "rebuild as-is" set through Ren'Py's own setuplib
(same Cython flags, same PyInit_ renaming + dotted module name fixup as renpy-build's RENPY_STATIC path).
Extra args: module names to add (accelerator/gl2model are tried unpatched)."""
import os, sys
BASE = os.getcwd(); sys.path.insert(0, os.path.join(BASE, "scripts"))
import setuplib, generate_styles
from setuplib import cython, generate_all_cython
setuplib.init()
generate_styles.generate()
MODS = """renpy.astsupport renpy.cslots renpy.lexersupport renpy.pydict renpy.style renpy.encryption renpy.tfd
renpy.audio.filter renpy.styledata.styleclass renpy.styledata.stylesets
renpy.display.matrix renpy.display.render renpy.display.accelerator renpy.display.quaternion
renpy.gl2.gl2mesh renpy.gl2.gl2mesh2 renpy.gl2.gl2mesh3 renpy.gl2.gl2polygon renpy.gl2.gl2model
renpy.text.textsupport renpy.text.texwrap""".split()
for m in MODS:
    if m == "renpy.tfd": continue   # tfd has C sources; handled below
    cython(m)
cython("renpy.tfd", ["src/tinyfiledialogs/tinyfiledialogs.c"])
for p in generate_styles.prefixes:
    cython("renpy.styledata.style_{}functions".format(p), pyx=setuplib.gen + "/style_{}functions.pyx".format(p))
generate_all_cython()
print("GENERATED", len(setuplib.necessary_gen))
open("tmp/modules.txt", "w").write("\n".join(e.name for e in setuplib.extensions))
