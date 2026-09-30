{
  description = "renpy_proj: a Ren'Py 7/8 player (research + build)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" "aarch64-linux" ];
      # LGPL-only FFmpeg 7 (ARCHITECTURE.md, Licensing): never GPL or nonfree.
      # Same options as the accepted video prototype (prototype/video-proto).
      ffmpegLgpl = pkgs: pkgs.ffmpeg_7-headless.override {
        withGPL = false;
        withGPLv3 = false;
        withVersion3 = false;
        withUnfree = false;
        withX264 = false;
        withX265 = false;
        withXvid = false;
        withAom = false;
        withSvtav1 = false;
        withVpx = false;
        withDav1d = true;
        withSmallBuild = false;
      };
    in
    {
      packages = forAllSystems (system: {
        ffmpeg-lgpl = ffmpegLgpl nixpkgs.legacyPackages.${system};
      });

      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in
        {
          # Research shell: analysis tools. Its ffmpeg is the GPL CLI build; never link against it.
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              rust-analyzer
              python3
              python312 # Ren'Py 8.4-8.5.3 pin CPython 3.12
              unrpa
              ffmpeg
              git
              gh
            ];
          };

          # Build shell for player/ (nix develop .#player). Links only LGPL FFmpeg.
          player = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              rust-analyzer
              pkg-config
              llvmPackages.libclang
              python312 # build-time Python: cythonize and layer packing
              python312Packages.cython
              (ffmpegLgpl pkgs).dev
              freetype
              harfbuzz
              SDL2.dev # headers only: stock ftfont/hbfont cimport sdl2.pxd (capsule shim bring-up)
              zlib
              zlib.static
              libiconv
              git
              gh
            ];
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            FFMPEG_LGPL = "${ffmpegLgpl pkgs}";
          };
        });
    };
}
