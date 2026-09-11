#!/usr/bin/env bash
# Sourced by ui.sh; all actions enter through native pointer/key input, with read-only IPC observations.
menus_guard() {
    local target="$1" canonical
    [[ -n "$target" && "$target" == /* && -f "$menu_box/.flea-test-sandbox" ]] || fail "menus: invalid sandbox target"
    canonical=$(realpath -m -- "$target") || fail "menus: cannot resolve $target"
    [[ "$canonical" == "$menu_box/"* && "$canonical" != "$menu_box" ]] || fail "menus: target outside owned sandbox: $target"
}

menus_expect() {
    local observer="$1" expression="$2" label="$3" observed deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        observed=$(ipc "$observer") || fail "menus: observer failed: $observer"
        if jq -e "$expression" <<< "$observed" >/dev/null; then
            menus_checks=$((menus_checks + 1))
            printf 'MENUS_CHECK %s %s\n' "$menus_checks" "$label"
            return
        fi
        sleep 0.05
    done
    fail "menus: $label: $observed"
}

menus_equal() {
    local label="$1" expected="$2" observed="$3"
    [[ "$observed" == "$expected" ]] || fail "menus: $label: expected $expected, observed $observed"
    menus_checks=$((menus_checks + 1))
    printf 'MENUS_CHECK %s %s expected=%q observed=%q\n' "$menus_checks" "$label" "$expected" "$observed"
}

menus_error() {
    local text="$1" label="$2" observed deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        observed="$(ipc statusPrimary) $(ipc statusDetail)" || fail "menus: error observer failed"
        if [[ "$(ipc statusError)" == true && "$observed" == *"$text"* ]]; then
            menus_checks=$((menus_checks + 1))
            printf 'MENUS_CHECK %s %s error=%q\n' "$menus_checks" "$label" "$observed"
            return
        fi
        sleep 0.05
    done
    fail "menus: $label did not report $text: $observed"
}

# The message a pane emitted, polled directly: lastMessage answers a bare string, so it is compared
# rather than filtered, and the status bar's own slot is a separate question (see menus_message).
menus_said() {
    local text="$1" label="$2" observed deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        observed=$(ipc lastMessage) || fail "menus: message observer failed"
        if [[ "$observed" == "$text" ]]; then
            menus_checks=$((menus_checks + 1))
            printf 'MENUS_CHECK %s %s said=%q\n' "$menus_checks" "$label" "$observed"
            return
        fi
        sleep 0.05
    done
    fail "menus: $label did not say $text: $observed"
}

menus_message() {
    local text="$1" label="$2" observed deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        observed=$(ipc statusPrimary) || fail "menus: primary status observer failed"
        if [[ "$(ipc statusError)" == false && "$observed" == *"$text"* ]]; then
            menus_checks=$((menus_checks + 1))
            printf 'MENUS_CHECK %s %s status=%q\n' "$menus_checks" "$label" "$observed"
            return
        fi
        sleep 0.05
    done
    fail "menus: $label did not report $text: $observed"
}

menus_same_file() {
    local label="$1" original="$2" copy="$3"
    cmp -s -- "$original" "$copy" || fail "menus: $label: contents differ: $original and $copy"
    menus_checks=$((menus_checks + 1))
    printf 'MENUS_CHECK %s %s original=%q copy=%q\n' "$menus_checks" "$label" "$original" "$copy"
}

menus_acknowledge() {
    if [[ "$(ipc statusError)" == true ]]; then
        key -k Escape >/dev/null
        menus_expect statusError '. == false' "Escape acknowledges the current error"
    fi
}

menus_visit() {
    local path="$1" count="$2"
    menus_guard "$path"
    key -M ctrl -k l -m ctrl "$path" -k Return >/dev/null
    wait_path "$path"
    wait_listing "$count"
}

menus_point() {
    local centre="$1" button="${2:-left}" cx cy wx wy ww wh
    read -r cx cy <<< "$centre"
    [[ "$cx" =~ ^-?[0-9]+$ && "$cy" =~ ^-?[0-9]+$ ]] || fail "menus: no live control centre: $centre"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    (( cx >= 0 && cy >= 0 && cx < ww && cy < wh )) || fail "menus: control is clipped outside the viewport"
    assert_focus
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" "$button" >/dev/null || fail "menus: pointer delivery failed"
}

menus_control() {
    local observer="$1" name="$2" state centre
    state=$(ipc "$observer") || fail "menus: cannot observe $observer"
    centre=$(jq -er --arg name "$name" '.controls[] | select(.name == $name and .visible) | .centre' <<< "$state") \
        || fail "menus: $name is not visible in $observer"
    menus_point "$centre"
}

menus_seek() {
    local action="$1" state target cursor step count
    state=$(ipc menuState)
    target=$(jq -er --arg action "$action" '.entries | to_entries[] | select(.value.action == $action and .value.disabled != true) | .key' <<< "$state") \
        || fail "menus: $action is absent or disabled: $state"
    count=$(jq -r '.entries | length' <<< "$state")
    for ((step = 0; step <= count; step++)); do
        cursor=$(ipc contextMenuCursor)
        [[ "$cursor" == "$target" ]] && return
        if (( cursor < target )); then key -k Down >/dev/null; else key -k Up >/dev/null; fi
    done
    fail "menus: keyboard could not reach $action"
}

menus_choose() {
    local action="$1" input="${2:-key}" target
    menus_seek "$action"
    if [[ "$input" == pointer ]]; then
        target=$(ipc contextMenuCursor)
        menus_point "$(ipc contextMenuRowCentre "$target")"
    else
        key -k Return >/dev/null
    fi
}

# OpenWith.html rule 3: the dialog's door is the flyout's tail row, under its own separator, so a
# plain Return on the parent row opens the flyout rather than the card.
menus_open_with_dialog() {
    local rows index
    menus_seek openWith
    key -k Right >/dev/null
    rows=$(ipc menuState | jq -r '[.entries[] | select(.action == "openWith") | .submenu[]] | length')
    [[ "$rows" -gt 0 ]] || fail "menus: the Open with flyout named nothing"
    for ((index = 1; index < rows; index++)); do key -k Down >/dev/null; done
    key -k Return >/dev/null
}

menus_file_menu() {
    local name="$1" input="${2:-pointer}" index
    index=$(row_index_of "$name")
    click_row "$index" left
    if [[ "$input" == key ]]; then key -M shift -k F10 -m shift >/dev/null
    elif [[ "$input" == menu-key ]]; then key -k Menu >/dev/null
    elif [[ "$input" == menu-letter ]]; then key m >/dev/null
    else click_row "$index" right; fi
    menus_expect menuState '.opened and .hasRow and (.forRail | not) and .snapshotReady and .snapshotId > 0' "file menu opens and snapshots by $input"
}

menus_shot() {
    local name="$1" png="$evidence_dir/menus-$1-$$.png" canonical
    [[ -f "$run_root/.flea-test-sandbox" && "$png" == /* && ! -e "$png" ]] || fail "menus: screenshot must be fresh and sandboxed"
    canonical=$(realpath -m -- "$png") || fail "menus: cannot resolve screenshot path"
    [[ "$canonical" == "$run_root/"* ]] || fail "menus: screenshot escaped evidence sandbox"
    mkdir -p "$evidence_dir" || fail "menus: cannot create evidence directory"
    omarchy-drive shot "$png" flea >/dev/null || fail "menus: native capture failed"
    [[ -s "$png" ]] || fail "menus: native capture is empty"
    printf 'MENUS_SHOT_REQUIRES_INSPECTION %s\n' "$png"
}

menus_confirmation() {
    local name="$1" preset="$2" token
    menus_file_menu "$name"
    menus_choose deletePermanently
    menus_expect menuDialogState '.confirmation.opened and (.confirmation.destructiveFocus | not)' "permanent deletion opens on Cancel"
    menus_shot "$preset-confirmation"
    key -k Return >/dev/null
    menus_expect menuDialogState '.opened | not' "reflexive Enter cancels deletion"
    [[ -f "$menu_dir/$name" ]] || fail "menus: Enter on Cancel deleted the fixture"
    menus_file_menu "$name" key
    menus_choose deletePermanently
    menus_expect menuDialogState '.confirmation.opened' "deletion can reopen after cancellation"
    key -k Tab >/dev/null
    menus_expect menuDialogState '.confirmation.destructiveFocus' "Tab reaches destructive choice"
    key h >/dev/null
    menus_expect menuDialogState '.confirmation.destructiveFocus | not' "h restores Cancel"
    key l >/dev/null
    menus_expect menuDialogState '.confirmation.destructiveFocus' "l reaches destructive choice"
    key -k Left >/dev/null
    menus_expect menuDialogState '.confirmation.destructiveFocus | not' "Left restores Cancel"
    key -k Right >/dev/null
    key -k Escape >/dev/null
    menus_expect menuDialogState '.opened | not' "Escape cancels from destructive choice"
    [[ -f "$menu_dir/$name" ]] || fail "menus: Escape deleted the fixture"
}

menus_permissions() {
    local name="$1" preset="$2" actual
    menus_file_menu "$name"
    menus_choose permissions pointer
    menus_expect permissionsState '.opened and .editable and .mode == "0644"' "permissions reads actual file mode"
    menus_shot "$preset-permissions"
    menus_control permissionsState 'Owner execute'
    menus_expect permissionsState '.mode == "0744"' "permission checkbox toggles one bit"
    key -k space >/dev/null
    menus_expect permissionsState '.mode == "0644"' "Space toggles focused permission checkbox"
    menus_control permissionsState Octal
    key -M ctrl -k a -m ctrl 999 >/dev/null
    menus_expect permissionsState '.mode == "999" and any(.controls[]; .name == "Apply" and (.enabled | not))' "invalid octal disables Apply"
    key -k Escape >/dev/null
    menus_expect permissionsState '.opened | not' "permissions Escape cancels"
    [[ "$(stat -c %a "$menu_dir/$name")" == 644 ]] || fail "menus: cancelled permissions changed disk mode"
    menus_file_menu "$name" key
    menus_choose permissions
    menus_expect permissionsState '.opened and .editable' "permissions reopens"
    menus_control permissionsState Octal
    key -M ctrl -k a -m ctrl 640 -k Return >/dev/null
    menus_expect permissionsState 'any(.controls[]; .name == "Apply" and .focused)' "valid octal Enter focuses Apply"
    menus_guard "$menu_dir/$name"
    key -k space >/dev/null
    menus_expect permissionsState '.opened | not' "Space applies mode"
    actual=$(stat -c %a "$menu_dir/$name")
    [[ "$actual" == 640 ]] || fail "menus: mode is $actual after Apply, expected 640"
    menus_guard "$menu_dir/$name"
    chmod 644 "$menu_dir/$name" || fail "menus: could not restore fixture mode"
}

menus_launcher_fixture() {
    local real_gio target
    real_gio=$(command -v gio) || fail "menus: GIO is required for the real application registry query"
    menus_guard "$menu_box/bin/gio"
    cat > "$menu_box/bin/gio" <<'SH'
#!/usr/bin/env bash
set -eu
box=${FLEA_MENUS_BOX:?}
[[ "$box" == /* && -f "$box/.flea-test-sandbox" ]] || exit 90
guard() {
    [[ -n "$1" && "$1" == /* ]] || exit 96
    local resolved
    resolved=$(realpath -m -- "$1") || exit 96
    [[ "$resolved" == "$box/"* && "$resolved" != "$box" ]] || exit 96
}
case "${1:-}" in
  info) exec "$FLEA_MENUS_GIO" "$@" ;;
  mime) [[ $# == 2 ]] || exit 91; exec "$FLEA_MENUS_GIO" "$@" ;;
  mount) [[ $# == 2 && "$2" == -l ]] || exit 92; exec "$FLEA_MENUS_GIO" "$@" ;;
  open)
    [[ $# == 2 ]] || exit 97
    guard "$2"
    guard "$box/gio-open.log"
    printf '%s\n' "$2" >> "$box/gio-open.log"
    exit 23 ;;
  trash|list|monitor)
    [[ "${DBUS_SESSION_BUS_ADDRESS:-}" == "$FLEA_MENUS_BUS" && "${XDG_DATA_HOME:-}" == "$box/data" ]] || exit 98
    if [[ "$1" == trash && "${2:-}" == -- && $# -ge 3 ]]; then
        shift 2
        for target in "$@"; do guard "$target"; done
        guard "$box/gio-trash.log"
        printf '%s\n' "$@" >> "$box/gio-trash.log"
        exec "$FLEA_MENUS_GIO" trash -- "$@"
    fi
    [[ $# == 2 && ( "$1 $2" == 'trash --list' || "$1 $2" == 'list trash:///' || "$1 $2" == 'monitor --dir=trash:///' ) ]] || exit 98
    exec "$FLEA_MENUS_GIO" "$@" ;;
  launch)
    [[ $# == 3 && "$2" == "$box/data/applications/"* && "$3" == "$box/list/"* ]] || exit 93
    [[ ! -L "$box/launcher.pid" && ! -L "$box/launcher-mode" ]] || exit 94
    printf '%s\n' "$$" > "$box/launcher.pid"
    if [[ "$(cat "$box/launcher-mode")" == block ]]; then read -r release < "$box/launcher-gate"; fi
    exit 23 ;;
esac
exit 95
SH
    chmod +x "$menu_box/bin/gio"
    printf '[Desktop Entry]\nType=Application\nName=Flea fixture viewer\nExec=/usr/bin/false %%f\nMimeType=text/plain;\n' > "$menu_box/data/applications/flea-menu-fixture.desktop"
    printf '[Default Applications]\ntext/plain=flea-menu-fixture.desktop;\n[Added Associations]\ntext/plain=flea-menu-fixture.desktop;\n' > "$menu_box/config/mimeapps.list"
    printf 'fail\n' > "$menu_box/launcher-mode"
    for target in gio-open.log gio-trash.log; do
        menus_guard "$menu_box/$target"
        : > "$menu_box/$target"
    done
    mkfifo "$menu_box/launcher-gate" || fail "menus: cannot create cancellation fixture gate"
    export FLEA_MENUS_BOX="$menu_box" FLEA_MENUS_GIO="$real_gio" FLEA_MENUS_BUS="$trash_bus_address" PATH="$menu_box/bin:$PATH"
}

menus_open_with() {
    local state target cursor count step pid deadline
    menus_file_menu a.txt
    menus_open_with_dialog
    menus_expect menuDialogState '.opened and (.busy | not) and any(.applications[]; .id == "flea-menu-fixture.desktop")' "Open With queries the real fixture registry"
    state=$(ipc menuDialogState)
    # The card draws a registered application twice, once per group, so the first seat is the target.
    target=$(jq -r '[.applications | to_entries[] | select(.value.id == "flea-menu-fixture.desktop")][0].key' <<< "$state")
    count=$(jq -r '.applications | length' <<< "$state")
    for ((step = 0; step <= count; step++)); do
        cursor=$(ipc menuDialogState | jq -r .cursor)
        [[ "$cursor" == "$target" ]] && break
        if (( cursor < target )); then key -k Down >/dev/null; else key -k Up >/dev/null; fi
    done
    [[ "$cursor" == "$target" ]] || fail "menus: cannot focus the fixture viewer"
    key -k Return >/dev/null
    menus_expect menuDialogState '.opened and (.busy | not) and (.error | contains("23"))' "launcher failure reaches live dialog"
    key -k Escape >/dev/null
    menus_guard "$menu_box/launcher.pid"
    rm -f -- "$menu_box/launcher.pid"
    printf 'block\n' > "$menu_box/launcher-mode"
    menus_file_menu a.txt
    menus_open_with_dialog
    menus_expect menuDialogState '.opened and (.busy | not) and .applications[0].id == "flea-menu-fixture.desktop"' "fixture viewer retains registry priority"
    key -k Return >/dev/null
    menus_expect menuDialogState '.busy and .committing and any(.controls[]; .name == "Cancel" and .enabled)' "Cancel remains available while launcher waits"
    deadline=$((SECONDS + 15))
    while [[ ! -s "$menu_box/launcher.pid" ]] && (( SECONDS < deadline )); do sleep 0.05; done
    [[ -s "$menu_box/launcher.pid" ]] || fail "menus: owned launcher never started"
    pid=$(cat "$menu_box/launcher.pid")
    menus_control menuDialogState Cancel
    menus_expect menuDialogState '.opened | not' "pointer Cancel closes launching Open With"
    deadline=$((SECONDS + 15))
    while kill -0 "$pid" 2>/dev/null && (( SECONDS < deadline )); do sleep 0.05; done
    kill -0 "$pid" 2>/dev/null && fail "menus: cancelled owned launcher remains alive"
}

menus_dialog_keys() {
    local preset="$1" action first name before
    before=$(stat -c '%d:%i:%u:%g:%a:%s:%Y' "$menu_dir/a.txt")
    for action in moveTo copyTo openWith; do
        first=Field
        forward=(Cancel Submit)
        probe=menuDialogState
        # OpenWith.html rule 8 gives its own card a wider cycle: search, list, always box, buttons.
        if [[ "$action" == openWith ]]; then
            first=Applications
            forward=(Always Cancel Open Field)
            probe=openWithState
        fi
        menus_file_menu a.txt key
        if [[ "$action" == openWith ]]; then menus_open_with_dialog; else menus_choose "$action"; fi
        menus_expect menuDialogState ".opened and (.busy | not) and .action == \"$action\"" "$preset $action opens"
        menus_expect "$probe" "any(.controls[]; .name == \"$first\" and .focused)" "$preset $action initial focus"
        for name in "${forward[@]}" "$first"; do
            key -k Tab >/dev/null
            menus_expect "$probe" "any(.controls[]; .name == \"$name\" and .focused and .visible)" "$preset $action Tab focuses $name"
        done
        for ((back = ${#forward[@]} - 1; back >= 0; back--)); do
            key -M shift -k Tab -m shift >/dev/null
            menus_expect "$probe" "any(.controls[]; .name == \"${forward[back]}\" and .focused and .visible)" "$preset $action Shift+Tab focuses ${forward[back]}"
        done
        key -M shift -k Tab -m shift >/dev/null
        menus_expect "$probe" "any(.controls[]; .name == \"$first\" and .focused and .visible)" "$preset $action Shift+Tab returns to $first"
        key -k Escape >/dev/null
        menus_expect menuDialogState '.opened | not' "$preset $action traversal dismisses without submitting"
        menus_equal "$preset $action restores listing focus" list "$(ipc focusView)"
    done
    menus_equal "$preset dialog traversal preserves the source" "$before" "$(stat -c '%d:%i:%u:%g:%a:%s:%Y' "$menu_dir/a.txt")"
}

menus_actions() {
    local preset="$1" directory="$menu_box/actions-$1" target open_count
    menus_guard "$directory"
    mkdir -p "$directory/destination" || fail "menus: cannot create action fixture"
    for target in source move rename trash; do
        menus_guard "$directory/$target.txt"
        printf '%s %s\n' "$preset" "$target" > "$directory/$target.txt"
    done
    menus_guard "$directory/destination/landing.txt"
    printf 'paste target\n' > "$directory/destination/landing.txt"
    menus_visit "$directory" 5

    menus_file_menu source.txt menu-letter
    menus_choose copy
    menus_message 'Copied 1 item, p pastes.' "$preset Copy captures source"
    menus_file_menu destination menu-key
    menus_choose open
    wait_path "$directory/destination"
    wait_listing 1
    menus_file_menu landing.txt key
    menus_guard "$directory/destination/source.txt"
    menus_choose paste
    wait_listing 2
    menus_same_file "$preset Copy/Paste preserves bytes and source" "$directory/source.txt" "$directory/destination/source.txt"

    menus_visit "$directory" 5
    menus_file_menu move.txt key
    menus_guard "$directory/move.txt"
    menus_choose cut
    menus_message 'Cut 1 item, p pastes.' "$preset Cut captures source"
    menus_file_menu destination
    menus_choose open pointer
    wait_path "$directory/destination"
    wait_listing 2
    menus_file_menu landing.txt
    menus_guard "$directory/move.txt"
    menus_guard "$directory/destination/move.txt"
    menus_choose paste pointer
    wait_listing 3
    [[ ! -e "$directory/move.txt" ]] || fail "menus: $preset Cut/Paste left its source"
    menus_equal "$preset Cut/Paste retains bytes at destination" "$preset move" "$(cat "$directory/destination/move.txt")"

    menus_visit "$directory" 4
    menus_file_menu source.txt
    menus_guard "$directory/source copy.txt"
    menus_choose duplicate pointer
    wait_listing 5
    menus_same_file "$preset Duplicate uses the first free copy name" "$directory/source.txt" "$directory/source copy.txt"
    menus_file_menu rename.txt menu-letter
    menus_choose rename
    menus_expect renameEditorLive '. == true' "$preset Rename opens inline editor"
    menus_guard "$directory/rename.txt"
    menus_guard "$directory/renamed.txt"
    key -M ctrl -k a -m ctrl renamed.txt -k Return >/dev/null
    wait_marker "$directory/renamed.txt" "menus: $preset Rename did not commit"
    menus_expect renameEditorLive '. == false' "$preset Rename commit closes editor"
    [[ ! -e "$directory/rename.txt" ]] || fail "menus: $preset Rename left the old name"
    menus_equal "$preset Rename preserves contents" "$preset rename" "$(cat "$directory/renamed.txt")"

    menus_file_menu trash.txt menu-key
    menus_guard "$directory/trash.txt"
    trash_guard_store "$menus_trashed"
    menus_choose trash
    menus_trashed=$((menus_trashed + 1))
    wait_listing 4
    menus_expect trashState ".count == $menus_trashed" "$preset Trash updates private count"
    trash_guard_store "$menus_trashed"
    [[ ! -e "$directory/trash.txt" ]] || fail "menus: $preset Trash left its source"
    menus_equal "$preset Trash reaches the real private GIO provider" "$directory/trash.txt" "$(tail -n 1 "$menu_box/gio-trash.log")"

    open_count=$(wc -l < "$menu_box/gio-open.log")
    menus_file_menu source.txt
    menus_choose open pointer
    menus_error 'No application on this system opened that file.' "$preset Open reports the real launcher refusal"
    menus_equal "$preset Open calls gio once" "$((open_count + 1))" "$(wc -l < "$menu_box/gio-open.log")"
    menus_equal "$preset Open retains the captured path" "$directory/source.txt" "$(tail -n 1 "$menu_box/gio-open.log")"
    menus_acknowledge
    menus_visit "$menu_dir" 4
}

menus_replace() {
    local original="$1" retained="$2" metadata inode
    menus_guard "$original"
    menus_guard "$retained"
    [[ -f "$original" && ! -L "$original" && ! -e "$retained" ]] || fail "menus: replacement requires a fresh retained path and a regular fixture"
    metadata=$(stat -c '%s|%y|%a' "$original")
    inode=$(stat -c '%d:%i' "$original")
    mv --no-clobber -- "$original" "$retained" || fail "menus: cannot retain source identity"
    [[ ! -e "$original" && -f "$retained" ]] || fail "menus: original identity was not retained"
    printf 'replacement\n' > "$original"
    chmod --reference="$retained" "$original" || fail "menus: cannot preserve replacement mode"
    touch -r "$retained" "$original" || fail "menus: cannot preserve replacement timestamp"
    menus_equal 'replacement keeps listed size, timestamp and mode' "$metadata" "$(stat -c '%s|%y|%a' "$original")"
    [[ "$(stat -c '%d:%i' "$original")" != "$inode" ]] || fail "menus: replacement reused the captured identity"
}

menus_stale_actions() {
    local action directory retained token open_count trash_count
    printf 'MENUS_SHARED_PROOF preset=default; all presets separately exercise native menu delivery and the same ContextMenu.chosen/PaneMenuActions.activate path.\n'
    for action in open cut copy duplicate trash rename; do
        directory="$menu_box/stale-$action"
        retained="$menu_box/retained-$action.txt"
        menus_guard "$directory"
        mkdir -p "$directory/destination" || fail "menus: cannot create stale fixture"
        menus_guard "$directory/target.txt"
        printf 'originalone\n' > "$directory/target.txt"
        menus_guard "$directory/sentinel.txt"
        printf 'clipboard sentinel\n' > "$directory/sentinel.txt"
        menus_guard "$directory/destination/landing.txt"
        printf 'paste target\n' > "$directory/destination/landing.txt"
        menus_visit "$directory" 3
        menus_file_menu sentinel.txt
        menus_choose copy
        menus_message 'Copied 1 item, p pastes.' "$action refusal starts with a known copy clipboard"
        menus_file_menu target.txt key
        menus_seek "$action"
        token=$(ipc menuState | jq -r .snapshotId)
        open_count=$(wc -l < "$menu_box/gio-open.log")
        trash_count=$(wc -l < "$menu_box/gio-trash.log")
        menus_replace "$directory/target.txt" "$retained"
        menus_expect menuState ".opened and .snapshotReady and .snapshotId == $token" "$action retains the originally opened snapshot"
        trash_guard_store "$menus_trashed"
        key -k Return >/dev/null
        menus_error 'Selected item changed' "$action refuses replacement at native menu activation"
        menus_expect menuState '.opened | not' "$action refusal closes the menu"
        menus_expect renameEditorLive '. == false' "$action refusal does not open Rename"
        menus_equal "$action refusal preserves replacement" replacement "$(cat "$directory/target.txt")"
        menus_equal "$action refusal preserves captured original" originalone "$(cat "$retained")"
        menus_equal "$action refusal preserves navigation" "$directory" "$(ipc path)"
        [[ ! -e "$directory/target copy.txt" ]] || fail "menus: $action refusal duplicated the replacement"
        menus_equal "$action refusal does not call the opener" "$open_count" "$(wc -l < "$menu_box/gio-open.log")"
        menus_equal "$action refusal does not call GIO Trash" "$trash_count" "$(wc -l < "$menu_box/gio-trash.log")"
        trash_guard_store "$menus_trashed"
        menus_acknowledge
        if [[ "$action" == copy || "$action" == cut ]]; then
            menus_visit "$directory/destination" 1
            menus_file_menu landing.txt
            menus_guard "$directory/sentinel.txt"
            menus_guard "$directory/destination/sentinel.txt"
            menus_choose paste
            wait_listing 2
            menus_same_file "$action refusal retains copy clipboard and intent" "$directory/sentinel.txt" "$directory/destination/sentinel.txt"
            [[ ! -e "$directory/destination/target.txt" ]] || fail "menus: $action refusal replaced the clipboard"
        fi
    done
    menus_visit "$menu_dir" 4
}

menus_stale_rename_commit() {
    local directory="$menu_box/stale-rename-commit" retained="$menu_box/retained-rename-commit.txt"
    menus_guard "$directory"
    mkdir "$directory" || fail "menus: cannot create Rename commit fixture"
    menus_guard "$directory/target.txt"
    printf 'originalone\n' > "$directory/target.txt"
    menus_visit "$directory" 1
    menus_file_menu target.txt
    menus_choose rename
    menus_expect renameEditorLive '. == true' 'Rename captures identity before editing'
    menus_replace "$directory/target.txt" "$retained"
    menus_guard "$directory/renamed.txt"
    key -M ctrl -k a -m ctrl renamed.txt -k Return >/dev/null
    menus_expect renameState '.focused and (.pending | not) and .text == "renamed.txt" and (.error | contains("Selected item changed"))' 'Rename commit retains the draft after replaced-source refusal'
    [[ ! -e "$directory/renamed.txt" ]] || fail "menus: stale Rename committed the replacement"
    menus_equal 'Rename refusal preserves replacement' replacement "$(cat "$directory/target.txt")"
    menus_equal 'Rename refusal preserves captured original' originalone "$(cat "$retained")"
    key -k Escape >/dev/null
    menus_expect renameEditorLive '. == false' 'Escape dismisses retained Rename refusal'
    menus_file_menu target.txt menu-key
    menus_choose rename
    menus_expect renameEditorLive '. == true' 'a fresh Rename recaptures the replacement'
    menus_guard "$directory/target.txt"
    menus_guard "$directory/renamed.txt"
    key -M ctrl -k a -m ctrl renamed.txt -k Return >/dev/null
    wait_marker "$directory/renamed.txt" 'menus: fresh Rename did not recover'
    menus_expect renameEditorLive '. == false' 'fresh Rename recovery closes the editor'
    [[ ! -e "$directory/target.txt" ]] || fail 'menus: fresh Rename left the old path'
    menus_equal 'fresh Rename recovery preserves replacement contents' replacement "$(cat "$directory/renamed.txt")"
    menus_visit "$menu_dir" 4
}

case_menuscoverage() (
    local menu_box="$fixture_root/menus" menu_dir="$fixture_root/menus/list" menus_checks=0
    local trash_box="$fixture_root/menus" trash_checks=0 menus_trashed=0
    local trash_parent_bus_id="" trash_private_bus_id="" trash_bus_address="" trash_bus_pid="" trash_provider_pid=""
    local preset before target token
    sandbox_scratch "$menu_box"
    : > "$menu_box/.flea-test-sandbox"
    for target in list state config cache data bin data/applications; do
        menus_guard "$menu_box/$target"
        mkdir -p "$menu_box/$target" || fail "menus: cannot create $target fixture"
    done
    menus_guard "$menu_dir/a.txt"
    printf 'alpha\n' > "$menu_dir/a.txt"
    printf 'beta\n' > "$menu_dir/b.txt"
    mkdir "$menu_dir/folder"
    printf 'child\n' > "$menu_dir/folder/child.txt"
    ln -s a.txt "$menu_dir/link"
    export XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config" XDG_DATA_HOME="$menu_box/data" XDG_CACHE_HOME="$menu_box/cache"
    [[ "$(realpath -e "$(command -v gio)")" == /usr/bin/gio ]] || fail "menus: GIO already resolves to a stub"
    trash_start_bus
    menus_launcher_fixture
    for preset in default vim mac windows; do
        printf 'MENUS_PRESET=%s\n' "$preset"
        "$flea_bin" --ui-state "{\"view\":\"list\",\"keys\":\"$preset\",\"menu\":{\"hidden\":[]}}" >/dev/null || fail "menus: fixture settings failed"
        launch "$menu_dir"
        wait_listing 4
        trash_guard_store "$menus_trashed"
        menus_file_menu a.txt menu-key
        menus_expect menuState '.entries as $entries | ["open","cut","copy","paste","duplicate","rename","trash","deletePermanently","openWith","openTerminal","moveTo","copyTo","properties","permissions","copypath","toggleHidden"] | all(.[]; . as $action | any($entries[]; .action == $action))' "full applicable plain-file inventory"
        menus_expect menuState 'any(.entries[]; .action == "paste" and .disabled)' "Paste remains visible with empty clipboard"
        before=$(ipc listContentY)
        menus_seek properties
        menus_shot "$preset-file-menu"
        menus_choose properties pointer
        menus_expect menuDialogState '.opened and .facts.ok and .facts.kind == "File"' "Properties consumes captured file identity"
        [[ "$(ipc listContentY)" == "$before" ]] || fail "menus: menu traversal scrolled the covered listing"
        key -k Tab >/dev/null
        menus_expect menuDialogState 'any(.controls[]; .name == "Cancel" and .focused)' "Properties contains Tab focus"
        key -k Escape >/dev/null
        menus_confirmation a.txt "$preset"
        menus_permissions a.txt "$preset"
        menus_dialog_keys "$preset"
        menus_file_menu link
        menus_expect menuState 'any(.entries[]; .action == "permissions" and .disabled and .hint == "Symlink target not changed")' "symlink permissions stays disabled"
        key -k Escape >/dev/null
        local first_index second_index
        first_index=$(row_index_of a.txt)
        second_index=$(row_index_of b.txt)
        click_row "$first_index" left
        menus_expect dualState ".panes[.focused].selected == [$first_index]" "plain click selects a.txt alone"
        click_row "$second_index" left --mods ctrl
        menus_expect selectionCount '. == 2' "Ctrl-click creates two selected items before menu eligibility"
        menus_expect dualState ".panes[.focused].selected == [$first_index,$second_index]" "selected identities are a.txt and b.txt"
        click_row "$(row_index_of a.txt)" right
        menus_expect menuState '.entries as $entries | ["rename","duplicate","openWith","properties","permissions"] | all(.[]; . as $action | any($entries[]; .action == $action and .disabled))' "multi-selection eligibility"
        key -k Escape >/dev/null
        key -k Escape >/dev/null
        menus_actions "$preset"
        kill_flea
    done
    "$flea_bin" --ui-state '{"keys":"default","menu":{"hidden":[]}}' >/dev/null || fail "menus: default fixture preset failed"
    launch "$menu_dir"
    wait_listing 4
    menus_open_with
    menus_stale_actions
    menus_stale_rename_commit
    menus_file_menu folder
    menus_choose deletePermanently
    menus_expect menuDialogState '.confirmation.opened and .confirmation.count == 1' "directory deletion snapshots one root"
    token=$(ipc menuDialogState | jq -r .confirmation.token)
    menus_guard "$menu_dir/folder/arrived.txt"
    printf 'arrived after confirmation\n' > "$menu_dir/folder/arrived.txt"
    menus_expect menuDialogState ".confirmation.opened and .confirmation.token != $token and (.confirmation.destructiveFocus | not)" "nested arrival replaces stale confirmation"
    menus_guard "$menu_dir/folder"
    key -k Tab -k Return >/dev/null
    menus_expect menuDialogState '(.opened | not) and (.committing | not)' "confirmed permanent deletion completes"
    [[ ! -e "$menu_dir/folder" ]] || fail "menus: freshly confirmed directory was not deleted"
    [[ "$(cat "$menu_dir/a.txt")" == alpha && "$(cat "$menu_dir/b.txt")" == beta ]] || fail "menus: deletion widened outside its confirmation"
    menus_shot deletion-completed
    printf 'MENUS_NATIVE_CHECKS=%s\n' "$menus_checks"
    printf 'MENUS_UNVERIFIED new-file/new-folder, archive/convert, provider states, hidden-row persistence, work-area/scale matrix, partial deletion failure, directory Permissions scope, concurrent windows\n'
    trash_cleanup 0
)
