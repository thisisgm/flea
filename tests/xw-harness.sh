#!/bin/bash
# Headless pins for the two-window UI case's pointer targeting and exit cleanup.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
repo=$(pwd)
. "$repo/tests/lib/hypr-dispatch.sh" || exit 1
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
source_file=${XW_HARNESS_SOURCE:-$repo/tests/ui.sh}
# Sample source assignments: ipc_call_timeout=2s, ipc_call_kill_after=1s, xw_hang_s=30, xw_poll_s=0.2, xw_ui_poll_s=0.05, xw_ui_poll_tries=100, xw_window_poll_tries=300.
eval "$(sed -nE '/^(ipc_call_timeout|ipc_call_kill_after|xw_hang_s|xw_poll_s|xw_ui_poll_s|xw_ui_poll_tries|xw_window_poll_tries)=/p' "$source_file")" || exit 1
for helper in case_xwwatch xw_ipc xw_click_background xw_cleanup owned_trash_monitors xw_editor_diagnostics xw_wait_dialog; do
    eval "$(sed -n "/^$helper()/,/^}/p" "$source_file")" || exit 1
done
fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}
fixture_root="$tmp/fixtures"
flea_ui="$tmp/ui"
flea_bin="$tmp/flea"
run_root="$tmp/run"
settle_s=0
drain_wait_s=30
addr_a=0xaaa
addr_b=0xbbb
pidA=111
pidB=222
mkdir -p "$fixture_root" "$run_root"

# Sample clients: A at [100,200], B at [1000,200], both 880 by 620.
hyprctl() {
    if [[ "$*" == 'clients -j' ]]; then
        printf '[{"address":"%s","class":"com.thisisgm.flea","pid":111,"at":[100,200],"size":[880,620]},{"address":"%s","class":"com.thisisgm.flea","pid":222,"at":[1000,200],"size":[880,620]}]\n' "$addr_a" "$addr_b"
    elif [[ "$#" == 2 && "$2" == *".focus({ window = \"address:$addr_a\" })" ]]; then
        printf 'ok\n'
    else
        fail "unexpected compositor arguments: $*"
    fi
}
flea_window_class=com.thisisgm.flea
flea_process_owned() {
    [[ "$1" == 111 || "$1" == 222 ]]
}
omarchy-drive() {
    [[ "$*" == 'click 400 600 right' ]] || fail "pointer reached another window: $*"
    printf 'New folder|New file|Paste\n' > "$tmp/menu"
    printf '%s\n' "$*" >> "$tmp/clicks"
}
# Exercise the real xw_ipc timeout and argv against a per-process qs stub.
mkdir -p "$tmp/bin"
export XW_HARNESS_TMP="$tmp" a_cursor=0 background_y=400
export PATH="$tmp/bin:$PATH"
# The extracted xw_ipc runs its helper under the real timeout, so the helper is an executable that answers through the qs stub.
cat > "$tmp/bin/flea-ipc-helper" <<'STUB'
#!/bin/sh
exec qs ipc "$@"
STUB
chmod +x "$tmp/bin/flea-ipc-helper"
FLEA_IPC_HELPER="$tmp/bin/flea-ipc-helper"
cat > "$tmp/bin/qs" <<'STUB'
#!/bin/bash
set -uo pipefail
tmp=$XW_HARNESS_TMP
# Record every requested pid; route assertions belong to the harness, not this stub.
printf '%q ' "$@" >> "$tmp/ipc-argv"
printf '\n' >> "$tmp/ipc-argv"
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
[[ "$#" -ge 6 && "$1" == ipc && "$2" == --pid && "$4" == call && "$5" == flea ]] \
    || fail "unexpected IPC route: $*"
pid=$3
query=$6
shift 6
case "$query" in
    listingBackgroundCentre) printf '300 %s\n' "$background_y" ;;
    fileRowHeight) printf '37\n' ;;
    rowCentre) printf '300 74\n' ;;
    total) if [[ -f "$tmp/created" && ! -f "$tmp/deleted" ]]; then printf '6\n'; else printf '5\n'; fi ;;
    contextMenuEntries) cat "$tmp/menu" ;;
    cursor)
        if [[ "$pid" == 222 ]]; then printf '3\n'; else printf '%s\n' "$a_cursor"; fi
        ;;
    selectionCount)
        if [[ "$pid" == 111 ]]; then printf '0\n'
        elif [[ -f "$tmp/deleted" ]]; then printf '2\n'
        else printf '3\n'; fi
        ;;
    selectedIndices)
        if [[ -f "$tmp/deleted" ]]; then printf '3,4\n'
        elif [[ "${mode-}" == wrong-mark && -f "$tmp/created" ]]; then printf '2,3,5\n'
        else printf '2,3,4\n'; fi
        ;;
    menuDialogState)
        if [[ -f "$tmp/dialog.open" ]]; then printf '{"opened":true,"action":"newFile","controls":[{"name":"Field","focused":true}]}\n'
        else printf '{"opened":false}\n'; fi
        ;;
    rowAt)
        case "$1" in
            0) printf 'created-by-a.txt|file\n' ;;
            1) if [[ -f "$tmp/renamed" ]]; then printf 'renamed-by-a.txt|file\n'; else printf 'renamed-later.txt|file\n'; fi ;;
            2) printf 'sel-one.txt|file\n' ;;
            3) printf 'sel-two.txt|file\n' ;;
            4) printf 'sel-three.txt|file\n' ;;
            *) printf 'untouched.txt|file\n' ;;
        esac
        ;;
    *) fail "unexpected IPC query: $query" ;;
esac
STUB
chmod +x "$tmp/bin/qs"
ipc() { fail 'plain IPC cannot address either window in this harness'; }
sandbox_scratch() {
    mkdir -p "$1"
}
xw_sweep_stale() {
    :
}
launch() {
    touch "$tmp/B.window" "$tmp/B.backend" "$tmp/B.monitor"
}
xw_second_window() {
    touch "$tmp/A.window" "$tmp/A.backend" "$tmp/A.monitor"
    printf '%s\n' "$addr_a"
}
flea_pid() {
    printf '222\n'
}
xw_addr_for_pid() {
    printf '%s\n' "$addr_b"
}
wait_listing() {
    :
}
seek_row_named() {
    :
}
key() {
    :
}
settle() {
    :
}
xw_settled() {
    :
}
xw_wait_total() {
    [[ "$(xw_ipc "$1" total)" == "$2" ]] || fail "wrong total for $3"
    printf '%s %s %s\n' "$1" "$2" "$3" >> "$tmp/total-waits"
}
xw_wait_row() {
    local row
    for row in 0 1 2 3 4 5; do
        if [[ "$(xw_ipc "$1" rowAt "$row")" == "$2|"* ]]; then
            printf '%s %s %s\n' "$1" "$2" "$3" >> "$tmp/row-waits"
            return
        fi
    done
    fail "missing row for $3"
}
xw_goto() {
    a_cursor=$3
}
xw_key() {
    if [[ "$*" == "$addr_a m" ]]; then
        printf 'Open|Open with|Cut|Copy\n' > "$tmp/menu"
    elif [[ "$*" == "$addr_a -k Delete" ]]; then
        touch "$tmp/deleted"
    elif [[ "$*" == "$addr_a -k F2" ]]; then
        touch "$tmp/rename.open"
    elif [[ "$*" == "$addr_a -k Return" ]]; then
        if [[ -f "$tmp/rename.open" ]]; then
            touch "$tmp/renamed"
            rm -f "$tmp/rename.open"
        elif [[ -f "$tmp/dialog.open" ]]; then
            [[ "$(cat "$tmp/filename")" == created-by-a.txt ]] || fail 'dialog submitted the wrong filename'
            touch "$fixture_root/xwwatch/created-by-a.txt" "$tmp/created"
            rm -f "$tmp/dialog.open"
        else
            touch "$tmp/dialog.open"
        fi
    elif [[ "$*" == "$addr_a created-by-a.txt" ]]; then
        [[ -f "$tmp/dialog.open" ]] || fail 'New File typed without a dialog'
        printf 'created-by-a.txt\n' > "$tmp/filename"
    fi
}
xw_menu_seek() {
    local entries
    entries=$(xw_ipc "$2" contextMenuEntries)
    [[ "$entries" == 'New folder|New file'* ]] || fail "New File requires the background menu: $entries"
}
xw_wait_editor() {
    [[ "$mode" != failure ]] || fail 'injected editor failure'
}
xw_kill_second() {
    rm -f "$tmp/A.window"
}
kill_flea() {
    local window
    rm -f "$tmp/B.window"
    for window in A B; do
        if [[ ! -f "$tmp/$window.window" ]]; then
            rm -f "$tmp/$window.backend" "$tmp/$window.monitor"
        fi
    done
    [[ ! -f "$tmp/A.window" ]] || fail 'copied window survived teardown'
}

failures=0
# Nondefault budgets must reach timeout through the real IPC helper.
probe_ipc_call_timeout=4s
probe_ipc_call_kill_after=3s
result=0
bounds=$(
    ipc_call_timeout=$probe_ipc_call_timeout
    ipc_call_kill_after=$probe_ipc_call_kill_after
    timeout() { printf '%s %s\n' "$1" "$2"; }
    xw_ipc "$pidA" total
) || result=$?
if [[ "$result" != 0 || "$bounds" != "--kill-after=$probe_ipc_call_kill_after $probe_ipc_call_timeout" ]]; then
    printf 'FAIL xw_ipc ignored named IPC bounds: %s\n' "$bounds"
    failures=$((failures + 1))
fi

# One failed total read must poll at the named interval before the matching read.
probe_poll_s=0.07
result=0
(
    eval "$(sed -n '/^xw_wait_total()/,/^}/p' "$source_file")" || exit 1
    xw_poll_s=$probe_poll_s
    seen_total=0
    xw_ipc() { printf '%s\n' "$seen_total"; }
    sleep() { printf '%s\n' "$1" > "$tmp/total-poll"; seen_total=1; }
    xw_wait_total "$pidA" 1 'named poll'
) > "$tmp/total-poll.log" 2>&1 || result=$?
poll=$(cat "$tmp/total-poll" 2>/dev/null)
if [[ "$result" != 0 || "$poll" != "$probe_poll_s" ]]; then
    printf 'FAIL xw_wait_total ignored named poll interval: %s\n' "$poll"
    failures=$((failures + 1))
fi

# A row wait must use the same interval as the count wait, with one failed read before success.
result=0
(
    eval "$(sed -n '/^xw_wait_row()/,/^}/p' "$source_file")" || exit 1
    xw_poll_s=$probe_poll_s
    ready=0
    xw_ipc() {
        if [[ "$2" == total ]]; then
            printf '1\n'
        elif (( ready )); then
            printf 'wanted|file\n'
        else
            printf 'other|file\n'
        fi
    }
    sleep() {
        printf '%s\n' "$1" > "$tmp/row-poll"
        ready=1
    }
    xw_wait_row "$pidA" wanted 'named row poll'
) > "$tmp/row-poll.log" 2>&1 || result=$?
poll=$(cat "$tmp/row-poll" 2>/dev/null)
if [[ "$result" != 0 || "$poll" != "$probe_poll_s" ]]; then
    printf 'FAIL xw_wait_row ignored named poll interval: %s\n' "$poll"
    failures=$((failures + 1))
fi

# A nondefault interval must reach each UI helper's sleep without waiting in real time.
probe_ui_poll_s=0.03
# Three failed attempts distinguish the named cap from the original hundred polls.
probe_ui_poll_tries=3
for helper in xw_wait_editor xw_wait_dialog xw_settled; do
    : > "$tmp/$helper-polls"
    result=0
    (
        eval "$(sed -n "/^$helper()/,/^}/p" "$source_file")" || exit 1
        xw_ui_poll_s=$probe_ui_poll_s
        xw_ui_poll_tries=$probe_ui_poll_tries
        sleep() {
            printf '%s\n' "$1" >> "$tmp/$helper-polls"
        }
        xw_editor_diagnostics() {
            :
        }
        # Sample unready reply: {"index":-1,"focused":false,"opened":false}.
        xw_ipc() {
            printf '{"index":-1,"focused":false,"opened":false}\n'
        }
        if [[ "$helper" == xw_wait_editor ]]; then
            xw_wait_editor "$pidA" 'named UI poll'
        elif [[ "$helper" == xw_wait_dialog ]]; then
            xw_wait_dialog "$pidA" open 'named UI poll'
        else
            xw_settled "$pidA"
        fi
    ) > "$tmp/$helper-poll.log" 2>&1 || result=$?
    poll=$(sort -u "$tmp/$helper-polls")
    poll_count=$(wc -l < "$tmp/$helper-polls")
    if [[ "$result" != 1 || "$poll" != "$probe_ui_poll_s" ]]; then
        printf 'FAIL %s ignored named UI poll interval: %s\n' "$helper" "$poll"
        failures=$((failures + 1))
    fi
    if [[ "$result" != 1 ]] || (( poll_count != probe_ui_poll_tries )); then
        printf 'FAIL %s ignored named UI poll attempts: %s\n' "$helper" "$poll_count"
        failures=$((failures + 1))
    fi
done

# Both second-window stages must obey one named attempt cap and the shared UI interval.
probe_window_tries=2
for stage in process address; do
    result=0
    : > "$tmp/window-polls"
    : > "$tmp/window-attempts"
    (
        eval "$(sed -n '/^xw_second_window()/,/^}/p' "$source_file")" || exit 1
        xw_window_poll_tries=$probe_window_tries
        xw_ui_poll_s=$probe_ui_poll_s
        mkdir() { :; }
        cp() { :; }
        setsid() { :; }
        sleep() { printf '%s\n' "$1" >> "$tmp/window-polls"; }
        xw_owned_pid_for_arg() {
            if [[ "$stage" == process ]]; then
                printf 'process\n' >> "$tmp/window-attempts"
            else
                printf '111\n'
            fi
        }
        xw_window_addr_now() {
            printf 'address\n' >> "$tmp/window-attempts"
        }
        xw_second_window /d "$tmp/ui-copy"
    ) > "$tmp/window-$stage.log" 2>&1 || result=$?
    poll=$(sort -u "$tmp/window-polls")
    attempts=$(wc -l < "$tmp/window-attempts")
    if [[ "$result" != 1 || "$poll" != "$probe_ui_poll_s" ]]; then
        printf 'FAIL second-window %s ignored named interval: %s\n' "$stage" "$poll"
        failures=$((failures + 1))
    fi
    if [[ "$result" != 1 ]] || (( attempts != probe_window_tries )); then
        printf 'FAIL second-window %s ignored named attempts: %s\n' "$stage" "$attempts"
        failures=$((failures + 1))
    fi
done

# Stale sweep probes replace every process read and signal with a private fake /proc tree.
result=0
(
    for helper in flea_process_owned xw_process_abandoned xw_sweep_stale; do
        eval "$(sed -n "/^$helper()/,/^}/p" "$source_file" | sed 's|/proc/\$pid|$process_root/$pid|g')" || exit 1
    done
    process_root="$tmp/proc"
    mkdir -p "$process_root"
    FIXTURE_ROOT="$fixture_root"
    pgrep() { printf '111\n'; }
    flea_process_dir() { printf '%s/%s\n' "$process_root" "$1"; }
    kill() { printf '%s\n' "$*" >> "$tmp/sweep-signals"; }
    sleep() { :; }
    checks=0
    sweep_failures=0
    for sample in live-peer current-root dead-runner gone-root untagged suffix-marker duplicate-marker unreadable-runner unknown-runner other-command; do
        rm -rf "$process_root"
        mkdir -p "$process_root/111"
        peer_root="$tmp/run-$sample"
        mkdir -p "$peer_root"
        : > "$peer_root/.flea-test-sandbox"
        printf '222\n' > "$peer_root/runner.pid"
        printf 'qs\0-p\0%s/xwwatch-ui\0' "$FIXTURE_ROOT" > "$process_root/111/cmdline"
        printf 'FLEA_TEST_RUN_ROOT=%s\0' "$peer_root" > "$process_root/111/environ"
        expected=0
        case "$sample" in
            live-peer) mkdir -p "$process_root/222" ;;
            current-root)
                printf 'FLEA_TEST_RUN_ROOT=%s\0' "$run_root" > "$process_root/111/environ"
                printf '222\n' > "$run_root/runner.pid"
                mkdir -p "$process_root/222"
                ;;
            dead-runner) expected=1 ;;
            gone-root)
                rm -rf "$peer_root"
                expected=1
                ;;
            untagged) printf 'USER=gm\0' > "$process_root/111/environ" ;;
            suffix-marker) printf 'OTHER_FLEA_TEST_RUN_ROOT=%s\0' "$peer_root" > "$process_root/111/environ" ;;
            duplicate-marker) printf 'FLEA_TEST_RUN_ROOT=%s\0' "$run_root" >> "$process_root/111/environ" ;;
            unreadable-runner) rm "$peer_root/runner.pid" ;;
            unknown-runner) printf 'unknown\n' > "$peer_root/runner.pid" ;;
            other-command) printf 'qs\0-p\0/operator/ui\0' > "$process_root/111/cmdline" ;;
        esac
        : > "$tmp/sweep-signals"
        xw_sweep_stale || exit 1
        signals=$(wc -l < "$tmp/sweep-signals")
        checks=$((checks + 1))
        if (( signals != expected )); then
            printf 'FAIL stale sweep %s: %s signals, expected %s\n' "$sample" "$signals" "$expected"
            sweep_failures=$((sweep_failures + 1))
        fi
    done
    printf 'xw-stale: %s checks, %s failed\n' "$checks" "$sweep_failures"
    (( sweep_failures == 0 ))
) > "$tmp/stale.log" 2>&1 || result=$?
cat "$tmp/stale.log"
if [[ "$result" != 0 ]]; then
    failures=$((failures + 1))
fi

for mode in success failure wrong-mark; do
    export mode
    rm -f "$tmp/menu" "$tmp/deleted" "$tmp/clicks" "$tmp/ipc-argv" "$tmp/dialog.open" "$tmp/rename.open" "$tmp/created" "$tmp/renamed" "$tmp/filename" "$tmp/total-waits" "$tmp/row-waits" "$fixture_root/xwwatch/created-by-a.txt"
    result=0
    ( case_xwwatch ) > "$tmp/$mode.log" 2>&1 || result=$?
    expected=0
    [[ "$mode" == success ]] || expected=1
    if [[ "$result" != "$expected" ]]; then
        printf 'FAIL xwwatch %s returned %s, expected %s\n' "$mode" "$result" "$expected"
        cat "$tmp/$mode.log"
        failures=$((failures + 1))
    fi
    if [[ "$mode" == success ]]; then
        if [[ ! -s "$tmp/ipc-argv" ]] \
            || grep -vE '^ipc --pid (111|222) call flea ' "$tmp/ipc-argv" \
            || ! grep -q '^ipc --pid 111 call flea listingBackgroundCentre ' "$tmp/ipc-argv" \
            || ! grep -q '^ipc --pid 222 call flea selectedIndices ' "$tmp/ipc-argv" \
            || [[ "$(cat "$tmp/clicks" 2>/dev/null)" != 'click 400 600 right' ]]; then
            printf 'FAIL pid argv or A background target was not exercised\n'
            failures=$((failures + 1))
        fi
    fi
    if [[ "$mode" == success ]] && { ! grep -q '^111 6 create in A$' "$tmp/total-waits" \
        || ! grep -q '^222 6 create$' "$tmp/total-waits" \
        || ! grep -q '^111 created-by-a.txt create row in A$' "$tmp/row-waits" \
        || ! grep -q '^222 created-by-a.txt create row in B$' "$tmp/row-waits"; }; then
        printf 'FAIL create did not verify totals and names in both windows\n'
        failures=$((failures + 1))
    fi
    if [[ "$mode" == wrong-mark ]] && ! grep -q 'B marks after create' "$tmp/$mode.log"; then
        printf 'FAIL same-count changed-mark control was not refused\n'
        failures=$((failures + 1))
    fi
    if [[ "$mode" == failure ]] && ! grep -q 'injected editor failure' "$tmp/$mode.log"; then
        printf 'FAIL failure control did not reach the editor\n'
        failures=$((failures + 1))
    fi
    if compgen -G "$tmp/*.window" >/dev/null || compgen -G "$tmp/*.backend" >/dev/null || compgen -G "$tmp/*.monitor" >/dev/null; then
        printf 'FAIL xwwatch %s left an owned window, backend or monitor\n' "$mode"
        failures=$((failures + 1))
        rm -f "$tmp/"*.window "$tmp/"*.backend "$tmp/"*.monitor
    fi
done

if declare -F xw_click_background >/dev/null; then
    result=0
    (
        background_y=74
        xw_click_background "$addr_a" 111
    ) > "$tmp/row.log" 2>&1 || result=$?
    if [[ "$result" == 0 ]] || ! grep -q "background point lands on row" "$tmp/row.log"; then
        printf 'FAIL a point on a listing row was accepted as empty space\n'
        failures=$((failures + 1))
    fi
fi

# Both windows' reads must carry their requested pid through the production helper.
for window in A B; do
    pid=$pidA
    [[ "$window" != B ]] || pid=$pidB
    : > "$tmp/ipc-argv"
    result=0
    xw_ipc "$pid" total > "$tmp/route.out" 2> "$tmp/route.err" || result=$?
    forwarded=$(cat "$tmp/ipc-argv")
    if [[ "$result" != 0 || "$forwarded" != "ipc --pid $pid call flea total " ]]; then
        printf 'FAIL %s IPC read did not forward pid %s: %s\n' "$window" "$pid" "$forwarded"
        failures=$((failures + 1))
    fi
done

# Drive the copied-window stopper against a real child carrying this run's ownership marker.
eval "$(sed -n '/^xw_kill_second()/,/^}/p' "$source_file")" || exit 1
eval "$(sed -n '/^flea_process_owned()/,/^}/p' "$source_file")" || exit 1
flea_process_dir() {
    printf '/proc/%s\n' "$1"
}
dummy_lifetime_s=10
FLEA_TEST_RUN_ROOT="$run_root" bash -c 'exec -a "$1" sleep "$2"' _ "$fixture_root/xwwatch-ui" "$dummy_lifetime_s" &
test_pid=$!
pgrep() {
    printf '%s\n' "$test_pid"
}
# Wait until the dummy cmdline carries the marker before driving the stopper.
dummy_wait_s=5
dummy_deadline=$((SECONDS + dummy_wait_s))
dummy_cmdline=""
while :; do
    dummy_cmdline=$(tr '\0' ' ' 2>/dev/null < "/proc/$test_pid/cmdline") || dummy_cmdline=""
    [[ "$dummy_cmdline" == *"$fixture_root/xwwatch-ui"* ]] && break
    [[ -d "/proc/$test_pid" ]] || { printf 'FAIL dummy %s left before carrying its marker\n' "$test_pid"; failures=$((failures + 1)); break; }
    (( SECONDS < dummy_deadline )) || { printf 'FAIL dummy %s showed no marker within %s s\n' "$test_pid" "$dummy_wait_s"; failures=$((failures + 1)); break; }
    sleep 0.05
done
result=0
( xw_kill_second "$fixture_root/xwwatch-ui" ) > "$tmp/stop.log" 2>&1 || result=$?
stopped=0
wait "$test_pid" 2>/dev/null || stopped=$?
terminated_status=143
if [[ "$result" != 0 || "$stopped" != "$terminated_status" ]]; then
    printf 'FAIL owned copied window was not terminated: helper=%s child=%s\n' "$result" "$stopped"
    failures=$((failures + 1))
fi

# A gio exits after ownership was proved but before the monitor's environ read.
mock_process="$tmp/process"
mkdir -p "$mock_process"
pgrep() {
    printf '333\n'
}
flea_process_dir() {
    printf '%s\n' "$mock_process"
}
flea_process_owned() {
    if [[ -d "$mock_process" ]]; then
        rmdir "$mock_process"
        return 0
    fi
    return 2
}
result=0
owned_trash_monitors > "$tmp/monitors.out" 2> "$tmp/monitors.err" || result=$?
if [[ "$result" != 0 || -s "$tmp/monitors.out" || -s "$tmp/monitors.err" ]]; then
    printf 'FAIL vanished Trash monitor emitted output or returned %s\n' "$result"
    cat "$tmp/monitors.err"
    failures=$((failures + 1))
fi
# Drive the real failure observer for both call sites without a compositor or polling delay.
for step in 'New File after menu Return' 'rename after F2' 'New File after submit'; do
    result=0
    (
        eval "$(sed -n '/^xw_wait_editor()/,/^}/p' "$source_file")"
        pidA=111 pidB=222 addrA=$addr_a addrB=$addr_b dir=$fixture_root/xwwatch
        sleep() { :; }
        xw_ipc() {
            case "$2" in
                renameState) printf '{"index":-1,"focused":false,"pid":%s}\n' "$1" ;;
                contextMenuVisible) printf 'false\n' ;;
                menuDialogState) printf '{"opened":true,"action":"newFile"}\n' ;;
                cursor) printf '1\n' ;;
                rowAt) [[ "$3" == 1 ]] || return 1; printf 'renamed-later.txt|file\n' ;;
                total) printf '5\n' ;;
                *) return 1 ;;
            esac
        }
        hyprctl() {
            [[ "$*" == 'activewindow -j' ]] || return 1
            printf '{"address":"0xbbb","pid":222}\n'
        }
        if [[ "$step" == 'New File after menu Return' ]]; then
            xw_wait_dialog "$pidA" open "$step"
        elif [[ "$step" == 'rename after F2' ]]; then
            xw_wait_editor "$pidA" "$step"
        else
            xw_wait_dialog "$pidA" closed "$step" "$dir/missing.txt"
        fi
    ) > "$tmp/editor.log" 2>&1 || result=$?
    diagnostic="FAIL: xwwatch: $step: the rename editor never opened or took focus"
    [[ "$step" != 'New File after menu Return' ]] || diagnostic="FAIL: xwwatch: $step: the New File name field never opened or took focus"
    [[ "$step" != 'New File after submit' ]] || diagnostic="FAIL: xwwatch: $step: the New File dialog never closed or $fixture_root/xwwatch/missing.txt never appeared on disk"
    if [[ "$result" != 1 ]] \
        || ! grep -Fq "step=$step A=0xaaa/111 B=0xbbb/222" "$tmp/editor.log" \
        || ! grep -Fq 'A renameState={"index":-1,"focused":false,"pid":111} contextMenuVisible=false' "$tmp/editor.log" \
        || ! grep -Fq 'B renameState={"index":-1,"focused":false,"pid":222} contextMenuVisible=false' "$tmp/editor.log" \
        || ! grep -Fq 'menuDialogState={"opened":true,"action":"newFile"}' "$tmp/editor.log" \
        || ! grep -Fq 'A cursor=1 rowAt=renamed-later.txt|file total=5; B total=5;' "$tmp/editor.log" \
        || ! grep -Fq 'activewindow={"address":"0xbbb","pid":222,"isA":false,"isB":true}' "$tmp/editor.log" \
        || ! grep -Fq 'sel-one.txt' "$tmp/editor.log" \
        || ! grep -Fq "$diagnostic" "$tmp/editor.log"; then
        printf 'FAIL editor failure snapshot for %s\n' "$step"
        cat "$tmp/editor.log"
        failures=$((failures + 1))
    fi
done
# Pin the dialog predicate against wrong actions, missing focus, missing files, and unreadable IPC.
for sample in focused wrong-action unfocused completed missing-file still-open unreadable; do
    result=0
    (
        sleep() { :; }
        xw_editor_diagnostics() { :; }
        xw_ipc() {
            [[ "$1" == 111 && "$2" == menuDialogState ]] || return 1
            case "$sample" in
                focused | still-open) printf '{"opened":true,"action":"newFile","controls":[{"name":"Field","focused":true}]}\n' ;;
                wrong-action) printf '{"opened":true,"action":"copyTo","controls":[{"name":"Field","focused":true}]}\n' ;;
                unfocused) printf '{"opened":true,"action":"newFile","controls":[{"name":"Field","focused":false}]}\n' ;;
                completed | missing-file) printf '{"opened":false}\n' ;;
                unreadable) return 1 ;;
            esac
        }
        file="$tmp/dialog-created.txt"
        rm -f "$file"
        want=open
        case "$sample" in
            completed | still-open) touch "$file"; want=closed ;;
            missing-file) want=closed ;;
        esac
        xw_wait_dialog 111 "$want" "$sample" "$file"
    ) > "$tmp/dialog-$sample.log" 2>&1 || result=$?
    expected=1
    case "$sample" in focused | completed) expected=0 ;; esac
    if [[ "$result" != "$expected" ]]; then
        printf 'FAIL dialog condition %s returned %s, expected %s\n' "$sample" "$result" "$expected"
        cat "$tmp/dialog-$sample.log"
        failures=$((failures + 1))
    fi
done
printf 'xw-harness: 35 checks (27 existing checks, named row poll, settled interval and attempts, second-window process and address interval and attempts, stale-sweep group); %s failed\n' "$failures"
[[ "$failures" == 0 ]]
