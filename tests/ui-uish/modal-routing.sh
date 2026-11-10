# GPUI overrides for UISH-09; seeded from the unchanged oracle at 2faf8d099.
case_collide() {
    local dir="$fixture_root/collide" menus_checks=0 name wx wy rx ry collide_trash
    sandbox_scratch "$dir"
    mkdir -p "$dir/to"
    for name in a b c d; do printf 'yours %s\n' "$name" > "$dir/$name.txt"; done
    printf 'notes\n' > "$dir/notes.txt"
    # Text bodies under a text name: a .png holding text makes the preview column log a decode failure, which the log check refuses.
    printf 'yours\n' > "$dir/photo.txt"
    printf 'there\n' > "$dir/to/photo.txt"
    export XDG_DATA_HOME="$fixture_root/collide-data"
    sandbox_scratch "$XDG_DATA_HOME"
    # GLib trashes to the home store or the fixture mount's top trash by device.
    collide_trash=$(sandbox_trash_files_dir "$dir/photo.txt") || fail "collide: no GLib trash directory for $dir/photo.txt"
    # Undo finds a replaced item through gio trash --list, which only a gvfsd started with this XDG_DATA_HOME answers, so the window gets a private session bus.
    local bus
    mapfile -t bus < <(dbus-daemon --session --fork --print-address=1 --print-pid=1)
    [[ ${#bus[@]} -eq 2 && "${bus[1]}" =~ ^[0-9]+$ ]] || fail "collide: no private session bus, dbus-daemon printed: ${bus[*]}"
    collide_bus_pid=${bus[1]}
    trap '( kill_flea ) >/dev/null 2>&1 || true; kill "$collide_bus_pid" 2>/dev/null || true' EXIT
    export DBUS_SESSION_BUS_ADDRESS="${bus[0]}"
    # Enter on a marked set opens every marked file (APL-FILE-11), so the open handoff is a recording stub nothing may reach.
    local collide_opened="$fixture_root/collide-opened.log" collide_bin="$fixture_root/collide-bin" collide_path="$PATH"
    sandbox_scratch "$collide_bin"
    : > "$collide_opened"
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$collide_opened"
    } > "$collide_bin/$open_handoff"
    chmod +x "$collide_bin/$open_handoff"
    export PATH="$collide_bin:$PATH"
    collide_nothing_opened() {
        [[ ! -s "$collide_opened" ]] || fail "collide: $1 handed files to the opener: $(tr '\n' ' ' < "$collide_opened")"
    }

    # A finished transfer's own line, polled because it stands only until the bar clears it; the undo hint tells it from the clipboard's.
    collide_said() {
        local want="$1" seen=""
        for _attempt in $(seq 1 300); do
            seen=$(ipc lastMessage)
            [[ "$seen" == "$want"* ]] && return 0
            sleep 0.05
        done
        fail "collide: the bar never said $want, it said $seen"
    }
    collide_until() {
        local what="$1"; shift
        for _attempt in $(seq 1 300); do
            "$@" && return 0
            sleep 0.05
        done
        fail "collide: $what"
    }
    collide_holds() { [[ "$(cat "$1" 2>/dev/null)" == "$2" ]]; }
    collide_absent() { [[ ! -e "$1" && ! -L "$1" ]]; }
    collide_untouched() { collide_absent "$dir/to/photo copy.txt" && collide_absent "$dir/to/notes.txt" && collide_absent "$dir/to/c.txt" && collide_holds "$dir/to/photo.txt" there; }
    # The six files onto the clipboard and the pane into to, which is the whole of every paste below.
    collide_copy_six() {
        launch "$dir"
        wait_listing 7
        seek_row_named "a.txt"
        key v >/dev/null
        for _ in 1 2 3 4 5; do key J >/dev/null; done
        settle
        [[ "$(ipc selectedIndices)" == "1,2,3,4,5,6" ]] || fail "collide: the six files are not selected, $(ipc selectedIndices) is"
        key y >/dev/null
        menus_expect keyDeliveryState '(.clipboard.paths | length) == 6 and (.clipboard.cut | not)' "the six files are on the clipboard"
        # Escape drops the marks and keeps the clipboard, so Return on to enters it instead of opening the six.
        key -k Escape >/dev/null
        settle
        [[ -z "$(ipc selectedIndices)" ]] || fail "collide: Escape left $(ipc selectedIndices) marked"
        menus_expect keyDeliveryState '(.clipboard.paths | length) == 6' "the clipboard keeps the six after Escape"
        seek_row_named "to"
        key -k Return >/dev/null
        wait_path "$dir/to"
        collide_nothing_opened "entering to"
    }
    # Nothing on the card can wrap: the title and the names elide, and the one explanation line stays one line.
    collide_one_line() {
        menus_expect collideState '.buttonsFit and (.titleTruncated | not) and .explainLines == 1 and ([.buttons[] | select(.name != "merge") | .visible] | all)' "$1"
    }

    collide_copy_six
    wait_listing 1

    echo "-- asked once, on Keep both, and Cancel sends nothing --"
    key p >/dev/null
    menus_expect collideState '.opened and .title == "photo.txt already exists in to." and .names == ["photo.txt"] and .more == ""
        and .explain == "Replaced items go to Trash, and Undo restores them." and .focus == "keep"' "one collision is asked about once, on Keep both"
    collide_one_line "the one-name card keeps every line to one line"
    menus_expect collideState '[.buttons[] | select(.name == "merge") | .visible] == [false]' "a file-on-file card hides Merge"
    shot collide-one-keep
    key -M ctrl -k Return -m ctrl >/dev/null
    settle
    menus_expect collideState '.opened' "a modified Enter is ignored"

    echo "-- the mouse back button leaves the pane where it is behind the card --"
    command -v ydotool >/dev/null || fail "collide: ydotool is missing, so the mouse back button cannot be pressed"
    export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-$XDG_RUNTIME_DIR/.ydotool_socket}"
    [[ -S "$YDOTOOL_SOCKET" ]] || fail "collide: no ydotoold socket at $YDOTOOL_SOCKET"
    read -r wx wy _ _ < <(window_box) || fail "native window coordinates unavailable"
    read -r rx ry <<< "$(ipc rowCentre 0)"
    [[ -n "$rx" && -n "$ry" ]] || fail "collide: row 0 has no on-screen centre to park the pointer on"
    omarchy-drive move "$((rx + wx))" "$((ry + wy))" >/dev/null || fail "collide: the pointer could not be parked over the window"
    ydotool click 0xC3 >/dev/null 2>&1 || fail "collide: ydotool refused the mouse back button"
    settle
    settle
    [[ "$(ipc path)" == "$dir/to" ]] || fail "collide: the mouse back button took the pane to $(ipc path) behind the card"
    menus_expect collideState '.opened and .focus == "keep"' "the mouse back button leaves the card as it was"

    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels"
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 6 and (.paneFocus or .listFocus)' "Cancel keeps the clipboard and hands the list back"
    settle
    collide_untouched || fail "collide: Cancel changed $dir/to: $(ls -A "$dir/to")"
    # The control: the same press at the same spot with the card shut does go back, so the one above reached the pane and was refused.
    ydotool click 0xC3 >/dev/null 2>&1 || fail "collide: ydotool refused the control press"
    wait_path "$dir"
    seek_row_named "to"
    key -k Return >/dev/null
    wait_path "$dir/to"
    wait_listing 1

    echo "-- Enter takes Keep both, and Undo takes the copies away --"
    key p >/dev/null
    menus_expect collideState '.opened and .focus == "keep"' "the card opens on Keep both again"
    key -k Return >/dev/null
    collide_said "Copied 6 items · z undoes"
    collide_holds "$dir/to/photo copy.txt" yours || fail "collide: Keep both wrote no photo copy.txt"
    collide_holds "$dir/to/photo.txt" there || fail "collide: Keep both touched the photo.txt already there"
    collide_holds "$dir/to/notes.txt" notes || fail "collide: Keep both did not copy the rest"
    key z >/dev/null
    collide_until "Undo left the kept copies in place" collide_untouched

    echo "-- Skip leaves the collision and copies the rest --"
    key p >/dev/null
    menus_expect collideState '.opened' "the card opens for Skip"
    key h >/dev/null
    menus_expect collideState '.focus == "replace"' "h moves from Keep Both to its left neighbour Replace"
    key h >/dev/null
    menus_expect collideState '.focus == "skip"' "h moves from Replace to Skip"
    key -k Return >/dev/null
    collide_said "Copied 5 of 6 · 1 skipped · z undoes"
    # The files are small, so the transfer card is gone by now and the line it leaves is what shows.
    shot collide-skip-done
    collide_holds "$dir/to/photo.txt" there || fail "collide: Skip touched the photo.txt already there"
    collide_absent "$dir/to/photo copy.txt" || fail "collide: Skip kept a copy"
    collide_holds "$dir/to/notes.txt" notes || fail "collide: Skip did not copy the rest"
    key z >/dev/null
    collide_until "Undo left the skipped transfer's copies in place" collide_untouched

    echo "-- Replace sends the old item to Trash, and one Undo swaps them back --"
    key p >/dev/null
    menus_expect collideState '.opened' "the card opens for Replace"
    key h >/dev/null
    menus_expect collideState '.focus == "replace"' "h moves from Keep Both to Replace"
    shot collide-one-replace
    trash_trap_own "$collide_trash" "$dir" "collide" photo.txt
    key -k Return >/dev/null
    collide_said "Copied 6 items · z undoes"
    collide_holds "$dir/to/photo.txt" yours || fail "collide: Replace did not put the incoming photo.txt in place"
    # The trash landing trails the status line, so it is awaited bounded instead of asserted once.
    for _attempt in $(seq 1 300); do collide_holds "$collide_trash/photo.txt" there && break; sleep 0.05; done
    collide_holds "$collide_trash/photo.txt" there \
        || fail "collide: the replaced photo.txt is not in $collide_trash: $(ls -A "$collide_trash" 2>&1)"
    shot collide-replaced
    key z >/dev/null
    collide_until "Undo did not restore the replaced photo.txt" collide_untouched
    collide_absent "$collide_trash/photo.txt" || fail "collide: Undo left the old photo.txt in $collide_trash as well"

    echo "-- several names, all of them listed --"
    for name in a b; do printf 'there %s\n' "$name" > "$dir/to/$name.txt"; done
    key p >/dev/null
    menus_expect collideState '.opened and .title == "3 items already exist in to." and .names == ["a.txt","b.txt","photo.txt"] and .more == ""' \
        "three collisions are all named, with no more line"
    collide_one_line "the several-name card keeps every line to one line"
    shot collide-several
    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels the several-name card"

    echo "-- more names than the card lists: three of them, and the rest counted --"
    for name in c d; do printf 'there %s\n' "$name" > "$dir/to/$name.txt"; done
    key p >/dev/null
    menus_expect collideState '.opened and .title == "5 items already exist in to." and .names == ["a.txt","b.txt","c.txt"] and .more == "and 2 more"' \
        "five collisions name three and count the rest"
    collide_one_line "the and-more card keeps every line to one line"
    shot collide-more
    key -k Tab >/dev/null
    menus_expect collideState '.focus == "cancel"' "Tab wraps from Keep Both to Cancel"
    key -k Tab >/dev/null
    menus_expect collideState '.focus == "skip"' "Tab walks to Skip"
    key -k Tab >/dev/null
    menus_expect collideState '.focus == "replace"' "Tab walks to Replace"
    key -k Tab >/dev/null
    menus_expect collideState '.focus == "keep"' "Tab completes the button cycle"
    key -M shift -k Tab -m shift >/dev/null
    menus_expect collideState '.focus == "replace"' "reverse Tab reaches Replace"
    key -M shift -k Tab -m shift >/dev/null
    menus_expect collideState '.focus == "skip"' "reverse Tab reaches Skip"
    key -M shift -k Tab -m shift >/dev/null
    menus_expect collideState '.focus == "cancel"' "reverse Tab reaches Cancel"
    key -k Return >/dev/null
    menus_expect collideState '.opened | not' "Enter on Cancel cancels"
    settle
    for name in a b c d; do collide_holds "$dir/to/$name.txt" "there $name" || fail "collide: Cancel touched $name.txt"; done
    collide_holds "$dir/to/photo.txt" there || fail "collide: Cancel touched photo.txt"

    echo "-- at the largest text-size stop nothing on the card wraps --"
    seed_ui_state "$fixture_root/collide-state" '{"display":{"textSize":{"mode":20}}}'
    collide_copy_six
    wait_listing 5
    [[ "$(ipc bodyPx)" == 20 ]] || fail "collide: the seeded largest stop draws at $(ipc bodyPx)px, not 20"
    key p >/dev/null
    menus_expect collideState '.opened and .names == ["a.txt","b.txt","c.txt"] and .more == "and 2 more"' "the card opens at the largest stop"
    collide_one_line "at the largest stop the buttons fit on one row and no line wraps"
    shot collide-largest-text
    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels at the largest stop"
    kill_flea
    collide_nothing_opened "the case"
    export PATH="$collide_path"
    # A mount-top trash is shared, so only this run's own entry goes, each pair checked in the command.
    if [[ "$collide_trash" != "$XDG_DATA_HOME/Trash/files" ]]; then
        sandbox_trash_remove_own "$collide_trash" "$dir" photo.txt \
            || fail "collide: owned entries could not be removed from $collide_trash"
    fi
}

case_menu() {
    local dir="$fixture_root/menu" state="$fixture_root/menu-state"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/c.txt"
    : > "$dir/plain.txt"
    # Fifteen filler rows keep a list row beneath Rename after the menu's Move to Trash group.
    local z
    for z in $(seq 1 15); do : > "$dir/z$z.txt"; done
    seed_ui_state "$state" '{"view":"list","keys":"default"}'
    launch "$dir"
    wait_listing 20
    local row_height row_padding_x centre cx cy wx wy ww wh row_left beneath_y metrics
    metrics=$(ipc metrics) || fail "menu: metrics unavailable"
    read -r _body _caption row_padding_x row_height <<< "$metrics"
    click_row 1 right
    settle
    printf 'MENU on-file visible=%s cursor=%s\n' "$(ipc contextMenuVisible)" "$(ipc cursor)"
    shot menu-open
    printf 'MENU_OCR_BEGIN\n'
    omarchy-drive ocr flea || true
    printf 'MENU_OCR_END\n'
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "right click did not open the context menu"
    [[ "$(ipc cursor)" == "1" ]] || fail "right click did not set the cursor, it is $(ipc cursor)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "Escape did not close the context menu"
    key k >/dev/null
    settle
    [[ "$(ipc cursor)" == "0" ]] || fail "the list did not take the keyboard back after the menu closed"
    # The pointer rule, both halves: a menu opened under a resting pointer keeps its first row (0d626ed,
    # or Enter fires the pointer's row), and a pointer that then moves lights the row it moved onto.
    # An action row by name: a separator sits at index 2 and disables hover on purpose, so an index alone proves nothing.
    local rest_x rest_y rest_row
    key m >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "menu: the m key did not open the menu at the cursor"
    rest_row=$(menu_row_index "Rename")
    [[ -n "$rest_row" && "$rest_row" -ge 0 ]] || fail "menu: no Rename row in $(ipc contextMenuEntries)"
    read -r rest_x rest_y <<< "$(ipc contextMenuRowCentre "$rest_row")"
    [[ -n "$rest_y" ]] || fail "menu: the menu has no row $rest_row to rest the pointer on"
    key -k Escape >/dev/null
    settle
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    # A warp alone reaches Qt as no motion at all, so the one uinput pixel is what makes the pointer rest there.
    omarchy-drive move "$((wx + rest_x))" "$((wy + rest_y))" >/dev/null
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
    settle
    # m through uinput (evdev 50): the key helper refocuses the window first, and Hyprland warps the cursor on focus without telling Qt.
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 50:1 50:0 >/dev/null 2>&1
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "menu: the m key did not reopen the menu under the resting pointer"
    # The target is proven before the move is judged: the reopened menu's Rename row is where it was, and the pointer is over it.
    [[ "$(ipc contextMenuRowCentre "$rest_row")" == "$rest_x $rest_y" ]] || fail "menu: the reopened menu put Rename at $(ipc contextMenuRowCentre "$rest_row"), not $rest_x $rest_y"
    # Where the pointer is, is what the centre above proves. Qt reports no hover on a row built
    # under a pointer that has not moved since, which is the very rule the two checks below read.
    [[ "$(ipc contextMenuCursor)" == "0" ]] || fail "menu: a menu opened under a resting pointer moved its cursor to row $(ipc contextMenuCursor)"
    printf 'MENU probe before the move: %s\n' "$(ipc contextMenuRowProbe "$rest_row")"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 3 -y 0 >/dev/null 2>&1
    settle
    printf 'MENU probe after the move: %s\n' "$(ipc contextMenuRowProbe "$rest_row")"
    [[ "$(ipc contextMenuCursor)" == "$rest_row" ]] || fail "menu: the pointer moved inside row $rest_row (Rename) and the cursor stayed on row $(ipc contextMenuCursor)"
    printf 'MENU pointer rest=0 moved=%s\n' "$rest_row"
    key -k Escape >/dev/null
    settle
    centre=$(ipc rowCentre 0)
    read -r cx cy <<< "$centre"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"

    # Clicking outside still closes through the backdrop after menu pointer ownership changes.
    click_row 0 right
    settle
    centre=$(ipc rowCentre 1)
    read -r _beneath_cx beneath_y <<< "$centre"
    row_left=$(ipc rowLeft 1)
    omarchy-drive click "$((wx + row_left + row_padding_x))" "$((wy + beneath_y))" left >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "an outside click did not close the context menu"

    # Rename's click belongs only to the menu, never to the list row it lies over. Both the row's own
    # index and the list row beneath it are found live: the Menus settings section can move Rename.
    click_row 0 right
    settle
    local rename_index rename_centre rx ry beneath
    rename_index=$(menu_row_index "Rename") || fail "menu: the open menu has no Rename row"
    rename_centre=$(ipc contextMenuRowCentre "$rename_index")
    read -r rx ry <<< "$rename_centre"
    [[ -n "$ry" ]] || fail "menu: the Rename row has no on-screen centre"
    beneath=$(list_row_at_y "$ry")
    [[ -n "$beneath" ]] || fail "menu: no list row lies under Rename, so a pass-through cannot happen"
    omarchy-drive click "$((wx + rx))" "$((wy + ry))" left >/dev/null
    settle
    printf 'MENU rename-over-row index=%s cursor=%s renaming=%s live=%s text=%q beneath=%s\n' \
        "$rename_index" "$(ipc cursor)" "$(ipc renamingIndex)" "$(ipc renameEditorLive)" \
        "$(ipc renameEditorText)" "$(ipc rowAt "$beneath" | cut -d'|' -f1)"
    shot menu-rename-over-row
    [[ "$(ipc cursor)" == "0" ]] || fail "Rename passed its click to row $(ipc cursor)"
    [[ "$(ipc renamingIndex)" == "0" ]] || fail "Rename retargeted or committed row 0, renamingIndex is $(ipc renamingIndex)"
    [[ "$(ipc renameEditorLive)" == "true" ]] || fail "Rename left no live editor on row 0"
    [[ "$(ipc renameEditorText)" == "subdir" ]] || fail "Rename opened over '$(ipc renameEditorText)', not subdir"
    key -k Escape >/dev/null
    settle

    click_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "right click on the directory opened no menu"
    omarchy-drive click "$((cx + wx + row_height))" "$((cy + wy + row_height / 2))" left >/dev/null
    settle
    printf 'MENU chosen path=%q visible=%s message=%q\n' \
        "$(ipc path)" "$(ipc contextMenuVisible)" "$(ipc lastMessage)"
    shot menu-chosen
    [[ "$(ipc path)" == "$dir/subdir" ]] || fail "the Open action did not open the directory"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "the menu stayed open after its action ran"
}

case_clickthrough() {
    local dir="$fixture_root/clickthrough"
    sandbox_scratch "$dir"
    local i
    # Eighty rows overflow the window at every density; forty stopped short of a compact screen's 43.
    for i in $(seq -w 1 80); do : > "$dir/f$i.txt"; done
    # Every menu row shown for Permissions, and a favourite that cannot open, so its click leaves the path; the suite's state home comes back after.
    local suite_state_home="$XDG_STATE_HOME"
    seed_ui_state "$fixture_root/clickthrough-state" '{"menu":{"hidden":[]},"places":{"favourites":[{"label":"Unopenable","path":"relative/unopenable"}]}}'
    launch "$dir"
    wait_listing 80
    local parked; parked=$(ipc cursor)
    [[ "$parked" == "0" ]] || fail "clickthrough: the cursor did not start on row 0, it is on $parked"

    key -k Tab >/dev/null
    settle
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "clickthrough: the rail's a key did not open the network dialog"
    local p
    for p in SFTP FTPS WebDAV NFS SMB; do
        rows_run_under "$(ipc networkCardRect)" "the network card"
        clickthrough_target "$(ipc networkChipCentre "$p")" "$p chip" "$parked"
        click_chip "$p"
        settle
        [[ "$(ipc networkProtocol)" == "$p" ]] || fail "clickthrough: the $p chip did not take its click, protocol is $(ipc networkProtocol)"
        [[ "$(ipc cursor)" == "$parked" && "$(ipc path)" == "$dir" ]] \
            || fail "clickthrough: the $p chip click reached the pane beneath, cursor $(ipc cursor), path $(ipc path)"
    done
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "clickthrough: Escape did not close the network dialog"
    key -k Escape >/dev/null
    settle

    key , >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "clickthrough: the comma key did not open settings"
    local section centre cx cy wx wy
    for section in keys menus display places; do
        rows_run_under "$(ipc settingsCardRect)" "the settings card"
        centre=$(ipc settingsRailRowCentre "$section")
        [[ -n "$centre" ]] || fail "clickthrough: the settings rail has no $section row"
        clickthrough_click "$centre" "$section rail row" "$dir" "$parked"
        [[ "$(ipc settingsSection)" == "$section" ]] || fail "clickthrough: the $section rail row did not take its click, section is $(ipc settingsSection)"
        [[ "$(ipc cursor)" == "$parked" && "$(ipc path)" == "$dir" ]] \
            || fail "clickthrough: the $section rail row click reached the pane beneath, cursor $(ipc cursor), path $(ipc path)"
    done
    clickthrough_click "$(ipc settingsRowCentre favourite:0)" "Places favourite" "$dir" "$parked"
    [[ "$(ipc lastMessage)" == "Could not open Unopenable"* ]] \
        || fail "clickthrough: the Places favourite did not take its click, the footer says $(ipc lastMessage)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "false" ]] || fail "clickthrough: Escape did not close settings"
    clickthrough_permissions "$dir" "$parked"
    clickthrough_delete "$dir" "$parked"
    printf 'CLICKTHROUGH chips=ok rail=ok favourite=ok permissions=ok close=ok delete=ok cursor=%s\n' "$parked"
    kill_flea
    export XDG_STATE_HOME="$suite_state_home"
}

case_wheelunder() {
    local dir="$fixture_root/wheelunder"
    sandbox_scratch "$dir"
    local i
    for i in $(seq -w 1 80); do : > "$dir/f$i.txt"; done
    launch "$dir"
    wait_listing 80
    local wx wy ww wh cx cy
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    read -r cx cy <<< "$(ipc rowCentre 5)"
    omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null
    # The control: with nothing open the same wheel moves the list, so a still list below is not a lost wheel.
    omarchy-drive scroll down 3 >/dev/null
    settle
    [[ "$(ipc listContentY)" != "0" ]] || fail "wheelunder: three notches with nothing open left contentY at 0, so the wheel is not reaching the list"
    key -k Home >/dev/null
    settle
    [[ "$(ipc listContentY)" == "0" ]] || fail "wheelunder: Home did not bring contentY back to 0, it is $(ipc listContentY)"

    wheel_holds() {
        local what="$1" reader="$2"
        omarchy-drive scroll down 3 >/dev/null
        settle
        [[ "$(ipc listContentY)" == "0" ]] || fail "wheelunder: three notches under $what scrolled the listing to contentY $(ipc listContentY)"
        [[ "$(ipc "$reader")" == "true" ]] || fail "wheelunder: the wheel closed $what"
    }
    # 80 rows leave no ground, so the menu is opened on row 5 itself: the same overlay, and the wheel under it is the question.
    read -r cx cy <<< "$(ipc rowCentre 5)"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" right >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "wheelunder: the right click on row 5 opened no menu"
    wheel_holds "the context menu" contextMenuVisible
    key -k Escape >/dev/null
    settle
    key , >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "wheelunder: the comma key did not open settings"
    wheel_holds "the settings card" settingsOpen
    key -k Escape >/dev/null
    settle
    key '?' >/dev/null
    settle
    [[ "$(ipc keymapSheetOpen)" == "true" ]] || fail "wheelunder: the ? key did not open the keymap sheet"
    wheel_holds "the keymap sheet" keymapSheetOpen
    key -k Escape >/dev/null
    settle
    key -k Tab >/dev/null
    settle
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "wheelunder: the rail's a key did not open the network dialog"
    wheel_holds "the network dialog" dialogOpen
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    printf 'WHEELUNDER menu=ok settings=ok keymap=ok network=ok\n'
    kill_flea
}

# The HIG card dismisses through Cancel; every checkbox still receives a native click over the listing.
clickthrough_permissions() {
    local dir="$1" parked="$2" box name centre mode
    local -a boxes
    key m >/dev/null
    settle
    menu_seek Permissions
    key -k Return >/dev/null
    clickthrough_wait permissionsState '.opened and (.busy == false) and .editable' "Permissions did not open editable on row 0"
    rows_run_under "$(ipc permissionsState | jq -r '.rect')" "the Permissions card"
    # Sample reader row: "Owner read<TAB>612 388".
    mapfile -t boxes < <(ipc permissionsState | jq -r '.controls[] | select(.bit != null) | [.name, .centre] | @tsv')
    [[ "${#boxes[@]}" == 9 ]] || fail "clickthrough: Permissions drew ${#boxes[@]} checkboxes, not nine"
    for box in "${boxes[@]}"; do
        IFS=$'\t' read -r name centre <<< "$box"
        mode=$(ipc permissionsState | jq -r '.mode')
        clickthrough_click "$centre" "$name checkbox" "$dir" "$parked"
        [[ "$(ipc permissionsState | jq -r '.mode')" != "$mode" ]] || fail "clickthrough: the $name checkbox did not take its click"
    done
    clickthrough_click "$(ipc permissionsState | jq -r '.controls[] | select(.name == "Cancel") | .centre')" "Permissions Cancel button" "$dir" "$parked"
    clickthrough_wait permissionsState '.opened | not' "Cancel did not close Permissions"
}
