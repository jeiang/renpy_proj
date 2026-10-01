# Fixed-output sources of the player build. Nothing else is fetched at build time.
{ pkgs }:
let
  inherit (pkgs) lib;
  # CPython 3.12.8: the SHA-256 python.org publishes (also checked again by crates/pyhost/cpython/build*.sh).
  cpythonTarball = pkgs.fetchurl {
    url = "https://www.python.org/ftp/python/3.12.8/Python-3.12.8.tar.xz";
    sha256 = "c909157bb25ec114e5869124cc2a9c4a4d4c1e957ca4ff553f1edc692101154e";
  };
  # The pure-Python wheels of player/engine/wheels.txt (the same table fetch.sh reads): ecdsa, six, requests,
  # urllib3, idna, certifi, charset_normalizer.
  wheelRows = builtins.filter (l: l != [ ] && !(lib.hasPrefix "#" (builtins.head l)))
    (map (l: lib.splitString " " l) (lib.splitString "\n" (builtins.readFile ../player/engine/wheels.txt)));
  wheelFiles = map
    (r: pkgs.fetchurl {
      name = builtins.elemAt r 0;
      url = builtins.elemAt r 1;
      sha256 = builtins.elemAt r 2;
    })
    (builtins.filter (r: builtins.length r == 3) wheelRows);
  pywheels = pkgs.linkFarm "player-pywheels" (map (w: { name = w.name; path = w; }) wheelFiles);
in
{ inherit cpythonTarball pywheels; }
