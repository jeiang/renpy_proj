# The player: one cargo build of the workspace's `player` binary with vendored crates and prefetched sources.
# `rar = true` (package `urp`): default cargo features, which include `rar` (UnRAR, extraction only; not an open source licence).
# `rar = false` (package `urp-norar`): `--no-default-features --features wayland`, free software only.
{ pkgs, ffmpegLgpl, sources, cpython, renpySrc, renpyRev, rar ? true }:
let
  inherit (pkgs) lib;
  # RARLAB's UnRAR licence: freeware, redistributable, not free software. nixpkgs has no attribute for it.
  unrarLicense = {
    shortName = "unrar";
    fullName = "UnRAR licence";
    url = "https://www.rarlab.com/rar/unrarsrc-7.1.10.tar.gz";
    free = false;
    redistributable = true;
  };
  linuxLibs = with pkgs; [ vulkan-loader wayland libxkbcommon libx11 libxcursor libxrandr libxi libxcb alsa-lib systemdLibs libdrm ];
  # The window-system, Vulkan and audio libraries are opened with dlopen. fixupPhase shrinks the runpath to the
  # NEEDED libraries, so postFixup puts the dlopen directories back.
  dlopenRpath = lib.optionalString pkgs.stdenv.hostPlatform.isLinux
    "${lib.makeLibraryPath linuxLibs}:/run/opengl-driver/lib";
in
pkgs.rustPlatform.buildRustPackage {
  pname = if rar then "urp" else "urp-norar";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.difference ../player (lib.fileset.unions (map lib.fileset.maybeMissing [ ../player/target ../player/upstream ../player/build-out ]));
  };
  sourceRoot = "source/player";
  cargoLock.lockFile = ../player/Cargo.lock;
  cargoBuildFlags = [ "-p" "player" ];
  buildNoDefaultFeatures = !rar;
  buildFeatures = lib.optionals (!rar) [ "wayland" ];
  doCheck = false;

  nativeBuildInputs = with pkgs; [
    pkg-config
    rustPlatform.bindgenHook
    python312
    python312Packages.cython
    patch
  ];
  buildInputs = [ ffmpegLgpl.dev pkgs.SDL2.dev pkgs.zlib ]
    ++ lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.libiconv ]
    ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux (linuxLibs);

  PLAYER_RENPY_SRC = renpySrc;
  PLAYER_RENPY_COMMIT = renpyRev;
  PLAYER_PYWHEELS = sources.pywheels;
  FFMPEG_LGPL = "${ffmpegLgpl}";
  PYO3_CONFIG_FILE = "/build/source/player/crates/pyhost/pyo3-config.txt";
  PLAYER_DLOPEN_RPATH = dlopenRpath;

  postFixup = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
    patchelf --add-rpath "${dlopenRpath}" $out/bin/urp
  '';

  meta = {
    description = "Universal Ren'Py Player" + lib.optionalString (!rar) " (without RAR support)";
    mainProgram = "urp";
    license = [ lib.licenses.mit lib.licenses.asl20 ] ++ lib.optional rar unrarLicense;
  };

  preBuild = ''
    mkdir -p build-out
    cp -r ${cpython} build-out/cpython
    chmod -R u+w build-out
    export PYO3_CONFIG_FILE=$PWD/crates/pyhost/pyo3-config.txt
  '';
}
