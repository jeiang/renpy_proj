#!/bin/sh
# Which video decoders are linked into the Ren'Py 8.5.3 SDK's engine library (ffmpeg is static inside librenpython).
# Usage: ffmpeg_decoders.sh [SDK_DIR]   (macOS universal build; same for the linux/windows libs by name)
SDK=${1:-/Users/aidanp/Projects/renpy_proj/research/shared-engine-launcher/sdk/renpy-8.5.3-sdk}
grep -a -o "ff_[a-z0-9_]*_decoder" "$SDK/lib/py3-mac-universal/librenpython.dylib" | sort -u | tr '\n' ' '; echo
echo -n "h264 decoder linked: "; grep -a -c "ff_h264_decoder" "$SDK/lib/py3-mac-universal/librenpython.dylib" || true
