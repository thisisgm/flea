#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Issue 315: a filter over a folder bigger than the held window finds a name the window does not hold, draws its row, and Esc parks the listing on the cursor; the real Pane over the real backend, offscreen.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

if ! command -v qs >/dev/null; then
    echo "filter-folder.sh: qs is not installed, cannot build the views"
    exit 1
fi
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[ -x "$bin" ] || { echo "filter-folder.sh: build the backend first: $bin"; exit 1; }

sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-filter-folder.XXXXXX") || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/runtime" "$test_root/fixture" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/filter-folder.qml "$test_root/config/shell.qml" || exit 1
# The issue's own reproducer: 5,000 names, of which a window holds a screenful and a buffer each side.
( cd "$test_root/fixture" && seq -f 'file-%04g.txt' 1 5000 | xargs touch ) || exit 1

output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
    FLEA_BIN="$bin" FILTERFOLDER_DIR="$test_root/fixture" timeout 90 qs -p "$test_root/config" 2>&1)

# Sample input, the receipt: "  INFO qml: FILTERFOLDER PASS held=330 shown=1 row=file-2345.txt strip=1 of 5,000 shown ...".
pass_count=$(printf '%s\n' "$output" | grep -c 'FILTERFOLDER PASS')
done_count=$(printf '%s\n' "$output" | grep -c 'FILTERFOLDER DONE')
noisy=$(printf '%s\n' "$output" | grep -av 'does not support setting window masks' | grep -aiE 'WARN|ERROR|Binding loop|failed to load|Unable to assign|Cannot assign|TypeError|ReferenceError|is not a function|is not defined')
if [ "$pass_count" -ne 1 ] || [ "$done_count" -ne 1 ] || printf '%s\n' "$output" | grep -aq 'FILTERFOLDER FAIL' || [ -n "$noisy" ]; then
    printf 'FAIL the filter did not cover the folder, or Esc left the listing blank, or the views warned\n'
    printf '%s\n' "$output" | grep -a 'FILTERFOLDER'
    [ -n "$noisy" ] && printf '%s\n' "$noisy" | head -20
    exit 1
fi
printf '%s\n' "$output" | grep -a 'FILTERFOLDER PASS'
