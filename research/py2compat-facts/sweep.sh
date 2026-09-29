#!/bin/zsh
# Kill every engine process started from this worktree's corpus/ and assert none is left. Source or run.
sweep() {
  local pat="py3-mac.*renpy_proj/.worktrees/py2compat-facts/corpus/"
  pkill -9 -f "$pat"; sleep 1
  if pgrep -f "$pat" >/dev/null; then echo "SWEEP FAILED: still running:"; pgrep -fl "$pat"; return 1; fi
  echo "sweep: no game process left"
}
[[ ${ZSH_EVAL_CONTEXT:-} == *:file* ]] || sweep
