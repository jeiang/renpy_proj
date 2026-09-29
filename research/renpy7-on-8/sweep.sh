#!/bin/zsh
# Kill every engine process started from this worktree's corpus/ and assert none is left. Source or run.
# Pattern needs the SDK's lib/py3-mac binary AND the worktree corpus path in the command line, so it can't match the
# driver scripts themselves (their args carry the corpus path but not lib/py3-mac).
# Why: Ren'Py treats SIGTERM as a quit request and shows its exit-confirmation screen, so `kill $pid` leaves a live window.
sweep() {
  local pat="py3-mac.*renpy_proj-renpy7-on-8/corpus/"
  pkill -9 -f "$pat"; sleep 1
  if pgrep -f "$pat" >/dev/null; then echo "SWEEP FAILED: still running:"; pgrep -fl "$pat"; return 1; fi
  echo "sweep: no game process left"
}
[[ ${ZSH_EVAL_CONTEXT:-} == *:file* ]] || sweep
