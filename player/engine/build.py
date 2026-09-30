#!/usr/bin/env python3
"""Engine layer build (run by crates/engine/build.rs inside `nix develop .#player`).

Inputs : player/upstream/renpy-8.5.3 (engine/fetch.sh), engine/patches, engine/python, engine/extra.
Outputs: player/build-out/engine/
    libengine_cy.a   all Cython modules plus their C helpers (one static library)
    inittab.txt      "<dotted module> <PyInit symbol>" per line
    link.txt         "search <dir>" / "lib <name>" lines for the native libraries the archive needs
    layer.zip        renpy/**/*.py (patched) + engine/python as unchecked-hash .pyc
    common.zip       renpy/common (sources and assets)
    fingerprint.txt  BUILD_FINGERPRINT
    tree/            patched Ren'Py tree (scratch)

Usage: build.py [--py-include DIR]
Needs on PATH: python3.12, cython (3.x), cc, ar, pkg-config, git, and the devshell pkg-config path
(freetype2, harfbuzz, sdl2).
"""

import argparse
import concurrent.futures
import hashlib
import importlib.util
import marshal
import os
import re
import shutil
import subprocess
import sys
import sysconfig
import types
import zipfile
from pathlib import Path

ENGINE = Path(__file__).resolve().parent
PLAYER = ENGINE.parent
UPSTREAM = PLAYER / "upstream" / "renpy-8.5.3"
OUT = PLAYER / "build-out" / "engine"
TAG = "8.5.3.26051504"
VERSION_NAME = "We Can Go to the Moon"

# Cython modules that stay (research/boundary section 2.1, with the accelerator and gl2model patches),
# plus the bring-up text modules. Value: extra C sources compiled into the archive once.
KEPT = {
    "renpy.astsupport": [],
    "renpy.cslots": [],
    "renpy.lexersupport": [],
    "renpy.pydict": [],
    "renpy.style": [],
    "renpy.encryption": [],  # includes libhydrogen/hydrogen.c itself
    "renpy.tfd": ["src/tinyfiledialogs/tinyfiledialogs.c"],
    "renpy.audio.filter": [],
    "renpy.styledata.styleclass": [],
    "renpy.styledata.stylesets": [],
    "renpy.display.matrix": [],
    "renpy.display.render": [],
    "renpy.display.accelerator": [],
    "renpy.display.quaternion": [],
    "renpy.gl2.gl2polygon": [],
    "renpy.gl2.gl2mesh": [],
    "renpy.gl2.gl2mesh2": [],
    "renpy.gl2.gl2mesh3": [],
    "renpy.gl2.gl2model": [],
    "renpy.text.textsupport": [],
    "renpy.text.texwrap": [],
    "renpy.text.ftfont": ["src/ftsupport.c", "src/ttgsubtable.c"],
    "renpy.text.hbfont": ["src/ftsupport.c", "src/ttgsubtable.c"],
}

# Python files that are data for the common zip, not importable layer modules.
COMMON_DIR = "renpy/common"
STORED_EXT = {".ogg", ".oga", ".mp3", ".opus", ".png", ".jpg", ".jpeg", ".webp", ".ttf", ".otf", ".ttc", ".woff"}

PROBE_NOTE = "generated at build time from the SDL2 headers"


def log(*a):
    print("[engine-build]", *a, flush=True)


def run(cmd, cwd=None, env=None, capture=False):
    r = subprocess.run(cmd, cwd=cwd, env=env, text=True, capture_output=capture)
    if r.returncode:
        if capture:
            sys.stderr.write(r.stdout or "")
            sys.stderr.write(r.stderr or "")
        raise SystemExit(f"command failed ({r.returncode}): {' '.join(map(str, cmd))}")
    return r.stdout if capture else None


def pkg_config(*args):
    return subprocess.check_output(["pkg-config", *args], text=True).split()


def files_under(root: Path):
    return sorted(p for p in root.rglob("*") if p.is_file() and "__pycache__" not in p.parts)


def digest_inputs(extra_files):
    """BUILD_FINGERPRINT: Ren'Py tag, patches and python/ (contract). Returns (fingerprint, stamp)."""
    h = hashlib.sha256()
    h.update(TAG.encode())
    for root in (ENGINE / "patches", ENGINE / "python"):
        for p in files_under(root):
            h.update(str(p.relative_to(root)).encode() + b"\0" + p.read_bytes() + b"\0")
    fingerprint = h.hexdigest()[:16]
    s = hashlib.sha256(fingerprint.encode())
    for p in extra_files:
        s.update(os.path.relpath(p, ENGINE).encode() + b"\0" + p.read_bytes() + b"\0")
    commit = subprocess.check_output(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"], text=True).strip()
    s.update(commit.encode())
    return fingerprint, s.hexdigest()


# ----------------------------------------------------------------------------------------------
# Tree preparation
# ----------------------------------------------------------------------------------------------


def prepare_tree(tree: Path):
    if tree.exists():
        shutil.rmtree(tree)
    tree.mkdir(parents=True)
    ignore = shutil.ignore_patterns("__pycache__", "*.pyc", "*.rpyc")
    for d in ("renpy", "src"):
        shutil.copytree(UPSTREAM / d, tree / d, ignore=ignore)
    (tree / "scripts").mkdir()
    shutil.copy2(UPSTREAM / "scripts" / "generate_styles.py", tree / "scripts" / "generate_styles.py")

    for patch in sorted((ENGINE / "patches").glob("*.patch")):
        log("apply", patch.name)
        # Not `git apply`: inside the worktree it silently skips paths that git ignores.
        run(["patch", "-p1", "--no-backup-if-mismatch", "--fuzz=0", "-i", str(patch)], cwd=tree)

    # Overlay: engine/python (same relative paths as the Ren'Py tree, plus _player), then engine/extra.
    for root in (ENGINE / "python", ENGINE / "extra"):
        for p in files_under(root):
            dest = tree / p.relative_to(root)
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(p, dest)


def write_vc_version(tree: Path):
    """Ren'Py computes its version from git or vc_version.py; a plain tree would report the main branch."""
    (tree / "renpy" / "vc_version.py").write_text(
        "# Written by player/engine/build.py: the stock 8.5.3 version, so scripts and saves see the same value.\n"
        f"branch = 'fix'\nnightly = False\nofficial = True\nversion = {TAG!r}\nversion_name = {VERSION_NAME!r}\n"
    )


def generate_styles(tree: Path):
    """Runs scripts/generate_styles.py (setup.py L110-111) with a stand-in for scripts/setuplib.py,
    which needs setuptools. generate_styles only reads setuplib.gen."""
    stub = types.ModuleType("setuplib")
    stub.gen = "tmp/gen3"
    sys.modules["setuplib"] = stub
    sys.path.insert(0, str(tree / "scripts"))
    cwd = os.getcwd()
    os.chdir(tree)
    try:
        (tree / "tmp" / "gen3").mkdir(parents=True, exist_ok=True)
        spec = importlib.util.spec_from_file_location("generate_styles", tree / "scripts" / "generate_styles.py")
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        mod.generate()
        return [p for p in mod.prefixes]
    finally:
        os.chdir(cwd)
        sys.path.remove(str(tree / "scripts"))
        del sys.modules["setuplib"]


def generate_pygame_constants(tree: Path, work: Path):
    """Pure-Python renpy.pygame.locals and the colour table, from the stock .pyx/.pxi sources.

    locals.pyx defines its constants from SDL2 symbols. A small C probe compiled against the SDL2 headers
    prints every symbol's value; the symbols in the source text are then replaced by those numbers, so
    the constants keep SDL2's numeric values."""
    inc = tree / "src" / "pygame" / "include"
    text = (tree / "renpy" / "pygame" / "locals.pyx").read_text()

    def expand(m):
        return (inc / m.group(1)).read_text()

    text = re.sub(r'^include "([\w.]+\.pxi)"\s*$', expand, text, flags=re.M)
    text = re.sub(r"^(from sdl2 cimport \*|cimport sdl2)\s*$", "", text, flags=re.M)

    code_only = "\n".join(re.sub(r"#.*", "", ln) for ln in text.splitlines())
    symbols = set(re.findall(r"\bsdl2\.([A-Za-z_][A-Za-z_0-9]*)", code_only))
    bare = set(re.findall(r"(?<![\w.])(SDL\w*)", code_only))
    bare -= set(re.findall(r"^\s*(\w+)\s*=(?!=)", code_only, flags=re.M))  # names the file defines itself
    symbols |= bare
    symbols = sorted(symbols)

    probe = work / "sdl_probe.c"
    probe.write_text(
        "#include <stdio.h>\n#include <SDL.h>\nint main(void) {\n"
        + "".join(f'    printf("{s}=%lld\\n", (long long)({s}));\n' for s in symbols)
        + "    return 0;\n}\n"
    )
    exe = work / "sdl_probe"
    run(["cc", "-w", str(probe), "-o", str(exe), *pkg_config("--cflags", "sdl2")])
    values = dict(line.split("=") for line in subprocess.check_output([str(exe)], text=True).splitlines())

    def sub_qualified(m):
        return values[m.group(1)]

    text = re.sub(r"\bsdl2\.([A-Za-z_][A-Za-z_0-9]*)", sub_qualified, text)
    text = re.sub(r"(?<![\w.])(SDL\w*)", lambda m: values.get(m.group(1), m.group(1)), text)
    dest = tree / "renpy" / "pygame" / "locals.py"
    dest.write_text(f"# renpy.pygame.locals, {PROBE_NOTE}; SDL2 numeric values.\n" + text)
    compile(dest.read_text(), str(dest), "exec")  # syntax check

    cd = (inc / "color_dict.pxi").read_text().replace("cdef object colors = {", "colors = {", 1)
    (tree / "renpy" / "pygame" / "_color_dict.py").write_text(f"# Colour names, {PROBE_NOTE}.\n" + cd)


# ----------------------------------------------------------------------------------------------
# Cython and C
# ----------------------------------------------------------------------------------------------


def module_list(tree: Path, style_prefixes):
    mods = dict(KEPT)
    for p in style_prefixes:
        mods[f"renpy.styledata.style_{p}functions"] = []
    # extra/: additive modules owned by other slices; the module name is the path under extra/.
    for p in files_under(ENGINE / "extra"):
        if p.suffix == ".pyx":
            mods[".".join(p.relative_to(ENGINE / "extra").with_suffix("").parts)] = []
    return mods


def pyx_path(tree: Path, name: str) -> Path:
    if name.startswith("renpy.styledata.style_") and name.endswith("functions"):
        return tree / "tmp" / "gen3" / (name.rsplit(".", 1)[1] + ".pyx")
    return tree / (name.replace(".", "/") + ".pyx")


def cythonize(tree: Path, cdir: Path, mods):
    cdir.mkdir(parents=True, exist_ok=True)
    includes = ["-Isrc", "-Isrc/pygame/include", "-Itmp/gen3", "-I."]
    flags = ["-X", "profile=False", "-X", "embedsignature=True", "-X", "embedsignature.format=python"]

    def one(name):
        src = pyx_path(tree, name)
        if not src.exists():
            raise SystemExit(f"missing Cython source for {name}: {src}")
        out = cdir / (name + ".c")
        r = subprocess.run(
            ["cython", *includes, *flags, str(src.relative_to(tree)), "-o", str(out)],
            cwd=tree, text=True, capture_output=True,
        )
        if r.returncode:
            sys.stderr.write(r.stdout + r.stderr)
            raise SystemExit(f"cython failed for {name}")
        return name

    with concurrent.futures.ThreadPoolExecutor() as ex:
        for name in ex.map(one, sorted(mods)):
            log("cython", name)


def init_symbol(name: str) -> str:
    return "PyInit_" + name.replace(".", "_")


def compile_all(tree: Path, cdir: Path, odir: Path, mods, py_include: str):
    odir.mkdir(parents=True, exist_ok=True)
    cflags = [
        "-O2", "-DNDEBUG", "-std=gnu99", "-fno-strict-aliasing", "-w",
        "-I" + py_include, "-I" + str(tree / "src"), "-I" + str(tree / "tmp" / "gen3"),
        *pkg_config("--cflags", "freetype2", "harfbuzz", "sdl2"),
    ]
    jobs = []  # (source, object, extra flags)
    helper_seen = set()
    for name, helpers in mods.items():
        leaf = name.rsplit(".", 1)[1]
        jobs.append((cdir / (name + ".c"), odir / (name + ".o"), [f"-DPyInit_{leaf}={init_symbol(name)}"]))
        for h in helpers:
            if h not in helper_seen:
                helper_seen.add(h)
                jobs.append((tree / h, odir / (Path(h).stem + ".helper.o"), []))

    def one(job):
        src, obj, extra = job
        run(["cc", "-c", *cflags, *extra, str(src), "-o", str(obj)])
        return obj

    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as ex:
        objs = list(ex.map(one, jobs))
    lib = OUT / "libengine_cy.a"
    if lib.exists():
        lib.unlink()
    run(["ar", "rcs", str(lib), *map(str, objs)])
    log("archive", lib, f"{lib.stat().st_size // 1024} KiB", f"{len(objs)} objects")


# ----------------------------------------------------------------------------------------------
# Zips
# ----------------------------------------------------------------------------------------------

PYC_MAGIC = importlib.util.MAGIC_NUMBER


def add_pyc(z: zipfile.ZipFile, src: Path, arc: str):
    add_pyc_source(z, src.read_bytes(), arc)


def add_pyc_source(z: zipfile.ZipFile, source: bytes, arc: str):
    code = compile(source, arc, "exec", dont_inherit=True)
    # flags=1: unchecked hash-based pyc, so the importer never compares source timestamps.
    z.writestr(zipfile.ZipInfo(arc[:-3] + ".pyc", (1980, 1, 1, 0, 0, 0)),
               PYC_MAGIC + (1).to_bytes(4, "little") + b"\0" * 8 + marshal.dumps(code),
               compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)


def build_layer_zip(tree: Path, dest: Path):
    n = 0
    with zipfile.ZipFile(dest, "w") as z:
        for root in ("renpy", "_player"):
            for p in files_under(tree / root):
                rel = p.relative_to(tree).as_posix()
                if p.suffix != ".py" or rel.startswith(COMMON_DIR + "/"):
                    continue
                add_pyc(z, p, rel)
                n += 1
        # Bundled pure-Python packages (see fetch.sh).
        for wheel in sorted((UPSTREAM.parent / "pywheels").glob("*.whl")):
            with zipfile.ZipFile(wheel) as w:
                for name in sorted(w.namelist()):
                    if name.endswith(".py") and ".dist-info/" not in name:
                        add_pyc_source(z, w.read(name), name)
                        n += 1
                    elif name.endswith(".pem"):  # certifi's CA bundle, read through importlib.resources
                        z.writestr(zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0)), w.read(name),
                                   compress_type=zipfile.ZIP_DEFLATED)
    log("layer.zip", n, "modules", dest.stat().st_size, "bytes")


# ---- Build-time .rpyc for renpy/common (packaging slice) --------------------------------------
#
# The player binary does not exist while this script runs, so Ren'Py's parser runs in the host python3.12
# (the dev shell's) against the same patched tree, with the Cython modules rebuilt as shared libraries.
# packaging/compile_common.py does the work. See its docstring.

PACKAGING = PLAYER / "packaging"
HOST_SKIP = {"renpy.text.ftfont", "renpy.text.hbfont"}  # need FreeType and HarfBuzz; the parser does not


def build_host_tree(tree: Path, cdir: Path, mods, dest: Path):
    """dest/renpy = the patched renpy package plus every Cython module built as a host-loadable library."""
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(tree / "renpy", dest / "renpy", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "common"))
    host_include = sysconfig.get_paths()["include"]
    cflags = [
        "-O1", "-DNDEBUG", "-std=gnu99", "-fno-strict-aliasing", "-w", "-fPIC",
        "-I" + host_include, "-I" + str(tree / "src"), "-I" + str(tree / "tmp" / "gen3"),
        *pkg_config("--cflags", "sdl2"),
    ]
    ldflags = ["-bundle", "-undefined", "dynamic_lookup"] if sys.platform == "darwin" else ["-shared"]

    def one(item):
        name, helpers = item
        out = dest / (name.replace(".", "/") + ".so")
        run(["cc", *cflags, *ldflags, str(cdir / (name + ".c")), *[str(tree / h) for h in helpers], "-o", str(out)])

    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as ex:
        list(ex.map(one, [(n, h) for n, h in sorted(mods.items()) if n not in HOST_SKIP]))
    log("host modules", len(mods) - len(HOST_SKIP))


def compile_common(tree: Path, cdir: Path, mods) -> Path:
    """Compiles renpy/common to .rpyc. Returns the directory that holds them (rpyc/common/*.rpyc)."""
    work = OUT / "common-rpyc"
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    build_host_tree(tree, cdir, mods, work / "host")
    run([sys.executable, str(PACKAGING / "compile_common.py"),
         "--hosttree", str(work / "host"),
         "--common", str(tree / COMMON_DIR), "--out", str(work / "out"),
         "--workdir", str(work / "scratch"), "--pywheels", str(UPSTREAM.parent / "pywheels")])
    return work / "out" / "rpyc" / "common"


def build_common_zip(tree: Path, dest: Path, rpyc_dir: Path):
    n = 0
    base = tree / COMMON_DIR
    with zipfile.ZipFile(dest, "w") as z:
        # Marker: _player.boot finds this zip among the mounted zips by this entry.
        z.writestr(zipfile.ZipInfo(".renpy-common", (1980, 1, 1, 0, 0, 0)), b"renpy/common " + TAG.encode() + b"\n")
        for p in files_under(base):
            if p.suffix in (".rpyc", ".rpymc", ".pyc"):
                continue
            stored = p.suffix.lower() in STORED_EXT
            zi = zipfile.ZipInfo(p.relative_to(base).as_posix(), (1980, 1, 1, 0, 0, 0))
            z.writestr(zi, p.read_bytes(),
                       compress_type=zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED,
                       compresslevel=None if stored else 9)
            n += 1
        # The compiled scripts, at the zip root next to their sources: the loader prefers them.
        for p in sorted(rpyc_dir.rglob("*.rpy*c")):
            z.writestr(zipfile.ZipInfo(p.relative_to(rpyc_dir).as_posix(), (1980, 1, 1, 0, 0, 0)), p.read_bytes(),
                       compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
            n += 1
    log("common.zip", n, "files", dest.stat().st_size, "bytes")


# ----------------------------------------------------------------------------------------------


def native_link_info():
    """Native libraries the archive needs at final link time (ftfont and hbfont)."""
    lines = []
    for tok in pkg_config("--libs", "freetype2", "harfbuzz"):
        if tok.startswith("-L"):
            lines.append("search " + tok[2:])
        elif tok.startswith("-l"):
            lines.append("lib " + tok[2:])
    return lines


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--py-include", help="Python 3.12 headers (default: pyhost's build, else this python)")
    args = ap.parse_args()

    if not UPSTREAM.exists() or not (UPSTREAM.parent / "pywheels").is_dir():
        run(["bash", str(ENGINE / "fetch.sh")])

    py_include = args.py_include
    if not py_include:
        cand = PLAYER / "build-out" / "cpython" / "include" / "python3.12"
        py_include = str(cand) if cand.is_dir() else sysconfig.get_paths()["include"]
    log("python headers:", py_include)

    inputs = [p for root in (ENGINE / "patches", ENGINE / "python", ENGINE / "extra") for p in files_under(root)]
    inputs += [ENGINE / "build.py", ENGINE / "fetch.sh", PACKAGING / "compile_common.py"]
    fingerprint, stamp = digest_inputs(inputs)
    stamp += py_include
    stamp_file = OUT / "stamp.txt"
    needed = ["libengine_cy.a", "layer.zip", "common.zip", "inittab.txt", "fingerprint.txt", "link.txt"]
    if stamp_file.exists() and stamp_file.read_text() == stamp and all((OUT / n).exists() for n in needed):
        log("up to date, fingerprint", fingerprint)
        return

    OUT.mkdir(parents=True, exist_ok=True)
    stamp_file.unlink(missing_ok=True)
    tree = OUT / "tree"
    prepare_tree(tree)
    write_vc_version(tree)
    prefixes = generate_styles(tree)
    generate_pygame_constants(tree, OUT)

    (tree / "_player" / "build.py").write_text(
        f"# Written by player/engine/build.py.\nFINGERPRINT = {fingerprint!r}\nRENPY_TAG = {TAG!r}\n"
    )

    mods = module_list(tree, prefixes)
    cdir, odir = OUT / "c", OUT / "obj"
    for d in (cdir, odir):
        if d.exists():
            shutil.rmtree(d)
    cythonize(tree, cdir, mods)
    compile_all(tree, cdir, odir, mods, py_include)

    build_layer_zip(tree, OUT / "layer.zip")
    build_common_zip(tree, OUT / "common.zip", compile_common(tree, cdir, mods))

    (OUT / "inittab.txt").write_text("".join(f"{n} {init_symbol(n)}\n" for n in sorted(mods)))
    (OUT / "link.txt").write_text("\n".join(native_link_info()) + "\n")
    (OUT / "fingerprint.txt").write_text(fingerprint)
    stamp_file.write_text(stamp)
    log("done, fingerprint", fingerprint, f"{len(mods)} modules")


if __name__ == "__main__":
    main()
