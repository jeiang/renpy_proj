r"""The two one-line patches from research/boundary (section 8), applied in place to C:\spike\renpy-src (the tree is a git-archive copy).
accelerator.pyx: drop the sdl2 / PySurface_AsSurface imports and nogil_copy.  gl2model.pyx: drop the GLTexture cimport and
turn its isinstance check into a duck-type check (objects that have .load())."""
import re
p = "renpy/display/accelerator.pyx"; s = open(p, encoding="utf-8").read()
s = s.replace("from sdl2 cimport *\n\nfrom renpy.pygame.surface cimport PySurface_AsSurface\n", "")
s = re.sub(r"\ndef nogil_copy\(src, dest\):.*?SDL_UpperBlit\(src_surf, NULL, dest_surf, NULL\)\n", "\n", s, flags=re.S)
open(p, "w", encoding="utf-8").write(s)
p = "renpy/gl2/gl2model.pyx"; s = open(p, encoding="utf-8").read()
s = s.replace("from renpy.gl2.gl2texture cimport GLTexture\n", "")
s = s.replace("if isinstance(i, GLTexture):", "if hasattr(i, 'load'):")
open(p, "w", encoding="utf-8").write(s)
