#!/usr/bin/env bash
# Re-downloads the primary licence texts cited in README.md into upstream/ (gitignored). Not committed: third-party text.
set -u
cd "$(dirname "$0")"; mkdir -p upstream; cd upstream
g() { curl -fsSL "$2" -o "$1" || echo "FAILED $1"; }
g ffmpeg-LICENSE.md   https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/LICENSE.md
g ffmpeg-legal.html   https://ffmpeg.org/legal.html
g ffmpeg-configure    https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/configure
g dav1d-COPYING       https://raw.githubusercontent.com/videolan/dav1d/master/COPYING
g libvpx-LICENSE      https://raw.githubusercontent.com/webmproject/libvpx/main/LICENSE
g libvpx-PATENTS      https://raw.githubusercontent.com/webmproject/libvpx/main/PATENTS
g wgpu-LICENSE.APACHE https://raw.githubusercontent.com/gfx-rs/wgpu/trunk/LICENSE.APACHE
g wgpu-LICENSE.MIT    https://raw.githubusercontent.com/gfx-rs/wgpu/trunk/LICENSE.MIT
g wgpu-Cargo.toml     https://raw.githubusercontent.com/gfx-rs/wgpu/trunk/Cargo.toml
g freetype-README   https://raw.githubusercontent.com/freetype/freetype/master/docs/README
g freetype-FTL https://raw.githubusercontent.com/freetype/freetype/master/docs/FTL.TXT
g dav1d-PATENTS https://raw.githubusercontent.com/videolan/dav1d/master/doc/PATENTS
g pbs-running.rst https://raw.githubusercontent.com/astral-sh/python-build-standalone/main/docs/running.rst
g pbs-quirks.rst https://raw.githubusercontent.com/astral-sh/python-build-standalone/main/docs/quirks.rst
g harfbuzz-README https://raw.githubusercontent.com/harfbuzz/harfbuzz/main/README.md
g harfbuzz-COPYING    https://raw.githubusercontent.com/harfbuzz/harfbuzz/main/COPYING
g fribidi-COPYING     https://raw.githubusercontent.com/fribidi/fribidi/master/COPYING
g sdl-LICENSE         https://raw.githubusercontent.com/libsdl-org/SDL/main/LICENSE.txt
g psf-LICENSE         https://raw.githubusercontent.com/python/cpython/3.12/LICENSE
g opus-COPYING        https://raw.githubusercontent.com/xiph/opus/main/COPYING
g gamecontrollerdb-LICENSE https://raw.githubusercontent.com/mdqinc/SDL_GameControllerDB/master/LICENSE
g twemoji-LICENSE-GRAPHICS https://raw.githubusercontent.com/jdecked/twemoji/main/LICENSE-GRAPHICS
g openh264-LICENSE    https://raw.githubusercontent.com/cisco/openh264/master/LICENSE
g x264-COPYING        https://raw.githubusercontent.com/mirror/x264/master/COPYING
for c in wgpu naga str0m webrtc webrtc-rs rtc ffmpeg-next ffmpeg-sys-next rsmpeg pyo3 winit sdl2 sdl3 cpal rodio symphonia kira image ttf-parser swash cosmic-text rustybuzz harfrust fontdue ab_glyph fontdb skrifa read-fonts glsl-lang glslang unicode-bidi unicode-linebreak unicode-segmentation opus audiopus magnetic fdk-aac libvpx-sys dav1d dav1d-sys rav1d webrtc-audio-processing rcgen openssl ring aws-lc-rs dimpl vk-video shiguredo_webrtc gstreamer gstreamer-webrtc gst-plugin-webrtc; do
  curl -fsSL "https://crates.io/api/v1/crates/$c" -A "renpy_proj-licence-audit" -o "crate-$c.json" 2>/dev/null || echo "no crate $c"
done
