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

Numbers under each row (0 to 255 scale; the page header explains each): `mean_abs`, the same for the left and the right half, the share of pixels off by more than 24, and `own on/off` (how much the flag changes one engine's picture). `own on/off` is a plain pixel difference of the two halves of one picture, so read it only as a stock-against-player comparison within a row, not as a size of the effect (the `tilt` halves are also different views, and on Linux the 1896x1056 window has pillarbox bars). Numbers do not tell which engine is "right"; they show where to look.

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

## Why Linux differs for `linear` with anisotropy on

After the sampler fix (`gl_anisotropic` now reaches textures without mips) macOS matches stock in all 27 rows. On Linux (stock 8.5.3 on Mesa 26.2.3 radeonsi, OpenGL 4.6; player on wgpu Vulkan, RADV, RX 9070 XT) the `linear` rows with anisotropy on still differ (mean difference 0.21 to 2.18; `mip` and `nearest` rows and every anisotropy-off half match within 0.26). The cause is a sampler state that GL can program on AMD hardware and Vulkan cannot. The player already uses the closest sampler Vulkan has, so the engine code is unchanged (only its comment).

### Evidence

All numbers are the mean absolute difference over all channels, 0 to 255, on the whole picture. Tools: `harness/testgames/aniso-probe/` (a game with one centered panel per case, so the "on" and "off" shots of one engine have the same geometry; `gl_probe.c`, a raw OpenGL program; `compare_ppm.py`) and `player/crates/gfx/examples/aniso_probe.rs` (raw wgpu). Mesa source is read from `research/mesa-src/mesa-26.2.3` (gitignored; `curl -fL https://archive.mesa3d.org/mesa-26.2.3.tar.xz`).

1. **GL applies anisotropy to a non-mipmapped sampler. Candidate "GL ignores it" is wrong.** In the probe game, on-versus-off on the same geometry, stock radeonsi: `tilt_grating_linear` 8.26, `tilt_checker_linear` 4.64, `mina_grating_linear` 3.97, `mina_checker_linear` 2.76. Raw GL: `GL_LINEAR` with anisotropy 16 against 1: 8.43 (checker) and 13.62 (grating). Mesa's GL layer does not turn it off (`st_atom_sampler.c` copies `max_anisotropy`; `si_create_sampler_state` sets `ANISO_BILINEAR`). `minu` (same scale in X and Y) is 0.000 in every engine, as expected.
2. **The engine and the GL semantics are not the cause: zink equals the player.** Stock under zink (`MESA_LOADER_DRIVER_OVERRIDE=zink`, GL over RADV) and the player give identical pictures: 0.000 on the `mina` and `minu` rows and at most 0.013 on the `tilt` rows. Stock radeonsi against either of them is 1.09 (`mina_checker_linear`), 1.34 (`mina_grating_linear`), 0.22 (`mina_text_linear`), 0.91 (`tilt_checker_linear`), 1.61 (`tilt_grating_linear`), 0.19 (`tilt_text_linear`) with anisotropy on, and 0.003 to 0.036 with it off. The same Ren'Py code, the same textures and the same GL calls give different pictures only when the driver below changes from radeonsi to RADV.
3. **The player's sampler is the radeonsi hardware path with a LOD clamp.** In the raw GL program a sampler with a mip filter and `GL_TEXTURE_MAX_LOD 0` (`GL_LINEAR_MIPMAP_LINEAR`, anisotropy 16) is what Vulkan's `mipmap_filter Linear`, `lod_max_clamp 0` becomes. It matches the raw wgpu picture with that sampler to 0.10 (checker) and 0.14 (grating). That residual is the derivative and rasterization difference of the two APIs (the anisotropy-off halves of the 27 rows match to the same size, at most 0.139). So texel centers, derivatives, sRGB (both sides sample `Rgba8` and `GL_RGBA8` as plain values) and premultiplication are not the cause.
4. **The difference is the mip filter state.** `GL_LINEAR` makes radeonsi write `MIP_FILTER` none (`si_tex_mipfilter` with `PIPE_TEX_MIPFILTER_NONE`) with `max_lod` unclamped. RADV writes `POINT` or `LINEAR` (`radv_tex_mipfilter`); Vulkan has no "none", and wgpu allows anisotropy only with a Linear mipmap filter. With the same level-0 texture the two states give: raw GL `GL_LINEAR` a16 against the Vulkan-style sampler `1.65` (checker) and `2.51` (grating), against `gl_lml_lod0_a16` `1.64` and `2.50`. The picture with "none" is slightly smoother: mean horizontal and vertical gradient 14.67 and 26.88 against 14.86 and 27.75 (checker). The sample count itself is not the whole story: anisotropy 16, 8 and 4 are within 0.3 to 0.5 of each other for both states.
5. **Other descriptor fields do not explain it.** `ANISO_OVERRIDE` (RADV `aniso_single_level`, radeonsi leaves it 0) only acts on one-level images; raw wgpu with a one-level texture, a one-level view of a mipmapped texture and the full texture give identical pictures (0.000). Zink on RADV, which sets `radv_disable_aniso_single_level`, still equals the player. A GL LOD bias (`GL_TEXTURE_LOD_BIAS` -1 to +1) changes neither state's picture.

### What was tried to get closer (raw wgpu picture against raw GL `GL_LINEAR` a16, checker / grating)

| Player sampler or shader | Difference |
|---|---|
| current: Linear mip filter, `lod_max_clamp 0`, anisotropy 16 | 1.65 / 2.51 |
| anisotropy 8 | 1.83 / 2.55 |
| anisotropy 4 | 1.85 / 2.61 |
| anisotropy 2 | 3.13 / 5.18 |
| `lod_max_clamp` 0.5 or 1 (raw GL, `GL_TEXTURE_MAX_LOD`) | 1.89 or 2.33 / 2.80 or 3.43 |
| trilinear anisotropy (the `mip` look) | 2.59 / 3.74 |
| shader taps (N bilinear taps of level 0 along the long axis, `emu_ceil` in `aniso_probe.rs`) | 2.17 / 3.04 |

No state that wgpu can express is closer than the current one, and a shader model of the hardware is worse than the hardware sampler. A better model would need the undocumented behavior of the "none" filter and would be a radeonsi-only fit (NVIDIA and Intel GL drivers differ again), so the player keeps the plain sampler. Decision: **keep `mipmap_filter Linear`, `lod_max_clamp 0`, anisotropy 16 for a non-mipmapped texture with all filters linear** (`gpu.rs`, `Shared::sampler`). Expected Linux residual: 0.2 to 1.5 on screens with strongly anisotropic footprints, only in the anisotropy-on half of `linear` rows. It is a driver difference between radeonsi and RADV, not a flag leak.

### How to reproduce

```sh
# artemis, in a checkout with the gitignored textures (python3 harness/testgames/aniso{,-probe}/build.py)
python3 harness/tools/runlock.py -- gcc ... harness/testgames/aniso-probe/gl_probe.c    # see the header of gl_probe.c
cargo run --release -p gfx --example aniso_probe -- out/probe-raw
python3 harness/testgames/aniso-probe/compare_ppm.py out/probe-gl out/probe-raw
python3 gate.py run --engine stock --game testgames/aniso-probe --tier m1 --only route --route-runs 1 \
    --plan testgames/aniso-probe/probe.plan --out out/probe/stock                            # radeonsi
python3 gate.py run ... --env MESA_LOADER_DRIVER_OVERRIDE=zink --out out/probe/zink          # zink
```
