# M6 status

Deep runs: seeded playthroughs (3 seeds, up to 30 min each) on the corpus; `player upgrade` on every Python 2 error.

## Deep-run coverage

Lines are distinct script lines that hold at least one executed node, of all script lines with a node (common code excluded). Run time is the sum over seeds.

| game | Ren'Py | seeds: stop reason | lines hit | labels hit | run time | errors by class | check |
|---|---|---|---|---|---|---|---|
| AHouseInTheRift | 7.6.1 | not run | | | | | |
| AWorldBetweenUs | 7.4.8 | not run | | | | | |
| AlexsVantasticAdventure | 7.4.8 | not run | | | | | |
| AstralLust | 7.8.2 | not run | | | | | |
| BlackRose | 7.7.3 | not run | | | | | |
| BloomWar | 7.4.11 | not run | | | | | |
| BraveheartAcademy | 7.4.8 | not run | | | | | |
| Bumpkin014 | 7.5.3 | not run | | | | | |
| CabinByTheLake | 7.4.8 | not run | | | | | |
| DFraction | 7.4.11 | not run | | | | | |
| DTRemake | 7.4.11 | not run | | | | | |
| Dreamscape | 7.4.11 | not run | | | | | |
| HaremHotel | 7.4.11 | not run | | | | | |
| InterimDomain | 7.4.5 | not run | | | | | |
| LuckyParadox | 7.4.11 | not run | | | | | |
| MaidandMaidens | 7.5.3 | not run | | | | | |
| WhiteRussian | 7.4.11 | not run | | | | | |
| Bumpkin015 | 8.1.3 | not run | | | | | |
| DOF | 8.3.2 | not run | | | | | |
| Ripples | 8.2.1 | not run | | | | | |
| SecretIsland | 8.0.1 | not run | | | | | |
| TheStormWithinUs | 8.5.3 | not run | | | | | |
| WaifuAcademy | 8.2.3 | not run | | | | | |

## Errors found

| id | game | class | exception | where | patchable | outcome |
|---|---|---|---|---|---|---|

## Player bug list

Errors that are not Python 2 patterns and not the game's own (stock does not show them), or that happen in a Ren'Py 8 game.

None found.

## Patch outcomes

| id | outcome | detail |
|---|---|---|
| (none yet) | | |

## State at hand-over (Oct 1, 14:0x local)

- **Deep-run campaign is still running** in the background on the Mac: `harness/deep_all.py --out harness/out/m6` (log `/tmp/m6-campaign.log`, player copy `player/target/m6-campaign-player`, 3 seeds x 30 min, the 17 Ren'Py 7 games first, then the 6 Ren'Py 8 games). Games not listed with a `deep` check above have not finished. It resumes after a stop (finished games are skipped). When it ends, rebuild this file: `nix develop .#player -c python3 harness/tools/m6_status.py --runs harness/out/m6 --data <data dir> > harness/M6-status.md`.
- **No Python 2 error was found yet** in the games that finished, so `player upgrade` has not run on a real error. It was exercised end to end on a synthetic Ren'Py 7 test game (NameError in an `init python` function) with a mock OpenAI-compatible server: reproduce on the unpatched game, first answer rejected (bad edit), second answer applied, pre-error save loaded on the patched game, failing node ran clean, sidecar `proposed`, `player patches ... accept` made it active, `apply-test` matched it. The full-gate leg (`--tier full --with-proposed --seed-data`) was started on that test game but had not finished (machine lock queue).
- **Model endpoint:** the artemis llama.cpp server (`llm-server`, Qwen3.6-35B-A3B) listens on artemis `127.0.0.1:8080` (OpenAI API at `/v1`, model id `Qwen3.6-35B-A3B-MTP-UD-Q4_K_XL.gguf`, no key). From the Mac use a tunnel: `ssh -N -L 18081:127.0.0.1:8080 user@<artemis-host>`, then `PLAYER_UPGRADE_BASE_URL=http://127.0.0.1:18081/v1 PLAYER_UPGRADE_MODEL=Qwen3.6-35B-A3B-MTP-UD-Q4_K_XL.gguf`. The server is stopped while a `gamemoderun` run is active on artemis, so the model phase must not overlap such runs.
- **Known driver limits:** games whose story needs clicks on custom screens stop on `loop` after the stall time (the driver presses random safe buttons of the screen after 20 s without a new line; `screen_actions` in corpus.toml still wins). Rolling saves use at most a tenth of the run time (adaptive gap); the error-time save `deep-errN` always exists.

