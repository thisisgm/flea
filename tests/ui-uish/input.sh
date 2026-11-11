# GPUI input uses literal shifted text, with the pinned range and overlay assertions retained.
case_shiftscroll() {
    # The wheel over the list until row 5 leaves the viewport and row 150 is on screen.
    shiftscroll_wheel() {
        local wx wy ww wh attempt
        # Sample input: 100 80 1100 700 (native window x, y, width, height).
        read -r wx wy ww wh < <(window_box) || fail "shiftscroll: native window coordinates unavailable"
        # omarchy-drive scroll takes no point: warp there, then one uinput pixel so Qt sees a pointer frame, as case_scroll does.
        hypr_cursor_move "$((wx + ww / 2 - hypr_pointer_frame_offset))" "$((wy + wh / 2))" || fail "shiftscroll: pointer motion failed"
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
        settle
        for attempt in $(seq 1 20); do
            if [[ -z "$(ipc rowCentre 5)" && -n "$(ipc rowCentre 150)" ]]; then return 0; fi
            omarchy-drive scroll down 5 >/dev/null
            settle
        done
        fail "shiftscroll: after the wheel row 5 centre is [$(ipc rowCentre 5)] and row 150 centre is [$(ipc rowCentre 150)]"
    }
    command -v wl-paste >/dev/null && command -v wl-copy >/dev/null \
        || fail "shiftscroll: wl-paste or wl-copy is missing, so the copied uris cannot be checked"
    local dir="$fixture_root/shiftscroll"
    sandbox_scratch "$dir"
    # Zero-padded names keep name order the same as index order.
    local i
    for i in $(seq -w 0 499); do : > "$dir/f$i.txt"; done
    launch "$dir"
    wait_listing 500
    settle
    # A plain click marks row 5 alone, the start every range below is measured from.
    click_row 5 left
    settle
    [[ "$(ipc rowAt 5)" == "f005.txt|"* ]] \
        || fail "shiftscroll: row 5 reads $(ipc rowAt 5), not f005.txt"
    [[ "$(ipc selectedIndices)" == "5" ]] \
        || fail "shiftscroll: a plain click selected '$(ipc selectedIndices)', not row 5 alone"
    shiftscroll_wheel
    # The wheel pulled the cursor onto the viewport; the range must still start at row 5.
    local pulled=$(ipc cursor)
    printf 'SHIFTSCROLL pulled cursor=%s total=%s\n' "$pulled" "$(ipc total)"
    (( pulled > 5 )) && [[ -n "$(ipc rowCentre "$pulled")" ]] \
        || fail "shiftscroll: the wheel left the cursor on row $pulled, not pulled past row 5 onto the viewport"
    [[ "$(ipc selectedIndices)" == "5" ]] \
        || fail "shiftscroll: after the wheel the marks read '$(ipc selectedIndices)', not row 5 alone"
    # The last row with an on-screen centre, the way click_row proves a row is clickable.
    local clicked=$pulled _attempt
    for _attempt in $(seq 1 500); do
        [[ -n "$(ipc rowCentre $((clicked + 1)))" ]] || break
        clicked=$((clicked + 1))
    done
    (( clicked >= 105 )) \
        || fail "shiftscroll: the last visible row is $clicked, under 105, so no row 100 past row 5 is on screen"
    click_row "$clicked" left --mods shift
    settle
    local want_count=$((clicked - 5 + 1))
    [[ "$(ipc selectionCount)" == "$want_count" ]] \
        || fail "shiftscroll: shift+click to row $clicked selected $(ipc selectionCount) rows, not $want_count from row 5"
    [[ "$(ipc selectedIndices)" == 5,6,7* ]] \
        || fail "shiftscroll: shift+click to row $clicked selected '$(ipc selectedIndices)', not starting 5,6,7"
    shot "shiftscroll-list"
    # A text sentinel owns the clipboard first, so only the real Ctrl+C offer can answer text/uri-list below.
    printf 'shiftscroll-sentinel' | timeout 3 wl-copy || fail "shiftscroll: the clipboard sentinel never reached the clipboard"
    [[ "$(timeout 3 wl-paste -n -t text/plain 2>/dev/null)" == "shiftscroll-sentinel" ]] \
        || fail "shiftscroll: the clipboard does not hold the sentinel before the copy"
    key -M ctrl -k c -m ctrl >/dev/null
    settle
    local offer="" deadline=$((SECONDS + 5))
    while (( SECONDS < deadline )); do
        offer=$(timeout 3 wl-paste -t text/uri-list 2>/dev/null || true)
        if [[ -n "$offer" ]]; then break; fi
        sleep 0.05
    done
    [[ -n "$offer" ]] || fail "shiftscroll: no text/uri-list offer after copying $want_count rows"
    # Sample input: file:///dir/f005.txt lines with a CRLF terminator, as case_clipboard shows.
    local uris uri_lines uri_count want_last
    uris=$(tr -d '\r' <<< "$offer")
    mapfile -t uri_lines <<< "$uris"
    uri_count=${#uri_lines[@]}
    [[ "$uri_count" == "$want_count" ]] \
        || fail "shiftscroll: the clipboard holds $uri_count uris, not the $want_count selected rows"
    [[ "${uri_lines[0]}" == */f005.txt ]] \
        || fail "shiftscroll: the first uri is ${uri_lines[0]}, not f005.txt"
    printf -v want_last 'f%03d.txt' "$clicked"
    [[ "${uri_lines[$((uri_count - 1))]}" == */"$want_last" ]] \
        || fail "shiftscroll: the last uri is ${uri_lines[$((uri_count - 1))]}, not $want_last"
    printf 'SHIFTSCROLL copy uris=%s first=%s last=%s\n' "$uri_count" "${uri_lines[0]}" "${uri_lines[$((uri_count - 1))]}"
    # Keyboard twin: escape clears the marks, Home brings row 5 back on screen for the plain click.
    key -k Escape >/dev/null
    settle
    key -k Home >/dev/null
    settle
    click_row 5 left
    settle
    [[ "$(ipc selectedIndices)" == "5" ]] \
        || fail "shiftscroll: the second plain click selected '$(ipc selectedIndices)', not row 5 alone"
    shiftscroll_wheel
    local pulled_key=$(ipc cursor)
    (( pulled_key > 5 )) && [[ -n "$(ipc rowCentre "$pulled_key")" ]] \
        || fail "shiftscroll: the second wheel left the cursor on row $pulled_key, not pulled past row 5 onto the viewport"
    key J >/dev/null
    settle
    [[ "$(ipc cursor)" == "$((pulled_key + 1))" ]] \
        || fail "shiftscroll: shift+J moved the cursor to $(ipc cursor), not one row past $pulled_key"
    local after=$(( pulled_key + 1 - 5 + 1 ))
    [[ "$(ipc selectionCount)" == "$after" ]] \
        || fail "shiftscroll: shift+J selected $(ipc selectionCount) rows, not $after from row 5 to cursor $(ipc cursor)"
    [[ "$(ipc selectedIndices)" == 5,6,7* ]] \
        || fail "shiftscroll: shift+J selected '$(ipc selectedIndices)', not starting 5,6,7"
    printf 'SHIFTSCROLL keyboard cursor=%s count=%s\n' "$(ipc cursor)" "$(ipc selectionCount)"
}

case_overlays() {
    local dir="$fixture_root/overlays" mode n cx cy wx wy before after menu_row menus_checks=0
    views_fixture "$dir"
    launch "$dir"
    wait_listing 406
    for mode in grid list columns; do
        switch_view "$mode"
        key -k Home >/dev/null
        settle
        n=$(( $(ipc visibleRows) - 3 ))
        [[ -n "$(ipc rowRect "$n")" ]] || fail "$mode: row $n has no box, the reader answers nothing here"
        hover_row "$n"
        settle
        [[ "$(ipc rowHovered "$n")" == "true" ]] || fail "$mode: hover over row $n with nothing open did not lift it, so the checks below prove nothing"
        shot "overlays-$mode-hover"
        omarchy-drive scroll down 3 >/dev/null
        settle
        [[ "$(ipc viewContentY)" != "0" ]] || fail "$mode: three notches with nothing open left the view at 0, so the wheel checks below prove nothing"
        key -k Home >/dev/null
        settle
        [[ "$(ipc viewContentY)" == "0" ]] || fail "$mode: Home did not bring the view back to 0, it is $(ipc viewContentY)"
        menu_row=$(ipc visibleRowName 2)
        click_row 2 right
        settle
        shot "overlays-$mode-menu"
        local menu_cursor=2
        [[ "$mode" != columns ]] || menu_cursor=0
        # Columns keeps the cursor where it was, so only the menu's own snapshot shows it opened on row 2.
        [[ "$mode" != columns ]] || menus_expect menuState ".snapshotReady and .snapshotRowName == \"$menu_row\"" "columns: the menu acts on row 2, $menu_row"
        [[ "$(ipc contextMenuVisible)" == "true" && "$(ipc cursor)" == "$menu_cursor" ]] || fail "$mode: a right click on row 2 opened no menu with the expected cursor (visible $(ipc contextMenuVisible), cursor $(ipc cursor))"
        before=$(ipc viewContentY)
        hover_row "$n"
        settle
        [[ "$(ipc rowHovered "$n")" == "false" ]] || fail "$mode: row $n lifted under the open menu"
        omarchy-drive scroll down 3 >/dev/null
        settle
        after=$(ipc viewContentY)
        [[ "$after" == "$before" && "$(ipc contextMenuVisible)" == "true" ]] || fail "$mode: the wheel under the menu moved the view from $before to $after (menu $(ipc contextMenuVisible))"
        click_row_beside_menu "$n" left
        settle
        [[ "$(ipc contextMenuVisible)" == "false" && "$(ipc cursor)" == "$menu_cursor" ]] || fail "$mode: the click outside the menu left it $(ipc contextMenuVisible) and moved the cursor to $(ipc cursor)"
        key -k Home >/dev/null
        settle
        printf 'OVERLAYS_DIAG %s before view=%s focus=%s settings=%s menu=%s path=%s\n' \
            "$mode" "$(ipc viewMode)" "$(ipc keyDeliveryState)" "$(ipc settingsOpen)" "$(ipc contextMenuVisible)" "$(ipc path)"
        key , >/dev/null
        settle
        printf 'OVERLAYS_DIAG %s after settings=%s focus=%s\n' "$mode" "$(ipc settingsOpen)" "$(ipc keyDeliveryState)"
        [[ "$(ipc settingsOpen)" == "true" ]] || fail "$mode: the comma key did not open settings"
        before=$(ipc viewContentY)
        hover_row "$n"
        settle
        [[ "$(ipc rowHovered "$n")" == "false" ]] || fail "$mode: row $n lifted under the settings card"
        shot "overlays-$mode-settings"
        omarchy-drive scroll down 3 >/dev/null
        settle
        after=$(ipc viewContentY)
        [[ "$after" == "$before" && "$(ipc settingsOpen)" == "true" ]] || fail "$mode: the wheel under settings moved the view from $before to $after (settings $(ipc settingsOpen))"
        # Sample input: 240 120 620 500 (Settings card x, y, width, height).
        read -r cx cy _cw _ch <<< "$(ipc settingsCardRect)"
        # Sample input: 100 80 1100 700 (native window x, y, width, height).
        read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
        omarchy-drive click "$((wx + cx + 40))" "$((wy + cy + 40))" right >/dev/null
        settle
        [[ "$(ipc settingsOpen)" == "true" && "$(ipc contextMenuVisible)" == "false" && "$(ipc cursor)" == "0" ]] \
            || fail "$mode: a right click on the card body: settings $(ipc settingsOpen), menu $(ipc contextMenuVisible), cursor $(ipc cursor)"
        click_row_edge "$n" right
        settle
        [[ "$(ipc settingsOpen)" == "false" && "$(ipc contextMenuVisible)" == "false" && "$(ipc cursor)" == "0" ]] \
            || fail "$mode: a right click on the ground: settings $(ipc settingsOpen), menu $(ipc contextMenuVisible), cursor $(ipc cursor)"
        key , >/dev/null
        settle
        click_row_edge "$n" left
        settle
        [[ "$(ipc settingsOpen)" == "false" && "$(ipc cursor)" == "0" ]] || fail "$mode: a left click on the ground left settings $(ipc settingsOpen) and the cursor on $(ipc cursor)"
        printf 'OVERLAYS %s menu=ok settings=ok\n' "$mode"
    done
    kill_flea
}
