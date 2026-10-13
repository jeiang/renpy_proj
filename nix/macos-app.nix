# macOS: wraps a player build so that `$out/Applications/<name>.app` exists next to `$out/bin/urp`.
# nix-darwin lists the app in /Applications/Nix Apps, and home-manager in ~/Applications/Home Manager Apps,
# so Spotlight, the Dock and Launchpad find it. `$out/bin/urp` is unchanged, so the CLI and `nix run` still work.
#
# The app's executable is a copy of the Nix-built binary (the bytes and the linker ad-hoc signature are kept).
# Nothing is re-signed here and the hardened runtime is not turned on: see packaging/macos.sh for why the signed
# Developer ID build differs. The bundle is laid out by packaging/macos/make-app.sh, the same script that
# packaging/macos.sh uses.
{ pkgs, raw, name, bundleId }:
let
  # The only repo inputs: the layout script with its plist template, the icon script and the licence notices.
  packaging = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [
      ../LICENSE-MIT
      ../LICENSE-APACHE
      ../THIRD_PARTY.md
      ../player/packaging/macos
      ../player/packaging/icon
      ../player/packaging/licences
    ];
  };
in
pkgs.runCommand raw.name {
  nativeBuildInputs = [ pkgs.python3 ];
  inherit (raw) version meta;
  # Keep the copied binary byte-for-byte: no strip, no patchShebangs, no re-signing by the stdenv fixup.
  dontFixup = true;
} ''
  mkdir -p $out/bin
  ln -s ${raw}/bin/urp $out/bin/urp
  bash ${packaging}/player/packaging/macos/make-app.sh \
    "$out/Applications/${name}.app" ${raw}/bin/urp ${raw.version} ${bundleId} "${name}"
''
