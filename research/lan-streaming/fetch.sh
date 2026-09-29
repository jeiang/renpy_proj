#!/usr/bin/env bash
# Re-fetch the upstream docs quoted in README.md into upstream/ (gitignored; third-party text, not vendored).
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p upstream
get() { gh api -H 'Accept: application/vnd.github.raw' "repos/$1/contents/$2" > "upstream/$3"; }
get LizardByte/Sunshine README.md Sunshine.README.md
get LizardByte/Sunshine docs/configuration.md sunshine_configuration.md
get LizardByte/Sunshine docs/getting_started.md sunshine_getting_started.md
get MrCreativ3001/moonlight-web-stream README.md moonlight-web-stream.README.md
get m1k1o/neko README.md neko.README.md
get selkies-project/selkies README.md selkies.README.md
get GStreamer/gst-plugins-rs net/webrtc/README.md webrtcsink.README.md
