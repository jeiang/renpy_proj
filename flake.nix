{
  description = "renpy_proj: a Ren'Py 7/8 player (research + build)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  # Ren'Py 8.5.3 source (tag 8.5.3.26051504), pinned by flake.lock; the Nix build never clones.
  inputs.renpy-src = { url = "github:renpy/renpy/8.5.3.26051504"; flake = false; };

  outputs = { nixpkgs, renpy-src, ... }:
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
      # LGPL-only FFmpeg as a media generator for harness/testgames (never linked): the same GPL-free
      # build, with BSD-licensed encoders added (libvpx, libaom, libopus, libtheora, libwebp, OpenH264).
      ffmpegSynth = pkgs: (ffmpegLgpl pkgs).override {
        withVpx = true;
        withAom = true;
        withOpus = true;
        withTheora = true;
        withWebp = true;
        withOpenh264 = true;
      };
      # The pure player package is built for the two platforms of the M4 gate.
      playerSystems = [ "aarch64-darwin" "x86_64-linux" ];
      # The macOS app bundle names. Distinct, so both apps can sit in /Applications/Nix Apps together.
      urpAppName = "Universal Ren'Py Player";
      urpNorarAppName = "Universal Ren'Py Player (no RAR)";
      # `urp` carries the UnRAR licence (freeware, not free software), which check-meta rejects in
      # nixpkgs.legacyPackages. The player packages therefore come from a nixpkgs import that admits that one
      # package by name. Same nixpkgs revision, so every dependency is the same derivation as in legacyPackages.
      playerPkgs = system: import nixpkgs {
        inherit system;
        config.allowUnfreePredicate = pkg: nixpkgs.lib.getName pkg == "urp";
      };
      playerPackages = system:
        let
          pkgs = playerPkgs system;
          sources = import ./nix/sources.nix { inherit pkgs; };
          cpython = import ./nix/cpython.nix { inherit pkgs sources; };
          mkUrp = rar: import ./nix/player.nix {
            inherit pkgs sources cpython rar;
            ffmpegLgpl = ffmpegLgpl pkgs;
            renpySrc = renpy-src;
            renpyRev = renpy-src.rev;
          };
        in
        rec {
          player-cpython = cpython;
          # On macOS each one also carries Applications/<name>.app (nix/macos-app.nix). Elsewhere: the plain build.
          onMac = raw: name: bundleId:
            if pkgs.stdenv.hostPlatform.isDarwin
            then import ./nix/macos-app.nix { inherit pkgs raw name bundleId; }
            else raw;
          # Default cargo features: RAR support (UnRAR, unfree-redistributable).
          urp = onMac (mkUrp true) urpAppName "io.github.jeiang.renpy-player";
          # `--no-default-features --features wayland`: no UnRAR, free software only.
          urp-norar = onMac (mkUrp false) urpNorarAppName "io.github.jeiang.renpy-player.norar";
          default = urp;
        };
    in
    {
      packages = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in
        {
          ffmpeg-lgpl = ffmpegLgpl pkgs;
          ffmpeg-synth = ffmpegSynth pkgs;
        } // nixpkgs.lib.optionalAttrs (builtins.elem system playerSystems) (playerPackages system));

      checks = nixpkgs.lib.genAttrs playerSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          packages = playerPackages system;
        in
        {
          urp-smoke = import ./nix/smoke.nix { inherit pkgs; urp = packages.urp; name = "urp-smoke"; appName = urpAppName; };
          urp-norar-smoke = import ./nix/smoke.nix { inherit pkgs; urp = packages.urp-norar; name = "urp-norar-smoke"; appName = urpNorarAppName; };
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
            ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (with pkgs; [
              # Linux (M4): CPython build tools, link-time audio and input libs, and the libraries
              # that wgpu, winit and the Wayland/X11 stack open at run time.
              gnumake
              patchelf
              curl
              xz
              alsa-lib # cpal (ALSA, reaches PipeWire through its plugin)
              systemdLibs # libudev for gilrs
              vulkan-loader
              wayland
              libxkbcommon
              libx11
              libxcursor
              libxrandr
              libxi
              libxcb
              libdrm # libva-drm (VA-API)
            ]);
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            FFMPEG_LGPL = "${ffmpegLgpl pkgs}";
            # Linux: the loader and window-system libraries that are opened with dlopen. The GPU drivers
            # come from the host (/run/opengl-driver on NixOS; the system ones elsewhere).
            PLAYER_DLOPEN_RPATH = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux "${pkgs.lib.makeLibraryPath (with pkgs; [ vulkan-loader wayland libxkbcommon libx11 libxcursor libxrandr libxi libxcb alsa-lib systemdLibs libdrm ])}:/run/opengl-driver/lib";
            shellHook = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isDarwin ''
              # `libclang` puts the unwrapped clang first on PATH, and CXX=clang++ then finds no libc++
              # headers (the unrar crate compiles C++). The wrapped compilers are cc and c++.
              export CC=cc CXX=c++
            '' + pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
              export BINDGEN_EXTRA_CLANG_ARGS="-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.lib.versions.major pkgs.llvmPackages.libclang.version}/include -isystem ${pkgs.glibc.dev}/include"
              export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (with pkgs; [ vulkan-loader wayland libxkbcommon libx11 libxcursor libxrandr libxi libxcb alsa-lib systemdLibs libdrm ])}:/run/opengl-driver/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            '';
          };
        });
    };
}
