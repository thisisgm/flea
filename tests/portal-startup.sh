#!/usr/bin/env bash
# Starts the real xdg-desktop-portal frontend on a private bus, because only its own start builds a proxy for each preferred backend (issue #123).
set -u
set -o pipefail

# A signal ends the run through the EXIT trap, so cleanup runs once and the script never resumes after it.
trap_signals() {
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
}

frontend_bin=${FLEA_PORTAL_FRONTEND:-/usr/lib/xdg-desktop-portal}
backend=org.freedesktop.impl.portal.desktop.flea
desktop=org.freedesktop.portal.Desktop

if [ -z "${FLEA_PORTAL_STARTUP_BUS:-}" ]; then
    # Stage one builds the sandbox before the bus exists, because the bus daemon reads its service directory once at its own start.
    command -v dbus-run-session >/dev/null 2>&1 && command -v setsid >/dev/null 2>&1 || {
        printf 'portal-startup.sh: dbus-run-session or setsid is missing, and this suite will not run on the session bus\n' >&2
        exit 1
    }
    [ -x "$frontend_bin" ] || {
        printf 'portal-startup.sh: %s is missing, so there is no frontend to start\n' "$frontend_bin" >&2
        exit 1
    }
    # Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
    . "$(dirname "$0")/../tools/flea-sandbox-guard"
    cd "$(dirname "$0")/.." || exit 1
    tree=$PWD
    installer_binary=${FLEA_BIN:-"$tree/target/release/flea"}
    for file in tools/flea-portal packaging/flea.portal packaging/$backend.service; do
        [ -f "$file" ] || { printf 'portal-startup.sh: %s is missing, refusing to report on nothing\n' "$file" >&2; exit 1; }
    done
    dir="$FIXTURE_ROOT/flea-portal-startup-$$"
    session=
    # The document portal mounts a FUSE tree under the runtime dir and unmounts it as it exits, so nothing is deleted while a mount is still there.
    remove_sandbox() {
        local waited=0
        # The session is its own process group, so one signal reaches the bus, the frontend and every backend, and none of them is writing as the tree goes.
        if [ -n "$session" ]; then
            kill -TERM -- "-$session" 2>/dev/null
            wait "$session" 2>/dev/null
            while kill -0 -- "-$session" 2>/dev/null && [ "$waited" -lt 50 ]; do
                sleep 0.1
                waited=$((waited + 1))
            done
            waited=0
        fi
        while grep -qF " $dir/" /proc/mounts && [ "$waited" -lt 50 ]; do
            sleep 0.1
            waited=$((waited + 1))
        done
        if grep -qF " $dir/" /proc/mounts; then
            printf 'FAIL portal-startup.sh: a mount is still under %s, so the sandbox is left in place\n' "$dir"
            exit 1
        fi
        sandbox_remove "$dir"
    }
    trap remove_sandbox EXIT
    trap_signals
    sandbox_make "$dir"
    for arm in control flea; do
        mkdir -p "$dir/$arm/data/xdg-desktop-portal/portals" "$dir/$arm/config/xdg-desktop-portal"
    done
    mkdir -p "$dir/run" "$dir/data/dbus-1/services" "$dir/share"
    chmod 700 "$dir/run"
    # The frontend aborts without GSettings schemas, and they are the one thing it takes from the system data dirs.
    ln -s /usr/share/glib-2.0 "$dir/share/glib-2.0"

    # The control backend owns a name, answers the FileChooser version and never exits.
    cat > "$dir/stub-backend" <<'STUB'
#!/usr/bin/python3
import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

XML = "<node><interface name='org.freedesktop.impl.portal.FileChooser'><property name='version' type='u' access='read'/></interface></node>"
info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]


def acquired(connection, _name):
    connection.register_object("/org/freedesktop/portal/desktop", info, None, lambda *_: GLib.Variant("u", 3), None)


Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.impl.portal.desktop.stub", Gio.BusNameOwnerFlags.NONE, acquired, None, None)
GLib.MainLoop().run()
STUB
    chmod +x "$dir/stub-backend"
    printf '[portal]\nDBusName=org.freedesktop.impl.portal.desktop.stub\nInterfaces=org.freedesktop.impl.portal.FileChooser;\n' \
        > "$dir/control/data/xdg-desktop-portal/portals/stub.portal"
    printf '[D-BUS Service]\nName=org.freedesktop.impl.portal.desktop.stub\nExec=%s/stub-backend\n' "$dir" \
        > "$dir/data/dbus-1/services/org.freedesktop.impl.portal.desktop.stub.service"

    # The packaged files, with only the Exec line pointed at the tree's backend.
    cp packaging/flea.portal "$dir/flea/data/xdg-desktop-portal/portals/flea.portal"
    sed "s|^Exec=.*|Exec=$tree/tools/flea-portal|" packaging/$backend.service > "$dir/data/dbus-1/services/$backend.service"

    # A frontend the suite did not start must not be activated by a probe, so its service file is shadowed ahead of the system one.
    printf '[D-BUS Service]\nName=%s\nExec=/bin/false\n' "$desktop" > "$dir/data/dbus-1/services/$desktop.service"

    # The picker the backend spawns in place of a window: it notes that it ran, records the request and answers 1, the user's own refusal.
    cat > "$dir/picker" <<'PICKER'
#!/bin/sh
printf 'picker ran: %s\n' "$*" >> "$FLEA_PORTAL_PICKER_LOG"
printf '%s' "$FLEA_PICKER" > "$FLEA_PORTAL_CAPTURE"
printf '{"response": 1}' > "$2"
PICKER
    chmod +x "$dir/picker"

    # The frontend closes a request when its caller leaves the bus, and the backend then kills the picker, so the caller stays until the Response signal.
    cat > "$dir/caller" <<'CALLER'
#!/usr/bin/python3
import sys
import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
loop = GLib.MainLoop()
result = {}


def on_response(*args):
    result["response"] = args[5].unpack()[0]
    loop.quit()


bus.signal_subscribe("org.freedesktop.portal.Desktop", "org.freedesktop.portal.Request", "Response", None, None, Gio.DBusSignalFlags.NONE, on_response)
handle = bus.call_sync("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.FileChooser", "OpenFile",
                       GLib.Variant("(ssa{sv})", ("", "portal-startup.sh", {})), GLib.VariantType("(o)"), Gio.DBusCallFlags.NONE, -1, None)
print("caller: OpenFile returned %s" % handle.unpack()[0], file=sys.stderr, flush=True)
if "response" not in result:
    GLib.timeout_add_seconds(int(sys.argv[1]), loop.quit)
    loop.run()
print(result.get("response", "never"))
CALLER
    chmod +x "$dir/caller"

    export FLEA_PORTAL_STARTUP_BUS=1 FLEA_PORTAL_STARTUP_DIR="$dir" FLEA_BIN="$dir/picker" FLEA_PORTAL_CAPTURE="$dir/request.json" FLEA_PORTAL_PICKER_LOG="$dir/picker.log"
    export XDG_RUNTIME_DIR="$dir/run" XDG_DATA_HOME="$dir/data" XDG_DATA_DIRS="$dir/share" XDG_CURRENT_DESKTOP=Hyprland
    unset XDG_DESKTOP_PORTAL_DIR XDG_CONFIG_HOME DISPLAY WAYLAND_DISPLAY HYPRLAND_INSTANCE_SIGNATURE
    # The bus daemon's and the backends' stderr is kept apart, so the last line of a run is the suite's own and a failure still prints it.
    setsid dbus-run-session -- "$tree/tests/portal-startup.sh" "$@" 2> "$dir/bus.log" &
    session=$!
    # A wait and not a foreground run, because bash defers a trapped signal until a foreground command ends.
    wait "$session"
    status=$?
    session=
    if [ "$status" != 0 ]; then
        echo "--- bus daemon and backend log ---"
        cat "$dir/bus.log"
    else
        python3 "$tree/tests/portal-startup-lifetime.py"
        status=$?
        python3 "$tree/tests/portal-startup-installer-binary.py"
        binary_status=$?
        [ "$status" != 0 ] || status=$binary_status
        if [ "$status" = 0 ]; then
            FLEA_BIN="$installer_binary" python3 "$tree/tests/portal-startup-installer.py"
            status=$?
        fi
    fi
    exit "$status"
fi

dir=${FLEA_PORTAL_STARTUP_DIR:-}
# A marker file in the sandbox is what says this stage was started by stage one and not by hand on the operator's bus.
[ -n "$dir" ] && [ -f "$dir/.flea-test-sandbox" ] || { printf 'portal-startup.sh: no sandbox, so this suite will not run on the session bus\n' >&2; exit 1; }
frontend=0
cleanup() {
    if [ "$frontend" != 0 ] && kill -0 "$frontend" 2>/dev/null; then
        kill "$frontend" 2>/dev/null
        wait "$frontend" 2>/dev/null
    fi
}
trap cleanup EXIT
trap_signals

fail=0
check() {
    local label="$1" expected="$2" actual="$3"
    if [ "$expected" != "$actual" ]; then
        echo "FAIL $label"
        echo "  expected: $expected"
        echo "  actual:   $actual"
        fail=1
    else
        echo "ok   $label"
    fi
}

# Sets has_owner to 1 when the bus has an owner for the name and 0 when it has none, and ends the run when gdbus itself fails; NameHasOwner never activates a service.
owned() {
    local out
    out=$(gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.NameHasOwner "$1" 2>&1) || {
        printf 'FAIL portal-startup.sh: gdbus NameHasOwner %s failed: %s\n' "$1" "$out"
        exit 1
    }
    has_owner=0
    case $out in *true*) has_owner=1 ;; esac
}

# Polls until the name is owned (up) or gone (down) and sets elapsed to the seconds it took, or to "never" when the limit passed or the frontend died.
wait_name() {
    local want="$1" name="$2" limit="$3" goal=0 start now tenths
    [ "$want" = up ] && goal=1
    start=$(date +%s%N)
    while :; do
        owned "$name"
        [ "$has_owner" = "$goal" ] && break
        now=$(date +%s%N)
        if [ $(((now - start) / 1000000)) -gt $((limit * 1000)) ] || { [ "$frontend" != 0 ] && ! kill -0 "$frontend" 2>/dev/null; }; then
            elapsed=never
            return 1
        fi
        sleep 0.1
    done
    now=$(date +%s%N)
    tenths=$(((now - start + 50000000) / 100000000))
    elapsed=$((tenths / 10)).$((tenths % 10))
}

# The reporter's portals.conf, written to the arm's own config home, then the frontend started the way the user unit starts it.
start_arm() {
    printf '[preferred]\norg.freedesktop.impl.portal.FileChooser=%s\n' "$2" > "$dir/$1/config/xdg-desktop-portal/portals.conf"
    XDG_DATA_HOME="$dir/$1/data" XDG_CONFIG_HOME="$dir/$1/config" "$frontend_bin" -v >> "$dir/frontend-$1.log" 2>&1 &
    frontend=$!
}

stop_arm() {
    kill "$frontend" 2>/dev/null
    wait "$frontend" 2>/dev/null
    frontend=0
}

owned $desktop
check "the private bus starts with no frontend on it" "0" "$has_owner"

# Each arm is its name, its FileChooser= value and the bus name its backend owns.
for arm in "control stub org.freedesktop.impl.portal.desktop.stub" "flea flea;gtk $backend"; do
    read -r name conf owner <<< "$arm"
    start_arm "$name" "$conf"
    wait_name up $desktop 5
    took=$elapsed
    check "$name arm: the frontend owns $desktop within 5 s" "0" "$([ "$took" = never ] && echo 1 || echo 0)"
    echo "     $name arm: $desktop owned after $took s"
    if [ "$took" = never ]; then
        stop_arm
        continue
    fi
    check "$name arm: FileChooser introspects on the frontend" "1" \
        "$(gdbus introspect --session --dest $desktop --object-path /org/freedesktop/portal/desktop 2>/dev/null | grep -c 'interface org.freedesktop.portal.FileChooser')"
    owned "$owner"
    check "$name arm: the frontend activated the backend at its own start" "1" "$has_owner"
    if [ "$name" = flea ]; then
        reply=$(busctl --user --auto-start=no --timeout=5s get-property "$desktop" /org/freedesktop/portal/desktop org.freedesktop.portal.FileChooser version 2>&1)
        check "flea arm: the installer's readiness probe reaches the real frontend" "0" "$?"
        check "flea arm: the readiness reply is a nonzero unsigned version" "1" "$(grep -Ec '^u [1-9][0-9]*$' <<< "$reply")"
        wait_name down "$owner" 60
        released=$elapsed
        check "flea arm: the backend released its name after its idle exit" "0" "$([ "$released" = never ] && echo 1 || echo 0)"
        echo "     flea arm: backend name released after $released s"
        : > "$FLEA_PORTAL_CAPTURE"
        # The backend answers only after the picker has exited, so the Response signal is the event that the picker ran; 30 s is the bound.
        response=$("$dir/caller" 30 2> "$dir/openfile.log")
        printf 'caller answered: %s\n' "$response" >> "$dir/openfile.log"
        wait_name up "$owner" 10
        again=$elapsed
        check "flea arm: OpenFile through the frontend activated the backend again" "0" "$([ "$again" = never ] && echo 1 || echo 0)"
        check "flea arm: the caller got the picker's refusal as response 1 within 30 s" "1" "$response"
        check "flea arm: the stub recorded the request" "1" "$([ -s "$FLEA_PORTAL_CAPTURE" ] && echo 1 || echo 0)"
        owned $desktop
        check "flea arm: the frontend still owns $desktop" "1" "$has_owner"
        # The reporter's operation was a restart, which meets a backend still running inside its idle window, so that is read between the stop and the start.
        stop_arm
        owned "$owner"
        check "flea arm: the backend is still running when the frontend restarts" "1" "$has_owner"
        start_arm "$name" "$conf"
        wait_name up $desktop 5
        took=$elapsed
        check "flea arm: a restarted frontend owns $desktop within 5 s" "0" "$([ "$took" = never ] && echo 1 || echo 0)"
        echo "     flea arm: $desktop owned after $took s on restart"
    fi
    stop_arm
done

if [ "$fail" = 0 ]; then
    echo "portal-startup.sh: all checks passed"
else
    echo "portal-startup.sh: FAILED"
    for file in openfile.log picker.log; do
        echo "--- $file ---"
        cat "$dir/$file" 2>/dev/null
    done
    for arm in control flea; do
        echo "--- $arm frontend log ---"
        cat "$dir/frontend-$arm.log" 2>/dev/null
    done
fi
exit "$fail"
