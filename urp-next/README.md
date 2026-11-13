# urp-next

Greenfield Ren'Py 7/8 loader. `player/` is reference code, not a dependency.
N0 establishes the build boundary and pure test loop. There is no player binary yet.

## Commands

From the repository root, with Cargo and rustc on PATH:

```sh
urp-next/tools/test t0
urp-next/tools/test rules
```

The repository pins Rust through nixpkgs in the root `flake.lock`. Reuse it with
`nix develop -c urp-next/tools/test t0`. No second rust-toolchain pin is maintained.
T0 itself requires only the Rust toolchain and a system linker, not Nix, Python,
FFmpeg, a GPU, or any `URP_*` variable. Cargo downloads Rust dependencies on the
first invocation if they are not cached. Subsequent runs use the lockfile.

`t0` tests `urp-core`, including the dependency rule check. As more pure crates
land, add their package selections to `tools/test`. Do not use workspace-wide
builds for this tier. `rules` runs only the architecture check. It reads
`cargo metadata --all-features`, traverses all resolved dependency kinds, and
rejects pyo3 and pyo3 support crates transitively in every workspace package
except `py-*`, `urp-pyhost`, and `urp-host`. Metadata does not execute build scripts.
Only `urp-pyhost` may have a build script. Non-linking exceptions require an
explicit reviewed addition to the empty allowlist in `tools/dependency-rules.rs`.
This fails closed rather than trying to recognise link strings in arbitrary code.
Third-party dependency build scripts are not subject to this workspace rule.

Dev and test builds use line tables, optimisation level 1 for workspace crates,
and level 2 for dependencies. Release is reserved for packaged builds.

`t1` and `t2` need the prebuilt native inputs, so run them in the `next` shell:
`nix develop .#next -c urp-next/tools/test t1`. `t1` boots the static CPython with
the engine modules through `urp-pyhost`. `t2` runs the engine Python tests of the
old tree in `urp-enginepy`, with no cargo build; they move here with the code they
test. Cargo never builds a native input: `urp-pyhost` reads `URP_CPYTHON` and
`URP_ENGINE`, which the shell sets to Nix store paths.

Planned, not runnable commands: T3 media and GPU crate tests; T4
headless synthetic-game goldens; T5 stock differential; T6 real games and GPUs.
No placeholder tier commands exist.

## Launch contract

The loader takes a game path and a manager-written JSON launch record, not a
library name or key. `LaunchSpec::from_files` derives and validates the immutable
spec before the host starts. A record looks like:

```json
{
  "schema_version": 1,
  "data_root": "../urp-next-data",
  "mode": "headless",
  "viewport": [1280, 720],
  "args": [],
  "renpy_major": null
}
```

A relative data root is relative to the record file. `LaunchSpec::new`
also accepts an already-parsed record, with an absolute data root. The record
carries no automation script: N1 defines the automation interface. The optional
`renpy_major` override is `"7"` or `"8"`, for stripped distributions. Otherwise
`GameProfile` reads the shipped engine version, selects the Python 2 branch in
dual-version distributions using runtime markers, or records an unknown major.
Unknown is not silently classified as Ren'Py 8. The eventual host must decide
whether it can launch an unknown profile. No Python executes during detection.

Game paths may be a distribution root, its `game/` directory, a bare script
folder, or a `.app` with `Contents/Resources/autorun/game`. Paths are canonicalised
once. A symlink and its target share an ID. The ID is `g-` plus SHA-256 of the
canonical game directory with a versioned domain prefix; moving a game changes
its ID. Display names never become path components. Paths must be UTF-8 because
they cross the JSON and Python boundary.

Fresh data layout, with no migration from the old keys, saves, or mods:

```text
<data_root>/games/<game_id>/
  cache/<cache_epoch>/
  saves/
  logs/
  reports/
```

Core only derives paths. The host creates directories when needed. The loader
supplies the cache epoch, because only the loader knows which engine build it
carries. The manager supplies any merged game directory; the loader knows
nothing about mod or library storage.

`StatusEvent::write_line` serialises and flushes one JSON object per line to a
dedicated writable pipe. Logs must not use that pipe. Event tags are
`engine_imported`, `script_loaded`, `init_done`, `quit` (with `code`), and `failed`
(with `phase` and `message`). Embedded newlines are JSON-escaped.

## Native boundary

Cargo must never build native inputs. Nix supplies these paths to the host:

- `URP_CPYTHON`: static patched CPython 3.12.8 prefix, including
  `lib/libpython3.12.a`, static dependencies, `include/python3.12`, `stdlib.zip`,
  and `pyo3-config.txt`.
- `URP_FFMPEG`: LGPL FFmpeg prefix.
- `URP_ENGINE`: `libengine_cy.a`, `inittab.txt`, `layer.zip`, `common.zip`, and
  patched Ren'Py 8.5.3 under `tree/`.
- `URP_ENGINEPY`: test interpreter executable with the Cython builtins and no Rust.

Only `urp-pyhost` emits link directives. Pure crates never depend on pyo3.
