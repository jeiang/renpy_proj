#!/usr/bin/env python3
"""Hash the renpy/ engine tree of a stock Ren'Py SDK, streamed from renpy.org.

Usage: stock_manifest.py VERSION OUT.json

Writes {"renpy-<VERSION>-sdk/renpy/<rel>": {"sha256": ..., "size": ...}}, the same shape as the
renpy.json files written by archive_survey.py. Nothing is written to disk except OUT.json.
The SDK is streamed, so there is no temporary copy.
"""
import hashlib
import json
import sys
import tarfile
import urllib.request


def main():
    version, out = sys.argv[1], sys.argv[2]
    # Nightly SDKs live in a "<version>+nightly" directory, with '+' URL-encoded.
    url_dir = version.replace("+", "%2B")
    url = f"https://www.renpy.org/dl/{url_dir}/renpy-{version}-sdk.tar.bz2"
    manifest = {}
    with urllib.request.urlopen(url, timeout=120) as resp:
        with tarfile.open(fileobj=resp, mode="r|bz2") as tf:
            for m in tf:
                if not m.isfile():
                    continue
                parts = m.name.split("/", 1)
                if len(parts) != 2 or not parts[1].startswith("renpy/"):
                    continue
                h = hashlib.sha256()
                f = tf.extractfile(m)
                while True:
                    chunk = f.read(1 << 20)
                    if not chunk:
                        break
                    h.update(chunk)
                manifest[m.name] = {"sha256": h.hexdigest(), "size": m.size}
    if not manifest:
        sys.exit(f"stock_manifest: no renpy/ files found in {url}")
    with open(out, "w") as fh:
        json.dump(manifest, fh, sort_keys=True)
    print(f"{version}: {len(manifest)} renpy/ files -> {out}")


if __name__ == "__main__":
    main()
