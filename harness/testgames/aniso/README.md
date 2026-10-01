# Anisotropic filtering test game

A small Ren'Py 8 game for a human A/B check of `gl_anisotropic` between stock Ren'Py 8.5.3 (OpenGL) and the player (Metal on macOS, Vulkan on Linux). Every texture is made by `build.py` with the Python standard library: no third-party art or fonts.

## What is in it

27 still screens, one label each (`case_<geometry>_<texture>_<scaling>`). A screen has two halves and a caption strip:

- **Left half:** `gl_anisotropic True`. **Right half:** `gl_anisotropic False`. All cases use `gl_mipmap True`.
- **Geometry**
  - `tilt`: a 2048 px texture minified to 614 px by a `mesh` Transform, then tilted 72 degrees in perspective by a parent `Transform(perspective=True, matrixtransform=RotateMatrix(72, 0, 0))`. Each half turns about its own centre.
  - `mina`: the 2048 px texture drawn at 130x520 px (minified 16 times in X, 4 times in Y, so the footprint is strongly anisotropic).
  - `minu`: the texture drawn at 300x300 px (minified 6.8 times, same in X and Y).
- **Texture**
  - `checker`: 8 px squares, red lines every 256 px.
  - `grating`: four quadrants. Top left: vertical 1 px lines (period 4). Top right: horizontal 1 px lines (period 4). Bottom left: diagonal lines (period 6). Bottom right: a zone plate (rings that get finer toward the edge).
  - `text`: large (40x56 px glyphs) and small (15x21 px glyphs) lines in a built-in bitmap font.
- **Scaling** (`gl_texture_scaling`): `nearest`, `linear`, `mip` (`linear_mipmap_linear`, the Ren'Py default for mipmapped textures).

Nothing moves. Stock Ren'Py forces anisotropy to 1 when the minification filter is nearest, so `nearest` rows are expected to show no left/right difference in stock. The `linear` rows have no mipmap filtering, so a sampler without mipmaps has nothing to be anisotropic about; they are kept because the task asks for both scalings.

## What to look at in each row

The page puts stock, player and a difference image (|a-b| times 4) side by side. Look at:

- **Shimmer / moire** (grating, checker): in a still frame it shows as moire patterns, aliased bands that do not match the real lines. Compare the left (True) and right (False) halves of each engine, then the same half between engines. The zone plate (bottom right of `grating`) makes the aliasing rings obvious.
- **Blur along the tilt** (`tilt`, `mina`): with anisotropy off, the GPU picks one mip level from the longer axis of the pixel footprint, so the texture goes soft in the direction that is stretched less. With anisotropy on the far part of the `tilt` plane should stay sharper in the vertical direction. Read the text rows: the small text is the first to become unreadable.
- **Mip-level seams** (`tilt`, `mina`, `mip` rows): horizontal bands where sharpness changes abruptly across the plane (one band per mip level). Compare where the bands are, and whether the player's bands are in the same places and as hard as stock's.
- **Nearest rows:** both halves should look the same in both engines. Any left/right difference there is a flag leak.

Numbers under each row (0 to 255 scale; the page header explains each): `mean_abs`, the same for the left and the right half, the share of pixels off by more than 24, and `own on/off` (how much the flag changes one engine's picture). Numbers do not tell which engine is "right"; they show where to look.

## Rerun

```sh
python3 harness/testgames/aniso/build.py          # textures (gitignored) and cases.rpy, aniso.plan
cd harness
# macOS (nix develop gives Python 3.12 for gate.py)
nix develop -c python3 gate.py run --engine stock  --game testgames/aniso --tier m1 --only route --route-runs 1 --plan testgames/aniso/aniso.plan --out out/aniso/mac-stock
nix develop -c python3 gate.py run --engine player --game testgames/aniso --tier m1 --only route --route-runs 1 --plan testgames/aniso/aniso.plan --out out/aniso/mac-player
python3 tools/compare_page.py --out out/aniso/index.html \
  --set "macOS" out/aniso/mac-stock/route-1/shots out/aniso/mac-player/route-1/shots
```

`gate.py` takes the machine lock, clones the game, sets nothing else (the game needs no menu: it starts in `label start` and the plan jumps to each case). The shots are window captures that include the macOS title bar; `compare_page.py` crops the title bar and the caption strip. On Linux run the same two commands on artemis (see `harness/README.md`, Linux), with the 8.5.3 Linux SDK at `research/shared-engine-launcher/sdk/renpy-8.5.3-sdk` and `--player-bin` set. The gate sets the window opaque and uses `grim` there.

`out/` is gitignored: screenshots, the page and the logs stay local.
