# Downloaded archive survey (ticket #18)

This survey covers 50 game archives (`.tar.zst`) that the user placed on artemis. It checks each one for engine version, script format, RPA archive versions, movie codecs and resolutions, and obfuscation signals. The goal is to find test candidates that fill the gaps in the [corpus](../README.md): Ren'Py 8.4 and later, 4K video, obfuscated games, and modified engines.

Nothing was extracted in full. Each archive was streamed. Only small metadata files were unpacked, and movies were probed without writing them out in full. The archives were never written to.

Results: [survey-downloads.tsv](survey-downloads.tsv) (one row per archive, 18 columns) and [stock-diff.tsv](stock-diff.tsv) (one row per game, compared with the official SDK of the same engine version).

## Files

| File | Role |
|---|---|
| `archive_survey.py` | Streams one archive. Writes `<stem>.json` (metadata and signals), `<stem>.renpy.json` (hashes of the bundled `renpy/` tree), `<stem>.rpa.json` (RPA headers), and `<stem>.movies.json` (movie probes). Usage: `archive_survey.py ARCHIVE OUTDIR [SCRATCH]` |
| `run_survey.sh` | Runs `archive_survey.py` over the Played and Unplayed lists, two archives at a time, with one scratch directory per archive |
| `stock_manifest.py` | Streams an official SDK and hashes its `renpy/` tree. Usage: `stock_manifest.py VERSION OUT.json`. Nightly builds use a version such as `7.8.8.25051901+nightly` |
| `stock_diff.py` | Compares each game's `renpy/` tree with the stock manifest for its engine version. Usage: `stock_diff.py SURVEY_OUT_DIR STOCK_DIR ARCHIVE_LIST` |
| `aggregate.py` | Builds `survey-downloads.tsv` from the per-archive JSON. Usage: `aggregate.py OUTDIR ARCHIVE_LIST TSV_OUT [CORRUPT_LIST]` |

## Method

1. **Metadata pass.** `archive_survey.py` reads the top-level `game/`, `lib/`, and `renpy/` layout. It takes the engine version from `renpy_version_tuple`, or from `renpy_vc_version` when the tuple is absent (the tuple is missing in about half the games). It takes the script version from `script_version.txt` or the `.rpyc` header.
2. **RPA pass.** RPA headers are read from every `.rpa` file, and the version and key are recorded. Entries are walked from the index. A walk bug (an entry was read twice and the next entry was overrun) was fixed before the final run. Every archive was re-run after the fix.
3. **Movie pass.** Each movie is probed with `ffprobe` on an 8 MiB prefix. The largest movie is streamed to scratch so that files with `moov` at the end can be probed. Counts for files with `moov` at the end can be incomplete. The `largest_video` column is marked `(partial probe)` where this applies.
4. **Signal pass.** Signals are regex hits over the game's scripts and `.rpyc` slots (decompressed, not unpickled). The `rpe_literal` signal matches `.rpe` and also `.rpe.py`, the engine's own script name, so it is a weak signal. Treat it as a hint only.
5. **Stock comparison.** Each game's `renpy/` tree is hashed and compared with the stock tree of the same engine version. Differences are split into **source** (`.py`, `.pyx`, `.rpy`, `.rpym`) and **bytecode** (`__pycache__`, `.pyc`, `.pyo`, `.rpyc`, `.rpymc`). Bytecode differences come from the interpreter that compiled the file, so they are expected. Source differences are the signal for a modified engine.

## Stock comparison: results and caveats

- 46 of 50 games were compared. Most games have **zero source differences**. Their `renpy/` trees differ from stock only in bytecode. Their engines match the official SDK.
- Stock manifests exist for 19 versions. These include nightly 7.8.8 (`7.8.8.25051901+nightly`). MyBestDeal reports vc build `7.8.8.25060602`, which has no matching nightly. The nearest nightly gives 2 source differences (`display/dragdrop.py`, `vc_version.py`). `vc_version.py` is probably a build stamp, not a code change. This was not checked.
- **No stock SDK for 8.4.2.** The download index (`renpy.org/dl/`) lists 8.4.0 and 8.4.1 only. The nightly URL for the exact build (`8.4.2.25092901+nightly`) returns 404. UnexpectedConsequences has no stock baseline. A proxy comparison with 8.4.1 gives 229 source differences. Those are expected between two minor releases, so the proxy does not show whether the engine is modified. Do not cite it as a stock result.
- **Source differences with a shared file set.** Four games differ from stock 8.0 to 8.2 in the same engine files: `__init__.py`, `add_from.py`, `arguments.py`, `ast.py`, `atl.py`. They are IndependentHorizonsChapterI-Completed-pc (8.0.0, 224 files), SweetAffection-0.11.9-pc (8.0.0, 224), The Awakening (8.2.1, 240), and The Fiery Scion (8.2.0, 211). The cause is not known. Possible causes are a modified engine, a repackaged engine (line endings, a different build), or a different engine branch. This was not checked by hand.
- Smaller source differences (1 to 42 files) are in games that ship their own `common/` or `main.py` files, or (StrayIncubus) `.pyx` sources. These are not confirmed as engine patches.

## Shortlist

Ranked by how well each entry fills a gap in [the corpus](../README.md).

1. **UnexpectedConsequences-0.18-pc** (engine 8.4.2, py3). 548 VP9 videos, all at 3840x2160 60 fps. The GUI is also 3840x2160. This is the only 8.4+ candidate with a real 4K workload. No stock baseline (see above).
2. **ChampionOfRealms-0.102-pc** (engine 8.4.1, py3). 1157 VP9 videos at 1080p60 (plus some 2560x1440). 91 RPA-3.0 archives. Signals: `crypto` 31, `base64` 14, `renpy_loader` 12, `zlib` 4, and `exec_call` 1 in `script.rpa` (`script/launcher.rpy`). Bundled `python-packages` (requests, urllib3). Source matches stock 8.4.1 exactly. The signals are an obfuscation or loader candidate. **Not confirmed.**
3. **LeapofFaith-1.0.3c-dlc-pc** (engine 8.0.2, py3). Source matches stock 8.0.2 exactly. Signals: `crypto` 31, `base64` 5, `xor_key` 2, and `exec_call` 1. An obfuscation candidate, **not confirmed**. Its engine is clean, so the signals come from the game side.
4. **IndependentHorizonsChapterI-Completed-pc** (engine 8.0.0, script 7.4.11, py2). 92 videos. 47 of them are at 3840x2160 (33 VP9 at 30000/1001, 9 MPEG-4 at 600/1, 5 Theora). Source differences with stock (see above). A 4K candidate and a modified-engine candidate. Pre-8.4.
5. **Homeforwaywardtravelers-0.21-pc** (engine 8.1.3, py3). 260 VP9 videos at 1920x1080 30 fps. The GUI is set to 3840x2160 (a 4K UI with 1080p video). Source matches stock.
6. **TheSwordbearer-0.11-pc** (engine 7.4.6, py2). Mixed RPA versions: RPA-2.0 ×1 and RPA-3.0 ×3. Source matches stock. Useful for RPA-2 coverage.
7. **The Awakening** (engine 8.2.1, py2 and py3). 476 RPA-3.0 archives, the most in the set. Use it as an RPA scale test. It is also a modified-engine candidate (source differences above).
8. **Mynewneighbors-1.83-pc** (engine 8.3.4, py3). 2563 VP9 videos at 1080p60, the largest movie count. Source matches stock.
9. **Lust Age** (engine 8.0.3, script 7.5.3, py2). 1313 VP8 videos at 1080p60. Source matches stock. A pre-8.4 video workload.
10. **SnowStorm-v0.10-WW-pc** (engine 7.3.5). 202 movies (WebM). Three VP8 videos at 3840x2160 (partial probe). This is a Ren'Py 7 4K sample. An earlier note that it had no movies was wrong and is corrected here.
11. **Artemis-0.6.2-pc** (engine 8.1.3, py3). 375 VP8 and VP9 videos. One VP9 video at 3840x2160 60 fps (partial probe). Source matches stock.

## Anomalies

- **MatrixHearts-3.9-pc**: the archive is corrupt. `zstd -t` reports `Decoding error (36): Data corruption detected` at 7,013,203,968 bytes. Download it again before use. It is not in the TSV results.
- **Out of Touch**: no engine version, no script, no `.rpyc`, no RPA, and zero media. The archive does not match the expected layout. Check the archive by hand.
- **LivingTogether_alpha_052_subscriber-0.52-pc**: no `renpy/` engine tree is in the archive. The archive bundles `python-packages/` (numpy, requests, urllib3) and a C and Fortran source tree under `lib/py3-windows-x86_64`. The `pickle_load` hits are mostly bundled numpy. The only game-side crypto hit is `Scripts/ui.rpyc`. Obfuscation is **not** confirmed.
- **Layout quirk, not an error.** TerminusReachSentinel2-Update40-pc, TribulationsOfAMage-0.51.0-pc, and SnowStorm use `lib/pythonlib2.7/` (Ren'Py 7.3.5). The py2 check does not detect this layout, so the TSV reports no Python version for them.
- **Mixed versions.** The Fiery Scion has engine 8.2.0 and script 8.3.7. StrayIncubus has engine 8.3.4 and script 7.4.11. The engine column follows `renpy_version_tuple` (or the vc fallback). The script column follows `script_version`.
- **Odd video heights.** Some movies have odd heights (for example 1920x1058, 1920x1062, 1041x7565, 2112x6829). The Arson-Betrayal has six movies that failed to probe. These are probe artefacts or real edge cases. They are not verified.

## Obfuscation: what was and was not found

No archive has a confirmed obfuscated engine. The signals in ChampionOfRealms and LeapofFaith are regex hits in game scripts. They need a manual read before the games are used as obfuscation tests. LivingTogether's hits come from bundled libraries, not the game. The `rpe_literal` signal matches the engine's own `.rpe.py` script name and is not evidence of a `.rpe` handler.

## Reproduce

Run on artemis under `nix shell nixpkgs#python3 nixpkgs#ffmpeg-headless`. Do not write to `~/Downloads` or `~/Games`.

```sh
# one archive
python3 archive_survey.py ~/Downloads/Unplayed/Game.tar.zst OUTDIR SCRATCH
# all archives, two at a time
./run_survey.sh
# stock trees for every engine version seen (nightly for 7.8.8 and the others the survey needs)
python3 stock_manifest.py 8.3.7 out/stock/8.3.7.json
# stock comparison, then the TSV
python3 stock_diff.py OUTDIR out/stock archives.txt > stock-diff.tsv
python3 aggregate.py OUTDIR archives.txt survey-downloads.tsv corrupt.txt
```
