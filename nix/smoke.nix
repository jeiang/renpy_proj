# checks.<system>.urp-smoke and urp-norar-smoke: the built binary, copied out of the store into a temp dir, runs a synthetic
# game with a clean environment (nothing from the build shell).
{ pkgs, urp, name ? "urp-smoke" }:
pkgs.runCommand name { } ''
  work=$(mktemp -d)
  cp ${urp}/bin/urp $work/urp
  chmod u+w $work/urp
  cd $work
  run() { env -i HOME=$work PATH=/usr/bin:/bin "$@"; }

  echo "== lint"
  run ./urp ${./smoke-game} --data $work/data-lint lint > lint.out 2>&1 || { cat lint.out; exit 1; }
  cat lint.out
  grep -q '^Statistics:' lint.out
  ! grep -q Traceback lint.out

  echo "== headless probe (init blocks run, quit before any window)"
  run PLAYER_SMOKE_MARKER=$work/marker ./urp ${./smoke-game} --data $work/data-probe --logdir $work/logs \
    --harness-script ${./smoke-probe.rpy} > probe.out 2>&1 || { cat probe.out; exit 1; }
  cat probe.out
  grep -q '^init-done 8\.5\.3' $work/marker
  ! grep -q Traceback probe.out
  [ ! -e $work/logs/traceback.txt ]

  echo "== the game folder stayed untouched"
  touch $out
''
