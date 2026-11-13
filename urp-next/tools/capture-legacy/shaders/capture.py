#!/usr/bin/env python3
"""Capture today's legacy shader inputs and translator outputs without Cargo.

python3 capture.py --repo /path/to/todays/repo --out /path/to/fixtures/shaders \
    --renpy-source /path/to/renpy-8.5.3-sdk

Requires Python 3 and rustc on PATH. --repo supplies both extract_shaders.py and
player/crates/gfx/src/translate.rs; it is never modified. The optional source
argument defaults to the source tree populated by player/engine/fetch.sh.
Output must be absent or empty. Extraction/translation failure leaves no partial
fixture set. The copied Ren'Py sources and license carry MIT attribution; stage
files remain byte-for-byte extractor/translator outputs, without added headers.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def capture(args):
    repo = args.repo.resolve(strict=True)
    source = (args.renpy_source or repo / "player/upstream/renpy-8.5.3").resolve(strict=True)
    out = args.out.absolute()
    if out.exists() and (not out.is_dir() or any(out.iterdir())):
        raise RuntimeError(f"output must be absent or empty: {out}")
    extractor = repo / "research/renderer/extract_shaders.py"
    translator = repo / "player/crates/gfx/src/translate.rs"
    wrapper = Path(__file__).with_name("translate-wrapper.rs")
    for path in (extractor, translator, wrapper):
        if not path.is_file():
            raise FileNotFoundError(path)
    license_path = next((source / name for name in ("LICENSE.txt", "LICENSE")
                         if (source / name).is_file()), None)
    if license_path is None:
        raise FileNotFoundError(f"Ren'Py source license missing in {source}")
    license_text = license_path.read_text(encoding="utf-8-sig")
    if "Permission is hereby granted, free of charge" not in license_text:
        raise RuntimeError(f"Ren'Py license lacks expected MIT grant: {license_path}")

    out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".shader-capture-", dir=out.parent) as temp:
        work = Path(temp)
        stage = work / "capture"
        original = stage / "original"
        translated = stage / "translated"
        translated.mkdir(parents=True)
        previous_argv = sys.argv
        try:
            sys.argv = [str(extractor), str(source), str(original)]
            extracted = runpy.run_path(str(extractor), run_name="__main__")
        finally:
            sys.argv = previous_argv
        combos = extracted["combos"]
        parts = extracted["parts"]
        if len(combos) != 25:
            raise RuntimeError(f"expected 25 legacy combinations / 50 programs, got {len(combos)} combinations")
        if len(list(original.glob("*.vert"))) != 50 or len(list(original.glob("*.frag"))) != 50:
            raise RuntimeError("extractor did not emit exactly 50 vertex/fragment program pairs")

        # Copy registrations verbatim as well as their parsed, unexpanded kwargs.
        # This preserves source-part whitespace, priorities, functions and notices.
        sources = {}
        for relative in extracted["FILES"]:
            path = source / relative
            destination = stage / "sources" / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
            sources[relative] = {
                "file": destination.relative_to(stage).as_posix(),
                "sha256": sha256(path),
            }
        shutil.copyfile(license_path, stage / "RENPY-LICENSE.txt")
        write_json(stage / "parts.json", parts)

        # A path attribute (rather than include!) preserves translate.rs's inner
        # module documentation. Only this temporary wrapper is substituted.
        wrapper_source = wrapper.read_text(encoding="utf-8")
        if wrapper_source.count("__TRANSLATOR_PATH__") != 1:
            raise RuntimeError("translator wrapper must contain exactly one path marker")
        # Rust accepts JSON quoting for ordinary path characters; reject the few
        # control characters JSON would encode with Rust-incompatible escapes.
        translator_path = str(translator)
        if any(ord(character) < 32 for character in translator_path):
            raise RuntimeError("translator path contains a control character")
        (work / "wrapper.rs").write_text(
            wrapper_source.replace("__TRANSLATOR_PATH__", json.dumps(translator_path, ensure_ascii=False)),
            encoding="utf-8",
        )
        binary = work / "translate-shaders"
        subprocess.run(["rustc", "--edition=2024", str(work / "wrapper.rs"), "-o", str(binary)], check=True)
        rustc_version = subprocess.run(["rustc", "--version"], check=True, text=True, capture_output=True).stdout.strip()
        programs = []
        for name, names in sorted(combos.items()):
            for tag, language in (("120", "GLSL120"), ("100", "ESSL100")):
                key = f"{name}.{tag}"
                paths = [original / f"{key}.vert", original / f"{key}.frag",
                         translated / f"{key}.vert", translated / f"{key}.frag"]
                subprocess.run([str(binary), *map(str, paths)], check=True)
                stages = {}
                for suffix, stage_name in (("vert", "vertex"), ("frag", "fragment")):
                    stages[stage_name] = {}
                    for kind, directory in (("original", original), ("translated", translated)):
                        path = directory / f"{key}.{suffix}"
                        stages[stage_name][kind] = {
                            "file": path.relative_to(stage).as_posix(), "sha256": sha256(path),
                        }
                programs.append({"id": key, "combination": name, "language": language,
                                 "parts": names, "stages": stages})
        manifest = {
            "schema_version": 1,
            "capture": "legacy built-in shader extraction and current player translation",
            "program_count": len(programs),
            "combination_count": len(combos),
            "part_count": len(parts),
            "provenance": {
                "extractor": {"path": "research/renderer/extract_shaders.py", "sha256": sha256(extractor)},
                "translator": {"path": "player/crates/gfx/src/translate.rs", "sha256": sha256(translator)},
                "wrapper": {"file": "translate-wrapper.rs", "sha256": sha256(wrapper)},
                "rustc": rustc_version,
                "renpy_sources": sources,
                "renpy_license": {"file": "RENPY-LICENSE.txt", "source": license_path.name,
                                  "sha256": sha256(license_path), "license": "MIT"},
                "parts": {"file": "parts.json", "sha256": sha256(stage / "parts.json"),
                          "representation": "original extractor kwargs, unexpanded; _file identifies source"},
            },
            "programs": programs,
        }
        shutil.copyfile(wrapper, stage / "translate-wrapper.rs")
        write_json(stage / "manifest.json", manifest)
        os.replace(stage, out)
    print(f"Captured {len(programs)} programs (100 original + 100 translated stages) in {out}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", type=Path, required=True, help="today's legacy repository root")
    parser.add_argument("--out", type=Path, required=True, help="absent or empty fixture output directory")
    parser.add_argument("--renpy-source", type=Path, help="Ren'Py source/SDK root containing renpy/ and LICENSE.txt")
    args = parser.parse_args()
    try:
        capture(args)
    except Exception as error:
        parser.exit(1, f"shader capture failed: {error}\n")


if __name__ == "__main__":
    main()
