#!/usr/bin/env bash
# Drives ui/ShareResolve.qml, the chooser's mount-and-info legs, against a gio stub: a share that
# mounts, one that wants a password, one the rail already mounted, an sftp host, a refusal, a mount
# with no FUSE path, a leg that misses its deadline, and a second line while one is still running.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

test_root="$FIXTURE_ROOT/flea-share-resolve-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/bin" "$test_root/home" "$test_root/config"
ln -s "$PWD/ui/ShareResolve.qml" "$test_root/config/ShareResolve.qml"
ln -s "$PWD/ui/js" "$test_root/config/js"
ln -s "$PWD/tests/share-resolve.qml" "$test_root/config/shell.qml"
operations="$test_root/operations"
: > "$operations"

# Named from the product's own command. Every call is logged so the shape of each leg is asserted
# below, and the wording is gio's own under the C locale the resolver pins.
cat > "$test_root/bin/gio" <<'EOS'
#!/bin/sh
printf '%s\n' "$*" >> "$FLEA_TEST_OPERATIONS"
[ "$LC_ALL" = C ] || { echo "gio: not pinned to C" >&2; exit 64; }
case "$*" in
  "mount --anonymous smb://nas/isos") exit 0 ;;
  "info smb://nas/isos") printf 'uri: smb://nas/isos/\nlocal path: /gvfs/isos\n' ;;
  "mount --anonymous smb://nas/locked")
    echo "gio: smb://nas/locked/: Failed to mount Windows share: Permission denied" >&2; exit 1 ;;
  "info smb://nas/locked") echo "gio: smb://nas/locked/: The specified location is not mounted" >&2; exit 1 ;;
  "mount --anonymous smb://nas/live")
    echo "gio: smb://nas/live/: Location is already mounted" >&2; exit 1 ;;
  "info smb://nas/live") printf 'local path: /gvfs/live\n' ;;
  "mount sftp://box/") exit 0 ;;
  "info sftp://box/") printf 'local path: /gvfs/box\n' ;;
  "mount --anonymous smb://nas/gone")
    echo "gio: smb://nas/gone/: Failed to mount Windows share: No such file or directory" >&2; exit 1 ;;
  "info smb://nas/gone") exit 1 ;;
  "mount --anonymous smb://nas/nopath") exit 0 ;;
  "info smb://nas/nopath") printf 'uri: smb://nas/nopath/\n' ;;
  "mount --anonymous smb://nas/slow") exit 0 ;;
  "info smb://nas/slow") sleep 3; printf 'local path: /gvfs/slow\n' ;;
  *) echo "gio: unexpected $*" >&2; exit 64 ;;
esac
EOS
chmod +x "$test_root/bin/gio"

output=$(env \
    FLEA_TEST_OPERATIONS="$operations" \
    HOME="$test_root/home" \
    PATH="$test_root/bin:/usr/bin:/bin" \
    QT_QPA_PLATFORM=offscreen \
    QT_FORCE_STDERR_LOGGING=1 \
    timeout 9 qs -p "$test_root/config" 2>&1)

pass_count=$(printf '%s\n' "$output" | grep -c 'SHARE_RESOLVE PASS steps=8')
fail_count=$(printf '%s\n' "$output" | grep -c 'SHARE_RESOLVE FAIL')
busy_count=$(printf '%s\n' "$output" | grep -c 'SHARE_RESOLVE busy smb://nas/overlap')
if [ "$pass_count" -ne 1 ] || [ "$fail_count" -ne 0 ] || [ "$busy_count" -ne 1 ]; then
    printf 'FAIL share-resolve: pass=%s fail=%s busy=%s\n%s\n' "$pass_count" "$fail_count" "$busy_count" "$output"
    exit 1
fi
# The overlapping line never reached gio, and the timed-out leg was not followed by another.
if grep -q 'overlap' "$operations"; then
    printf 'FAIL share-resolve: the busy line started a gio leg\n'; cat "$operations"; exit 1
fi
slow_calls=$(grep -c 'smb://nas/slow' "$operations")
if [ "$slow_calls" -ne 2 ]; then
    printf 'FAIL share-resolve: the slow root ran %s gio legs, not 2\n' "$slow_calls"; cat "$operations"; exit 1
fi

printf 'share-resolve: 8 roots answered, overlap refused, deadline kept\n'
