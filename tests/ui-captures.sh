# Board capture cases (outside the default wanted list); tests/ui.sh supplies fixture, helpers and shots.

# Resize floats the owned window exact, the permissions_viewport idiom, so sizes are set not assumed.
cap_resize() {
    local target_width="$1" target_height="$2"
    local client address result wx wy width height end=$((SECONDS + 20))
    client=$(hyprctl clients -j | jq -ec --argjson pid "$(flea_pid)" '.[] | select(.pid == $pid)') \
        || fail "captures: owned window unavailable for resize to ${target_width}x${target_height}"
    address=$(jq -er '.address' <<< "$client") || fail "captures: owned window has no address"
    [[ "$address" =~ ^0x[0-9a-fA-F]+$ ]] || fail "captures: invalid owned window address $address"
    if ! jq -e '.floating' <<< "$client" >/dev/null; then
        omarchy-drive window float "$address" >/dev/null || fail "captures: owned window could not float"
    fi
    result=$(hypr_window_resize "$address" "$target_width" "$target_height" 2>&1) \
        || fail "captures: compositor refused resize to ${target_width}x${target_height}: $result"
    omarchy-drive window center "$address" >/dev/null || fail "captures: owned window could not center"
    while (( SECONDS < end )); do
        read -r wx wy width height < <(window_box) || fail "captures: native window coordinates unavailable"
        [[ "$width" == "$target_width" && "$height" == "$target_height" ]] && return 0
        sleep 0.05
    done
    fail "captures: viewport did not reach ${target_width}x${target_height}, it is ${width}x${height}"
}

# One menuState read must satisfy a jq expression within a bound before a shot is taken, so every state is asserted, never assumed.
cap_menu_expect() {
    local expression="$1" label="$2" observed poll menu_polls=100
    for poll in $(seq 1 "$menu_polls"); do
        observed=$(ipc menuState) || fail "captures: the menu observer failed for $label"
        jq -e "$expression" <<< "$observed" >/dev/null && return 0
        sleep 0.05
    done
    fail "captures: $label: $observed"
}

# Tabs040: three tabs with one held mid-drag, then Settings View Opening on Last folder, and its tail scrolled into frame with the hint.
case_cap_tabs() {
    local dir="$fixture_root/cap-tabs"
    sandbox_scratch "$dir"
    mkdir -p "$dir/alpha" "$dir/beta" "$dir/gamma"
    seed_ui_state "$fixture_root/cap-tabs-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 3
    cap_resize 900 541
    key t >/dev/null
    settle
    key t >/dev/null
    settle
    [[ "$(ipc tabCount)" == "3" ]] || fail "cap_tabs: t twice did not make 3 tabs, count=$(ipc tabCount)"
    click_tab 0
    seek_row_named "alpha" || fail "cap_tabs: could not find alpha"
    key -k Return >/dev/null
    wait_path "$dir/alpha"
    click_tab 1
    seek_row_named "beta" || fail "cap_tabs: could not find beta"
    key -k Return >/dev/null
    wait_path "$dir/beta"
    click_tab 2
    seek_row_named "gamma" || fail "cap_tabs: could not find gamma"
    key -k Return >/dev/null
    wait_path "$dir/gamma"
    [[ "$(ipc tabLabels)" == "alpha|beta|gamma" ]] || fail "cap_tabs: tabs label [$(ipc tabLabels)], not alpha|beta|gamma"
    local c0x c0y c1x c1y c2x c2y w
    read -r c0x c0y <<< "$(ipc tabCentre 0)"
    read -r c1x c1y <<< "$(ipc tabCentre 1)"
    read -r c2x c2y <<< "$(ipc tabCentre 2)"
    [[ -n "$c1y" && -n "$c2y" ]] || fail "cap_tabs: a tab has no centre"
    w=$((c1x - c0x))
    (( w > 0 )) || fail "cap_tabs: tab centres do not step right [$c0x,$c1x,$c2x]"
    # tabdrag_to presses, moves, shots while held, then releases: the held shot is the specimen.
    tabdrag_to "$c1x" "$c1y" "$((c2x + w / 2 + 3))" "$c2y" cap-tabs-drag-held
    [[ "$(ipc tabLabels)" == "alpha|gamma|beta" ]] || fail "cap_tabs: drag labelled [$(ipc tabLabels)], not alpha|gamma|beta"
    settings_open_key
    settle
    settings_section view
    settings_focus_row startIn
    key l >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"Last folder reopens every tab you had."* ]] \
        || fail "cap_tabs: Last folder drew no tab hint, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" == *"New tabs open in"* ]] \
        || fail "cap_tabs: Opening drew no New tabs row, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" == *"Open items with"* ]] \
        || fail "cap_tabs: Opening drew no Open items row, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" == *"Click a selected name to rename"* ]] \
        || fail "cap_tabs: Opening drew no click-rename row, got $(ipc settingsRows)"
    shot cap-tabs-opening-last-folder
    # The shot above stops above the hint; the last control in the group scrolls the whole Opening card into frame.
    settings_focus_row clickRename
    settle
    shot cap-tabs-opening-last-folder-tail
    key -k Escape >/dev/null
    settle
    printf 'CAP_TABS drag=held labels=%s hint=shown\n' "$(ipc tabLabels)"
    kill_flea
}

# ClickAndRefresh: a slow-click rename as it opens, then Opening on Single click where
# Click a selected name to rename greys. Board specimen: the stem selected, ".md" muted.
case_cap_click() {
    local dir="$fixture_root/cap-click"
    sandbox_scratch "$dir"
    printf 'bench notes\n' > "$dir/field-bench-notes.md"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    seed_ui_state "$fixture_root/cap-click-state" '{"keys":"default","view":"list","openMode":"double","clickRename":true}'
    launch "$dir"
    wait_listing 3
    seek_row_named "field-bench-notes.md" || fail "cap_click: could not find field-bench-notes.md"
    local idx
    idx=$(row_index_of "field-bench-notes.md")
    click_row_name "$idx" left
    settle
    # A second click after the double-click interval starts rename; inside it opens instead.
    sleep 0.7
    click_row_name "$idx" left
    settle
    local waited
    for waited in $(seq 1 100); do
        [[ "$(ipc renameEditorLive)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc renameEditorLive)" == "true" ]] || fail "cap_click: the slow click never opened rename"
    [[ "$(ipc renameEditorText)" == "field-bench-notes.md" ]] \
        || fail "cap_click: rename holds '$(ipc renameEditorText)', not field-bench-notes.md"
    [[ "$(ipc renameState | jq -r .selectedText)" == "field-bench-notes" ]] \
        || fail "cap_click: rename selects '$(ipc renameState | jq -r .selectedText)', not the stem"
    shot cap-click-rename-slow
    key -k Escape >/dev/null
    settle
    settings_open_key
    settle
    settings_section view
    settings_focus_row openMode
    key l >/dev/null
    settle
    [[ "$(ipc settingsModel | jq -er '[.[] | select(.id == "openMode")][0].value')" == *"Single"* ]] \
        || fail "cap_click: Open items with never reached Single click"
    [[ "$(ipc settingsModel | jq -er '[.[] | select(.id == "clickRename")][0].available')" == "false" ]] \
        || fail "cap_click: Click a selected name to rename never greyed under Single click"
    shot cap-click-opening-single
    key -k Escape >/dev/null
    settle
    printf 'CAP_CLICK rename=stem-selected single=greyed\n'
    kill_flea
}

# ClickAndRefresh slow click rename: the one editor in every view at text size 14 (decided 2026-10-03), the rail on a Network place.
case_cap_rename() {
    local dir="$fixture_root/cap-rename" fixture_home="$fixture_root/cap-rename-home"
    local real_home="$HOME" saved_path="$PATH" waited
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin" "$dir/subdir"
    printf 'notes\n' > "$dir/notes.txt"
    : > "$dir/todo.md"
    printf '#!/bin/sh\nexit 0\n' > "$dir/bin/gio"
    chmod +x "$dir/bin/gio"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf 'smb://192.168.1.10/data NAS\n' > "$fixture_home/.config/gtk-3.0/bookmarks"
    seed_ui_state "$fixture_root/cap-rename-state" '{"keys":"default","view":"list","display":{"textSize":{"mode":14}}}'
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin, subdir, notes.txt and todo.md
    wait_listing 4
    cap_resize 1100 700
    for waited in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "NAS|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "NAS|network|share|false" ]] || fail "cap_rename: the rail reads $(ipc networkEntries), not the saved NAS"
    cap_rename_open list notes.txt cap-rename-list
    click_chrome columns
    settle
    cap_rename_open columns notes.txt cap-rename-columns-file
    cap_rename_open columns subdir cap-rename-columns-folder
    # A slash is refused in place, so the editor stays up and shows its error line under the frame.
    key -M ctrl -k a -m ctrl -k BackSpace >/dev/null
    key "a/b" >/dev/null
    key -k Return >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc renameState | jq -r .error)" == *"cannot"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc renameState | jq -r .error)" == *"cannot"* ]] || fail "cap_rename: the slash name raised no error in the columns"
    settle
    shot cap-rename-columns-error
    key -k Escape >/dev/null
    settle
    click_chrome grid
    settle
    cap_rename_open grid notes.txt cap-rename-grid
    click_chrome dual
    settle
    cap_rename_open dual notes.txt cap-rename-dual
    click_chrome list
    settle
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "cap_rename: Tab did not reach the rail"
    rail_seek NAS
    key -k F2 >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc railRenameFieldShown)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc railRenameFieldShown)" == "true" ]] || fail "cap_rename: F2 on the NAS row opened no rail editor"
    settle
    shot cap-rename-rail
    key -k Escape >/dev/null
    settle
    printf 'CAP_RENAME views=list,columns-file,columns-folder,grid,dual,rail error=columns\n'
    kill_flea
}

# The cursor on a named row; the grid goes by its own rows (j) and columns (l), since l clamps at a row's end and never wraps to the next row.
cap_seek_named() {
    local want="$1" idx got
    [[ "$(ipc viewMode)" == grid ]] || { seek_row_named "$want"; return; }
    idx=$(row_index_of "$want") || fail "cap_seek_named: no row named $want in the grid listing"
    goto_row "$idx"
    got=$(ipc renameState | jq -r .cursorName)
    [[ "$got" == "$want" ]] || fail "cap_seek_named: the grid cursor is on [$got] at index $(ipc cursor), not $want at index $idx"
}

# The cursor on a named row, F2, and a shot once the editor holds the caret; Escape closes it unless the caller goes on to type.
cap_rename_open() {
    local view="$1" name="$2" shot_name="$3" waited
    [[ "$(ipc viewMode)" == "$view" || "$view" == dual ]] || fail "cap_rename: the chrome drew '$(ipc viewMode)', not $view"
    cap_seek_named "$name"
    key -k F2 >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc renameState | jq -r .focused)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc renameState | jq -r .focused)" == "true" ]] || fail "cap_rename: $view opened no focused editor on $name"
    [[ "$(ipc renameEditorText)" == "$name" ]] || fail "cap_rename: $view editor holds '$(ipc renameEditorText)', not $name"
    settle
    shot "$shot_name"
    if [[ "$shot_name" != cap-rename-columns-folder ]]; then
        key -k Escape >/dev/null
        settle
    fi
}

# KeyboardFlows "View, Cursor at defaults": the Cursor group with both rows off.
case_cap_cursor() {
    local dir="$fixture_root/cap-cursor"
    sandbox_scratch "$dir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    seed_ui_state "$fixture_root/cap-cursor-state" '{"keys":"default","view":"list","wrapAtEnds":false,"escapeUp":false}'
    launch "$dir"
    wait_listing 2
    settings_open_key
    settle
    settings_section view
    [[ "$(ipc settingsRows)" == *"Wrap at list ends"* ]] \
        || fail "cap_cursor: View drew no Wrap at list ends, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" == *"Escape goes up a folder"* ]] \
        || fail "cap_cursor: View drew no Escape row, got $(ipc settingsRows)"
    [[ "$(ipc settingsModel | jq -er '[.[] | select(.id == "wrapAtEnds")][0].on')" == "false" ]] \
        || fail "cap_cursor: Wrap at list ends is not off"
    [[ "$(ipc settingsModel | jq -er '[.[] | select(.id == "escapeUp")][0].on')" == "false" ]] \
        || fail "cap_cursor: Escape goes up a folder is not off"
    shot cap-cursor-defaults
    key -k Escape >/dev/null
    settle
    printf 'CAP_CURSOR wrap=off escape=off\n'
    kill_flea
}

# MenuAdditions040: the file menu with the Copy as flyout open, Paste as once the
# clipboard holds a file, the symlink menu with Show original, the background menu at
# defaults, and Settings Menus listing the new rows. Key hints on throughout.
case_cap_menus() {
    local dir="$fixture_root/cap-menus"
    sandbox_scratch "$dir"
    printf 'target\n' > "$dir/target.txt"
    ln -s target.txt "$dir/link.txt" || fail "cap_menus: the symlink fixture could not be made"
    seed_ui_state "$fixture_root/cap-menus-state" '{"keys":"default","view":"list","keyHints":true,"menu":{"hidden":["delete","openTerminal","moveto","copyto","properties","permissions","invertSelection"]}}'
    launch "$dir"
    wait_listing 2
    click_row "$(row_index_of target.txt)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_menus: the file menu never opened"
    [[ "|$(ipc contextMenuEntries)|" == *"|Copy As|"* ]] \
        || fail "cap_menus: the file menu offers no Copy As, got $(ipc contextMenuEntries)"
    menu_seek "Copy As"
    key -k Right >/dev/null
    settle
    [[ "$(ipc contextMenuSubmenuEntries)" == *"Path"* ]] \
        || fail "cap_menus: the Copy As flyout offers $(ipc contextMenuSubmenuEntries)"
    shot cap-menus-file-copyas
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_menus: the Copy As menu stayed open"
    click_row "$(row_index_of target.txt)" left
    settle
    key y >/dev/null
    settle
    [[ "$(ipc keyDeliveryState | jq -er '.clipboard.paths | length')" == "1" ]] \
        || fail "cap_menus: y put no file on the clipboard"
    click_row "$(row_index_of target.txt)" right
    settle
    menu_seek "Paste as"
    key -k Right >/dev/null
    settle
    [[ "$(ipc contextMenuSubmenuEntries)" == *"Link"* ]] \
        || fail "cap_menus: the Paste as flyout offers $(ipc contextMenuSubmenuEntries)"
    shot cap-menus-file-pasteas
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_menus: the Paste as menu stayed open"
    click_row "$(row_index_of link.txt)" right
    settle
    [[ "|$(ipc contextMenuEntries)|" == *"|Show Original|"* ]] \
        || fail "cap_menus: the symlink menu offers no Show original, got $(ipc contextMenuEntries)"
    shot cap-menus-symlink
    key -k Escape >/dev/null
    settle
    kill_flea
    seed_ui_state "$fixture_root/cap-menus-background-state" '{"keys":"default","view":"list","keyHints":true,"menu":{"hidden":["delete","openTerminal","openAgent","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","extThumbs"]}}'
    launch "$dir"
    wait_listing 2
    click_row "$(row_index_of target.txt)" left
    settle
    local background_selection_count
    background_selection_count=$(ipc selectionCount)
    [[ "$background_selection_count" == "1" ]] \
        || fail "cap_menus: background specimen needs one selected row, got $background_selection_count"
    key y >/dev/null
    settle
    click_background
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_menus: the background menu never opened"
    local background_labels='New Folder|New File|-|Paste|Select All|Invert Selection|-|Open in Terminal|Add to Favorites|-|Sort By|Show Hidden Files|Hide Code Icons|-|Settings…'
    [[ "$(ipc contextMenuEntries)" == "$background_labels" ]] \
        || fail "cap_menus: background specimen drew $(ipc contextMenuEntries), want $background_labels"
    shot cap-menus-background
    key -k Escape >/dev/null
    settle
    kill_flea
    seed_ui_state "$fixture_root/cap-menus-state" '{"keys":"default","view":"list","keyHints":true,"menu":{"hidden":["delete","openTerminal","moveto","copyto","properties","permissions","invertSelection"]}}'
    launch "$dir"
    wait_listing 2
    settings_open_key
    settle
    settings_section menus
    [[ "$(ipc settingsRows)" == *"Copy As"* ]] || fail "cap_menus: Menus lists no Copy As"
    [[ "$(ipc settingsRows)" == *"Paste As"* ]] || fail "cap_menus: Menus lists no Paste As"
    [[ "$(ipc settingsRows)" == *"Invert Selection"* ]] || fail "cap_menus: Menus lists no Invert Selection"
    [[ "$(ipc settingsRows)" == *"Permissions"* ]] || fail "cap_menus: Menus lists no Permissions"
    shot cap-menus-settings
    key -k Escape >/dev/null
    settle
    printf 'CAP_MENUS copyas=ok pasteas=ok symlink=ok background=ok settings=ok\n'
    kill_flea
}

# MenuAdditions040 and SettingsMenus states the first set leaves out: Make executable, the two-file menu, both flyouts with hints off, Settings > Menus at its tail, the Places row menu with and without Copy path.
case_cap_menus2() {
    local dir="$fixture_root/cap-menus2" attempt favourite_index
    local makeexec_polls=40 menu_hidden_file menu_hidden_place
    sandbox_scratch "$dir"
    mkdir -p "$dir/Work"
    printf '#!/bin/sh\necho flea\n' > "$dir/run.sh"
    chmod 0644 "$dir/run.sh" || fail "cap_menus2: the 0644 script fixture mode failed"
    printf 'one\n' > "$dir/one.txt"
    printf 'two\n' > "$dir/two.txt"
    menu_hidden_file='"delete","openTerminal","moveto","copyto","properties","permissions","invertSelection"'
    seed_ui_state "$fixture_root/cap-menus2-defaults-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 4
    click_row "$(row_index_of run.sh)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_menus2: the script's menu never opened"
    for attempt in $(seq 1 "$makeexec_polls"); do
        [[ "|$(ipc contextMenuEntries)|" == *"|Make Executable|"* ]] && break
        sleep 0.25
    done
    [[ "|$(ipc contextMenuEntries)|" == *"|Make Executable|"* ]] \
        || fail "cap_menus2: a 0644 script at defaults offers no Make Executable, got $(ipc contextMenuEntries)"
    shot cap-menus2-makeexec
    key -k Escape >/dev/null
    settle
    click_row "$(row_index_of one.txt)" left
    settle
    click_row "$(row_index_of two.txt)" left --mods ctrl
    settle
    [[ "$(ipc selectionCount)" == "2" ]] || fail "cap_menus2: a ctrl click selected $(ipc selectionCount) rows, not 2"
    click_row "$(row_index_of one.txt)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_menus2: the two-file menu never opened"
    shot cap-menus2-two-files
    key -k Escape >/dev/null
    settle
    kill_flea
    seed_ui_state "$fixture_root/cap-menus2-nohints-state" "$(printf '{"keys":"default","view":"list","keyHints":false,"menu":{"hidden":[%s]}}' "$menu_hidden_file")"
    launch "$dir"
    wait_listing 4
    click_row "$(row_index_of one.txt)" right
    settle
    menu_seek "Copy As"
    key -k Right >/dev/null
    settle
    [[ "$(ipc contextMenuSubmenuEntries)" == *"Path"* ]] \
        || fail "cap_menus2: the Copy As flyout offers $(ipc contextMenuSubmenuEntries)"
    shot cap-menus2-copyas-nohints
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    # No second click: one.txt is already the sole selection under the cursor, and a click there is a slow click.
    key y >/dev/null
    settle
    [[ "$(ipc keyDeliveryState | jq -er '.clipboard.paths | length')" == "1" ]] \
        || fail "cap_menus2: y put no file on the clipboard"
    click_row "$(row_index_of one.txt)" right
    settle
    menu_seek "Paste as"
    key -k Right >/dev/null
    settle
    [[ "$(ipc contextMenuSubmenuEntries)" == *"Link"* ]] \
        || fail "cap_menus2: the Paste as flyout offers $(ipc contextMenuSubmenuEntries)"
    shot cap-menus2-pasteas-nohints
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    kill_flea
    seed_ui_state "$fixture_root/cap-menus2-settings-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 4
    settings_open_key
    settle
    settings_section menus
    [[ "$(ipc settingsRows)" == *"Paste As"* ]] || fail "cap_menus2: Menus lists no Paste As"
    settings_focus_row invertSelection
    settle
    [[ "$(ipc settingsRows)" == *"Invert Selection"* ]] || fail "cap_menus2: Menus lists no Invert Selection"
    shot cap-menus2-settings-tail
    key -k Escape >/dev/null
    settle
    kill_flea
    menu_hidden_place='"delete","openTerminal","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs"'
    seed_ui_state "$fixture_root/cap-menus2-place-state" "$(printf '{"keys":"default","view":"list","menu":{"hidden":[%s]},"places":{"favourites":[{"label":"Work","path":"%s/Work"}]}}' "$menu_hidden_place" "$dir")"
    launch "$dir"
    wait_listing 4
    favourite_index=$(ipc railEntries | jq -r 'map(.label) | index("Work")')
    [[ -n "$favourite_index" && "$favourite_index" != "null" ]] \
        || fail "cap_menus2: the seeded favourite is not on the rail, which carries $(ipc railEntries)"
    click_rail_row "$favourite_index" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_menus2: the favourite's menu never opened"
    [[ "|$(ipc contextMenuEntries)|" == *"|Open in New Tab|"* && "|$(ipc contextMenuEntries)|" != *"|Copy Path|"* ]] \
        || fail "cap_menus2: the place-menu-only specimen drew $(ipc contextMenuEntries)"
    shot cap-menus2-place-only
    key -k Escape >/dev/null
    settle
    kill_flea
    seed_ui_state "$fixture_root/cap-menus2-place-copypath-state" "$(printf '{"keys":"default","view":"list","menu":{"hidden":["delete","runScript","moveto","copyto","properties","permissions","pasteAs","invertSelection","extThumbs"]},"places":{"favourites":[{"label":"Work","path":"%s/Work"}]}}' "$dir")"
    launch "$dir"
    wait_listing 4
    favourite_index=$(ipc railEntries | jq -r 'map(.label) | index("Work")')
    [[ -n "$favourite_index" && "$favourite_index" != "null" ]] \
        || fail "cap_menus2: the seeded favourite is not on the rail, which carries $(ipc railEntries)"
    click_rail_row "$favourite_index" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_menus2: the favourite's menu never opened with Copy path on"
    [[ "|$(ipc contextMenuEntries)|" == *"|Copy Path|"* && "|$(ipc contextMenuEntries)|" != *"|Copy As|"* ]] \
        || fail "cap_menus2: the Copy path specimen drew $(ipc contextMenuEntries)"
    shot cap-menus2-place-copypath
    key -k Escape >/dev/null
    settle
    printf 'CAP_MENUS2 makeexec=ok two-files=ok nohints=ok settings-tail=ok place=ok\n'
    kill_flea
}

# MenuAdditions040 states the first two menu cases leave out: board a as drawn (defaults plus Copy as and Paste as, a file on the clipboard, hints on, both flyouts), c and P at shipped defaults (the lone flyout), board f (Invert on, hints off, empty clipboard) and Show original's result.
case_cap_menus3() {
    local dir="$fixture_root/cap-menus3" hidden_board_a hidden_board_f copyas_leaves pasteas_leaves cursor_row polls clip_types
    local show_original_polls=100
    sandbox_scratch "$dir"
    mkdir -p "$dir/orig"
    printf 'target\n' > "$dir/orig/target.txt"
    printf 'plain\n' > "$dir/plain.txt"
    ln -s orig/target.txt "$dir/link.txt" || fail "cap_menus3: the symlink fixture could not be made"
    copyas_leaves='Path|Relative Path|Name|Name Without Extension|Folder Path|File URI|Shell-Quoted'
    pasteas_leaves='Link|Absolute Link|Hard Link'
    # DEFAULTS' twelve less Copy as and Paste as, which board a switches on; the other ten stay as shipped.
    hidden_board_a='"delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","invertSelection","extThumbs"'
    seed_ui_state "$fixture_root/cap-menus3-a-state" "$(printf '{"keys":"default","view":"list","keyHints":true,"menu":{"hidden":[%s]}}' "$hidden_board_a")"
    launch "$dir"
    wait_listing 3
    click_row "$(row_index_of plain.txt)" left
    settle
    key y >/dev/null
    settle
    [[ "$(ipc keyDeliveryState | jq -er '.clipboard.paths | length')" == "1" ]] \
        || fail "cap_menus3: y put no file on the clipboard for board a"
    click_row "$(row_index_of plain.txt)" right
    settle
    cap_menu_expect '.opened and .hasRow and ([.entries[] | select(.action == "copyAs" or .action == "pasteAs")] | length) == 2 and ([.entries[] | select(.action == "paste")][0].disabled == false)' "board a: the file menu shows Copy as and Paste as with a live Paste"
    [[ "$(ipc contextMenuHints | tr -d '|')" != "" ]] || fail "cap_menus3: board a drew no key hints"
    menu_seek "Copy As"
    key -k Right >/dev/null
    settle
    [[ "$(ipc contextMenuSubmenuEntries)" == "$copyas_leaves" ]] \
        || fail "cap_menus3: board a's Copy As flyout offers $(ipc contextMenuSubmenuEntries), want $copyas_leaves"
    shot cap-menus3-board-a-copyas
    key -k Escape >/dev/null
    settle
    menu_seek "Paste as"
    key -k Right >/dev/null
    settle
    [[ "$(ipc contextMenuSubmenuEntries)" == "$pasteas_leaves" ]] \
        || fail "cap_menus3: board a's Paste as flyout offers $(ipc contextMenuSubmenuEntries), want $pasteas_leaves"
    shot cap-menus3-board-a-pasteas
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_menus3: board a's menu stayed open"
    kill_flea
    seed_ui_state "$fixture_root/cap-menus3-lone-state" '{"keys":"default","view":"list","keyHints":true}'
    launch "$dir"
    wait_listing 3
    click_row "$(row_index_of plain.txt)" left
    settle
    key y >/dev/null
    settle
    [[ "$(ipc keyDeliveryState | jq -er '.clipboard.paths | length')" == "1" ]] \
        || fail "cap_menus3: y put no file on the clipboard for the lone Paste as flyout"
    key c >/dev/null
    settle
    cap_menu_expect '.opened and .submenu and ([.entries[] | select(.action == "copyAs")] | length) == 0 and ([.submenuEntries[].label] | join("|")) == "'"$copyas_leaves"'"' "c at defaults opens the lone Copy as flyout"
    shot cap-menus3-lone-copyas
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_menus3: the lone Copy As menu stayed open"
    key P >/dev/null
    settle
    cap_menu_expect '.opened and .submenu and ([.entries[] | select(.action == "pasteAs")] | length) == 0 and ([.submenuEntries[].label] | join("|")) == "'"$pasteas_leaves"'"' "P at defaults opens the lone Paste as flyout"
    shot cap-menus3-lone-pasteas
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    kill_flea
    # The y presses above left plain.txt on the session clipboard, so it is emptied for board f's dead Paste.
    wl-copy --clear || fail "cap_menus3: wl-copy could not empty the clipboard for board f"
    # wl-clipboard 2.3, minipc's, answers an empty clipboard with "Nothing is copied"; a connection failure prints something else.
    clip_types=$(wl-paste --list-types 2>&1)
    [[ -z "$clip_types" || "$clip_types" == "Nothing is copied" ]] \
        || fail "cap_menus3: wl-paste did not report an empty clipboard after wl-copy --clear, it printed: $clip_types"
    # DEFAULTS' twelve less Invert selection, which board f switches on, with hints off and nothing on the clipboard.
    hidden_board_f='"delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","extThumbs"'
    seed_ui_state "$fixture_root/cap-menus3-f-state" "$(printf '{"keys":"default","view":"list","keyHints":false,"menu":{"hidden":[%s]}}' "$hidden_board_f")"
    launch "$dir"
    wait_listing 3
    click_row "$(row_index_of plain.txt)" left
    settle
    [[ "$(ipc selectionCount)" == "1" ]] || fail "cap_menus3: board f needs one selected row, got $(ipc selectionCount)"
    [[ "$(ipc keyDeliveryState | jq -er '.clipboard.paths | length')" == "0" ]] \
        || fail "cap_menus3: board f needs an empty clipboard, got $(ipc keyDeliveryState)"
    click_background
    settle
    cap_menu_expect '.opened and (.hasRow | not) and ([.entries[] | select(.action == "invertSelection")] | length) == 1 and ([.entries[] | select(.action == "paste")][0].disabled == true)' "board f: Invert selection shows and Paste is dead"
    [[ "$(ipc contextMenuHints | tr -d '|')" == "" ]] || fail "cap_menus3: board f drew hints with them off: $(ipc contextMenuHints)"
    shot cap-menus3-board-f
    key -k Escape >/dev/null
    settle
    kill_flea
    seed_ui_state "$fixture_root/cap-menus3-original-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 3
    click_row "$(row_index_of link.txt)" right
    settle
    [[ "|$(ipc contextMenuEntries)|" == *"|Show Original|"* ]] \
        || fail "cap_menus3: the symlink menu offers no Show original, got $(ipc contextMenuEntries)"
    menu_seek "Show Original"
    key -k Return >/dev/null
    wait_path "$dir/orig"
    # The reveal lands the target's row as the sole selection once the listing of its folder arrives.
    for polls in $(seq 1 "$show_original_polls"); do
        cursor_row=$(ipc rowAt "$(ipc cursor)" 2>/dev/null || printf none)
        [[ "$cursor_row" == "target.txt|"* && "$(ipc selectionCount)" == "1" ]] && break
        sleep 0.05
    done
    [[ "$cursor_row" == "target.txt|"* && "$(ipc selectionCount)" == "1" ]] \
        || fail "cap_menus3: Show original left the cursor on $cursor_row with $(ipc selectionCount) selected, not target.txt alone"
    shot cap-menus3-show-original
    printf 'CAP_MENUS3 board-a=ok lone=ok board-f=ok show-original=ok\n'
    kill_flea
}

# Permissions040: Tab or Shift+Tab until the dialog's focused control has this name, within the whole ring of controls.
cap_permissions_focus() {
    local want="$1" direction="$2" state tabs
    local focus_limit=16
    for ((tabs = 0; tabs <= focus_limit; tabs++)); do
        state=$(ipc permissionsState) || fail "cap_permissions: the permissions reader failed"
        [[ "$(jq -r --arg want "$want" '[.controls[] | select(.focused and .name == $want)] | length' <<< "$state")" == "1" ]] && return 0
        if [[ "$direction" == back ]]; then key -M shift -k Tab -m shift >/dev/null; else key -k Tab >/dev/null; fi
        settle
    done
    fail "cap_permissions: Tab never reached $want, last $state"
}

# Permissions040: a focused check box in the state its bit holds, off, on or mixed ("some" in the control state).
cap_permissions_box() {
    local name="$1" value="$2" shot_name="$3"
    cap_permissions_focus "$name" forward
    [[ "$(ipc permissionsState | jq -r --arg want "$name" '[.controls[] | select(.name == $want)][0].value')" == "$value" ]] \
        || fail "cap_permissions: $name does not hold $value in the fixture"
    shot "$shot_name"
}

# Permissions040: one jq predicate over the permissions reader, asserted before a shot is taken.
cap_permissions_expect() {
    local filter="$1" message="$2" state
    state=$(ipc permissionsState) || fail "cap_permissions: the permissions reader failed"
    jq -e "$filter" <<< "$state" >/dev/null || fail "cap_permissions: $message, state $state"
}
# Permissions040: poll the permissions reader until one jq predicate holds, to a deadline, and name the last state when it never does.
cap_permissions_await() {
    local filter="$1" message="$2" state="" settle_limit_s=15 end
    end=$((SECONDS + settle_limit_s))
    while (( SECONDS < end )); do
        state=$(ipc permissionsState) || fail "cap_permissions: the permissions reader failed"
        jq -e "$filter" <<< "$state" >/dev/null && return 0
        sleep 0.05
    done
    fail "cap_permissions: $message, last state $state"
}
# Permissions040: wait until the card has closed, as a result arrives and Apply dismisses it.
cap_permissions_closed() {
    local end=$((SECONDS + 15))
    while (( SECONDS < end )); do
        [[ "$(ipc permissionsState | jq -r .opened)" == "false" ]] && return 0
        sleep 0.05
    done
    fail "cap_permissions: the card never closed after Apply"
}
# Permissions040 pointer states: a hover or a held press proven through ipc, the press let go off the control so nothing acts; an optional jq predicate holds before the shot.
cap_permissions_pointer() {
    local name="$1" mode="$2" shot_name="$3" expect="${4:-}" centre cx cy wx wy ww wh
    local settle_limit_s=5 away_px=150 nudge_px=1
    centre=$(ipc permissionsState | jq -er --arg name "$name" '.controls[] | select(.name == $name and .visible) | .centre') \
        || fail "cap_permissions: no visible $name control to point at"
    read -r cx cy <<< "$centre"
    read -r wx wy ww wh < <(window_box) || fail "cap_permissions: native window coordinates unavailable"
    assert_focus
    # Two moves so the first lands as the resting point, then a seat nudge there and back, since Hyprland's cursor move sends Qt no pointer frame.
    omarchy-drive move "$((wx + cx - nudge_px * 6))" "$((wy + cy))" >/dev/null || fail "cap_permissions: pointer approach to $name failed"
    omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null || fail "cap_permissions: pointer move onto $name failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x "$nudge_px" -y 0 >/dev/null 2>&1 || fail "cap_permissions: pointer nudge failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x "-$nudge_px" -y 0 >/dev/null 2>&1 || fail "cap_permissions: pointer nudge back failed"
    cap_permissions_wait_pointer "$name" hovered true "$settle_limit_s"
    if [[ "$mode" == hover ]]; then
        settle
        [[ -z "$expect" ]] || cap_permissions_expect "$expect" "$name hover state before $shot_name"
        shot "$shot_name"
        return 0
    fi
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 || fail "cap_permissions: pointer press on $name failed"
    cap_permissions_wait_pointer "$name" pressed true "$settle_limit_s"
    settle
    [[ -z "$expect" ]] || cap_permissions_expect "$expect" "$name press state before $shot_name"
    shot "$shot_name"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 0 -y "-$away_px" >/dev/null 2>&1 || fail "cap_permissions: pointer move off $name failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || fail "cap_permissions: pointer release failed"
    cap_permissions_wait_pointer "$name" pressed false "$settle_limit_s"
}
cap_permissions_wait_pointer() {
    local name="$1" field="$2" want="$3" limit="$4" end state
    end=$((SECONDS + limit))
    while (( SECONDS < end )); do
        state=$(ipc permissionsState) || fail "cap_permissions: the permissions reader failed"
        [[ "$(jq -r --arg name "$name" --arg field "$field" '[.controls[] | select(.name == $name)][0][$field]' <<< "$state")" == "$want" ]] && return 0
        sleep 0.05
    done
    fail "cap_permissions: $name never reported $field=$want, last $state"
}
# One pointer click on a grid box, asserted to land on the value the cycle names (mixed, on, off, mixed).
cap_permissions_click_box() {
    local name="$1" want="$2" shot_name="$3" centre end
    local settle_limit_s=5
    centre=$(ipc permissionsState | jq -er --arg name "$name" '.controls[] | select(.name == $name and .visible) | .centre') \
        || fail "cap_permissions: no visible $name box to click"
    permissions_click_at "$centre"
    end=$((SECONDS + settle_limit_s))
    while (( SECONDS < end )); do
        [[ "$(ipc permissionsState | jq -r --arg name "$name" '[.controls[] | select(.name == $name)][0].value')" == "$want" ]] && break
        sleep 0.05
    done
    [[ "$(ipc permissionsState | jq -r --arg name "$name" '[.controls[] | select(.name == $name)][0].value')" == "$want" ]] \
        || fail "cap_permissions: $name did not reach $want after the click"
    # The click leaves the pointer over the box, so the shot is the box as it reads after that click.
    settle
    shot "$shot_name"
}
permissions_click_at() {
    local centre="$1" cx cy wx wy ww wh
    read -r cx cy <<< "$centre"
    [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "cap_permissions: control has no centre"
    read -r wx wy ww wh < <(window_box) || fail "cap_permissions: native window coordinates unavailable"
    assert_focus
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null || fail "cap_permissions: pointer click failed"
}
# Sample output: live, disabled or absent; a live entry carries no disabled key at all, so a missing one reads live.
cap_permissions_makeexec_state() {
    ipc menuState | jq -r '[.entries[] | select(.action == "makeExecutable")][0] | if . == null then "absent" elif (.disabled // false) then "disabled" else "live" end'
}

# Permissions040 callout 3: the file menu on a shebang script at 0644 with Permissions unhidden offers Make executable beside its glyph.
cap_permissions_menu_specimen() {
    local entries="" end
    local settle_limit_s=15
    click_row 0 left
    settle
    click_row 4 right
    end=$((SECONDS + settle_limit_s))
    # The row goes live when the two-byte shebang read answers, so the wait is on the live row, not on its label.
    while (( SECONDS < end )); do
        entries=$(ipc contextMenuEntries)
        [[ "$entries" == *"Make Executable"* && "$(cap_permissions_makeexec_state)" == "live" ]] && break
        sleep 0.1
    done
    [[ "$entries" == *"Make Executable"* && "$entries" == *"Permissions"* ]] \
        || fail "cap_permissions: the shebang script's menu lacks Make Executable or Permissions, got $entries"
    [[ "$(cap_permissions_makeexec_state)" == "live" ]] \
        || fail "cap_permissions: Make Executable is not live on the shebang script, it reads $(cap_permissions_makeexec_state)"
    shot cap-permissions-makeexec-menu
    key -k Escape >/dev/null
    settle
}
# Permissions040: the single-item card on a setuid file keeps its values and dims every box to the disabled opacity.
cap_permissions_special_single() {
    local state
    click_row 0 left
    settle
    cap_permissions_open 5
    state=$(ipc permissionsState) || fail "cap_permissions: the permissions reader failed on the setuid file"
    shot cap-permissions-special-single
    [[ "$(jq -r '.displayedError' <<< "$state")" == "Read-only: setuid bit is present." ]] \
        || fail "cap_permissions: the setuid card names no reason, state $state"
    [[ "$(jq -r '[.controls[] | select(.bit != null and .enabled == false)] | length' <<< "$state")" == "9" ]] \
        || fail "cap_permissions: the setuid card leaves a box enabled, state $state"
    [[ "$(jq -r '[.controls[] | select(.bit != null and .value == "on")] | length' <<< "$state")" == "4" ]] \
        || fail "cap_permissions: the setuid card lost its 4644 values, state $state"
    key -k Escape >/dev/null
    settle
}

# Permissions040: from the file menu on the cursor row (right click), open the card and wait for it to settle idle.
cap_permissions_open() {
    local row="$1"
    click_row "$row" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_permissions: the menu on row $row never opened"
    [[ "$(ipc menuState | jq -er '[.entries[] | select(.action == "permissions")][0].disabled')" == "false" ]] \
        || fail "cap_permissions: Permissions is not live on row $row"
    menu_seek "Permissions"
    key -k Return >/dev/null
    cap_permissions_settled
}
# Permissions040: wait until the card is open and idle.
cap_permissions_settled() {
    local end state settle_limit_s=15
    end=$((SECONDS + settle_limit_s))
    while (( SECONDS < end )); do
        state=$(ipc permissionsState) || fail "cap_permissions: the permissions reader failed"
        [[ "$(jq -r .opened <<< "$state")" == "true" && "$(jq -r .busy <<< "$state")" == "false" ]] && return 0
        sleep 0.05
    done
    fail "cap_permissions: Permissions never settled open and idle, last $state"
}
# Permissions040: a selected file removed under the open card, so Apply fails on it, the card names the file in the backend's own words, and a second Apply changes the rest and names the one it left.
cap_permissions_vanished() {
    local row="$1" gone="$2" listed_after="$3"
    local want="zz-gone.txt keeps its mode: Could not inspect permissions: item not found."
    local left="zz-gone.txt kept its mode."
    [[ "$gone" == /?*/zz-gone.txt && "$gone" == "$fixture_root"/* ]] || fail "cap_permissions: the vanishing file is not inside the case's fixture"
    cap_permissions_open "$row"
    rm -f -- "$gone" || fail "cap_permissions: the fixture file could not be removed"
    # The watch refresh lands before Apply, so the batch is the only thing left to fail.
    wait_listing "$listed_after"
    cap_permissions_focus "Owner execute" forward
    key -k Space >/dev/null
    cap_permissions_await '[.controls[] | select(.name == "Owner execute")][0].value == "on"' "Owner execute did not turn on before Apply"
    cap_permissions_focus Apply forward
    key -k Return >/dev/null
    cap_permissions_await ".opened and (.busy | not) and .displayedError == \"$want\"" "the vanished file draws another line than the note naming it"
    shot cap-permissions-other-note
    cap_permissions_focus Apply forward
    key -k Return >/dev/null
    cap_permissions_closed
    [[ "$(ipc statusPrimary)" == "$left" ]] || fail "cap_permissions: the status line after the second Apply reads $(ipc statusPrimary), not $left"
    shot cap-permissions-other-reapply
}

# Permissions040: Apply on a selection that holds skips, the status line it leaves and the card that stays when every file is skipped, then a file this user does not own.
cap_permissions_skips() {
    local applied="special.txt kept its mode."
    local foreign="y-foreign.txt keeps its mode because you do not own it."
    local foreign_applied="y-foreign.txt kept its mode."
    local note="2 items keep their modes because a special bit is set: special.txt, x-special.txt."
    click_row 0 left
    settle
    click_row 5 left --mods ctrl
    settle
    cap_permissions_open 0
    cap_permissions_focus Apply forward
    key -k Return >/dev/null
    cap_permissions_closed
    [[ "$(ipc statusPrimary)" == "$applied" ]] || fail "cap_permissions: the status line after Apply reads $(ipc statusPrimary), not $applied"
    shot cap-permissions-applied-skip
    click_row 5 left
    settle
    click_row 6 left --mods ctrl
    settle
    cap_permissions_open 5
    cap_permissions_expect ".displayedError == \"$note\"" "the two setuid files draw another note than the card note"
    # Permissions040: a box shows the files' bit, so two 4644 files read rw-r--r-- at the disabled opacity and Apply cannot be pressed.
    cap_permissions_expect '([.controls[] | select(.bit != null and .enabled == false)] | length == 9) and ([.controls[] | select(.bit != null and .value == "on")] | length == 4) and ([.controls[] | select(.name == "Apply")][0] | .enabled | not)' "the all-skipped card is not nine disabled boxes holding the files' rw-r--r-- with Apply disabled"
    shot cap-permissions-all-skipped-note
    key -k Escape >/dev/null
    settle
    click_row 0 left
    settle
    click_row 7 left --mods ctrl
    settle
    cap_permissions_open 0
    cap_permissions_expect ".displayedError == \"$foreign\"" "the foreign file draws no ownership note"
    shot cap-permissions-multi-note-foreign
    cap_permissions_focus Apply forward
    key -k Return >/dev/null
    cap_permissions_closed
    [[ "$(ipc statusPrimary)" == "$foreign_applied" ]] || fail "cap_permissions: the status line after Apply reads $(ipc statusPrimary) beside a foreign file"
    shot cap-permissions-applied-foreign
}
# Permissions040: Apply held in flight by a paused owned backend leaves Cancel and the close mark disabled, then the backend resumes.
cap_permissions_inflight() {
    click_row 1 left
    settle
    cap_permissions_open 1
    mapfile -t pids < <(backend_pids)
    [[ "${#pids[@]}" == 1 ]] || fail "cap_permissions: the in-flight shot needs one owned backend"
    pid="${pids[0]}"
    permissions_stopped="$pid"
    # Each case runs in a subshell whose EXIT trap is cleared (tests/ui.sh:14560), so this trap owns EXIT.
    trap 'permissions_resume_stopped "$permissions_stopped"; kill_flea' EXIT
    convert_pause_backend "$pid"
    cap_permissions_focus Apply forward
    key -k Return >/dev/null
    cap_permissions_await '.opened and .busy and ([.controls[] | select(.name == "Cancel" or .name == "Close")] | length == 2 and all(.enabled | not))' "Cancel and the close mark stay live while Apply is in flight"
    settle
    shot cap-permissions-apply-inflight
    permissions_resume_stopped "$pid" || fail "cap_permissions: the owned backend did not resume"
    permissions_stopped=""
    trap - EXIT
    cap_permissions_closed
}

# Permissions040: the several-items card with a focused check box in each state, the single-item card with an invalid octal, the errored symlink row and the note.
case_cap_permissions() {
    local dir="$fixture_root/cap-permissions" permissions_listing="$fixture_root/cap-permissions" state pid permissions_stopped=""
    local -a pids
    sandbox_scratch "$dir"
    printf 'one\n' > "$dir/a.txt"
    printf 'two\n' > "$dir/b.txt"
    printf 'three\n' > "$dir/c.txt"
    printf 'special\n' > "$dir/special.txt"
    printf 'second special\n' > "$dir/x-special.txt"
    printf 'foreign\n' > "$dir/y-foreign.txt"
    printf 'gone\n' > "$dir/zz-gone.txt"
    printf '#!/bin/sh\necho run\n' > "$dir/run.sh"
    chmod 0644 "$dir/run.sh" || fail "cap_permissions: the shebang fixture mode failed"
    ln -s a.txt "$dir/link.txt" || fail "cap_permissions: the symlink fixture failed"
    chmod 0644 "$dir/a.txt" || fail "cap_permissions: the 644 fixture mode failed"
    chmod 0600 "$dir/b.txt" || fail "cap_permissions: the 600 fixture mode failed"
    chmod 0755 "$dir/c.txt" || fail "cap_permissions: the 755 fixture mode failed"
    chmod 4644 "$dir/special.txt" "$dir/x-special.txt" || fail "cap_permissions: the setuid fixture mode failed"
    chmod 0644 "$dir/y-foreign.txt" || fail "cap_permissions: the foreign fixture mode failed"
    # A file another uid owns, made inside this fixture by a rootless user namespace: its owner maps to a sub-uid, so this user cannot change its mode.
    unshare --map-auto --map-root-user chown 1:1 "$dir/y-foreign.txt" || fail "cap_permissions: unshare could not give the foreign fixture another owner"
    [[ "$(stat -c '%u' "$dir/y-foreign.txt")" != "$(id -u)" ]] || fail "cap_permissions: the foreign fixture is still owned by this user"
    seed_ui_state "$fixture_root/cap-permissions-state" '{"keys":"default","view":"list","menu":{"hidden":["delete","openTerminal","moveto","copyto","properties","copyAs","pasteAs","invertSelection"]}}'
    launch "$dir"
    wait_listing 9
    cap_resize 904 699
    click_row 0 left
    settle
    click_row 1 left --mods ctrl
    settle
    click_row 2 left --mods ctrl
    settle
    [[ "$(ipc selectionCount)" == "3" ]] || fail "cap_permissions: three ctrl clicks selected $(ipc selectionCount), not 3"
    cap_permissions_open 0
    shot cap-permissions-multi
    # Tab from Cancel runs Apply, Close, then the grid: Owner read is on, Owner execute mixed, Group write off.
    cap_permissions_box "Owner read" on cap-permissions-box-on
    cap_permissions_box "Owner execute" some cap-permissions-box-mixed
    cap_permissions_box "Group write" off cap-permissions-box-off
    cap_permissions_pointer "Owner read" hover cap-permissions-box-hover
    cap_permissions_pointer "Owner read" press cap-permissions-box-pressed
    # Mixed, one click on, a second click off: the Owner execute bit differs across the three files.
    cap_permissions_click_box "Owner execute" on cap-permissions-box-mixed-click1
    cap_permissions_click_box "Owner execute" off cap-permissions-box-mixed-click2
    key -k Escape >/dev/null
    settle
    click_row 0 left
    settle
    cap_permissions_open 0
    cap_permissions_focus Apply forward
    shot cap-permissions-apply-focus
    # The close mark left the title (HIG permissions-close, APL-DIALOG-33): Cancel and esc dismiss the card.
    cap_permissions_expect '[.controls[] | select(.name == "Close")] | length == 0' "the card still lists a close mark"
    cap_permissions_focus Octal back
    shot cap-permissions-octal-focus
    key -M ctrl -k a -m ctrl -k 9 >/dev/null
    settle
    [[ "$(ipc permissionsState | jq -r '.displayedError | length')" != "0" ]] || fail "cap_permissions: an invalid octal drew no error"
    shot cap-permissions-octal-error
    cap_permissions_focus Cancel forward
    state=$(ipc permissionsState) || fail "cap_permissions: the permissions reader failed before the disabled-Apply shot"
    [[ "$(jq -r '[.controls[] | select(.name == "Apply")][0].enabled' <<< "$state")" == "false" ]] \
        || fail "cap_permissions: Apply is not disabled over the invalid octal, state $state"
    [[ "$(jq -r '.mode | test("9")' <<< "$state")" == "true" ]] \
        || fail "cap_permissions: the invalid octal left the field before the disabled-Apply shot, mode $(jq -r .mode <<< "$state")"
    shot cap-permissions-apply-disabled
    cap_permissions_pointer Cancel hover cap-permissions-cancel-hover
    cap_permissions_pointer Cancel press cap-permissions-cancel-pressed
    key -k Escape >/dev/null
    settle
    # Apply is live over a valid octal, so its hover and press are shot on a fresh card.
    click_row 0 left
    settle
    cap_permissions_open 0
    cap_permissions_pointer Apply hover cap-permissions-apply-hover
    cap_permissions_pointer Apply press cap-permissions-apply-pressed
    # The keyboard moves to Apply, so Cancel is shot hovered and pressed with no ring of its own.
    cap_permissions_focus Apply forward
    cap_permissions_pointer Cancel hover cap-permissions-cancel-hover-apply-focus '([.controls[] | select(.name == "Apply")][0].focused) and ([.controls[] | select(.name == "Cancel")][0] | .hovered and (.focused | not))'
    cap_permissions_pointer Cancel press cap-permissions-cancel-pressed-apply-focus '([.controls[] | select(.name == "Apply")][0].focused) and ([.controls[] | select(.name == "Cancel")][0] | .pressed and (.focused | not))'
    key -k Escape >/dev/null
    settle
    click_row 3 right
    settle
    [[ "$(ipc menuState | jq -er '[.entries[] | select(.action == "permissions")][0].disabled')" == "true" ]] \
        || fail "cap_permissions: Permissions is not the errored row on a symlink"
    shot cap-permissions-symlink-menu
    key -k Escape >/dev/null
    settle
    cap_permissions_menu_specimen
    cap_permissions_special_single
    click_row 0 left
    settle
    click_row 5 left --mods ctrl
    settle
    [[ "$(ipc selectionCount)" == "2" ]] || fail "cap_permissions: the setuid pair selected $(ipc selectionCount), not 2"
    cap_permissions_open 0
    [[ "$(ipc permissionsState | jq -r '.displayedError | length')" != "0" ]] || fail "cap_permissions: the setuid file drew no note for several items"
    shot cap-permissions-multi-note
    key -k Escape >/dev/null
    settle
    cap_permissions_skips
    click_row 0 left
    settle
    click_row 8 left --mods ctrl
    settle
    cap_permissions_vanished 0 "$dir/zz-gone.txt" 8
    cap_permissions_inflight
    printf 'CAP_PERMISSIONS mixed=3rows boxes=on,mixed,off,hover,pressed,click1,click2 single=apply,octal,error,disabled,close,special,pointer symlink=errored note=setuid menu=makeexec skips=applied,note,foreign other=note inflight=disabled closemark=absent\n'
    kill_flea
}

# PropertiesCount: Format.js size() for a byte total, round half up on the double's exact value as toFixed does; the HIG boards group the byte count (2,560,562 bytes).
cap_propcount_size() {
    python3 -I - "$1" <<'EOF2' || fail "cap_propcount: the expected size did not derive from $1"
import sys
from decimal import Decimal, ROUND_HALF_UP
b = int(sys.argv[1])
units = ["B", "kB", "MB", "GB", "TB"]
if b < 1000:
    print("%d B (%d bytes)" % (b, b))
else:
    v, u = float(b), 0
    while v >= 1000 and u < 4:
        v, u = v / 1000, u + 1
    print("%s %s (%s bytes)" % (Decimal(v).quantize(Decimal("0.1"), rounding=ROUND_HALF_UP), units[u], format(b, ",")))
EOF2
}

# PropertiesCount: Properties on a folder of known contents shows the exact size and item count once its walk is done.
case_cap_properties_count() {
    local dir="$fixture_root/cap-propcount" counted entry_bytes total_bytes total_items want_size want_items state shown_size shown_items end
    local settle_limit_s=15
    counted="$dir/counted"
    sandbox_scratch "$dir"
    mkdir -p "$counted/sub" || fail "cap_propcount: the fixture folders failed"
    head -c 1000 /dev/zero > "$counted/a.bin"
    head -c 2500 /dev/zero > "$counted/b.bin"
    head -c 40 /dev/zero > "$counted/c.bin"
    head -c 700 /dev/zero > "$counted/sub/d.bin"
    head -c 1 /dev/zero > "$counted/sub/e.bin"
    # The walk counts the folder's own entry and every entry below it by lstat size, and the items below it alone.
    total_bytes=0
    while IFS= read -r entry_bytes; do
        total_bytes=$((total_bytes + entry_bytes))
    done < <(find "$counted" -printf '%s\n')
    total_items=$(find "$counted" -mindepth 1 | wc -l)
    [[ "$total_items" == "6" ]] || fail "cap_propcount: the fixture holds $total_items entries, not 6"
    want_size=$(cap_propcount_size "$total_bytes") || fail "cap_propcount: the expected size did not derive from $total_bytes"
    want_items="6 items"
    seed_ui_state "$fixture_root/cap-propcount-state" '{"keys":"default","view":"list","menu":{"hidden":["delete","openTerminal","moveto","copyto","permissions","copyAs","pasteAs","invertSelection"]}}'
    launch "$dir"
    wait_listing 1
    click_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_propcount: the folder menu never opened"
    menu_seek "Properties"
    key -k Return >/dev/null
    # The count is done when the Contains line has lost its counting tail; until the reply the card has no lines at all.
    end=$((SECONDS + settle_limit_s))
    state=""
    while (( SECONDS < end )); do
        state=$(ipc menuDialogState) || fail "cap_propcount: the dialog reader failed"
        jq -e '.opened and .action == "properties" and .facts.directory == true and .liveLines.contains.tail == ""' <<< "$state" >/dev/null && break
        sleep 0.05
    done
    jq -e '.opened and .action == "properties" and .facts.directory == true and .liveLines.contains.tail == ""' <<< "$state" >/dev/null \
        || fail "cap_propcount: the count never finished inside ${settle_limit_s} s: $state"
    jq -e '.liveLines.size.value and .liveLines.contains.value' <<< "$state" >/dev/null \
        || fail "cap_propcount: the card never finished counting, or menuDialogState carries no liveLines, last state $state"
    shown_size=$(jq -er '.liveLines.size.value' <<< "$state")
    shown_items=$(jq -er '.liveLines.contains.value' <<< "$state")
    shot cap-properties-count
    printf 'CAP_PROPERTIES_COUNT size=%s items=%s expected=%s,%s\n' "$shown_size" "$shown_items" "$want_size" "$want_items"
    [[ "$shown_size" == "$want_size" ]] || fail "cap_propcount: Size reads [$shown_size], the fixture holds [$want_size]"
    [[ "$shown_items" == "$want_items" ]] || fail "cap_propcount: Contains reads [$shown_items], the fixture holds [$want_items]"
    key -k Escape >/dev/null
    kill_flea
}

# Sidebar040: Settings Places at defaults with two favourites, Recent switched on and listed as the current place, then a favourites reorder held mid-drag; the Flea home and its recently-used.xbel are the fixture's own, so paths read home-relative as the board writes them.
case_cap_sidebar() {
    local home="$fixture_root/cap-sidebar-home"
    local dir="$home/Documents/claude" downloads="$home/Downloads" name
    fixture_home_make "$home"
    mkdir -p "$dir/alpha" "$dir/beta" "$downloads" "$home/Music" "$home/Pictures" "$home/Videos" "$home/.local/share"
    for name in field-bench-notes.md screenshot-2026-08-30.png panel-demo.mp4 mix.flac backup.tar.zst; do
        printf 'x\n' > "$dir/$name"
    done
    printf 'x\n' > "$downloads/receipt.pdf"
    # Newest visited first, the order and the stamps Sidebar040's Recent specimen lists.
    cat > "$home/.local/share/recently-used.xbel" <<EOS
<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0">
  <bookmark href="file://$dir/field-bench-notes.md" visited="2026-09-23T10:47:00Z"/>
  <bookmark href="file://$dir/screenshot-2026-08-30.png" visited="2026-09-23T09:12:00Z"/>
  <bookmark href="file://$dir/panel-demo.mp4" visited="2026-09-22T19:27:00Z"/>
  <bookmark href="file://$downloads/receipt.pdf" visited="2026-09-22T08:15:00Z"/>
  <bookmark href="file://$dir/mix.flac" visited="2026-09-21T12:02:00Z"/>
  <bookmark href="file://$dir/backup.tar.zst" visited="2026-09-18T21:05:00Z"/>
</xbel>
EOS
    seed_ui_state "$fixture_root/cap-sidebar-state" "$(printf '{"keys":"default","view":"list","places":{"favourites":[{"label":"Alpha","path":"%s/alpha"},{"label":"Beta","path":"%s/beta"}]}}' "$dir" "$dir")"
    local real_home="$HOME" real_data="${XDG_DATA_HOME-}"
    # An earlier case may have exported its own data home, which would hide this case's history.
    export HOME="$home" XDG_DATA_HOME="$home/.local/share"
    launch "$dir"
    export HOME="$real_home"
    if [[ -n "$real_data" ]]; then export XDG_DATA_HOME="$real_data"; else unset XDG_DATA_HOME; fi
    wait_listing 7
    cap_resize 1120 543
    # cap_resize returns once the size is right while the centring may still be moving the window, so the box must hold still across two reads before anything is shot.
    local box previous="" end=$((SECONDS + 20))
    while (( SECONDS < end )); do
        box=$(window_box) || fail "cap_sidebar: native window coordinates unavailable"
        [[ "$box" == "$previous" ]] && break
        previous="$box"
        sleep 0.2
    done
    [[ "$box" == "$previous" && "$box" == *" 1120 543" ]] || fail "cap_sidebar: the window never held still at 1120x543, last box [$box]"
    wait_rail_label "Alpha"
    wait_rail_label "Beta"
    ipc railEntries | jq -e 'all(.[]; .group != "recent")' >/dev/null \
        || fail "cap_sidebar: Recent is on at defaults"
    settings_open_key
    settle
    settings_section places
    [[ "$(ipc settingsRows)" == *"Recent"* ]] || fail "cap_sidebar: Places lists no Recent row"
    [[ "$(ipc settingsRows)" == *"$dir/alpha"* ]] || fail "cap_sidebar: Places lists no Alpha favourite, got $(ipc settingsRows)"
    shot cap-sidebar-places
    settings_click_control "places.showRecent"
    settings_wait_value ".places.showRecent == true"
    key -k Escape >/dev/null
    settle
    wait_rail_label "Recent"
    click_rail_row "$(rail_row_of "Recent")" left
    local mode="" total=""
    for _attempt in $(seq 1 200); do
        mode=$(ipc recentMode 2>/dev/null || printf unavailable)
        total=$(ipc total 2>/dev/null || printf unavailable)
        [[ "$mode" == "results" && "$total" == "6" && "$(ipc listInFlight 2>/dev/null)" == "false" ]] && break
        sleep 0.05
    done
    local inflight
    inflight=$(ipc listInFlight 2>/dev/null || printf unavailable)
    [[ "$mode" == "results" && "$total" == "6" && "$inflight" == "false" ]] \
        || fail "cap_sidebar: clicking Recent listed mode [$mode] total [$total] in flight [$inflight], not results, 6 and settled"
    [[ "$(ipc headerTitles)" == "Name|Location|Size|Used" ]] \
        || fail "cap_sidebar: the Recent header reads $(ipc headerTitles)"
    [[ "$(ipc railCursor)" == "$(rail_row_of "Recent")" ]] || fail "cap_sidebar: Recent is not the lit rail row"
    shot cap-sidebar-recent
    local alpha_index beta_index ax ay bx by before_alpha before_beta after_alpha after_beta
    alpha_index=$(rail_row_of "Alpha")
    beta_index=$(rail_row_of "Beta")
    (( alpha_index < beta_index )) || fail "cap_sidebar: favourites order is not Alpha then Beta"
    read -r ax ay <<< "$(ipc railRowCentre "$alpha_index")"
    read -r bx by <<< "$(ipc railRowCentre "$beta_index")"
    [[ -n "$ay" && -n "$by" ]] || fail "cap_sidebar: a favourite has no rail centre"
    before_alpha="$alpha_index"
    tabdrag_to "$ax" "$ay" "$bx" "$by" cap-sidebar-drag-held
    alpha_index=$(rail_row_of "Alpha")
    beta_index=$(rail_row_of "Beta")
    after_alpha="$alpha_index"
    after_beta="$beta_index"
    (( after_beta < after_alpha )) || fail "cap_sidebar: the held drag never reordered, Alpha at $after_alpha Beta at $after_beta (was $before_alpha)"
    printf 'CAP_SIDEBAR places=defaults recent=current drag=held reorder=ok\n'
    kill_flea
}

# How long the sheet or the Permissions dialog may take to read open or closed after a key.
cap_sheet_wait_s=10

# Types a word into the open keymap sheet one key at a time, then waits for the query to read back whole.
cap_sheet_type() {
    local word="$1" i
    for ((i = 0; i < ${#word}; i++)); do
        key "${word:i:1}" >/dev/null
    done
    flea_ipc_wait -p "$flea_ui/boot" flea keymapQuery "$word" --timeout "$cap_sheet_wait_s" >/dev/null \
        || fail "cap_sheet: the query never reached $word, it is '$(ipc keymapQuery)'"
    [[ "$(ipc keymapQuery)" == "$word" ]] || fail "cap_sheet: the query is '$(ipc keymapQuery)', not $word"
}

# Waits until the sheet lists the given row, so a query's async section is read after it lands.
cap_sheet_rows_hold() {
    local row="$1" what="$2" end
    end=$((SECONDS + cap_sheet_wait_s))
    while (( SECONDS < end )); do
        grep -Fxq "$row" <<< "$(ipc keymapSheetRows)" && return 0
        settle
    done
    fail "cap_sheet: $what: $(ipc keymapSheetRows | tr '\n' '|')"
}

# Waits until a reader (or its jq field) answers the exact word, or fails naming the last value.
cap_sheet_expect() {
    local reader="$1" want="$2" what="$3" field="${4:-.}" end got=""
    end=$((SECONDS + cap_sheet_wait_s))
    while (( SECONDS < end )); do
        got=$(ipc "$reader") || fail "cap_sheet: $reader failed $what"
        [[ "$field" == . ]] || got=$(jq -r "$field" <<< "$got")
        [[ "$got" == "$want" ]] && return 0
        settle
    done
    fail "cap_sheet: $what, last value '$got'"
}

# Waits until the result row with the given label draws the given muted where, the text its delegate shows beside the name.
cap_sheet_where() {
    local label="$1" where="$2" what="$3"
    cap_sheet_expect keymapSheetResults "$where" "$what" "[.[] | select(.label == \"$label\")][0].where // \"none\""
}

# CommandPalette: the sheet at rest, then its query with a place, a place beside a recent file, leaves alone, a key that works in one place, the cursor on a file, and the delete card.
case_cap_sheet() {
    local dir="$fixture_root/cap-sheet"
    local places="$fixture_root/cap-sheet-places"
    local fixture_home="$fixture_root/cap-sheet-home" real_home="$HOME" real_data="${XDG_DATA_HOME-}"
    local sheet_rows flea_at mix_at
    sandbox_scratch "$dir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    # fixture_home_make wipes the home it makes, so the history and the recent file go in after it.
    fixture_home_make "$fixture_home"
    # The favourite flea and the recent file mix.flac stand in the case's own fixture home, so no other bookmark moves the rows.
    mkdir -p "$places/flea" "$fixture_home/Documents/claude" "$fixture_home/.local/share"
    : > "$fixture_home/Documents/claude/mix.flac"
    cat > "$fixture_home/.local/share/recently-used.xbel" <<EOS
<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0">
  <bookmark href="file://$fixture_home/Documents/claude/mix.flac" added="2026-09-26T10:00:00Z" modified="2026-09-26T10:00:00Z" visited="2026-09-26T10:00:00Z"/>
</xbel>
EOS
    seed_ui_state "$fixture_root/cap-sheet-state" "$(printf '{"keys":"default","view":"list","places":{"favourites":[{"label":"flea","path":"%s/flea"}]}}' "$places")"
    export HOME="$fixture_home" XDG_DATA_HOME="$fixture_home/.local/share"
    launch "$dir"
    export HOME="$real_home"
    if [[ -n "$real_data" ]]; then export XDG_DATA_HOME="$real_data"; else unset XDG_DATA_HOME; fi
    wait_listing 2
    key '?' >/dev/null
    flea_ipc_wait -p "$flea_ui/boot" flea keymapSheetOpen true --timeout "$cap_sheet_wait_s" >/dev/null \
        || fail "cap_sheet: ? opened no keymap sheet"
    [[ "$(ipc keymapSheetOpen)" == "true" ]] || fail "cap_sheet: ? opened no keymap sheet"
    [[ "$(ipc keymapQuery)" == "" ]] || fail "cap_sheet: the resting sheet carries query '$(ipc keymapQuery)'"
    shot cap-sheet-rest
    # A place named exactly ranks first, and the file menu's Move to Trash lists once under its key.
    cap_sheet_type trash
    sheet_rows=$(ipc keymapSheetRows)
    [[ "$(head -n 1 <<< "$sheet_rows")" == ' Open Trash' ]] \
        || fail "cap_sheet: the trash query does not lead with the Trash place: ${sheet_rows//$'\n'/ | }"
    shot cap-sheet-query-trash
    key -k Escape >/dev/null
    cap_sheet_expect keymapSheetOpen false "Escape did not close the sheet after trash"
    # A favourite and a recent file answer one query, the place first, each with its muted where.
    key '?' >/dev/null
    cap_sheet_expect keymapSheetOpen true "? did not reopen the sheet"
    cap_sheet_type fl
    # The recent file arrives from an async xbel read the first key starts, so the rows are read once it has landed.
    cap_sheet_rows_hold ' Open mix.flac' "the fl query lists no recent file mix.flac"
    sheet_rows=$(ipc keymapSheetRows)
    grep -Fxq ' Open flea' <<< "$sheet_rows" \
        || fail "cap_sheet: the fl query lists no favourite flea: ${sheet_rows//$'\n'/ | }"
    flea_at=$(grep -Fxn ' Open flea' <<< "$sheet_rows" | head -n 1 | cut -d: -f1)
    mix_at=$(grep -Fxn ' Open mix.flac' <<< "$sheet_rows" | head -n 1 | cut -d: -f1)
    (( flea_at < mix_at )) || fail "cap_sheet: the fl query does not lead with the favourite: ${sheet_rows//$'\n'/ | }"
    # The where reads "in Favorites" as the sheet draws it (settings.sheet.in, g2/st1/sheet_view.rs); no HIG board draws it.
    cap_sheet_where "Open flea" "in Favorites" "the favourite flea draws no where Favorites"
    cap_sheet_where "Open mix.flac" "in ~/Documents/claude" "the recent mix.flac draws no where ~/Documents/claude"
    shot cap-sheet-query-fl
    key -k Escape >/dev/null
    cap_sheet_expect keymapSheetOpen false "Escape did not close the sheet after fl"
    # The cursor row's Compress flyout answers with its leaves alone, the zip leaf holding the cursor and no row with a cap.
    key '?' >/dev/null
    cap_sheet_expect keymapSheetOpen true "? did not reopen the sheet"
    cap_sheet_type comp
    sheet_rows=$(ipc keymapSheetRows)
    [[ "$(head -n 1 <<< "$sheet_rows")" == ' Compress to .zip' ]] \
        || fail "cap_sheet: the comp query lists no Compress to .zip leaf row first: ${sheet_rows//$'\n'/ | }"
    ! grep -Fxq ' Compress' <<< "$sheet_rows" || fail "cap_sheet: the comp query lists the Compress parent: ${sheet_rows//$'\n'/ | }"
    ! grep -q '^[^ ]' <<< "$sheet_rows" || fail "cap_sheet: the comp query lists a row with a cap: ${sheet_rows//$'\n'/ | }"
    shot cap-sheet-query-comp
    key -k Escape >/dev/null
    cap_sheet_expect keymapSheetOpen false "Escape did not close the sheet after comp"
    # A key that works in one place only lists its action row, and the sheet draws the muted where beside it.
    key '?' >/dev/null
    cap_sheet_expect keymapSheetOpen true "? did not reopen the sheet"
    cap_sheet_type mute
    cap_sheet_rows_hold 'm mute' "the mute query lists no m mute row"
    cap_sheet_where mute "in Preview" "the mute row draws no where Preview"
    shot cap-sheet-query-mute
    key -k Escape >/dev/null
    cap_sheet_expect keymapSheetOpen false "Escape did not close the sheet after mute"
    # The cursor stays on a.txt: Delete permanently lists once under its key, and Permissions is a live row.
    key '?' >/dev/null
    cap_sheet_expect keymapSheetOpen true "? did not reopen the sheet"
    cap_sheet_type perm
    sheet_rows=$(ipc keymapSheetRows)
    grep -Fxq 'shift-delete delete permanently' <<< "$sheet_rows" \
        || fail "cap_sheet: the perm query lists no delete permanently row: ${sheet_rows//$'\n'/ | }"
    [[ "$(grep -ci 'delete permanently' <<< "$sheet_rows")" == 1 ]] \
        || fail "cap_sheet: delete permanently is listed more than once: ${sheet_rows//$'\n'/ | }"
    grep -Fxq ' Permissions' <<< "$sheet_rows" \
        || fail "cap_sheet: Permissions reads unavailable with the cursor on a file: ${sheet_rows//$'\n'/ | }"
    shot cap-sheet-query
    # The cursor opens on row one and Down moves it one row, so Permissions must be row two or the Return below runs another row.
    [[ "$(sed -n 2p <<< "$sheet_rows")" == ' Permissions' ]] \
        || fail "cap_sheet: the second perm row is not Permissions: ${sheet_rows//$'\n'/ | }"
    key -k Down >/dev/null
    shot cap-sheet-query-perm-file
    key -k Return >/dev/null
    cap_sheet_expect keymapSheetOpen false "Enter on Permissions did not close the sheet"
    cap_sheet_expect permissionsState true "Enter on Permissions opened no dialog" .opened
    key -k Escape >/dev/null
    cap_sheet_expect permissionsState false "Escape did not close the Permissions dialog" .opened
    # Enter on the first perm row, Delete permanently, opens the menu's own card with Cancel's ring; Tab moves the ring to Delete and Escape deletes nothing.
    key '?' >/dev/null
    cap_sheet_expect keymapSheetOpen true "? did not reopen the sheet"
    cap_sheet_type perm
    key -k Return >/dev/null
    cap_sheet_expect menuDialogState true "Enter on Delete permanently opened no card" .confirmation.opened
    [[ "$(ipc menuDialogState | jq -r '.confirmation.count')|$(ipc menuDialogState | jq -r '.confirmation.title')" == '1|Delete 1 item permanently?' ]] \
        || fail "cap_sheet: the delete card does not ask about 1 item: $(ipc menuDialogState | jq -c '.confirmation | {count, title}')"
    [[ "$(ipc menuDialogState | jq -r '.confirmation.destructiveFocus')" == false ]] \
        || fail "cap_sheet: the delete card opens with its ring off Cancel"
    shot cap-sheet-delete-card
    key -k Tab >/dev/null
    cap_sheet_expect menuDialogState true "Tab did not move the delete card's focus to Delete" .confirmation.destructiveFocus
    shot cap-sheet-delete-tab
    key -k Escape >/dev/null
    cap_sheet_expect menuDialogState false "Escape did not close the delete card" .confirmation.opened
    [[ -e "$dir/a.txt" ]] || fail "cap_sheet: Escape on the delete card still deleted a.txt"
    printf 'CAP_SHEET rest=ok queries=trash,fl,comp,mute,perm permissions=opened delete=cancel-then-delete\n'
    kill_flea
}

# The current matrix cell's drawn picture box, "x y width height", from the frontend under test.
matrix_media_rect() {
    if [[ "$test_frontend" == gpui ]]; then
        ipc previewStrip | jq -er '.mediaRect | select(length > 0)'
    else
        ipc previewSurfaceRect
    fi
}

# Aspect rule check for one matrix cell: drawn picture against ownsize, fit or fill.
matrix_check() {
    local label="$1" frame="$2" picture="$3" src="$4" mode="$5" inset="$6"
    python3 - "$label" "$frame" "$picture" "$src" "$mode" "$inset" <<'PYEOF' || fail "previewmatrix: $label drew outside its rule"
import sys
label, frame, picture, src, mode, inset = sys.argv[1:7]
inset = float(inset)
fx, fy, fw, fh = [float(v) for v in frame.split()]
px, py, pw, ph = [float(v) for v in picture.split()]
sw, sh = [float(v) for v in src.split()]
if mode == "ownsize":
    want_w, want_h = sw, sh
else:
    # Fit keeps own pixels and fill enlarges a small clip; both draw the aspect-fit of the box.
    scale = min((fw - inset) / sw, (fh - inset) / sh)
    want_w, want_h = sw * scale, sh * scale
assert abs(pw - want_w) <= 2, "width %s, rule wants %s" % (pw, want_w)
assert abs(ph - want_h) <= 2, "height %s, rule wants %s" % (ph, want_h)
assert abs(2 * px + pw - (2 * fx + fw)) <= 4, "not centred horizontally"
assert abs(2 * py + ph - (2 * fy + fh)) <= 4, "not centred vertically"
print("PREVIEWMATRIX %s frame=%sx%s drawn=%sx%s rule=%s ok" % (label, fw, fh, pw, ph, mode))
PYEOF
}

# A column picture that is decoded and on screen: the state, the shown mark and the ready frame.
# Sample input, previewSelectionState: {"view":"columns","index":2,"path":"/fixture/b-large.jpg"}.
matrix_wait_column() {
    local want="$1" file="$2" state path
    local end=$((SECONDS + 10))
    # Same-kind seeks land on the previous file's Ready frame first, so wait for this file's path.
    while (( SECONDS < end )); do
        path=$(ipc previewSelectionState | jq -r .path)
        [[ "$path" == *"$file" ]] && break
        sleep 0.1
    done
    [[ "$path" == *"$file" ]] \
        || fail "previewmatrix: the column still shows $path, not $file"
    end=$((SECONDS + 25))
    while (( SECONDS < end )); do
        state=$(ipc previewColumnState)
        [[ "$state" == "$want" && "$(ipc columnThumbShown)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$state" == "$want" && "$(ipc columnThumbShown)" == "true" ]] \
        || fail "previewmatrix: the column shows $state, thumb shown $(ipc columnThumbShown), not $want"
    end=$((SECONDS + 10))
    while (( SECONDS < end )); do
        [[ "$(ipc columnFrameReady)" == "true" ]] && return 0
        sleep 0.1
    done
    fail "previewmatrix: the column frame never read Ready"
}

# Quick Look ownsize needs a surface larger than the source, or the rule misjudges on a small tile.
# Sample input, previewSurfaceRect: 240 151 2080 1137.
matrix_require_surface() {
    local min_width="$1" min_height="$2" rect sw sh
    rect=$(ipc previewSurfaceRect) || fail "previewmatrix: the overlay surface never reported"
    [[ "$rect" =~ ^-?[0-9]+\ -?[0-9]+\ [0-9]+\ [0-9]+$ ]] \
        || fail "previewmatrix: the overlay surface has no valid rectangle: $rect"
    read -r _ _ sw sh <<< "$rect"
    (( sw > min_width && sh > min_height )) \
        || fail "previewmatrix: surface ${sw}x${sh} cannot carry the ${min_width}x${min_height} ownsize cell"
}

matrix_click_play() {
    local cx cy wx wy
    read -r cx cy <<< "$(ipc columnPlayCentre)"
    [[ -n "$cy" ]] || fail "previewmatrix: the transport has no play centre to press"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" left >/dev/null
}

# Matrix: images draw min(own pixels, aspect-fit), never enlarged; a small clip poster and player fill.
case_previewmatrix() {
    command -v ffmpeg >/dev/null || fail "ffmpeg is missing, so the clip fixtures cannot be built"
    command -v magick >/dev/null || fail "magick is missing, so the image fixtures cannot be built"
    local dir="$fixture_root/previewmatrix"
    sandbox_scratch "$dir"
    magick -size 64x48 xc:'#7aa2f7' "$dir/a-small.png" \
        || fail "previewmatrix: the 64x48 fixture failed"
    magick -size 1920x1080 xc:'#7aa2f7' "$dir/b-large.jpg" \
        || fail "previewmatrix: the 1920x1080 fixture failed"
    magick -size 1080x1920 xc:'#e0af68' "$dir/c-portrait.png" \
        || fail "previewmatrix: the portrait fixture failed"
    # Fifteen seconds: an ipc round trip costs 190 to 565 ms, so a short clip starves the play poll.
    ffmpeg -y -f lavfi -i "testsrc=duration=15:size=64x64:rate=10" "$dir/d-tiny.mp4" >/dev/null 2>&1 \
        || fail "previewmatrix: ffmpeg could not make d-tiny.mp4"
    ffmpeg -y -f lavfi -i "testsrc=duration=15:size=1920x1080:rate=10" "$dir/e-big.mp4" >/dev/null 2>&1 \
        || fail "previewmatrix: ffmpeg could not make e-big.mp4"

    seed_ui_state "$fixture_root/previewmatrix-state" '{"keys":"default","view":"columns"}'
    launch "$dir"
    wait_listing 5
    settle
    [[ "$(ipc viewMode)" == columns ]] || fail "previewmatrix: the fixture did not open its columns view"
    # Pin the window: the Quick Look ownsize cells below need a surface larger than 1920x1080.
    local matrix_width=2560 matrix_height=1440
    cap_resize "$matrix_width" "$matrix_height"

    # Columns, one row per class: images draw min(own pixels, fit) and video fills either way.
    seek_row_named "a-small.png"
    matrix_wait_column image a-small.png
    matrix_check "columns a-small.png" "$(ipc columnFrameRect)" "$(ipc columnPictureRect)" "64 48" ownsize 2
    shot matrix-col-a-small
    seek_row_named "b-large.jpg"
    matrix_wait_column image b-large.jpg
    matrix_check "columns b-large.jpg" "$(ipc columnFrameRect)" "$(ipc columnPictureRect)" "1920 1080" fit 2
    shot matrix-col-b-large
    seek_row_named "c-portrait.png"
    matrix_wait_column image c-portrait.png
    matrix_check "columns c-portrait.png" "$(ipc columnFrameRect)" "$(ipc columnPictureRect)" "1080 1920" fit 2
    shot matrix-col-c-portrait
    seek_row_named "d-tiny.mp4"
    matrix_wait_column video d-tiny.mp4
    matrix_check "columns d-tiny.mp4 poster" "$(ipc columnFrameRect)" "$(ipc columnPictureRect)" "64 64" fill 2
    shot matrix-col-d-poster
    seek_row_named "e-big.mp4"
    matrix_wait_column video e-big.mp4
    matrix_check "columns e-big.mp4 poster" "$(ipc columnFrameRect)" "$(ipc columnPictureRect)" "1920 1080" fill 2
    shot matrix-col-e-poster

    # The column player, on the small and the large clip: the content rect fills the same way.
    seek_row_named "d-tiny.mp4"
    matrix_wait_column video d-tiny.mp4
    matrix_click_play
    local end=$((SECONDS + 10))
    while (( SECONDS < end )); do
        [[ "$(ipc columnMediaPlaying)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc columnMediaPlaying)" == "true" ]] || fail "previewmatrix: the column player never played d-tiny.mp4"
    matrix_check "columns d-tiny.mp4 player" "$(ipc columnFrameRect)" "$(ipc columnPictureRect)" "64 64" fill 2
    shot matrix-col-d-player
    seek_row_named "e-big.mp4"
    matrix_wait_column video e-big.mp4
    matrix_click_play
    end=$((SECONDS + 10))
    while (( SECONDS < end )); do
        [[ "$(ipc columnMediaPlaying)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc columnMediaPlaying)" == "true" ]] || fail "previewmatrix: the column player never played e-big.mp4"
    matrix_check "columns e-big.mp4 player" "$(ipc columnFrameRect)" "$(ipc columnPictureRect)" "1920 1080" fill 2
    shot matrix-col-e-player

    # Quick Look on the pinned surface: small and 1920x1080 hold own size, portrait takes fit, video fills.
    seek_row_named "a-small.png"
    matrix_wait_column image a-small.png
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "image" ]] \
        || fail "previewmatrix: Space never opened the image overlay"
    matrix_check "quicklook a-small.png" "$(ipc previewSurfaceRect)" "$(ipc previewPictureRect)" "64 48" ownsize 0
    shot matrix-look-a-small
    key -k Escape >/dev/null
    settle
    seek_row_named "b-large.jpg"
    matrix_wait_column image b-large.jpg
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "image" ]] \
        || fail "previewmatrix: Space never opened the large overlay"
    matrix_require_surface 1920 1080
    matrix_check "quicklook b-large.jpg" "$(ipc previewSurfaceRect)" "$(ipc previewPictureRect)" "1920 1080" ownsize 0
    shot matrix-look-b-large
    key -k Escape >/dev/null
    settle
    seek_row_named "c-portrait.png"
    matrix_wait_column image c-portrait.png
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "image" ]] \
        || fail "previewmatrix: Space never opened the portrait overlay"
    matrix_check "quicklook c-portrait.png" "$(ipc previewSurfaceRect)" "$(ipc previewPictureRect)" "1080 1920" fit 0
    shot matrix-look-c-portrait
    key -k Escape >/dev/null
    settle
    seek_row_named "d-tiny.mp4"
    matrix_wait_column video d-tiny.mp4
    key -k space >/dev/null
    wait_preview_state playing
    matrix_check "quicklook d-tiny.mp4" "$(matrix_media_rect)" "$(ipc previewPictureRect)" "64 64" fill 0
    shot matrix-look-d-player
    key -k Escape >/dev/null
    settle
    seek_row_named "e-big.mp4"
    matrix_wait_column video e-big.mp4
    key -k space >/dev/null
    wait_preview_state playing
    matrix_check "quicklook e-big.mp4" "$(matrix_media_rect)" "$(ipc previewPictureRect)" "1920 1080" fill 0
    shot matrix-look-e-player
    key -k Escape >/dev/null
    settle

    # One contact sheet for the whole matrix, labelled by file name, beside the shot evidence.
    magick montage -label '%f' "$evidence_dir"/matrix-*.png -tile 4x -geometry 320x240+4+4 \
        "$evidence_dir/previewmatrix-sheet.png" \
        || fail "previewmatrix: the contact sheet failed"
    printf 'PREVIEWMATRIX cells=12 ok=12\n'
    printf 'PREVIEWMATRIX_SHEET %s\n' "$evidence_dir/previewmatrix-sheet.png"
    kill_flea
}
