#!/usr/bin/env bash
# Exercises overlapping public openShare calls through real Quickshell Process instances.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

test_root="$FIXTURE_ROOT/flea-network-open-share-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/bin" "$test_root/home/.config" "$test_root/config" "$test_root/runtime"
chmod 700 "$test_root/runtime"
ln -s "$PWD/ui/NetworkMounts.qml" "$test_root/config/NetworkMounts.qml"
# The Service instantiates both of these, so a config directory without them resolves neither.
ln -s "$PWD/ui/MountListing.qml" "$test_root/config/MountListing.qml"
ln -s "$PWD/ui/NetworkPlaces.qml" "$test_root/config/NetworkPlaces.qml"
# NetworkMounts hosts the GVFS bridge, which opens this fixture's local paths at once and starts nothing.
ln -s "$PWD/ui/GvfsBridge.qml" "$test_root/config/GvfsBridge.qml"
ln -s "$PWD/ui/js" "$test_root/config/js"
ln -s "$PWD/tests/network-open-share.qml" "$test_root/config/shell.qml"
list_started="$test_root/list-started"

cat > "$test_root/bin/gio" <<'EOS'
#!/bin/sh
case "$1 $2" in
  "mount -li") exit 0 ;;
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

output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u QT_QPA_PLATFORMTHEME \
    FLEA_TEST_LIST_STARTED="$list_started" \
    HOME="$test_root/home" XDG_RUNTIME_DIR="$test_root/runtime" \
    PATH="$test_root/bin:/usr/bin:/bin" \
    QT_QPA_PLATFORM=offscreen \
    QT_FORCE_STDERR_LOGGING=1 \
    timeout 7 qs -p "$test_root/config" 2>&1)

pass_count=$(printf '%s\n' "$output" | grep -c 'NETWORK_OPEN_SHARE overlap=blocked sequential=open alias=child')
fail_count=$(printf '%s\n' "$output" | grep -c 'NETWORK_OPEN_SHARE FAIL')
if [ "$pass_count" -ne 1 ] || [ "$fail_count" -ne 0 ]; then
    printf 'FAIL overlapping openShare changed active share state\n%s\n' "$output"
    exit 1
fi

printf 'network-open-share: overlap blocked\n'

# Same service, now with the real bridge held inside its test -d helper. Both paths are fixture
# directories; no real GVFS daemon or share is contacted. The second open is local to the bridge,
# which admits it while the first waits, exposing request-state reuse deterministically.
mkdir -p "$test_root/runtime/gvfs/dav:host=first.example/space" "$test_root/second"
: > "$test_root/mounts"
ln -sf "$PWD/tests/network-alias.qml" "$test_root/config/shell.qml"
cat > "$test_root/bin/gio" <<'EOS'
#!/bin/sh
case "$1 $2" in
  "mount -li") exec /usr/bin/cat "$FLEA_TEST_ALIAS_ROOT/mounts" ;;
  "info davs://first.example/space")
    printf 'local path: %s/gvfs/dav:host=first.example/space\n' "$XDG_RUNTIME_DIR" ;;
  "info davs://second.example/vault")
    printf 'local path: %s/second\n' "$FLEA_TEST_ALIAS_ROOT" ;;
  *) exit 64 ;;
esac
EOS
cat > "$test_root/bin/test" <<'EOS'
#!/bin/sh
if [ "$1" = -d ] && [ "$2" = "$XDG_RUNTIME_DIR/gvfs/dav:host=first.example/space" ]; then
    : > "$FLEA_TEST_ALIAS_ROOT/check-started"
    timeout 7 sh -c 'while [ ! -e "$FLEA_TEST_ALIAS_ROOT/release-check" ]; do sleep 0.01; done' || exit 2
fi
exec /usr/bin/test "$@"
EOS
chmod +x "$test_root/bin/test"
output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u QT_QPA_PLATFORMTHEME \
    FLEA_TEST_ALIAS_ROOT="$test_root" \
    HOME="$test_root/home" XDG_RUNTIME_DIR="$test_root/runtime" \
    PATH="$test_root/bin:/usr/bin:/bin" \
    QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
    timeout 10 qs -p "$test_root/config" 2>&1)
pass_count=$(printf '%s\n' "$output" | grep -c 'NETWORK_ALIAS PASS delayed=isolated poll=saved renames=reactive reopen=saved')
if [ "$pass_count" -ne 1 ] || printf '%s\n' "$output" | grep -q 'NETWORK_ALIAS FAIL'; then
    printf 'FAIL network aliases changed across a bridge wait, poll or saved-place rename\n%s\n' "$output"
    exit 1
fi
printf 'network-open-share: delayed aliases isolated; poll, rename and reopen keep saved names\n'
