#!/usr/bin/env bash
# Exercise the real backend on a private bus with a marked Trash fixture.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -B tests/empty-trash-deadline.py
repo=$(pwd -P)
mkdir -p "$repo/.superpowers/b-empty-trash"
root=$(mktemp -d "$repo/.superpowers/b-empty-trash/test.XXXXXXXX")
case "$root" in "$repo/.superpowers/b-empty-trash/test."?*) ;; *) exit 2 ;; esac
: > "$root/.flea-test-sandbox"
cleanup() {
    case "${root:-}" in
        "$repo/.superpowers/b-empty-trash/test."?*)
            [[ -n "$root" && "$root" = /* && -f "$root/.flea-test-sandbox" ]] && rm -rf -- "$root"
            ;;
    esac
}
trap cleanup EXIT
mkdir -p "$root/home" "$root/data" "$root/runtime" "$root/tmp" "$root/cache" "$root/config"
chmod 700 "$root/runtime"
env HOME="$root/home" XDG_DATA_HOME="$root/data" XDG_RUNTIME_DIR="$root/runtime" \
    GVFS_DISABLE_FUSE=1 XDG_CACHE_HOME="$root/cache" XDG_CONFIG_HOME="$root/config" TMPDIR="$root/tmp" \
    dbus-run-session -- python3 tests/empty-trash.py "$repo/target/debug/flea" "$root"
