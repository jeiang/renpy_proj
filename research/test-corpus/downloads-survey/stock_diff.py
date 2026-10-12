#!/usr/bin/env python3
"""Compare each game's bundled renpy/ tree with the stock SDK of the same version.

Usage: stock_diff.py SURVEY_OUT_DIR STOCK_DIR ARCHIVE_LIST

- SURVEY_OUT_DIR holds <stem>.renpy.json (archive_survey.py output).
- STOCK_DIR holds <version>.json (stock_manifest.py output), one per version.
- ARCHIVE_LIST is archives.txt (lines: "Played|name" or "Unplayed|name").

Prints one TSV row per game: game, engine, status, game_files, stock_files,
game_only, stock_only, sha_differ_src, sha_differ_bytecode, and the first
source sha-differing paths (relative to renpy/). Bytecode is .pyc, .pyo,
.rpyc, .rpymc, or anything under __pycache__. Games whose engine version has
no stock manifest are reported with status "no-stock".
"""

import json
import os
import sys

from aggregate import version_from_tuple

BYTECODE_SUFFIXES = (".pyc", ".pyo", ".rpyc", ".rpymc")


def is_bytecode(rel: str) -> bool:
    return "__pycache__" in rel or rel.endswith(BYTECODE_SUFFIXES)


def rel_renpy(key: str):
    """Return the path after the first '/renpy/' component, or None."""
    marker = "/renpy/"
    i = key.find(marker)
    if i < 0:
        return None
    return key[i + len(marker):]


def load_tree(path: str):
    with open(path) as f:
        raw = json.load(f)
    out = {}
    for k, v in raw.items():
        rel = rel_renpy(k)
        if rel is not None:
            out[rel] = v["sha256"]
    return out


def main(argv):
    if len(argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    out_dir, stock_dir, archive_list = argv[1], argv[2], argv[3]

    games = []
    with open(archive_list) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            _, name = line.split("|", 1)
            games.append(name)

    print("\t".join([
        "game", "engine", "status", "game_files", "stock_files",
        "game_only", "stock_only", "sha_differ_src", "sha_differ_bytecode", "sha_src_paths",
    ]))
    for name in games:
        stem = name
        for suffix in (".tar.zst",):
            if stem.endswith(suffix):
                stem = stem[: -len(suffix)]
        renpy_json = os.path.join(out_dir, stem + ".renpy.json")
        game_json = os.path.join(out_dir, stem + ".json")
        if not os.path.exists(renpy_json) or not os.path.exists(game_json):
            print("\t".join([stem, "", "no-survey", "", "", "", "", "", "", ""]))
            continue
        with open(game_json) as f:
            meta = json.load(f)
        engine = (
            version_from_tuple(meta.get("renpy_version_tuple"), parts_max=3)
            or version_from_tuple(meta.get("renpy_vc_version"), parts_max=3)
            or ""
        )
        stock_path = os.path.join(stock_dir, engine + ".json") if engine else ""
        game = load_tree(renpy_json)
        if not engine or not os.path.exists(stock_path):
            print("\t".join([stem, engine, "no-stock", str(len(game)), "", "", "", "", "", ""]))
            continue
        stock = load_tree(stock_path)
        game_only = [p for p in game if p not in stock]
        stock_only = [p for p in stock if p not in game]
        both = [p for p in game if p in stock]
        sha_src = sorted(p for p in both if game[p] != stock[p] and not is_bytecode(p))
        sha_byte = sum(1 for p in both if game[p] != stock[p] and is_bytecode(p))
        print("\t".join([
            stem, engine, "ok", str(len(game)), str(len(stock)),
            str(len(game_only)), str(len(stock_only)), str(len(sha_src)),
            str(sha_byte), ";".join(sha_src[:5]),
        ]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
