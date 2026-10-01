# The player: one cargo build of the workspace's `player` binary with vendored crates and prefetched sources.
{ pkgs, ffmpegLgpl, sources, cpython, renpySrc, renpyRev }:
let
  inherit (pkgs) lib;
  linuxLibs = with pkgs; [ vulkan-loader wayland libxkbcommon libx11 libxcursor libxrandr libxi libxcb alsa-lib systemdLibs libdrm ];
in
pkgs.rustPlatform.buildRustPackage {
  pname = "player";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.difference ../player (lib.fileset.unions [ ../player/target ../player/upstream ../player/build-out ]);
  };
  sourceRoot = "source/player";
  cargoLock.lockFile = ../player/Cargo.lock;
  cargoBuildFlags = [ "-p" "player" ];
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
  PLAYER_DLOPEN_RPATH = lib.optionalString pkgs.stdenv.hostPlatform.isLinux
    "${lib.makeLibraryPath linuxLibs}:/run/opengl-driver/lib";

  preBuild = ''
    mkdir -p build-out
    cp -r ${cpython} build-out/cpython
    chmod -R u+w build-out
    export PYO3_CONFIG_FILE=$PWD/crates/pyhost/pyo3-config.txt
  '';
}
