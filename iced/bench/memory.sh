#!/bin/sh
# Memory of each app with a drawing open: PSS (shared pages split fairly)
# summed over the whole process tree, after it has settled. WebKit runs in
# several processes; all of them count.
#
#   sh iced/bench/memory.sh [SCENE...]
#
# Both apps get a throwaway config directory, so the real sessions and
# autosave are not touched; for the iced app that also keeps it from handing
# the drawing to a copy already open. The Tauri app's single-instance plugin
# goes through D-Bus instead, so the script refuses to run while Excalidraw
# Desktop is open.
set -eu
cd "$(dirname "$0")/.."
tauri=../src-tauri/target/release/excalidraw-desktop
scenes=${*:-small 2k 10k}

pss_tree() {
  total=0
  for pid in $1 $(pgrep -P "$1" 2>/dev/null) $(for c in $(pgrep -P "$1" 2>/dev/null); do pgrep -P "$c" 2>/dev/null; done); do
    kb=$(awk '/^Pss:/{print $2}' "/proc/$pid/smaps_rollup" 2>/dev/null || echo 0)
    total=$((total + ${kb:-0}))
  done
  echo $((total / 1024))
}

if pgrep -x excalidraw-desk >/dev/null; then
  echo "Excalidraw Desktop is running; close it first." >&2
  exit 1
fi
for s in $scenes; do
  drawing=$(realpath "target/bench/$s.excalidraw")
  config=$(mktemp -d)
  XDG_CONFIG_HOME=$config ./target/release/excalidraw-iced "$drawing" >/dev/null 2>&1 &
  pid=$!
  sleep 6
  echo "$s: iced $(pss_tree $pid) MB"
  kill $pid
  sleep 1
  rm -rf "$config"
  config=$(mktemp -d)
  XDG_CONFIG_HOME=$config "$tauri" "$drawing" >/dev/null 2>&1 &
  pid=$!
  sleep 10
  echo "$s: Tauri $(pss_tree $pid) MB"
  kill $pid
  sleep 1
  rm -rf "$config"
done
