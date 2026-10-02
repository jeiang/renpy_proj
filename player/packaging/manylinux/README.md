# manylinux_2_28 build of the Linux player

This directory builds the Linux x86_64 player inside the `manylinux_2_28` container image
(AlmaLinux 8, glibc 2.28, gcc 14). The binary runs on any glibc 2.28 or newer host.
`player/packaging/linux.sh` (nix build) is not changed. Its binary needs the newer glibc of the nix toolchain.

## Run it

```sh
player/packaging/manylinux/build.sh [CACHE_DIR]     # CACHE_DIR defaults to ~/.cache/renpy-manylinux
```

Output (gitignored, under `player/build-out`):

```
player/build-out/manylinux/player-linux-x86_64/player        binary, RUNPATH $ORIGIN/lib, interpreter /lib64/ld-linux-x86-64.so.2
player/build-out/manylinux/player-linux-x86_64/lib/          LGPL FFmpeg 7.1, dav1d, libva, libva-drm, libz
player/build-out/manylinux/player-linux-x86_64.tar.gz
```

| File | Role |
|---|---|
| `build.sh` | Host driver. Starts rootless `podman` with the repository at `/src` and the cache at `/cache`. |
| `container-build.sh` | Runs in the container. All pinned inputs are in the `PINS` table at its top. |
| `check-glibc.sh` | Symbol-version policy check (`objdump -T`). The container script runs it at the end. It also works alone: `check-glibc.sh <dir>`. |

Host requirements: Linux x86_64, rootless `podman` (setuid `newuidmap` and `newgidmap`, `/etc/subuid` and `/etc/subgid`
entries for the user), network access. `build.sh` writes minimal `~/.config/containers/{policy.json,registries.conf}` when absent.
Without `podman` on `PATH` it uses `nix shell nixpkgs#podman nixpkgs#fuse-overlayfs`. `PODMAN_GRAPHROOT` moves the image store.
On NixOS add `/run/wrappers/bin` to `PATH` (`build.sh` does). The first run needs about 25 minutes (8 cores); a run with a warm cache needs about 3 minutes (CPython is rebuilt when its inputs change).

## What `container-build.sh` does

1. `dnf install` clang, clang-devel (libclang for bindgen), nasm (powertools repo), libva-devel, libdrm-devel, systemd-devel (libudev.pc), alsa-lib-devel.
2. `pip install` the pinned build tools (Cython 3.2.5, meson, ninja, patchelf) into the image's CPython 3.12. `rustup` installs the stable toolchain into the cache.
3. Builds static, PIC zlib, bzip2, xz, expat, libffi and OpenSSL from the pinned tarballs.
4. Builds the SDL2 headers only (`pkg-config --cflags sdl2` for the `pygame.locals` probe in `engine/build.py`; nothing links SDL2).
5. Builds CPython 3.12.8 static with the recipe of `crates/pyhost/cpython/build_linux.sh`. The script reads `PYVER`, `PYSHA`, `INC`, `CORE` and the `Setup.local` heredoc from that file, so the two cannot drift. Only the library paths differ. The result goes to `player/build-out/cpython` with a `stamp`, so `crates/pyhost/build.rs` does not run the nix script.
6. Builds dav1d (meson, shared) and an LGPL FFmpeg 7.1 shared build. It mirrors `ffmpegLgpl` in `flake.nix`: dav1d on; no GPL, version3, nonfree, x264, x265, xvid, aom, svt-av1 or vpx; VA-API on; vulkan, X11, ALSA and bzip2/lzma/iconv off. The script stops if `config.h` has `CONFIG_GPL`, `CONFIG_VERSION3` or `CONFIG_NONFREE` other than 0.
7. `cargo build --release -p player` with `PKG_CONFIG_PATH` for FFmpeg (ffmpeg-sys-next finds it with pkg-config), `LIBCLANG_PATH=/usr/lib64`, and a `cc` shim that adds `-fPIC`. `engine/build.py` calls a bare `cc` and ignores `CFLAGS`. The nix gcc builds PIE by default and this image's gcc does not, but the Rust linker makes a PIE.
8. Packages like `linux.sh`: copies the non-host libraries that each ELF file needs directly (`objdump -p` NEEDED), sets RUNPATH `$ORIGIN` in `lib/*.so*` and `$ORIGIN/lib` in `player`, strips, checks that every NEEDED entry is bundled or host-provided, writes the tarball, runs `check-glibc.sh`.

Host-provided (never bundled, same set as `linux.sh`): glibc and its companions, libgcc_s, libstdc++, libdrm, ALSA (`libasound`), libudev, the Vulkan loader, Wayland, xkbcommon, X11, xcb, PipeWire, GL/EGL. `libasound.so.2`, `libudev.so.1`, `libdrm.so.2`, `libgcc_s.so.1` and `libstdc++.so.6` are linked at load time (NEEDED), so the host MUST have them. The rest is opened with `dlopen`. `libstdc++` is host-provided because the Vulkan drivers (Mesa) that the player loads need a newer one than this image has: a bundled copy in `lib/` (RUNPATH `$ORIGIN/lib`) is found first and makes every Mesa driver fail to load (`GLIBCXX_3.4.29 not found`, wgpu finds no adapter). `check-glibc.sh` keeps the player at GLIBCXX 3.4.25 or older.

## Pinned inputs

| Input | Version | Source of the pin |
|---|---|---|
| Image | `quay.io/pypa/manylinux_2_28_x86_64@sha256:c2261579b9c2e5d45aa93312f73e2a302182e3e977b558581a1838d6fed3d8e6` | digest in `build.sh` |
| CPython | 3.12.8 | `build_linux.sh` (python.org SHA-256) |
| zlib, bzip2, xz, expat, libffi, OpenSSL 3.0, SDL2, dav1d, FFmpeg | see the `PINS` table | SHA-256 per tarball in `container-build.sh`. OpenSSL and dav1d equal the publisher's `.sha256` file. |
| Cython, meson, ninja, patchelf | exact PyPI versions in `PIP_TOOLS` | version only (no hash) |
| Rust | `stable` through rustup (rustc 1.99.0 in the recorded run) | not pinned |
| dnf packages | AlmaLinux 8 repos of the day (clang 21.1.8, libva 2.13.0, nasm 2.15.03) | not pinned |

## The glibc check

`check-glibc.sh` reads `objdump -T` of the binary and every `lib/*.so*`, takes the newest required `GLIBC_`, `GLIBCXX_`, `CXXABI_` and `GCC_` version per file, and fails when one exceeds the manylinux_2_28 limits (GLIBC 2.28, GLIBCXX 3.4.25, CXXABI 1.3.11, GCC 8.0.0).

## Results (artemis, 2026-10-01)

Build: `build.sh` with rootless podman 5.8.7 (`nix shell nixpkgs#podman nixpkgs#fuse-overlayfs`, overlay storage under `~/Projects/renpy_proj-remote/podman/store`). The image has glibc 2.28.

`check-glibc.sh` (`objdump -T`, newest required version per file):

```
libavutil.so.59: GLIBC=2.28     libavformat.so.61: GLIBC=2.28   libavcodec.so.61: GLIBC=2.27
libva-drm.so.2, libva.so.2: GLIBC=2.4   libswresample/libswscale/libdav1d/libz: GLIBC=2.14
player: GLIBC=2.28 GCC=4.2.0 (no GLIBCXX, no CXXABI)
max: GLIBC=2.28 (limit 2.28) GLIBCXX=none CXXABI=none GCC=4.2.0 (limit 8.0.0)
check-glibc: OK (manylinux_2_28)
```

An independent `objdump -T` on the host gave the same maximum (`GLIBC_2.28`). The binary is a PIE with RUNPATH `$ORIGIN/lib`. `libavutil` reports `LGPL version 2.1 or later`.

Run from a temp directory with an empty environment (`env -i HOME=<tmp> PATH=/usr/bin:/bin NIX_LD=... NIX_LD_LIBRARY_PATH=...`), the package loads every FFmpeg, dav1d, libva and libz library from its own `lib/` (checked with `LD_DEBUG=files`). On artemis (NixOS) `NIX_LD_LIBRARY_PATH` MUST list alsa-lib, libdrm, udev and the window-system libraries (the gate adds them). The nix-ld default set lacks `libdrm.so.2`: the player stops at load time without it. Other distributions have it.

Gate (the lock is held by the harness; the machine lock was held for the whole run):

```
python3 harness/gate.py run --engine player --player-bin <tmp>/player-linux-x86_64/player --game SecretIsland --tier m1 --out harness/out/manylinux-si-m1
lint pass (64793 dialogue blocks), probe pass (66 of 60 lines, digest 5b3a5b1c65f1ae8d), route pass (self diff 0.00000)
gate exit=0
```

The `m1` tier includes `lint`.
