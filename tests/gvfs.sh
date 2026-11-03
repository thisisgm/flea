#!/usr/bin/env bash
# Proves Flea's test fixture crosses the real GIO, GVfs, FUSE, and unmount boundaries.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1
. ./tools/flea-sandbox-guard
sandbox_root_ok
mkdir -p "$SANDBOX_ROOT"
fixture_dir=$(mktemp -d "$SANDBOX_ROOT/flea-gvfs-suite.XXXXXXXX")
: > "$fixture_dir/$SANDBOX_MARKER"
chmod 700 "$fixture_dir"

cleanup() {
    local status=$?
    trap - EXIT
    if (( status >= 128 )) || mountpoint -q "$fixture_dir/runtime/gvfs"; then
        printf 'gvfs: FAIL retaining interrupted or mounted fixture: %s\n' "$fixture_dir" >&2
        exit 1
    fi
    sandbox_remove "$fixture_dir" || status=1
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

python3 -B tests/lib/gvfs_fixture.py "$fixture_dir"
printf 'gvfs: mount=ok list=ok read=ok fuse=ok unmount=ok\n'
