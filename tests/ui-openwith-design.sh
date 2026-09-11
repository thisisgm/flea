#!/usr/bin/env bash
# Sourced by ui.sh; OpenWith.html's flyout and card, driven natively and measured against the board.
# shellcheck disable=SC2154 # ui.sh supplies the owned fixture and native driver settings.

# How many rows the flyout last named, so the caller can walk to its tail without counting twice.
openwith_flyout_rows=0

openwith_open_flyout() {
    # Left first, then right: menus_file_menu opens a row menu the same way, because the right press
    # lands on a pane that has to hold the focus and the row before it means anything.
    menus_expect menuState '.opened | not' 'the previous menu is gone'
    click_row "$(row_index_of "$1")" left
    click_row "$(row_index_of "$1")" right
    menus_expect menuState '.opened and .snapshotReady and .hasRow' 'row menu captures the Open with source'
    menus_seek openWith
    key -k Right >/dev/null || fail 'openwith: opening the flyout failed'
    menus_expect menuState '.submenu' 'the Open with row opens its flyout'
    openwith_flyout_rows=$(ipc menuState | jq -r '[.entries[] | select(.action == "openWith") | .submenu[]] | length')
    [[ "$openwith_flyout_rows" -ge 2 ]] || fail "openwith: the flyout named $openwith_flyout_rows rows"
}

openwith_open_dialog() {
    local index
    openwith_open_flyout "$1"
    for ((index = 1; index < openwith_flyout_rows; index++)); do key -k Down >/dev/null; done
    key -k Return >/dev/null || fail 'openwith: the tail row did not answer'
    menus_expect openWithState '.opened and (.busy | not)' 'the tail row opens the card'
}

openwith_search() {
    printf '%s' "$1" | omarchy-drive key --window flea - >/dev/null || fail "openwith: typing $1 failed"
    settle
}

openwith_clear_search() {
    local index length
    length=$(ipc openWithState | jq -r '.search | length')
    for ((index = 0; index < length; index++)); do key -k BackSpace >/dev/null; done
    menus_expect openWithState '.search == ""' 'the search line empties'
}

case_openwithdesign() (
    local menu_box listing config data state name frame flyout mime_before menus_checks=0 openwith_flyout_rows=0
    local -a numbers
    sandbox_require "$fixture_root"
    menu_box=$(mktemp -d "$fixture_root/openwith-design.XXXXXXXX") || fail 'openwith: owned fixture creation failed'
    printf 'native Open with fixture\n' > "$menu_box/.flea-test-sandbox"
    listing="$menu_box/listing"; config="$menu_box/config"; data="$menu_box/data"; state="$menu_box/state"
    for name in "$listing" "$config" "$data" "$data/applications"; do
        menus_guard "$name"
        mkdir -p "$name" || fail 'openwith: fixture directory creation failed'
    done
    # The fixture entry execs /bin/true, so an Open that reaches the launcher starts nothing.
    menus_guard "$data/applications/zzflea-openwith.desktop"
    printf '[Desktop Entry]\nType=Application\nName=Zzflea Fixture\nExec=/bin/true %%f\nIcon=text-x-generic\nMimeType=text/plain;\n' \
        > "$data/applications/zzflea-openwith.desktop"
    # gio resolves the id named below through the desktop database, and an applications directory
    # with no cache answers for nothing, so the default silently did not take.
    update-desktop-database "$data/applications" 2>/dev/null \
        || fail 'openwith: the fixture desktop database could not be built'
    # A default the desktop really reads back, so the board's "default" caption has something to name.
    menus_guard "$config/mimeapps.list"
    printf '[Default Applications]\ntext/plain=zzflea-openwith.desktop\n' > "$config/mimeapps.list"
    export XDG_CONFIG_HOME="$config" XDG_DATA_HOME="$data"
    # The system roots stay on the path: the shared mime database names the kind, and dropping it
    # would leave the eyebrow reading "text/plain" instead of the type's own name.
    export XDG_DATA_DIRS="$data:/usr/local/share:/usr/share"
    menus_guard "$listing/notes.txt"
    printf 'a note with real bytes in it\n' > "$listing/notes.txt"
    seed_ui_state "$state" '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"off"},"menu":{"hidden":[]}}'
    mime_before=$(stat -c %Y "$config/mimeapps.list")
    trap 'kill_flea' EXIT
    launch "$listing"
    wait_listing 1

    # Rule 2: the flyout is the parent's own width, flush against its right edge, its frame top on
    # the opening row's top. Rule 3: the default leads and carries the muted caption.
    openwith_open_flyout notes.txt
    frame=$(ipc menuState | jq -r .frame)
    flyout=$(ipc menuState | jq -r .flyout)
    read -r -a numbers <<< "$frame $flyout"
    [[ "${numbers[2]}" == "${numbers[6]}" ]] \
        || fail "openwith: the flyout is ${numbers[6]} wide, the menu is ${numbers[2]}"
    (( numbers[4] == numbers[0] + numbers[2] )) \
        || fail "openwith: the flyout starts at ${numbers[4]}, not flush at $(( numbers[0] + numbers[2] ))"
    menus_expect menuState '[.entries[] | select(.action == "openWith") | .submenu[]][0] | .id == "zzflea-openwith.desktop" and .hint == "default"' \
        'the desktop default leads the flyout and says so'
    menus_expect menuState '[.entries[] | select(.action == "openWith") | .submenu[]] | (.[-2].separator == true) and (.[-1].glyph == "app-window")' \
        'the tail row sits under its own separator with the app-window glyph'
    # Both rungs of AppLibrary.qml's ladder: the app and device index answers with a file where it
    # can, and a name it cannot place is passed through for the themed lookup rather than dropped.
    # The fixture names a mimetype icon on purpose, which is exactly what that index does not hold.
    menus_expect menuState '[.entries[] | select(.action == "openWith") | .submenu[] | .icon // empty] | any(startswith("/"))' \
        "an entry the app icon index places arrives as a file"
    menus_expect menuState '[.entries[] | select(.action == "openWith") | .submenu[] | select(.id == "zzflea-openwith.desktop")][0].icon == "text-x-generic"' \
        "a name that index cannot place is left for the themed lookup"
    shot openwith-flyout
    # Two: the first closes the flyout, the second the menu holding it open behind it.
    key -k Escape >/dev/null
    key -k Escape >/dev/null

    # Rules 4 and 5: the Convert popup family's width, two eyebrows naming the kind, and a list that
    # ends on a row boundary at seven applications plus the eyebrows standing over them.
    openwith_open_dialog notes.txt
    menus_expect openWithState '.kind == "Plain text document" and .name == "notes.txt"' 'the card names the file and its kind'
    menus_expect openWithState '[.rows[] | select(.eyebrow) | .eyebrow] == ["Registered for Plain text document", "All applications"]' \
        'both groups draw under their own eyebrow'
    menus_expect openWithState '.rows[1].id == "zzflea-openwith.desktop" and .rows[1].isDefault' 'the registered group leads with the default'
    # Arithmetic on live tokens, never a pixel count: the box's own text size decides the row, and
    # a literal here passed at base-size 14 and failed at 20 without anything being wrong.
    menus_expect openWithState '(.listRect | split(" ")[3] | tonumber) == (7 * .rowHeight + 2 * .eyebrowHeight)' \
        'the list ends on a row boundary at seven applications'
    menus_expect openWithState '(.rect | split(" ")[2] | tonumber) > 0' "the card takes the Convert family's own width"
    shot openwith-dialog

    # Rule 5: the search filters both groups, and a group that matches nothing takes its eyebrow with it.
    openwith_search Zzflea
    menus_expect openWithState '[.rows[] | .label // .eyebrow] == ["Registered for Plain text document", "Zzflea Fixture", "All applications", "Zzflea Fixture"]' \
        'the search filters both groups and keeps both eyebrows'
    openwith_clear_search
    openwith_search zzqq
    menus_expect openWithState '(.rows | length) == 0 and (.listRect | split(" ")[3] | tonumber) == (7 * .rowHeight + 2 * .eyebrowHeight)' \
        'rule 6 keeps the list height when nothing matches'
    menus_expect openWithState 'any(.controls[]; .name == "Open" and (.enabled | not)) and any(.controls[]; .name == "Cancel" and .enabled)' \
        'rule 6 dims Open and leaves Cancel live'
    shot openwith-nomatch
    openwith_clear_search

    # Rule 7: Space toggles the always box from an empty search line, and the write lands once.
    key -k space >/dev/null
    menus_expect openWithState '.always' 'Space toggles the always box'
    openwith_search Zzflea
    menus_expect openWithState '.cursor == 0 and .rows[1].id == "zzflea-openwith.desktop"' 'the cursor holds the only match'
    key -k Return >/dev/null
    menus_expect openWithState '.opened | not' 'Open closes the card'
    grep -q '^text/plain=zzflea-openwith.desktop' "$config/mimeapps.list" \
        || fail "openwith: the default was not written: $(cat "$config/mimeapps.list")"
    [[ "$(stat -c %Y "$config/mimeapps.list")" != "$mime_before" ]] \
        || fail 'openwith: mimeapps.list was never rewritten'
    menus_equal 'the card restores listing focus' list "$(ipc focusView)"
    printf 'OPENWITH flyout=flush default=first card=630 list=315 write=own-config\n'
)
