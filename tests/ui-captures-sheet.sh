#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
# Runs the real capture case with controlled replies and a clock advanced only by settle.
set -uo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)" || exit 1
. "$repo/tests/ui-captures.sh"
. "$repo/tools/flea-sandbox-guard"
sandbox_root_ok
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-capsheet.XXXXXX") || exit 1
: > "$scratch/$SANDBOX_MARKER"
trap 'sandbox_remove "$scratch"' EXIT

deadline_s=10
advance_s=1
delayed_clear_s=2
# The case closes the sheet with Escape after trash, fl, comp and mute, and each close waits out the delay once.
sheet_escapes=4
failed=0
checks=0

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

seed_ui_state() {
    :
}

# The real helper wipes the fixture home and copies the box's theme into it; the stub keeps the wipe.
fixture_home_make() {
    sandbox_scratch "$1"
}

launch() {
    :
}

wait_listing() {
    :
}

# Sample input: cap-sheet-query-trash, appended to the case's shot list.
shot() {
    printf '%s\n' "$1" >> "$case_dir/shots"
}

kill_flea() {
    :
}

# The sheet opens on ?, Escape closes it (or the card standing over it), Enter on a row closes it and opens a card, Down moves the cursor, Tab flips a confirm card's button.
key() {
    if [[ "$1" == -k && "$2" == Escape ]]; then
        if [[ "$confirm_open" == true ]]; then
            [[ "$scenario" == confirm-stays ]] || confirm_open=false
        elif [[ "$dialog_open" == true ]]; then
            [[ "$scenario" == dialog-stays ]] || dialog_open=false
        else
            escape_at=$SECONDS
        fi
    elif [[ "$1" == -k && "$2" == Return ]]; then
        sheet_open=false
        if [[ "$typed_query" == perm && "$cursor_row" == 0 ]]; then
            [[ "$scenario" == confirm-never ]] || confirm_open=true
            confirm_danger=false
        else
            [[ "$scenario" == dialog-never ]] || dialog_open=true
        fi
    elif [[ "$1" == -k && "$2" == Down ]]; then
        cursor_row=$((cursor_row + 1))
    elif [[ "$1" == -k && "$2" == Tab ]]; then
        [[ "$scenario" == tab-stays ]] || { [[ "$confirm_danger" == true ]] && confirm_danger=false || confirm_danger=true; }
    elif [[ "$1" == '?' ]]; then
        sheet_open=true
        escape_at=-1
        typed_query=""
        cursor_row=0
    elif [[ "$1" != -k ]]; then
        typed_query+="$1"
        cursor_row=0
    fi
}

settle() {
    SECONDS=$((SECONDS + advance_s))
    printf '%s\n' "$SECONDS" > "$case_dir/elapsed"
}

# Sample input: the perm query, answered with the action row and the live Permissions row; a bad scenario answers one wrong row.
sheet_rows() {
    case "$typed_query" in
        perm)
            case "$scenario" in
                dup-delete) printf 'shift-delete delete permanently\nshift-delete delete permanently\n Permissions\n' ;;
                perm-disabled) printf 'shift-delete delete permanently\n Permissions (disabled)\n' ;;
                rank-moved) printf ' Permissions\nshift-delete delete permanently\n' ;;
                *) printf 'shift-delete delete permanently\n Permissions\n' ;;
            esac ;;
        trash) printf ' Open Trash\nd trash\n' ;;
        # The mute row stands in the Preview context only, so a stub that drops it must fail the case's row check.
        mute) [[ "$scenario" == mute-missing ]] || printf 'm mute\n' ;;
        fl)
            case "$scenario" in
                fl-no-recent) printf ' Open flea\n' ;;
                fl-no-place) printf ' Open mix.flac\n' ;;
                fl-order) printf ' Open mix.flac\n Open flea\n' ;;
                # The recent row stands only while the case's own history survives in its fixture home.
                *) printf ' Open flea\n'; if [[ -f "$fixture_root/cap-sheet-home/.local/share/recently-used.xbel" ]]; then printf ' Open mix.flac\n'; fi ;;
            esac ;;
        comp)
            if [[ "$scenario" == comp-cap ]]; then printf ' Compress to .zip\nz Compress to .tar\n'
            elif [[ "$scenario" == comp-parent-only ]]; then printf ' Compress\n'
            elif [[ "$scenario" == comp-parent-last ]]; then printf ' Compress to .zip\n Compress\n'
            else printf ' Compress to .zip\n Compress to .tar\n'; fi ;;
    esac
}

# Sample input: {"opened":true,"count":1,"destructiveFocus":false,"title":"Delete 1 item permanently?"}, the card Enter on Delete permanently opens.
menu_dialog_state() {
    if [[ "$confirm_open" != true ]]; then
        printf '{"opened":false,"confirmation":{"opened":false}}\n'
        return
    fi
    local count=1
    [[ "$scenario" == confirm-count ]] && count=2
    printf '{"opened":true,"confirmation":{"opened":true,"count":%s,"destructiveFocus":%s,"title":"Delete %s item permanently?"}}\n' \
        "$count" "$confirm_danger" "$count"
}

# Sample input: [{"keys":"m","label":"mute","where":" in Preview","disabled":false}], the sheet's result rows with the where each delegate draws.
sheet_results() {
    local mix_where=" in ~/Documents/claude" flea_where=" in Favorites" mute_where=" in Preview"
    [[ "$scenario" == mute-no-where ]] && mute_where=""
    [[ "$scenario" == mute-wrong-where ]] && mute_where=" in Places"
    [[ "$scenario" == fl-no-fav-where ]] && flea_where=""
    [[ "$scenario" == fl-recent-where-moved ]] && mix_where=" in ~/Documents"
    case "$typed_query" in
        mute) [[ "$scenario" == mute-missing ]] && printf '[]\n' \
            || printf '[{"keys":"m","label":"mute","where":"%s","disabled":false}]\n' "$mute_where" ;;
        fl)
            printf '[{"keys":"","label":"Open flea","where":"%s","disabled":false}' "$flea_where"
            if [[ -f "$fixture_root/cap-sheet-home/.local/share/recently-used.xbel" ]]; then
                printf ',{"keys":"","label":"Open mix.flac","where":"%s","disabled":false}' "$mix_where"
            fi
            printf ']\n' ;;
        *) printf '[]\n' ;;
    esac
}

# What keymapSheetOpen answers after an Escape, by scenario; relative to the Escape, so each close is judged alone.
sheet_open_reply() {
    if [[ "$escape_at" -lt 0 ]]; then
        printf '%s\n' "$sheet_open"
    elif [[ "$scenario" == persistent ]]; then
        printf 'true\n'
    elif [[ "$scenario" == whitespace ]]; then
        printf ' \n'
    elif [[ "$scenario" == ipc-failure ]]; then
        return 1
    elif (( SECONDS - escape_at < clear_after_s )); then
        printf 'true\n'
    else
        printf 'false\n'
    fi
}

ipc() {
    case "$1" in
        keymapSheetOpen) sheet_open_reply ;;
        keymapSheetRows) sheet_rows ;;
        keymapSheetResults) sheet_results ;;
        keymapQuery) printf '%s\n' "$typed_query" ;;
        menuDialogState) menu_dialog_state ;;
        permissionsState) printf '{"opened":%s,"busy":false}\n' "$dialog_open" ;;
        *) return 2 ;;
    esac
}

# Sample input: flea_ipc_wait -p /fixture/ui/boot flea keymapQuery "" --timeout 10
flea_ipc_wait() {
    local out
    out=$(ipc "$4") || return $?
    [[ "$out" == *"$5"* ]]
}

# The shots a clean run takes, in order: the sheet at rest, then the query with a place, a place beside a recent file, leaves alone, a key that works in one place, a live row, and the delete card on Cancel and on Delete.
expected_shots='cap-sheet-rest
cap-sheet-query-trash
cap-sheet-query-fl
cap-sheet-query-comp
cap-sheet-query-mute
cap-sheet-query
cap-sheet-query-perm-file
cap-sheet-delete-card
cap-sheet-delete-tab'

check_case() {
    local scenario="$1" expected_rc="$2" clear_after_s="$3" expected_elapsed="$4" diagnostic="$5"
    local case_dir="$scratch/$scenario" rc elapsed
    mkdir -p -- "$case_dir"
    : > "$case_dir/$SANDBOX_MARKER"
    printf '0\n' > "$case_dir/elapsed"
    (
        fixture_root="$case_dir"
        flea_ui="$case_dir/ui"
        sheet_open=false
        dialog_open=false
        escape_at=-1
        typed_query=""
        cursor_row=0
        confirm_open=false
        confirm_danger=false
        unset SECONDS
        SECONDS=0
        case_cap_sheet
    ) > "$case_dir/log" 2>&1
    rc=$?
    elapsed=$(cat "$case_dir/elapsed")
    checks=$((checks + 1))
    # Sample input: CAP_SHEET rest=ok queries=trash,fl,comp,mute,perm permissions=opened delete=cancel-then-delete
    if [[ "$rc" != "$expected_rc" || "$elapsed" != "$expected_elapsed" ]]; then
        printf 'FAIL %s: expected exit %s at %s s, got exit %s at %s s\n' \
            "$scenario" "$expected_rc" "$expected_elapsed" "$rc" "$elapsed"
        failed=$((failed + 1))
    elif [[ -n "$diagnostic" ]] && ! grep -Fq -- "$diagnostic" "$case_dir/log"; then
        printf 'FAIL %s: missing diagnostic %s\n' "$scenario" "$diagnostic"
        failed=$((failed + 1))
    elif [[ "$rc" == 0 ]] && ! grep -Fxq 'CAP_SHEET rest=ok queries=trash,fl,comp,mute,perm permissions=opened delete=cancel-then-delete' "$case_dir/log"; then
        printf 'FAIL %s: capture reported no success\n' "$scenario"
        failed=$((failed + 1))
    elif [[ "$rc" == 0 && "$(cat "$case_dir/shots")" != "$expected_shots" ]]; then
        printf 'FAIL %s: the shot list is %s\n' "$scenario" "$(tr '\n' ' ' < "$case_dir/shots")"
        failed=$((failed + 1))
    elif [[ "$rc" != 0 ]] && grep -q '^CAP_SHEET ' "$case_dir/log"; then
        printf 'FAIL %s: rejected sheet still reported capture success\n' "$scenario"
        failed=$((failed + 1))
    else
        printf 'ok %s\n' "$scenario"
    fi
}

# Each close is judged alone: a sheet that stays open (or answers blank) holds the deadline, a late close misses it.
check_case persistent 1 0 "$deadline_s" "last value 'true'"
check_case whitespace 1 0 "$deadline_s" "last value ' '"
check_case delayed 0 "$delayed_clear_s" "$((sheet_escapes * delayed_clear_s))" ""
check_case immediate 0 0 0 ""
check_case late 1 "$((deadline_s + advance_s))" "$deadline_s" "last value 'true'"
check_case ipc-failure 1 0 0 "keymapSheetOpen failed"
# Each assertion the case adds has a control: the stub answers the bad value and the case must fail with that assertion's message.
check_case dup-delete 1 0 0 "delete permanently is listed more than once"
check_case perm-disabled 1 0 0 "Permissions reads unavailable"
check_case comp-cap 1 0 0 "the comp query lists a row with a cap"
check_case comp-parent-only 1 0 0 "the comp query lists no Compress to .zip leaf row"
check_case comp-parent-last 1 0 0 "the comp query lists the Compress parent"
check_case mute-missing 1 0 "$deadline_s" "the mute query lists no m mute row"
check_case mute-no-where 1 0 "$deadline_s" "the mute row draws no where Preview"
check_case mute-wrong-where 1 0 "$deadline_s" "the mute row draws no where Preview"
check_case fl-no-fav-where 1 0 "$deadline_s" "the favourite flea draws no where Favorites"
check_case fl-recent-where-moved 1 0 "$deadline_s" "the recent mix.flac draws no where ~/Documents/claude"
check_case fl-no-recent 1 0 "$deadline_s" "the fl query lists no recent file mix.flac"
check_case fl-no-place 1 0 0 "the fl query lists no favourite flea"
check_case fl-order 1 0 0 "the fl query does not lead with the favourite"
check_case rank-moved 1 0 0 "the second perm row is not Permissions"
check_case dialog-never 1 0 "$deadline_s" "Enter on Permissions opened no dialog"
check_case dialog-stays 1 0 "$deadline_s" "Escape did not close the Permissions dialog"
check_case confirm-never 1 0 "$deadline_s" "Enter on Delete permanently opened no card"
check_case confirm-count 1 0 0 "the delete card does not ask about 1 item"
check_case tab-stays 1 0 "$deadline_s" "Tab did not move the delete card's focus to Delete"
check_case confirm-stays 1 0 "$deadline_s" "Escape did not close the delete card"
printf 'ui-captures-sheet: %s checks, %s failed\n' "$checks" "$failed"
(( failed == 0 ))
