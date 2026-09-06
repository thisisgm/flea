#!/usr/bin/env bash
# A share that goes away between two polls must leave the listing empty. onExited falls back to the
# text onStreamFinished cached, and that cache used to survive the poll that replaced it, so an
# empty listing could be answered with the shares the poll before it found.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

test_root="$FIXTURE_ROOT/flea-mount-listing-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/bin" "$test_root/home" "$test_root/config"
ln -s "$PWD/ui/MountListing.qml" "$test_root/config/MountListing.qml"
ln -s "$PWD/tests/mount-listing.qml" "$test_root/config/shell.qml"
poll_count="$test_root/poll-count"

# One share on the first "gio mount -l" and nothing after that, which is what a share being
# unmounted between two polls looks like: no output at all, and still exit 0.
cat > "$test_root/bin/gio" <<'EOS'
#!/bin/sh
[ "$1 $2" = "mount -l" ] || exit 64
count=$(cat "$FLEA_TEST_POLL_COUNT" 2>/dev/null || echo 0)
echo $((count + 1)) > "$FLEA_TEST_POLL_COUNT"
[ "$count" -eq 0 ] || exit 0
cat <<'EOM'
Drive(0): fixture
  Type: GProxyDrive (GProxyVolumeMonitorGoa)
Mount(0): share on fixture -> smb://fixture/share/
  Type: GDaemonMount
EOM
EOS
chmod +x "$test_root/bin/gio"

output=$(env \
    FLEA_TEST_POLL_COUNT="$poll_count" \
    HOME="$test_root/home" \
    PATH="$test_root/bin:/usr/bin:/bin" \
    QT_QPA_PLATFORM=offscreen \
    QT_FORCE_STDERR_LOGGING=1 \
    timeout 12 qs -p "$test_root/config" 2>&1)

pass_count=$(printf '%s\n' "$output" | grep -c 'MOUNT_LISTING populated=share empty=none carried=none')
fail_count=$(printf '%s\n' "$output" | grep -c 'MOUNT_LISTING FAIL')
if [ "$pass_count" -ne 1 ] || [ "$fail_count" -ne 0 ]; then
    printf 'FAIL an emptied listing did not report empty\n%s\n' "$output"
    exit 1
fi

printf 'mount-listing: populated then empty reports empty, and carries nothing over\n'
