{
  # THROWAWAY prototype shell (ticket #23). Builds an LGPL-only FFmpeg 7.1 with
  # VideoToolbox (auto-detected on darwin) and dav1d; no GPL, no nonfree, no x264/x265.
  description = "video-proto: LGPL FFmpeg 7.1 + rust";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { nixpkgs, ... }:
    let
      system = "aarch64-darwin";
      pkgs = nixpkgs.legacyPackages.${system};
      ffmpegLgpl = pkgs.ffmpeg_7-headless.override {
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
        withSmallBuild = false; # --enable-small is -Os and would understate speed
      };
    in {
      packages.${system}.ffmpeg = ffmpegLgpl;
      devShells.${system}.default = pkgs.mkShell {
        packages = [ pkgs.cargo pkgs.rustc pkgs.pkg-config pkgs.llvmPackages.libclang ffmpegLgpl.dev pkgs.libiconv ];
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        FFMPEG_LGPL = "${ffmpegLgpl}";
      };
    };
}
