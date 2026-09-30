# Windows packaging spike on a Windows Server VM (ticket #32)

Question: does the route (one Rust binary = static CPython 3.12 + Ren'Py 8.5.3's Python layer + Ren'Py's own Cython modules rebuilt unchanged, no per-game `lib/`) hold on Windows? Follows [Single-binary CPython 3.12 packaging per OS](../pypack/README.md) (its §3 left Windows open), [research/boundary](../boundary/README.md) (the "rebuild as-is" module list) and [research/licence](../licence/README.md).

**Verified** = built and run on the VM below (all numbers measured here). **[INFERENCE]** = not exercised. No game was run (out of scope); "imports" means `importlib.import_module` inside the embedded interpreter.

## 1. Answer

| | result |
|---|---|
| **Q1a** PBS Windows objects linked statically | **Works.** PBS has no static flavour, but its `pgo-full` archive ships the 210 core COFF objects and each extension's objects (`PYTHON.json`). Archived into a `.lib` and linked into an MSVC Rust exe: interpreter starts, in-memory stdlib blob loads, dotted builtin submodule registers, `_ctypes`/`_ssl`/`_hashlib`/`_decimal`/`_lzma`/`_bz2` all builtin. **Dynamic CRT only** (`/MD`): needs `VCRUNTIME140.dll` + UCRT. §3 |
| **Q1b** self-built static CPython 3.12.8 with `/MT` | **Also works** (probed although (a) already worked, because Q3 asks about the CRT). 256 sources compiled with plain `cl`, no PCbuild static config; exe imports **only Windows system DLLs** (no `VCRUNTIME140`, no `api-ms-win-crt-*`). Missing: `_lzma` (§3.3). |
| **Q1c** `python312.dll` beside the exe | not needed |
| **Q2** Ren'Py Cython modules | **All 31 generated C files compile with MSVC and import** as builtin dotted modules: the 29 unpatched, plus `accelerator` and `gl2model` with the two one-line boundary patches. §4 |
| **Q3** | Static vs dynamic CRT and signing findings in §5. |

Recommendation: ship the Windows player from variant **(b)** (self-built `/MT`) if the CI can carry a 256-file compile script; use **(a)** (PBS objects) as the fast path/cross-check. Both are one exe with the same `main.rs`. Do not use `+crt-static` with (a): it does not link (§5).

## 2. VM and toolchain (installs, all under `C:\spike\`, left in place)

Windows Server 2022 Standard build 20348, 4 vCPU, 8 GB, no GPU. Installed (scripts `vm/01-install-toolchain.ps1`, `vm/b0-vcpkg.bat`, `vm/b0b-vcpkg-mt.bat`):

| tool | version | how |
|---|---|---|
| Visual Studio Build Tools 2022, workload `VCTools` (`--includeRecommended`) | 17.14.41; MSVC 14.44.35207 (`cl` 19.44.35229, `link` 14.44.35229); Windows SDK 10.0.26100; bundled CMake 3.31.6 | `vs_BuildTools.exe --quiet --wait --norestart` |
| Git for Windows | 2.56.0 | `Git-2.56.0-64-bit.exe /VERYSILENT` |
| Python (build host) | 3.12.8 in `C:\spike\tools\py312` | `python-3.12.8-amd64.exe /quiet` |
| rustup + rustc/cargo | rustc 1.98.1 stable-x86_64-pc-windows-msvc, `RUSTUP_HOME`/`CARGO_HOME` in `C:\spike\tools`, set machine-wide | `rustup-init.exe -y` |
| pip: Cython 3.2.4 (the version in Ren'Py's `uv.lock`), setuptools 84.0.0, six 1.17.0, ecdsa 0.19.2 | | |
| vcpkg | 2026-09-26 (`C:\spike\vcpkg`): `libffi` 3.8.0 and `openssl` 3.6.4 for triplets `x64-windows-static-md` and `x64-windows-static`; it downloaded its own portable msys2, Strawberry Perl 5.42.2.1, NASM 3.01, jom 1.1.7 | 2 x ~16 min |
| CPython 3.12.8 source (`Python-3.12.8.tgz`) + `PCbuild\get_externals.bat` (zlib 1.3.1, bzip2 1.0.8, xz 5.2.5, libffi-3.4.4 bin, openssl-bin 3.0.15, sqlite, tcltk; only zlib and bzip2 were used) | | |
| python-build-standalone `20260924` CPython 3.12.14 `x86_64-pc-windows-msvc-pgo-full` | | `fetch.sh` |

Disk: 40 GB free of 59 after everything. Scheduled tasks `spike_*` (one-shot, SYSTEM) remain; they are how long jobs ran detached (`vm/run-detached.ps1`). SSH never dropped during the short jobs; every long one used a task plus a log.

## 3. CPython on Windows with no per-game `lib/`

### 3.1 The host (same for both variants)

`host/` = `pyo3-ffi` declarations only, `PYO3_CONFIG_FILE` with `shared=false` and `suppress_build_script_link_lines=true`, and `build.rs` linking the libs. `src/main.rs` is the macOS probe (`../pypack/embed-probe`) ported: frozen `encodings` (`mkboot.py`), the stdlib zip as `include_bytes!` exposed as builtin `_blob`, the 40-line importer `bootstrap_blob.py`, isolated `PyConfig` with an empty `sys.path` and `site_import=0`, a Rust builtin `probe_pkg.fast` registered as a dotted name with `PyImport_AppendInittab`, plus `inittab_gen.rs` (from `mkinittab.py`) registering every extra builtin. The blob also carries Ren'Py's `renpy/**/*.py`.

Windows faults that cost a rebuild each (put in the CI recipe):
1. **`/WHOLEARCHIVE` is required for the PBS `python312.lib`.** A normal static link left `PyImport_FrozenModules` unresolved; PBS's objects are `/GL` (LTCG IL) and archive-member lookup does not see every symbol. `link.exe` prints "restarting link with /LTCG": PBS built with MSC v.1944 and Build Tools 17.14 is 14.44, so they matched. A mismatched toolset would refuse the IL objects **[INFERENCE: not tested]** — the same class of problem as PBS's LLVM-22 bitcode on macOS.
2. `pyo3-ffi` still emits `link(name="pythonXY")` on Windows: satisfy it with an empty `pythonXY.lib` (`vm/a2-core-lib.bat`).
3. PBS's extension objects are compiled `dllimport`. Link.exe resolves `__imp_Py*` against the objects defining the symbol (warnings LNK4286/LNK4217, no errors), so `_socket`, `select`, `unicodedata`, `_bz2`, `_lzma`, `pyexpat`, `_elementtree`, `_overlapped`, `_asyncio`, `_decimal`, `_multiprocessing`, `_queue`, `_zoneinfo`, `_uuid` link as-is. **Third-party libraries it does not resolve**: libffi needs a shim (`vm/ffi_imp_shim.c`: `void *__imp_ffi_call = &ffi_call;` per symbol, 15 of them) and its objects define a second `DllMain`, worked around with `/FORCE:MULTIPLE` (variant (a) only; a wart).
4. `pyexpat` and `_elementtree` each carry their own expat objects; keep one copy (`vm/a4_extlib.py`).
5. Static builds have no `MS_COREDLL`, so **`sys.dllhandle` is missing and `import ctypes` fails** (`ctypes/__init__.py` L468). The host sets `sys.dllhandle` to `GetModuleHandleW(NULL)` (`main.rs`). `ctypes.pythonapi` only finds symbols if the exe exports them (true for PBS's dllexport objects, not for (b)) [INFERENCE: `pythonapi` not exercised].
6. `sys.prefix` still comes out as the user profile directory (getpath fallback); set explicit prefix fields in the real host (same as macOS fault 5).

### 3.2 Variant (a): PBS objects (`vm/a1..a5`, `vm/b3`)

`lib.exe` over `build/core/*.obj` gives a 76 MB `python312.lib`; extension objects go into `pbsext.lib`. `_ctypes`, `_hashlib`, `_ssl` need libraries PBS only supplies as DLLs (libffi-8, libcrypto/libssl-1_1): linked against vcpkg `x64-windows-static-md` `ffi`, `libssl`, `libcrypto` (OpenSSL 3.6.4) instead. `_lzma` links PBS's own static `liblzma.lib`. `_sqlite3` and `winsound`/`_wmi` were skipped (not needed).

### 3.3 Variant (b): self-built `/MT` (`vm/c0..c2`)

`vm/c1_static_crt_build.py` takes the file list from PBS's `PYTHON.json` (core + extension objects) and compiles each mapped source from the python.org 3.12.8 tree with `cl /MT /O2 /DPy_NO_ENABLE_SHARED /DPy_BUILD_CORE`, 256 files, **40 s on 4 cores**, 0 failures. Things it needs that are not in the tarball:
- generated frozen headers and `Python/deepfreeze/deepfreeze.c`: run `msbuild PCbuild\_freeze_module.vcxproj` first (`vm/c0-msbuild-core.bat`; `pythoncore` alone fails on `getpath.h`);
- per-file defines from the vcxproj: `getpath.c` (`PREFIX=NULL EXEC_PREFIX=NULL VERSION=NULL VPATH PYDEBUGEXT PLATLIBDIR`), `sysmodule.c` (`VPATH`), `_Py_HAVE_ZLIB`/`USE_ZLIB_CRC32` with zlib from `externals/`, `CONFIG_64`/`ANSI` and the `libmpdec` include only for `_decimal` (a global `CONFIG_64` breaks `posixmodule.c`), `FFI_STATIC_BUILD` for `_ctypes`;
- name collisions between groups (`context.c` in `Python/` vs `libmpdec`, `crc32.c` in zlib vs xz): resolved by group.
Extensions built the same way: `_socket select unicodedata pyexpat _elementtree _asyncio _queue _zoneinfo _uuid _multiprocessing _overlapped _decimal _bz2 _ctypes _ssl _hashlib`. **`_lzma` is missing**: it needs liblzma built `/MT`; PBS's `liblzma.lib` is `/MD`; xz 5.2.5's tree is a separate build [INFERENCE that it is a mechanical add]. Because it is our own build, no shims, no `DllMain` clash, no `/FORCE`, and no LTCG dependence on the toolset version.

### 3.4 Measurements (`results/*-run.txt`)

Host built `--release`, no debuginfo, no LTO; `CARGO_BUILD_JOBS=4`; link peak memory was not a problem on 8 GB.

| | (a) PBS objects, all ext | (b) self-built `/MT`, all ext but `_lzma` |
|---|---|---|
| exe size (incl. 5.9 MB blob: 4.1 MB stdlib zip + `renpy/**/*.py` + `ecdsa`/`six`; + 31 Cython modules, 12 MB of objects) | **22.97 MB** | **22.56 MB** |
| for scale: stock `python312.dll` (PCbuild) | 7.5 MB | |
| `Py_InitializeFromConfig` | 6.9-9.7 ms | 6.6-8.5 ms |
| blob importer bootstrap | 6.2-10.4 ms | 6.3-9.1 ms |
| whole process, empty script (median/min/max of 20, incl. PowerShell spawn) | 26.4 / 24.8 / 31.6 ms | 26.2 / 24.8 / 30.9 ms |
| in-process import of the 104-name macOS census list | 100 ok, 40 ms | 99 ok, 40 ms |
| whole process running that import script | 112 ms | 115 ms |
| `import renpy` (blob, incl. `vc_version` stub) | 29-31 ms | 31 ms |
| DLLs imported | 16 system + **VCRUNTIME140 + 13 `api-ms-win-crt-*`** | **16 system only** (`KERNEL32 ADVAPI32 WS2_32 USER32 SHELL32 OLE32 OLEAUT32 CRYPT32 COMDLG32 RPCRT4 IPHLPAPI BCRYPT VERSION ntdll` + api-set core-path/synch) |

The 4-5 census names that fail are macOS/POSIX only (`_posixsubprocess`, `_scproxy`, `fcntl`, `grp`), plus `_lzma` in (b). `hashlib`, `ssl` (OpenSSL 3.6.4) and `ctypes` import in both. Compared with macOS (12.3 MB, init 5-8 ms, 40-50 ms whole process): larger because of Ren'Py's layer, OpenSSL and the Cython modules; start-up costs are the same.

## 4. Ren'Py 8.5.3 Cython modules with MSVC (`vm/gen_cython.py`, `b1..b3`, `cy_probe.py`)

Generation runs Ren'Py's own `scripts/setuplib.py` (`RENPY_STATIC=1`, Cython 3.2.4): same Cython flags, and the same `PyInit_renpy_display_render` renaming + dotted `__pyx_moduledef` name that renpy-build's `librenpy` uses. Compiled with `cl /O2` against the 3.12 headers; objects linked into the host next to CPython; registered via `PyImport_AppendInittab` under the dotted name; imported with the `BuiltinSubmoduleImporter` from `pypack`.

Result per module (`results/cython-compile-patched.json`, `results/cython-import-run.txt`): **31 of 31 compile and import** (MSVC 19.44, with `/w`: warnings were not audited):

| module | compile | import (as builtin) |
|---|---|---|
| `astsupport` `cslots` `lexersupport` `pydict` `style` `encryption` (libhydrogen in-tree) `tfd` (tinyfiledialogs; needs `comdlg32`) `audio.filter` `display.matrix` `display.render` `display.quaternion` | ok | ok |
| `styledata.styleclass` `stylesets` and the 11 generated `style_*functions` | ok | ok |
| `text.textsupport` `text.texwrap` | ok | ok |
| `gl2.gl2mesh` `gl2mesh2` `gl2mesh3` `gl2polygon` | ok | ok |
| `display.accelerator` | **fails unpatched**: `SDL2/SDL.h` not found (`results/cython-compile-unpatched.txt`); ok with the boundary patch (drop `nogil_copy` and the sdl2 imports) | ok |
| `gl2.gl2model` | **fails unpatched**: `renpygl.h` includes `SDL2/SDL.h`; ok with the one-line patch (drop `GLTexture` cimport, duck-type the `isinstance`) | ok |

So the boundary research's claim holds on Windows: 20 modules plus the generated style functions build unmodified, two need their one-liner. Nothing MSVC-specific was needed in the C (`-std=gnu99`/`-Wno-unused-function` in `setuplib` are ignored by our driver).

Import conditions found (each is host work, not MSVC):
- **`renpy.pygame` is imported at module load by `render.pyx`**, and `renpy.import_all()` imports every replaced module (`ftfont`, `hbfont`, `gl2draw`, ...). To import the kept modules the probe installs empty stand-in modules for the replaced set (`cy_probe.py`, dummy classes: proves only that the kept modules' own imports work, not that Ren'Py runs). `renpy.import_all()` then gets past all real imports and stops in a stand-in artefact (`Cannot pickle renpy.loader.RWopsIO`); it was not pursued.
- `renpy/vc_version.py` (generated by renpy-build at packaging time; stand-in in `vcv/`) must exist, else `renpy/versions.py` calls `open(__file__/../.git/HEAD)`. The blob importer also now sets `__file__` (`bootstrap_blob.py`).
- **`setuplib` reads `.pyx` with the locale codec**: on Windows `PYTHONUTF8=1` is required (cp1252 fails on byte 0x90). Add to the CI environment.
- `ecdsa` (+`six`) must be in the blob or `import_all` stops at `renpy/savetoken.py:26`, as in the pypack census.
- `_renpy`, `renpysound`, `ftfont`, `hbfont`, `bidi`, `gl2draw` and friends, `assimp` were not built (SDL/FreeType/FFmpeg headers absent); they are the replaced set.

## 5. Things that change the macOS plan

### 5.1 CRT: static (`/MT`) vs dynamic (`/MD`)
- **PBS's objects are `/MD`.** Linking them into a `+crt-static` Rust exe fails with hundreds of `unresolved external symbol __imp_strncpy/__imp_close/__imp_clock...` (`vm/a6-crt-static-test.bat`). So variant (a) means `VCRUNTIME140.dll` + UCRT (`api-ms-win-crt-*`) at run time: UCRT is part of Windows 10/Server 2016+; the VC++ redistributable is not, so (a) needs the redistributable's `vcruntime140.dll` beside the exe or installed **[INFERENCE for a clean machine: the VM has VS installed, so VCRUNTIME140 was always present here]**.
- **Variant (b) with `/MT` removes both**, and does not need the Rust host and the interpreter to agree on anything else. Rust's default MSVC target is `/MD`; use `RUSTFLAGS=-C target-feature=+crt-static` for the host (done, `vm/c2-mt-host.bat`). Everything linked into the exe must then be `/MT`: vcpkg triplet `x64-windows-static` (not `-static-md`), Ren'Py's Cython objects compiled `/MT` (done, `cy_mt`), and the future wgpu/ffmpeg/dav1d libs. **One `/MD` static lib in the graph breaks the choice** (`LNK4098`/`__imp_` errors); the macOS plan has no equivalent constraint.
- Rule for the plan: pick **`/MT` for the whole Windows player**, build CPython ourselves, and keep PBS objects as a cross-check only.

### 5.2 Windows Authenticode (`vm/d1-signing.ps1`, `results/sign-run.txt`)
Self-signed cert (`New-SelfSignedCertificate`, `Cert:\LocalMachine\My`; the per-user store is `NTE_PERM` over SSH), trusted locally, `Set-AuthenticodeSignature` SHA256, on the 18.7 MB single exe:
- signed after link: `Valid`, signed exe runs, size +1.4 KB;
- **appended data after signing: `NotSigned`** (breaks); **append then sign: `Valid`** and runs; one byte flipped in `.text`: `HashMismatch`;
- so an embedded blob (`include_bytes!`) is inside the hashed image and is fine; sign as the last build step. A real certificate, SmartScreen reputation and timestamping were not tested (no cert available). The mac ordering constraint ("appending breaks signing") applies to Windows too, but only if the append comes after signing.

### 5.3 Other deltas to the macOS plan
- `PyImport_FrozenModules` and the rest of the frozen-`encodings` recipe work unchanged on Windows.
- The `BuiltinSubmoduleImporter` + `PyImport_AppendInittab` route works unchanged for Cython modules (31 registered).
- `winreg`, `msvcrt`, `_winapi`, `zlib` are already in CPython's core objects (no extra work); `_overlapped` builds; `nt`-only concerns need no `posixsubprocess`.
- `getpath` leaks a user-profile prefix into `sys.prefix`; set `PyConfig` prefixes.
- OpenSSL: PBS's `_ssl`/`_hashlib` objects are built against the OpenSSL 3 API and accept vcpkg's static OpenSSL 3.6.4; PBS's own import libraries name 1.1.1 DLLs and are not usable statically. Updater/`renpy.fetch` only need this; `hashlib` works without it.

## Implications for the build

1. **Windows one-binary is confirmed.** One exe, no `python312.dll`, no `lib/`: 22.6-23 MB with stdlib, Ren'Py's Python layer, OpenSSL, ctypes and the 31 rebuilt Cython modules; ~7 ms interpreter init, ~26 ms whole-process for an empty script.
2. **Build CPython ourselves with `/MT`** (variant b): `vm/c0`+`c1` recipe (freeze via `_freeze_module`, then 256 `cl` calls, 40 s), `x64-windows-static` vcpkg libs, `RUSTFLAGS=-C target-feature=+crt-static`. It gives system-DLL-only imports, no shims, no `/FORCE:MULTIPLE`, and no LTCG toolset pin. Add `_lzma` (static liblzma `/MT`) to close the module gap.
3. **PBS objects are a working fallback** (variant a) but link only as `/MD` (needs the VC++ redistributable), need `/WHOLEARCHIVE`, the `pythonXY.lib` stub, libffi `__imp_` shims, a `DllMain` workaround, and a matching MSVC 14.44. Use them to cross-check, not to ship.
4. **Everything else in the Windows graph must be `/MT` and static**: one `/MD` library makes the choice fail. Carry this into the wgpu/FFmpeg/dav1d/FreeType/HarfBuzz tickets.
5. **Ren'Py's Cython modules build as-is with MSVC**: 29 unpatched, `accelerator` and `gl2model` with the two boundary one-liners, generated through Ren'Py's own `setuplib` in `RENPY_STATIC` mode (needs `PYTHONUTF8=1` on Windows). Register with the same dotted inittab names as macOS.
6. **The host must provide** `sys.dllhandle` (static build), explicit `PyConfig` prefixes, `renpy/vc_version.py`, `ecdsa`+`six` in the blob, and the replaced modules (`renpy.pygame`, `ftfont`, ... ) before `renpy.import_all()`: the kept Cython modules import them at load time (`render.pyx`).
7. **Signing**: sign the finished exe last (SHA256 Authenticode + timestamp with a real cert, HITL); do not append data after signing. An embedded blob is safe.
8. **Open**: `_lzma` for (b); `ctypes.pythonapi` under (b) (needs exports); `_sqlite3`/`winsound`/`_wmi` were not linked; a real certificate and SmartScreen behaviour; a clean machine without VS for (a)'s redistributable claim; ARM64 Windows (PBS has aarch64 objects, not tried); no game was run.

## Reproduce

Everything under `vm/` runs on the VM (`C:\spike\ws\vm\`, `env.bat` sets the MSVC/rust/python environment). Order: `01-install-toolchain.ps1` (detached, `run-detached.ps1`); copy `upstream/pbs.zip`, `renpy-src.tgz` (from `./fetch.sh`) to `C:\spike\dl`, unpack to `C:\spike\pbs`, `C:\spike\renpy-src`; `pip install cython==3.2.4 setuptools six ecdsa`; `a1-blobs`, `a2-core-lib`, `a4-ext` + `a5-host-ext` (variant a base); `gen_cython.py` (with `PYTHONUTF8=1 RENPY_STATIC=1 RENPY_REGENERATE_CYTHON=1 RENPY_CYTHON=...`), `b1-compile`, `b2-patched`, `b3-cy-host` (variant a with Cython, ctypes, ssl; needs `b0-vcpkg`); `b0b-vcpkg-mt`, `c0-msbuild-core`, `c2-mt-host` (variant b; runs `c1_static_crt_build.py`); `a6-crt-static-test`, `d1-signing.ps1`, `measure.ps1`. Probe scripts: `probe_main.py`, `cy_probe.py`, `ssl_probe.py` (run with `PROBE_EXEC=<file>`), `bootstrap_blob.py`, `mkzip.py`, `mkboot.py`, `mkinittab.py`. Raw run outputs: `results/`.
