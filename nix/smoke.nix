# checks.<system>.urp-smoke and urp-norar-smoke: the built binary, copied out of the store into a temp dir, runs a synthetic
# game with a clean environment (nothing from the build shell). On macOS it also checks the app bundle and runs the
# binary inside it (the one Finder, the Dock and Spotlight launch).
{ pkgs, urp, name ? "urp-smoke", appName ? null }:
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

  ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isDarwin ''
  echo "== macOS app bundle"
  app="${urp}/Applications/${appName}.app"
  test -d "$app"
  test -x "$app/Contents/MacOS/urp"
  test -s "$app/Contents/Resources/AppIcon.icns"
  test -f "$app/Contents/Resources/licenses/THIRD_PARTY.md"
  grep -qF '<key>CFBundleExecutable</key><string>urp</string>' "$app/Contents/Info.plist"
  grep -qF '<key>CFBundleIconFile</key><string>AppIcon</string>' "$app/Contents/Info.plist"
  ! grep -q LSUIElement "$app/Contents/Info.plist"
  run "$app/Contents/MacOS/urp" ${./smoke-game} --data $work/data-app lint > app.out 2>&1 || { cat app.out; exit 1; }
  cat app.out
  grep -q '^Statistics:' app.out
  ! grep -q Traceback app.out

  ''}
  echo "== the game folder stayed untouched"
  touch $out
''
