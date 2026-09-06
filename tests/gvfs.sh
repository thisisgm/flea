#!/usr/bin/env bash
# Proves Flea's test fixture crosses the real GIO, GVfs, FUSE, and unmount boundaries.
set -u
set -o pipefail
cd "$(dirname "$0")/.." || exit 1

fixture_dir="${FLEA_FIXTURE_ROOT:-/home/flea-sandbox}/flea-gvfs-suite-$$"

cleanup() {
    DIR="$fixture_dir" ./tools/flea-gvfs-fixture clean >/dev/null 2>&1 || true
}
trap cleanup EXIT HUP INT TERM

fail() {
    printf 'gvfs: FAIL %s\n' "$*" >&2
    exit 1
}

uri=$(DIR="$fixture_dir" ./tools/flea-gvfs-fixture make) || fail "fixture creation"
DIR="$fixture_dir" ./tools/flea-gvfs-fixture mount || fail "mount"

gio mount -l | grep -Fq -- "-> $uri" || fail "mounted URI absent from gio mount -l"
listing=$(gio list "$uri" | sort)
[[ "$listing" == $'alpha.txt\nbeta.txt' ]] || fail "unexpected listing: $listing"
[[ "$(gio cat "${uri}alpha.txt")" == "alpha" ]] || fail "alpha.txt content"

local_path=$(gio info "$uri" | sed -n 's/^local path: //p')
[[ "$local_path" == "/run/user/$(id -u)/gvfs/"* ]] || fail "unexpected FUSE path: $local_path"
[[ -f "$local_path/alpha.txt" ]] || fail "FUSE path cannot read alpha.txt"

DIR="$fixture_dir" ./tools/flea-gvfs-fixture unmount || fail "unmount"
! gio mount -l | grep -Fq -- "-> $uri" || fail "URI survived unmount"

printf 'gvfs: mount=ok list=ok read=ok fuse=ok unmount=ok\n'
