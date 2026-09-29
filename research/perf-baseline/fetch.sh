#!/bin/sh
# Download the Ren'Py 7.8.2 SDK this ticket needs (gitignored sdk/). 8.0.1 and 8.5.3 come from research/shared-engine-launcher, 8.2.3 from research/test-corpus.
set -eu
cd "$(dirname "$0")" && mkdir -p sdk && cd sdk
[ -d renpy-7.8.2-sdk ] || { curl -fLO https://www.renpy.org/dl/7.8.2/renpy-7.8.2-sdk.tar.bz2 && tar xjf renpy-7.8.2-sdk.tar.bz2; }
