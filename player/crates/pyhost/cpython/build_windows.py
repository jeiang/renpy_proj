#!/usr/bin/env python3
"""Static CPython 3.12.8 for the player on Windows x64 (MSVC, /MT). Run by crates/pyhost/build.rs.

Usage: python -X utf8 build_windows.py <player-dir>

Follows research/win-spike (variant b): the python.org source is compiled with plain `cl /MT
/DPy_NO_ENABLE_SHARED`, file lists are read from the PCbuild .vcxproj files, and every extension module
is built in. The result needs no VCRUNTIME140.dll and no python312.dll.

    source  -> <player>/upstream/src/Python-3.12.8        (fetched here, SHA-256 checked)
    scratch -> <player>/upstream/cpython-build            (objects, logs)
    output  -> <player>/build-out/cpython/{lib,deps,include/python3.12,boot,stdlib.zip,smoke.zip,stamp}

Needs: Visual Studio 2022 Build Tools (found with vswhere when `cl` is not on PATH), the Windows SDK, the
host Python 3.12 that runs this script (PCbuild's find_python uses it), and vcpkg for the static libffi and
OpenSSL (`PLAYER_VCPKG`, default C:\\spike\\vcpkg; the triplet x64-windows-static is installed when missing).
"""

import hashlib
import os
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
PYVER = "3.12.8"
PYURL = f"https://www.python.org/ftp/python/{PYVER}/Python-{PYVER}.tar.xz"
PYSHA = "c909157bb25ec114e5869124cc2a9c4a4d4c1e957ca4ff553f1edc692101154e"
TRIPLET = "x64-windows-static"

# Extension modules that become builtin (win-spike section 3.3, plus _lzma). Names are PCbuild project files.
EXT = ["_socket", "select", "pyexpat", "_elementtree", "_asyncio", "_queue", "_zoneinfo", "_uuid",
       "_multiprocessing", "_overlapped", "_decimal", "_ctypes", "_bz2", "_lzma", "_ssl", "_hashlib"]
# Sources that live outside the project file of the module that needs them.
EXTRA_PROJECTS = {"_lzma": ["liblzma"]}
SKIP_SOURCES = {"applink.c"}


def log(*a):
    print("[pyhost-win]", *a, flush=True)


def run(cmd, **kw):
    r = subprocess.run(cmd, text=True, **kw)
    if r.returncode:
        raise SystemExit(f"command failed ({r.returncode}): {cmd}")


def msvc_env():
    """The environment with the MSVC x64 tools on PATH (vcvars64.bat), when `cl` is not there yet."""
    if shutil.which("cl") and os.environ.get("VSCMD_ARG_TGT_ARCH") == "x64":
        return dict(os.environ)
    vswhere = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
    root = subprocess.check_output([str(vswhere), "-latest", "-products", "*", "-property", "installationPath"], text=True).strip()
    if not root:
        raise SystemExit("pyhost: Visual Studio 2022 Build Tools not found")
    out = subprocess.check_output(f'cmd /c ""{root}\\VC\\Auxiliary\\Build\\vcvars64.bat" >nul && set"', text=True, shell=True)
    env = dict(l.split("=", 1) for l in out.splitlines() if "=" in l)
    return env


def fetch(player: Path):
    up = player / "upstream"
    src = up / "src" / f"Python-{PYVER}"
    if not src.exists():
        (up / "src").mkdir(parents=True, exist_ok=True)
        tar = up / f"Python-{PYVER}.tar.xz"
        log("fetching CPython", PYVER)
        urllib.request.urlretrieve(PYURL, tar)
        got = hashlib.sha256(tar.read_bytes()).hexdigest()
        if got != PYSHA:
            tar.unlink()
            raise SystemExit(f"pyhost: checksum mismatch for Python-{PYVER}.tar.xz: {got}")
        with tarfile.open(tar) as t:
            t.extractall(up / "src", filter="data")
    return src


def externals_and_headers(src: Path, env, scratch: Path):
    """zlib, bzip2, xz (CPython's own source-deps) and the generated frozen headers."""
    if not (src / "externals" / "zlib-1.3.1").exists():
        log("get_externals")
        run(["cmd", "/c", str(src / "PCbuild" / "get_externals.bat"), "--no-tkinter", "--no-openssl", "--no-libffi"], cwd=src, env=env)
    if not (src / "Python" / "deepfreeze" / "deepfreeze.c").exists():
        # The tarball has no frozen headers or deepfreeze.c; the stock build generates them. `pythoncore`
        # alone fails on getpath.h, so _freeze_module runs first.
        log("msbuild _freeze_module + pythoncore (generates the frozen headers)")
        for proj in ("_freeze_module", "pythoncore"):
            run(["msbuild", f"PCbuild\\{proj}.vcxproj", "/m", "/p:Configuration=Release", "/p:Platform=x64", "/v:m"], cwd=src, env=env)


def vcpkg_dir(env):
    root = Path(os.environ.get("PLAYER_VCPKG", r"C:\spike\vcpkg"))
    inst = root / "installed" / TRIPLET
    if not (inst / "lib" / "libssl.lib").exists() or not (inst / "lib" / "ffi.lib").exists():
        log("vcpkg install libffi openssl", TRIPLET)
        run([str(root / "vcpkg.exe"), "install", f"libffi:{TRIPLET}", f"openssl:{TRIPLET}"], env=env)
    return inst


def vcxproj_sources(src: Path, name: str):
    text = (src / "PCbuild" / f"{name}.vcxproj").read_text(encoding="utf-8")
    out = []
    for m in re.finditer(r'<ClCompile Include="([^"]+)"', text):
        p = m.group(1)
        p = p.replace("$(zlibDir)", "externals/zlib-1.3.1").replace("$(bz2Dir)", "externals/bzip2-1.0.8") \
             .replace("$(lzmaDir)", "externals/xz-5.2.5/")
        if "$(" in p:
            continue
        p = p.replace("\\", "/")
        full = (src / "PCbuild" / p).resolve() if p.startswith("..") else (src / p).resolve()
        if full.name in SKIP_SOURCES or full.suffix != ".c":
            continue
        out.append(full)
    return out


def sources(src: Path):
    jobs = []  # (group, path)
    seen = set()
    for grp, projs in [("core", ["pythoncore"])] + [(m, [m, *EXTRA_PROJECTS.get(m, [])]) for m in EXT]:
        for proj in projs:
            for p in vcxproj_sources(src, proj):
                if p in seen:
                    continue  # pyexpat and _elementtree both list the expat sources
                seen.add(p)
                jobs.append((grp, p))
    return jobs


def compile_all(src: Path, env, obj: Path, vcpkg: Path, jobs):
    obj.mkdir(parents=True, exist_ok=True)
    inc = ["Include", "Include/internal", "PC", "Python", "Modules/expat", "Modules", "externals/bzip2-1.0.8",
           "Modules/_hacl/include", "externals/zlib-1.3.1", "Modules/_hacl/internal"]
    flags = ["/nologo", "/c", "/O2", "/MT", "/utf-8", "/w", "/DNDEBUG", "/DWIN32", "/D_WINDOWS", "/DPy_NO_ENABLE_SHARED",
             "/D_CRT_SECURE_NO_WARNINGS", "/DUSE_PYEXPAT_CAPI", "/DXML_STATIC", "/DHAVE_EXPAT_CONFIG_H", "/DPy_BUILD_CORE",
             "/D_Py_HAVE_ZLIB", "/DUSE_ZLIB_CRC32", '/DMS_DLL_ID="3.12"', '/DPY3_DLLNAME=L"python3"'] + \
            ["/I" + str(src / i) for i in inc]
    lzma = src / "externals" / "xz-5.2.5"

    def build(job):
        grp, p = job
        n = p.stem
        o = obj / f"{grp}__{n}.obj"
        ex = ["/DPy_BUILD_CORE_BUILTIN"] if grp != "core" else []
        if grp in ("_ssl", "_hashlib", "_ctypes"):
            ex += ["/I" + str(vcpkg / "include")]
        if grp == "_ctypes":
            ex += ["/DFFI_STATIC_BUILD"]
        if grp == "_decimal":
            ex += ["/DCONFIG_64=1", "/DANSI=1", "/I" + str(src / "Modules/_decimal/libmpdec")]
        if grp == "_lzma":
            ex += ["/DLZMA_API_STATIC", "/DHAVE_CONFIG_H", "/D_FILE_OFFSET_BITS=64", "/I" + str(lzma / "windows/vs2019"),
                   *["/I" + str(lzma / "src" / d) for d in ("liblzma/api", "liblzma/common", "common", "liblzma/check",
                                                           "liblzma/delta", "liblzma/lz", "liblzma/lzma",
                                                           "liblzma/rangecoder", "liblzma/simple")]]
        if n == "getpath":
            ex += ["/DPREFIX=NULL", "/DEXEC_PREFIX=NULL", "/DVERSION=NULL", '/DVPATH="..\\\\.."', '/DPYDEBUGEXT=""', '/DPLATLIBDIR="DLLs"']
        if n == "sysmodule":
            ex += ['/DVPATH="..\\\\.."']
        r = subprocess.run(["cl", *flags, *ex, str(p), "/Fo" + str(o)], capture_output=True, text=True, env=env)
        return job, r.returncode, r.stdout + r.stderr

    with ThreadPoolExecutor(os.cpu_count() or 4) as ex:
        res = list(ex.map(build, jobs))
    bad = [(j, out) for j, rc, out in res if rc]
    for (grp, p), out in bad:
        log("FAIL", grp, p.name, [l for l in out.splitlines() if "error" in l][:2])
    if bad:
        raise SystemExit(f"pyhost: {len(bad)} sources failed to compile")
    log("compiled", len(res), "sources")


def main():
    player = Path(sys.argv[1]).resolve()
    out = player / "build-out" / "cpython"
    scratch = player / "upstream" / "cpython-build-win"
    scratch.mkdir(parents=True, exist_ok=True)
    env = msvc_env()
    env["PYTHONUTF8"] = "1"
    env["PYTHON"] = sys.executable  # PCbuild's find_python.bat uses it before `py -3.12` (not registered for every user)
    os.environ.update(env)  # cl, lib, msbuild resolve through PATH of this process
    src = fetch(player)
    vcpkg = vcpkg_dir(env)
    externals_and_headers(src, env, scratch)

    jobs = sources(src)
    recipe = hashlib.sha256(("\n".join(f"{g} {p}" for g, p in jobs) + Path(__file__).read_text()).encode()).hexdigest()
    lib = scratch / "python312.lib"
    stale = not lib.exists() or not (scratch / "recipe").exists() or (scratch / "recipe").read_text() != recipe
    if stale:
        obj = scratch / "obj"
        shutil.rmtree(obj, ignore_errors=True)
        compile_all(src, env, obj, vcpkg, jobs)
        rsp = scratch / "objs.rsp"
        rsp.write_text("\n".join(f'"{o}"' for o in sorted(obj.glob("*.obj"))))
        lib.unlink(missing_ok=True)
        run(["lib", "/nologo", f"/OUT:{lib}", f"@{rsp}"], env=env)
        (scratch / "recipe").write_text(recipe)

    # ---- outputs ----
    for d in ("lib", "deps", "boot"):
        (out / d).mkdir(parents=True, exist_ok=True)
    shutil.copy2(lib, out / "lib" / "python312.lib")
    for f in ("ffi.lib", "libssl.lib", "libcrypto.lib"):
        shutil.copy2(vcpkg / "lib" / f, out / "deps" / f)
    inc = out / "include" / "python3.12"
    shutil.rmtree(inc, ignore_errors=True)
    shutil.copytree(src / "Include", inc, ignore=shutil.ignore_patterns("*.py"))
    shutil.copy2(src / "PC" / "pyconfig.h", inc / "pyconfig.h")

    py = sys.executable
    lib_src = str(src / "Lib")
    run([py, str(HERE / "mkboot.py"), lib_src, str(out / "boot")])
    run([py, str(HERE / "mkzip.py"), lib_src, str(out / "stdlib.zip")])
    empty = scratch / "empty-lib"
    empty.mkdir(exist_ok=True)
    smoke = HERE.parent / "examples" / "smokepkg"
    run([py, str(HERE / "mkzip.py"), str(empty), str(out / "smoke.zip"), str(smoke)])
    run([py, "-c", "import zipfile,sys\nz=zipfile.ZipFile(sys.argv[1],'a',zipfile.ZIP_DEFLATED); z.write('smokepkg/data.txt'); z.close()",
         str(out / "smoke.zip")], cwd=HERE.parent / "examples")
    (out / "stamp").write_text(recipe)
    log("CPython", PYVER, "ready in", out)


if __name__ == "__main__":
    main()
