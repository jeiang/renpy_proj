{ pkgs, ffmpegLgpl, renpySrc, renpyRev }:
let
  inherit (pkgs) lib;
  sources = import ../sources.nix { inherit pkgs; };
  cpython = import ./cpython.nix { inherit pkgs sources; };
  engine = pkgs.stdenv.mkDerivation {
    pname = "urp-engine";
    version = "8.5.3";
    src = lib.fileset.toSource {
      root = ../..;
      fileset = lib.fileset.unions [
        (lib.fileset.difference ../../player/engine
          (lib.fileset.unions [ ../../player/engine/python/tests
            (lib.fileset.fileFilter (file: file.hasExt "md") ../../player/engine) ]))
        ../../player/packaging/compile_common.py
      ];
    };
    nativeBuildInputs = [ pkgs.python312 pkgs.python312Packages.cython pkgs.pkg-config pkgs.patch ];
    buildInputs = [ pkgs.SDL2.dev ];
    PLAYER_RENPY_SRC = renpySrc;
    PLAYER_RENPY_COMMIT = renpyRev;
    PLAYER_PYWHEELS = sources.pywheels;
    dontConfigure = true;
    buildPhase = ''
      runHook preBuild
      python3.12 player/engine/build.py --py-include ${cpython}/include/python3.12
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      # Existing engine tests locate common sources relative to this host-tree layout.
      mkdir -p $out/common-rpyc/host
      ln -s ../../tree/renpy $out/common-rpyc/host/renpy
      mkdir -p $out
      cp -r player/build-out/engine/{libengine_cy.a,inittab.txt,layer.zip,common.zip,tree} $out/
      runHook postInstall
    '';
  };
  enginepy = pkgs.stdenv.mkDerivation {
    pname = "urp-enginepy";
    version = "8.5.3";
    dontUnpack = true;
    nativeBuildInputs = [ pkgs.python312 ];
    buildPhase = ''
      python3.12 - ${engine}/inittab.txt ${./bootstrap.py} <<'PY'
      import json, pathlib, sys
      rows = [line.split() for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
      pathlib.Path('inittab.h').write_text(
          "".join('extern PyObject *%s(void);\n' % symbol for _, symbol in rows) +
          'static struct _inittab engine_inittab[] = {\n' +
          "".join('{"%s", %s},\n' % (name, symbol) for name, symbol in rows) + '{NULL, NULL}};\n')
      pathlib.Path('bootstrap.h').write_text('static const char *bootstrap = ' + json.dumps(pathlib.Path(sys.argv[2]).read_text()) + ';\n')
      PY
      cc -I. -I${cpython}/include/python3.12 \
        '-DCPYTHON="${cpython}"' \
        '-DENGINE_TREE="\"${engine}/tree\""' \
        '-DENGINE_LAYER="\"${engine}/layer.zip\""' \
        ${./enginepy.c} ${engine}/libengine_cy.a \
        -L${cpython}/lib -L${cpython}/deps \
        -lpython3.12 -lffi -lbz2 -llzma -lexpat -lssl -lcrypto -lz \
        ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux "-ldl -lm -lpthread"} \
        ${lib.optionalString pkgs.stdenv.hostPlatform.isDarwin "-framework CoreFoundation -framework SystemConfiguration -liconv"} \
        -o urp-enginepy
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp urp-enginepy $out/bin/
    '';
  };
in {
  urp-cpython = cpython;
  urp-ffmpeg = pkgs.symlinkJoin {
    name = "urp-ffmpeg";
    paths = [ ffmpegLgpl ffmpegLgpl.dev ];
  };
  urp-engine = engine;
  urp-enginepy = enginepy;
}
