# Static CPython 3.12.8 for the player: runs the same recipe as the dev shell (crates/pyhost/cpython/build*.sh)
# with prefetched inputs. $out has the layout of player/build-out/cpython.
{ pkgs, sources }:
let
  inherit (pkgs) lib stdenv;
  s = pkgs.pkgsStatic;
  script = if stdenv.hostPlatform.isDarwin then "build.sh" else "build_linux.sh";
in
stdenv.mkDerivation {
  pname = "player-cpython";
  version = "3.12.8";
  src = lib.fileset.toSource {
    root = ../player/crates/pyhost;
    fileset = lib.fileset.unions [ ../player/crates/pyhost/cpython ../player/crates/pyhost/examples ];
  };
  nativeBuildInputs = [ pkgs.xz pkgs.perl ]; # perl: shasum
  PYHOST_CPYTHON_TARBALL = sources.cpythonTarball;
  PYHOST_ENV_CC = "1";
  PYHOST_BZ = s.bzip2.out;
  PYHOST_BZ_DEV = s.bzip2.dev;
  PYHOST_XZ = s.xz.out;
  PYHOST_XZ_DEV = s.xz.dev;
  PYHOST_EXP = s.expat.out;
  PYHOST_EXP_DEV = s.expat.dev;
  PYHOST_ZL = s.zlib.out;
  PYHOST_ZL_DEV = s.zlib.dev;
  PYHOST_SSL = s.openssl.out;
  PYHOST_SSL_DEV = s.openssl.dev;
  dontConfigure = true;
  dontFixup = true; # static archives and zips only
  buildPhase = ''
    runHook preBuild
    mkdir -p work/player/crates/pyhost
    cp -r cpython examples work/player/crates/pyhost/
    chmod -R u+w work
    sh work/player/crates/pyhost/cpython/${script} work/player
    runHook postBuild
  '';
  installPhase = ''
    runHook preInstall
    cp -r work/player/build-out/cpython $out
    runHook postInstall
  '';
}
