#!/usr/bin/env bash
# Sourced by ui.sh; Permissions changes only marked fixtures through native controls.

permissions_guard() {
    local path="$1" canonical
    [[ -n "$path" && "$path" == /* && -f "$permissions_box/.flea-test-sandbox" ]] \
        || fail "permissions: mutation needs an absolute path and owned marker"
    canonical=$(realpath -m -- "$path") || fail "permissions: mutation path did not resolve"
    [[ "$canonical" == "$permissions_box/"* && "$canonical" != "$permissions_box" ]] \
        || fail "permissions: mutation escaped the owned fixture"
}

permissions_wait() {
    local expression="$1" label="${2:-$1}" state end=$((SECONDS + 20))
    while (( SECONDS < end )); do
        state=$(ipc permissionsState) || fail "permissions: read-only state unavailable"
        if jq -e "$expression" <<< "$state" >/dev/null; then
            permissions_checks=$((permissions_checks + 1))
            printf 'PERMISSIONS_PASS %s expected=%q observed=%s\n' "$label" "$expression" "$state"
            return
        fi
        sleep 0.05
    done
    fail "permissions: $label; last native state: $state"
}

permissions_expect() {
    local reader="$1" expected="$2" observed end=$((SECONDS + 20))
    while (( SECONDS < end )); do
        observed=$(ipc "$reader") || fail "permissions: $reader unavailable"
        if [[ "$observed" == "$expected" ]]; then
            permissions_checks=$((permissions_checks + 1))
            printf 'PERMISSIONS_PASS %s expected=%q observed=%q\n' "$reader" "$expected" "$observed"
            return
        fi
        sleep 0.05
    done
    fail "permissions: $reader expected $expected, observed $observed"
}

permissions_point() {
    local centre="$1" button="${2:-left}" cx cy wx wy ww wh
    read -r cx cy <<< "$centre"
    [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "permissions: native control has no centre"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    (( cx < ww && cy < wh )) || fail "permissions: control is outside the actual viewport"
    assert_focus
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" "$button" >/dev/null \
        || fail "permissions: native pointer activation failed"
}

permissions_control() {
    local name="$1" centre
    centre=$(ipc permissionsState | jq -er --arg name "$name" '.controls[] | select(.name == $name and .visible) | .centre') \
        || fail "permissions: no visible $name control"
    permissions_point "$centre"
}

permissions_viewport() {
    local target_width="${1:-1100}" target_height="${2:-800}"
    local client address result wx wy width height end=$((SECONDS + 20))
    client=$(hyprctl clients -j | jq -ec --argjson pid "$(flea_pid)" '.[] | select(.pid == $pid)') \
        || fail "permissions: owned window unavailable"
    address=$(jq -er '.address' <<< "$client") || fail "permissions: owned window has no address"
    [[ "$address" =~ ^0x[0-9a-fA-F]+$ ]] || fail "permissions: invalid owned window address"
    if ! jq -e '.floating' <<< "$client" >/dev/null; then
        omarchy-drive window float "$address" >/dev/null || fail "permissions: owned window could not float"
    fi
    result=$(hyprctl dispatch "hl.dsp.window.resize({ x = $target_width, y = $target_height, exact = true, window = \"address:$address\" })") \
        || fail "permissions: compositor resize failed"
    [[ "$result" == ok* ]] || fail "permissions: compositor refused resize: $result"
    omarchy-drive window center "$address" || fail "permissions: owned window could not center"
    while (( SECONDS < end )); do
        read -r wx wy width height < <(window_box) || fail "native window coordinates unavailable"
        [[ "$width" == "$target_width" && "$height" == "$target_height" ]] && return
        sleep 0.05
    done
    fail "permissions: viewport did not reach ${target_width}x${target_height}"
}

permissions_setup() {
    local root
    sandbox_require "$fixture_root"
    permissions_box=$(mktemp -d "$fixture_root/permissions.XXXXXXXX") || fail "permissions: fixture creation failed"
    printf 'native Permissions fixture\n' > "$permissions_box/.flea-test-sandbox"
    [[ "$permissions_box" == "$(realpath -e "$permissions_box")" ]] || fail "permissions: fixture is not canonical"
    permissions_listing="$permissions_box/listing"
    for root in "$permissions_listing" "$permissions_box/data" "$permissions_box/config" "$permissions_box/cache"; do
        permissions_guard "$root"
        mkdir "$root" || fail "permissions: fixture directory creation failed"
    done
    export XDG_DATA_HOME="$permissions_box/data" XDG_CONFIG_HOME="$permissions_box/config" XDG_CACHE_HOME="$permissions_box/cache"
    seed_ui_state "$permissions_box/state" '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"off"},"menu":{"hidden":[]}}'
    printf 'permission fixture content\n' > "$permissions_listing/notes.md"
    printf 'other fixture content\n' > "$permissions_listing/other.txt"
    printf 'special bit fixture\n' > "$permissions_listing/special.txt"
    printf 'unreadable owned fixture\n' > "$permissions_listing/unreadable.txt"
    mkdir -p "$permissions_listing/site/nested" || fail "permissions: directory fixture failed"
    printf 'child scope canary\n' > "$permissions_listing/site/child.txt"
    printf 'nested scope canary\n' > "$permissions_listing/site/nested/leaf.txt"
    ln -s notes.md "$permissions_listing/link" || fail "permissions: symlink fixture failed"
    permissions_guard "$permissions_listing/notes.md"
    chmod 0644 "$permissions_listing/notes.md" "$permissions_listing/other.txt" "$permissions_listing/special.txt" \
        || fail "permissions: ordinary fixture modes failed"
    chmod 0000 "$permissions_listing/unreadable.txt" || fail "permissions: unreadable fixture mode failed"
    chmod 0755 "$permissions_listing/site" || fail "permissions: directory fixture mode failed"
    chmod 0711 "$permissions_listing/site/nested" || fail "permissions: nested directory fixture mode failed"
    chmod 0640 "$permissions_listing/site/child.txt" || fail "permissions: child fixture mode failed"
    chmod 0600 "$permissions_listing/site/nested/leaf.txt" || fail "permissions: nested fixture mode failed"
    launch "$permissions_listing"
    wait_listing 6
    permissions_viewport
}

permissions_open() {
    local name="$1" entry="${2:-pointer}" row target
    wait_path "$permissions_listing"
    row=$(row_index_of "$name")
    if [[ "$entry" == pointer ]]; then click_row "$row" right
    else click_row "$row" left; key m >/dev/null; fi
    permissions_expect contextMenuVisible true
    ipc contextMenuModel | jq -e 'any(.[]; .action == "permissions" and .disabled != true)' >/dev/null \
        || fail "permissions: single-item menu entry is missing or disabled"
    target=$(menu_row_index Permissions) || fail "permissions: no native Permissions row"
    if [[ "$entry" == pointer ]]; then permissions_point "$(ipc contextMenuRowCentre "$target")"
    else menu_seek Permissions; key -k Return >/dev/null; fi
    permissions_wait '.opened and (.busy == false)' "native $entry Permissions entry for $name"
    target=$(jq -cn --arg path "$permissions_listing/$name" '$path')
    permissions_wait ".path == $target" 'dialog reviews the selected fixture path'
}

permissions_mode() {
    local mode="$1" value
    [[ "$mode" =~ ^0?[0-7]{3}$ ]] || fail "permissions: invalid test mode"
    value=$((8#$mode))
    permissions_wait ".mode == \"$mode\" and [.controls[] | select(.bit != null) | .bit] == [256,128,64,32,16,8,4,2,1] and all(.controls[] | select(.bit != null); .checked == ((($value / .bit | floor) % 2) == 1))" 'octal text and all nine checkboxes agree'
}

permissions_octal() {
    local text="$1" quoted
    permissions_control Octal
    permissions_wait 'any(.controls[]; .name == "Octal" and .focused and .enabled)' 'octal field owns keyboard'
    key -M ctrl -k a -m ctrl -k BackSpace >/dev/null
    [[ -z "$text" ]] || key -- "$text" >/dev/null
    quoted=$(jq -cn --arg text "$text" '$text')
    permissions_wait ".mode == $quoted" 'typed mode remains visible'
}

permissions_apply() {
    local path="$1" mode="$2" entry="${3:-pointer}"
    permissions_guard "$path"
    permissions_wait '.opened and .editable and (.busy == false) and any(.controls[]; .name == "Apply" and .enabled)' 'Apply is eligible on the held fixture'
    [[ "$(ipc permissionsState | jq -r '.path')" == "$path" ]] || fail "permissions: Apply targets a different item"
    if [[ "$entry" == pointer ]]; then permissions_control Apply
    else
        permissions_wait 'any(.controls[]; .name == "Apply" and .focused)' 'Apply owns keyboard'
        if [[ "$entry" == space ]]; then key -k space >/dev/null
        else key -k Return >/dev/null; fi
    fi
    permissions_wait '(.opened == false)' 'successful Apply closes Permissions'
    [[ "$(stat -c '%a' "$path")" == "$mode" ]] || fail "permissions: applied filesystem mode differs from $mode"
    [[ "$(ipc focusView)" == list ]] || fail "permissions: Apply did not restore listing focus"
}

permissions_file() {
    local bit name value wanted invalid before grid row_count=0
    permissions_wait '.facts.ok and (.facts.directory == false) and .editable and .mode == "0644" and any(.controls[]; .name == "Cancel" and .focused)' 'owned file opens with Cancel focused'
    permissions_wait '(.controls | length) == 13 and ([.controls[].name] | unique | length) == 13' 'file exposes exactly thirteen distinct controls'
    permissions_wait '.displayedSummary | contains("Requested mode 0644") and contains("Scope this item only") and contains("ownership unchanged")' 'file summary states exact effect'
    permissions_mode 0644
    before=$(stat -c '%u:%g:%a' "$permissions_listing/notes.md")
    grid=$(ipc permissionsState | jq -r '.controls[] | select(.bit != null) | [.name, .bit] | @tsv') \
        || fail "permissions: checkbox inventory unavailable"
    # Sample control row: "Owner read<TAB>256".
    while IFS=$'\t' read -r name bit; do
        permissions_control "$name"
        value=$((8#0644 ^ bit))
        printf -v wanted '%04o' "$value"
        permissions_mode "$wanted"
        key -k space >/dev/null
        permissions_mode 0644
        row_count=$((row_count + 1))
    done <<< "$grid"
    [[ "$row_count" == 9 ]] || fail "permissions: did not activate all nine checkboxes"
    [[ "$(stat -c '%u:%g:%a' "$permissions_listing/notes.md")" == "$before" ]] || fail "permissions: checkbox editing wrote before Apply"
    for invalid in '' 64 888 4755 ' 644' '0644 ' 00000 -1; do
        permissions_octal "$invalid"
        permissions_wait '.editable and all(.controls[] | select(.name == "Apply"); .enabled == false) and (.displayedError | contains("Enter three octal digits"))' 'invalid mode stays visible and Apply is disabled'
        permissions_control Apply
        permissions_wait '.opened and (.busy == false)' 'disabled Apply does not commit'
        [[ "$(stat -c '%u:%g:%a' "$permissions_listing/notes.md")" == "$before" ]] || fail "permissions: invalid mode changed the file"
    done
    permissions_octal 600
    permissions_mode 600
    key -k Return >/dev/null
    permissions_wait '.opened and any(.controls[]; .name == "Apply" and .focused)' 'Return from octal focuses Apply without committing'
    [[ "$(stat -c '%u:%g:%a' "$permissions_listing/notes.md")" == "$before" ]] || fail "permissions: first octal Return committed early"
    permissions_apply "$permissions_listing/notes.md" 600 keyboard
    [[ "$(cat "$permissions_listing/notes.md")" == 'permission fixture content' ]] || fail "permissions: mode write changed contents"
    [[ "$(stat -c '%u:%g' "$permissions_listing/notes.md")" == "${before%:*}" ]] || fail "permissions: mode write changed ownership"
    permissions_open notes.md keyboard
    permissions_mode 0600
    shot "permissions-$permissions_group-file-applied"
    permissions_control Cancel
    permissions_wait '(.opened == false)'
}

permissions_directory() {
    local before child nested leaf
    before=$(stat -c '%u:%g' "$permissions_listing/site")
    child=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/site/child.txt")
    nested=$(stat -c '%d:%i:%u:%g:%a' "$permissions_listing/site/nested")
    leaf=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/site/nested/leaf.txt")
    permissions_open site
    shot "permissions-$permissions_group-directory-baseline"
    permissions_wait '.facts.directory and .editable and ([.controls[] | select(.bit == 64 or .bit == 8 or .bit == 1) | .name] == ["Owner enter","Group enter","Everyone enter"])' 'directory traversal bits are labelled enter'
    permissions_wait '.displayedSummary | contains("Scope this directory only") and contains("enclosed items unchanged") and contains("ownership unchanged")' 'directory summary excludes descendants and ownership'
    permissions_mode 0755
    permissions_octal 0700
    permissions_apply "$permissions_listing/site" 700
    [[ "$(stat -c '%u:%g' "$permissions_listing/site")" == "$before" ]] || fail "permissions: directory ownership changed"
    [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/site/child.txt")" == "$child" \
        && "$(stat -c '%d:%i:%u:%g:%a' "$permissions_listing/site/nested")" == "$nested" \
        && "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/site/nested/leaf.txt")" == "$leaf" \
        && "$(cat "$permissions_listing/site/child.txt")" == 'child scope canary' \
        && "$(cat "$permissions_listing/site/nested/leaf.txt")" == 'nested scope canary' ]] \
        || fail "permissions: directory Apply changed an enclosed item"
    permissions_open unreadable.txt keyboard
    permissions_mode 0000
    permissions_wait '.editable and .facts.reason == ""' 'owned mode 0000 remains repairable'
    permissions_octal 0600
    permissions_apply "$permissions_listing/unreadable.txt" 600
    [[ "$(cat "$permissions_listing/unreadable.txt")" == 'unreadable owned fixture' ]] || fail "permissions: mode 0000 repair changed contents"
}

permissions_readonly_controls() {
    local reason="$1" mode="$2" quoted before name grid path count=0
    path=$(ipc permissionsState | jq -r '.path')
    permissions_guard "$path"
    quoted=$(jq -cn --arg reason "$reason" '$reason')
    permissions_wait ".facts.ok and (.editable == false) and .mode == \"$mode\" and .displayedError == $quoted" 'read-only reason names the actual refusal'
    before=$(stat -c '%d:%i:%u:%g:%a:%s' "$path")
    permissions_wait 'all(.controls[] | select(.bit != null or .name == "Octal" or .name == "Apply"); .enabled == false)' 'all mutation controls are disabled'
    grid=$(ipc permissionsState | jq -r '.controls[] | select(.bit != null) | .name')
    while IFS= read -r name; do
        permissions_control "$name"
        permissions_wait ".opened and .mode == \"$mode\"" 'disabled checkbox cannot change the draft'
        count=$((count + 1))
    done <<< "$grid"
    [[ "$count" == 9 ]] || fail "permissions: read-only grid does not contain all nine controls"
    permissions_control Octal
    key -M ctrl -k a -m ctrl >/dev/null
    key 0600 >/dev/null
    permissions_wait ".opened and .mode == \"$mode\"" 'read-only octal rejects editing'
    permissions_control Apply
    permissions_wait '.opened and (.busy == false)' 'disabled Apply leaves dialog open'
    [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$path")" == "$before" ]] \
        || fail "permissions: read-only controls changed the item"
    permissions_wait ".displayedSummary | contains(\"Current mode $mode\") and contains(\"No changes available\")" 'read-only summary reports the retained current mode'
    permissions_wait 'any(.controls[]; .name == "Cancel" and .focused)' 'disabled controls do not steal initial Cancel focus'
    key -k Tab >/dev/null
    permissions_wait 'any(.controls[]; .name == "Close" and .focused)' 'read-only Tab reaches Close'
    key -k Tab >/dev/null
    permissions_wait 'any(.controls[]; .name == "Cancel" and .focused)' 'read-only Tab skips every disabled mutation control'
    key -M shift -k Tab -m shift >/dev/null
    permissions_wait 'any(.controls[]; .name == "Close" and .focused)' 'read-only reverse traversal reaches Close'
    key -M shift -k Tab -m shift >/dev/null
    permissions_wait 'any(.controls[]; .name == "Cancel" and .focused)' 'read-only reverse traversal wraps to Cancel'
    permissions_control Cancel
    permissions_wait '(.opened == false)'
}

permissions_readonly() {
    local bit label
    for bit in 4 2 1; do
        case "$bit" in 4) label=setuid ;; 2) label=setgid ;; 1) label=sticky ;; esac
        permissions_guard "$permissions_listing/special.txt"
        chmod "${bit}644" "$permissions_listing/special.txt" || fail "permissions: could not stage $label fixture"
        permissions_open special.txt
        shot "permissions-$permissions_group-readonly-$label"
        permissions_readonly_controls "Read-only: $label bit is present." "${bit}644"
    done
    permissions_guard "$permissions_listing/special.txt"
    chmod 0644 "$permissions_listing/special.txt" || fail "permissions: could not restore ordinary fixture mode"
}

permissions_stale() {
    local before replacement target
    permissions_open notes.md
    permissions_octal 0600
    before=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")
    permissions_guard "$permissions_listing/notes.md"
    permissions_guard "$permissions_box/retired-notes"
    mv "$permissions_listing/notes.md" "$permissions_box/retired-notes" || fail "permissions: could not retire reviewed fixture"
    printf 'replacement must remain untouched\n' > "$permissions_listing/notes.md"
    chmod 0640 "$permissions_listing/notes.md" || fail "permissions: replacement mode setup failed"
    replacement=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")
    permissions_control Apply
    permissions_wait '.opened and (.busy == false) and .mode == "0600" and .displayedError == "Selected item changed; reopen Permissions."' 'replacement identity refuses the write and retains the draft'
    shot "permissions-$permissions_group-stale-replacement"
    [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_box/retired-notes")" == "$before" \
        && "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")" == "$replacement" \
        && "$(cat "$permissions_box/retired-notes")" == 'permission fixture content' \
        && "$(cat "$permissions_listing/notes.md")" == 'replacement must remain untouched' ]] \
        || fail "permissions: stale Apply modified the old or replacement inode"
    permissions_control Cancel
    permissions_wait '(.opened == false)'
    permissions_open notes.md keyboard
    permissions_mode 0640
    permissions_octal 0600
    permissions_apply "$permissions_listing/notes.md" 600

    permissions_open other.txt
    permissions_octal 0600
    before=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/other.txt")
    permissions_guard "$permissions_listing/other.txt"
    permissions_guard "$permissions_box/retired-other"
    mv "$permissions_listing/other.txt" "$permissions_box/retired-other" || fail "permissions: could not move reviewed fixture"
    permissions_control Apply
    permissions_wait '.opened and (.busy == false) and .mode == "0600" and .displayedError == "Selected item moved or disappeared; reopen Permissions."' 'missing pathname refuses the write and retains the draft'
    [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_box/retired-other")" == "$before" ]] || fail "permissions: missing-path Apply changed the held inode"
    permissions_guard "$permissions_box/retired-other"
    permissions_guard "$permissions_listing/other.txt"
    mv "$permissions_box/retired-other" "$permissions_listing/other.txt" || fail "permissions: could not restore reviewed inode"
    permissions_apply "$permissions_listing/other.txt" 600
    [[ "$(cat "$permissions_listing/other.txt")" == 'other fixture content' ]] || fail "permissions: recovered Apply changed contents"

    permissions_open special.txt
    permissions_octal 0600
    before=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/special.txt")
    target=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")
    permissions_guard "$permissions_listing/special.txt"
    permissions_guard "$permissions_box/retired-special"
    mv "$permissions_listing/special.txt" "$permissions_box/retired-special" || fail "permissions: could not retire symlink-race fixture"
    ln -s notes.md "$permissions_listing/special.txt" || fail "permissions: could not stage symlink replacement"
    permissions_control Apply
    permissions_wait '.opened and (.busy == false) and .mode == "0600" and .displayedError == "Selected item changed; reopen Permissions."' 'symlink replacement cannot redirect the mode write'
    [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_box/retired-special")" == "$before" \
        && "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")" == "$target" \
        && -L "$permissions_listing/special.txt" ]] || fail "permissions: symlink-race Apply changed a protected inode"
    permissions_control Cancel
    permissions_wait '(.opened == false)'
}

permissions_keys() {
    local preset name quoted before wanted current row focus
    local -a cycle=(Apply Close 'Owner read' 'Owner write' 'Owner execute' 'Group read' 'Group write' 'Group execute' 'Everyone read' 'Everyone write' 'Everyone execute' Octal Cancel)
    local -a reverse=(Octal 'Everyone execute' 'Everyone write' 'Everyone read' 'Group execute' 'Group write' 'Group read' 'Owner execute' 'Owner write' 'Owner read' Close Apply Cancel)
    for preset in default vim mac windows; do
        kill_flea
        seed_ui_state "$permissions_box/state" "{\"view\":\"list\",\"keys\":\"$preset\",\"preview\":{\"column\":false,\"thumbnails\":\"off\"},\"menu\":{\"hidden\":[]}}"
        launch "$permissions_listing"
        wait_listing 6
        permissions_viewport
        permissions_expect keymapPreset "$preset"
        permissions_open notes.md keyboard
        permissions_wait 'any(.controls[]; .name == "Cancel" and .focused)' "$preset Cancel receives initial focus"
        permissions_wait '(.controls | length) == 13 and ([.controls[].name] | unique | length) == 13' "$preset has thirteen distinct native controls"
        before=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")
        for name in "${cycle[@]}"; do
            key -k Tab >/dev/null
            quoted=$(jq -cn --arg name "$name" '$name')
            permissions_wait "any(.controls[]; .name == $quoted and .focused)" "$preset Tab focuses $name"
        done
        for name in "${reverse[@]}"; do
            key -M shift -k Tab -m shift >/dev/null
            quoted=$(jq -cn --arg name "$name" '$name')
            permissions_wait "any(.controls[]; .name == $quoted and .focused)" "$preset Shift+Tab focuses $name"
        done
        key -k Return >/dev/null
        permissions_wait '(.opened == false)' "$preset Return on Cancel dismisses"
        permissions_expect focusView list
        [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")" == "$before" ]] || fail "permissions: $preset focus traversal changed the file"
        permissions_open notes.md keyboard
        current=$(ipc permissionsState | jq -r '.mode')
        permissions_control 'Owner execute'
        printf -v wanted '%04o' "$((8#$current ^ 64))"
        permissions_mode "$wanted"
        key -k space >/dev/null
        permissions_mode "$current"
        if [[ "$current" == 0600 ]]; then wanted=0640; else wanted=0600; fi
        permissions_octal "$wanted"
        key -k Return >/dev/null
        permissions_wait 'any(.controls[]; .name == "Apply" and .focused)' "$preset Octal Return focuses Apply"
        shot "permissions-$permissions_group-$preset-keyboard"
        permissions_apply "$permissions_listing/notes.md" "${wanted#0}" space
        for name in Escape Close Cancel CloseReturn CloseSpace CancelSpace; do
            permissions_open notes.md keyboard
            row=$(ipc cursor)
            permissions_octal 0777
            case "$name" in
                Escape) key -k Escape >/dev/null ;;
                Close|Cancel) permissions_control "$name" ;;
                CloseReturn|CloseSpace)
                    for focus in Cancel Apply Close; do
                        key -k Tab >/dev/null
                        permissions_wait "any(.controls[]; .name == \"$focus\" and .focused)" "$preset dismissal focuses $focus"
                    done
                    if [[ "$name" == CloseReturn ]]; then key -k Return >/dev/null
                    else key -k space >/dev/null; fi
                    ;;
                CancelSpace)
                    key -k Tab >/dev/null
                    permissions_wait 'any(.controls[]; .name == "Cancel" and .focused)' "$preset Space dismissal focuses Cancel"
                    key -k space >/dev/null
                    ;;
            esac
            permissions_wait '(.opened == false)' "$preset $name discards the draft"
            permissions_expect focusView list
            [[ "$(stat -c '%a' "$permissions_listing/notes.md")" == "${wanted#0}" ]] || fail "permissions: $preset $name committed a draft"
            key -k Down >/dev/null
            permissions_expect cursor "$((row + 1))"
        done
    done
    kill_flea
    seed_ui_state "$permissions_box/state" '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"off"},"menu":{"hidden":[]}}'
    launch "$permissions_listing"
    wait_listing 6
    permissions_viewport
}

permissions_eligibility() {
    local first second index before
    settings_open_key
    permissions_expect settingsOpen true
    settings_section menus
    settings_click_control permissions
    settings_wait_value '.menu.hidden | index("permissions") != null'
    key -k Escape >/dev/null
    permissions_expect settingsOpen false
    first=$(row_index_of notes.md)
    click_row "$first" right
    permissions_expect contextMenuVisible true
    ipc contextMenuModel | jq -e 'all(.[]; .action != "permissions")' >/dev/null || fail "permissions: hidden preference left menu entry visible"
    key -k Escape >/dev/null
    launch "$permissions_listing"
    wait_listing 6
    permissions_viewport
    click_row "$(row_index_of notes.md)" right
    permissions_expect contextMenuVisible true
    ipc contextMenuModel | jq -e 'all(.[]; .action != "permissions")' >/dev/null || fail "permissions: restart lost hidden-menu preference"
    key -k Escape >/dev/null
    settings_open_key
    permissions_expect settingsOpen true
    settings_section menus
    settings_click_control permissions
    settings_wait_value '.menu.hidden | index("permissions") == null'
    key -k Escape >/dev/null
    permissions_expect settingsOpen false
    permissions_open notes.md keyboard
    key -k Escape >/dev/null
    permissions_wait '(.opened == false)' 'restored menu preference reaches native dialog'

    first=$(row_index_of notes.md)
    second=$(row_index_of other.txt)
    click_row "$first" left
    permissions_expect selectedIndices "$first"
    permissions_expect selectionCount 1
    click_row "$second" left --mods ctrl
    permissions_expect selectionCount 2
    click_row "$first" right
    permissions_expect contextMenuVisible true
    ipc contextMenuModel | jq -e 'any(.[]; .action == "permissions" and .disabled and .hint == "Unavailable")' >/dev/null \
        || fail "permissions: multi-selection does not show the specified disabled entry"
    shot "permissions-$permissions_group-multiselection"
    index=$(menu_row_index Permissions)
    permissions_point "$(ipc contextMenuRowCentre "$index")"
    permissions_wait '(.opened == false)' 'multi-selection pointer activation cannot open Permissions'
    permissions_expect selectionCount 2
    key -k Escape >/dev/null
    before=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")
    click_row "$(row_index_of link)" right
    permissions_expect contextMenuVisible true
    ipc contextMenuModel | jq -e 'any(.[]; .action == "permissions" and .disabled and .hint == "Symlink target not changed")' >/dev/null \
        || fail "permissions: symlink entry lacks its specific refusal"
    shot "permissions-$permissions_group-symlink"
    index=$(menu_row_index Permissions)
    permissions_point "$(ipc contextMenuRowCentre "$index")"
    permissions_wait '(.opened == false)' 'symlink pointer activation cannot open Permissions'
    [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")" == "$before" ]] || fail "permissions: symlink eligibility changed target"
    key -k Escape >/dev/null
}

permissions_overlay() {
    local i row parked before wx wy ww wh cx cy rx ry rw rh left top width height name
    for i in $(seq -w 1 80); do
        permissions_guard "$permissions_listing/z$i.txt"
        printf 'overlay fixture\n' > "$permissions_listing/z$i.txt"
    done
    wait_listing 86
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    read -r cx cy <<< "$(ipc rowCentre 5)"
    omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null || fail "permissions: wheel control pointer failed"
    omarchy-drive scroll down 3 >/dev/null || fail "permissions: wheel control failed"
    settle
    [[ "$(ipc listContentY)" != 0 ]] || fail "permissions: unoccluded list did not respond to wheel control"
    key -k Home >/dev/null
    permissions_expect listContentY 0
    permissions_open notes.md
    parked=$(ipc cursor)
    before=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")
    read -r rx ry rw rh <<< "$(ipc permissionsState | jq -r '.rect')"
    row=$(row_index_of other.txt)
    read -r cx cy <<< "$(ipc rowCentre "$row")"
    left=$(ipc rowLeft "$row")
    [[ "$left" =~ ^[0-9]+$ ]] || fail "permissions: overlay test has no real underlying row"
    cx=$((left + 1))
    (( cx < rx || cx >= rx + rw || cy < ry || cy >= ry + rh )) || fail "permissions: outside-card target overlaps the card"
    omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null || fail "permissions: overlay hover failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1 \
        || fail "permissions: actual hover motion failed"
    permissions_expect cursor "$parked"
    omarchy-drive scroll down 3 >/dev/null || fail "permissions: covered wheel failed"
    permissions_expect listContentY 0
    permissions_wait '.opened' 'outside-card wheel leaves Permissions open'
    for name in j k f r m; do
        key "$name" >/dev/null
        permissions_wait '.opened and any(.controls[]; .name == "Cancel" and .focused)' "listing key $name remains contained"
        permissions_expect cursor "$parked"
        permissions_expect contextMenuVisible false
        permissions_expect path "$permissions_listing"
    done
    permissions_point "$((rx + 1)) $((ry + rh / 2))"
    permissions_wait '.opened' 'card padding absorbs its pointer press'
    shot "permissions-$permissions_group-overlay"
    permissions_point "$cx $cy" right
    permissions_wait '(.opened == false)' 'outside right-click dismisses Permissions'
    permissions_expect cursor "$parked"
    permissions_expect contextMenuVisible false
    permissions_open notes.md
    permissions_point "$cx $cy"
    permissions_wait '(.opened == false)' 'outside left-click dismisses Permissions'
    permissions_expect cursor "$parked"
    [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")" == "$before" ]] || fail "permissions: overlay dismissal committed a mode"

    permissions_viewport 560 400
    permissions_open notes.md keyboard
    shot "permissions-$permissions_group-small-initial"
    for name in Apply Close 'Owner read' 'Owner write' 'Owner execute' 'Group read' 'Group write' 'Group execute' 'Everyone read' 'Everyone write' 'Everyone execute' Octal Cancel; do
        key -k Tab >/dev/null
        permissions_wait "any(.controls[]; .name == $(jq -cn --arg name "$name" '$name') and .focused)" "small viewport focuses $name"
        if [[ "$name" == Close ]]; then
            read -r rx ry rw rh <<< "$(ipc permissionsState | jq -r '.rect')"
        else
            read -r rx ry rw rh <<< "$(ipc permissionsState | jq -er '.bodyRect')"
        fi
        read -r left top width height <<< "$(ipc permissionsState | jq -r '.controls[] | select(.focused) | .rect')"
        [[ "$left $top $width $height" =~ ^-?[0-9]+\ -?[0-9]+\ [0-9]+\ [0-9]+$ ]] || fail "permissions: focused control has no rectangle"
        (( left >= rx && top >= ry && left + width <= rx + rw && top + height <= ry + rh )) \
            || fail "permissions: keyboard-focused $name is outside its visible clip"
    done
    shot "permissions-$permissions_group-small-keyboard"
    key -k Escape >/dev/null
    permissions_wait '(.opened == false)'
    permissions_viewport
}

permissions_failure() {
    local real_qs saved_path="$PATH" before contents wanted pid
    command -v bwrap >/dev/null || fail "permissions: installed bubblewrap is required for the real EROFS case"
    real_qs=$(command -v qs) || fail "permissions: real Quickshell executable unavailable"
    [[ "$real_qs" == /* && -x "$real_qs" ]] || fail "permissions: Quickshell path is not absolute and executable"
    kill_flea
    permissions_guard "$permissions_box/bin"
    mkdir "$permissions_box/bin" || fail "permissions: could not create the owned launch directory"
    permissions_guard "$permissions_box/bin/qs"
    cat > "$permissions_box/bin/qs" <<'SH'
#!/usr/bin/env bash
set -eu
[[ "$PERMISSIONS_REAL_QS" == /* && -x "$PERMISSIONS_REAL_QS" ]] || exit 80
if [[ "$#" != 2 || "$1" != -p || "$2" != "$PERMISSIONS_UI" ]]; then exec "$PERMISSIONS_REAL_QS" "$@"; fi
[[ -n "$PERMISSIONS_BIND_ROOT" && "$PERMISSIONS_BIND_ROOT" == /* && -f "$PERMISSIONS_BIND_ROOT/.flea-test-sandbox" ]] || exit 81
[[ -n "$PERMISSIONS_BIND_FILE" && "$PERMISSIONS_BIND_FILE" == "$PERMISSIONS_BIND_ROOT/"* ]] || exit 82
[[ -f "$PERMISSIONS_BIND_FILE" && ! -L "$PERMISSIONS_BIND_FILE" ]] || exit 83
[[ "$(realpath -e -- "$PERMISSIONS_BIND_FILE")" == "$PERMISSIONS_BIND_FILE" ]] || exit 84
# A plain bind is nodev; retain the native GPU while making only the fixture file read-only.
exec bwrap --die-with-parent --unshare-user --bind / / --dev-bind /dev/dri /dev/dri --ro-bind "$PERMISSIONS_BIND_FILE" "$PERMISSIONS_BIND_FILE" -- "$PERMISSIONS_REAL_QS" "$@"
SH
    chmod 0700 "$permissions_box/bin/qs" || fail "permissions: could not make the owned launcher executable"
    export PERMISSIONS_REAL_QS="$real_qs" PERMISSIONS_UI="$flea_ui"
    export PERMISSIONS_BIND_ROOT="$permissions_box" PERMISSIONS_BIND_FILE="$permissions_listing/notes.md"
    permissions_guard "$PERMISSIONS_BIND_FILE"
    before=$(stat -c '%d:%i:%u:%g:%a:%s' "$PERMISSIONS_BIND_FILE")
    contents=$(sha256sum < "$PERMISSIONS_BIND_FILE")
    export PATH="$permissions_box/bin:$saved_path"
    launch "$permissions_listing"
    export PATH="$saved_path"
    wait_listing 6
    permissions_viewport
    pid=$(flea_pid)
    [[ "$(readlink "/proc/$pid/ns/user")" != "$(readlink "/proc/$$/ns/user")" ]] \
        || fail "permissions: candidate did not enter a distinct user namespace"
    # Sample mountinfo: "112 29 0:25 /notes.md /owned/listing/notes.md ro,relatime - ext4 /dev/device rw".
    python3 - "$pid" "$PERMISSIONS_BIND_FILE" <<'PY' || fail "permissions: candidate file mount is not read-only"
import pathlib, re, sys
pid, target = sys.argv[1:]
matches = []
for line in pathlib.Path('/proc', pid, 'mountinfo').read_text().splitlines():
    fields = line.split()
    path = re.sub(r'\\([0-7]{3})', lambda match: chr(int(match[1], 8)), fields[4])
    if path == target:
        matches.append(fields[5].split(','))
if len(matches) != 1 or 'ro' not in matches[0]:
    raise SystemExit('exact fixture mount must exist once with ro')
print('PERMISSIONS_MOUNT pid=' + pid + ' path=' + target + ' options=' + ','.join(matches[0]))
PY
    permissions_open notes.md
    for wanted in 0600 0640; do
        permissions_octal "$wanted"
        permissions_wait '.editable and any(.controls[]; .name == "Apply" and .enabled)' 'owned read-only mount reaches normal Apply'
        permissions_control Apply
        permissions_wait ".opened and (.busy == false) and .editable and .mode == \"$wanted\" and .displayedError == \"Could not change mode: filesystem is read-only. No change was applied.\"" 'real kernel refusal retains editable fields and reports one sentence'
        [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$PERMISSIONS_BIND_FILE")" == "$before" \
            && "$(sha256sum < "$PERMISSIONS_BIND_FILE")" == "$contents" ]] || fail "permissions: refused mode write changed the fixture"
        shot "permissions-$permissions_group-erofs-$wanted"
    done
    permissions_control Cancel
    permissions_wait '(.opened == false)'
    kill_flea
    unset PERMISSIONS_REAL_QS PERMISSIONS_UI PERMISSIONS_BIND_ROOT PERMISSIONS_BIND_FILE
    launch "$permissions_listing"
    wait_listing 6
    permissions_viewport
    permissions_open notes.md keyboard
    permissions_octal 0600
    permissions_apply "$permissions_listing/notes.md" 600
    [[ "$(sha256sum < "$permissions_listing/notes.md")" == "$contents" ]] || fail "permissions: recovery changed contents"
    printf 'PERMISSIONS_RECOVERY same candidate and fixture, reopened outside the failed read-only mount\n'
}

permissions_backend_owned() {
    local pid="$1"
    [[ "$pid" =~ ^[0-9]+$ && -r "/proc/$pid/environ" ]] || return 1
    [[ "$(readlink "/proc/$pid/exe")" == "$(realpath -e "$flea_bin")" \
        && "$(stat -c '%u' "/proc/$pid")" == "$(id -u)" ]] || return 1
    backend_pids | grep -Fx "$pid" >/dev/null || return 1
    tr '\0' '\n' < "/proc/$pid/environ" | grep -Fx "FLEA_BIN=$flea_bin" >/dev/null || return 1
    tr '\0' '\n' < "/proc/$pid/environ" | grep -Fx "FLEA_PATH=$permissions_listing" >/dev/null || return 1
    tr '\0' '\n' < "/proc/$pid/environ" | grep -Fx "XDG_STATE_HOME=$XDG_STATE_HOME" >/dev/null
}

permissions_resume_stopped() {
    local pid="$1"
    [[ -n "$pid" ]] || return 0
    if permissions_backend_owned "$pid"; then
        kill -CONT "$pid" || { printf 'FAIL: permissions: could not resume owned backend %s\n' "$pid" >&2; return 1; }
    elif [[ -d "/proc/$pid" ]]; then
        printf 'FAIL: permissions: paused backend %s changed identity; no signal sent\n' "$pid" >&2
        return 1
    fi
}

permissions_backenddeath() {
    local phase pid before contents wanted original end status expected wanted_key permissions_stopped=""
    local -a pids
    for phase in completing idle applying; do
        permissions_open notes.md
        before=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")
        contents=$(sha256sum < "$permissions_listing/notes.md")
        original=$(ipc permissionsState | jq -r '.mode')
        if [[ "$original" == 0600 ]]; then wanted=0640; else wanted=0600; fi
        permissions_octal "$wanted"
        permissions_guard "$permissions_listing/notes.md"
        mapfile -t pids < <(backend_pids)
        [[ "${#pids[@]}" == 1 ]] || fail "permissions: backend-death case requires one attributable backend"
        pid="${pids[0]}"
        permissions_backend_owned "$pid" || fail "permissions: backend belongs to another candidate, fixture, or session"
        if [[ "$phase" != idle ]]; then
            permissions_stopped="$pid"
            trap 'permissions_resume_stopped "$permissions_stopped"' EXIT
            kill -STOP "$pid" || fail "permissions: could not pause the owned backend"
            end=$((SECONDS + 20))
            while (( SECONDS < end )); do
                # Sample process state: "Tsl", whose initial T confirms SIGSTOP took effect.
                status=$(ps -o stat= -p "$pid") || fail "permissions: paused backend disappeared"
                [[ "$status" == T* ]] && break
                sleep 0.05
            done
            [[ "$status" == T* ]] || fail "permissions: owned backend did not stop"
            permissions_control Apply
            permissions_wait '.opened and .busy and (.editable == false) and (.displayedError | startswith("Applying permissions"))' 'real Apply waits for the paused backend'
            for wanted_key in Escape Return space; do
                key -k "$wanted_key" >/dev/null
                permissions_wait '.opened and .busy' "pending Apply survives $wanted_key until its result arrives"
            done
            permissions_control Cancel
            permissions_control Close
            permissions_wait '.opened and .busy and all(.controls[] | select(.name == "Close" or .name == "Cancel"); .enabled == false)' 'pending Apply cannot discard its result through dismissal'
        fi
        if [[ "$phase" == completing ]]; then
            permissions_resume_stopped "$pid"
            permissions_stopped=""
            trap - EXIT
            permissions_wait '.opened == false' 'resumed Apply delivers its result before closing'
            [[ "$(stat -c '%a' "$permissions_listing/notes.md")" == "${wanted#0}" \
                && "$(sha256sum < "$permissions_listing/notes.md")" == "$contents" ]] \
                || fail "permissions: resumed Apply did not commit only the requested mode"
            continue
        fi
        permissions_backend_owned "$pid" || fail "permissions: backend identity changed before the failure signal"
        kill -KILL "$pid" || fail "permissions: could not terminate the owned backend"
        permissions_stopped=""
        trap - EXIT
        if [[ "$phase" == applying ]]; then
            expected='(.displayedError | contains("outcome is unknown") and contains("check the current mode"))'
        else
            expected='(.displayedError | contains("Permissions is unavailable") and contains("restart Flea"))'
        fi
        permissions_wait ".opened and (.busy == false) and (.editable == false) and .mode == \"$wanted\" and $expected" "$phase backend death retains the draft and reports the actual failure"
        permissions_wait 'all(.controls[] | select(.bit != null or .name == "Octal" or .name == "Apply"); .enabled == false)' 'lost descriptor disables every mutation control'
        shot "permissions-$permissions_group-backend-dead-$phase"
        permissions_control Apply
        permissions_wait ".opened and (.busy == false) and $expected" 'disabled Apply cannot erase the failure or reuse the lost descriptor'
        [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/notes.md")" == "$before" \
            && "$(sha256sum < "$permissions_listing/notes.md")" == "$contents" ]] \
            || fail "permissions: controlled backend death changed the paused fixture"
        printf 'PERMISSIONS_BACKEND_DEATH phase=%s pid=%s source=%s path=%s\n' "$phase" "$pid" "$flea_bin" "$permissions_listing/notes.md"
        permissions_control Cancel
        permissions_wait '(.opened == false)'
        launch "$permissions_listing"
        wait_listing 6
        permissions_viewport
        permissions_open notes.md keyboard
        permissions_mode "$original"
        permissions_wait '.editable and .displayedError == ""' 'restarted candidate obtains a new valid descriptor'
        shot "permissions-$permissions_group-backend-reopened-$phase"
        permissions_octal "$wanted"
        permissions_apply "$permissions_listing/notes.md" "${wanted#0}"
        [[ "$(sha256sum < "$permissions_listing/notes.md")" == "$contents" ]] || fail "permissions: recovered mode write changed contents"
    done
}

case_permissionsbaseline() { case_permissions baseline; }
case_permissionsfile() { case_permissions file; }
case_permissionsdirectory() { case_permissions directory; }
case_permissionsreadonly() { case_permissions readonly; }
case_permissionsstale() { case_permissions stale; }
case_permissionskeys() { case_permissions keys; }
case_permissionseligibility() { case_permissions eligibility; }
case_permissionsoverlay() { case_permissions overlay; }
case_permissionsfailure() { case_permissions failure; }
case_permissionsbackenddeath() { case_permissions backenddeath; }

case_permissionsnonowner() {
    local external="${FLEA_PERMISSIONS_NONOWNER_ROOT:-}" session root before contents uid gid
    [[ -n "$external" && "$external" == /* && -d "$external" && -O "$external" \
        && -f "$external/.flea-test-sandbox" && ! -L "$external/.flea-test-sandbox" ]] \
        || fail "permissionsnonowner: FLEA_PERMISSIONS_NONOWNER_ROOT must name an owned, marked absolute fixture root"
    [[ "$(realpath -e -- "$external")" == "$external" ]] || fail "permissionsnonowner: fixture root is not canonical"
    local FIXTURE_ROOT="$external" fixture_root="$external"
    local permissions_box="$external" permissions_listing="$external/listing" permissions_checks=0 permissions_group=nonowner
    sandbox_root_ok
    sandbox_require "$FIXTURE_ROOT/listing"
    permissions_guard "$permissions_listing/owned-by-other.txt"
    [[ -f "$permissions_listing/owned-by-other.txt" && ! -L "$permissions_listing/owned-by-other.txt" ]] \
        || fail "permissionsnonowner: listing/owned-by-other.txt must be a regular file"
    uid=$(stat -c '%u' "$permissions_listing/owned-by-other.txt")
    gid=$(stat -c '%g' "$permissions_listing/owned-by-other.txt")
    [[ "$uid" != "$(id -u)" && "$(stat -c '%a' "$permissions_listing/owned-by-other.txt")" == 644 ]] \
        || fail "permissionsnonowner: fixture must belong to another uid and have ordinary mode 0644"
    before=$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/owned-by-other.txt")
    contents=$(sha256sum < "$permissions_listing/owned-by-other.txt")
    session=$(mktemp -d "$external/permissions-session.XXXXXXXX") || fail "permissionsnonowner: session creation failed"
    printf 'native nonowner Permissions session\n' > "$session/.flea-test-sandbox"
    for root in "$session/data" "$session/config" "$session/cache"; do
        permissions_guard "$root"
        mkdir "$root" || fail "permissionsnonowner: session directory creation failed"
    done
    export XDG_DATA_HOME="$session/data" XDG_CONFIG_HOME="$session/config" XDG_CACHE_HOME="$session/cache"
    seed_ui_state "$session/state" '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"off"},"menu":{"hidden":[]}}'
    trap 'kill_flea' EXIT
    launch "$permissions_listing"
    wait_listing 1
    permissions_viewport
    permissions_open owned-by-other.txt keyboard
    shot permissions-nonowner-readonly
    permissions_wait ".facts.uid == $uid and .facts.gid == $gid" 'displayed ownership facts match the real nonowner fixture'
    permissions_readonly_controls 'Read-only: you are not the owner.' 0644
    [[ "$(stat -c '%d:%i:%u:%g:%a:%s' "$permissions_listing/owned-by-other.txt")" == "$before" \
        && "$(sha256sum < "$permissions_listing/owned-by-other.txt")" == "$contents" ]] \
        || fail "permissionsnonowner: native refusal changed the external fixture"
    kill_flea
    trap - EXIT
    permissions_guard "$session"
    sandbox_remove "$session"
    printf 'PERMISSIONS_NATIVE group=nonowner checks=%s external_fixture=%s preserved=yes; screenshots require separate inspection.\n' "$permissions_checks" "$external"
}

case_permissions() {
    local permissions_box permissions_listing permissions_checks=0 permissions_group="${1:-full}"
    permissions_setup
    permissions_open notes.md
    shot "permissions-$permissions_group-file-baseline"
    printf 'PERMISSIONS_BASELINE group=%s viewport=%q state=%s\n' "$permissions_group" "$(window_box)" "$(ipc permissionsState)"
    if [[ "$permissions_group" == full || "$permissions_group" == file ]]; then
        permissions_file
    else
        key -k Escape >/dev/null
        permissions_wait '(.opened == false)'
    fi
    case "$permissions_group" in
        baseline|file) ;;
        directory) permissions_directory ;;
        readonly) permissions_readonly ;;
        stale) permissions_stale ;;
        keys) permissions_keys ;;
        eligibility) permissions_eligibility ;;
        overlay) permissions_overlay ;;
        failure) permissions_failure ;;
        backenddeath) permissions_backenddeath ;;
        full) permissions_directory; permissions_readonly; permissions_keys; permissions_eligibility; permissions_failure; permissions_backenddeath; permissions_stale; permissions_overlay ;;
        *) fail "permissions: unknown focused group $permissions_group" ;;
    esac
    printf 'PERMISSIONS_NATIVE group=%s checks=%s root=%s; screenshots require separate inspection.\n' "$permissions_group" "$permissions_checks" "$permissions_box"
    kill_flea
}
