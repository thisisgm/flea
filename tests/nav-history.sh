#!/usr/bin/env bash
# Back and Forward put the cursor back on the item it left, through the real window: ui/WindowBody.qml,
# ui/Pane.qml and the built backend, offscreen, the mouse's side buttons and the toolbar's Back pressed
# by QtTest. tests/nav-history.qml drives it; this file owns the fixture, the backend wrapper, the
# receipt and the cleanup.
#
# NAV_HISTORY_KEEP=1 keeps this run's marked fixture, screenshots included, and prints its path; the
# processes are stopped either way. Remove a kept one with the guard, e.g.
#   . tools/flea-sandbox-guard && sandbox_remove <printed path>
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
command -v qs >/dev/null || { echo "nav-history.sh: qs is not installed, cannot load the window"; exit 1; }
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[ -x "$bin" ] || { echo "nav-history.sh: build the candidate backend first: $bin"; exit 1; }
bin=$(readlink -f "$bin")
# Which scene the window runs: the main one, or the sort scene tests/nav-history-sort.sh asks for.
scene=${1:-main}
case "$scene" in
    main) qml=nav-history.qml ;;
    sort) qml=nav-history-sort.qml ;;
    *) echo "nav-history.sh: unknown scene '$scene', expected main or sort"; exit 2 ;;
esac

fixture="$FIXTURE_ROOT/nav-history-$$"
sandbox_make "$fixture"
fixture=$SANDBOX_TAKEN
home="$fixture/home"
qs_pid=""
keep=${NAV_HISTORY_KEEP:-0}

# Every process this run started carries HOME=$home in its own environment; nothing else is signalled.
owned_pids() {
    local env pid
    for env in /proc/[0-9]*/environ; do
        pid=${env#/proc/}; pid=${pid%/environ}
        [ "$pid" = "$$" ] && continue
        { tr '\0' '\n' < "$env"; } 2>/dev/null | grep -qx "HOME=$home" && printf '%s\n' "$pid"
    done
}
stop_owned() {
    local pids
    if [ -n "$qs_pid" ] && kill -0 "$qs_pid" 2>/dev/null; then kill -KILL -- "-$qs_pid" 2>/dev/null; fi
    for _ in $(seq 1 50); do
        pids=$(owned_pids)
        [ -z "$pids" ] && return 0
        sleep 0.1
    done
    # shellcheck disable=SC2086
    kill -KILL $pids 2>/dev/null
    sleep 0.2
}
cleanup() {
    local result=$?
    trap - EXIT
    stop_owned
    if [ "$keep" = 1 ]; then
        printf 'nav-history: kept fixture %s (screenshots in %s/shots)\n' "$fixture" "$fixture"
    else
        sandbox_remove "$fixture"
    fi
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$home/.local/share" "$home/.local/state" "$home/.config" "$fixture/cache" "$fixture/runtime" \
    "$fixture/tmp" "$fixture/shots" || exit 1
chmod 700 "$fixture/runtime" || exit 1
# The base the window opens on: 650 folders, two of them holding files, so a Forward has a cursor to
# put back as well. Its parent carries siblings on both sides of it for the climb case.
base="$home/nav"
mkdir -p "$base" "$home/alpha" "$home/beta" "$home/omega" "$home/zulu" || exit 1
mkdir "$base"/d{000..649} || exit 1
touch "$base"/d004/f{00..29}.txt "$base"/d450/f{00..29}.txt || exit 1

# The window's own state file, seeded through the real writer: the list view, no update poll. The sort
# scene also lists every folder in the plain name order, so only the tab it restores re-sorts.
state='{"view":"list","updates":{"autoCheck":false}}'
[ "$scene" = sort ] && state='{"view":"list","updates":{"autoCheck":false},"rememberSort":false}'
HOME="$home" XDG_STATE_HOME="$home/.local/state" XDG_CONFIG_HOME="$home/.config" \
    "$bin" --ui-state "$state" >/dev/null </dev/null \
    || { echo "FAIL could not seed the window's state"; exit 1; }

# The backend is the real one. The wrapper records every request line it is sent, and holds a located
# reply, or a rows reply, back for 1.5 s only while the QML file has that reply's gate file in place,
# which is how a late answer is staged against a real listing. Replies keep their order: a held one
# holds the ones behind it. --update answers nothing, so no run reaches the network.
requests="$fixture/backend-requests.log"
gate="$fixture/delay-located"
rows_gate="$fixture/delay-rows"
wrapper="$fixture/flea-backend-wrapper"
: > "$requests"
cat > "$wrapper" <<EOF
#!/usr/bin/env bash
case "\${1:-}" in
    --update) exit 1 ;;
    --backend) ;;
    *) exec '$bin' "\$@" ;;
esac
# The tee sits in a process substitution, so the wrapper ends when the backend does: Backend.qml's
# quitReady waits for this process to exit, and a tee still blocked on stdin would hold it open.
'$bin' "\$@" < <(tee -a '$requests') | while IFS= read -r line; do
    case \$line in
        *'"t":"located"'*) [ -e '$gate' ] && sleep 1.5 ;;
        *'"t":"rows"'*) [ -e '$rows_gate' ] && sleep 1.5 ;;
    esac
    printf '%s\n' "\$line"
done
EOF
chmod +x "$wrapper" || exit 1

# The real ui/ as the module under test, and the Omarchy modules its qs.* imports resolve to.
cp -a ui "$fixture/flea" || exit 1
ln -s "$(readlink -f ui/Commons)" "$fixture/Commons"
ln -s "$(readlink -f ui/Ui)" "$fixture/Ui"
cp "tests/$qml" "$fixture/shell.qml" || exit 1

nonce="$$-$(date +%s%N)"
log="$fixture/qs.log"
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u YDOTOOL_SOCKET -u FLEA_SELECT \
    HOME="$home" XDG_STATE_HOME="$home/.local/state" XDG_DATA_HOME="$home/.local/share" \
    XDG_CONFIG_HOME="$home/.config" XDG_CACHE_HOME="$fixture/cache" XDG_RUNTIME_DIR="$fixture/runtime" \
    TMPDIR="$fixture/tmp" FLEA_BIN="$wrapper" FLEA_PATH="$base" \
    NAV_HISTORY_BASE="$base" NAV_HISTORY_REQUESTS="$requests" NAV_HISTORY_GATE="$gate" NAV_HISTORY_ROWS_GATE="$rows_gate" \
    NAV_HISTORY_SHOTS="$fixture/shots" NAV_HISTORY_RECEIPT="$nonce" \
    QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    setsid qs -p "$fixture/shell.qml" >"$log" 2>&1 </dev/null &
qs_pid=$!

# Every wait inside the run is gated on the window settling and bounded at 20 s; this is the outer bound.
limit_seconds=240
receipt_re="nav-history $nonce: (PASS|FAIL) "
for _ in $(seq 1 $((limit_seconds * 10))); do
    grep -qE "$receipt_re" "$log" && break
    kill -0 "$qs_pid" 2>/dev/null || break
    sleep 0.1
done
# The window quits its backends itself after the receipt, and the last one drained kills qs with a
# plain kill (ui/WindowBody.qml backendDrained), so 143 is the expected end and 0 is accepted too.
drained=0
for _ in $(seq 1 100); do
    kill -0 "$qs_pid" 2>/dev/null || { drained=1; break; }
    sleep 0.1
done
qs_status=""
if [ "$drained" = 1 ]; then
    wait "$qs_pid"
    qs_status=$?
fi

grep -aE ' (ok  |FAIL) |geom |nav-history' "$log" | sed 's/^.*scene[^:]*: //' | uniq
refusals=""
refuse() { refusals="${refusals}FAIL $*"$'\n'; }
receipts=$(grep -acE "$receipt_re" "$log")
if [ "$receipts" -ne 1 ]; then
    refuse "expected exactly one receipt for run $nonce, found $receipts"
elif ! grep -aqE "nav-history $nonce: PASS " "$log"; then
    refuse "the run's receipt is FAIL"
fi
if [ "$drained" != 1 ]; then
    refuse "the window did not quit its backends and exit within 10 s of its receipt"
elif [ "$qs_status" != 0 ] && [ "$qs_status" != 143 ]; then
    refuse "qs exited with status $qs_status, not 0 or the 143 of its own kill"
fi
# A runtime error anywhere is a failure even when every check passed: it is code that did not run.
errors=$(grep -aE 'TypeError|ReferenceError|SyntaxError|RangeError|is not a type|is not a function|Cannot assign|Cannot read propert|Failed to load|caused by|Unable to assign|Loader.Error' "$log")
[ -z "$errors" ] || refuse "runtime errors in the window log:"$'\n'"$errors"
# Any warning or error level line is refused, but one: the offscreen platform has no window masks,
# which Quickshell says once per window at load ('  WARN: This plugin does not support setting window masks').
levels=$(grep -aE '(^|[^A-Za-z])(WARN|ERROR|CRIT|CRITICAL|FATAL)([^A-Za-z]|$)' "$log" \
    | grep -avE '^ *WARN: This plugin does not support setting window masks$')
[ -z "$levels" ] || refuse "warning or error lines in the window log:"$'\n'"$levels"
if [ -z "$refusals" ]; then
    echo "nav-history: passed"
    exit 0
fi
printf '%s' "$refusals"
printf -- '--- full window log %s ---\n' "$log"
cat "$log"
printf -- '--- end of window log ---\n'
[ "$keep" = 1 ] || echo "nav-history: NAV_HISTORY_KEEP=1 keeps the fixture, screenshots and request log"
echo "nav-history: failed"
exit 1
