| Subsystem | Python | Cython | C/C++ | Ren'Py script | other | Total |
|---|---:|---:|---:|---:|---:|---:|
| Dev tooling inside renpy/ (lint, update, test runner, translation, dump) | 9265 | 0 | 0 | 0 | 0 | 9265 |
| Live2D + 3D model import (out of scope) | 966 | 1504 | 639 | 0 | 0 | 3109 |
| pygame layer (renpy.pygame = ex-pygame_sdl2, SDL2 binding) | 722 | 8000 | 11087 | 0 | 925 | 20734 |
| GL2 renderer (gl2/, uguu/, shaders) | 1030 | 6689 | 0 | 0 | 0 | 7719 |
| Render tree core (render.pyx, matrix, swdraw, accelerator) | 767 | 2966 | 0 | 0 | 0 | 3733 |
| Text: layout, fonts, shaping, bidi | 3508 | 3129 | 745 | 0 | 0 | 7382 |
| Audio + video decode (renpysound, ffmedia, filters) | 2637 | 1174 | 2010 | 0 | 0 | 5821 |
| Style system (style.pyx, styledata, generated) | 160 | 648 | 0 | 0 | 0 | 808 |
| Save/load, persistent, save-token, crypto | 2057 | 41 | 2933 | 0 | 48 | 5079 |
| Rollback + revertable store + python exec | 3821 | 399 | 0 | 0 | 0 | 4220 |
| Script front-end: lexer/parser (source games only) | 4053 | 35 | 0 | 0 | 0 | 4088 |
| AST + script loading + interpreter (execution, game, main) | 13046 | 126 | 0 | 0 | 0 | 13172 |
| Asset loader (RPA/dir/apk, image + file access) | 2501 | 0 | 0 | 0 | 0 | 2501 |
| Platform glue (tfd dialogs, config, prefs, color) | 1379 | 221 | 6842 | 0 | 0 | 8442 |
| Display core: interface loop, displayables, layout, UI, screens | 22309 | 0 | 0 | 0 | 0 | 22309 |
| Standard library in Ren'Py script (renpy/common) | 1648 | 0 | 0 | 18127 | 0 | 20633 |
| Other / unassigned | 258 | 0 | 0 | 0 | 0 | 258 |
| **Runtime engine** (excl. dev tooling, Live2D/3D, renpy/common) | **57990** | **23428** | **23617** | **0** | **973** | **106008** |
| **Total** | **70127** | **24932** | **24256** | **18127** | **973** | **138415** |
