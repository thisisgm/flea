#!/usr/bin/env bash
# Sourced by ui.sh; every state and listing path belongs to its marked fixture root.
rail_assert_details() {
    local enabled="$1" count="$2" state background surface foreground muted rest
    state=$(ipc railDetails) || fail "rail: native detail observations unavailable"
    read -r background surface foreground muted rest <<< "$(ipc palette)"
    python3 - "$enabled" "$count" "$muted" "$state" <<'PY'
import json, math, sys
enabled, count, muted, state = sys.argv[1:]
enabled, count, state = enabled == "true", int(count), json.loads(state)
headers = [row for row in state["headers"] if row["visible"]]
assert headers[0]["text"] == "PLACES", headers
assert all(row["topPadding"] >= math.ceil(row["fontSize"] * .15) for row in headers), headers
assert not any(row["text"] == "FAVORITES" for row in headers), "Empty Favorites header must hide"
rows = [row for row in state["rows"] if row["group"] in ("trash", "device")]
assert any(row["kind"] == "disk" for row in rows), "No native internal disk row for capacity proof"
assert any(row["kind"] == "trash" for row in rows), "No native Trash row"
edges = []
for row in rows:
    if row["kind"] == "trash":
        expected = str(count) if enabled and count else ""
    else:
        value = row["size"]
        assert isinstance(value, int) and value >= 0, row
        unit = 0
        units = ("B", "kB", "MB", "GB", "TB")
        while value >= 1000 and unit < len(units) - 1:
            value /= 1000
            unit += 1
        expected = ((f"{value:.1f}" if unit else str(value)) + " " + units[unit]) if enabled else ""
    assert row["detail"] == expected, (row, expected)
    if expected:
        detail, slot = row["detailRect"], row["indicatorRect"]
        assert row["detailColor"] == muted and row["tabular"], row
        assert slot["width"] == row["fontSize"], row
        assert detail["x"] + detail["width"] < slot["x"], row
        assert row["kind"] != "disk" or not row["indicatorVisible"], row
        edges.append(detail["x"] + detail["width"])
assert len(set(edges)) <= 1, ("Rail detail right edges differ", edges)
print("RAIL_DETAILS " + json.dumps(state, sort_keys=True))
PY
    [[ "$?" == 0 ]] || fail "rail: actual detail text, geometry, or semantic role differs"
}

rail_details_native() {
    local flag item count=0
    settings_wait_value '.places.driveSize == false and .places.trashCount == false'
    rail_assert_details false 0
    settings_open_key; settle
    settings_section places
    ipc settingsModel | jq -e '.[0].label == "Favorites" and ([.[-3:][] | .label] == ["Show drive size", "Show Trash count", "Sidebar width"])' >/dev/null \
        || fail "rail: Places labels/control order differ from the ruled board"
    trash_shot settings-places-details-off
    for flag in driveSize trashCount; do
        settings_click_control "places.$flag"
        settings_wait_value ".places.$flag == true"
    done
    rail_assert_details true 0
    trash_shot settings-places-details-on
    key -k Escape >/dev/null; settle
    for item in $(seq 1 12); do
        trash_guard "$payload/detail-$item.txt"
        printf 'rail count fixture\n' > "$payload/detail-$item.txt"
    done
    wait_listing 12
    for item in $(seq 1 12); do
        trash_move "detail-$item.txt" "$count" "$((11 - count))"
        count=$((count + 1))
    done
    rail_assert_details true 12
    trash_shot rail-details-on
    kill_flea
    launch "$payload"; wait_listing 0
    trash_wait '.count == 12 and .rail.countText == "12"' 'enabled rail count survives restart'
    settings_wait_value '.places.driveSize == true and .places.trashCount == true'
    rail_assert_details true 12
    settings_open_key; settle
    settings_section places
    for flag in driveSize trashCount; do
        settings_focus_row "places.$flag"
        key -k Space >/dev/null; settle
        settings_wait_value ".places.$flag == false"
    done
    key -k Escape >/dev/null; settle
    rail_assert_details false 12
    kill_flea
    launch "$payload"; wait_listing 0
    trash_wait '.count == 12 and .rail.countText == ""' 'disabled rail count survives restart'
    settings_wait_value '.places.driveSize == false and .places.trashCount == false'
    rail_assert_details false 12
    trash_shot rail-details-off-populated
    printf 'RAIL native-toggle-on/off empty/populated restart-on/off=ok; pixels require separate inspection.\n'
}

places_records_diagnostic() {
    local phase="$1" cursor
    cursor=$(ipc settingsCursor)
    printf 'PLACES_STATE phase=%s section=%s side=%s cursor=%s preset=%s\n' \
        "$phase" "$(ipc settingsSection)" "$(ipc settingsSide)" "$cursor" "$(ipc keymapPreset)"
    printf 'PLACES_SESSION %s\n' "$(ipc uiSettings | jq -c '.places.favourites')"
    printf 'PLACES_PERSISTED %s\n' "$(jq -c '.places.favourites' "$XDG_STATE_HOME/flea/ui.json")"
    printf 'PLACES_FOCUSED_ROW %s\n' "$(ipc settingsModel | jq -c --argjson cursor "$cursor" '.[$cursor]')"
    printf 'PLACES_KEY_FOCUS %s\n' "$(ipc keyDeliveryState)"
    printf 'PLACES_STATUS error=%s message=%s\n' "$(ipc statusError)" "$(ipc lastMessage | jq -Rs .)"
    printf 'PLACES_SAVING %s\n' "$(ipc favouritesSaving)"
}

places_wait_records() {
    local expected="$1" attempt matched=false
    for attempt in $(seq 1 30); do
        if ipc uiSettings | jq -e --argjson expected "$expected" '.places.favourites == $expected' >/dev/null \
            && jq -e --argjson expected "$expected" '.places.favourites == $expected' "$XDG_STATE_HOME/flea/ui.json" >/dev/null \
            && [[ "$(ipc favouritesSaving)" == false ]]; then
            matched=true
            break
        fi
        sleep 0.1
    done
    if [[ "$matched" != true ]]; then
        places_records_diagnostic records-timeout
        fail "places: session and persisted records never agreed on $expected"
    fi
    ipc railEntries | jq -e --argjson expected "$expected" '[.[] | select(.kind == "favourite") | .original] == $expected' >/dev/null \
        || fail "places: rail records differ from saved originals"
}

places_wait_cursor() {
    local expected="$1" chord="$2" attempt cursor
    # The file watcher can publish records before the writer's exit callback restores the moved cursor.
    for attempt in $(seq 1 30); do
        cursor=$(ipc settingsCursor)
        [[ "$cursor" == "$expected" ]] && return
        sleep 0.1
    done
    places_records_diagnostic "after-$chord-cursor-failure expected=$expected observed=$cursor"
    fail "places: $chord lost the moved row's cursor; expected $expected, observed $cursor"
}

places_click_part() {
    local id="$1" part="$2" wx wy ww wh x y
    settings_focus_row "$id"
    read -r x y <<< "$(ipc settingsFavouriteControlCentre "$id" "$part")"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    [[ "$x" =~ ^[0-9]+$ && "$y" =~ ^[0-9]+$ ]] || fail "places: $id/$part has no actual control centre"
    (( x > 0 && y > 0 && x < ww && y < wh )) || fail "places: $id/$part is outside the viewport"
    omarchy-drive click "$((wx + x))" "$((wy + y))" left >/dev/null
    settle
}

places_click_menu() {
    local label="$1" index x y wx wy ww wh
    index=$(menu_row_index "$label") || fail "places: menu lacks $label"
    read -r x y <<< "$(ipc contextMenuRowCentre "$index")"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    [[ "$x" =~ ^[0-9]+$ && "$y" =~ ^[0-9]+$ ]] || fail "places: menu $label has no actual centre"
    omarchy-drive click "$((wx + x))" "$((wy + y))" left >/dev/null
    settle
}

places_require_store() {
    sandbox_require "$XDG_STATE_HOME/flea/ui.json"
    sandbox_under "$SANDBOX_PATH" "$fixture_root/settingsplaces" \
        || fail "places: refusing a state mutation outside this case's sandbox"
}

places_drag_row() {
    local from="$1" to="$2" x y target_x target_y wx wy ww wh cursor_x cursor_y dy step deadline
    settings_focus_row "favourite:$from"
    read -r x y <<< "$(ipc settingsFavouriteControlCentre "favourite:$from" drag)"
    read -r target_x target_y <<< "$(ipc settingsFavouriteControlCentre "favourite:$to" drag)"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    [[ "$x" =~ ^[0-9]+$ && "$y" =~ ^[0-9]+$ && "$target_y" =~ ^[0-9]+$ ]] \
        || fail "places: drag endpoints are not actual rendered handles"
    (( target_y > 0 && target_y < wh )) || fail "places: drag target is outside the viewport"
    export YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket"
    omarchy-drive move "$((wx + x))" "$((wy + y))" >/dev/null
    assert_focus
    ydotool click 0x40 >/dev/null 2>&1 || fail "places: could not press the drag handle"
    trap 'ydotool click 0x80 >/dev/null 2>&1 || true' EXIT
    deadline=$((SECONDS + 5))
    # Relative uinput supplies wl_pointer.frame; Omarchy's cursor warp alone does not deliver a Qt drag.
    while (( SECONDS < deadline )); do
        read -r cursor_x cursor_y <<< "$(hyprctl cursorpos | tr -d ',')"
        [[ "$cursor_y" =~ ^[0-9]+$ ]] || fail "places: compositor did not report a pointer position"
        dy=$((wy + target_y - cursor_y))
        (( dy >= -1 && dy <= 1 )) && break
        step=$((dy / 2))
        (( step != 0 )) || step=$((dy > 0 ? 1 : -1))
        ydotool mousemove -x 0 -y "$step" >/dev/null 2>&1 || fail "places: native drag motion failed"
        sleep 0.05
    done
    ydotool click 0x80 >/dev/null 2>&1 || fail "places: could not release the drag handle"
    trap - EXIT
    (( dy >= -1 && dy <= 1 )) || fail "places: native pointer never reached the measured destination handle"
    settle
}

places_concurrent() (
    local dir="$1" expected="$2" first_pid first_id second_pid="" second_id="" second_address=""
    local instance pid address attempt rows width cursor active permissions_checks=0
    first_pid=$(flea_pid)
    first_id=$(qs list --all --json | jq -er --arg path "$flea_ui/shell.qml" --argjson pid "$first_pid" '.[] | select(.config_path == $path and .pid == $pid) | .id')
    key -M ctrl -k n -m ctrl >/dev/null
    for attempt in $(seq 1 100); do
        rows=$(qs list --all --json | jq -c --arg path "$flea_ui/shell.qml" '[.[] | select(.config_path == $path)]')
        [[ "$(jq length <<< "$rows")" == 2 ]] && break
        sleep 0.05
    done
    [[ "$(jq length <<< "$rows")" == 2 ]] || fail "places: native Ctrl+N did not open exactly one candidate window"
    second_pid=$(jq -er --arg id "$first_id" '.[] | select(.id != $id) | .pid' <<< "$rows")
    second_id=$(jq -er --arg id "$first_id" '.[] | select(.id != $id) | .id' <<< "$rows")
    for attempt in $(seq 1 100); do
        hyprctl -j clients | jq -e --argjson pid "$second_pid" 'any(.[]; .pid == $pid)' >/dev/null && break
        sleep 0.05
    done

    places_owned_window() {
        local owned_id="$1" owned_pid="$2" clients
        flea_process_owned "$first_pid" || fail "places: first window run ownership changed"
        flea_process_owned "$second_pid" || fail "places: second window run ownership changed"
        [[ "$owned_pid" =~ ^[0-9]+$ && -r "/proc/$owned_pid/environ" ]] || fail "places: owned window process vanished"
        qs list --all --json | jq -e --arg path "$flea_ui/shell.qml" --arg id "$owned_id" --argjson pid "$owned_pid" \
            'any(.[]; .config_path == $path and .id == $id and .pid == $pid)' >/dev/null || fail "places: instance ownership changed"
        tr '\0' '\n' < "/proc/$owned_pid/environ" | grep -Fx "FLEA_BIN=$flea_bin" >/dev/null || fail "places: candidate binary changed"
        tr '\0' '\n' < "/proc/$owned_pid/environ" | grep -Fx "FLEA_UI=$flea_ui" >/dev/null || fail "places: candidate UI changed"
        tr '\0' '\n' < "/proc/$owned_pid/environ" | grep -Fx "XDG_STATE_HOME=$dir/state" >/dev/null || fail "places: state sandbox changed"
        clients=$(hyprctl -j clients) || fail "places: native window ownership unavailable"
        jq -er --argjson pid "$owned_pid" --argjson first "$first_pid" --argjson second "$second_pid" --arg class "$flea_window_class" \
            '[.[] | select(.class == $class)] | select(all(.[]; .pid == $first or .pid == $second))
             | map(select(.pid == $pid)) | select(length == 1) | .[0].address' <<< "$clients" \
            || fail "places: foreign or ambiguous native Flea window appeared"
    }
    places_close_second() {
        [[ -n "$second_pid" ]] || return
        second_address=$(places_owned_window "$second_id" "$second_pid") || return 1
        omarchy-drive window close "$second_address" >/dev/null || return 1
        for attempt in $(seq 1 100); do
            kill -0 "$second_pid" 2>/dev/null || { second_pid=""; return; }
            sleep 0.05
        done
        fail "places: owned second window did not close"
    }
    trap 'places_close_second || exit 1' EXIT
    places_use() {
        instance="$1"; pid="$2"
        address=$(places_owned_window "$instance" "$pid") || fail "places: cannot select an owned window"
        omarchy-drive focus "$address" >/dev/null
    }
    ipc() { qs ipc -i "$instance" call flea "$@"; }
    key() {
        [[ "$(places_owned_window "$instance" "$pid")" == "$address" ]] || fail "places: window address changed before native input"
        [[ "$(hyprctl activewindow -j | jq -r .address)" == "$address" ]] || fail "places: another window took keyboard focus"
        omarchy-drive key --window "$address" "$@"
    }
    window_box() {
        [[ "$(places_owned_window "$instance" "$pid")" == "$address" ]] || fail "places: window address changed before pointer input"
        hyprctl -j clients | jq -er --arg address "$address" '.[] | select(.address == $address) | [.at[0],.at[1],.size[0],.size[1]] | join(" ")'
    }
    places_use "$second_id" "$second_pid"
    wait_listing 3
    places_wait_records "$expected"
    places_use "$first_id" "$first_pid"
    settings_open_key; settle
    settings_section places
    settings_focus_row "favourite:$(jq 'length - 1' <<< "$expected")"
    settings_focus_row favouriteActions
    key l >/dev/null; settle
    places_use "$second_id" "$second_pid"
    settings_open_key; settle
    settings_section places
    settings_focus_row "favourite:$(jq 'length - 1' <<< "$expected")"
    settings_focus_row favouriteActions
    key l >/dev/null
    places_require_store
    key -k Return >/dev/null; settle
    expected=$(jq -c '.[0:-1]' <<< "$expected")
    places_wait_records "$expected"
    places_use "$first_id" "$first_pid"
    places_wait_records "$expected"
    ipc settingsModel | jq -e 'any(.[]; .id == "favouriteActions" and .canRemove == false)' >/dev/null \
        || fail "places: external removal left a different record selected for Remove"
    places_require_store
    key -k Return >/dev/null; settle
    places_wait_records "$expected"
    settings_focus_row places.sidebarWidth
    key l >/dev/null; settle
    width=$(ipc uiSettings | jq '.places.sidebarWidth')
    settings_wait_value ".places.sidebarWidth == $width"

    places_use "$second_id" "$second_pid"
    key -k Escape >/dev/null; settle
    key -M ctrl -k l -m ctrl "$dir/listing/Beta" -k Return >/dev/null
    wait_path "$dir/listing/Beta"; wait_listing 0
    click_background; settle
    places_click_menu 'Add to Favorites'
    expected=$(jq -c --arg path "$dir/listing/Beta" '. + [{label:"Beta",path:$path}]' <<< "$expected")
    places_wait_records "$expected"
    places_use "$first_id" "$first_pid"
    places_wait_records "$expected"
    [[ "$(ipc path)" == "$dir/listing" && "$(ipc viewMode)" == list ]] || fail "places: external favourite save changed the other window's navigation"
    [[ "$(ipc uiSettings | jq '.places.sidebarWidth')" == "$width" ]] || fail "places: external favourite save overwrote another Settings value"
    cursor=$(ipc settingsCursor)
    [[ "$(ipc settingsModel | jq -r --argjson cursor "$cursor" '.[$cursor].id')" == places.sidebarWidth ]] \
        || fail "places: changed favourite count displaced focus from the active Settings control"
    [[ ! -e "$evidence_dir/places-concurrent-first.png" ]] || fail "places: concurrent screenshot already exists"
    omarchy-drive shot "$evidence_dir/places-concurrent-first.png" "$address" >/dev/null
    [[ -s "$evidence_dir/places-concurrent-first.png" ]] || fail "places: concurrent screenshot is missing"
    printf 'SHOT %s/places-concurrent-first.png\n' "$evidence_dir"
    printf 'PLACES_CONCURRENT first=%s/%s second=%s/%s store=%s records=%s\n' "$first_id" "$first_pid" "$second_id" "$second_pid" "$XDG_STATE_HOME/flea/ui.json" "$expected"

    places_rail_target() {
        ipc railEntries | jq -c --argjson index "$(ipc railCursor)" \
            'if $index < 0 then null else .[$index] | [.group,.kind,(.device // .uri // .path),.original] end'
    }
    places_select_rail() {
        local target="$1" step
        [[ "$(ipc settingsOpen)" == false ]] || { key -k Escape >/dev/null; settle; }
        [[ "$(ipc focusView)" == rail ]] || { key -k Tab >/dev/null; settle; }
        [[ "$(ipc focusView)" == rail ]] || fail "places: native Tab did not reach the rail"
        key g >/dev/null
        for step in $(seq 1 "$target"); do key j >/dev/null; done
        settle
        [[ "$(ipc railCursor)" == "$target" ]] || fail "places: native rail selection did not reach $target"
    }
    places_second_remove_last() {
        places_use "$second_id" "$second_pid"
        [[ "$(ipc settingsOpen)" == true ]] || { settings_open_key; settle; }
        settings_section places
        settings_focus_row "favourite:$(jq 'length - 1' <<< "$expected")"
        settings_focus_row favouriteActions
        key l >/dev/null
        places_require_store
        key -k Return >/dev/null; settle
        expected=$(jq -c '.[0:-1]' <<< "$expected")
        places_wait_records "$expected"
    }
    local group target original_target target_path
    for group in network device; do
        places_use "$first_id" "$first_pid"
        target=$(ipc railEntries | jq -er --arg group "$group" \
            'to_entries | map(select(.value.group == $group and ($group != "device" or .value.kind == "disk"))) | .[0].key') \
            || fail "places: no native $group row is available for external-change proof"
        places_select_rail "$target"
        original_target=$(places_rail_target)
        target_path=$(ipc railEntries | jq -r --argjson target "$target" '.[$target].path')
        places_use "$second_id" "$second_pid"
        [[ "$(ipc settingsOpen)" == true ]] || { settings_open_key; settle; }
        settings_section places
        settings_focus_row favouriteActions
        key h >/dev/null
        places_require_store
        key -k Return >/dev/null; settle
        expected=$(jq -c --arg path "$dir/listing/Beta" '. + [{label:"Beta",path:$path}]' <<< "$expected")
        places_wait_records "$expected"
        places_use "$first_id" "$first_pid"
        places_wait_records "$expected"
        [[ "$(places_rail_target)" == "$original_target" && "$(ipc railCursor)" == "$((target + 1))" ]] \
            || fail "places: external addition changed the selected $group target"
        places_second_remove_last
        places_use "$first_id" "$first_pid"
        places_wait_records "$expected"
        [[ "$(places_rail_target)" == "$original_target" && "$(ipc railCursor)" == "$target" ]] \
            || fail "places: external removal changed the selected $group target"
        if [[ "$group" == device ]]; then
            key -k Return >/dev/null
            wait_path "$target_path"
            permissions_expect state ready
            permissions_expect listInFlight false
            key -M ctrl -k l -m ctrl "$dir/listing" -k Return >/dev/null
            wait_path "$dir/listing"; wait_listing 3
        fi
    done
    target=$(ipc railEntries | jq -er 'to_entries | map(select(.value.kind == "favourite")) | .[-1].key')
    places_select_rail "$target"
    places_second_remove_last
    places_use "$first_id" "$first_pid"
    places_wait_records "$expected"
    [[ "$(ipc railCursor)" == -1 ]] || fail "places: removed rail target silently selected its neighbour"
    key -k Return >/dev/null; settle
    [[ "$(ipc path)" == "$dir/listing" ]] || fail "places: Enter activated a replacement for the removed rail target"
    printf 'PLACES_RAIL_IDENTITY network/device add/remove target-retained device-enter=ok removed-target-enter=inert\n'
    places_close_second || fail "places: concurrent owned-window cleanup failed"
    trap - EXIT
)

places_external_failure() {
    local dir="$1" expected doc="$XDG_STATE_HOME/flea/ui.json" attempt mode
    expected=$(jq -c '.places.favourites' "$doc")
    launch "$dir/listing"; wait_listing 3
    places_wait_records "$expected"
    cp "$doc" "$dir/live-store.original"
    places_require_store
    sandbox_require "$dir/removed-ui.json"
    mv "$doc" "$dir/removed-ui.json"
    for attempt in $(seq 1 30); do
        [[ "$(ipc lastMessage)" == *'ui.json could not be read'* ]] && break
        sleep 0.1
    done
    [[ "$(ipc statusError)" == true && "$(ipc lastMessage)" == *'ui.json could not be read'* ]] \
        || fail "places: missing live store did not report its read failure"
    ipc uiSettings | jq -e --argjson expected "$expected" '.places.favourites == $expected' >/dev/null \
        || fail "places: missing live store erased visible records"
    key -k Escape >/dev/null; settle
    [[ "$(ipc statusError)" == false ]] || fail "places: missing-store acknowledgement left an unexplained error: $(ipc statusActivityState)"
    places_require_store
    sandbox_require "$dir/removed-ui.json"
    mv "$dir/removed-ui.json" "$doc"
    places_wait_records "$expected"
    places_require_store
    : > "$doc"
    for attempt in $(seq 1 30); do
        [[ "$(ipc lastMessage)" == *'invalid ui.json'* ]] && break
        sleep 0.1
    done
    [[ "$(ipc statusError)" == true && "$(ipc lastMessage)" == *'invalid ui.json'* && ! -s "$doc" ]] \
        || fail "places: empty live bytes were treated as a valid empty store"
    ipc uiSettings | jq -e --argjson expected "$expected" '.places.favourites == $expected' >/dev/null \
        || fail "places: empty live bytes dropped previously visible records"
    key -k Escape >/dev/null; settle
    [[ "$(ipc statusError)" == false ]] || fail "places: empty-store acknowledgement left an unexplained error: $(ipc statusActivityState)"
    places_require_store
    cp "$dir/live-store.original" "$doc"
    places_wait_records "$expected"
    printf '{"places":\n' > "$dir/malformed.original"
    places_require_store
    cp "$dir/malformed.original" "$doc"
    for attempt in $(seq 1 30); do
        [[ "$(ipc lastMessage)" == *'invalid ui.json'* ]] && break
        sleep 0.1
    done
    [[ "$(ipc statusError)" == true && "$(ipc lastMessage)" == *'invalid ui.json'* ]] \
        || fail "places: external malformed state did not report its live read failure"
    ipc uiSettings | jq -e --argjson expected "$expected" '.places.favourites == $expected' >/dev/null \
        || fail "places: malformed external state dropped previously visible records"
    cmp "$doc" "$dir/malformed.original" || fail "places: watcher rewrote malformed original bytes"
    shot places-malformed-retained
    key -k Escape >/dev/null; settle
    [[ "$(ipc statusError)" == false ]] || fail "places: malformed-store acknowledgement left an unexplained error: $(ipc statusActivityState)"
    settings_open_key; settle
    settings_section places
    settings_focus_row favouriteActions
    key h >/dev/null
    places_require_store
    key -k Return >/dev/null; settle
    expected=$(jq -cn --arg path "$dir/listing" '[{label:"listing",path:$path}]')
    places_wait_records "$expected"
    cmp "$doc.broken" "$dir/malformed.original" || fail "places: native recovery did not retain the exact malformed backup"

    mode=$(stat -c %a "$doc")
    cp "$doc" "$dir/readable.original"
    places_require_store
    chmod 000 "$doc"
    for attempt in $(seq 1 30); do
        [[ "$(ipc lastMessage)" == *'ui.json could not be read'* ]] && break
        sleep 0.1
    done
    [[ "$(ipc statusError)" == true && "$(ipc lastMessage)" == *'ui.json could not be read'* ]] \
        || fail "places: unreadable live store did not report its read failure: $(ipc statusActivityState)"
    key -k Return >/dev/null; settle
    for attempt in $(seq 1 30); do
        ipc statusActivityState | jq -e '.errors == 2' >/dev/null && break
        sleep 0.1
    done
    ipc statusActivityState | jq -e '.errors == 2' >/dev/null \
        || fail "places: unreadable Add did not queue its write refusal: $(ipc statusActivityState)"
    [[ "$(ipc lastMessage)" == *'ui.json could not be read'* ]] || fail "places: Add displaced the unacknowledged read error"
    key -k Escape >/dev/null; settle
    [[ "$(ipc settingsOpen)" == false ]] || fail "places: Escape did not close Settings before acknowledging errors"
    key -k Escape >/dev/null; settle
    places_require_store
    chmod "$mode" "$doc"
    [[ "$(ipc statusError)" == true && "$(ipc lastMessage)" == *'could not be read, so the state file was not written'* ]] \
        || fail "places: unreadable store did not refuse the real Add operation"
    places_wait_records "$expected"
    cmp "$doc" "$dir/readable.original" || fail "places: failed Add replaced an unreadable store"
    key -k Escape >/dev/null; settle
    [[ "$(ipc statusError)" == false ]] || fail "places: Add refusal acknowledgement left an unexplained error: $(ipc statusActivityState)"
    settings_open_key; settle
    settings_section places
    settings_focus_row favouriteActions
    key h >/dev/null
    key -k Return >/dev/null; settle
    expected=$(jq -c '. + .' <<< "$expected")
    places_wait_records "$expected"
    shot places-read-failure-recovery
    printf 'PLACES_FAILURE missing-retention empty-retention malformed-retention exact-backup unreadable-refusal native-retry=ok\n'
}

case_settingsplaces() {
    local dir="$fixture_root/settingsplaces" records expected initial index attempt
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing/Alpha" "$dir/listing/Beta" "$dir/state/flea" "$dir/config/gtk-3.0" "$dir/data"
    printf 'listing fixture\n' > "$dir/listing/proof.txt"
    export XDG_STATE_HOME="$dir/state" XDG_CONFIG_HOME="$dir/config" XDG_DATA_HOME="$dir/data" FLEA_UI="$flea_ui"
    printf 'file://%s GTK label preserved\ninvalid GTK entry\n' "$dir/listing/Alpha" > "$dir/config/gtk-3.0/bookmarks"
    cp "$dir/config/gtk-3.0/bookmarks" "$dir/bookmarks.original"
    printf '%s\n' '{"keys":"default","view":"list","places":{"favourites":[]}}' > "$dir/state/flea/ui.json"
    launch "$dir/listing"
    wait_listing 3
    places_wait_records '[]'
    settings_open_key; settle
    settings_section places
    settings_focus_row favouriteActions
    ipc settingsModel | jq -e 'any(.[]; .id == "favouriteActions" and .canRemove == false)' >/dev/null \
        || fail "places: empty manager enables Remove"
    key -k Space >/dev/null; settle
    places_wait_records '[]'
    places_click_part favouriteActions remove
    places_wait_records '[]'
    shot places-empty
    places_click_part favouriteActions add
    initial=$(jq -cn --arg path "$dir/listing" '[{label:"listing",path:$path}]')
    places_wait_records "$initial"
    key -k Return >/dev/null; settle
    expected=$(jq -c '. + .' <<< "$initial")
    places_wait_records "$expected"
    key -k Escape >/dev/null; settle

    seek_row_named Alpha
    click_row "$(ipc cursor)" right
    settle
    menu_seek 'Add to Favorites'
    key -k Return >/dev/null; settle
    expected=$(jq -c --arg path "$dir/listing/Alpha" '. + [{label:"Alpha",path:$path}]' <<< "$expected")
    places_wait_records "$expected"
    seek_row_named proof.txt
    click_row "$(ipc cursor)" right
    settle
    ipc contextMenuModel | jq -e 'any(.[]; .action == "addFavourite" and .disabled == true)' >/dev/null \
        || fail "places: file row can be pinned as a directory"
    places_click_menu 'Add to Favorites'
    places_wait_records "$expected"
    key -k Escape >/dev/null; settle
    click_background; settle
    places_click_menu 'Add to Favorites'
    expected=$(jq -c --arg path "$dir/listing" '. + [{label:"listing",path:$path}]' <<< "$expected")
    places_wait_records "$expected"
    shot places-listing-add

    settings_open_key; settle
    settings_section places
    settings_focus_row favourite:2
    places_records_diagnostic before-Shift+K
    key -M shift -k k -m shift >/dev/null; settle
    expected=$(jq -c '.[1:3] |= reverse' <<< "$expected")
    places_wait_records "$expected"
    places_wait_cursor 2 Shift+K
    places_records_diagnostic before-Shift+J
    key -M shift -k j -m shift >/dev/null; settle
    expected=$(jq -c '.[1:3] |= reverse' <<< "$expected")
    places_wait_records "$expected"
    places_wait_cursor 3 Shift+J
    places_require_store
    places_drag_row 2 1
    expected=$(jq -c '.[1:3] |= reverse' <<< "$expected")
    places_wait_records "$expected"
    shot places-pointer-reorder
    places_drag_row 1 2
    expected=$(jq -c '.[1:3] |= reverse' <<< "$expected")
    places_wait_records "$expected"
    places_drag_row 2 1
    expected=$(jq -c '.[1:3] |= reverse' <<< "$expected")
    places_wait_records "$expected"
    settings_focus_row "favourite:$(jq 'length - 1' <<< "$expected")"
    settings_focus_row favouriteActions
    key l >/dev/null; key -k Space >/dev/null; settle
    places_wait_records "$expected"
    places_require_store
    key -k Return >/dev/null; settle
    expected=$(jq -c '.[0:-1]' <<< "$expected")
    places_wait_records "$expected"
    settings_focus_row favourite:1
    local remove_x remove_y wx wy ww wh
    read -r remove_x remove_y <<< "$(ipc settingsFavouriteControlCentre favouriteActions remove)"
    read -r wx wy ww wh < <(window_box) || fail "places: native window coordinates unavailable"
    [[ "$remove_x" =~ ^[0-9]+$ && "$remove_y" =~ ^[0-9]+$ ]] || fail "places: Remove has no actual control centre"
    (( remove_x > 0 && remove_y > 0 && remove_x < ww && remove_y < wh )) || fail "places: Remove is outside the viewport"
    places_require_store
    omarchy-drive click "$((wx + remove_x))" "$((wy + remove_y))" left >/dev/null
    expected=$(jq -c 'del(.[1])' <<< "$expected")
    places_wait_records "$expected"
    key -k Escape >/dev/null; settle
    index=$(ipc railEntries | jq -er 'map(.kind) | index("favourite")')
    click_rail_row "$index" right; settle
    places_require_store
    places_click_menu Remove
    expected=$(jq -c 'del(.[0])' <<< "$expected")
    places_wait_records "$expected"
    launch "$dir/listing"; wait_listing 3
    places_wait_records "$expected"
    cmp "$dir/bookmarks.original" "$dir/config/gtk-3.0/bookmarks" \
        || fail "places: native manager rewrote GTK bookmarks"

    seek_row_named Beta
    click_row "$(ipc cursor)" right; settle
    menu_seek 'Add to Favorites'
    for attempt in $(seq 1 30); do
        ipc menuState | jq -e '.snapshotReady' >/dev/null && break
        sleep 0.1
    done
    ipc menuState | jq -e '.snapshotReady' >/dev/null || fail "places: selected directory snapshot never completed"
    sandbox_require "$dir/listing/Beta"
    sandbox_require "$dir/Beta-original"
    mv "$dir/listing/Beta" "$dir/Beta-original"
    mkdir "$dir/listing/Beta"
    key -k Return >/dev/null; settle
    [[ "$(ipc statusError)" == true && "$(ipc lastMessage)" == *'changed'* ]] \
        || fail "places: replaced directory did not refuse the captured menu action"
    places_wait_records "$expected"
    sandbox_require "$dir/listing/Beta"
    sandbox_require "$dir/Beta-replacement"
    sandbox_require "$dir/Beta-original"
    mv "$dir/listing/Beta" "$dir/Beta-replacement"
    mv "$dir/Beta-original" "$dir/listing/Beta"
    shot places-changed-directory-refused

    kill_flea
    records=$(jq -cn --arg path "$dir/listing/Alpha" --arg missing "$dir/missing" \
        '[{label:"  Original <label>  ",path:$path},{label:"Second label",path:$path},{label:"Unavailable",path:$missing},{label:"Relative",path:"relative"},17,{label:"Network URI",path:"smb://unavailable.invalid/share"}]')
    jq -cn --argjson records "$records" '{keys:"default",view:"list",places:{favourites:$records},unrelated:{retain:true}}' > "$dir/state/flea/ui.json"
    launch "$dir/listing"; wait_listing 3
    places_wait_records "$records"
    settings_open_key; settle
    settings_section places
    for attempt in $(seq 1 30); do
        ipc settingsModel | jq -e 'any(.[]; .id == "favourite:2" and (.error | length > 0))' >/dev/null && break
        sleep 0.1
    done
    ipc settingsModel | jq -e 'any(.[]; .id == "favourite:0" and .label == "  Original <label>  ") and any(.[]; .id == "favourite:3" and (.error | length > 0)) and any(.[]; .id == "favourite:4" and (.error | length > 0)) and any(.[]; .id == "favourite:5" and .value == "smb://unavailable.invalid/share")' >/dev/null \
        || fail "places: original labels, invalid rows, or URI scheme changed"
    settings_focus_row favourite:3
    key -k Return >/dev/null; settle
    [[ "$(ipc lastMessage)" == *'path must be absolute'* && "$(ipc path)" == "$dir/listing" ]] \
        || fail "places: invalid path activation did not explain its refusal"
    settings_focus_row favourite:4
    key -k Return >/dev/null; settle
    ipc statusActivityState | jq -e '.errors == 2' >/dev/null \
        || fail "places: invalid record did not queue its error: $(ipc statusActivityState); message=$(ipc lastMessage)"
    [[ "$(ipc lastMessage)" == *'path must be absolute'* ]] || fail "places: a new error displaced the unacknowledged error"
    key -k Escape >/dev/null; settle
    key -k Escape >/dev/null; settle
    [[ "$(ipc lastMessage)" == *'invalid favorite record'* ]] || fail "places: acknowledging the first error did not reveal the invalid record error: $(ipc lastMessage)"
    key -k Escape >/dev/null; settle
    [[ "$(ipc statusError)" == false ]] || fail "places: acknowledging both errors left an unexplained failure"
    settings_open_key; settle
    settings_section places
    places_wait_records "$records"
    shot places-invalid-originals
    settings_focus_row favourite:0
    key -k Return >/dev/null; settle
    wait_path "$dir/listing/Alpha"
    wait_listing 0
    settings_focus_row favouriteActions
    key h >/dev/null; key -k Return >/dev/null; settle
    expected=$(jq -c --arg path "$dir/listing/Alpha" '. + [{label:"Alpha",path:$path}]' <<< "$records")
    places_wait_records "$expected"
    key -k Escape >/dev/null; settle
    launch "$dir/listing"; wait_listing 3
    places_wait_records "$expected"
    jq -e '.unrelated.retain == true' "$dir/state/flea/ui.json" >/dev/null || fail "places: favourites edits dropped unrelated preferences"
    cmp "$dir/bookmarks.original" "$dir/config/gtk-3.0/bookmarks" || fail "places: original GTK bytes changed"
    places_concurrent "$dir" "$expected" || fail "places: concurrent-window proof failed"
    places_external_failure "$dir"
    printf 'PLACES original-records duplicates panel/listing/rail keyboard pointer-drag invalid-retention restart=ok\n'
    kill_flea
}
