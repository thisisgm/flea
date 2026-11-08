#!/usr/bin/env bash
# Run the directory-dialog New Folder regressions headless, and require their count.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/picker-new-folder"
unit_root=$(mktemp -d "$PWD/.superpowers/picker-new-folder/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime"
chmod 700 "$unit_root/runtime"
log="$unit_root/new-folder.log"
# A filter that matched nothing would pass with zero, so the count below is required.
set +e
setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
    XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
    XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    cargo test --locked -p flea-gui --lib g2::pk1::new_folder -- --nocapture \
    < /dev/null > "$log" 2>&1
rc=$?
set -e
[ "$rc" -eq 0 ] || { tail -60 "$log" >&2; printf 'picker-new-folder: FAIL tests exited %s\n' "$rc"; exit 1; }
count=$(sed -n 's/^test result: ok\. \([0-9]*\) passed; 0 failed.*/\1/p' "$log" | head -1)
expected=18
[ "${count:-0}" -eq "$expected" ] || { tail -40 "$log" >&2; printf 'picker-new-folder: FAIL only %s of %s checks ran\n' "${count:-0}" "$expected"; exit 1; }
printf 'PICKER-NEW-FOLDER PASS %s checks\n' "$count"
