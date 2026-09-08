#!/bin/bash
# Real FileView reads and writes in an isolated home, including the first save with no GTK
# directory. An external write between edits proves that removing a favorite preserves it.
set -eu
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
test_root="$FIXTURE_ROOT/flea-favorites-$$"
sandbox_make "$test_root"
trap 'sandbox_remove "$test_root"' EXIT
mkdir -p "$test_root/home" "$test_root/config" "$test_root/runtime"
ln -s "$PWD/ui/FavoritePlaces.qml" "$test_root/config/FavoritePlaces.qml"
ln -s "$PWD/ui/js" "$test_root/config/js"
ln -s "$PWD/tests/favorites.qml" "$test_root/config/shell.qml"
status=0
output=$(HOME="$test_root/home" XDG_RUNTIME_DIR="$test_root/runtime" QT_QPA_PLATFORM=offscreen \
    QT_QPA_PLATFORMTHEME=basic QT_FORCE_STDERR_LOGGING=1 timeout 10 qs -p "$test_root/config" 2>&1) || status=$?
if [[ "$status" != 0 && "$status" != 143 ]]; then
    printf '%s\n' "$output"
    exit 1
fi
printf '%s\n' "$output"
[[ "$output" == *"favorites: 0 failed"* && "$output" != *"FAIL "* ]]
