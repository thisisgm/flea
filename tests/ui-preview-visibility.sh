# Sourced by ui.sh; all fixtures and writes stay in its marked sandbox.
preview_selection_expect() {
    local expression="$1" observed deadline=$((SECONDS + 10))
    while (( SECONDS < deadline )); do
        observed=$(ipc previewSelectionState)
        jq -e "$expression" <<< "$observed" >/dev/null && return
        sleep 0.1
    done
    fail "previewvisibility: expected $expression, observed $observed"
}

case_previewvisibility() {
    local dir="$fixture_root/previewvisibility" preset mode
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing" "$dir/state/flea" "$dir/config"
    printf 'first preview\n' > "$dir/listing/a.txt"
    printf 'second preview\n' > "$dir/listing/b.txt"
    export XDG_STATE_HOME="$dir/state" XDG_CONFIG_HOME="$dir/config"
    for preset in default vim mac windows; do
        jq -n --arg preset "$preset" '{keys:$preset,view:"list",preview:{column:true,loadOn:"manual",thumbnails:"off"}}' > "$dir/state/flea/ui.json"
        launch "$dir/listing"
        wait_listing 2
        for mode in list grid; do
            click_chrome "$mode"
            settle
            click_row 0 left
            preview_selection_expect '(.inlineVisible | not) and .index == -1 and .width == .available'
            key -M ctrl -k Space -m ctrl >/dev/null
            preview_selection_expect '(.inlineVisible | not) and .index == -1'
            key -k Space >/dev/null
            settle
            [[ "$(ipc previewOpen)" == true ]] || fail "previewvisibility: $preset $mode Space did not open Quick Look"
            key -k Escape >/dev/null
            settle
            [[ "$(ipc previewOpen)" == false ]] || fail "previewvisibility: $preset $mode Escape did not close Quick Look"
            shot "preview-$preset-$mode-no-inline"
        done
        click_chrome columns
        settle
        click_row 0 left
        preview_selection_expect '.inlineVisible and .index == -1 and .path == "" and .pending == 0'
        key -M ctrl -k Space -m ctrl >/dev/null
        preview_selection_expect '.index == 0 and (.path | endswith("/a.txt"))'
        key -k Down >/dev/null
        preview_selection_expect '.inlineVisible and .index == -1 and .path == "" and .pending == 0'
        key -M ctrl -k Space -m ctrl >/dev/null
        preview_selection_expect '.index == 1 and (.path | endswith("/b.txt"))'
        click_row 0 left
        preview_selection_expect '.index == -1 and .path == "" and .pending == 0'
        key -M ctrl -k Space -m ctrl >/dev/null
        preview_selection_expect '.index == 0'
        click_row 1 left --mods ctrl
        preview_selection_expect '.index == -1 and .path == "" and .pending == 0'
        key -M alt -k p -m alt >/dev/null
        preview_selection_expect '(.inlineVisible | not) and .index == -1'
        key -M ctrl -k Space -m ctrl >/dev/null
        preview_selection_expect '(.inlineVisible | not) and .index == -1 and .pending == 0'
        key -M alt -k p -m alt >/dev/null
        preview_selection_expect '.inlineVisible and .index == -1 and .path == ""'
        click_row 0 left
        settings_open_key
        settle
        settings_section preview
        settings_focus_row preview.loadOn
        key h >/dev/null
        settle
        settings_wait_value '.preview.loadOn == "automatic"'
        key -k Escape >/dev/null
        preview_selection_expect '.index == 0 and (.path | endswith("/a.txt"))'
        key -k Down >/dev/null
        preview_selection_expect '.index == 1 and (.path | endswith("/b.txt"))'
        click_row 0 left
        preview_selection_expect '.index == 0 and (.path | endswith("/a.txt"))'
        key -M ctrl -k Tab -m ctrl >/dev/null
        preview_selection_expect '.focused'
        key -M ctrl -k Tab -m ctrl >/dev/null
        preview_selection_expect '(.focused | not)'
        shot "preview-$preset-columns-automatic"
        printf 'PREVIEW_VISIBILITY preset=%s no_list_grid_column=ok quicklook=ok manual=ok selection_clear=ok hidden=ok automatic=ok focus=ok\n' "$preset"
        kill_flea
    done
}
