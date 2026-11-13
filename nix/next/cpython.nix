{ pkgs, sources }:
let
  inherit (pkgs) lib;
  s = pkgs.pkgsStatic;
in pkgs.stdenv.mkDerivation {
  pname = "urp-cpython";
  version = "3.12.8";
  src = sources.cpythonTarball;
  patches = lib.filesystem.listFilesRecursive ../../player/crates/pyhost/cpython/patches;
  nativeBuildInputs = [ pkgs.pkg-config ];
  FFI = s.libffi.out;
  FFI_DEV = s.libffi.dev;
  BZ = s.bzip2.out;
  BZ_DEV = s.bzip2.dev;
  XZ = s.xz.out;
  XZ_DEV = s.xz.dev;
  EXP = s.expat.out;
  EXP_DEV = s.expat.dev;
  ZL = s.zlib.out;
  ZL_DEV = s.zlib.dev;
  SSL = s.openssl.out;
  SSL_DEV = s.openssl.dev;
  preConfigure = ''
    sh ${./setup.sh}
  '';
  configureFlags = [ "--disable-shared" "--without-ensurepip" "--disable-test-modules" ];
  env.CFLAGS = "-O2 -fPIC";
  enableParallelBuilding = true;
  postInstall = ''
    mkdir -p $out/deps
    cp $FFI/lib/libffi.a $BZ/lib/libbz2.a $XZ/lib/liblzma.a $EXP/lib/libexpat.a \
      $ZL/lib/libz.a $SSL/lib/libssl.a $SSL/lib/libcrypto.a $out/deps/
    $out/bin/python3.12 ${../../player/crates/pyhost/cpython/mkzip.py} $out/lib/python3.12 $out/stdlib.zip
    cp ${../../player/crates/pyhost/pyo3-config.txt} $out/pyo3-config.txt
    cat > $out/link.txt <<EOF
    rustc-link-search=native=$out/lib
    rustc-link-search=native=$out/deps
    rustc-link-lib=static=python3.12
    rustc-link-lib=static=ffi
    rustc-link-lib=static=bz2
    rustc-link-lib=static=lzma
    rustc-link-lib=static=expat
    rustc-link-lib=static=ssl
    rustc-link-lib=static=crypto
    rustc-link-lib=static=z
    EOF
  '' + lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
    printf '%s\n' rustc-link-lib=dl rustc-link-lib=m rustc-link-lib=pthread >> $out/link.txt
  '' + lib.optionalString pkgs.stdenv.hostPlatform.isDarwin ''
    printf '%s\n' rustc-link-lib=framework=CoreFoundation rustc-link-lib=framework=SystemConfiguration rustc-link-lib=iconv >> $out/link.txt
  '';
}
