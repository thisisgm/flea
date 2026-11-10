#!/usr/bin/env bash
# The failure-evidence helper of tests/ui-menus.sh never changes a verdict: under set -eu it returns 0 whatever fails.
set -u
repo="$(cd "$(dirname "$0")/.." && pwd)"
. "$repo/tools/flea-sandbox-guard"

fail() {
    printf 'FAIL menus-evidence: %s\n' "$*" >&2
    exit 1
}

# Sample input, the first extracted line: 'menus_evidence() {'
helper=$(sed -n '/^menus_evidence() {/,/^}/p' "$repo/tests/ui-menus.sh")
[[ -n "$helper" ]] || fail "could not extract menus_evidence from tests/ui-menus.sh"

sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-menus-evidence-XXXXXXXX") || fail "mktemp under $SANDBOX_ROOT failed"
: > "$test_root/$SANDBOX_MARKER" || fail "could not mark $test_root"
cleanup() { sandbox_remove "$test_root" 2>/dev/null || true; }
trap cleanup EXIT

# One case: the helper under errexit with every driver stubbed; shot is the omarchy-drive stub's body.
run_case() {
    local label="$1" evidence="$2" shot="$3" status
    case_output=$(
        set -eu
        ipc() { return 1; }
        hyprctl() { return 1; }
        omarchy-drive() { eval "$shot"; }
        evidence_dir="$evidence"
        eval "$helper"
        menus_evidence
        printf 'REACHED\n'
    ) 2>&1
    status=$?
    [[ "$status" -eq 0 ]] || fail "$label: the helper ended the run with status $status: $case_output"
    [[ "$case_output" == *REACHED* ]] || fail "$label: the run did not continue past the helper: $case_output"
}

run_case "failed capture" "$test_root/evidence" "return 1"
failed_shot=$case_output
grep -qx 'MENUS_EVIDENCE shot=failed' <<< "$failed_shot" || fail "a failed capture did not print its shot=failed line: $failed_shot"

: > "$test_root/blocker"
run_case "unwritable evidence dir" "$test_root/blocker/evidence" "return 0"
blocked=$case_output
grep -qx 'MENUS_EVIDENCE shot=failed' <<< "$blocked" || fail "an unwritable evidence dir did not print shot=failed: $blocked"

run_case "good capture" "$test_root/evidence" "return 0"
taken=$case_output
grep -q "^MENUS_EVIDENCE shot=$test_root/evidence/menus-failure-" <<< "$taken" || fail "a good capture did not name its png: $taken"
if grep -qx 'MENUS_EVIDENCE shot=failed' <<< "$taken"; then fail "a good capture said it failed: $taken"; fi
run_case "another case capture" "$test_root/evidence" "return 0"
first_png=$(sed -n 's/^MENUS_EVIDENCE shot=//p' <<< "$taken")
next_png=$(sed -n 's/^MENUS_EVIDENCE shot=//p' <<< "$case_output")
[[ "$first_png" != "$next_png" ]] || fail "two case subprocesses reused the same failure capture"
printf 'ok   menus_evidence returns 0 and says what happened under set -eu\n'

shot_helper=$(sed -n '/^menus_shot() {/,/^}/p' "$repo/tests/ui-menus.sh")
normal_capture() (
    set -eu
    run_root="$test_root"
    evidence_dir="$test_root/normal-evidence"
    omarchy-drive() { printf 'fixture image\n' > "$2"; }
    eval "$shot_helper"
    menus_shot shared-specimen
)
first_shot=$(normal_capture) || fail 'first ordinary case capture failed'
next_shot=$(normal_capture) || fail 'second ordinary case capture failed'
[[ "$first_shot" != "$next_shot" ]] || fail 'ordinary captures from two case subprocesses reused a path'
printf 'ok   ordinary captures retain both same-named specimens from different cases\n'

failure_helper=$(sed -n '/^fail() {/,/^}/p' "$repo/tests/ui.sh")
failure_control() {
    local mode="$1" path="$2" output status
    output=$(
        run_root="$test_root"
        evidence_dir="$path"
        test_frontend=gpui ipc_pid=42 FLEA_TEST_PID=42 window_pid=42
        [[ "$mode" != other-pid ]] || FLEA_TEST_PID=43
        [[ "$mode" != other-window ]] || window_pid=44
        flea_test_call() { [[ "$mode" != foreign ]] && printf 'true\n'; }
        hyprctl() { printf '[{"pid":%s,"class":"com.thisisgm.flea","address":"0x123"}]\n' "$window_pid"; }
        omarchy-drive() { printf 'fixture image\n' > "$2"; }
        eval "$failure_helper"
        fail 'expected control failure' 2>&1
    ) 2>&1
    status=$?
    [[ "$status" == 1 ]] || fail "failure evidence changed the failing exit status: $output"
    printf '%s\n' "$output"
}
owned_failure=$(failure_control owned "$test_root/evidence")
[[ "$owned_failure" == *"SHOT $test_root/evidence/failure-"* ]] || fail "owned failure left no capture: $owned_failure"
foreign_failure=$(failure_control foreign "$test_root/evidence")
[[ "$foreign_failure" != *SHOT* ]] || fail "unowned failure attempted a capture"
# Each ownership check alone must refuse while the window still answers ready.
other_pid_failure=$(failure_control other-pid "$test_root/evidence")
[[ "$other_pid_failure" != *SHOT* ]] || fail "a failure whose ipc_pid is not this run's window attempted a capture"
other_window_failure=$(failure_control other-window "$test_root/evidence")
[[ "$other_window_failure" != *SHOT* ]] || fail "a failure captured a window another process owns"
escaped_failure=$(failure_control owned "$test_root/evidence-other")
[[ "$escaped_failure" != *SHOT* ]] || fail "failure capture escaped its evidence directory"
printf 'ok   failed checks capture only an owned window inside their marked evidence root\n'

seek_helper=$(sed -n '/^seek_row_named() {/,/^}/p' "$repo/tests/ui.sh")
seek_control() (
    set -eu
    local wanted="$1" expected="$2" async_wait_ms=2000
    printf '1\n' > "$test_root/seek-cursor"
    ipc() {
        case "$1" in
            total) printf '3\n' ;;
            cursor)
                cat "$test_root/seek-cursor"
                [[ ! -f "$test_root/seek-pending" ]] || mv "$test_root/seek-pending" "$test_root/seek-cursor"
                ;;
            rowAt)
                case "$2" in 0) printf 'first|file\n' ;; 1) printf 'middle|file\n' ;; 2) printf 'last|file\n' ;; esac
                ;;
        esac
    }
    key() {
        case "$1" in
            g) printf '0\n' > "$test_root/seek-pending" ;;
            j) printf '%s\n' "$(( $(cat "$test_root/seek-cursor") + 1 ))" > "$test_root/seek-pending" ;;
        esac
    }
    eval "$seek_helper"
    seek_row_named "$wanted"
    [[ "$(cat "$test_root/seek-cursor")" == "$expected" && ! -f "$test_root/seek-pending" ]]
)
seek_control first 0 || fail 'seek returned before its queued Home key landed'
seek_control last 2 || fail 'seek returned before its queued movement landed'
printf 'ok   row seeking waits for native movement before observing the next row\n'

launcher_helper=$(sed -n '/^menus_launcher_fixture() {/,/^menus_open_with() {/p' "$repo/tests/ui-menus.sh" | sed '$d')
launcher_control() (
    set -eu
    menu_box="$test_root/launcher"
    mkdir -p "$menu_box/bin" "$menu_box/data/applications" "$menu_box/config" "$menu_box/list"
    : > "$menu_box/.flea-test-sandbox"
    menus_guard() { [[ "$1" == "$menu_box/"* && -f "$menu_box/.flea-test-sandbox" ]]; }
    trash_bus_address=fixture-private-bus
    eval "$launcher_helper"
    menus_launcher_fixture
    cat > "$menu_box/bin/record-gio" <<'SH'
#!/bin/bash
printf '%s\n' "$@" > "$FLEA_MENUS_RECORD"
SH
    chmod +x "$menu_box/bin/record-gio"
    export FLEA_MENUS_GIO="$menu_box/bin/record-gio" FLEA_MENUS_RECORD="$menu_box/arguments"
    for flag in -l -li; do
        "$menu_box/bin/gio" mount "$flag"
        [[ "$(cat "$FLEA_MENUS_RECORD")" == "$(printf 'mount\n%s' "$flag")" ]] || exit 1
    done
    printf 'untouched\n' > "$FLEA_MENUS_RECORD"
    for arguments in 'mount' 'mount -u /fixture' 'mount -e /fixture' 'mount -l extra' 'mount -li extra'; do
        read -r -a words <<< "$arguments"
        status=0
        "$menu_box/bin/gio" "${words[@]}" || status=$?
        [[ "$status" == 92 && "$(cat "$FLEA_MENUS_RECORD")" == untouched ]] || exit 1
    done
)
launcher_control || fail 'GIO fixture changed its read-only mount boundary'
printf 'ok   GIO fixture passes both exact listing probes and refuses mount mutations or extra arguments\n'

backend_helper=$(sed -n '/^backend_pids() {/,/^}/p' "$repo/tests/ui.sh")
backend_control() (
    set -eu
    flea_bin="$test_root/backend/flea"
    for pid in 41 42 43; do mkdir -p "$test_root/backend/$pid"; done
    printf '%s\0%s\0' "$flea_bin" --backend > "$test_root/backend/41/cmdline"
    printf '%s\0%s\0' "$flea_bin" --plugin-host > "$test_root/backend/42/cmdline"
    printf '%s\0%s\0' "$flea_bin" --backend > "$test_root/backend/43/cmdline"
    pgrep() { printf '41\n42\n43\n'; }
    flea_process_dir() { printf '%s/backend/%s\n' "$test_root" "$1"; }
    flea_process_owned() { [[ "$1" != 43 ]]; }
    eval "$backend_helper"
    [[ "$(backend_pids)" == 41 ]]
)
backend_control || fail 'backend discovery included the plugin host or a foreign backend'
printf 'ok   backend discovery separates the owned backend from its same-named plugin sibling\n'
