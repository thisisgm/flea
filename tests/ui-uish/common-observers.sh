#!/usr/bin/env bash
# GPUI observes the operator's current palette at each check, including changes during a run.
[[ "${test_frontend:-qml}" == gpui ]] || return 0

assert_theme() {
    local loaded seen expected accent
    local -a route=(-p "$flea_ui/boot")
    [[ -z "$ipc_pid" ]] || route=(--pid "$ipc_pid")
    flea_ipc_wait "${route[@]}" flea ready true --timeout 20 >/dev/null \
        || fail "the IPC seam never answered ready, so no reader of this window can be trusted"
    for _attempt in $(seq 1 100); do
        # Sample input: foreground = "#c1c497"
        expected=$(grep -E '^foreground' "$real_state_dir/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
        [[ "$expected" =~ ^#[0-9A-Fa-f]{6}$ ]] \
            || fail "no foreground in $real_state_dir/theme/colors.toml, so no shot could be checked against the live palette"
        # Sample input: accent = "#d28a41"
        accent=$(grep -E '^accent' "$real_state_dir/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
        [[ "$accent" =~ ^#[0-9A-Fa-f]{6}$ ]] \
            || fail "no accent in $real_state_dir/theme/colors.toml, so no focused field could be checked against the live palette"
        seen=$(ipc themeForeground 2>/dev/null || true)
        loaded=$(ipc themeLoaded 2>/dev/null || true)
        [[ "$loaded" != true || "${seen,,}" != "${expected,,}" ]] || break
        sleep 0.05
    done
    [[ "$loaded" == true ]] || fail "themeLoaded is '$loaded', not true, so this window has no parsed palette"
    [[ "${seen,,}" == "${expected,,}" ]] \
        || fail "this window paints foreground '$seen', not the live theme's $expected, so no shot or colour claim from it is real"
    real_foreground=$expected
    real_accent=$accent
}

case_scroll() {
    [[ -d "$bench_dir" ]] || fail "the 100,000-file fixture is missing at $bench_dir"
    launch "$bench_dir"
    wait_listing 100000
    settle
    local wx wy ww wh cx cy before after
    [[ "$(ipc wheelLines)" == "3" ]] || fail "scroll: the platform reports $(ipc wheelLines) lines a notch, this case assumes 3"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    read -r cx cy <<< "$(ipc rowCentre 5)"
    # omarchy-drive scroll takes no point: warp there, then one uinput pixel so Qt sees a pointer frame.
    hypr_cursor_move "$((wx + cx - hypr_pointer_frame_offset))" "$((wy + cy))" || fail "scroll: pointer motion failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
    settle
    before=$(ipc listContentY)
    [[ "$before" == "0" ]] || fail "scroll: the list did not start at the top, contentY $before"
    omarchy-drive scroll up 1 >/dev/null
    settle
    [[ "$(ipc listContentY)" == "0" ]] || fail "scroll: a notch up at the top moved contentY to $(ipc listContentY)"
    omarchy-drive scroll down 1 >/dev/null
    settle
    after=$(ipc listContentY)
    [[ "$after" == "288" ]] || fail "scroll: one notch down moved contentY to $after, not 288 (3 lines x 24 px x 4)"
    omarchy-drive scroll down 2 >/dev/null
    settle
    after=$(ipc listContentY)
    [[ "$after" == "864" ]] || fail "scroll: two more notches moved contentY to $after, not 864"
    printf 'SCROLL one notch 288, three notches 864, top held at 0\n'
    shot scroll-three-notches
}

case_colour() {
    local dir="$fixture_root/colour"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir"
    : > "$dir/plain.txt"
    launch "$dir"
    wait_listing 2
    local foreground accent dir_colour file_colour
    read -r _background _surface foreground _muted accent _rest <<< "$(ipc palette)"
    dir_colour=$(ipc rowNameColor 0)
    file_colour=$(ipc rowNameColor 1)
    printf 'COLOUR dir=%s file=%s foreground=%s accent=%s cursor=%s\n' \
        "$dir_colour" "$file_colour" "$foreground" "$accent" "$(ipc cursor)"
    shot colour-directory
    [[ "$dir_colour" != "$accent" ]] || fail "the directory name is still the cursor accent $accent"
    [[ "$dir_colour" == "$foreground" ]] || fail "the directory name is $dir_colour, not foreground $foreground"
    [[ "$file_colour" == "$foreground" ]] || fail "the file name is $file_colour, not foreground $foreground"
}

case_lifted() {
    local dir="$fixture_root/lifted"
    sandbox_scratch "$dir"
    : > "$dir/a-plain.txt"
    ln -s "$dir/a-plain.txt" "$dir/z-link"
    launch "$dir"
    wait_listing 2

    # Found by name, not by a predicted sort position, see goto_row and icon_of for the same rule.
    local foreground symlink_colour i link_row=-1 plain_row=-1
    read -r _background _surface foreground _muted _accent _error symlink_colour _executable _rest <<< "$(ipc palette)"
    for ((i = 0; i < 2; i++)); do
        case "$(ipc rowAt "$i")" in
            z-link\|*) link_row=$i ;;
            a-plain.txt\|*) plain_row=$i ;;
        esac
    done
    [[ "$link_row" -ge 0 && "$plain_row" -ge 0 ]] || fail "the lifted fixture did not list both rows"

    goto_row "$plain_row"
    [[ "$(ipc cursor)" == "$plain_row" ]] || fail "the cursor did not land on the plain row"
    local off_name off_cell
    off_name=$(ipc rowNameColor "$link_row")
    # The symlink row is the one under test throughout; here the cursor sits elsewhere, so it reads unlifted.
    off_cell=$(ipc rowCellColor "$link_row")
    printf 'LIFTED off name=%s symlink=%s foreground=%s cell=%s\n' \
        "$off_name" "$symlink_colour" "$foreground" "$off_cell"
    [[ "$off_name" == "$symlink_colour" ]] \
        || fail "the symlink off the cursor is $off_name, not the symlink colour $symlink_colour"
    [[ "$off_name" != "$foreground" ]] || fail "the symlink off the cursor is already foreground"
    [[ "$off_cell" != "$foreground" ]] || fail "the symlink row's cell is foreground before it is ever lifted"

    goto_row "$link_row"
    [[ "$(ipc cursor)" == "$link_row" ]] || fail "the cursor did not land on the symlink row"
    local on_name on_cell
    on_name=$(ipc rowNameColor "$link_row")
    on_cell=$(ipc rowCellColor "$link_row")
    printf 'LIFTED on name=%s cell=%s foreground=%s\n' "$on_name" "$on_cell" "$foreground"
    shot lifted
    [[ "$on_name" == "$foreground" ]] || fail "the cursor's symlink name is $on_name, not foreground $foreground"
    [[ "$on_cell" == "$foreground" ]] || fail "the cursor's cell colour is $on_cell, not foreground $foreground"
}

case_settings() {
    local dir="$fixture_root/settings"
    local config="$fixture_root/settings-config"
    local state="$fixture_root/settings-state"
    sandbox_scratch "$dir"
    sandbox_scratch "$config"
    sandbox_scratch "$state"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    mkdir -p "$dir/sub" || fail "settings: the second favourite's folder could not be made"
    local real_config="${XDG_CONFIG_HOME-}"
    local real_state="${XDG_STATE_HOME-}"
    export XDG_CONFIG_HOME="$config"
    export XDG_STATE_HOME="$state"
    local stored="$state/flea/ui.json"

    # Seeded through the window's own CLI, so src/uistate.rs's merge is what keeps the extra keys.
    settings_seed "$state" "$config" "$stored"

    launch "$dir"
    wait_listing 3

    settings_doors
    settings_view
    settings_preview
    settings_places "$dir"
    settings_about
    settings_display
    settings_menus
    settings_keys

    # One override left standing, so the restart below has a text size to bring back as well.
    key -M ctrl -M shift -k equal -m shift -m ctrl >/dev/null
    settle
    local pinned_base
    pinned_base=$(token_of baseSize)
    # rowNamePx reads the list's delegate, so the list view must show with row 0 a named row.
    [[ "$(ipc viewMode)" == "list" && "$(ipc rowAt 0)" == *"|"* ]] || fail "settings: the typography proof needs the list view on a named row, got $(ipc viewMode) and '$(ipc rowAt 0)'"
    [[ "$(ipc bodyPx)" == "$pinned_base" && "$(ipc rowNamePx 0)" == "$pinned_base" ]] \
        || fail "settings: at the ${pinned_base}px stop the body draws $(ipc bodyPx) and row 0's name $(ipc rowNamePx 0)"

    # Restart survival: each value is checked in the file, through the backend, and after a relaunch.
    [[ -f "$stored" ]] || fail "settings: the panel wrote no state file at $stored"
    grep -q '"paste"' "$stored" || fail "settings: the hidden action never reached the state file"
    grep -q '"keys": "windows"' "$stored" || fail "settings: the preset never reached the state file"
    grep -q "\"mode\": $pinned_base" "$stored" \
        || fail "settings: the state file holds no ${pinned_base}px stop"
    # An override stores a stop, never a free number or a multiplier.
    ! grep -q 'uiScale' "$stored" || fail "settings: the state file still carries an interface-scale multiplier"
    ! grep -q '"px"' "$stored" || fail "settings: the state file still carries 0.1.3's override shape"
    [[ ! -e "$config/flea/view.json" ]] \
        || fail "settings: a second settings file was written at $config/flea/view.json"

    settings_assert_backend "$state" "$config" "$pinned_base"

    kill_flea
    launch "$dir"
    wait_listing 3
    [[ "$(token_of baseSize)" == "$pinned_base" ]] \
        || fail "settings: a restart lost the ${pinned_base}px override, it draws at $(token_of baseSize)"
    settings_open_key
    settle
    settings_section display
    [[ "$(ipc settingsRows)" == *"fact|Effective|${pinned_base} px"* ]] \
        || fail "settings: a restart brought the panel back on a different stop"
    # The master row is derived from the stored set, so a restart must still draw five of six.
    settings_section menus
    # The restarted GPUI page resolves the same catalog heading as the shared Menus helper.
    [[ "$(ipc settingsRows)" == *"group|Basic File Actions|5 of 6"* ]] \
        || fail "settings: a restart did not derive the master back to five of six, got $(ipc settingsRows)"
    key -k Escape >/dev/null
    settle
    click_row 0 right
    settle
    local reopened="|$(ipc contextMenuEntries)|"
    [[ "$reopened" != *"|Paste|"* ]] || fail "settings: a restart brought the hidden Paste row back"
    [[ "$reopened" == *"|Cut|"* ]] || fail "settings: a restart lost the rows that were left enabled"
    key -k Escape >/dev/null
    settle
    key -M ctrl -k h -m ctrl >/dev/null
    settle
    [[ "$(ipc showHidden)" == "true" ]] || fail "settings: the stored Windows preset did not survive a restart"
    key -M ctrl -k h -m ctrl >/dev/null
    settle

    settings_write_refused "$state" "$pinned_base"

    # Back to following, so nothing after this case runs at a size it did not ask for.
    key -M ctrl -M shift -k 0 -m shift -m ctrl >/dev/null
    settle
    # Following, running text is Omarchy's own regular body, which is its base size, and the row name draws it.
    [[ "$(ipc bodyPx)" == "$(token_of baseSize)" && "$(ipc rowNamePx 0)" == "$(ipc bodyPx)" ]] \
        || fail "settings: following Omarchy the body draws $(ipc bodyPx) against base $(token_of baseSize), row 0's name $(ipc rowNamePx 0)"

    settings_read_refused "$stored" "$dir"

    printf 'SETTINGS doors=ok view=ok places=ok preview=ok about=ok display=ok menus=ok keys=ok restart=ok backend=ok refused=ok unread=ok\n'
    if [[ -n "$real_config" ]]; then export XDG_CONFIG_HOME="$real_config"; else unset XDG_CONFIG_HOME; fi
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    kill_flea
}
