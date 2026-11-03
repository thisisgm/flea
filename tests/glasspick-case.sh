#!/usr/bin/env bash
# Exercises capture targeting and teardown with fixture windows and real tagged children, without a display.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/glasspick-case.XXXXXX") || exit 1
[[ "$test_root" == /*/* ]] || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
mkdir -p "$test_root/foreign"
: > "$test_root/foreign/$SANDBOX_MARKER"
run_root="$test_root"
owned_pid=""
foreign_pid=""
# SIGKILL exits as 137; the outer trap returns 1 when it refuses the unrelated PID.
interrupted_replay_status=137
interrupted_cleanup_status=1
interrupted_fixture_count=3 # Backdrop, terminal and Flea.
control_lifetime_s=60
fail() { printf 'FAIL glasspick-case: %s\n' "$*" >&2; exit 1; }
sed -n '/^flea_process_dir()/p; /^flea_process_owned()/,/^}/p' tests/ui.sh > "$test_root/ownership.sh"
. "$test_root/ownership.sh"
. "$PWD/tests/ui-captures-glass.sh"
. "$PWD/tests/lib/hypr-dispatch.sh"
cleanup() {
    local status=$? pids pid
    run_root="$test_root"
    glasspick_stop "$owned_pid" || status=1
    for pids in "$test_root"/replay-*/pids; do
        [[ -f "$pids" ]] || continue
        # Sample input: one PID per line, for example "12345".
        while read -r pid; do glasspick_stop "$pid" || status=1; done < "$pids"
    done
    run_root="$test_root/foreign"
    glasspick_stop "$foreign_pid" || status=1
    if [[ "$status" == 0 ]]; then sandbox_remove "$test_root"; fi
    exit "$status"
}
trap cleanup EXIT
env FLEA_TEST_RUN_ROOT="$test_root" sleep 60 &
owned_pid=$!
env FLEA_TEST_RUN_ROOT="$test_root/foreign" sleep 60 &
foreign_pid=$!
wait_tag() {
    local pid="$1" end=$((SECONDS + 3))
    while (( SECONDS < end )); do
        flea_process_owned "$pid" && return 0
        sleep 0.01
    done
    fail "child $pid did not reach its expected environment tag"
}
wait_tag "$owned_pid"
run_root="$test_root/foreign"
wait_tag "$foreign_pid"
run_root="$test_root"
glasspick_stop "$owned_pid" || fail "owned child did not stop"
glasspick_stop "$owned_pid" || fail "an already exited child was not accepted"
owned_pid=""
if glasspick_stop "$foreign_pid" > "$test_root/refusal.log" 2>&1; then
    fail "foreign-tagged child was accepted"
fi
grep -q 'refusing to stop unowned or unreadable fixture' "$test_root/refusal.log" || fail "foreign refusal was not explicit"
kill -0 "$foreign_pid" || fail "foreign child was signaled despite its ownership mismatch"
run_root="$test_root/foreign"
flea_process_owned "$foreign_pid" || fail "foreign control cannot be reclaimed by its own tag"
glasspick_stop "$foreign_pid" || fail "foreign control did not stop under its own owner"
foreign_pid=""
run_root="$test_root"
printf 'GLASSPICK_CASE owned=stopped foreign=refused gone=accepted\n'

# Replay the actual matrix and EXIT handler with tagged children; key, focus, IPC and teardown helpers stay real.
capture_targeting() (
    set -e
    trap - EXIT
    local scenario="$1" helper replay="$test_root/replay-$1" capture_pid=100 cursor=0 launches=0 status
    local ipc_pid=sentinel flea_window_class=com.thisisgm.flea ipc_call_timeout=2s ipc_call_kill_after=1s
    local repo="$PWD" fixture_root="$replay/fixtures" evidence_dir="$replay/evidence" real_state_dir="$replay/current"
    local real_foreground='#dddddd' real_accent='#7799bb' caller_trap pid ownership expected
    for helper in ipc focused_class focused_pid window_addr_of_pid assert_focus key xw_ipc wait_listing seek_row_named; do
        # Sample input: a line 'ipc() {' through a line '}' in tests/ui.sh.
        eval "$(sed -n "/^$helper()/,/^}/p" tests/ui.sh)"
    done
    mkdir -p "$fixture_root" "$real_state_dir/theme" "$replay/bin" "$replay/foot-ready"
    local PATH="$replay/bin:$PATH"
    printf '#!/bin/sh\nexec sleep 60\n' > "$replay/bin/qs"
    python3 -B - "$repo/tests/fixtures/glasspick-hyprctl.json" "$replay/bin/hyprctl" <<'PYCODE'
import json, pathlib, sys
pathlib.Path(sys.argv[2]).write_text("\n".join(json.loads(pathlib.Path(sys.argv[1]).read_text())["program"]) + "\n")
PYCODE
    chmod +x "$replay/bin/hyprctl"
    export GLASSPICK_HYPR_ROOT="$replay"
    printf '100\n' > "$replay/capture-pid"
    printf '100\n' > "$replay/focus-pid"
    cat > "$replay/bin/foot" <<'STUB'
#!/bin/bash
set -uo pipefail
if [[ "${1-}" == --check-config ]]; then
    [[ "$#" == 2 && "$2" == --config=* ]] || exit 2
    exec python3 -B - "${2#--config=}" "$GLASSPICK_FOOT_SCENARIO" "$GLASSPICK_FOOT_CHECKS" <<'PY'
import configparser, pathlib, sys
path, scenario, checks = pathlib.Path(sys.argv[1]), sys.argv[2], pathlib.Path(sys.argv[3])
with checks.open('a') as log:
    log.write(str(path) + '\n')
if scenario in ('check-rejected', 'check-diagnostic'):
    path.write_text(path.read_text().replace('[colors-dark]', '[colors]'))
parser = configparser.ConfigParser(interpolation=None)
# Sample input (newlines shown as \n): [main]\nfont=monospace:size=11\n[colors-dark]\nforeground=dddddd\nbackground=222222\nalpha=0.65\n
parser.read(path)
if parser.sections() != ['main', 'colors-dark']:
    print('foot.ini:3: [colors]: invalid section name: colors', file=sys.stderr)
    sys.exit(0 if scenario == 'check-diagnostic' else 1)
assert dict(parser['main']) == {'font': 'monospace:size=11'}
mode = path.parent.name.rsplit('-', 1)[1]
assert dict(parser['colors-dark']) == {
    'foreground': 'dddddd', 'background': '222222', 'alpha': '0.65' if mode == 'glassy' else '1'
}
PY
fi
if [[ "$GLASSPICK_FOOT_SCENARIO" == startup-error ]]; then
    printf 'err: config.c: foot.ini:3: [colors]: invalid section name: colors\n' >&2
fi
: > "$GLASSPICK_FOOT_READY/$$"
exec sleep 60
STUB
    chmod +x "$replay/bin/foot" "$replay/bin/qs"
    export FLEA_TEST_RUN_ROOT="$run_root" GLASSPICK_FOOT_SCENARIO="$scenario" GLASSPICK_FOOT_CHECKS="$replay/foot-checks"
    export GLASSPICK_FOOT_READY="$replay/foot-ready"
    printf 'replay\n' > "$real_state_dir/theme.name"
    printf 'foreground = "#dddddd"\naccent = "#7799bb"\nbackground = "#222222"\n' > "$real_state_dir/theme/colors.toml"
    : > "$real_state_dir/theme/shell.toml"
    for helper in keys launches previews shots pids cleanup focus foot-checks; do : > "$replay/$helper"; done
    trap 'printf "caller-exit\n" >> "$replay/caller-exit"' EXIT
    caller_trap=$(trap -p EXIT)
    remember_pid() {
        local deadline=$((SECONDS + 3))
        wait_tag "$1"
        if [[ "$1" == "${glasspick_term_pid-}" ]]; then
            while [[ ! -f "$replay/foot-ready/$1" ]]; do
                (( SECONDS < deadline )) || fail "fixture Foot $1 did not finish startup"
                sleep 0.01
            done
        fi
        if ! grep -qx "$1" "$replay/pids"; then printf '%s\n' "$1" >> "$replay/pids"; fi
    }
    fixture_home_make() {
        mkdir -p "$1/.local/state/omarchy/current/theme"
        command cp "$real_state_dir/theme/"* "$1/.local/state/omarchy/current/theme/"
    }
    cp() {
        case "$1" in
            /usr/share/omarchy/themes/*/colors.toml) command cp "$real_state_dir/theme/colors.toml" "$2" ;;
            /usr/share/omarchy/themes/*/shell.toml) command cp "$real_state_dir/theme/shell.toml" "$2" ;;
            *) command cp "$@" ;;
        esac
    }
    foot() { :; }
    grim() { :; }
    magick() { :; }
    settle() { :; }
    timeout() { shift 2; "$@"; }
    seed_ui_state() { mkdir -p "$1"; }
    kill_flea() {
        if [[ "${FUNCNAME[1]}" == glasspick_cleanup ]]; then
            printf 'ipc=%s term=%s back=%s shots=%s\n' "$ipc_pid" "${glasspick_term_pid-unset}" \
                "${glasspick_back_pid-unset}" "${glasspick_shots-unset}" >> "$replay/cleanup"
        fi
        if [[ -s "$replay/launches" ]]; then glasspick_stop "$capture_pid"; fi
    }
    glasspick_address() { remember_pid "$1"; printf '0x%x\n' "$1"; }
    glasspick_client() { printf '{"at":[0,0]}\n'; }
    glasspick_place() { remember_pid "$1"; }
    launch() {
        [[ -z "$ipc_pid" ]] || fail "launch inherited a stale capture pid: $ipc_pid"
        launches=$((launches + 1))
        env FLEA_TEST_RUN_ROOT="$run_root" sleep 60 &
        capture_pid=$!; cursor=0
        printf '%s\n' "$capture_pid" > "$replay/capture-pid"
        printf '%s\n' "$capture_pid" > "$replay/focus-pid"
        remember_pid "$capture_pid"
        printf '%s\n' "$capture_pid" >> "$replay/launches"
    }
    flea_pid() { printf '%s\n' "$capture_pid"; }
    qs() {
        [[ "$1" != -p ]] || return 0
        [[ "$1 $2 $3 $4 $5" == "ipc --pid $capture_pid call flea" ]] || fail "IPC missed owned capture: $*"
        shift 5
        case "$1" in
            total) printf '4\n' ;;
            listInFlight) printf 'false\n' ;;
            rowAt) local -a names=(Alpha Beta notes.txt preview.png); printf '%s|file\n' "${names[$2]}" ;;
            cursor) printf '%s\n' "$cursor" ;;
            metrics) printf '13 12 14 37\n' ;;
            viewMode) printf 'columns\n' ;;
            # Opaque and bare Transparent launches draw opaque; glassy keeps its ground.
            glass) jq -cn --arg mode "$mode" --argjson focused "$([[ "$(cat "$replay/focus-pid")" == "$capture_pid" ]] && printf true || printf false)" \
                '{automatic:true, mode:$mode, focused:$focused, alpha:(if $mode == "glassy" then 192/255 else 1 end)}' ;;
            *) fail "unexpected IPC call: $*" ;;
        esac
    }
    # The registry entry the evaluated xw_ipc calls answers through the case's qs double, keeping every captured IPC byte owned.
    flea_ipc_helper_stub() { qs ipc "$@"; }
    FLEA_IPC_HELPER=flea_ipc_helper_stub
    omarchy-drive() {
        [[ "$1 $2" != 'window center' ]] || return 0
        if [[ "$1 $2 $3" == 'key --window com.thisisgm.flea' ]]; then
            printf 'ambiguous selector flea: 3 fixture windows\n' >&2
            return 1
        fi
        [[ "$1 $2 $3" == "key --window $(printf '0x%x' "$capture_pid")" ]] || fail "key missed owned address: $*"
        printf '%s\n' "$*" >> "$replay/keys"
        case "$4" in g) cursor=0 ;; j) cursor=$((cursor + 1)) ;; *) fail "unexpected key: $4" ;; esac
    }
    matrix_wait_column() {
        [[ "$*" == 'image preview.png' && "$(ipc cursor)" == 3 ]] || fail "preview did not follow owned keys"
        printf '%s\n' "$capture_pid" >> "$replay/previews"
        if [[ "$scenario" == interrupted ]]; then kill -KILL "$BASHPID"; fi
        [[ "$scenario" != failure ]] || fail "injected early capture failure"
    }
    glasspick_shot() {
        [[ "$ipc_pid" == "$capture_pid" && "$2" == "$capture_pid" ]] || fail "shot inherited the wrong capture pid"
        ipc glass >/dev/null
        printf '%s\n' "$capture_pid" >> "$replay/shots"
        glasspick_shots=$((glasspick_shots + 1))
    }
    if case_cap_glasspick > "$replay/capture.log" 2>&1; then status=0; else status=$?; fi
    if [[ "$scenario" == interrupted ]]; then
        [[ "$status" == "$interrupted_replay_status" ]] || fail "replay was not interrupted by SIGKILL: $status"
        printf '%s\n' "$interrupted_foreign_pid" >> "$replay/pids"
        printf 'GLASSPICK_CASE interrupted-status=%s\n' "$status"
        return "$status"
    fi
    expected=0
    [[ "$scenario" == success ]] || expected=1
    [[ "$status" == "$expected" ]] || { cat "$replay/capture.log"; fail "capture returned $status, expected $expected"; }
    [[ "$ipc_pid" == sentinel ]] || fail "capture changed caller IPC scope: $ipc_pid"
    [[ "$(trap -p EXIT)" == "$caller_trap" ]] || fail "capture replaced the caller EXIT trap"
    [[ "$(wc -l < "$replay/cleanup")" == 1 ]] || fail "actual EXIT cleanup did not run exactly once before return"
    [[ "$(tail -1 "$replay/focus")" == 0x64 ]] || fail "actual EXIT cleanup did not restore caller focus"
    while read -r pid; do
        ownership=0
        flea_process_owned "$pid" || ownership=$?
        [[ "$ownership" == 2 ]] || fail "owned fixture $pid survived actual cleanup (ownership=$ownership)"
    done < "$replay/pids"
    if [[ "$scenario" == check-* ]]; then
        [[ ! -s "$replay/shots" && ! -s "$replay/launches" && "$(wc -l < "$replay/pids")" == 1 ]] || fail "rejected Foot config reached a terminal or shot"
        [[ "$(wc -l < "$replay/foot-checks")" == 1 ]] || fail "rejected config bypassed Foot's check flag"
        grep -q 'invalid section name: colors' "$replay/capture.log" || fail "config refusal lost its diagnostic"
        if [[ "$scenario" == check-rejected ]]; then
            grep -q 'Foot config check failed (exit 1)' "$replay/capture.log" || fail "nonzero Foot check was accepted"
        else
            grep -q 'Foot config/startup diagnostics refuse capture' "$replay/capture.log" || fail "zero-exit Foot diagnostic was accepted"
        fi
        printf 'GLASSPICK_CASE foot=%s shots=0 fixtures=1 stopped=1 focus=restored ipc=restored trap=preserved\n' "$scenario"
        return
    fi
    if [[ "$scenario" != success ]]; then
        if [[ "$scenario" == failure ]]; then
            grep -q 'injected early capture failure' "$replay/capture.log" || fail "early failure has the wrong cause"
        else
            grep -q 'Foot config/startup diagnostics refuse capture' "$replay/capture.log" || fail "startup config errors reached a shot"
            [[ "$(wc -l < "$replay/foot-checks")" == 1 ]] || fail "startup refusal skipped preflight"
        fi
        [[ "$(wc -l < "$replay/pids")" == 3 && ! -s "$replay/shots" ]] || fail "early failure did not leave three fixtures for cleanup"
        grep -Eq '^ipc=[0-9]+ term=[0-9]+ back=[0-9]+ shots=0$' "$replay/cleanup" || fail "early cleanup lost its local fixture PIDs"
        printf 'GLASSPICK_CASE cleanup=%s status=1 fixtures=3 stopped=3 focus=restored ipc=restored trap=preserved\n' "$scenario"
        return
    fi
    [[ "$(wc -l < "$replay/launches") $(wc -l < "$replay/previews") $(wc -l < "$replay/shots") $(wc -l < "$replay/pids")" == '12 12 16 28' ]] || fail "capture matrix or owned fixture count changed"
    grep -qx 'ipc= term= back= shots=16' "$replay/cleanup" || fail "successful cleanup lost its locals"
    [[ "$(wc -l < "$replay/foot-checks")" == 12 ]] || fail "Foot checks did not cover the full matrix"
    [[ "$(grep -c '^GLASSPICK foot-config=ok$' "$replay/capture.log")" == 12 ]] || fail "missing Foot preflight receipts"
    [[ "$(grep -c '^GLASSPICK owned-keys=ok preview=preview.png$' "$replay/capture.log")" == 12 ]] || fail "missing owned key receipts"
    grep -qx 'GLASSPICK shots=16' "$replay/capture.log" || fail "missing complete native receipt"
    capture_pid=$(tail -1 "$replay/launches")
    printf '%s\n' "$capture_pid" > "$replay/focus-pid"
    ipc_pid=""
    if ( test_frontend=qml; key g ) > "$replay/ambiguous.log" 2>&1; then fail "broad selector was accepted"; fi
    grep -qx 'ambiguous selector flea: 3 fixture windows' "$replay/ambiguous.log" || fail "broad selector failed for another reason"
    if ( test_frontend=gpui; key g ) > "$replay/unowned.log" 2>&1; then fail "unowned native selector was accepted"; fi
    grep -q '^FAIL.*native input has no owned PID$' "$replay/unowned.log" || fail "unowned native selector failed for another reason"
    ipc_pid="$capture_pid"
    printf '12\n' > "$replay/focus-pid"
    if ( key g ) > "$replay/focus.log" 2>&1; then fail "foreign focus was accepted"; fi
    grep -q 'so this case sends no more keys' "$replay/focus.log" || fail "focus guard failed for another reason"
    [[ "$(wc -l < "$replay/keys")" == 48 ]] || fail "a refused key reached the driver"
    printf 'GLASSPICK_CASE targeting=owned launches=12 shots=16 scope=restored ambiguous=refused foreign-focus=refused\n'
    printf 'GLASSPICK_CASE cleanup=success status=0 fixtures=28 stopped=28 focus=restored ipc=restored trap=preserved\n'
)

interrupted_replay_control() {
    local run_root="$run_root" replay_owner="$run_root" foreign_pid="" interrupted_foreign_pid
    local log="$test_root/interrupted-outer.log" status pid ownership count=0 survivors=0 failed=false
    env FLEA_TEST_RUN_ROOT="$test_root/foreign" sleep "$control_lifetime_s" &
    foreign_pid=$!
    interrupted_foreign_pid="$foreign_pid"
    run_root="$test_root/foreign"
    wait_tag "$foreign_pid"
    run_root="$replay_owner"
    (
        foreign_pid=""
        trap cleanup EXIT
        capture_targeting interrupted
        status=$?
        exit "$status"
    ) > "$log" 2>&1
    status=$?
    [[ "$status" == "$interrupted_cleanup_status" ]] || failed=true
    # Sample input: GLASSPICK_CASE interrupted-status=137, followed by the outer trap's refusal.
    grep -qx "GLASSPICK_CASE interrupted-status=$interrupted_replay_status" "$log" || failed=true
    # Sample input: "FAIL glasspick: refusing to stop unowned or unreadable fixture 12345".
    grep -Fq "refusing to stop unowned or unreadable fixture $foreign_pid" "$log" || failed=true
    # Sample input (newlines shown as \n): 12345\n12346\n12347\n54321\n, with the foreign PID last.
    while read -r pid; do
        [[ "$pid" != "$foreign_pid" ]] || continue
        count=$((count + 1))
        ownership=0
        flea_process_owned "$pid" || ownership=$?
        if [[ "$ownership" != 2 ]]; then
            survivors=$((survivors + 1))
            glasspick_stop "$pid" || failed=true
        fi
    done < "$test_root/replay-interrupted/pids"
    [[ "$count" == "$interrupted_fixture_count" && "$survivors" == 0 ]] || failed=true
    run_root="$test_root/foreign"
    flea_process_owned "$foreign_pid" && kill -0 "$foreign_pid" || failed=true
    glasspick_stop "$foreign_pid" || failed=true
    foreign_pid=""
    if [[ "$failed" == true ]]; then
        cat "$log"
        fail "interrupted replay: outer trap left $survivors of $count fixtures alive (owner=$replay_owner)"
    fi
    printf 'GLASSPICK_CASE outer-interrupt=%s owned=%s stopped=%s foreign=alive refusal=checked\n' \
        "$interrupted_replay_status" "$count" "$count"
}
interrupted_replay_control

for scenario in success failure check-rejected check-diagnostic startup-error; do
    capture_targeting "$scenario" > "$test_root/replay-$scenario.log" 2>&1
    status=$?
    if [[ "$status" != 0 ]]; then cat "$test_root/replay-$scenario.log"; fail "$scenario replay failed (exit $status)"; fi
    [[ "$(cat "$test_root/replay-$scenario/caller-exit")" == caller-exit ]] || fail "$scenario did not preserve caller EXIT execution"
    cat "$test_root/replay-$scenario.log"
done
