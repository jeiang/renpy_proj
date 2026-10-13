#!/usr/bin/env python3
"""Build the Windows test kit zip for a non-developer tester.

Run on a developer machine with Python 3.11 or later (for tomllib), for example:

    nix shell nixpkgs#python3 -c python3 player/packaging/windows-test-kit/build-kit.py \
        --package /path/to/urp-windows-x86_64.zip --out /tmp/kit-out

The package input is either the CI artifact zip (urp-windows-x86_64.zip) or an
extracted folder holding urp.exe, its DLLs and a licenses/ folder.

The synthetic game folders must already be built with harness/testgames/build.py.
"""

import argparse
import datetime as dt
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
HARNESS = REPO / "harness"
KIT_NAME = "urp-windows-test-kit"

# Games with no main menu. The probe and save/resume stages need a menu, so
# these get only the route plan and the lint stage. Mirrors the corpus, where
# SynthAniso has no main menu.
NO_MENU = {"SynthAniso"}

# UHD is about 18 MB, so it is opt-in.
OPTIONAL = {"SynthUHD"}

# Top-level runtime files a previous run may leave in a game folder. Not shipped.
SKIP_TOP = {"log.txt", "traceback.txt", "saves", "cache"}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def git_commit():
    try:
        out = subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "--short=12", "HEAD"],
            capture_output=True, text=True, check=True,
        )
        return out.stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def checks_for(gid):
    """Engine checks the runner applies to a game's probe reports.

    py7-preflight: Ren'Py 7 build with a preflight report (all Synth7 games).
    py7-rewrites:  the full rewrite and syntax table (SynthRen'Py 7 base game only).
    patch:         the runtime patch was applied (Synth7Patch).
    engine-copy:   the engine-copy feature is reported as info (SynthObfEngine).
    """
    if gid == "Synth7":
        return ["py7-preflight", "py7-rewrites"]
    if gid == "Synth7Patch":
        return ["py7-preflight", "patch"]
    if gid.startswith("Synth7"):
        return ["py7-preflight"]
    if gid == "SynthObfEngine":
        return ["engine-copy"]
    return []


# The gate's command-line defaults for a game that sets no key in corpus.toml.
GATE_DEFAULTS = {"probe_lines": 60, "save_after": 30, "resume_lines": 20}


def load_games(include_optional):
    corpus = tomllib.loads((HARNESS / "corpus.toml").read_text())
    games = []
    for gid, g in corpus.get("games", {}).items():
        src = g.get("source", "")
        if not src.startswith("harness/testgames/build/"):
            continue
        if gid in OPTIONAL and not include_optional:
            continue
        games.append({
            "id": gid,
            "folder": Path(src).name,
            "source": REPO / src,
            "renpy": g.get("renpy", ""),
            "plan_src": g.get("plan"),
            "probe_lines": g.get("probe_lines", GATE_DEFAULTS["probe_lines"]),
            "save_after": g.get("save_after", GATE_DEFAULTS["save_after"]),
            "resume_lines": g.get("resume_lines", GATE_DEFAULTS["resume_lines"]),
            "stages": g.get("stages", {}),
            "after_start": g.get("after_start", []),
        })
    games.sort(key=lambda x: x["id"])
    return games


def copy_game(src, dst):
    def ignore(directory, names):
        out = []
        at_top = Path(directory) == src
        for n in names:
            if n == "__pycache__" or n.endswith(".pyc"):
                out.append(n)
            elif at_top and n in SKIP_TOP:
                out.append(n)
        return out

    shutil.copytree(src, dst, ignore=ignore)


def keep_package_member(rel):
    if rel == "urp.exe" or rel.startswith("licenses/"):
        return True
    return "/" not in rel and rel.lower().endswith(".dll")


def copy_package(src, dst):
    """Copy urp.exe, its DLLs and licenses/ from a CI zip or an extracted folder."""
    dst.mkdir(parents=True, exist_ok=True)
    picked = []
    if src.is_file() and src.suffix == ".zip":
        with zipfile.ZipFile(src) as z:
            for info in z.infolist():
                if info.is_dir():
                    continue
                parts = info.filename.split("/")
                rel = "/".join(parts[1:]) if len(parts) > 1 else parts[0]
                if not keep_package_member(rel):
                    continue
                target = dst / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                with z.open(info) as s, open(target, "wb") as d:
                    shutil.copyfileobj(s, d)
                picked.append(rel)
    elif src.is_dir():
        for p in sorted(src.rglob("*")):
            if not p.is_file():
                continue
            rel = p.relative_to(src).as_posix()
            if not keep_package_member(rel):
                continue
            target = dst / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(p, target)
            picked.append(rel)
    else:
        sys.exit(f"--package must be a CI zip or a folder: {src}")
    if "urp.exe" not in picked:
        sys.exit("package has no urp.exe")
    return picked


def ofl_text():
    """Return the SIL OFL 1.1 text from the repo's dependency licence summary."""
    doc = (REPO / "player" / "packaging" / "licences" / "RUST_DEPENDENCIES.md").read_text()
    m = re.search(r"### SIL Open Font License 1\.1.*?```text\n(.*?)```", doc, re.S)
    if not m:
        sys.exit("OFL 1.1 text not found in RUST_DEPENDENCIES.md")
    return m.group(1)


def write_sha256sums(root, out_path):
    lines = []
    for p in sorted(root.rglob("*")):
        if p.is_file() and p != out_path:
            lines.append(f"{sha256(p)}  {p.relative_to(root).as_posix()}")
    out_path.write_text("\n".join(lines) + "\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--package", required=True, type=Path,
                    help="CI zip (urp-windows-x86_64.zip) or extracted folder")
    ap.add_argument("--out", required=True, type=Path,
                    help="folder to write the kit zip into")
    ap.add_argument("--version", default=None,
                    help="kit version label (default: dev-<commit>)")
    ap.add_argument("--include-uhd", action="store_true",
                    help="include SynthUHD (about 18 MB)")
    args = ap.parse_args()

    if sys.version_info < (3, 11):
        sys.exit("needs Python 3.11+ for tomllib")

    games = load_games(args.include_uhd)
    if not games:
        sys.exit("no games found; run harness/testgames/build.py first")
    for g in games:
        if not g["source"].is_dir():
            sys.exit(f"missing built game {g['source']}; run harness/testgames/build.py")

    commit = git_commit()
    version = args.version or f"dev-{commit}"
    now = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    ofl = ofl_text()

    args.out.mkdir(parents=True, exist_ok=True)
    zip_path = args.out / f"{KIT_NAME}-{version}.zip"

    with tempfile.TemporaryDirectory(prefix="winkit-") as tmp:
        stage = Path(tmp)
        root = stage / KIT_NAME
        root.mkdir()

        # Program and its runtime DLLs.
        package_files = copy_package(args.package, root / "urp")
        exe = root / "urp" / "urp.exe"

        # Games. A game's base folder is the folder that contains game/.
        for g in games:
            copy_game(g["source"], root / "games" / g["folder"])

        # Harness script and plans.
        (root / "harness").mkdir()
        shutil.copyfile(HARNESS / "rpy" / "zz_harness.rpy",
                        root / "harness" / "zz_harness.rpy")
        plans = root / "plans"
        plans.mkdir()
        shutil.copyfile(HARNESS / "plans" / "probe.plan.tmpl",
                        plans / "probe.plan.tmpl")
        shutil.copyfile(HARNESS / "plans" / "route.plan", plans / "route.plan")

        # Patch seed for the Synth7Patch port-patch check. The runner fills in the node hash at test time.
        seed = root / "seed"
        seed.mkdir()
        shutil.copyfile(HARNESS / "testgames" / "py7patch" / "patches" / "patch.toml",
                        seed / "py7patch-patch.toml")

        games_json = []
        for g in games:
            has_menu = g["id"] not in NO_MENU
            plan = None
            if g["plan_src"] == "route":
                plan = "plans/route.plan"
            elif g["plan_src"]:
                plan = f"plans/{g['folder']}.plan"
                shutil.copyfile(HARNESS / g["plan_src"], root / plan)
            games_json.append({
                "id": g["id"],
                "folder": g["folder"],
                "renpy": g["renpy"],
                "menu": has_menu,
                "plan": plan,
                "probe_lines": g["probe_lines"] if has_menu else None,
                "save_after": g["save_after"] if has_menu else None,
                "resume_lines": g["resume_lines"] if has_menu else None,
                "stages": g["stages"],
                "after_start": g["after_start"],
                "checks": checks_for(g["id"]),
            })
        (root / "games.json").write_text(
            json.dumps({"games": games_json}, indent=2) + "\n")

        # Runner, launcher, tester README.
        # The launcher and the tester README get CRLF endings, so cmd.exe and Notepad show them correctly.
        # The runner keeps LF endings: Windows PowerShell 5.1 reads both.
        for name in ("run-tests.ps1", "run-tests.bat", "README-TESTER.txt"):
            data = (HERE / name).read_bytes()
            if name != "run-tests.ps1":
                data = data.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
            (root / name).write_bytes(data)

        # Licences: the program's own set, plus the OFL text for the bundled fonts.
        lic = root / "licenses"
        lic.mkdir()
        pkg_lic = root / "urp" / "licenses"
        if pkg_lic.is_dir():
            for p in sorted(pkg_lic.rglob("*")):
                if p.is_file():
                    shutil.copyfile(p, lic / f"urp-{p.name}")
        (lic / "OFL-1.1-fonts.txt").write_text(
            "The text test game bundles Open Sans and Noto Naskh Arabic (SIL OFL 1.1).\n"
            "Their copyright notices are in the font files.\n\n" + ofl)

        manifest = [
            f"kit: {KIT_NAME}",
            f"version: {version}",
            f"built_utc: {now}",
            f"repo_commit: {commit}",
            f"urp.exe sha256: {sha256(exe)}",
            f"urp.exe bytes: {exe.stat().st_size}",
            f"package files: {len(package_files)}",
            f"include_uhd: {'yes' if args.include_uhd else 'no'}",
            "seed: seed/py7patch-patch.toml (Synth7Patch port patch; the runner fills in its node hash)",
            "games:",
        ]
        for g in games_json:
            manifest.append(f"  {g['id']} folder={g['folder']} renpy={g['renpy']}")
        (root / "manifest.txt").write_text("\n".join(manifest) + "\n")

        write_sha256sums(root, root / "SHA256SUMS.txt")

        with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
            for p in sorted(root.rglob("*")):
                if p.is_file():
                    z.write(p, p.relative_to(stage).as_posix())

    size_mb = zip_path.stat().st_size / (1024 * 1024)
    print(f"wrote {zip_path} ({size_mb:.1f} MB), {len(games)} games")


if __name__ == "__main__":
    main()
