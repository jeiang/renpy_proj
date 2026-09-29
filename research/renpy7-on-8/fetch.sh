#!/bin/sh
# Download the SDKs this ticket needs beyond the shared ones (gitignored sdk/). 8.0.1 and 8.5.3 come from
# research/shared-engine-launcher; unrpyc from research/rpyc-loading (see run_matrix.py / port_*.sh for paths).
set -eu
cd "$(dirname "$0")" && mkdir -p sdk && cd sdk
for v in 8.1.1 8.3.2; do
  [ -d renpy-$v-sdk ] || { curl -fLO https://www.renpy.org/dl/$v/renpy-$v-sdk.tar.bz2 && tar xjf renpy-$v-sdk.tar.bz2; }
done
