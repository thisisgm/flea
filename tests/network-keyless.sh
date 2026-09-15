#!/usr/bin/env bash
# A public key mounts an sftp place with no password, and only a refused mount asks for one.
# 0.2.1 demanded a credential for every sftp://user@host before it ever ran gio, so no key could
# ever open one; see AGENTS.md "A public key mounts sftp, and a password is asked only after".
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

test_root="$FIXTURE_ROOT/flea-network-keyless-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/bin" "$test_root/home/.config" "$test_root/config" "$test_root/state"
ln -s "$PWD/ui/NetworkMounts.qml" "$test_root/config/NetworkMounts.qml"
# The Service instantiates both of these, so a config directory without them resolves neither.
ln -s "$PWD/ui/MountListing.qml" "$test_root/config/MountListing.qml"
ln -s "$PWD/ui/NetworkPlaces.qml" "$test_root/config/NetworkPlaces.qml"
ln -s "$PWD/ui/js" "$test_root/config/js"
ln -s "$PWD/tests/network-keyless.qml" "$test_root/config/shell.qml"
helper_log="$test_root/state/helper.log"
: > "$helper_log"

# The credentialed route, which a passwordless mount must never reach.
cat > "$test_root/bin/flea-gio-auth" <<'EOS'
#!/bin/sh
printf 'launched\n' >> "$FLEA_TEST_HELPER_LOG"
exit 1
EOS
chmod +x "$test_root/bin/flea-gio-auth"

cat > "$test_root/bin/gio" <<'EOS'
#!/bin/sh
case "$1 ${2:-}" in
  "mount -l") exit 0 ;;
  # A key authenticates this one, the way gvfsd-sftp's own ssh does, so no prompt ever appears.
  "mount sftp://key@slot.test/home") exit 0 ;;
  "info sftp://key@slot.test/home") printf 'local path: %s\n' "$FLEA_TEST_KEY_PATH" ;;
  # A server root that a password would open: no path of its own, and no shares without one.
  "list sftp://ask@slot.test/") exit 2 ;;
  # Everything else wants a password: measured against a real host, gio answers this in 171 ms
  # with exit 2 rather than waiting on a stdin nobody is reading.
  *) exit 2 ;;
esac
EOS
chmod +x "$test_root/bin/gio"

output=$(env \
    FLEA_TEST_KEY_PATH="/key-should-open" \
    FLEA_TEST_HELPER_LOG="$helper_log" \
    FLEA_GIO_AUTH="$test_root/bin/flea-gio-auth" \
    HOME="$test_root/home" \
    PATH="$test_root/bin:/usr/bin:/bin" \
    QT_QPA_PLATFORM=offscreen \
    QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$test_root/config" 2>&1)

pass_count=$(printf '%s\n' "$output" | grep -c 'NETWORK_KEYLESS passwordless=open needs-password=asked bare-root=asked remembered=kept')
fail_count=$(printf '%s\n' "$output" | grep -c 'NETWORK_KEYLESS FAIL')
if [ "$pass_count" -ne 1 ] || [ "$fail_count" -ne 0 ]; then
    printf 'FAIL sftp keyless mount changed its answer\n%s\n' "$output"
    exit 1
fi
if [ -s "$helper_log" ]; then
    printf 'FAIL a passwordless mount launched the credential helper\n'
    exit 1
fi

printf 'network-keyless: passwordless opens, only a refused mount asks\n'
