# GPUI overrides for UISH-21; seeded from the unchanged oracle at 2faf8d099.
case_dd() {
    local dir="$fixture_root/dd"
    sandbox_scratch "$dir"
    local i dd_trash
    for i in 1 2 3 4 5 6; do printf 'body\n' > "$dir/f$i.txt"; done
    # The trash this case fills is its own, inside the sandbox this case owns, never the operator's.
    export XDG_DATA_HOME="$fixture_root/dd-data"
    mkdir -p "$XDG_DATA_HOME"
    # GLib trashes to the home store or the fixture mount's top trash by device; both stay off the operator's.
    dd_trash=$(sandbox_trash_files_dir "$dir/f2.txt") || fail "dd: no GLib trash directory for $dir/f2.txt"

    launch "$dir"
    wait_listing 6

    echo "-- a block leaves together, and the cursor takes the block's place --"
    goto_row 1
    key v >/dev/null
    key J >/dev/null
    settle
    [[ "$(ipc selectedIndices)" == "1,2" ]] || fail "dd: the block is $(ipc selectedIndices), not rows 1,2"
    trash_trap_own "$dd_trash" "$dir" "dd" f2.txt f3.txt
    key d >/dev/null
    wait_message "Press d again to move them to Trash, or press Delete on its own."
    key d >/dev/null
    for _attempt in $(seq 1 40); do [[ -e "$dir/f3.txt" ]] || break; sleep 0.25; done
    [[ -e "$dir/f3.txt" ]] && fail "dd: the block was not trashed, the bar reads $(ipc lastMessage)"
    # Hard rule 9 in the case itself: the rows went to this case's own trash and not the operator's.
    [[ -s "$dd_trash/f2.txt" && -s "$dd_trash/f3.txt" ]] \
        || fail "dd: the block did not land in $dd_trash, which holds $(ls -A "$dd_trash" 2>&1)"
    wait_listing 4
    settle
    [[ "$(ipc cursor)" == "1" ]] \
        || fail "dd: after a block the cursor is $(ipc cursor), not 1, the row the block left"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "f4.txt|"* ]] \
        || fail "dd: after a block the cursor sits on $(ipc rowAt "$(ipc cursor)"), not the row that slid up"
    printf 'DD block row=%s cursor=%s\n' "$(ipc rowAt "$(ipc cursor)")" "$(ipc cursor)"

    echo "-- one row, and the cursor keeps its own index --"
    key -k Escape >/dev/null
    settle
    seek_row_named "f5.txt"
    local at
    at=$(ipc cursor)
    trash_trap_own "$dd_trash" "$dir" "dd" f5.txt
    key d >/dev/null
    wait_message "Press d again to move it to Trash, or press Delete on its own."
    key d >/dev/null
    for _attempt in $(seq 1 40); do [[ -e "$dir/f5.txt" ]] || break; sleep 0.25; done
    [[ -e "$dir/f5.txt" ]] && fail "dd: the row was not trashed, the bar reads $(ipc lastMessage)"
    wait_listing 3
    settle
    [[ "$(ipc cursor)" == "$at" ]] || fail "dd: the cursor left row $at for $(ipc cursor)"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "f6.txt|"* ]] \
        || fail "dd: the cursor sits on $(ipc rowAt "$(ipc cursor)"), not the row that slid up"

    # The delete's anchor selects the row it landed on, which is the cursor row, so the next dd
    # takes the cursor's row; Escape is what clears the selection.
    [[ "$(ipc selectionCount)" == "1" ]] \
        || fail "dd: the row the cursor landed on is not selected, count is $(ipc selectionCount)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "0" ]] || fail "dd: Escape left $(ipc selectionCount) rows selected"

    echo "-- the last row clamps rather than running past the end --"
    seek_row_named "f6.txt"
    at=$(ipc cursor)
    trash_trap_own "$dd_trash" "$dir" "dd" f6.txt
    key d >/dev/null
    wait_message "Press d again to move it to Trash, or press Delete on its own."
    key d >/dev/null
    for _attempt in $(seq 1 40); do [[ -e "$dir/f6.txt" ]] || break; sleep 0.25; done
    [[ -e "$dir/f6.txt" ]] && fail "dd: the last row was not trashed, the bar reads $(ipc lastMessage)"
    wait_listing 2
    settle
    [[ "$(ipc cursor)" == "$((at - 1))" ]] \
        || fail "dd: after the last row the cursor is $(ipc cursor), not $((at - 1))"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "f4.txt|"* ]] \
        || fail "dd: the clamped cursor sits on $(ipc rowAt "$(ipc cursor)")"
    kill_flea
    # A mount-top trash is shared, so only this run's own entries go, each pair checked in the command.
    if [[ "$dd_trash" != "$XDG_DATA_HOME/Trash/files" ]]; then
        sandbox_trash_remove_own "$dd_trash" "$dir" f2.txt f3.txt f5.txt f6.txt \
            || fail "dd: owned entries could not be removed from $dd_trash"
    fi
}

case_ddclick() {
    local view dir count cur want stride ddclick_trash
    for view in list grid; do
        dir="$fixture_root/ddclick-$view"
        sandbox_scratch "$dir"
        # The grid leg holds two full strides plus one, so Down from 0 lands mid-list and cannot clamp to a short last row.
        if [[ "$view" == grid ]]; then count=61; else count=2; fi
        local i
        for i in $(seq 0 $((count - 1))); do printf 'body\n' > "$dir/f$i.txt"; done
        export XDG_DATA_HOME="$fixture_root/ddclick-$view-data"
        mkdir -p "$XDG_DATA_HOME"
        # GLib trashes to the home store or the fixture mount's top trash by device.
        ddclick_trash=$(sandbox_trash_files_dir "$dir/f0.txt") || fail "ddclick ($view): no GLib trash directory for $dir/f0.txt"

        launch "$dir"
        wait_listing "$count"
        if [[ "$view" == grid ]]; then
            click_chrome grid
            settle
            [[ "$(ipc viewMode)" == grid ]] || fail "ddclick: the chrome did not switch to the grid"
            stride="$(ipc gridColumns)"
            [[ "$stride" =~ ^[0-9]+$ && "$stride" -ge 2 ]] || fail "ddclick (grid): gridColumns answered '$stride'"
            (( count >= 2 * stride + 1 )) || fail "ddclick (grid): $count files hold fewer than 2*$stride+1, so Down could clamp"
        fi

        click_row 0 left
        settle
        [[ "$(ipc cursor)" == "0" ]] || fail "ddclick ($view): the click left the cursor on $(ipc cursor), not 0"
        [[ "$(ipc selectedIndices)" == "0" ]] || fail "ddclick ($view): the click selected $(ipc selectedIndices), not 0"
        key -k Down >/dev/null
        settle
        cur="$(ipc cursor)"
        if [[ "$view" == grid ]]; then
            [[ "$cur" == "$stride" ]] || fail "ddclick (grid): Down left the cursor on $cur, not one stride ($stride) below 0"
        else
            [[ "$cur" == "1" ]] || fail "ddclick ($view): Down left the cursor on $cur, not 1"
        fi
        [[ "$(ipc selectedIndices)" == "$cur" ]] || fail "ddclick ($view): the lone selection is $(ipc selectedIndices), not the cursor row $cur"
        want="f$cur.txt"
        trash_trap_own "$ddclick_trash" "$dir" "ddclick ($view)" "$want"
        key d >/dev/null
        wait_message "Press d again to move it to Trash, or press Delete on its own."
        key d >/dev/null
        for _attempt in $(seq 1 40); do [[ -e "$dir/$want" ]] || break; sleep 0.25; done
        [[ -e "$dir/$want" ]] && fail "ddclick ($view): $want survived, the bar reads $(ipc lastMessage)"
        [[ -e "$dir/f0.txt" ]] || fail "ddclick ($view): the clicked f0.txt went to Trash instead of the cursor row"
        # Hard rule 9 in the case itself: the row went to this case's own trash and not the operator's.
        [[ -s "$ddclick_trash/$want" ]] \
            || fail "ddclick ($view): $want did not land in $ddclick_trash, which holds $(ls -A "$ddclick_trash" 2>&1)"
        printf 'DDCLICK view=%s kept=f0.txt trashed=%s cursor=%s\n' "$view" "$want" "$cur"
        kill_flea
        # A mount-top trash is shared, so only this run's own entry goes, each pair checked in the command.
        if [[ "$ddclick_trash" != "$XDG_DATA_HOME/Trash/files" ]]; then
            sandbox_trash_remove_own "$ddclick_trash" "$dir" "$want" \
                || fail "ddclick ($view): owned entries could not be removed from $ddclick_trash"
        fi
    done
}

case_operations() {
    local dir="$fixture_root/operations"
    sandbox_scratch "$dir"
    printf 'body\n' > "$dir/notes.txt"
    magick -size 48x32 xc:navy "$dir/shot.png"
    bsdtar -a -c -f "$dir/bundle.tar.zst" -C "$dir" notes.txt
    # #165: the card case needs an extraction still running when the harness looks.
    python3 - "$dir/slowpayload" <<'PY' || fail "operations: the slow extraction fixture could not be populated"
from pathlib import Path
import sys

root = Path(sys.argv[1])
root.mkdir()
for index in range(100000):
    (root / f"member_{index}").touch()
PY
    bsdtar -a -c -f "$dir/slow.zip" -C "$dir/slowpayload" . \
        || fail "operations: the slow extraction archive could not be built"

    launch "$dir"
    wait_listing 5

    # The submenu is exactly the table the backend probed, never a fixed list.
    [[ "$(ipc archiveFormats)" == *"tar.zst"* ]] || fail "operations: the probed formats are $(ipc archiveFormats)"
    [[ "$(ipc canConvert)" == "true" ]] || fail "operations: this box reports no converter"

    # Each row kind is offered exactly the operations that apply to it, which is the design's gating.
    seek_row_named "shot.png"
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Convert"* ]] || fail "operations: an image was offered no Convert"
    [[ "$(ipc contextMenuEntries)" != *"Extract"* ]] || fail "operations: an image was offered Extract"
    key -k Escape >/dev/null
    settle

    seek_row_named "bundle.tar.zst"
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Extract"* ]] || fail "operations: an archive was offered no Extract"
    [[ "$(ipc contextMenuEntries)" != *"Convert"* ]] || fail "operations: an archive was offered Convert"
    printf 'OPERATIONS archive-menu=%s\n' "$(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    echo "-- compress through the submenu --"
    seek_row_named "notes.txt"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Compress"
    key -k Return >/dev/null
    settle
    # The first submenu row is the first format the table offered.
    key -k Return >/dev/null
    for _ in $(seq 1 40); do [[ -s "$dir/notes.zip" ]] && break; sleep 0.25; done
    [[ -s "$dir/notes.zip" ]] || fail "operations: compress wrote no archive, bar says $(ipc lastMessage)"
    printf 'OPERATIONS compressed=%s\n' "$(ipc lastMessage)"
    shot operations-compressed

    echo "-- convert through the one popup --"
    seek_row_named "shot.png"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Convert"
    key -k Return >/dev/null
    settle
    [[ "$(ipc convertOpen)" == "true" ]] || fail "operations: the convert popup did not open"
    # The format that starts picked is never the one the file already is, and strip starts off.
    [[ "$(ipc convertFormat)" == "jpg" ]] || fail "operations: the popup opened on $(ipc convertFormat)"
    [[ "$(ipc convertStrip)" == "false" ]] \
        || fail "operations: remove-metadata started ticked, which is not least surprise"
    # Same fall-through as the settings panel: a click on the card's title must not close the popup.
    local cwx cwy ctx cty
    read -r cwx cwy _ _ < <(window_box) || fail "native window coordinates unavailable"
    read -r ctx cty <<< "$(ipc convertTitleCentre)"
    [[ -n "$cty" ]] || fail "operations: the convert popup has no title to click"
    omarchy-drive click "$((cwx + ctx))" "$((cwy + cty))" left >/dev/null
    settle
    [[ "$(ipc convertOpen)" == "true" ]] || fail "operations: a click on the popup's own title closed it"
    shot operations-convert
    key -k Return >/dev/null
    for _ in $(seq 1 40); do [[ -s "$dir/shot (converted).jpg" ]] && break; sleep 0.25; done
    [[ -s "$dir/shot (converted).jpg" ]] || fail "operations: convert wrote nothing, bar says $(ipc lastMessage)"
    [[ "$(magick identify -format '%m' "$dir/shot (converted).jpg")" == "JPEG" ]] \
        || fail "operations: the converted file is not a jpeg"
    # Never in place: the file it came from is untouched.
    [[ "$(magick identify -format '%m' "$dir/shot.png")" == "PNG" ]] \
        || fail "operations: the source was written over"
    printf 'OPERATIONS converted=%s\n' "$(ipc lastMessage)"

    # #165: Extract drives the copy card, and its Escape cancel must leave no destination.
    echo "-- extract in the copy card, cancelled with the transfer's own key --"
    local state deadline
    seek_row_named "slow.zip"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Extract"
    key -k Return >/dev/null
    deadline=$((SECONDS + 20))
    state=""
    while (( SECONDS < deadline )); do
        state=$(ipc statusActivityState) || fail "operations: extract-card observation failed"
        jq -e '.transferCard.visible and (.transferCard.byteLine == "") and (.transferCard.cancel.visible and .transferCard.cancel.enabled) and (.transferCard.headline == "Extracting") and (.transferCard.sourceLine == "slow.zip")' <<< "$state" >/dev/null && break
        sleep 0.05
    done
    jq -e '.transferCard.visible and (.transferCard.byteLine == "") and (.transferCard.cancel.visible and .transferCard.cancel.enabled) and (.transferCard.headline == "Extracting") and (.transferCard.sourceLine == "slow.zip")' <<< "$state" >/dev/null \
        || fail "operations: no live no-byte Extracting card before the deadline: $state"
    printf 'OPERATIONS extract-card=%s\n' "$state"
    shot operations-extracting
    key -k Escape >/dev/null
    deadline=$((SECONDS + 20))
    while (( SECONDS < deadline )); do
        state=$(ipc statusActivityState) || fail "operations: extract-cancel observation failed"
        jq -e '(.transferCard.visible | not) and (.activities | length) == 0' <<< "$state" >/dev/null && break
        sleep 0.05
    done
    jq -e '(.transferCard.visible | not) and (.activities | length) == 0' <<< "$state" >/dev/null \
        || fail "operations: a cancelled extraction never reached a terminal state: $state"
    [[ ! -e "$dir/slow" ]] || fail "operations: a cancelled extract published $dir/slow"
    printf 'OPERATIONS extract-cancelled=%s\n' "$state"

    echo "-- a completed extract writes the tree beside the archive --"
    seek_row_named "bundle.tar.zst"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Extract"
    key -k Return >/dev/null
    for _ in $(seq 1 80); do [[ -f "$dir/bundle/notes.txt" ]] && break; sleep 0.25; done
    [[ -f "$dir/bundle/notes.txt" ]] || fail "operations: extract wrote nothing, bar says $(ipc lastMessage)"
    printf 'OPERATIONS extracted=%s\n' "$(ipc lastMessage)"
    kill_flea
}

case_transferlive() {
    local menus_checks=0
    transferlive_root="$fixture_root/transferlive"
    transferlive_bus_pid=""
    transferlive_writer_pids=()
    transferlive_sample_one="" transferlive_sample_batch="" transferlive_sample_replace=""
    trap 'transferlive_teardown' EXIT
    sandbox_make "$transferlive_root"
    seed_ui_state "$transferlive_root/state" "$transferlive_ui_state"
    export XDG_DATA_HOME="$transferlive_root/data"
    mkdir "$XDG_DATA_HOME" || fail "transferlive: no private data home"
    transferlive_prepare
    transferlive_start_bus
    transferlive_one
    transferlive_batch
    transferlive_replace
    printf 'TRANSFERLIVE copy=%s/s random_write=%s/s payload=%s half=%s one=%s batch=%s replace=%s\n' \
        "$(transferlive_size "$transferlive_copy_rate")" "$(transferlive_size "$transferlive_random_rate")" \
        "$(transferlive_size "$transferlive_full_bytes")" "$(transferlive_size "$transferlive_half_bytes")" \
        "$transferlive_sample_one" "$transferlive_sample_batch" "$transferlive_sample_replace"
    transferlive_teardown
    trap - EXIT
    [[ ! -e "$transferlive_root" ]] || fail "transferlive: $transferlive_root survived the teardown"
}

# Two owned windows share one session journal, so one z in either undoes an operation made in the other.
case_xwundo() {
    local dir="$fixture_root/xwundo" dirB="$fixture_root/xwundo-b"
    sandbox_scratch "$dir"
    sandbox_scratch "$dirB"
    printf 'note\n' > "$dir/note.txt"
    printf 'boxed\n' > "$dirB/b.txt"
    local blog="$run_root/flea-b.log"
    : > "$blog"

    # launch_window empties the session journal; only the two backends share its dir.
    launch_window "$dir"
    wait_listing 1
    local pidA="$ipc_pid" addrA="" addrB="" pidB=""
    assert_window
    addrA=$(omarchy-drive windows --json | jq -r '[.windows[] | select(.title == "Flea")] | .[0].address // empty')
    [[ -n "$addrA" ]] || fail "xwundo: window A never appeared"
    [[ "$(hyprctl clients -j | jq -r --arg a "$addrA" '.[] | select(.address == $a) | .pid')" == "$pidA" ]] \
        || fail "xwundo: the window at $addrA does not belong to owned pid $pidA"
    local before
    before=$(flea_pids | tr '\n' ' ')
    flea_test_launch "$dirB" "$blog" --ui "$flea_ui" --binary "$flea_bin" \
        || fail "xwundo: the second owned window launch failed"
    local attempt pid
    for attempt in $(seq 1 150); do
        if [[ "$(omarchy-drive windows --json | jq '[.windows[] | select(.title == "Flea")] | length')" == "2" ]]; then
            addrB=$(omarchy-drive windows --json | jq -r --arg a "$addrA" '.windows[] | select(.title == "Flea" and .address != $a) | .address')
            [[ -n "$addrB" ]] && break
        fi
        sleep 0.1
    done
    [[ -n "$addrB" ]] || fail "xwundo: window B never appeared beside A"
    pidB=""
    for pid in $(flea_pids); do
        [[ " $before " == *" $pid "* ]] || pidB=$pid
    done
    [[ -n "$pidB" ]] || fail "xwundo: window B has no owned pid"
    flea_process_owned "$pidB" || fail "xwundo: window B pid $pidB is not owned by this run"
    [[ "$pidB" != "$pidA" ]] || fail "xwundo: A and B share pid $pidA"
    [[ "$(hyprctl clients -j | jq -r --arg a "$addrB" '.[] | select(.address == $a) | .pid')" == "$pidB" ]] \
        || fail "xwundo: window B at $addrB does not belong to owned pid $pidB"
    omarchy-drive focus "$addrB" >/dev/null || fail "xwundo: window B never took focus"

    echo "-- rename in A, z in B restores the name --"
    omarchy-drive focus "$addrA" >/dev/null || fail "xwundo: window A never took focus back"
    omarchy-drive key --window "$addrA" r >/dev/null || fail "xwundo: the rename keypress failed"
    settle
    printf '%s' "note2" | omarchy-drive key --window "$addrA" - >/dev/null \
        || fail "xwundo: typing the new name failed"
    omarchy-drive key --window "$addrA" -k Return >/dev/null || fail "xwundo: the rename commit failed"
    xwundo_wait_file "$dir/note2.txt" "xwundo: the rename never landed"
    [[ ! -e "$dir/note.txt" ]] || fail "xwundo: the old name is still on disk"
    omarchy-drive focus "$addrB" >/dev/null || fail "xwundo: window B never took focus back"
    settle
    omarchy-drive key --window "$addrB" z >/dev/null || fail "xwundo: B's undo keypress failed"
    xwundo_wait_file "$dir/note.txt" "xwundo: B never undid A's rename"
    [[ ! -e "$dir/note2.txt" ]] || fail "xwundo: the renamed file survived B's undo"
    printf 'XWUNDO rename ok\n'

    echo "-- move by paste in B, z in A puts it back --"
    omarchy-drive focus "$addrB" >/dev/null || fail "xwundo: window B lost focus before the cut"
    omarchy-drive key --window "$addrB" x >/dev/null || fail "xwundo: the cut keypress failed"
    settle
    # The path bar opens with the current path selected, so typing replaces it whole.
    omarchy-drive key --window "$addrB" : >/dev/null || fail "xwundo: the path bar never opened"
    settle
    printf '%s' "$dir" | omarchy-drive key --window "$addrB" - >/dev/null \
        || fail "xwundo: typing the destination failed"
    omarchy-drive key --window "$addrB" -k Return >/dev/null || fail "xwundo: the path bar commit failed"
    local -a ipcB=(timeout --kill-after="$ipc_call_kill_after" "$ipc_call_timeout" "$FLEA_IPC_HELPER" --pid "$pidB" call flea)
    # B gets path_polls x poll_seconds (15 s) to reach $dir before the paste.
    local path_polls=150 poll_seconds=0.1
    for attempt in $(seq 1 "$path_polls"); do
        [[ "$("${ipcB[@]}" path 2>/dev/null)" == "$dir" ]] && break
        sleep "$poll_seconds"
    done
    [[ "$("${ipcB[@]}" path)" == "$dir" ]] || fail "xwundo: B never reached $dir before the paste"
    omarchy-drive key --window "$addrB" p >/dev/null || fail "xwundo: the paste keypress failed"
    xwundo_wait_file "$dir/b.txt" "xwundo: the paste never landed"
    [[ ! -e "$dirB/b.txt" ]] || fail "xwundo: the source survived its own move"
    omarchy-drive focus "$addrA" >/dev/null || fail "xwundo: window A never took focus back"
    settle
    omarchy-drive key --window "$addrA" z >/dev/null || fail "xwundo: A's undo keypress failed"
    xwundo_wait_file "$dirB/b.txt" "xwundo: A never undid B's move"
    [[ ! -e "$dir/b.txt" ]] || fail "xwundo: the moved file survived A's undo"
    [[ "$(ls -A "$dir")" == "note.txt" ]] || fail "xwundo: A's directory holds more than the restored note"
    printf 'XWUNDO move ok\n'

    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    cat "$blog" >> "$run_log" 2>/dev/null || true
    kill_flea
}
