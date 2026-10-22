#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# The rename editor's frame in every host (list, dual, columns, grid, rail) against its row, in the real WindowBody, offscreen.
set -u
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
command -v qs >/dev/null || { echo 'FAIL rename-frame needs qs'; exit 1; }
command -v dbus-run-session >/dev/null || { echo 'FAIL rename-frame needs a private D-Bus session'; exit 1; }
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL rename-frame needs the built backend'; exit 1; }
sandbox_root_ok
probe=$(mktemp -d "$SANDBOX_ROOT/flea-rename-frame.XXXXXX") || exit 1
: > "$probe/$SANDBOX_MARKER" || exit 1
qs_pid=""
stop_qs() {
    [[ -n "$qs_pid" ]] || return 0
    local tick
    kill -TERM -- "-$qs_pid" 2>/dev/null || true
    for ((tick = 0; tick < 20; tick++)); do
        kill -0 -- "-$qs_pid" 2>/dev/null || break
        sleep 0.1
    done
    if kill -0 -- "-$qs_pid" 2>/dev/null; then
        kill -KILL -- "-$qs_pid" 2>/dev/null || true
        for ((tick = 0; tick < 20; tick++)); do
            kill -0 -- "-$qs_pid" 2>/dev/null || break
            sleep 0.1
        done
    fi
    wait "$qs_pid" 2>/dev/null || true
    if kill -0 -- "-$qs_pid" 2>/dev/null; then
        printf 'FAIL rename-frame owned process group %s survived teardown\n' "$qs_pid"
        return 1
    fi
    qs_pid=""
}
cleanup() { stop_qs || return 1; chmod u+w "$probe/home/readonly" 2>/dev/null; sandbox_remove "$probe"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$probe"/{home,config,state,data,cache,runtime} "$probe/home"/{fixture,readonly,Downloads} || exit 1
mkdir -p "$probe/data/Trash"/{files,info}
printf 'discarded\n' > "$probe/data/Trash/files/discarded.txt"
printf '[Trash Info]\nPath=%s\nDeletionDate=2026-09-23T10:00:00\n' "$probe/home/discarded.txt" > "$probe/data/Trash/info/discarded.txt.trashinfo"
chmod 700 "$probe/runtime"
printf 'a\n' > "$probe/home/fixture/a.txt"
printf 'b\n' > "$probe/home/fixture/b.txt"
ln -s a.txt "$probe/home/fixture/link.txt"
mkdir "$probe/home/fixture/sub"
printf 'read only\n' > "$probe/home/readonly/ro.txt"
touch -d '2000-01-01 00:00:00 UTC' "$probe/home/fixture/a.txt"
chmod 555 "$probe/home/readonly"
cat > "$probe/data/recently-used.xbel" <<XML
<?xml version="1.0"?><xbel version="1.0">
<bookmark href="file://$probe/home/fixture/a.txt" visited="2026-09-23T10:47:00Z"/>
<bookmark href="file://$probe/home/fixture/b.txt" visited="2026-09-22T09:12:00Z"/>
</xbel>
XML
# One saved share gives the rail its Network row, and a gio that lists no mounts answers the rail's poll.
mkdir -p "$probe/home/.config/gtk-3.0" "$probe/bin" || exit 1
printf 'smb://legacy.test/data NAS\n' > "$probe/home/.config/gtk-3.0/bookmarks"
printf '#!/bin/sh\nexit 0\n' > "$probe/bin/gio"
chmod +x "$probe/bin/gio"
env HOME="$probe/home" XDG_STATE_HOME="$probe/state" "$bin" --ui-state \
    '{"preview":{"column":false,"thumbnails":"off"},"updates":{"autoCheck":false}}' >/dev/null || exit 1
ln -s "$PWD/ui" "$probe/config/flea"
headless_commons "$probe" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui"
# The window loads its tab catcher from the shell directory, so the probe ships it as the product does.
ln -s "$PWD/ui/boot/fleatab.qml" "$probe/config/fleatab.qml"
cp tests/rename-frame.qml "$probe/config/shell.qml"
cp tests/rename-frame-checks.js "$probe/config/rename-frame-checks.js"
# Qt 6.12 lays the CI face's 9 px line at 12 px, filling the Tight row, so a second stop 9 Tight pass under Noto Sans Mono (13 px) measures the overhang.
cat > "$probe/overhang-fonts.conf" <<'XML'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <include ignore_missing="no">/etc/fonts/fonts.conf</include>
  <match target="pattern">
    <test name="family"><string>monospace</string></test>
    <edit name="family" mode="prepend_first" binding="strong"><string>Noto Sans Mono</string></edit>
  </match>
</fontconfig>
XML
# run_pass NAME [VAR=value ...]: one qs run over the probe with these extra variables, its receipt read from qs-NAME.log.
run_pass() {
    local name=$1 tick tally
    shift
    log="$probe/qs-$name.log"
    env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
        PATH="$probe/bin:$PATH" HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_CONFIG_HOME="$probe/config" \
        XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
        FLEA_PATH="$probe/home/fixture" FLEA_BIN="$bin" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
        QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 "$@" setsid dbus-run-session -- qs -p "$probe/config" >"$log" 2>&1 </dev/null &
    qs_pid=$!
    # The every-stop pass is 28 stops and densities across six hosts, a seventh at one stop; each pass is polled every 0.1 s for at most three minutes.
    receipt_ticks=1800
    for ((tick = 0; tick < receipt_ticks; tick++)); do
        grep -aq 'RENAMEFRAME DONE checks=[0-9]* failed=[0-9]*' "$log" && break
        sleep 0.1
    done
    stop_qs || exit 1
    printf 'rename-frame %s: owned process group gone\n' "$name"
    grep -aE 'RENAMEFRAME|TypeError|ReferenceError|ERROR|Cannot assign' "$log"
    tally=$(grep -ao 'RENAMEFRAME DONE checks=[0-9]* failed=[0-9]*' "$log" | tail -1)
    [[ -n "$tally" ]] || { printf 'FAIL rename-frame %s produced no completion receipt inside 180s; log tail:\n' "$name"; tail -20 "$log"; exit 1; }
    [[ "$tally" == *' failed=0' ]] || exit 1
    if grep -aqE 'TypeError|ReferenceError|ERROR|Cannot assign' "$log"; then
        echo "FAIL rename-frame $name logged an engine error beside its receipt"
        exit 1
    fi
}
run_pass every-stop
run_pass overhang RENAMEFRAME_PASS=overhang FONTCONFIG_FILE="$probe/overhang-fonts.conf"
