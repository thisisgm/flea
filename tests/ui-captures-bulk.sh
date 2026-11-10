# BulkRename board captures (outside the default wanted list); tests/ui.sh supplies fixture, helpers and shots.

# One bulkRenameState read must satisfy a jq expression within a bound before a shot is taken, so every state is asserted, never assumed.
cap_bulk_expect() {
    local expression="$1" label="$2" observed
    for _ in $(seq 1 100); do
        observed=$(ipc bulkRenameState) || fail "cap_bulk: the bulk observer failed for $label"
        jq -e "$expression" <<< "$observed" >/dev/null && return 0
        sleep 0.05
    done
    fail "cap_bulk: $label: $observed"
}

# The caret into a named card stop is one Tab away, read back through the card's own focus reader.
cap_bulk_tab() {
    local want="$1"
    key -k Tab >/dev/null
    settle
    cap_bulk_expect ".focus == \"$want\"" "Tab never reached $want"
}

# Down and Up walk the name lines, read back through the card's own focus reader.
cap_bulk_updown() {
    local want="$1" press="$2"
    key -k "$press" >/dev/null
    settle
    cap_bulk_expect ".focus == \"$want\"" "$press never reached $want"
}

# One names-editor line rewritten whole: select all, delete, type the replacement.
cap_bulk_set_line() {
    local text="$1"
    key -M ctrl -k a -m ctrl -k BackSpace >/dev/null
    settle
    key "$text" >/dev/null
    settle
}

# BulkRename board: six-row card, Find/Replace fill, two invalid lines with reasons, a swap, focus on each button (callouts 4-5 unshot: one-undo is protocol-pinned, the editor needs a real terminal).
case_cap_bulk() {
    local dir="$fixture_root/cap-bulk" swap="$fixture_root/cap-bulk-swap"
    sandbox_scratch "$dir"
    printf 'x\n' > "$dir/IMG_4120.HEIC"
    printf 'x\n' > "$dir/IMG_4121.MOV"
    printf 'x\n' > "$dir/IMG_4122.HEIC"
    printf 'x\n' > "$dir/IMG_4123.HEIC"
    printf 'x\n' > "$dir/IMG_4124.MOV"
    printf 'notes\n' > "$dir/notes.txt"
    sandbox_scratch "$swap"
    : > "$swap/a.txt"
    : > "$swap/b.txt"
    seed_ui_state "$fixture_root/cap-bulk-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 6
    cap_resize 1100 700
    key -M ctrl -k a -m ctrl >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "6" ]] || fail "cap_bulk: ctrl-a selected $(ipc selectionCount), not 6"
    key -k F2 >/dev/null
    cap_bulk_expect ".opened" "F2 over six rows opened no bulk card"
    cap_bulk_expect '.count == 6 and .footer == "0 renamed · 6 unchanged" and .rename == "Rename" and .canRename == false' "the fresh card"
    shot bulk-card
    cap_bulk_expect '.focus == "Find"' "Find did not take the caret on open"
    key "IMG_" >/dev/null
    settle
    cap_bulk_tab Replace
    key "trip-" >/dev/null
    settle
    cap_bulk_expect '.footer == "5 renamed · 1 unchanged" and .rename == "Rename 5" and .canRename' "Find IMG_ with Replace trip-"
    shot bulk-find-replace
    cap_bulk_tab "line 1"
    cap_bulk_set_line "trip-4122.HEIC"
    cap_bulk_expect '.footer == "4 renamed · 1 unchanged · 1 invalid"' "the duplicated second line"
    cap_bulk_tab "line 2"
    cap_bulk_set_line "trip/4123.HEIC"
    cap_bulk_expect '.footer == "3 renamed · 1 unchanged · 2 invalid" and .rename == "Rename 3" and .canRename == false' "the two invalid lines"
    shot bulk-invalid
    key -k Escape >/dev/null
    settle
    cap_bulk_expect ".opened == false" "Escape did not close the bulk card"
    printf 'CAP_BULK card=ok find-replace=ok invalid=ok\n'
    kill_flea
    launch "$swap"
    wait_listing 2
    cap_resize 1100 700
    key -M ctrl -k a -m ctrl >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "2" ]] || fail "cap_bulk: ctrl-a selected $(ipc selectionCount), not 2"
    key -k F2 >/dev/null
    cap_bulk_expect ".opened" "F2 over two rows opened no bulk card"
    cap_bulk_expect '.focus == "Find"' "Find did not take the caret on open"
    cap_bulk_tab Replace
    cap_bulk_tab "line 1"
    cap_bulk_set_line "b.txt"
    cap_bulk_tab "line 2"
    cap_bulk_set_line "a.txt"
    cap_bulk_expect '.footer == "2 renamed · 0 unchanged" and .rename == "Rename 2" and .canRename' "the swapped names"
    shot bulk-swap
    key -k Tab >/dev/null
    settle
    cap_bulk_expect '.focus == "Edit in terminal"' "Tab never reached Edit in terminal"
    shot bulk-focus-terminal
    key -k Tab >/dev/null
    settle
    cap_bulk_expect '.focus == "Cancel"' "Tab never reached Cancel"
    shot bulk-focus-cancel
    key -k Tab >/dev/null
    settle
    cap_bulk_expect '.focus == "Rename"' "Tab never reached Rename"
    shot bulk-focus-rename
    key -k Escape >/dev/null
    settle
    cap_bulk_expect ".opened == false" "Escape did not close the swap card"
    printf 'CAP_BULK swap=ok focus=terminal,cancel,rename\n'
    kill_flea
}

# Up and Down walk the name lines and stop at the ends: 2 Down lands 3, two Ups land 1, Up on 1 stays.
case_cap_bulk_updown() {
    local dir="$fixture_root/cap-bulk-updown"
    sandbox_scratch "$dir"
    printf 'x\n' > "$dir/a.txt"
    printf 'x\n' > "$dir/b.txt"
    printf 'x\n' > "$dir/c.txt"
    printf 'x\n' > "$dir/d.txt"
    seed_ui_state "$fixture_root/cap-bulk-updown-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 4
    cap_resize 1100 700
    key -M ctrl -k a -m ctrl >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "4" ]] || fail "cap_bulk_updown: ctrl-a selected $(ipc selectionCount), not 4"
    key -k F2 >/dev/null
    cap_bulk_expect ".opened" "F2 over four rows opened no bulk card"
    cap_bulk_expect '.focus == "Find"' "Find did not take the caret on open"
    cap_bulk_tab Replace
    cap_bulk_tab "line 1"
    cap_bulk_tab "line 2"
    cap_bulk_updown "line 3" Down
    cap_bulk_updown "line 2" Up
    cap_bulk_updown "line 1" Up
    cap_bulk_updown "line 1" Up
    key -k Escape >/dev/null
    settle
    cap_bulk_expect ".opened == false" "Escape did not close the updown card"
    printf 'CAP_BULK_UPDOWN updown=ok\n'
    kill_flea
}
