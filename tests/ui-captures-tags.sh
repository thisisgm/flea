# Tags and TagsThemes captures (slots, Tags row and flyout, toggles, rail, search, untagged).

# Sets one fixture file's tags through user.xdg.tags and reads them back.
cap_tags_set() {
    local file="$1" value="$2"
    [[ "$file" == "$fixture_root/"* ]] || fail "cap_tags: refusing a path outside the fixture root: $file"
    python3 - "$file" "$value" <<'EOF' || fail "cap_tags: user.xdg.tags did not land"
import os, sys
path, value = sys.argv[1], sys.argv[2]
os.setxattr(path, "user.xdg.tags", value.encode())
assert os.getxattr(path, "user.xdg.tags") == value.encode(), "read-back mismatch"
EOF
}

# needs the operator theme switch, driven by the controller
cap_tags_skip_themes() {
    printf 'CAP SKIP tags-themes: per-theme 3x swatches and digit fallbacks need the live theme switch\n'
    return 0
}

# HIG open-refused: Enter acts on marked subjects, so folder navigation selects its own target.
cap_tags_enter_sub() {
    local dir="$1" capture="$2"
    click_row "$(row_index_of plain-sub)" left
    settle
    [[ "$(ipc rowAt "$(ipc cursor)")" == "plain-sub|"* && "$(ipc selectionCount)" == 1 ]] \
        || fail "cap_tags: the untagged specimen did not select plain-sub"
    shot "$capture"
    key -k Return >/dev/null
    wait_path "$dir/plain-sub"
}

# Tags040 states: slots, Tags row and flyout, toggles, rail, search, untagged.
case_cap_tags() {
    local dir="$fixture_root/cap-tags" menus_checks=0 waited noop_before noop_now refused
    sandbox_scratch "$dir"
    printf 'one\n' > "$dir/red-holiday-1.txt"
    printf 'two\n' > "$dir/red-holiday-2.txt"
    printf 'three\n' > "$dir/yellow-holiday.txt"
    printf 'plain\n' > "$dir/plain.txt"
    # ruling A: six tags overflow the 88 px slot, which keeps the 5 whole swatches that fit
    printf 'many\n' > "$dir/many-tags.txt"
    mkdir -p "$dir/plain-sub"
    printf 'a\n' > "$dir/plain-sub/a.txt"
    printf 'b\n' > "$dir/plain-sub/b.txt"
    cap_tags_set "$dir/red-holiday-1.txt" "Red"
    cap_tags_set "$dir/red-holiday-2.txt" "Red,Blue"
    cap_tags_set "$dir/yellow-holiday.txt" "Yellow,Green,Purple"
    cap_tags_set "$dir/many-tags.txt" "Orange,Yellow,Green,Blue,Purple,Gray"
    seed_ui_state "$fixture_root/cap-tags-state" '{"keys":"default","view":"list","keyHints":true}'
    launch "$dir"
    wait_listing 6
    cap_resize 1100 700
    seek_row_named "yellow-holiday.txt" || fail "cap_tags: could not find yellow-holiday.txt"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "yellow-holiday.txt|"* ]] \
        || fail "cap_tags: the cursor is not on yellow-holiday.txt"
    shot tags-list
    click_row "$(row_index_of red-holiday-1.txt)" left
    settle
    click_row "$(row_index_of red-holiday-2.txt)" left --mods ctrl
    settle
    [[ "$(ipc selectionCount)" == "2" ]] || fail "cap_tags: two ctrl clicks selected $(ipc selectionCount), not 2"
    click_row "$(row_index_of red-holiday-1.txt)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_tags: the file menu never opened"
    [[ "|$(ipc contextMenuEntries)|" == *"|Tags|"* ]] \
        || fail "cap_tags: the file menu offers no Tags, got $(ipc contextMenuEntries)"
    [[ "|$(ipc contextMenuHints)|" == *"|T|"* ]] \
        || fail "cap_tags: the Tags row shows no T hint, got $(ipc contextMenuHints)"
    shot tags-menu
    menu_seek "Tags"
    key -k Right >/dev/null
    settle
    cap_menu_expect '.opened and .submenu' "tags: the Tags flyout never opened"
    cap_menu_expect '([.submenuEntries[] | select(.separator != true) | .label] | join("|")) == "Red|Orange|Yellow|Green|Blue|Purple|Gray|Remove All Tags"' "tags: the flyout lists another row than the seven colours and the clear"
    cap_menu_expect '([.submenuEntries[] | select(.label == "Red")][0].tagRow.state) == "applied"' "tags: Red is not checked over two Red files"
    cap_menu_expect '([.submenuEntries[] | select(.label == "Blue")][0].tagRow.state) == "mixed"' "tags: Blue shows no mixed bar over one of two files"
    shot tags-flyout
    # toggling the mixed tag applies it to both selected items; the line counts the one item it changed
    key 5 >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc statusPrimary)" == *"Tagged 1 item Blue"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc statusPrimary)" == *"Tagged 1 item Blue"* ]] \
        || fail "cap_tags: digit 5 applied no Blue, status reads $(ipc statusPrimary)"
    # the digit toggles in place: the menu and the flyout stay open and Blue turns checked on its own row, with no T
    cap_menu_expect '.opened and .submenu' "tags: the digit closed the menu or the flyout"
    cap_menu_expect '([.submenuEntries[] | select(.label == "Blue")][0].tagRow.state) == "applied"' "tags: Blue is not checked over both files after the mixed toggle"
    shot tags-mixed-apply
    key -k Escape >/dev/null
    settle
    cap_menu_expect '.opened and (.submenu | not)' "tags: one Escape did not leave the menu open with no flyout"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_tags: the Tags menu stayed open"
    click_row "$(row_index_of plain.txt)" left
    settle
    key T >/dev/null
    settle
    cap_menu_expect '.opened and .submenu' "tags: T opened no flyout from the listing"
    cap_menu_expect '([.submenuEntries[] | select(.label == "Red")][0].tagRow.state) == "absent"' "tags: Red is not unchecked on the untagged row"
    shot tags-t-open
    key 3 >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc statusPrimary)" == *"Tagged 1 item Yellow"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc statusPrimary)" == *"Tagged 1 item Yellow"* ]] \
        || fail "cap_tags: digit 3 toggled no Yellow, status reads $(ipc statusPrimary)"
    # a digit fires like a click and leaves the flyout up, so the second digit needs no T
    cap_menu_expect '.opened and .submenu' "tags: the digit toggle closed the flyout"
    cap_menu_expect '([.submenuEntries[] | select(.label == "Yellow")][0].tagRow.state) == "applied"' "tags: Yellow is not checked after the digit toggle"
    shot tags-digit-toggle
    # ruling A: removing the tag prints the Removed line with its z hint
    key 3 >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc statusPrimary)" == *"Removed Yellow from 1 item"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc statusPrimary)" == *"Removed Yellow from 1 item"* ]] \
        || fail "cap_tags: digit 3 removed no Yellow, status reads $(ipc statusPrimary)"
    cap_menu_expect '.opened and .submenu and ([.submenuEntries[] | select(.label == "Yellow")][0].tagRow.state) == "absent"' "tags: Yellow is not unchecked after the second digit"
    shot tags-removed
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_tags: the toggled menu stayed open"
    # tagging is journaled, so z restores the tags from before the removal
    key z >/dev/null
    wait_message "Restored the previous tags."
    menus_expect listInFlight '. == false' "cap_tags: the tag undo re-read settles"
    # a tag undo re-reads in place like a mode undo, so T still targets the file it restored
    [[ "$(ipc rowAt "$(ipc cursor)")" == "plain.txt|"* ]] \
        || fail "cap_tags: the tag undo moved the cursor to $(ipc rowAt "$(ipc cursor)")"
    key T >/dev/null
    settle
    cap_menu_expect '([.submenuEntries[] | select(.label == "Yellow")][0].tagRow.state) == "applied"' "tags: z restored no Yellow on plain.txt"
    shot tags-undo
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_tags: the undo check left the menu open"
    settings_open_key
    settle
    settings_section places
    [[ "$(ipc settingsRows)" == *"Show tags"* ]] || fail "cap_tags: Places lists no Show tags"
    settings_click_control "places.showTags"
    settings_wait_value '.places.showTags == true'
    key -k Escape >/dev/null
    settle
    wait_rail_label "Red"
    [[ "|$(ipc railLabels)|" == *"|Red|"* && "|$(ipc railLabels)|" == *"|Blue|"* ]] \
        || fail "cap_tags: the TAGS section lists no Red and Blue, rail reads $(ipc railLabels)"
    shot tags-rail
    click_rail_row "$(rail_row_of "Red")" left
    settle
    menus_expect keyDeliveryState '.searchMode == "results" and (.searchRunning | not)' "tags: the Red rail row walks for its tag"
    menus_expect keyDeliveryState '.searchQuery == "tag:Red"' "tags: the Red rail row searches another query than tag:Red"
    [[ "$(ipc total)" == "2" ]] || fail "cap_tags: the Red rail walk found $(ipc total), not 2"
    shot tags-rail-search
    key -k Escape >/dev/null
    settle
    wait_listing 6
    key f >/dev/null
    menus_expect keyDeliveryState '.searchMode == "typing"' "tags: f opened no query line"
    key tag:red >/dev/null
    key -k Space >/dev/null
    key holiday >/dev/null
    menus_expect keyDeliveryState '.searchQuery == "tag:red holiday"' "tags: the query reads $(ipc keyDeliveryState)"
    key -M ctrl -k Return -m ctrl >/dev/null
    menus_expect keyDeliveryState '.searchMode == "results" and (.searchRunning | not)' "tags: tag:red holiday never settled its walk"
    [[ "$(ipc total)" == "2" ]] || fail "cap_tags: tag:red holiday found $(ipc total), not 2"
    shot tags-search
    key -k Escape >/dev/null
    settle
    wait_listing 6
    click_chrome grid
    settle
    [[ "$(ipc viewMode)" == "grid" ]] || fail "cap_tags: the chrome never reached grid"
    shot tags-grid
    click_chrome columns
    settle
    [[ "$(ipc viewMode)" == "columns" ]] || fail "cap_tags: the chrome never reached columns"
    shot tags-columns
    click_chrome list
    settle
    [[ "$(ipc viewMode)" == "list" ]] || fail "cap_tags: the chrome never returned to list"
    # the overflow slot keeps the 5 whole swatches that fit, never past the slot
    seek_row_named "many-tags.txt" || fail "cap_tags: could not find many-tags.txt"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "many-tags.txt|"* ]] \
        || fail "cap_tags: the cursor is not on many-tags.txt"
    shot tags-overflow
    cap_tags_enter_sub "$dir" tags-untagged-target
    wait_listing 2
    shot tags-untagged
    # callout 4: a foreign word survives a Flea toggle and shows in Properties
    printf 'foreign\n' > "$dir/plain-sub/foreign.txt"
    cap_tags_set "$dir/plain-sub/foreign.txt" "Red,Project-X"
    # Properties is hidden on a fresh ui.json (src/uischema.rs menu.hidden), so this callout starts a window whose hidden list is the default one less properties
    seed_ui_state "$fixture_root/cap-tags-properties-state" '{"keys":"default","view":"list","keyHints":true,"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText"]}}'
    launch "$dir/plain-sub"
    wait_path "$dir/plain-sub"
    wait_listing 3
    cap_resize 1100 700
    seek_row_named "foreign.txt" || fail "cap_tags: could not find foreign.txt"
    click_row "$(row_index_of foreign.txt)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_tags: the file menu never opened on foreign.txt"
    menu_seek "Properties"
    key -k Return >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc menuDialogState | jq -er '.opened and .action == "properties" and ((.facts.tags // []) | index("Project-X") != null)' 2>/dev/null)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc menuDialogState | jq -er '.opened and .action == "properties" and ((.facts.tags // []) | index("Project-X") != null)')" == "true" ]] \
        || fail "cap_tags: Properties shows no Project-X, got $(ipc menuDialogState)"
    shot tags-unknown-properties
    key -k Escape >/dev/null
    settle
    [[ "$(ipc menuDialogState | jq -er '.opened')" == "false" ]] || fail "cap_tags: Properties stayed open"
    seek_row_named "foreign.txt" || fail "cap_tags: lost foreign.txt after Properties"
    key T >/dev/null
    settle
    cap_menu_expect '.opened and .submenu' "tags: T opened no flyout on foreign.txt"
    key 5 >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc statusPrimary)" == *"Tagged 1 item Blue"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc statusPrimary)" == *"Tagged 1 item Blue"* ]] \
        || fail "cap_tags: digit 5 toggled no Blue on foreign.txt, status reads $(ipc statusPrimary)"
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$dir/plain-sub/foreign.txt" == "$fixture_root/"* ]] || fail "cap_tags: refusing a path outside the fixture root"
    python3 - "$dir/plain-sub/foreign.txt" <<'EOF' || fail "cap_tags: Project-X did not survive the Blue toggle"
import os, sys
value = os.getxattr(sys.argv[1], "user.xdg.tags").decode()
assert "Project-X" in value.split(","), value
EOF
    # HIG tags-menu hides the clear action while the subject carries no tag.
    printf 'noop\n' > "$dir/plain-sub/noop.txt"
    wait_listing 4
    seek_row_named "noop.txt" || fail "cap_tags: could not find noop.txt"
    key T >/dev/null
    settle
    cap_menu_expect '.opened and .submenu' "tags: T opened no flyout on noop.txt"
    noop_before="$(ipc statusPrimary)"
    cap_menu_expect '.opened and .submenu and all(.submenuEntries[]; .label != "Remove All Tags")' "tags: an untagged subject offers Remove All Tags"
    # Keep the untagged subject open for three seconds; a late write or Removed line still fails.
    waited=$((SECONDS + 3))
    while (( SECONDS < waited )); do
        noop_now="$(ipc statusPrimary)"
        [[ -z "$noop_now" || "$noop_now" == "$noop_before" ]] \
            || fail "cap_tags: the untagged flyout printed $noop_now"
        sleep 0.2
    done
    [[ "$dir/plain-sub/noop.txt" == "$fixture_root/"* ]] || fail "cap_tags: refusing a path outside the fixture root"
    python3 - "$dir/plain-sub/noop.txt" <<'EOF' || fail "cap_tags: an xattr appeared on the untagged row"
import os, sys
try:
    os.getxattr(sys.argv[1], "user.xdg.tags")
except OSError as e:
    assert e.errno == 61, e
else:
    raise SystemExit("user.xdg.tags appeared on the untagged row")
EOF
    shot tags-noop
    # Adding Blue reveals the clear action, whose Return is proved by the status and the bare file.
    key 5 >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc statusPrimary)" == *"Tagged 1 item Blue"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc statusPrimary)" == *"Tagged 1 item Blue"* ]] \
        || fail "cap_tags: digit 5 tagged no Blue on noop.txt, status reads $(ipc statusPrimary)"
    cap_menu_expect '.opened and .submenu and any(.submenuEntries[]; .label == "Remove All Tags")' "tags: the tagged subject offers no Remove All Tags"
    menu_seek_submenu "Remove All Tags"
    cap_menu_expect '.submenuEntries[.submenuCursor].label == "Remove All Tags"' "tags: Return would not land on Remove All Tags"
    key -k Return >/dev/null
    for waited in $(seq 1 100); do
        [[ "$(ipc statusPrimary)" == *"Removed all tags from 1 item"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc statusPrimary)" == *"Removed all tags from 1 item"* ]] \
        || fail "cap_tags: Return on Remove All Tags cleared nothing, status reads $(ipc statusPrimary)"
    cap_menu_expect '.opened and ([.entries[] | select(.id == "tags") | .submenu[] | .label] == ["Red","Orange","Yellow","Green","Blue","Purple","Gray"])' "tags: clearing the last tag left another tag inventory"
    [[ "$dir/plain-sub/noop.txt" == "$fixture_root/"* ]] || fail "cap_tags: refusing a path outside the fixture root"
    python3 - "$dir/plain-sub/noop.txt" <<'EOF2' || fail "cap_tags: Return left the Blue tag on noop.txt"
import os
import sys
try:
    os.getxattr(sys.argv[1], "user.xdg.tags")
except OSError as e:
    assert e.errno == 61, e
else:
    raise SystemExit("user.xdg.tags stayed on noop.txt")
EOF2
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_tags: the no-op menu stayed open"
    # callout 4: a copy carries the source tags onto the new item
    key h >/dev/null
    wait_path "$dir"
    wait_listing 6
    click_row "$(row_index_of red-holiday-1.txt)" left
    settle
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and (.clipboard.cut | not)' "tags: red-holiday-1.txt is on the clipboard"
    cap_tags_enter_sub "$dir" tags-copy-target
    wait_listing 4
    key p >/dev/null
    for waited in $(seq 1 200); do
        [[ -e "$dir/plain-sub/red-holiday-1.txt" ]] && break
        sleep 0.05
    done
    [[ -e "$dir/plain-sub/red-holiday-1.txt" ]] || fail "cap_tags: the copy never landed in plain-sub"
    wait_listing 5
    [[ "$dir/plain-sub/red-holiday-1.txt" == "$fixture_root/"* ]] || fail "cap_tags: refusing a path outside the fixture root"
    python3 - "$dir/red-holiday-1.txt" "$dir/plain-sub/red-holiday-1.txt" <<'EOF' || fail "cap_tags: the copy lost a tag"
import os, sys
source = os.getxattr(sys.argv[1], "user.xdg.tags")
copied = os.getxattr(sys.argv[2], "user.xdg.tags")
assert copied == source, f"the copy carries {copied!r}, not the source {source!r}"
EOF
    row_index_of "red-holiday-1.txt" >/dev/null || fail "cap_tags: the copied row is not listed"
    shot tags-copy-kept
    # callout 4: a move carries the tags to the new path and leaves no source behind
    key h >/dev/null
    wait_path "$dir"
    wait_listing 6
    click_row "$(row_index_of red-holiday-2.txt)" left
    settle
    key x >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and .clipboard.cut' "tags: red-holiday-2.txt is cut to the clipboard"
    cap_tags_enter_sub "$dir" tags-move-target
    wait_listing 5
    key p >/dev/null
    for waited in $(seq 1 200); do
        [[ -e "$dir/plain-sub/red-holiday-2.txt" && ! -e "$dir/red-holiday-2.txt" ]] && break
        sleep 0.05
    done
    [[ -e "$dir/plain-sub/red-holiday-2.txt" ]] || fail "cap_tags: the move never landed in plain-sub"
    [[ ! -e "$dir/red-holiday-2.txt" ]] || fail "cap_tags: the move left its source behind"
    wait_listing 6
    [[ "$dir/plain-sub/red-holiday-2.txt" == "$fixture_root/"* ]] || fail "cap_tags: refusing a path outside the fixture root"
    python3 - "$dir/plain-sub/red-holiday-2.txt" <<'EOF' || fail "cap_tags: the move lost a tag"
import os, sys
assert os.getxattr(sys.argv[1], "user.xdg.tags") == b"Red,Blue", "the moved file carries another tag than Red,Blue"
EOF
    row_index_of "red-holiday-2.txt" >/dev/null || fail "cap_tags: the moved row is not listed"
    shot tags-move-kept
    # callout 5: procfs holds no user xattrs, so the Tags row is absent and T sends no write
    launch "/proc" "/proc/version"
    wait_path "/proc"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "version|"* ]] || fail 'cap_tags: the selected procfs specimen is not version'
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_tags: the file menu never opened on /proc/version"
    [[ "|$(ipc contextMenuEntries)|" != *"|Tags|"* ]] \
        || fail "cap_tags: the file menu offers Tags where no xattr stands, got $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle
    key T >/dev/null
    # T without a Tags row refuses instead of writing, so poll for the refusal line
    refused=""
    for waited in $(seq 1 100); do
        refused="$(ipc lastMessage)"
        [[ "$refused" == *"does not support file tags"* ]] && break
        sleep 0.05
    done
    [[ "$refused" == *"does not support file tags"* ]] \
        || fail "cap_tags: T on /proc/version said $refused, not the tags refusal"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "cap_tags: T opened a flyout where the Tags row is absent"
    shot tags-nosupport
    cap_tags_skip_themes
    # The shot-only tokens claim a capture and nothing more; tests/tagrows.sh pins those slots on the real delegates.
    printf 'CAP_TAGS list=shot menu=T flyout=7+clear mixed=apply t=open digit=toggle removed=line undo=z rail=section search=tag:red grid=shot columns=shot overflow=shot untagged=shot unknown=properties noop=noz copy=kept move=kept nosupport=absent\n'
    kill_flea
}
