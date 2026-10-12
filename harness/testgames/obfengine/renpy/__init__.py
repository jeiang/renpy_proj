# Engine copy in the game folder of SynthObfEngine (see ../build.py). The file is modified on purpose: importing it fails.
# Stock Ren'Py runs its own engine and urp runs its bundled one, so neither may import this file. Both runs must pass,
# and the player report must say that the game ships an engine copy that urp does not load.
version_tuple = (8, 5, 3)

raise ImportError("the game folder's renpy/ copy was imported; neither stock Ren'Py nor urp may load it")
