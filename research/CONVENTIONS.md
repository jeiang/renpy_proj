# Research conventions

Rules for any agent (or subagent) resolving a ticket on the wayfinder map, [Map: Ren'Py 8 player feasibility and architecture](https://github.com/jeiang/renpy_proj/issues/1). Learned from the first research round.

## Git

- One ticket, one branch, one worktree, all kept inside the repo under the gitignored `.worktrees/`: `git worktree add .worktrees/<slug> -b research/<slug>`. Work and commit only there. Parallel agents never share an index.
- Output goes in `research/<slug>/`: a fact sheet `README.md` ending with "Implications for the route decision", plus kept scripts and small outputs.
- The coordinator merges `research/<slug>` into `main` (`git merge --no-ff`) and pushes. Subagents do not push or merge.
- Finished worktrees stay under `.worktrees/` with their gitignored artefacts (screenshots, SDKs, `out/`, corpus clones) until the user cleans up at the end of the session. Never run `git worktree remove --force`: it deletes ignored files, which is how the Ren'Py 7 on 8 screenshots were lost. Move large APFS clones with `mv`, never copy them (`rsync` breaks clone sharing).
- Games run by several agents at once distort timings and fight over the screen. Hold the machine lock while any game process runs: `python3 harness/tools/runlock.py -- <command...>`. It takes an `flock` on `/tmp/renpy_proj.run.flock` and the compatibility dir `/tmp/renpy_proj.run.lock` (with an `owner` file), runs the command, and releases both when the command exits. The kernel frees the flock if the holder dies. Do not `mkdir` the dir by hand. Never take the lock from an in-process tool or a long-lived shell. Keep each hold short (one run), and run long jobs in the background so a tool timeout cannot kill a holder. `harness/gatelib` takes the same lock for the gate. Never wrap `gate.py` (or the deep runners) in `runlock.py`: the gate takes the lock itself, so the nested request deadlocks.

## What to commit

- Commit your own notes, scripts, and small measured outputs.
- Never commit third-party text (READMEs, docs, source). Link it, and add a `fetch.sh` that re-downloads it into a gitignored directory (`upstream/`, `src/`, `sdk/`).
- Never commit clones, SDKs, media, `__pycache__`, or game assets. The root `.gitignore` covers `__pycache__/`, `*.pyc`, `research/*/upstream/`, `/corpus/`, `/harness/corpus.local.toml` and `/harness/local/` (per-game config, plans and drivers of the real games; list new local files in `docs/LOCAL-FILES.md`). Add a per-directory `.gitignore` for anything else over 1 MB.
- Before the coordinator merges, check `git show --stat` on each commit for vendored or binary files.

## GitHub tickets

- Claim the ticket first (`gh issue edit <n> --add-assignee @me`), then resolve: comment with the answer and a fact-sheet link, then close. Confirm the close with `gh issue view <n> --json state`. One agent reported a close it never made.
- Scripted ticket creation: create in one step and wire (sub-issues, `blocked_by`) in a second step. Check what already exists (`gh issue list --state all`) before creating, so a timeout and rerun make no duplicates. Give long `gh` loops a timeout of a few minutes.

## Running Ren'Py

- Never run a game against the user's real save directory. Ren'Py 7 ignores `RENPY_PATH_TO_SAVES`: pass `--savedir <scratch>` as well. Ren'Py also copies every `.rpy` it compiles into `~/Library/RenPy/backups/<game>`; set `RENPY_DISABLE_BACKUPS="I take responsibility for this."` for every stock run that compiles scripts. Pass `--savedir <scratch>` (`renpy/arguments.py`) or set `RENPY_PATH_TO_SAVES=<scratch>` (`renpy.py` L173, parent of the per-game dir), and check `~/Library/RenPy/<game>` is unchanged afterwards. Ren'Py also writes to `game/saves/` inside the game directory, so run a scratch copy, never the original. A newer engine (8.4+) rewrites `persistent` in a format older engines can't read.
- Never modify games under `~/Games`. Work on APFS clones in the repo's gitignored `corpus/`: `research/test-corpus/make_released.py` builds a released (`.rpyc`-only) copy. For a plain copy, use `/bin/cp -Rc` (the Nix shell's GNU `cp` has no `-c`). Always run `xattr -dr com.apple.quarantine <copy>` on copied `.app` bundles so Gatekeeper doesn't block them.
- Headless compatibility gate: `<engine> <game> lint` with `RENPY_PATH_TO_SAVES` pointing at scratch (see `research/test-corpus/lint_released.sh`). It loads every script without opening a window.
- No computer use or screen capture: the permission needs a terminal restart. Verify through `log.txt`, `traceback.txt`, exit codes, and process state. Record visual checks as HITL follow-ups.
- Kill every game process you start. A process-group `SIGKILL` has missed Ren'Py windows before, and the user had to close them by hand at the exit-confirmation screen. After each run, sweep by path: `pkill -9 -f "<your worktree>/corpus/"`. Then check with `pgrep -f` that nothing is left.
- "Process alive after N seconds" is not a pass, because Ren'Py stays up while it shows an error screen. Check `traceback.txt` and `log.txt` as well, and mark the result as not visually confirmed until a person has looked at it.

## Tools

- Run commands in `nix develop -c <cmd>`. Use `nix shell nixpkgs#<pkg> -c <cmd>` for a one-off tool and name it in the fact sheet. Don't install anything globally.
- The shell is zsh with unmatched globs as errors: quote globs.
