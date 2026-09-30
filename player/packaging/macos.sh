#!/usr/bin/env bash
# One signed player on macOS: release build, dylib check, signing, .app bundle, hardened-runtime probe.
#
#   player/packaging/macos.sh [--no-build] [--probe-only]
#
# Output (gitignored): player/build-out/macos/RenPyPlayer.app
#   Contents/MacOS/player       the player binary (this is the "signed binary" the gate runs)
#   Contents/Frameworks/*.dylib every library that is not a macOS system library (see below)
#
# Steps
#   1. `cargo build --release -p player` in the player dev shell (skipped with --no-build).
#   2. Lists the libraries the binary links. macOS system libraries (/usr/lib, /System) are fine. Any
#      other library (today FFmpeg and its dependencies; FreeType and HarfBuzz until the Text slice
#      lands) is copied into Contents/Frameworks and its install names are rewritten to
#      @executable_path/../Frameworks, so the bundle does not depend on /nix/store. Then it checks that
#      `otool -L` of every Mach-O file names only system libraries or that folder.
#   3. Signs every dylib, then the app, ad hoc with the hardened runtime:
#        codesign --force -s - -o runtime --timestamp=none
#      and runs `codesign --verify --strict --deep`.
#   4. Runs the ctypes probe (packaging/ctypes_probe.rpy) under the signed binary: `ctypes.CFUNCTYPE`
#      callbacks need libffi closures, which the hardened runtime can refuse (research/pypack open item d).
#   5. Developer ID signing and notarization (human-in-the-loop, they need the user's Apple account):
#        DEVELOPER_ID_IDENTITY   "Developer ID Application: Name (TEAMID)": signs with a secure timestamp
#        NOTARY_KEYCHAIN_PROFILE a `xcrun notarytool store-credentials` profile name: notarizes and staples
#      Each step is skipped, with a message, when its variable is not set.
#
# Environment: PLAYER_ENTITLEMENTS names the entitlements plist for the signature (default:
# packaging/entitlements.plist).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
player_dir="$(cd "$here/.." && pwd)"
repo_root="$(cd "$player_dir/.." && pwd)"
out="$player_dir/build-out/macos"
app="$out/RenPyPlayer.app"
exe="$app/Contents/MacOS/player"
fw="$app/Contents/Frameworks"
bundle_id="io.github.jeiang.renpy-player"
ent=(--entitlements "${PLAYER_ENTITLEMENTS:-$here/entitlements.plist}")

build=1
probe_only=0
for a in "$@"; do
    case "$a" in
        --no-build) build=0 ;;
        --probe-only) probe_only=1 ;;
        *) echo "unknown argument: $a" >&2; exit 2 ;;
    esac
done

log() { printf '[macos.sh] %s\n' "$*"; }

if [ "$(uname -s)" != Darwin ]; then
    echo "macos.sh runs on macOS only" >&2
    exit 1
fi

# --- 1. build ---------------------------------------------------------------------------------
if [ "$probe_only" = 0 ] && [ "$build" = 1 ]; then
    log "cargo build --release -p player"
    if [ -n "${FFMPEG_LGPL:-}" ]; then
        (cd "$player_dir" && cargo build --release -p player)
    else
        (cd "$player_dir" && nix develop "$repo_root#player" -c cargo build --release -p player)
    fi
fi

is_system() {
    case "$1" in /usr/lib/*|/System/*) return 0 ;; *) return 1 ;; esac
}

# Nix's libiconv exports `_libiconv`, macOS's /usr/lib/libiconv.2.dylib exports `_iconv`. A library that
# imports `_iconv` (FFmpeg's) needs the system one, a library that imports `_libiconv` (libidn2) needs the
# bundled copy of nix's. This prints the replacement path for library $1 as linked by Mach-O file $2, or
# nothing when the bundled copy is right.
system_alias() {
    case "$(basename "$1")" in
        libiconv.2.dylib|libcharset.1.dylib) ;;
        *) return 0 ;;
    esac
    if nm -u "$2" 2>/dev/null | grep -Eq '(^|[[:space:]])_(libiconv|locale_charset)'; then
        return 0
    fi
    echo "/usr/lib/$(basename "$1")"
}

# Absolute paths of the non-system libraries that Mach-O file $1 links (one per line). It skips the
# file's own install name (`otool -L` lists it first for a dylib) and names that are already relocated.
foreign_deps() {
    local self
    self="$(basename "$1")"
    otool -L "$1" | tail -n +2 | awk '{print $1}' | while read -r lib; do
        is_system "$lib" && continue
        case "$lib" in @*) continue ;; esac
        [ "$(basename "$lib")" = "$self" ] && continue
        echo "$lib"
    done
}

# Every library a Mach-O file names that is neither a system library nor in the bundle's Frameworks.
unresolved_deps() {
    local self
    self="$(basename "$1")"
    otool -L "$1" | tail -n +2 | awk '{print $1}' | while read -r lib; do
        is_system "$lib" && continue
        [ "$(basename "$lib")" = "$self" ] && continue
        case "$lib" in @executable_path/../Frameworks/*) ;; *) echo "$lib" ;; esac
    done
}

if [ "$probe_only" = 0 ]; then
    bin="$player_dir/target/release/player"
    [ -x "$bin" ] || { echo "missing $bin" >&2; exit 1; }

    # --- 2. bundle layout and libraries ------------------------------------------------------------
    rm -rf "$app"
    mkdir -p "$app/Contents/MacOS" "$fw" "$app/Contents/Resources"
    cp "$bin" "$exe"
    chmod u+w "$exe"

    version="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$player_dir/Cargo.toml" | head -1)"
    version="${version:-0.1.0}"
    cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleExecutable</key><string>player</string>
    <key>CFBundleIdentifier</key><string>$bundle_id</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>Ren'Py Player</string>
    <key>CFBundleDisplayName</key><string>Ren'Py Player</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

    log "libraries that are not macOS system libraries, before bundling:"
    foreign_deps "$exe" | sed 's/^/    /'

    # Copy every non-system library, and the ones it needs in turn, into Frameworks.
    todo="$(mktemp)"
    done_list="$(mktemp)"
    foreign_deps "$exe" >"$todo"
    while [ -s "$todo" ]; do
        lib="$(head -1 "$todo")"
        sed -i '' 1d "$todo"
        grep -qxF "$lib" "$done_list" && continue
        echo "$lib" >>"$done_list"
        base="$(basename "$lib")"
        cp "$lib" "$fw/$base"
        chmod u+w "$fw/$base"
        install_name_tool -id "@executable_path/../Frameworks/$base" "$fw/$base"
        foreign_deps "$fw/$base" >>"$todo"
    done
    # Point every reference at the copies.
    for target in "$exe" "$fw"/*.dylib; do
        [ -e "$target" ] || continue
        foreign_deps "$target" | while read -r lib; do
            alias="$(system_alias "$lib" "$target")"
            install_name_tool -change "$lib" "${alias:-@executable_path/../Frameworks/$(basename "$lib")}" "$target"
        done
    done
    rm -f "$todo" "$done_list"

    # Every Mach-O file names only system libraries or the bundle's Frameworks folder.
    bad=0
    for target in "$exe" "$fw"/*.dylib; do
        [ -e "$target" ] || continue
        while read -r lib; do
            echo "unresolved library in $target: $lib" >&2
            bad=1
        done < <(unresolved_deps "$target")
        while read -r lib; do
            case "$lib" in
                @executable_path/../Frameworks/*)
                    [ -e "$fw/$(basename "$lib")" ] || { echo "missing bundled $lib for $target" >&2; bad=1; } ;;
            esac
        done < <(otool -L "$target" | tail -n +2 | awk '{print $1}')
    done
    [ "$bad" = 0 ] || exit 1
    log "otool -L of the binary after bundling:"
    otool -L "$exe" | tail -n +2 | sed 's/^/    /'

    # --- 3. ad hoc signature, hardened runtime ---------------------------------------------------
    for d in "$fw"/*.dylib; do
        [ -e "$d" ] || continue
        codesign --force -s - -o runtime --timestamp=none "$d" 2>/dev/null
    done
    codesign --force -s - -o runtime --timestamp=none "${ent[@]}" "$app"
    codesign --verify --strict --deep --verbose=2 "$app"
    log "codesign -dvv:"
    codesign -dvv "$app" 2>&1 | sed 's/^/    /'
fi

[ -x "$exe" ] || { echo "no signed app at $app (run without --probe-only first)" >&2; exit 1; }

# --- 4. ctypes probe under the hardened runtime -----------------------------------------------------
probe="$(mktemp -d)"
mkdir -p "$probe/project/game" "$probe/data"
cp "$here/ctypes_probe.rpy" "$probe/project/game/script.rpy"
CTYPES_PROBE_OUT="$probe/result.txt" "$exe" "$probe/project" --data "$probe/data" >"$probe/stdout.txt" 2>&1 || true
if [ -s "$probe/result.txt" ] && grep -q '^ctypes callback ok' "$probe/result.txt"; then
    log "ctypes probe: $(cat "$probe/result.txt")"
else
    log "ctypes probe FAILED (output follows)"
    cat "$probe/result.txt" "$probe/stdout.txt" 2>/dev/null | sed 's/^/    /'
    find "$probe/data/logs" -name traceback.txt -exec cat {} \; 2>/dev/null | sed 's/^/    /'
    rm -rf "$probe"
    exit 1
fi
rm -rf "$probe"
[ "$probe_only" = 1 ] && exit 0

# --- 5. Developer ID signature and notarization (HITL) -----------------------------------------------
if [ -n "${DEVELOPER_ID_IDENTITY:-}" ]; then
    log "signing with Developer ID: $DEVELOPER_ID_IDENTITY"
    for d in "$fw"/*.dylib; do
        [ -e "$d" ] || continue
        codesign --force -s "$DEVELOPER_ID_IDENTITY" -o runtime --timestamp "$d" 2>/dev/null
    done
    codesign --force -s "$DEVELOPER_ID_IDENTITY" -o runtime --timestamp "${ent[@]}" "$app"
    codesign --verify --strict --deep --verbose=2 "$app"
    if [ -n "${NOTARY_KEYCHAIN_PROFILE:-}" ]; then
        zipfile="$out/RenPyPlayer-notarize.zip"
        ditto -c -k --keepParent "$app" "$zipfile"
        log "notarizing (this waits for Apple)"
        xcrun notarytool submit "$zipfile" --keychain-profile "$NOTARY_KEYCHAIN_PROFILE" --wait
        xcrun stapler staple "$app"
        spctl --assess --type execute --verbose=2 "$app"
    else
        log "notarization skipped: set NOTARY_KEYCHAIN_PROFILE (xcrun notarytool store-credentials) to notarize"
    fi
else
    log "Developer ID signing and notarization skipped: set DEVELOPER_ID_IDENTITY (and NOTARY_KEYCHAIN_PROFILE) to run them"
fi

log "done: $app"
