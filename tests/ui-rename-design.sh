#!/usr/bin/env bash
# Sourced by ui.sh; retained rename state is exercised through native keys and row menus.
# shellcheck disable=SC2154 # ui.sh supplies the owned fixture and native driver settings.

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

rename_design_open() {
    local input="$1" name="$2"
    click_row "$(row_index_of "$name")" left
    if [[ "$input" == menu ]]; then
        click_row "$(row_index_of "$name")" right
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
    rename_stopped=""
    menus_expect renameState '.index == -1 and (.pending | not) and .listingState == "error"' "$mode backend loss releases the editor and its pending request"
    menus_expect statusError '. == true' "$mode backend loss remains an acknowledged error"
    [[ "$(ipc statusPrimary)" == *'rename outcome unknown'* ]] || fail 'rename: backend loss misreported the operation outcome'
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

    # The columns view has no inline editor, see ui/ColumnPane.qml's corner comment.
    for mode in list grid; do
        switch_view "$mode"
        if [[ "$proof" == life ]]; then
            rename_design_backend_loss "$mode"
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
        click_row "$(row_index_of b-existing.md)" left
        menus_equal "$mode pointer selection works after refusal dismissal" "$(row_index_of b-existing.md)" "$(ipc cursor)"

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
        menus_guard "$menu_dir/a-original.md"
        menus_guard "$menu_box/retained-$mode.md"
        mv -- "$menu_dir/a-original.md" "$menu_box/retained-$mode.md" || fail 'rename: identity replacement setup failed'
        menus_guard "$menu_dir/a-original.md"
        printf 'replacement bytes\n' > "$menu_dir/a-original.md"
        rename_design_draft a-replaced.md
        rename_design_refusal a-replaced.md 'Selected item changed' "$mode keyboard rename refuses replaced source identity"
        [[ ! -e "$menu_dir/a-replaced.md" ]] || fail 'rename: changed source was renamed'
        menus_equal "$mode stale refusal preserves original" 'original bytes' "$(cat "$menu_box/retained-$mode.md")"
        menus_equal "$mode stale refusal preserves replacement" 'replacement bytes' "$(cat "$menu_dir/a-original.md")"
        menus_shot "rename-$mode-stale"
        key -k Escape >/dev/null
        menus_expect renameState '.index == -1' "$mode stale editor dismisses"
        rename_design_open F2 a-original.md
        rename_design_draft a-replaced.md
        menus_guard "$menu_dir/a-original.md"
        menus_guard "$menu_dir/a-replaced.md"
        key -k Return >/dev/null
        menus_expect renameState '.index == -1 and (.loading | not) and .cursorName == "a-replaced.md"' "$mode fresh entry captures replacement identity"
        menus_equal "$mode fresh rename preserves replacement bytes" 'replacement bytes' "$(cat "$menu_dir/a-replaced.md")"
        menus_guard "$menu_dir/a-replaced.md"
        menus_guard "$menu_box/accepted-$mode.md"
        mv -- "$menu_dir/a-replaced.md" "$menu_box/accepted-$mode.md" || fail 'rename: completed fixture retirement failed'
        menus_guard "$menu_box/retained-$mode.md"
        menus_guard "$menu_dir/a-original.md"
        mv -- "$menu_box/retained-$mode.md" "$menu_dir/a-original.md" || fail 'rename: original fixture restore failed'
        menus_visit "$menu_dir" 1202

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
        menus_expect renameState '.editorTop >= 0 and .editorBottom <= .viewportHeight' "$mode expanded bottom editor remains entirely within its viewport"
        menus_shot "rename-$mode-bottom-error"
        key -k Escape -k Home >/dev/null
        menus_expect renameState '.index == -1 and .cursor == 0' "$mode bottom editor dismissal restores navigation"
        click_row 0 left
        click_row 1 left --mods ctrl
        menus_equal "$mode Ctrl-click still marks two rows after dismissal" 2 "$(ipc selectionCount)"
        key -k Escape >/dev/null
        menus_equal "$mode selection Escape remains live" 0 "$(ipc selectionCount)"
        menus_shot "rename-$mode-recovered"
    done
    printf 'RENAME_%s_NATIVE_CHECKS=%s\n' "${proof^^}" "$menus_checks"
)
