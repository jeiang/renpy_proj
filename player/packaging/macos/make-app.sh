#!/usr/bin/env bash
# Lays out a macOS app bundle around one urp binary. Shared by packaging/macos.sh (signed, with the bundled
# libraries) and by the Nix build (nix/macos-app.nix). It does not bundle libraries or sign: the caller does.
#
#   packaging/macos/make-app.sh <App.app> <urp binary> <version> <bundle id> <display name>
#
# Writes Contents/MacOS/urp (a copy of the binary), Contents/Info.plist (from Info.plist.in),
# Contents/Resources/AppIcon.icns and the licence notices. Files already in the bundle are replaced.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
packaging="$(cd "$here/.." && pwd)"
[ "$#" -eq 5 ] || { echo "usage: make-app.sh <App.app> <urp binary> <version> <bundle id> <display name>" >&2; exit 2; }
app="$1"; bin="$2"; version="$3"; bundle_id="$4"; name="$5"
[ -f "$bin" ] || { echo "make-app.sh: missing binary: $bin" >&2; exit 1; }

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/urp"
chmod u+w "$app/Contents/MacOS/urp"

# The values contain no sed metacharacters (no &, no |), so plain substitution is safe.
sed -e "s|@BUNDLE_ID@|$bundle_id|g" -e "s|@NAME@|$name|g" -e "s|@VERSION@|$version|g" \
    "$here/Info.plist.in" >"$app/Contents/Info.plist"

python3 "$packaging/icon/make_icon.py" icns "$app/Contents/Resources/AppIcon.icns"

# Licence notices: THIRD_PARTY.md, LGPL text for the FFmpeg libraries, Rust crate licences.
bash "$packaging/licences/stage.sh" "$app/Contents/Resources/licenses"
