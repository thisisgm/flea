# GPUI overrides for UISH: the HIG boards' title-case menus and wording; seeded from the unchanged oracle at 4d09bd7f5.
menus_visit() {
    local path="$1" count="$2" observed="" deadline=$(( $(date +%s%3N) + async_wait_ms ))
    menus_guard "$path"
    key -M ctrl -k l -m ctrl >/dev/null || fail 'menus: the address shortcut failed'
    menus_expect pathBarOpen '. == true' 'the address field opens before typing'
    key "$path" >/dev/null || fail 'menus: address text delivery failed'
    while (( $(date +%s%3N) < deadline )); do
        observed=$(ipc pathBarText) || fail 'menus: the address text reader failed'
        [[ "$observed" == "$path" ]] && break
        sleep 0.05
    done
    [[ "$observed" == "$path" ]] || fail "menus: the address field reads $observed, expected $path"
    key -k Return >/dev/null || fail 'menus: address confirmation failed'
    wait_path "$path"
    wait_listing "$count"
}

# GPUI narrows the drawn rows on the frame after the query lands, so the count is polled within async_wait_ms.
querypaste_drawn() {
    local deadline=$(( $(date +%s%3N) + async_wait_ms ))
    until [[ "$(ipc drawnCount 2>/dev/null)" == "$1" ]]; do
        (( $(date +%s%3N) < deadline )) || return 1
        sleep 0.05
    done
}

case_querypaste() {
    local dir="$fixture_root/querypaste" paste_timeout_s=3 filter_query search_query
    command -v wl-copy >/dev/null || fail "querypaste: wl-copy is missing, so no clipboard paste can be staged"
    sandbox_scratch "$dir"
    mkdir -p "$dir/sub"
    printf 'notes\n' > "$dir/repo-notes.txt"
    printf 'plan\n' > "$dir/repo-plan.md"
    printf 'other\n' > "$dir/other.txt"
    launch "$dir"
    wait_listing 4
    # The clipboard carries two lines; the query lines take only the first.
    printf 'repo\nsecond line' | timeout "$paste_timeout_s" wl-copy \
        || fail "querypaste: the two-line paste source never reached the clipboard"
    # The filter opens on its keys.toml text key.
    key / >/dev/null || fail "querypaste: the filter key never reached the window"
    settle
    # Ctrl+V pastes the first line through the filter's typed path.
    key -M ctrl -k v -m ctrl >/dev/null || fail "querypaste: Ctrl+V never reached the filter line"
    settle
    filter_query=$(ipc keyDeliveryState 2>/dev/null | jq -er '.filterQuery') \
        || fail "querypaste: the filter query has no IPC reader answer"
    [[ "$filter_query" == "repo" ]] || fail "querypaste: the filter query reads '$filter_query', not 'repo'"
    querypaste_drawn 2 || fail "querypaste: the filter shows $(ipc drawnCount 2>/dev/null) rows, not 2"
    shot "querypaste-filter"
    # Two Escapes leave the filter line and restore the full listing.
    key -k Escape -k Escape >/dev/null || fail "querypaste: Escape never reached the filter line"
    settle
    # Shift+Insert pastes a new first line into the reopened filter, which must open empty.
    printf 'plan\nsecond line' | timeout "$paste_timeout_s" wl-copy \
        || fail "querypaste: the second paste source never reached the clipboard"
    key / >/dev/null || fail "querypaste: the filter key never reopened the filter line"
    settle
    filter_query=$(ipc keyDeliveryState 2>/dev/null | jq -er '.filterQuery') \
        || fail "querypaste: the reopened filter query has no IPC reader answer"
    [[ -z "$filter_query" && "$(ipc drawnCount 2>/dev/null)" == "4" ]] \
        || fail "querypaste: the reopened filter holds '$filter_query' over $(ipc drawnCount 2>/dev/null) rows, not empty over 4"
    key -M shift -k Insert -m shift >/dev/null || fail "querypaste: Shift+Insert never reached the filter line"
    settle
    filter_query=$(ipc keyDeliveryState 2>/dev/null | jq -er '.filterQuery') \
        || fail "querypaste: the reopened filter query has no IPC reader answer"
    [[ "$filter_query" == "plan" ]] || fail "querypaste: Shift+Insert left the filter query at '$filter_query', not 'plan'"
    querypaste_drawn 1 || fail "querypaste: the pasted filter shows $(ipc drawnCount 2>/dev/null) rows, not 1"
    key -k Escape -k Escape >/dev/null || fail "querypaste: Escape never left the reopened filter"
    settle
    # The search opens empty on its keys.toml text key and takes a third paste.
    printf 'other\nsecond line' | timeout "$paste_timeout_s" wl-copy \
        || fail "querypaste: the third paste source never reached the clipboard"
    key f >/dev/null || fail "querypaste: the search key never reached the window"
    settle
    search_query=$(ipc keyDeliveryState 2>/dev/null | jq -er '.searchQuery') \
        || fail "querypaste: the search query has no IPC reader answer"
    [[ -z "$search_query" ]] || fail "querypaste: the search opened holding '$search_query', not empty"
    key -M ctrl -k v -m ctrl >/dev/null || fail "querypaste: Ctrl+V never reached the search line"
    settle
    search_query=$(ipc keyDeliveryState 2>/dev/null | jq -er '.searchQuery') \
        || fail "querypaste: the search query has no IPC reader answer"
    [[ "$search_query" == "other" ]] || fail "querypaste: the search query reads '$search_query', not 'other'"
    shot "querypaste-search"
    key -k Escape >/dev/null || fail "querypaste: Escape never left the search"
    settle
    printf 'QUERYPASTE filter=%s search=%s\n' "$filter_query" "$search_query"
}

# actions-menu board: Actions takes Run Script's place and lists the scripts.
case_runscript() {
    local dir="$fixture_root/runscript"
    sandbox_scratch "$dir"
    printf 'one\n' > "$dir/a.txt"
    printf 'two\n' > "$dir/b.txt"
    local config="$fixture_root/runscript-config"
    sandbox_scratch "$config"
    mkdir -p "$config/flea/scripts"
    export XDG_CONFIG_HOME="$config"
    # Three, one of them not executable and one failing, which is the whole of the rule's own edges.
    printf '#!/bin/sh\nprintf "%%s\\n" "$@" > %s/ran.log\nprintf "%%s\\n" "$PWD" >> %s/ran.log\n' "$dir" "$dir" > "$config/flea/scripts/stamp.sh"
    printf '#!/bin/sh\nprintf "no such page\\n" >&2\nexit 2\n' > "$config/flea/scripts/ocr.sh"
    printf '#!/bin/sh\nexit 0\n' > "$config/flea/scripts/not-executable.sh"
    chmod +x "$config/flea/scripts/stamp.sh" "$config/flea/scripts/ocr.sh"
    # The switch on: everything else in the shipped set stays as it is.
    seed_ui_state "$fixture_root/runscript-state" '{"menu":{"hidden":["delete","openTerminal","placeMenu","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs"]}}'

    launch "$dir"
    wait_listing 2
    seek_row_named "a.txt"
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "runscript: the row's right click opened no menu"
    [[ "$(ipc contextMenuEntries)" == *"Actions"* ]] \
        || fail "runscript: the menu offers $(ipc contextMenuEntries)"
    menu_seek "Actions"
    key -k Return >/dev/null
    settle
    # Sorted by name, the label without the extension, and the file that is not executable absent.
    [[ "$(ipc contextMenuSubmenuEntries)" == "ocr|stamp" ]] \
        || fail "runscript: the submenu offers $(ipc contextMenuSubmenuEntries)"

    echo "-- a row runs its script with the selected paths, in the first one's folder --"
    menu_seek_submenu "stamp"
    key -k Return >/dev/null
    for _attempt in $(seq 1 40); do [[ -s "$dir/ran.log" ]] && break; sleep 0.25; done
    [[ -s "$dir/ran.log" ]] || fail "runscript: the script never ran, the bar says $(ipc lastMessage)"
    [[ "$(head -1 "$dir/ran.log")" == "$dir/a.txt" ]] \
        || fail "runscript: the script was handed $(head -1 "$dir/ran.log")"
    [[ "$(tail -1 "$dir/ran.log")" == "$dir" ]] \
        || fail "runscript: the script ran in $(tail -1 "$dir/ran.log"), not the file's own folder"
    printf 'RUNSCRIPT args=%s cwd=%s\n' "$(head -1 "$dir/ran.log")" "$(tail -1 "$dir/ran.log")"

    echo "-- and a non-zero exit is its own last stderr line --"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Actions"
    key -k Return >/dev/null
    settle
    menu_seek_submenu "ocr"
    key -k Return >/dev/null
    wait_message "ocr.sh · no such page"
    printf 'RUNSCRIPT said=%s\n' "$(ipc lastMessage)"

    echo "-- an empty directory offers no row at all --"
    rm -f "$config/flea/scripts/stamp.sh" "$config/flea/scripts/ocr.sh"
    click_row "$(ipc cursor)" right
    settle
    key -k Escape >/dev/null
    settle
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuEntries)" != *"Actions"* ]] \
        || fail "runscript: an empty directory still offers $(ipc contextMenuEntries)"
    printf 'RUNSCRIPT empty=%s\n' "$(ipc contextMenuEntries)"
    kill_flea
}

# APL-MENU-04: the background menu rows take title case and Settings… its ellipsis.
case_background() {
    local dir="$fixture_root/background"
    sandbox_scratch "$dir"
    mkdir -p "$dir/dest"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/c.txt"
    # Every row list below is the shipped column, and ui/js/Menu.js applyHidden builds it from
    # menu.hidden, which is operator state: without this seed the assertions read the operator's own
    # Menus section and fail on a box that has switched any of the seven back on.
    local real_state="${XDG_STATE_HOME-}"
    seed_ui_state "$fixture_root/background-state" "{\"menu\":{\"hidden\":$menu_shipped}}"
    launch "$dir"
    wait_listing 4
    key -k y >/dev/null
    settle

    click_background
    settle
    printf 'BACKGROUND visible=%s entries=%s glyphs=%s hints=%q\n' \
        "$(ipc contextMenuVisible)" "$(ipc contextMenuEntries)" \
        "$(ipc contextMenuGlyphs)" "$(ipc contextMenuHints)"
    shot background-menu
    printf 'BACKGROUND_OCR_BEGIN\n'
    omarchy-drive ocr flea || true
    printf 'BACKGROUND_OCR_END\n'
    [[ "$(ipc contextMenuVisible)" == "true" ]] \
        || fail "background: a right click on empty space opened no menu"
    # Menus.html's background column: New folder and New file lead it, and Add to Favorites stays.
    [[ "$(ipc contextMenuEntries)" == "New Folder|New File|-|Paste|Select All|-|Open in Terminal|Add to Favorites|-|Sort By|Show Hidden Files|Hide Code Icons|-|Settings…" ]] \
        || fail "background: the menu is not the board's column, it is $(ipc contextMenuEntries)"
    [[ "$(ipc contextMenuGlyphs)" == "folder-plus|file-plus|-|clipboard|check|-|terminal|star|-|sort|eye|code|-|sliders" ]] \
        || fail "background: a row lost its mark, the set is $(ipc contextMenuGlyphs)"
    # A right click ON a row still gets that row's own menu: the two entrances share one instance,
    # so a hasRow left standing from the last open would be the defect this asserts against.
    key -k Escape >/dev/null
    settle
    click_row 1 right
    settle
    [[ "$(ipc contextMenuEntries)" == Open\|* ]] \
        || fail "background: a row's own menu came back as $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    # Sort by, the one submenu row. Its flyout is the three orders the backend can produce, and the
    # order the listing lands in is read off the header's own mark, not off a row's contents.
    click_background
    settle
    menu_seek "Sort by"
    key -k Return >/dev/null
    settle
    printf 'BACKGROUND sort from=%s flyout=%s glyphs=%s\n' \
        "$(ipc sortMark)" "$(ipc contextMenuSubmenuEntries)" "$(ipc contextMenuSubmenuGlyphs)"
    shot background-sort
    # Four orders, because src/backend/ordering.rs answers kind as well as the three in sort.rs.
    [[ "$(ipc contextMenuSubmenuEntries)" == "Name|Size|Modified|Kind" ]] \
        || fail "background: the Sort by flyout is $(ipc contextMenuSubmenuEntries)"
    # The background-menu board (CHANGES.md, APL-MENU-22) marks only the order in effect, with a checkmark.
    [[ "$(ipc sortMark)" == "name:asc" ]] \
        || fail "background: the case expects name:asc before the flyout, read $(ipc sortMark)"
    [[ "$(ipc contextMenuSubmenuGlyphs)" == "checkmark|||" ]] \
        || fail "background: the sort flyout drew $(ipc contextMenuSubmenuGlyphs), not checkmark|||"
    key -k Down >/dev/null
    key -k Return >/dev/null
    settle
    printf 'BACKGROUND sorted to=%s\n' "$(ipc sortMark)"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "background: choosing an order left the menu open"
    [[ "$(ipc sortMark)" == "size:asc" ]] \
        || fail "background: Sort by Size left the listing in $(ipc sortMark)"
    # And back to the order this case found, which the steps below read row numbers against. It is a
    # literal and not a saved reading because ui/Backend.qml sets sortBy to name and sortDesc to
    # false on every list and nothing writes the order to ui.json, so name ascending is what every
    # window starts in; the flip above lives and dies with this window.
    menu_click "Sort by" name
    [[ "$(ipc sortMark)" == "name:asc" ]] \
        || fail "background: Sort by Name left the listing in $(ipc sortMark)"

    # Select all, clicked rather than keyed: a row only the keyboard can reach is not the row the
    # board drew. Escape then hands the listing back the empty selection the steps below want.
    menu_click "Select all"
    printf 'BACKGROUND selectall count=%s total=%s\n' "$(ipc selectionCount)" "$(ipc total)"
    [[ "$(ipc selectionCount)" == "$(ipc total)" ]] \
        || fail "background: Select all selected $(ipc selectionCount) of $(ipc total)"
    key -k Escape >/dev/null
    settle

    # Paste, with something really on the clipboard and a destination of its own, so the row is
    # exercised doing work rather than only answering the empty-clipboard sentence. Directories sort
    # first, so under name ascending row 0 is dest and row 1 is a.txt.
    click_row 1 left
    settle
    key y >/dev/null
    settle
    click_row 0 left
    settle
    key -k Return >/dev/null
    # wait_listing cannot answer for an empty directory: rowAt 0 has no delegate to describe and
    # says "loading" forever, so the empty state's own flag is what says the listing arrived.
    for _attempt in $(seq 1 300); do
        [[ "$(ipc emptyShown)" == "true" && "$(ipc path)" == "$dir/dest" ]] && break
        sleep 0.05
    done
    [[ "$(ipc path)" == "$dir/dest" ]] || fail "background: the case is in $(ipc path), not $dir/dest"
    [[ "$(ipc total)" == "0" ]] || fail "background: $dir/dest listed $(ipc total) rows, not 0"
    # The mark has to paint, not only be flagged (a z below the view's paint once hid it), and its draw starts blank, so the shot is polled across one replay.
    local mark lit=0 shots=0 captured=1 png="$evidence_dir/background-empty.png"
    mark=$(ipc emptyMarkRect)
    set -- $mark
    # A stale image at this path from an earlier run would score as a live one, so each shot is a fresh file or a failure.
    rm -f "$png"
    for _attempt in $(seq 1 "$mark_poll_shots"); do
        shots=$_attempt
        omarchy-drive shot "$png" flea >/dev/null || { captured=0; break; }
        lit=$(count_pixels "$png" "${3}x${4}+${1}+${2}" "((r+g+b)/3) > 0.25")
        (( lit > 0 )) && break
        sleep "$mark_poll_s"
    done
    printf 'SHOT %s\nBACKGROUND mark rect=%s lit=%s shots=%s captured=%s\n' "$png" "$mark" "$lit" "$shots" "$captured"
    (( captured )) || fail "background: omarchy-drive shot failed on shot $shots of $png"
    (( ! captured || lit > 0 )) || fail "background: the empty mark painted no pixel inside ${3}x${4}+${1}+${2} across $shots shots"
    # The empty listing is also the strongest case for this menu, and it has no row to aim from.
    click_background
    settle
    [[ "$(ipc contextMenuEntries)" == "New Folder|New File|-|Paste|Select All|-|Open in Terminal|Add to Favorites|-|Sort By|Show Hidden Files|Hide Code Icons|-|Settings…" ]] \
        || fail "background: an empty directory drew $(ipc contextMenuEntries)"
    # Its own name: the empty-mark poll above already owns background-empty.png.
    shot background-empty-menu
    key -k Escape >/dev/null
    settle
    menu_click "Paste"
    settle
    printf 'BACKGROUND paste landed=%s message=%q\n' \
        "$([[ -f "$dir/dest/a.txt" ]] && echo yes || echo no)" "$(ipc lastMessage)"
    [[ -f "$dir/dest/a.txt" ]] || fail "background: Paste put nothing in $dir/dest"

    # New folder, the only background row that writes on its own, so the directory is the proof.
    [[ ! -e "$dir/dest/New Folder" ]] || fail "background: New Folder existed before the row ran"
    menu_click "New folder"
    settle
    printf 'BACKGROUND newfolder made=%s message=%q\n' \
        "$([[ -d "$dir/dest/New Folder" ]] && echo yes || echo no)" "$(ipc lastMessage)"
    [[ -d "$dir/dest/New Folder" ]] || fail "background: New folder created nothing in $dir/dest"

    # Show hidden files, the row the Menus board locks, read off the state it flips and flipped back
    # through its own changed label so the case leaves the listing as it found it.
    local was
    was=$(ipc showHidden)
    menu_click "Show hidden files"
    printf 'BACKGROUND hidden %s -> %s\n' "$was" "$(ipc showHidden)"
    [[ "$(ipc showHidden)" != "$was" ]] || fail "background: the hidden toggle stayed $was"
    menu_click "Hide hidden files"
    [[ "$(ipc showHidden)" == "$was" ]] || fail "background: the hidden toggle did not flip back"

    # The grid and the columns view carry the same entrance, because the board draws one menu and
    # not a list-view menu: a right click on empty space means the same thing in all three.
    local view
    for view in grid columns; do
        key -M ctrl -k "$([[ "$view" == grid ]] && echo 3 || echo 2)" -m ctrl >/dev/null
        settle
        [[ "$(ipc viewMode)" == "$view" ]] || fail "background: the $view view did not come up"
        click_background
        settle
        printf 'BACKGROUND %s entries=%s\n' "$view" "$(ipc contextMenuEntries)"
        shot "background-$view"
        [[ "$(ipc contextMenuEntries)" == "New Folder|New File|-|Paste|Select All|-|Open in Terminal|Add to Favorites|-|Sort By|Show Hidden Files|Hide Code Icons|-|Settings…" ]] \
            || fail "background: the $view view drew $(ipc contextMenuEntries)"
        key -k Escape >/dev/null
        settle
    done

    # Issue 195 (@muellan): past the first screenful a row's right click raised this background menu
    # instead of the row's, in every view, because the handler added contentY to a point that was
    # already in content coordinates. Measured on 0.3.4: 52 of 52 scrolled rows. 150 rows overflow any window.
    local scrolled="$fixture_root/background-scrolled" i last
    sandbox_scratch "$scrolled"
    mkdir -p "$scrolled"
    for i in $(seq -w 1 150); do : > "$scrolled/f$i.txt"; done
    launch "$scrolled"
    wait_listing 150
    for view in list grid columns; do
        key -M ctrl -k "$(case "$view" in list) echo 1 ;; columns) echo 2 ;; grid) echo 3 ;; esac)" -m ctrl >/dev/null
        settle
        [[ "$(ipc viewMode)" == "$view" ]] || fail "background: the $view view did not come up over 150 rows"
        key -k End >/dev/null
        settle
        last=$(( $(ipc total) - 1 ))
        (( $(ipc viewContentY) > 0 )) || fail "background: End left the $view view unscrolled"
        click_row "$last" right
        settle
        printf 'BACKGROUND scrolled %s contentY=%s row=%s entries=%s\n' "$view" "$(ipc viewContentY)" "$last" "$(ipc contextMenuEntries)"
        [[ "$(ipc contextMenuEntries)" == Open\|* ]] \
            || fail "background: a right click on the last row of the scrolled $view view opened $(ipc contextMenuEntries)"
        key -k Escape >/dev/null
        settle
    done
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
}

case_hidden() {
    local dir="$fixture_root/hidden"
    sandbox_scratch "$dir"
    : > "$dir/.dotfile"
    : > "$dir/visible.txt"
    launch "$dir"
    wait_listing 1
    [[ "$(ipc showHidden)" == "false" ]] || fail "hidden: showHidden did not default to false"
    [[ "$(ipc rowAt 0)" == "visible.txt|"* ]] || fail "hidden: the dotfile leaked into the default listing, row 0 is $(ipc rowAt 0)"

    click_row 0 right
    settle
    # This case cares about one row, so it looks for that row rather than matching the whole menu:
    # which other entries are present depends on the box's live tailnet and on how many of the
    # operations design's own rows have shipped, and neither is this case's concern.
    [[ "$(ipc contextMenuEntries)" == *"Show Hidden Files"* ]] \
        || fail "hidden: the menu did not offer Show hidden files, got $(ipc contextMenuEntries)"
    shot hidden-menu-off
    key -k Escape >/dev/null
    settle

    key . >/dev/null
    wait_listing 2
    [[ "$(ipc showHidden)" == "true" ]] || fail "hidden: . did not flip showHidden on"
    [[ "$(ipc cursor)" == "0" ]] || fail "hidden: the toggle did not reset the cursor, it is $(ipc cursor)"
    shot hidden-shown

    click_row 0 right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Hide Hidden Files"* ]] \
        || fail "hidden: the menu label did not flip to Hide hidden files, got $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    key . >/dev/null
    wait_listing 1
    [[ "$(ipc showHidden)" == "false" ]] || fail "hidden: a second . did not flip back off"
    [[ "$(ipc rowAt 0)" == "visible.txt|"* ]] || fail "hidden: toggling back off left the dotfile visible, row 0 is $(ipc rowAt 0)"

    printf 'HIDDEN default=ok toggle-on=ok menu-label=ok toggle-off=ok\n'
    kill_flea
}

# reload-notice board: the notice counts items, where v0.3.12 counted rows.
case_reload() {
    local dir="$fixture_root/reload" chord before after errors message
    sandbox_scratch "$dir"
    : > "$dir/a.txt"
    launch "$dir"
    wait_listing 1

    for chord in F5 Ctrl+R; do
        before=$(ipc listRequests)
        if [[ "$chord" == F5 ]]; then
            key -k F5 >/dev/null
        else
            hotkey --global ctrl r flea >/dev/null
        fi
        settle
        errors=$(grep -E 'TypeError|ReferenceError' "$flea_log" | grep -E 'Reload\.js|Focus\.js|Pane\.qml' || true)
        [[ -z "$errors" ]] || fail "reload: $chord raised $errors"
        after=$(ipc listRequests)
        [[ "$after" =~ ^[0-9]+$ && "$before" =~ ^[0-9]+$ ]] || fail "reload: $chord returned invalid listRequests ($before to $after)"
        (( after == before + 1 )) || fail "reload: no single re-list after $chord, listRequests $before to $after"
        wait_listing 1
        [[ "$(ipc path)" == "$dir" ]] || fail "reload: $chord left the listing path; observed [$(ipc path)], expected [$dir]"
        message=$(ipc lastMessage)
        [[ "$message" != *Reloaded* ]] || fail "reload: $chord announced unchanged rows: $message"
        printf 'RELOAD key=%s lists=%s-to-%s unchanged=quiet log=clean\n' "$chord" "$before" "$after"
    done

    # The watcher re-reads 400 ms after a change and no hold lets a key through, so the owned backend is held: the reload's list is sent before the change is reported.
    local qs_pid pid held_pid="" held_requests held_inflight
    qs_pid=$(flea_pid)
    # Sample input: /proc/<pid>/cmdline "/usr/bin/flea\0--backend\0"; ViewState's writer is a flea child too, run as --ui-state.
    for pid in $(pgrep -P "$qs_pid" -x flea); do
        tr '\0' ' ' 2>/dev/null < "/proc/$pid/cmdline" | grep -Fq -- ' --backend ' && held_pid="$held_pid $pid"
    done
    held_pid=${held_pid# }
    [[ "$held_pid" =~ ^[0-9]+$ ]] || fail "reload: expected one flea --backend child of qs $qs_pid to hold, found '$held_pid'"
    before=$(ipc listRequests)
    # The pid is expanded here because the trap runs after this function's locals are gone.
    trap "kill -CONT $held_pid 2>/dev/null || true" EXIT
    kill -STOP "$held_pid" || fail "reload: could not stop backend $held_pid, so the watcher would report the change first"
    : > "$dir/b.txt"
    key -k F5 >/dev/null
    settle
    held_requests=$(ipc listRequests)
    held_inflight=$(ipc listInFlight)
    kill -CONT "$held_pid" || fail "reload: could not resume backend $held_pid"
    trap - EXIT
    [[ "$held_requests" == "$((before + 1))" && "$held_inflight" == true ]] \
        || fail "reload: with the backend held F5 sent $before to $held_requests list requests, in flight $held_inflight, want one unanswered request"
    settle
    errors=$(grep -E 'TypeError|ReferenceError' "$flea_log" | grep -E 'Reload\.js|Focus\.js|Pane\.qml' || true)
    [[ -z "$errors" ]] || fail "reload: changed-row F5 raised $errors"
    after=$(ipc listRequests)
    [[ "$after" =~ ^[0-9]+$ ]] || fail "reload: changed-row F5 returned invalid listRequests: $after"
    (( after > before )) || fail "reload: no re-list after changed-row F5, listRequests stayed $before"
    wait_listing 2
    message=$(ipc lastMessage)
    [[ "$message" == "Reloaded · 1 item changed" ]] || fail "reload: changed-row F5 said '$message', expected 'Reloaded · 1 item changed'"
    [[ "$(ipc path)" == "$dir" ]] || fail "reload: changed-row F5 left the listing path; observed [$(ipc path)], expected [$dir]"
    [[ "$(ipc rowAt 1)" == b.txt\|* ]] || fail "reload: changed-row F5 did not draw b.txt; observed [$(ipc rowAt 1)]"
    printf 'RELOAD changed-row F5=ok notice=%s log=clean\n' "$message"
    kill_flea
}

# The transport gives the window a sandbox HOME, so Home is read from the window.
case_focus() {
    local dir="$fixture_root/focus"
    sandbox_scratch "$dir"
    : > "$dir/plain.txt"
    launch "$dir"
    wait_listing 1
    [[ "$(ipc focusView)" == "list" ]] || fail "focus: Flea does not start on the list"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "focus: tab did not reach the rail"
    # Home plus real favourites, so cursorDown and cursorUp both have somewhere to land; avoids
    # racing the rail's two FileViews, which load after Sidebar exists and not before.
    wait_rail 3
    shot focus-railed
    key jj >/dev/null
    settle
    [[ "$(ipc railCursor)" == "2" ]] || fail "focus: j did not move the rail cursor"
    key k >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "focus: k did not move the rail cursor back up"
    key k >/dev/null
    settle
    [[ "$(ipc railCursor)" == "0" ]] || fail "focus: k did not return the rail cursor to Home"
    # Row 0 is always Home, which is $HOME on this box, so the opened path is predictable.
    key -k Return >/dev/null
    local home
    home=$(window_home) || fail "focus: the window's HOME is unreadable"
    wait_path "$home"
    [[ "$(ipc path)" == "$home" ]] || fail "focus: Enter on Home did not open $home, path is $(ipc path)"
    # #181: opening a rail row hands focus to the folder it opened, so the next key moves in Home.
    [[ "$(ipc focusView)" == "list" ]] || fail "focus: Enter on a favourite left focus on $(ipc focusView), not the list"
    shot focus-opened
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "focus: tab did not return to the rail after an open"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "focus: escape did not return to the list"
    printf 'FOCUS view=%s railCursor=%s path=%s\n' "$(ipc focusView)" "$(ipc railCursor)" "$(ipc path)"
    shot focus-listed
    kill_flea
}

case_columnsbackground() {
    local root="$fixture_root/columnsbackground" state="$fixture_root/columnsbackground-state"
    columnsbg_fixture "$root"
    seed_ui_state "$state" "{\"menu\":{\"hidden\":$menu_shipped}}"

    # Parent: empty space in P's column navigates to P, then New Folder lands in P alone.
    launch "$root/P/A"
    wait_listing 3
    columnsbg_to_columns
    seek_row_named "B"
    settle
    columnsbg_click_column "$(ipc columnParentRowCentre 0)"
    columnsbg_wait_menu_on "$root/P"
    [[ "$(ipc contextMenuEntries)" == *"New Folder"* ]] || fail "columnsbackground: parent menu lacks New folder in $(ipc contextMenuEntries)"
    columnsbg_choose_new_folder
    settle
    [[ -d "$root/P/New Folder" ]] || fail "columnsbackground: parent New Folder created nothing in $root/P"
    [[ ! -e "$root/P/A/New Folder" ]] || fail "columnsbackground: parent menu wrote into the active directory"
    [[ ! -e "$root/P/A/B/New Folder" ]] || fail "columnsbackground: parent menu wrote into the child directory"
    printf 'COLUMNSBACKGROUND parent=%s\n' "$(ls -A "$root/P" | tr '\n' '|')"
    key -k Escape >/dev/null
    settle
    kill_flea

    # Current: the middle column's own background still creates in A alone.
    columnsbg_fixture "$root"
    launch "$root/P/A"
    wait_listing 3
    columnsbg_to_columns
    click_background
    settle
    columnsbg_wait_menu_on "$root/P/A"
    columnsbg_choose_new_folder
    settle
    [[ -d "$root/P/A/New Folder" ]] || fail "columnsbackground: active New Folder created nothing in $root/P/A"
    [[ ! -e "$root/P/New Folder" ]] || fail "columnsbackground: active menu wrote into the parent directory"
    [[ ! -e "$root/P/A/B/New Folder" ]] || fail "columnsbackground: active menu wrote into the child directory"
    printf 'COLUMNSBACKGROUND active=%s\n' "$(ls -A "$root/P/A" | tr '\n' '|')"
    key -k Escape >/dev/null
    settle
    kill_flea

    # Child: empty space in B's column navigates to B, then New Folder lands in B alone.
    columnsbg_fixture "$root"
    launch "$root/P/A"
    wait_listing 3
    columnsbg_to_columns
    seek_row_named "B"
    settle
    columnsbg_click_column "$(ipc columnChildRowCentre 0)"
    columnsbg_wait_menu_on "$root/P/A/B"
    [[ "$(ipc contextMenuEntries)" == *"New Folder"* ]] || fail "columnsbackground: child menu lacks New folder in $(ipc contextMenuEntries)"
    columnsbg_choose_new_folder
    settle
    [[ -d "$root/P/A/B/New Folder" ]] || fail "columnsbackground: child New Folder created nothing in $root/P/A/B"
    [[ ! -e "$root/P/New Folder" ]] || fail "columnsbackground: child menu wrote into the parent directory"
    [[ ! -e "$root/P/A/New Folder" ]] || fail "columnsbackground: child menu wrote into the active directory"
    printf 'COLUMNSBACKGROUND child=%s\n' "$(ls -A "$root/P/A/B" | tr '\n' '|')"
    key -k Escape >/dev/null
    settle
    kill_flea

    # Empty active: the middle column's own background still creates in an empty E alone.
    columnsbg_fixture "$root"
    launch "$root/P/A/E"
    wait_listing 0
    columnsbg_to_columns
    click_background
    settle
    columnsbg_wait_menu_on "$root/P/A/E"
    columnsbg_choose_new_folder
    settle
    [[ -d "$root/P/A/E/New Folder" ]] || fail "columnsbackground: empty active New Folder created nothing in $root/P/A/E"
    [[ ! -e "$root/P/New Folder" ]] || fail "columnsbackground: empty active menu wrote into the parent directory"
    [[ ! -e "$root/P/A/New Folder" ]] || fail "columnsbackground: empty active menu wrote beside the empty directory"
    printf 'COLUMNSBACKGROUND empty-active=%s\n' "$(ls -A "$root/P/A/E" | tr '\n' '|')"
    key -k Escape >/dev/null
    settle
    kill_flea

    # Empty child: E draws no row, so its x comes from the column pitch (active minus parent); New Folder still lands in E alone.
    columnsbg_fixture "$root"
    launch "$root/P/A"
    wait_listing 3
    columnsbg_to_columns
    seek_row_named "E"
    settle
    [[ -z "$(ipc columnChildRowCentre 0 2>/dev/null)" ]] || fail "columnsbackground: the empty child drew a row centre"
    parent_cx=$(ipc columnParentRowCentre 0 | cut -d' ' -f1)
    active_cx=$(ipc rowCentre "$(ipc cursor)" | cut -d' ' -f1)
    [[ -n "$parent_cx" && -n "$active_cx" ]] || fail "columnsbackground: no column pitch to aim the empty child with"
    columnsbg_click_x "$((active_cx + active_cx - parent_cx))"
    columnsbg_wait_menu_on "$root/P/A/E"
    [[ "$(ipc contextMenuEntries)" == *"New Folder"* ]] || fail "columnsbackground: empty child menu lacks New folder in $(ipc contextMenuEntries)"
    columnsbg_choose_new_folder
    settle
    [[ -d "$root/P/A/E/New Folder" ]] || fail "columnsbackground: empty child New Folder created nothing in $root/P/A/E"
    [[ ! -e "$root/P/New Folder" ]] || fail "columnsbackground: empty child menu wrote into the parent directory"
    [[ ! -e "$root/P/A/New Folder" ]] || fail "columnsbackground: empty child menu wrote into the active directory"
    printf 'COLUMNSBACKGROUND empty-child=%s\n' "$(ls -A "$root/P/A/E" | tr '\n' '|')"
    key -k Escape >/dev/null
    settle
    kill_flea

    printf 'COLUMNSBACKGROUND parent+active+child+empty-active+empty-child=ok\n'
}

# Sample statusActivityState: {"errors":2}; Escape acknowledges exactly the currently displayed error.
menus_acknowledge() {
    local shown
    shown=$(ipc statusError) || fail 'menus: the statusError reader failed'
    if [[ "$shown" == true ]]; then
        local before
        before=$(ipc statusActivityState | jq -er '.errors') || fail "menus: error count unavailable"
        (( before > 0 )) || fail "menus: displayed error is absent from its queue"
        key -k Escape >/dev/null
        menus_expect statusActivityState ".errors == $((before - 1))" "Escape acknowledges the current queued error"
    fi
}
