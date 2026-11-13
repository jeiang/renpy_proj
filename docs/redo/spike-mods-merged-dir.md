# Spike: manager-built merged mod directory

**Verdict: yes, with conditions.** A merged directory built by the manager can replace today's runtime mod layer on both macOS (APFS) and Linux (btrfs). Today's player loaded a merged tree headless on artemis with its file layer switched off (`RENPY_PLAYER_NO_VFS=1`), and every override case gave the same result as the runtime layer. Cloning cost seconds of build time and almost no disk space. The conditions are about where the tree lives, what happens when cloning is impossible, and keeping files the game writes into its own folder.

## Rules the manager must follow
1. **No mods enabled: launch the base game directly.** Build no tree, and delete any old one.
2. **Where the tree lives:** `<data>/merged/<base key>/`, but only when `<data>` is on the same filesystem as the game and a test clone of one file succeeds. Remember that result for each pair of devices.
3. **How files are made:**
   - macOS: `clonefile(2)`. Cloning the whole base `game/` folder in one call and then laying the mod files over it was faster than cloning file by file (0.3 to 1.5 s against 2 to 6 s).
   - Linux: the `FICLONE` ioctl, or `copy_file_range`, which also reflinks on btrfs.
   - If the clone fails with `EXDEV`, `ENOTSUP`/`EOPNOTSUPP` or `EINVAL`, use the fallback in rule 9.
4. **Never use hard links.** They fail across btrfs subvolumes, and a game writing a file in place changes the original.
5. **Never use a pure symlink farm without the file layer.** Writes through a link change the original game (shown below).
6. **Merge rules:**
   - Layers go base game, then optional patch files, then mods in `order.txt` order; the later layer wins.
   - Skip `game/cache` and dotfiles in every layer. My spike script only skipped `game/cache` in the base layer; the real builder must skip it in mods too.
   - On a case-insensitive disk, compare names case-insensitively and keep the winning layer's spelling. Refuse a file and a folder with the same name.
7. **When to rebuild:** at every launch, under the game lock.
   - Walk all layers and compare against a manifest of path to (source, size, mtime). The walk took 0.01 to 1.5 s for these games.
   - Apply only the differences.
   - Do a full rebuild (into `<tree>.new`, then rename) only when the manifest is missing or the method changed.
   - This also catches a base game update.
8. **Keep what the game writes.** Files in the tree that are not in the manifest belong to the game: an update must not delete them, and a full rebuild must carry them over. If one collides with a new mod file, the mod wins and the collision is logged.
9. **Fallback when cloning is impossible:** copy small files and symlink the large ones (`.rpa`, `.rpi`, video, anything 16 MiB or more), which games do not rewrite. This copies 0.05 to 0.08 GB for Ripples and WaifuAcademy, 1.0 GB for WhiteRussian and 3.7 GB for WartribeAcademy. A full copy is the second choice: 4 to 7 s on Linux NVMe, and an estimated 47 s plus 13.5 GB of disk for Ripples on the Mac (extrapolated, not run). **Decision for you:** your section 0 names the runtime layer as the fallback. These numbers suggest the hybrid copy is enough, so the runtime file layer could be dropped entirely.
10. **The launch record carries the base game's key**, not one derived from the merged path. Today the compile cache, logs and overlay are keyed by game path. Saves are not affected: they are stored by the game's save folder name.

## What today's runtime layer does, and whether the merged tree matches
Read from `vfs/src/lib.rs`, `_player/vfs.py`, `library/src/mods.rs`, `mods/install.rs`, patches 0006, 0008, 0009 and 0017, and Ren'Py's `loader.py`.

| Behaviour | Runtime layer | Merged tree |
|---|---|---|
| Loose file override | highest layer that has the path wins | later layer wins (tested) |
| Mod `.rpy`/`.rpyc`/`.rpa` seen by the loader | listings are the union of all layers | real folder (tested) |
| Loose file beats the same name inside an `.rpa` | Ren'Py rule, not the layer's | same (tested: `inarc.txt`) |
| Mod ships its own `.rpa` | archives are searched in reverse name order (`zz_mod` before `archive`); a same-named `.rpa` replaces the base one whole | same (tested) |
| `.rpyc` versus `.rpy`, stale `.rpyc` | player cache first, then the game's `.rpyc` only if its stored digest matches the `.rpy` | same. Toggling a mod in place reused the cache with no stale result (test C) |
| Mod that ships only `.rpyc` over a base `.rpy` | its `.rpyc` is ignored (digest mismatch) | same. A merge *could* drop the lower `.rpy`, but that changes behaviour |
| Deletion markers | only for deletes the game makes at runtime (`.vfs-whiteouts`); mods have none | the game deletes a real file in the tree; rule 8 covers it |
| Shipped `game/cache` hidden | yes | skipped when building |
| Case clashes | folds case on macOS, not on Linux | folds on a case-insensitive disk (checked on macOS with the merge script only); on Linux both spellings stay, with the same result as the runtime layer (test E) |
| Saves and persistent data | `<data>/saves/<save_directory>` (patch 0008) | unaffected |
| Game writes into `game/` | goes to the overlay and survives mod changes | goes into the cloned tree, original untouched (test B2); needs rule 8 |
| `.app` bundle | the bundle is read in place; `renpy.macapp` is set | the tree is outside the bundle, so `macapp` is off (harmless with patch 0017) |

## Headless run on artemis
Today's `urp` (ec0381a), headless, software Vulkan (lavapipe). A synthetic mod overrides `script.rpy` and `images/probe.png`, adds a loose `inarc.txt` that shadows a file inside the base `archive.rpa`, and ships `zz_mod.rpa`.

| Run | Script | Image | inarc | modarc | Original game after the game writes into `game/` |
|---|---|---|---|---|---|
| A runtime layer (`urp mods install`) | mod | 64x48 | loose | loaded | unchanged |
| B merged clone, file layer on | mod | 64x48 | loose | loaded | n/a |
| B2 merged clone, file layer off | mod | 64x48 | loose | loaded | unchanged; the write landed in the tree |
| C one tree: base, then mod, then base | correct each time | | | | 2nd and 3rd runs 0.40 s against 0.69 s (cache reused) |
| D symlink farm, file layer off | mod | 64x48 | loose | loaded | **original base file overwritten** |
| D2 symlink farm, file layer on | mod | 64x48 | loose | loaded | unchanged |

Real game startup (WartribeAcademy, 31.5k files, three warm runs, start to end of init): runtime layer 3.30 to 3.50 s, merged with the file layer off 3.20 to 3.24 s, merged with it on 3.33 to 3.37 s. The merged tree is not slower.

## Build numbers
Mods are synthetic: mod A overrides 200 real file names and adds 50 files plus a 20 MiB `.rpa`; mod B overrides 200 and adds 20. "Toggle" means disabling mod B with an incremental update.

**Linux (artemis, btrfs, NVMe, warm cache)**

| Game | Files / size | Clone build | Toggle | Full rebuild | Extra disk |
|---|---|---|---|---|---|
| Synthetic | 20,072 / 1.6 GB | 0.68 s | 0.13 s | 1.14 s | 0 (statvfs +0.3 MiB) |
| WartribeAcademy | 31,585 / 4.9 GB | 1.09 to 1.11 s | 0.22 s | 2.0 s | about 0 |
| Ripples | 1,594 / 12.6 GB | 0.07 to 0.18 s | 0.03 to 0.05 s | 0.11 s | `btrfs fi du`: 10.89 GiB shared, 0 B exclusive |

- **Different device** (target on tmpfs): the clone fails with `EXDEV` and the copy fallback ran: synthetic 1.2 s, WartribeAcademy 3.0 s using 5.1 GB.
- **Same filesystem that cannot clone** (tmpfs to tmpfs): `ENOTSUP`, fallback worked.
- **Hard links:** fail with `EXDEV` from `~/Games` (a separate subvolume), while reflinks across it work.
- **Python copy on Linux reflinks anyway:** `shutil.copy2` uses `copy_file_range`, and its tree showed 0 B exclusive. A true copy (`cp -r --reflink=never` plus `sync`) took 4.3 s for WartribeAcademy (4.9 GB) and 7.1 s for Ripples (12.6 GB), all of it exclusive disk.
- **Symlink farm:** 0.06 to 0.6 s.

**macOS (APFS boot disk, 93% full; load average 28 to 48 from other jobs, so boot-disk times are noisy)**

| Game | Files / size | Clone per file | Clone `game/` in one call | Toggle | Full rebuild | Copy |
|---|---|---|---|---|---|---|
| Synthetic | 20,072 / 1.6 GB | 1.8 to 6.5 s | 0.86 to 0.89 s | 0.17 to 1.55 s | 3.3 to 6.0 s (one-call: 1.4 to 1.7 s) | 12 to 23 s |
| WhiteRussian | 9,885 / 1.1 GB | 3.0 to 5.8 s | 0.32 to 1.5 s | 0.16 to 1.2 s | 1.8 to 9.4 s (one-call: 2.0 s) | 17 s |
| Ripples | 691 / 13.5 GB | 0.15 s | 2.6 s | 0.05 to 0.08 s | 0.26 to 0.42 s | not run (disk space) |

- **Disk use,** measured on an otherwise empty APFS disk image:
  - Synthetic: clone 9.2 MiB, one-call clone 13.7 MiB, copy 1571 MiB.
  - WhiteRussian: clone 5.6 MiB, one-call clone about 0, copy 1086 MiB.
- **Different volume** (target in a mounted disk image): the clone fails with `EXDEV` and the copy fallback built WhiteRussian in 7.8 s using 1093 MiB. A 1.83 GB file crossed volumes in 6.3 s, which is where the Ripples estimate comes from.
- **Symlink farm:** WhiteRussian 4.4 s, Ripples 0.21 s. The loader follows symlinks on Linux: `scandir` and `open` follow them and linked `.rpa` files loaded (test D). Not run on the Mac. A farm built outside a `.app` only reads the bundle, so it does not break it. A game writing through a link would modify the bundle (and break its signature), as in test D.

## Where the merged tree is worse
- **Cloning impossible** (game on another volume, ext4, exFAT or HFS+): a full copy costs the whole game size and up to about a minute for 13 GB games. The hybrid fallback cuts this, but the symlinked large files are not protected from in-place writes.
- **Games that write into `game/` and expect it to stay:** a naive rebuild loses those files (rule 8).
- **Editing mod files live:** the runtime layer sees changes at once; the tree needs a rebuild (cheap, but only at launch). Ren'Py's developer reload watches the copies, not the sources.
- **First build on a loaded Mac** with 10k to 30k small files: 2 to 6 s cloning file by file.
- **Tracebacks** show merged paths instead of mod paths.
- **`.rpyc` cache reuse:** no worse, *if* the key stays the base game's (rule 10). A key taken from a changing merged path would recompile every time.

## Method
- **Scripts:** `/tmp/merge-spike/` on the Mac, and `~/Projects/renpy_proj-remote/redo/ModsMergeSpike/` on artemis, which persists.
  - `merge.py build|update|verify DEST --mode clone|copy|symlink|hardlink|auto [--dirclone] LAYER...` builds the tree. The clone uses `clonefile` through ctypes on macOS and the `FICLONE` ioctl on Linux; `auto` falls back to copying on error. It writes a manifest and folds case on case-insensitive disks.
  - `gen.py big|over` makes the synthetic game and mods; mods for real games reuse real file names with random content.
  - `bench.py` and `host_bench.sh` run build, verify, toggle, re-enable and full rebuild, and record the free-space change.
  - `probe_game.py` writes the probe game; `run_probe.sh` and `matrix.sh` run tests A to E.
  - `boot_bench.sh` runs the startup comparison; `sizes.py` computes the hybrid fallback sizes.
- **Headless runs:** inside `nix develop .#player` from `redo/src`, with `env.sh` (`VK_ICD_FILENAMES` set to the lavapipe ICD from the mesa store path, `VK_LOADER_LAYERS_DISABLE=~implicit~`, `WGPU_BACKEND=vulkan`) and `PLAYER_HEADLESS=1 PLAYER_NO_STOCK_IMPORT=1 --renderer wgpu`.
- **Disk use:** `btrfs filesystem du -s` on Linux; a 6 GB APFS sparse image (`hdiutil create -fs APFS -type SPARSE`) on the Mac.
- **Tools:** `nix shell nixpkgs#python3` on artemis (there is no python3 on its PATH), and `hdiutil`. Nothing was installed.
- **Clean-up:** large data on both hosts is deleted and the disk image is detached. No game material went into the repository, and no tracked file was changed.

## Not tested
- XFS: no XFS volume on artemis, and making one needs root.
- The player on macOS: no Mac build, so the merged tree was not loaded by the player there. macOS behaviour is inferred from the same Python loader code and from the merge script's case folding.
- Ren'Py 7 games under a merged tree.
- A real community mod.
- Python imports from a mod (`.py` modules), and the patch-files layer.
- External or exFAT drives.
- The hybrid copy-plus-symlink fallback was sized (`sizes.py`) but not built or run.