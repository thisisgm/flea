# Board 0402-settings-cursor: the optional row between list-end wrapping and Escape.
CASES+=("settings-grid-wrap-off 1100x700 surface:SettingsPanel"
        "settings-grid-wrap-on 1100x700 surface:SettingsPanel"
        "grid-wrap-row-ends 800x600 view:grid"
        "grid-wrap-shift-range 800x600 view:grid key:extendLeft key:extendRight")

grid_wrap_settings() {
    local on=$1 model target cursor attempt poll pane_limit=80 move_limit=20 poll_seconds=0.1
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    export LAUNCH_STATE=",\"gridWrapRows\":$on,\"display\":{\"textSize\":{\"mode\":14}}"
    launch grid "$W" || return 1
    key , || return 1
    wait_ipc settingsOpen true || return 1
    wait_ipc settingsSection view || return 1
    [ "$(ipc settingsSide)" = pane ] || key -k Tab
    wait_ipc settingsSide pane || return 1
    model=$(ipc settingsModel) || { state_failure "grid wrap: settingsModel read failed"; return 1; }
    # Sample input: [{"id":"wrapAtEnds","label":"Wrap at list ends","caption":"up at the top","on":false}]
    target=$(jq -er 'map(.id) | index("gridWrapRows")' <<< "$model") ||
        { state_failure "grid wrap: settingsModel has no gridWrapRows row"; return 1; }
    for attempt in $(seq 1 "$pane_limit"); do
        cursor=$(ipc settingsCursor)
        [ "$cursor" = "$target" ] && break
        if [ "$cursor" -gt "$target" ]; then key -k Up; else key -k Down; fi
        # A key lands after it is sent, so the next read waits for the cursor to move.
        for poll in $(seq 1 "$move_limit"); do
            [ "$(ipc settingsCursor)" != "$cursor" ] && break
            sleep "$poll_seconds"
        done
    done
    wait_ipc settingsCursor "$target" || return 1
    model=$(ipc settingsModel) || { state_failure "grid wrap: settingsModel read failed"; return 1; }
    # Sample input: [{"id":"wrapAtEnds","label":"Wrap at list ends","caption":"up at the top","on":false}]
    jq -e --argjson on "$on" --argjson at "$target" \
        '.[$at].label == "Wrap at row ends" and .[$at].caption == "left and right in Grid" and .[$at].on == $on and .[$at - 1].id == "wrapAtEnds" and .[$at + 1].id == "escapeUp"' \
        <<< "$model" >/dev/null || { state_failure "Grid row-wrap setting differs from its board"; return 1; }
    snap
}

case_settings_grid_wrap_off() ( grid_wrap_settings false )
case_settings_grid_wrap_on() ( grid_wrap_settings true )

grid_wrap_crossing() {
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local extend=$1 root=$C/grid-wrap index name columns
    mkdir -p "$root" || return 1
    : > "$root/.flea-test-sandbox" || return 1
    for index in {1..12}; do
        printf -v name 'file-%02d.txt' "$index"
        printf 'grid wrap %s\n' "$index" > "$root/$name" || return 1
    done
    export LAUNCH_STATE=',"gridWrapRows":true,"display":{"textSize":{"mode":14}},"preview":{"thumbnails":"off"}'
    launch grid "$root" || return 1
    size_window 800 600 || return 1
    columns=$(ipc gridColumns) || { state_failure "grid wrap: gridColumns read failed"; return 1; }
    [[ "$columns" =~ ^[0-9]+$ ]] && (( columns > 1 && columns < 12 )) ||
        { state_failure "grid wrap: $columns columns cannot show a row end in 12 files"; return 1; }
    for ((index=1; index<columns; index++)); do key -k Right || return 1; done
    wait_ipc cursor "$((columns - 1))" || return 1
    if [ "$extend" = true ]; then
        key -M shift -k Right -m shift || return 1
    else
        key -k Right || return 1
    fi
    wait_ipc cursor "$columns" || return 1
    if [ "$extend" = true ]; then
        wait_ipc selectedIndices "$((columns - 1)),$columns" || return 1
        key -M shift -k Left -m shift || return 1
        wait_ipc cursor "$((columns - 1))" || return 1
        key -M shift -k Right -m shift || return 1
        wait_ipc cursor "$columns" || return 1
        wait_ipc selectedIndices "$((columns - 1)),$columns" || return 1
    fi
    snap
}

case_grid_wrap_row_ends() ( grid_wrap_crossing false )
case_grid_wrap_shift_range() ( grid_wrap_crossing true )
