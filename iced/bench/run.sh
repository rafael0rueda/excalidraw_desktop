#!/bin/sh
# The phase 1 gate (docs/iced-port/PLAN.md): the same pan-then-zoom animation
# on the iced viewer and on Excalidraw 0.18.1 in WebKitGTK, per scene.
#
#   node iced/bench/gen-scenes.mjs && node iced/bench/webkit/build.mjs
#   cargo build --release -p excalidraw-iced --manifest-path iced/Cargo.toml
#   sh iced/bench/run.sh [SECONDS] [SCENE...]
set -eu
cd "$(dirname "$0")/.."
seconds=${1:-10}
[ $# -gt 0 ] && shift
scenes=${*:-small 2k 10k}
for s in $scenes; do
  drawing=target/bench/$s.excalidraw
  bounds=$(./target/release/excalidraw-iced --screenshot /dev/null "$drawing" 2>&1 | awk '/^bounds/{print $2, $3, $4, $5}')
  echo "=== $s: iced"
  ./target/release/excalidraw-iced --bench "$seconds" "$drawing" 2>&1 | grep -E "interval|CPU|first frame|load\+prepare"
  echo "=== $s: Excalidraw in WebKitGTK"
  # shellcheck disable=SC2086
  python3 bench/webkit.py bench "$drawing" "$seconds" $bounds 2>&1 | grep -E "interval|CPU|loaded|canvas"
done
