# GPUI overrides for UISH-20; seeded from the unchanged oracle at 2faf8d099.
case_tabs() {
    local dir="$fixture_root/tabs"
    sandbox_scratch "$dir"
    mkdir -p "$dir/alpha" "$dir/beta"
    : > "$dir/note.txt"
    launch "$dir"
    wait_listing 3
    [[ "$(ipc tabCount)" == "1" ]] || fail "tabs: started with $(ipc tabCount) tabs, not 1"
    [[ "$(ipc tabBarVisible)" == "false" ]] || fail "tabs: the bar showed with one tab"
    key t >/dev/null
    settle
    [[ "$(ipc tabCount)" == "2" ]] || fail "tabs: t did not open a second tab, count=$(ipc tabCount)"
    [[ "$(ipc tabBarVisible)" == "true" ]] || fail "tabs: the bar stayed hidden after t"
    [[ "$(ipc tabIndex)" == "1" ]] || fail "tabs: t did not land on the new tab, index=$(ipc tabIndex)"
    seek_row_named "alpha" || fail "tabs: could not find alpha"
    key -k Return >/dev/null
    wait_path "$dir/alpha"
    # The bar is how a tab is chosen here: ui/js/Keymap.js answers 1 to 9 with a tab in the TUI
    # alone, and lookupFor leaves a bare digit unbound in this frontend on purpose.
    click_tab 0
    wait_path "$dir"
    [[ "$(ipc tabIndex)" == "0" ]] || fail "tabs: clicking tab 0 did not return to it, index=$(ipc tabIndex)"
    click_tab 1
    wait_path "$dir/alpha"
    [[ "$(ipc tabIndex)" == "1" ]] || fail "tabs: clicking tab 1 did not return to it, index=$(ipc tabIndex)"
    key w >/dev/null
    wait_path "$dir"
    [[ "$(ipc tabCount)" == "1" ]] || fail "tabs: w did not close the current tab, count=$(ipc tabCount)"
    [[ "$(ipc tabBarVisible)" == "false" ]] || fail "tabs: the bar stayed up after the last extra tab closed"
    key w >/dev/null
    wait_message "The last tab cannot be closed"
    # A remembered strip is whole: the single tabs dir with its index, never a refused half patch.
    local tabs_doc=""
    for _attempt in $(seq 1 40); do
        tabs_doc=$(env XDG_STATE_HOME="$suite_state" "$flea_bin" --ui-state 2>/dev/null || true)
        [[ -n "$tabs_doc" ]] || { sleep 0.25; continue; }
        if printf '%s' "$tabs_doc" | jq -e --arg dir "$dir" '.lastPath == $dir and .lastTabs == {paths: [$dir], index: 0}' >/dev/null; then
            break
        fi
        sleep 0.25
    done
    printf '%s' "$tabs_doc" | jq -e --arg dir "$dir" '.lastPath == $dir and .lastTabs == {paths: [$dir], index: 0}' >/dev/null \
        || fail "tabs: stored strip is $(printf '%s' "$tabs_doc" | jq -c '{lastPath, lastTabs}' 2>/dev/null || printf 'unreadable'), not the single tabs dir"
    shot tabs-one
    key t >/dev/null
    settle
    local centre cx cy wx wy
    centre=$(ipc tabCentre 0)
    [[ -n "$centre" ]] || fail "tabs: tab 0 has no centre"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null
    settle
    [[ "$(ipc tabIndex)" == "0" ]] || fail "tabs: clicking tab 0 did not select it, index=$(ipc tabIndex)"
    # A count verdict stands until the count moves; the bar clears it after 4000 ms alone.
    key w >/dev/null
    settle
    [[ "$(ipc tabCount)" == "1" ]] || fail "tabs: w did not close to one tab, count=$(ipc tabCount)"
    # The clock starts before the press that raises the notice, so a read inside StatusBar.qml messageMs precedes its expiry.
    local notice_seen_ns withdraw_within_ms=4000
    notice_seen_ns=$(date +%s%N)
    key w >/dev/null
    wait_message "The last tab cannot be closed"
    key t >/dev/null
    settle
    [[ "$(ipc tabCount)" == "2" ]] || fail "tabs: t did not reopen a second tab, count=$(ipc tabCount)"
    [[ -z "$(ipc lastMessage)" ]] || fail "tabs: a tab-count notice outlived the count change"
    (( $(date +%s%N) - notice_seen_ns < withdraw_within_ms * 1000000 )) || fail "tabs: the empty read landed past $withdraw_within_ms ms, inside the bar's own expiry"
    shot tabs-two
    printf 'TABS count=%s index=%s labels=%s\n' "$(ipc tabCount)" "$(ipc tabIndex)" "$(ipc tabLabels)"
    kill_flea
}

case_tabdrag() {
    local dir="$fixture_root/tabdrag"
    sandbox_scratch "$dir"
    mkdir -p "$dir/alpha" "$dir/beta" "$dir/gamma"
    launch "$dir"
    wait_listing 3
    key t >/dev/null
    settle
    key t >/dev/null
    settle
    [[ "$(ipc tabCount)" == "3" ]] || fail "tabdrag: t twice did not make 3 tabs, count=$(ipc tabCount)"
    click_tab 0
    seek_row_named "alpha" || fail "tabdrag: could not find alpha"
    key -k Return >/dev/null
    wait_path "$dir/alpha"
    click_tab 1
    seek_row_named "beta" || fail "tabdrag: could not find beta"
    key -k Return >/dev/null
    wait_path "$dir/beta"
    click_tab 2
    seek_row_named "gamma" || fail "tabdrag: could not find gamma"
    key -k Return >/dev/null
    wait_path "$dir/gamma"
    [[ "$(ipc tabLabels)" == "alpha|beta|gamma" ]] || fail "tabdrag: tabs label [$(ipc tabLabels)], not alpha|beta|gamma"
    local c0x c0y c1x c1y c2x c2y w
    read -r c0x c0y <<< "$(ipc tabCentre 0)"
    read -r c1x c1y <<< "$(ipc tabCentre 1)"
    read -r c2x c2y <<< "$(ipc tabCentre 2)"
    w=$((c1x - c0x))
    (( w > 0 )) || fail "tabdrag: tab centres do not step right [$c0x,$c1x,$c2x]"
    # Tab 1 past tab 2's far edge: insertion point 3 of 3.
    tabdrag_to "$c1x" "$c1y" "$((c2x + w / 2 + 3))" "$c2y"
    [[ "$(ipc tabLabels)" == "alpha|gamma|beta" ]] || fail "tabdrag: drag labelled [$(ipc tabLabels)], not alpha|gamma|beta"
    # Tabs040 "Dragged tab": a drag reorders and never selects, so gamma stays current at its new place.
    [[ "$(ipc tabIndex)" == "1" ]] || fail "tabdrag: the current tab did not stay current, index=$(ipc tabIndex)"
    click_tab 2
    [[ "$(ipc tabIndex)" == "2" ]] || fail "tabdrag: clicking the moved tab did not select it, index=$(ipc tabIndex)"
    # The same tab back before tab 0: insertion point 0 of 3.
    read -r c0x c0y <<< "$(ipc tabCentre 0)"
    read -r c2x c2y <<< "$(ipc tabCentre 2)"
    w=$(( (c2x - c0x) / 2 ))
    tabdrag_to "$c2x" "$c2y" "$((c0x - w / 2 - 3))" "$c0y" tabdrag-held-back
    [[ "$(ipc tabLabels)" == "beta|alpha|gamma" ]] || fail "tabdrag: drag back labelled [$(ipc tabLabels)], not beta|alpha|gamma"
    [[ "$(ipc tabIndex)" == "0" ]] || fail "tabdrag: the dragged tab is not current, index=$(ipc tabIndex)"
    key "}" >/dev/null
    settle
    [[ "$(ipc tabLabels)" == "alpha|beta|gamma" ]] || fail "tabdrag: } labelled [$(ipc tabLabels)], not alpha|beta|gamma"
    [[ "$(ipc tabIndex)" == "1" ]] || fail "tabdrag: } left index=$(ipc tabIndex), not 1"
    key "{" >/dev/null
    settle
    [[ "$(ipc tabLabels)" == "beta|alpha|gamma" ]] || fail "tabdrag: { labelled [$(ipc tabLabels)], not beta|alpha|gamma"
    [[ "$(ipc tabIndex)" == "0" ]] || fail "tabdrag: { left index=$(ipc tabIndex), not 0"
    # A click still selects after a drag, so the DragHandler did not eat the TapHandler.
    click_tab 2
    [[ "$(ipc tabIndex)" == "2" ]] || fail "tabdrag: clicking tab 2 after a drag did not select it"
    printf 'TABDRAG labels=%s index=%s\n' "$(ipc tabLabels)" "$(ipc tabIndex)"
    shot tabdrag-dropped
    kill_flea
}

case_middleclick() {
    local dir="$fixture_root/middleclick"
    sandbox_scratch "$dir"
    mkdir -p "$dir/alpha" "$dir/fav"
    : > "$dir/note.txt"
    seed_ui_state "$fixture_root/middleclick-state" "$(printf '{"places":{"favourites":[{"label":"Fav","path":"%s/fav"}]}}' "$dir")"
    launch "$dir"
    # Measured row order: alpha, fav, note.txt.
    wait_listing 3
    [[ "$(ipc tabCount)" == "1" ]] || fail "middleclick: started with $(ipc tabCount) tabs, not 1"

    echo "-- a file opens no tab --"
    click_row 2 middle
    settle
    [[ "$(ipc tabCount)" == "1" ]] || fail "middleclick: a middle click on a file opened a tab"
    [[ "$(ipc path)" == "$dir" ]] || fail "middleclick: a middle click on a file left for $(ipc path)"

    echo "-- a directory opens in a new tab, in each view --"
    local count=1 view
    for view in list grid columns; do
        [[ "$view" == list ]] || click_chrome "$view"
        settle
        [[ "$(ipc viewMode)" == "$view" ]] || fail "middleclick: the chrome did not switch to $view"
        click_row 0 middle
        count=$((count + 1))
        wait_tabs "$count" "a middle click on alpha in $view"
        wait_path "$dir/alpha"
        printf 'MIDDLECLICK %s tabs=%s labels=%s\n' "$view" "$(ipc tabCount)" "$(ipc tabLabels)"
        click_tab 0
        wait_path "$dir"
        # rowAt reads the list view's own delegate, which a tab switch back into grid or columns leaves
        # unbuilt; the row the shown view draws is the one to wait on.
        for _attempt in $(seq 1 300); do
            [[ "$(ipc total)" == 3 && "$(ipc visibleRowName 0)" == alpha && "$(ipc listInFlight)" == false ]] && break
            sleep 0.05
        done
        [[ "$(ipc visibleRowName 0)" == alpha ]] || fail "middleclick: $view never drew alpha at row 0 after the tab switch"
    done
    shot middleclick-views

    echo "-- a Favorites row opens in a new tab, and the Trash row opens none --"
    local trash_index fav_index
    trash_index=$(ipc railEntries | jq -r 'map(.label) | index("Trash")')
    [[ -n "$trash_index" && "$trash_index" != "null" ]] || fail "middleclick: the rail has no Trash row"
    click_rail_row "$trash_index" middle
    settle
    [[ "$(ipc tabCount)" == "$count" ]] || fail "middleclick: a middle click on the Trash opened a tab"
    fav_index=$(ipc railEntries | jq -r 'map(.label) | index("Fav")')
    [[ -n "$fav_index" && "$fav_index" != "null" ]] \
        || fail "middleclick: the seeded favourite is not on the rail, which carries $(ipc railEntries)"
    click_rail_row "$fav_index" middle
    wait_tabs "$((count + 1))" "a middle click on the Fav row"
    wait_path "$dir/fav"
    shot middleclick-rail
    kill_flea
}

case_opentab() {
    local dir="$fixture_root/opentab"
    sandbox_scratch "$dir"
    mkdir -p "$dir/alpha"
    : > "$dir/note.txt"
    launch "$dir"
    wait_listing 2
    [[ "$(ipc tabCount)" == "1" ]] || fail "opentab: started with $(ipc tabCount) tabs, not 1"

    echo "-- a file opens no tab --"
    seek_row_named "note.txt" || fail "opentab: could not find note.txt"
    key -M ctrl -k Return -m ctrl >/dev/null || fail "opentab: key Ctrl+Return was rejected"
    wait_message "Only a folder opens in a new tab."
    [[ "$(ipc tabCount)" == "1" ]] || fail "opentab: Ctrl+Return on a file opened a tab"
    [[ "$(ipc path)" == "$dir" ]] || fail "opentab: Ctrl+Return on a file left for $(ipc path)"

    echo "-- a directory opens in a new tab, in each view --"
    local count=1 view chord
    for view in list grid columns; do
        [[ "$view" == list ]] || click_chrome "$view"
        settle
        [[ "$(ipc viewMode)" == "$view" ]] || fail "opentab: the chrome did not switch to $view"
        seek_row_named "alpha" || fail "opentab: could not find alpha in $view"
        local first_cursor
        first_cursor=$(ipc cursor)
        if [[ "$view" == grid ]]; then chord="KP_Enter"; else chord="Return"; fi
        key -M ctrl -k "$chord" -m ctrl >/dev/null || fail "opentab: key Ctrl+$chord was rejected"
        count=$((count + 1))
        wait_tabs "$count" "Ctrl+$chord on alpha in $view"
        wait_path "$dir/alpha"
        printf 'OPENTAB %s tabs=%s labels=%s\n' "$view" "$(ipc tabCount)" "$(ipc tabLabels)"
        click_tab 0
        wait_path "$dir"
        [[ "$(ipc cursor)" == "$first_cursor" ]] || fail "opentab: $view left the first tab's cursor at $(ipc cursor), not $first_cursor"
        for _attempt in $(seq 1 300); do
            [[ "$(ipc total)" == 2 && "$(ipc visibleRowName 0)" == alpha && "$(ipc listInFlight)" == false ]] && break
            sleep 0.05
        done
        [[ "$(ipc visibleRowName 0)" == alpha ]] || fail "opentab: $view never drew alpha at row 0 after the tab switch"
    done
    shot opentab-views
    kill_flea
}

case_folderviews() {
    local root="$fixture_root/folderviews" state="$fixture_root/folderviews-state" off_state="$fixture_root/folderviews-off-state" d
    local stored="$state/flea/ui.json"
    sandbox_scratch "$root"
    for d in a b c; do
        mkdir -p "$root/$d"
        : > "$root/$d/one.txt"
        : > "$root/$d/two.txt"
    done
    seed_ui_state "$state" '{"view":"list","rememberViewPerFolder":true}'
    launch "$root"; wait_listing 3
    folderviews_is list "the root, with no view of its own"
    folderviews_open "$root/a"
    switch_view grid
    key H >/dev/null; foldersort_wait_path "$root"; wait_listing 3
    folderviews_is list "the root after Back"
    key -k Down >/dev/null
    folderviews_open "$root/b"
    click_chrome columns
    folderviews_is columns "B after its chrome button"
    key H >/dev/null; foldersort_wait_path "$root"; wait_listing 3
    key -k Up >/dev/null
    folderviews_open "$root/a"
    folderviews_is grid "A reopened by its row"
    key H >/dev/null; foldersort_wait_path "$root"; wait_listing 3
    key L >/dev/null; foldersort_wait_path "$root/a"; wait_listing 2
    folderviews_is grid "A reached by Forward"
    key H >/dev/null; foldersort_wait_path "$root"; wait_listing 3
    key -k Down >/dev/null
    folderviews_open "$root/b"
    folderviews_is columns "B reopened by its row"
    key t >/dev/null
    settle
    [[ "$(ipc tabCount)" == 2 ]] || fail "folderviews: t opened $(ipc tabCount) tabs, not 2"
    folderviews_is columns "the new tab opened on B"
    key H >/dev/null; foldersort_wait_path "$root"; wait_listing 3
    key -k Up >/dev/null
    folderviews_open "$root/a"
    folderviews_is grid "A in the second tab"
    key -M ctrl -k Page_Down -m ctrl >/dev/null
    foldersort_wait_path "$root/b"
    folderviews_is columns "the first tab, back on B"
    key -M ctrl -k Page_Down -m ctrl >/dev/null
    foldersort_wait_path "$root/a"
    folderviews_is grid "the second tab, back on A"
    click_chrome dual
    settle
    ipc dualState | jq -e '.active' >/dev/null || fail "folderviews: the dual button did not turn Dual on"
    folderviews_is list "the primary pane in Dual"
    click_chrome list
    settle
    ipc dualState | jq -e '.active | not' >/dev/null || fail "folderviews: the list button did not leave Dual"
    folderviews_is list "the primary pane back from Dual, on the view its button chose"
    # A holds list on disk before Grid is chosen again, so the next read can only match the last save.
    folderviews_stored '.folderViews[$a] == "list"' "the list button"
    switch_view grid
    folderviews_is grid "A, with its Grid chosen again"
    folderviews_stored '.folderViews[$a] == "grid" and .folderViews[$b] == "columns" and (.folderViews | has($c) | not)' "Grid chosen again"
    launch "$root/a"; wait_listing 2
    folderviews_is grid "A after a relaunch"
    launch "$root/b"; wait_listing 2
    folderviews_is columns "B after a relaunch"
    launch "$root/c"; wait_listing 2
    folderviews_is list "C, which holds no view, after a relaunch"
    kill_flea

    seed_ui_state "$off_state" '{"view":"list"}'
    launch "$root/a"; wait_listing 2
    switch_view grid
    launch "$root/b"; wait_listing 2
    folderviews_is grid "B, off, in the last-used view"
    kill_flea
    jq -e '(.folderViews // {}) | length == 0' "$off_state/flea/ui.json" >/dev/null || fail "folderviews: off, the state file holds $(jq -c .folderViews "$off_state/flea/ui.json")"
    printf 'FOLDERVIEWS ok\n'
}
