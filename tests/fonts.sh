#!/usr/bin/env bash
# Real font install/uninstall under isolated user roots, followed by the shipped Quick Look and actual QML controls offscreen.
set -euo pipefail
cd "$(dirname "$0")/.."
. tools/flea-sandbox-guard
. tests/headless-commons.sh
for tool in qs fc-match fc-list fc-scan fc-cache timeout python3 magick bwrap; do
    command -v "$tool" >/dev/null || { echo "FAIL fonts: missing $tool"; exit 1; }
done
python3 tests/font-management.py
python3 tests/font-defaults.py
scratch="$FIXTURE_ROOT/flea-fonts-$$"
sandbox_make "$scratch"
trap 'sandbox_remove "$scratch"' EXIT
mkdir -p "$scratch"/{config,home,data,cache,state,runtime}
chmod 700 "$scratch/runtime"
python3 tests/font-formats.py "$scratch"
ln -s "$PWD/ui" "$scratch/config/flea"
headless_commons "$scratch"
ln -s "$PWD/ui/boot/Ui" "$scratch/config/Ui"
cp tests/font-preview.qml "$scratch/config/shell.qml"
cp "$(fc-match -f '%{file}')" "$scratch/a # font.ttf"
env -u WAYLAND_DISPLAY -u DISPLAY HOME="$scratch/home" XDG_DATA_HOME="$scratch/data" \
    XDG_CACHE_HOME="$scratch/cache" XDG_STATE_HOME="$scratch/state" XDG_RUNTIME_DIR="$scratch/runtime" \
    FONT_FIXTURE="$scratch/a # font.ttf" FLEA_BIN="$PWD/target/debug/flea" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software \
    timeout 55 qs -p "$scratch/config" > "$scratch/log" 2>&1 || status=$?
case ${status:-0} in 0|143) ;; *) cat "$scratch/log"; exit 1;; esac
cat "$scratch/log"
grep -q 'FONT DONE checks=.* failed=0' "$scratch/log"
if grep -E 'FONT FAIL|TypeError|ReferenceError|Binding loop|Unable to assign|Cannot load font|Failed to load|is not a type' "$scratch/log"; then exit 1; fi

# A second process drives the actual WindowBody and backend, with two fonts and no user files.
ln -s "$PWD/ui/boot/fleatab.qml" "$scratch/config/fleatab.qml"
cp tests/font-pane.qml "$scratch/config/shell.qml"
mkdir -p "$scratch/fixture"
cp "$scratch/a # font.ttf" "$scratch/fixture/a.ttf"
otf=$(fc-list -f '%{file}\n' | awk '/\.otf$/ && !font {font=$0} END {print font}')
second=b.ttf
if [ -n "$otf" ]; then second=b.otf; cp "$otf" "$scratch/fixture/$second"; else cp "$scratch/a # font.ttf" "$scratch/fixture/$second"; fi
status=0
env -u WAYLAND_DISPLAY -u DISPLAY HOME="$scratch/home" XDG_DATA_HOME="$scratch/data" \
    XDG_CACHE_HOME="$scratch/cache" XDG_STATE_HOME="$scratch/state" XDG_RUNTIME_DIR="$scratch/runtime" \
    FONT_SECOND="$second" FLEA_PATH="$scratch/fixture" FLEA_BIN="$PWD/target/debug/flea" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software \
    timeout 45 qs -p "$scratch/config" > "$scratch/pane.log" 2>&1 || status=$?
case $status in 0|143) ;; *) cat "$scratch/pane.log"; exit 1;; esac
cat "$scratch/pane.log"
grep -q 'FONT DONE checks=.* failed=0' "$scratch/pane.log"
if grep -E 'FONT FAIL|TypeError|ReferenceError|Binding loop|Unable to assign|Cannot load font|Failed to load|is not a type' "$scratch/pane.log"; then exit 1; fi

# Every real container above also reaches the actual Qt loader, collection selector and variable-axis binding.
cp tests/font-formats.qml "$scratch/config/shell.qml"
status=0
env -u WAYLAND_DISPLAY -u DISPLAY HOME="$scratch/home" XDG_DATA_HOME="$scratch/data" \
    XDG_CACHE_HOME="$scratch/cache" XDG_STATE_HOME="$scratch/state" XDG_RUNTIME_DIR="$scratch/runtime" \
    FONT_CASES="$(cat "$scratch/format-cases.json")" FLEA_BIN="$PWD/target/debug/flea" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software \
    timeout 70 qs -p "$scratch/config" > "$scratch/formats.log" 2>&1 || status=$?
case $status in 0|143) ;; *) cat "$scratch/formats.log"; exit 1;; esac
cat "$scratch/formats.log"
grep -q 'FONT DONE checks=.* failed=0' "$scratch/formats.log"
if grep -E 'FONT FAIL|TypeError|ReferenceError|Binding loop|Unable to assign|Cannot load font|Failed to load|is not a type' "$scratch/formats.log"; then exit 1; fi

# Defaults buttons call the shipped CLI under private config/state roots.
cp tests/font-defaults.qml "$scratch/config/shell.qml"
status=0
env -u WAYLAND_DISPLAY -u DISPLAY HOME="$scratch/home" XDG_CONFIG_HOME="$scratch/home/.config" \
    XDG_DATA_HOME="$scratch/data" XDG_CACHE_HOME="$scratch/cache" XDG_STATE_HOME="$scratch/state" \
    XDG_RUNTIME_DIR="$scratch/runtime" GSETTINGS_BACKEND=keyfile \
    FONT_DEFAULT_FIXTURE="$(fc-match monospace -f '%{file}')" FLEA_BIN="$PWD/target/debug/flea" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software \
    timeout 45 qs -p "$scratch/config" > "$scratch/defaults.log" 2>&1 || status=$?
case $status in 0|143) ;; *) cat "$scratch/defaults.log"; exit 1;; esac
cat "$scratch/defaults.log"
grep -q 'FONT DONE checks=.* failed=0' "$scratch/defaults.log"
if grep -E 'FONT FAIL|TypeError|ReferenceError|Binding loop|Unable to assign|Failed to load|is not a type' "$scratch/defaults.log"; then exit 1; fi
