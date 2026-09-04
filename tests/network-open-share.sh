#!/usr/bin/env bash
# Exercises overlapping public openShare calls through real Quickshell Process instances.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

test_root="$FIXTURE_ROOT/flea-network-open-share-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/bin" "$test_root/home/.config" "$test_root/config"
ln -s "$PWD/ui/NetworkMounts.qml" "$test_root/config/NetworkMounts.qml"
ln -s "$PWD/ui/js" "$test_root/config/js"
ln -s "$PWD/tests/network-open-share.qml" "$test_root/config/shell.qml"
list_started="$test_root/list-started"

cat > "$test_root/bin/gio" <<'EOS'
#!/bin/sh
case "$1 $2" in
  "mount -l") exit 0 ;;
  "info smb://first/") exit 0 ;;
  "list smb://first/")
    : > "$FLEA_TEST_LIST_STARTED"
    sleep 1
    printf 'first-share\n'
    ;;
  "info smb://second/")
    printf 'local path: /second-overlap\n'
    ;;
  "mount --anonymous")
    [ "$3" = "smb://first/child/" ]
    ;;
  "info smb://first/child")
    printf 'local path: /child-should-open\n'
    ;;
  *) exit 64 ;;
esac
EOS
chmod +x "$test_root/bin/gio"

output=$(env \
    FLEA_TEST_LIST_STARTED="$list_started" \
    HOME="$test_root/home" \
    PATH="$test_root/bin:/usr/bin:/bin" \
    QT_QPA_PLATFORM=offscreen \
    QT_FORCE_STDERR_LOGGING=1 \
    timeout 7 qs -p "$test_root/config" 2>&1)

pass_count=$(printf '%s\n' "$output" | grep -c 'NETWORK_OPEN_SHARE overlap=blocked sequential=open')
fail_count=$(printf '%s\n' "$output" | grep -c 'NETWORK_OPEN_SHARE FAIL')
if [ "$pass_count" -ne 1 ] || [ "$fail_count" -ne 0 ]; then
    printf 'FAIL overlapping openShare changed active share state\n%s\n' "$output"
    exit 1
fi

printf 'network-open-share: overlap blocked\n'
