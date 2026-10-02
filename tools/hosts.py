#!/usr/bin/env python3
"""Reads the machine-specific hosts from local/hosts.toml (gitignored; template: local/hosts.example.toml).

Needs Python 3.11 or later (the nix dev shell has it).

Python use (the repository root must be on sys.path, or load this file by path):

    from tools.hosts import host
    ssh = host("artemis", "ssh_target")

Shell use:

    ssh "$(python3 tools/hosts.py artemis.ssh_target)" uptime

A missing file or key stops with a message that names the key and the example file. The path of the
file can be changed with the environment variable RENPY_PROJ_HOSTS.
"""
from __future__ import annotations

import os
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_PATH = REPO / "local" / "hosts.toml"


def hosts_path() -> Path:
    return Path(os.environ.get("RENPY_PROJ_HOSTS", DEFAULT_PATH))


def load() -> dict:
    p = hosts_path()
    if not p.is_file():
        raise SystemExit(f"hosts: {p} not found. Copy local/hosts.example.toml to local/hosts.toml and fill in your values.")
    with p.open("rb") as f:
        return tomllib.load(f)


def host(section: str, key: str) -> str:
    """The string value of [section] key. Stops with a message when it is missing."""
    try:
        v = load()[section][key]
    except KeyError:
        raise SystemExit(f"hosts: [{section}] {key} is not set in {hosts_path()} (see local/hosts.example.toml)") from None
    if not isinstance(v, str) or not v:
        raise SystemExit(f"hosts: [{section}] {key} in {hosts_path()} must be a non-empty string")
    return v


if __name__ == "__main__":
    if len(sys.argv) != 2 or "." not in sys.argv[1]:
        raise SystemExit("usage: hosts.py <section>.<key>   (for example artemis.ssh_target)")
    sec, key = sys.argv[1].split(".", 1)
    print(host(sec, key))
