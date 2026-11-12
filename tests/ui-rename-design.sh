#!/usr/bin/env bash
# Sourced by ui.sh; retained rename state is exercised through native keys and row menus.
# shellcheck disable=SC2154 # ui.sh supplies the owned fixture and native driver settings.

# A still listing reads the same twice in a row: up to 40 reads, with a 50 ms sleep after each.
rename_design_still_polls=40
rename_design_still_s=0.05
# The GUI re-reads a changed folder 400 ms (WATCH_MS) after the change, so a held re-read is checked after twice that.
rename_design_watch_s=0.8

rename_design_draft() {
    local draft="$1"
    key -M ctrl -k a -m ctrl -k BackSpace >/dev/null || fail 'rename: clear draft failed'
    [[ -z "$draft" ]] || key "$draft" >/dev/null || fail 'rename: draft input failed'
}

rename_design_refusal() {
    local draft="$1" reason="$2" label="$3" quoted
    quoted=$(jq -cn --arg draft "$draft" '$draft') || fail 'rename: draft encoding failed'
    key -k Return >/dev/null || fail 'rename: submit failed'
    menus_expect renameState ".index >= 0 and .focused and (.pending | not) and .text == $quoted and (.error | contains(\"$reason\")) and .fieldHeight > 0 and .errorHeight > 0" "$label"
    printf 'RENAME_STATE %s %s\n' "$label" "$(ipc renameState)"
}

# A row's centre is read only once the listing holds still: a click aimed during a relist or a scroll lands on a neighbour.
rename_design_still() {
    local before="" now polls
    for polls in $(seq 1 "$rename_design_still_polls"); do
        now="$(ipc listInFlight) $(ipc viewContentY)"
        [[ "$now" == "$before" && "$now" == false\ * ]] && return 0
        before="$now"
        sleep "$rename_design_still_s"
    done
    fail "rename: the listing never held still before a click, last [$now]"
}

# A directory's names are unique, so the walk starts at the cursor and goes outward: a deep row costs a few reads, not one per row above it.
rename_design_index_of() {
    local want="$1" total cursor d i row
    total=$(ipc total)
    cursor=$(ipc cursor)
    (( cursor >= 0 && cursor < total )) || cursor=0
    for (( d = 0; d < total; d++ )); do
        # At distance 0 both sides are the cursor row, so the lower side is skipped there.
        for i in $((cursor + d)) $(( d > 0 ? cursor - d : -1 )); do
            (( i >= 0 && i < total )) || continue
            row=$(ipc rowAt "$i")
            [[ "$row" == "$want|"* ]] && { printf '%s' "$i"; return; }
            # Grid and columns leave the list view's delegate unbuilt; read the row the shown view draws.
            [[ "$row" == loading && "$(ipc visibleRowName "$i")" == "$want" ]] && { printf '%s' "$i"; return; }
        done
    done
    fail "no row named $want in a listing of $total"
}

# The window-relative top of a row's rectangle, read before and after a commit to prove the view did not move.
rename_design_y_of() {
    local rect
    rect=$(ipc rowRect "$1")
    # Sample input: 0 584 1000 31 (rowRect x y width height).
    [[ "$rect" =~ ^-?[0-9]+\ (-?[0-9]+)\ -?[0-9]+\ -?[0-9]+$ ]] || fail "rename: row $1 has no on-screen rectangle"
    printf '%s' "${BASH_REMATCH[1]}"
}

# The commit's kept row is the cursor row once the listing holds still, so its top edge must read what it read before.
rename_design_stays() {
    local label="$1" before="$2"
    rename_design_still
    menus_equal "$label" "$before" "$(rename_design_y_of "$(ipc cursor)")"
}

rename_design_open() {
    local input="$1" name="$2" index polls
    rename_design_still
    index=$(rename_design_index_of "$name") || fail "rename: no row named $name to open"
    click_row "$index" left
    # The click must select the row it aimed at, or the key below would rename the row the cursor was on.
    for polls in $(seq 1 "$rename_design_still_polls"); do
        [[ "$(ipc selectedIndices)" == "$index" ]] && break
        sleep "$rename_design_still_s"
    done
    [[ "$(ipc selectedIndices)" == "$index" ]] || fail "rename: the click on $name (row $index) selected [$(ipc selectedIndices)]"
    # The selected row must still be the named file, so a relist that renumbered the rows under the click goes red.
    [[ "$(rename_design_index_of "$name")" == "$index" ]] || fail "rename: $name left row $index while it was clicked"
    if [[ "$input" == menu ]]; then
        click_row "$index" right
        menus_expect menuState '.opened and .snapshotReady' 'native row menu captures rename source'
        menus_choose rename pointer
    else
        key -k "$input" >/dev/null || fail 'rename: key entry failed'
    fi
    menus_expect renameState '.index >= 0 and .focused and (.pending | not) and .error == ""' "$input opens the inline editor"
}

rename_design_backend_loss() {
    local mode="$1" pid
    local -a pids
    rename_design_open r a-original.md
    rename_design_draft a-unconfirmed.md
    mapfile -t pids < <(backend_pids)
    [[ "${#pids[@]}" == 1 ]] || fail 'rename: backend failure requires one owned backend'
    pid="${pids[0]}"
    rename_stopped="$pid"
    convert_pause_backend "$pid"
    key -k Return >/dev/null
    menus_expect renameState '.index >= 0 and .pending and .text == "a-unconfirmed.md"' "$mode rename waits for its paused backend"
    key -k Return -k Escape >/dev/null
    menus_expect renameState '.index >= 0 and .pending' "$mode repeated submit and Escape cannot abandon an unresolved write"
    permissions_backend_owned "$pid" || fail 'rename: backend identity changed before termination'
    kill -KILL "$pid" || fail 'rename: owned backend termination failed'
    if [[ "$test_frontend" == gpui ]]; then
        printf '%s\n' 'flea-gui: The backend closed its reply stream' >> "$expected_warnings"
    fi
    rename_stopped=""
    menus_expect renameState '.index == -1 and (.pending | not) and .listingState == "error"' "$mode backend loss releases the editor and its pending request"
    menus_expect statusError '. == true' "$mode backend loss remains an acknowledged error"
    local stopped_message
    stopped_message=$(by_frontend 'rename outcome unknown' 'Flea stopped responding, so the rename may not have finished. Check the name.')
    [[ "$(ipc statusPrimary)" == *"$stopped_message"* ]] || fail 'rename: backend loss misreported the operation outcome'
    menus_shot "rename-$mode-backend-lost"
    menus_acknowledge
    menus_guard "$menu_dir/a-original.md"
    menus_guard "$menu_dir/a-unconfirmed.md"
    menus_equal "$mode paused request preserved original bytes" 'original bytes' "$(cat "$menu_dir/a-original.md")"
    [[ ! -e "$menu_dir/a-unconfirmed.md" ]] || fail 'rename: stopped backend unexpectedly processed the request'
    kill_flea
    launch "$menu_dir"
    wait_listing 1202
    permissions_viewport 1000 700
    switch_view "$mode"
    rename_design_open F2 a-original.md
    key -k Escape >/dev/null
    menus_expect renameState '.index == -1 and (.pending | not) and .listingState == "ready"' "$mode restart restores native rename and Escape"
    menus_shot "rename-$mode-backend-recovered"
}

case_renamelife() { case_renamedesign life; }

case_renamedesign() (
    local menu_box menu_dir menus_checks=0 mode path draft before wx wy ww wh cx cy index
    local proof="${1:-design}" permissions_listing rename_stopped=""
    sandbox_require "$fixture_root"
    menu_box=$(mktemp -d "$fixture_root/rename-design.XXXXXXXX") || fail 'rename: owned fixture creation failed'
    printf 'native rename fixture\n' > "$menu_box/.flea-test-sandbox"
    menu_dir="$menu_box/listing"
    permissions_listing="$menu_dir"
    for path in "$menu_dir" "$menu_box/config" "$menu_box/cache" "$menu_box/data"; do
        menus_guard "$path"
        mkdir "$path" || fail 'rename: fixture directory creation failed'
    done
    export XDG_CONFIG_HOME="$menu_box/config" XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    seed_ui_state "$menu_box/state" '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"off"},"menu":{"hidden":[]}}'
    menus_guard "$menu_dir/a-original.md"
    printf 'original bytes\n' > "$menu_dir/a-original.md"
    menus_guard "$menu_dir/b-existing.md"
    printf 'existing bytes\n' > "$menu_dir/b-existing.md"
    for ((index = 0; index < 1200; index++)); do
        printf -v path '%s/f%04d.txt' "$menu_dir" "$index"
        menus_guard "$path"
        printf 'row %s\n' "$index" > "$path"
    done
    # Always restore the owned directory's write bit before harness teardown.
    trap 'permissions_resume_stopped "$rename_stopped"; menus_guard "$menu_dir"; chmod 700 "$menu_dir"; kill_flea' EXIT
    launch "$menu_dir"
    wait_listing 1202
    permissions_viewport 1000 700

    # Columns stays out of this loop: ui:cap_columnsrename covers its inline editor and active-column focus.
    for mode in list grid; do
        switch_view "$mode"
        if [[ "$proof" == life ]]; then
            rename_design_backend_loss "$mode"
            continue
        fi
        if [[ "$proof" == far ]]; then
            rename_design_far "$mode"
            continue
        fi
        key -k Home >/dev/null
        menus_expect renameState '(.loading | not) and .currentRowHeight > 0' "$mode draws actual row before editing"
        before=$(ipc renameState | jq -er .currentRowHeight)
        rename_design_open r a-original.md
        menus_expect renameState '.text == "a-original.md" and .selectedText == "a-original"' "$mode selects basename and preserves extension"
        menus_point "$(ipc renameState | jq -er .centre)"
        menus_expect renameState '.index >= 0 and .focused and .text == "a-original.md" and (.pending | not)' "$mode clicking the text field never invokes the row action"
        for draft in '' . .. bad/name; do
            rename_design_draft "$draft"
            rename_design_refusal "$draft" 'A name cannot' "$mode invalid basename retains draft and focus"
            key -k Return >/dev/null
            menus_expect renameState '.index >= 0 and .focused and (.pending | not)' "$mode repeated refusal stays editable"
            [[ -f "$menu_dir/a-original.md" ]] || fail 'rename: invalid draft changed original path'
        done
        rename_design_draft b-existing.md
        rename_design_refusal b-existing.md 'already exists' "$mode collision retains editor"
        menus_equal "$mode collision preserves original bytes" 'original bytes' "$(cat "$menu_dir/a-original.md")"
        menus_equal "$mode collision preserves sibling bytes" 'existing bytes' "$(cat "$menu_dir/b-existing.md")"
        if [[ "$mode" != grid ]]; then
            menus_expect renameState '.rowHeight > .normalRowHeight' "$mode error caption expands only edited row"
        fi
        menus_shot "rename-$mode-collision"
        key -k Escape >/dev/null
        menus_expect renameState '.index == -1 and .error == "" and (.pending | not)' "$mode Escape restores ordinary row"
        menus_expect renameState ".currentRowHeight == $before" "$mode dismissal restores released row height"
        click_row "$(rename_design_index_of b-existing.md)" left
        menus_equal "$mode pointer selection works after refusal dismissal" "$(rename_design_index_of b-existing.md)" "$(ipc cursor)"

        rename_design_open F2 a-original.md
        menus_guard "$menu_dir"
        chmod 500 "$menu_dir" || fail 'rename: owned permission failure setup failed'
        rename_design_draft a-renamed.md
        rename_design_refusal a-renamed.md 'Permission denied' "$mode backend failure retains editable draft"
        menus_shot "rename-$mode-permission"
        menus_guard "$menu_dir"
        chmod 700 "$menu_dir" || fail 'rename: owned permission recovery failed'
        menus_guard "$menu_dir/a-original.md"
        menus_guard "$menu_dir/a-renamed.md"
        key -k Return >/dev/null
        wait_marker "$menu_dir/a-renamed.md" 'rename: corrected permission did not permit retry'
        menus_expect renameState '.index == -1 and (.pending | not) and (.loading | not) and .cursorName == "a-renamed.md" and .currentRowHeight > 0' "$mode successful retry keeps renamed identity selected and visible"
        menus_equal "$mode retry preserves bytes" 'original bytes' "$(cat "$menu_dir/a-renamed.md")"
        rename_design_open menu a-renamed.md
        rename_design_draft a-original.md
        menus_guard "$menu_dir/a-renamed.md"
        menus_guard "$menu_dir/a-original.md"
        key -k Return >/dev/null
        menus_expect renameState '.index == -1 and (.loading | not) and .cursorName == "a-original.md"' "$mode pointer menu restores original name through same editor"

        rename_design_open r a-original.md
        local replacement_requests
        replacement_requests=$(ipc listRequests)
        [[ "$replacement_requests" =~ ^[0-9]+$ ]] || fail "rename: listRequests read [$replacement_requests] before the replacement"
        menus_guard "$menu_dir/a-original.md"
        menus_guard "$menu_box/retained-$mode.md"
        mv -- "$menu_dir/a-original.md" "$menu_box/retained-$mode.md" || fail 'rename: identity replacement setup failed'
        menus_guard "$menu_dir/a-original.md"
        printf 'replacement bytes\n' > "$menu_dir/a-original.md"
        local replacement_identity
        replacement_identity=$(stat -c '%d:%i' "$menu_dir/a-original.md")
        rename_design_draft a-replaced.md
        rename_design_refusal a-replaced.md 'Item changed; reopen Rename.' "$mode keyboard rename refuses replaced source identity"
        [[ ! -e "$menu_dir/a-replaced.md" ]] || fail 'rename: changed source was renamed'
        menus_equal "$mode stale refusal preserves original" 'original bytes' "$(cat "$menu_box/retained-$mode.md")"
        menus_equal "$mode stale refusal preserves replacement" 'replacement bytes' "$(cat "$menu_dir/a-original.md")"
        menus_equal "$mode stale refusal preserves replacement inode" "$replacement_identity" "$(stat -c '%d:%i' "$menu_dir/a-original.md")"
        menus_shot "rename-$mode-stale"
        sleep "$rename_design_watch_s"
        menus_equal "$mode replacement is not re-listed behind the open editor" "$replacement_requests" "$(ipc listRequests)"
        key -k Escape >/dev/null
        menus_expect renameState '.index == -1' "$mode stale editor dismisses"
        menus_relisted "$replacement_requests" "$mode stale editor releases the replacement re-read"
        rename_design_open F2 a-original.md
        rename_design_draft a-replaced.md
        menus_guard "$menu_box/roundtrip-$mode.md"
        mv -- "$menu_dir/a-original.md" "$menu_box/roundtrip-$mode.md" || fail 'rename: same-inode move out failed'
        mv -- "$menu_box/roundtrip-$mode.md" "$menu_dir/a-original.md" || fail 'rename: same-inode move back failed'
        menus_guard "$menu_dir/a-original.md"
        menus_guard "$menu_dir/a-replaced.md"
        key -k Return >/dev/null
        menus_expect renameState '.index == -1 and (.loading | not) and .cursorName == "a-replaced.md"' "$mode fresh entry captures replacement identity"
        menus_equal "$mode fresh rename preserves replacement bytes" 'replacement bytes' "$(cat "$menu_dir/a-replaced.md")"
        menus_equal "$mode fresh rename preserves the moved-back inode" "$replacement_identity" "$(stat -c '%d:%i' "$menu_dir/a-replaced.md")"
        menus_guard "$menu_dir/a-replaced.md"
        menus_guard "$menu_box/accepted-$mode.md"
        mv -- "$menu_dir/a-replaced.md" "$menu_box/accepted-$mode.md" || fail 'rename: completed fixture retirement failed'
        menus_guard "$menu_box/retained-$mode.md"
        menus_guard "$menu_dir/a-original.md"
        mv -- "$menu_box/retained-$mode.md" "$menu_dir/a-original.md" || fail 'rename: original fixture restore failed'
        # The successful rename left a selection, which defers the watch. Same-path Enter is a no-op.
        key -k Escape >/dev/null || fail 'rename: restored fixture selection release failed'
        menus_expect selectionCount '. == 0' "$mode fixture restore releases the watch"
        menus_expect renameState '.index == -1 and (.loading | not) and .cursorName == "a-original.md"' "$mode directory watch lands the restored name"
        wait_listing 1202
        menus_equal "$mode restored fixture preserves original bytes" 'original bytes' "$(cat "$menu_dir/a-original.md")"
        menus_equal "$mode retired fixture preserves replacement bytes" 'replacement bytes' "$(cat "$menu_box/accepted-$mode.md")"

        rename_design_open r a-original.md
        rename_design_draft b-existing.md
        rename_design_refusal b-existing.md 'already exists' "$mode scroll starts with expanded error row"
        read -r cx cy <<< "$(ipc listingBackgroundCentre)"
        read -r wx wy ww wh < <(window_box) || fail 'rename: native window geometry unavailable'
        [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail 'rename: listing centre unavailable'
        (( cx < ww && cy < wh )) || fail 'rename: listing centre escaped owned window'
        assert_focus
        omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null
        omarchy-drive scroll down "$fling_clicks" >/dev/null
        menus_expect renameState '.index == -1 and .error == ""' "$mode scrolling released editor cannot leave keyboard trapped"
        key -k Home >/dev/null
        menus_expect renameState '.cursor == 0 and .currentRowHeight > 0' "$mode Home restores held first row after scroll"
        menus_expect renameState ".currentRowHeight == $before" "$mode row geometry recovers after scroll cancellation"
        key -k End >/dev/null
        menus_expect renameState '.cursorName == "f1199.txt" and .currentRowHeight > 0' "$mode reaches the last visible row"
        rename_design_open F2 f1199.txt
        rename_design_draft b-existing.md
        rename_design_refusal b-existing.md 'already exists' "$mode bottom-row refusal retains the draft"
        menus_expect renameState '.editorTop >= 0 and .errorBottom > .editorBottom and .errorBottom <= .viewportHeight' "$mode expanded bottom row and refusal remain entirely within its viewport"
        menus_shot "rename-$mode-bottom-error"
        key -k Escape -k Home >/dev/null
        menus_expect renameState '.index == -1 and .cursor == 0' "$mode bottom editor dismissal restores navigation"
        click_row 0 left
        click_row 1 left --mods ctrl
        menus_equal "$mode Ctrl-click still marks two rows after dismissal" 2 "$(ipc selectionCount)"
        key -k Escape >/dev/null
        menus_equal "$mode selection Escape remains live" 0 "$(ipc selectionCount)"
        menus_shot "rename-$mode-recovered"

        # A click-away commits and keeps the clicked neighbour, at the top and deep.
        key -k Home >/dev/null
        menus_expect renameState '.cursor == 0' "$mode click-away setup at top"
        rename_design_open r a-original.md
        rename_design_draft a-clickaway-top.md
        click_at=$(rename_design_index_of b-existing.md)
        click_y=$(rename_design_y_of "$click_at")
        click_row "$click_at" left
        menus_expect renameState '.index == -1 and (.pending | not) and (.loading | not) and .cursorName == "b-existing.md"' "$mode click-away keeps clicked neighbour at top"
        rename_design_stays "$mode click-away at top leaves the clicked row where it was" "$click_y"
        menus_equal "$mode click-away cursor at top" "$(rename_design_index_of b-existing.md)" "$(ipc cursor)"
        menus_guard "$menu_dir/a-clickaway-top.md"
        [[ -f "$menu_dir/a-clickaway-top.md" ]] || fail 'rename: click-away did not write top draft'
        rename_design_open r a-clickaway-top.md
        rename_design_draft a-original.md
        click_y=$(rename_design_y_of "$(ipc cursor)")
        key -k Return >/dev/null
        menus_expect renameState '.index == -1 and (.pending | not) and (.loading | not) and .cursorName == "a-original.md"' "$mode Enter keeps renamed row at top"
        rename_design_stays "$mode Enter at top leaves the renamed row where it was" "$click_y"
        key -k Escape >/dev/null
        key -k End >/dev/null
        menus_expect renameState '.cursorName == "f1199.txt"' "$mode click-away setup at bottom"
        rename_design_open F2 f1199.txt
        rename_design_draft f1199-clickaway.txt
        click_at=$(rename_design_index_of f1198.txt)
        click_y=$(rename_design_y_of "$click_at")
        click_row "$click_at" left
        menus_expect renameState '.index == -1 and (.pending | not) and (.loading | not) and .cursorName == "f1198.txt"' "$mode click-away keeps clicked neighbour at bottom"
        rename_design_stays "$mode deep click-away leaves the clicked row where it was" "$click_y"
        menus_equal "$mode deep click-away cursor" "$(rename_design_index_of f1198.txt)" "$(ipc cursor)"
        rename_design_open F2 f1199-clickaway.txt
        rename_design_draft f1199.txt
        click_y=$(rename_design_y_of "$(ipc cursor)")
        key -k Return >/dev/null
        menus_expect renameState '.index == -1 and (.pending | not) and (.loading | not) and .cursorName == "f1199.txt"' "$mode deep Enter keeps renamed row"
        rename_design_stays "$mode deep Enter leaves the renamed row where it was" "$click_y"
        [[ "$(ipc cursor)" != "0" ]] || fail 'rename: deep Enter dropped the cursor to row 0'
        key -k Escape >/dev/null
        # One more click while the write is pending: the last clicked row wins.
        key -k Home >/dev/null
        menus_expect renameState '.cursor == 0' "$mode pending-click setup"
        mapfile -t pids < <(backend_pids)
        [[ "${#pids[@]}" == 1 ]] || fail 'rename: pending click requires one owned backend'
        click_pid="${pids[0]}"
        # The r key opens the editor on the backend's menu snapshot reply, so open it before the pause.
        rename_design_open r a-original.md
        rename_design_draft a-pending-click.md
        rename_stopped="$click_pid"
        convert_pause_backend "$click_pid"
        key -k Return >/dev/null
        menus_expect renameState '.index >= 0 and .pending' "$mode rename waits while backend paused"
        click_row "$(rename_design_index_of b-existing.md)" left
        click_row "$(rename_design_index_of f0000.txt)" left
        click_y=$(rename_design_y_of "$(ipc cursor)")
        permissions_resume_stopped "$click_pid" || fail 'rename: owned backend did not resume'
        rename_stopped=""
        menus_expect renameState '.index == -1 and (.pending | not) and (.loading | not) and .cursorName == "f0000.txt"' "$mode second click while pending wins"
        rename_design_stays "$mode pending click-away leaves the clicked row where it was" "$click_y"
        menus_equal "$mode pending-click cursor" "$(rename_design_index_of f0000.txt)" "$(ipc cursor)"
        rename_design_open r a-pending-click.md
        rename_design_draft a-original.md
        click_y=$(rename_design_y_of "$(ipc cursor)")
        key -k Return >/dev/null
        menus_expect renameState '.index == -1 and (.pending | not) and (.loading | not) and .cursorName == "a-original.md"' "$mode pending fixture restored"
        rename_design_stays "$mode Enter after the pending click leaves the renamed row where it was" "$click_y"
        key -k Escape >/dev/null
    done
    printf 'RENAME_%s_NATIVE_CHECKS=%s\n' "${proof^^}" "$menus_checks"
)
