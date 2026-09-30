"""Which script-language rules apply to the game that is loading.

`renpy7` is set by `_player.boot.path_to_common` from the Ren'Py 7 detection. The engine patches
0750-0799 (Ren'Py 7 script-parser leniencies) read it while they parse `.rpy` files.
"""

renpy7 = False
