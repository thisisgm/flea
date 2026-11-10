#!/usr/bin/env bash
# Display-free proof that every Permissions Apply wait polls to a deadline and the in-flight pause always resumes its backend.
set -uo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)" || exit 1
. "$repo/tests/ui-captures.sh"
. "$repo/tests/ui-pdf.sh"
. "$repo/tools/flea-sandbox-guard"
sandbox_root_ok
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-capperm.XXXXXX") || exit 1
: > "$scratch/$SANDBOX_MARKER"
trap 'sandbox_remove "$scratch"' EXIT

checks=0
failed=0
foreign="y-foreign.txt keeps its mode because you do not own it."
foreign_applied="y-foreign.txt kept its mode."
other="Could not inspect permissions: item not found."
held="zz-gone.txt keeps its mode: $other"
left="zz-gone.txt kept its mode."
applied="special.txt kept its mode."
note="2 items keep their modes because a special bit is set: special.txt, x-special.txt."

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}
# The harness's own pieces, each one a stub: this suite drives the helpers' logic and no display.
sandbox_scratch() { mkdir -p -- "$1"; }
click_row() { :; }
settle() { :; }
shot() { :; }
kill_flea() { printf 'kill\n' >> "$log"; }
backend_pids() { printf '4242\n'; }
convert_pause_backend() { printf 'pause %s\n' "$1" >> "$log"; }
permissions_resume_stopped() { [[ -z "$1" ]] || { printf 'resume %s\n' "$1" >> "$log"; printf '1\n' > "$sdir/resumed"; }; }
cap_permissions_focus() { :; }
menu_seek() { :; }
# The listing wait logs whether the file was already gone, so the order rm then wait is proven.
wait_listing() { local state=present; [[ -e "$fixture_root/zz-gone.txt" ]] || state=removed; printf 'listing %s %s\n' "$1" "$state" >> "$log"; }
# A sleep advances the shell's own clock, so a wait's deadline passes without a wall-clock second.
sleep() { SECONDS=$((SECONDS + 1)); }
# Each Return is one Apply and each open one card, which is what picks the state the reader answers.
key() { [[ "$1 $2" == "-k Return" ]] && bump returns; return 0; }
cap_permissions_open() { bump opens; }
bump() { printf '%s\n' "$(( $(cat "$sdir/$1") + 1 ))" > "$sdir/$1"; }
# A reply the card gives at its first read and then stops giving: the pre-state the old one-shot asserts read.
first_read() {
    local reads
    reads=$(cat "$sdir/reads")
    printf '%s\n' "$((reads + 1))" > "$sdir/reads"
    (( reads == 0 ))
}
closed_state='{"opened":false,"busy":false,"displayedError":"","controls":[]}'
flight_state='{"opened":true,"busy":true,"displayedError":"","controls":[{"name":"Cancel","enabled":false}]}'
idle_state='{"opened":true,"busy":false,"displayedError":"","controls":[{"name":"Cancel","enabled":true}]}'
# The all-skipped card: nine disabled boxes, four on (rw-r--r--), Apply disabled, the note kept; the live variant is the defect, an Apply that can only fail.
boxes_state() {
    local apply_enabled="$1" bit rows=""
    for bit in 256 128 64 32 16 8 4 2 1; do
        rows+=",{\"name\":\"box\",\"bit\":$bit,\"enabled\":false,\"value\":\"$([[ " 256 128 32 4 " == *" $bit "* ]] && echo on || echo off)\"}"
    done
    printf '{"opened":true,"busy":false,"displayedError":"%s","controls":[{"name":"Apply","enabled":%s}%s]}' "$note" "$apply_enabled" "$rows"
}
all_skipped=$(boxes_state false)
all_skipped_live=$(boxes_state true)
ipc() {
    local returns opens resumed
    returns=$(cat "$sdir/returns")
    opens=$(cat "$sdir/opens")
    resumed=$(cat "$sdir/resumed")
    case "$1/$mode" in
        statusPrimary/skips|statusPrimary/stuck|statusPrimary/liveapply) (( returns == 1 )) && printf '%s\n' "$applied" || printf '%s\n' "$foreign_applied" ;;
        permissionsState/skips|permissionsState/stuck|permissionsState/liveapply)
            if (( returns == 1 && opens == 1 )); then
                if [[ "$mode" == stuck ]]; then printf '%s\n' "$idle_state"; else printf '%s\n' "$closed_state"; fi
            elif (( returns == 1 && opens == 2 )); then
                if [[ "$mode" == liveapply ]]; then printf '%s\n' "$all_skipped_live"; else printf '%s\n' "$all_skipped"; fi
            elif (( returns == 1 )); then printf '{"opened":true,"busy":false,"displayedError":"%s"}\n' "$foreign"
            else printf '%s\n' "$closed_state"; fi ;;
        previewSwapState/fading) first_read && printf '{"look":{},"lookVisible":true}\n' || printf '{"look":{},"lookVisible":false}\n' ;;
        previewSwapState/faded-never) printf '{"look":{},"lookVisible":true}\n' ;;
        contextMenuVisible/*) printf 'true\n' ;;
        menuState/*) printf '{"entries":[{"action":"permissions","disabled":false}]}\n' ;;
        statusPrimary/vanish|statusPrimary/vanishnoreapply) printf '%s\n' "$left" ;;
        statusPrimary/vanishwrongleft) printf '%s\n' "zz-gone.txt kept its mode" ;;
        permissionsState/vanish|permissionsState/vanishwrong|permissionsState/vanishbox|permissionsState/vanishnoreapply|permissionsState/vanishwrongleft)
            if (( returns == 0 )); then
                # Before Apply the box reads off, and the Space turns it on unless the key never landed.
                if [[ "$mode" == vanishbox ]]; then printf '{"opened":true,"busy":false,"displayedError":"","controls":[{"name":"Owner execute","value":"off"}]}\n'
                else printf '{"opened":true,"busy":false,"displayedError":"","controls":[{"name":"Owner execute","value":"on"}]}\n'; fi
            elif (( returns == 1 )); then
                # The defect: the card keeps the bare batch error where the note naming the file belongs.
                if [[ "$mode" == vanishwrong ]]; then printf '{"opened":true,"busy":false,"displayedError":"%s"}\n' "$other"
                else printf '{"opened":true,"busy":false,"displayedError":"%s"}\n' "$held"; fi
            elif [[ "$mode" == vanishnoreapply ]]; then printf '{"opened":true,"busy":false,"displayedError":"%s"}\n' "$held"
            else printf '%s\n' "$closed_state"; fi ;;
        permissionsState/flight|permissionsState/flightnow|permissionsState/never)
            if (( resumed == 1 )); then printf '%s\n' "$closed_state"
            elif [[ "$mode" == never ]] || (( returns == 0 )); then printf '%s\n' "$idle_state"
            elif [[ "$mode" == flight ]] && first_read; then printf '%s\n' "$idle_state"
            else printf '%s\n' "$flight_state"; fi ;;
        *) return 2 ;;
    esac
}

# run NAME MODE FUNCTION: one helper in a fresh subshell and state dir; sets rc and log.
run() {
    name="$1"; mode="$2"
    sdir="$scratch/$name"
    log="$sdir/log"
    mkdir -p -- "$sdir"
    printf '0\n' > "$sdir/returns"; printf '0\n' > "$sdir/opens"; printf '0\n' > "$sdir/reads"; printf '0\n' > "$sdir/resumed"; : > "$log"
    (
        fixture_root="$sdir"
        unset SECONDS
        SECONDS=0
        # As tests/ui.sh runs a case: the parent's cleanup is cleared first.
        trap - EXIT
        "$3"
    ) > "$sdir/out" 2>&1
    rc=$?
}
expect() {
    checks=$((checks + 1))
    if [[ "$2" == "$3" ]]; then printf 'ok %s\n' "$1"; else printf 'FAIL %s: got [%s], want [%s]\n' "$1" "$2" "$3"; failed=$((failed + 1)); fi
}

run flight-settles flight cap_permissions_inflight
expect "inflight waits for the busy state, then resumes" "$rc $(tr '\n' ' ' < "$log")" "0 pause 4242 resume 4242 "
run flight-now flightnow cap_permissions_inflight
expect "inflight passes when the state is already there" "$rc" "0"
# Before any Apply the card reads idle in every flight mode, so the wait cannot pass on a state read ahead of the Return.
pre_apply_read() { ipc permissionsState; }
run pre-apply flightnow pre_apply_read
expect "a read before the first Apply answers idle, never in flight" "$(cat "$sdir/out")" "$idle_state"
run flight-fail never cap_permissions_inflight
expect "inflight on a failed wait still resumes the backend and kills the window" "$rc $(tr '\n' ' ' < "$log")" "1 pause 4242 resume 4242 kill "
expect "an Apply wait that never settles ends at its own deadline" "$(grep -c 'Cancel stays disabled and no Close draws while Apply is in flight, last state' "$sdir/out")" "1"
run skips-settle skips cap_permissions_skips
expect "skips pass on the card the all-skipped selection settles on, then the foreign file" "$rc $(grep -c . "$sdir/out")" "0 0"
run skips-stuck stuck cap_permissions_skips
expect "skips fail at a deadline when the first Apply never closes the card" "$rc $(grep -c 'the card never closed after Apply' "$sdir/out")" "1 1"
run skips-live liveapply cap_permissions_skips
expect "skips fail when the all-skipped card keeps a live Apply or an enabled box" "$rc $(grep -c 'is not nine disabled boxes holding the files' "$sdir/out")" "1 1"

# One call of the helper against the file the stub's fixture would hold, named after the 8 rows left once it goes.
vanish_case() { : > "$fixture_root/zz-gone.txt"; cap_permissions_vanished 0 "$fixture_root/zz-gone.txt" 8; }
run vanished vanish vanish_case
expect "a file removed under the open card, the listing waited at 8 rows, the note naming it, then a second Apply that closes the card and names the file it left" "$rc $(tr '\n' ' ' < "$log")" "0 listing 8 removed "
run vanished-wrong vanishwrong vanish_case
expect "the bare batch error with no file name fails the vanished-file shot" "$rc $(grep -c 'draws another line than the note naming it' "$sdir/out")" "1 1"
run vanished-box vanishbox vanish_case
expect "a box that never turns on fails before Apply" "$rc $(grep -c 'Owner execute did not turn on before Apply' "$sdir/out")" "1 1"
run vanished-noreapply vanishnoreapply vanish_case
expect "a second Apply that never closes the card fails at its deadline" "$rc $(grep -c 'the card never closed after Apply' "$sdir/out")" "1 1"
run vanished-wrong-left vanishwrongleft vanish_case
expect "a second Apply whose status line does not name the file it left fails" "$rc $(grep -c 'the status line after the second Apply reads' "$sdir/out")" "1 1"

# The Quick Look wait before a listing shot: it returns once the overlay stops drawing, and fails at its deadline while it still does.
overlay_gone() { pdf_overlay_gone listing; }
run fading fading overlay_gone
expect "the listing shot waits while Quick Look still fades, then goes" "$rc $(grep -c . "$sdir/out")" "0 0"
run faded-never faded-never overlay_gone
expect "a Quick Look that never stops drawing fails at the deadline" "$rc $(grep -c 'Quick Look still draws after Escape' "$sdir/out")" "1 1"

printf 'ui-captures-permissions: %s checks, %s failed\n' "$checks" "$failed"
(( failed == 0 ))
