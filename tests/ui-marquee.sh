#!/usr/bin/env bash
# Sourced by ui.sh; pointer drags use the real relative-uinput path characterized by tests/drag.sh.
# shellcheck disable=SC2034,SC2154 # ui.sh and the case supply shared ownership and evidence state.

marquee_guard() {
    local target="$1" canonical
    [[ -n "$target" && "$target" == /* && -f "$marquee_box/.flea-test-sandbox" ]] || fail "marquee: invalid sandbox target"
    canonical=$(realpath -m -- "$target") || fail "marquee: target resolution failed"
    [[ "$canonical" == "$marquee_box/"* && "$canonical" != "$marquee_box" ]] || fail "marquee: target escaped own sandbox"
}

marquee_expect() {
    local observer="$1" expected="$2" label="$3" seen deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        seen=$(ipc "$observer") || fail "marquee: observer failed: $observer"
        if [[ "$seen" == "$expected" ]]; then
            marquee_checks=$((marquee_checks + 1))
            printf 'MARQUEE_CHECK %s %s observed=%q\n' "$marquee_checks" "$label" "$seen"
            return
        fi
        sleep 0.05
    done
    fail "marquee: $label: expected [$expected], observed [$seen]"
}

marquee_state() {
    local expression="$1" label="$2" observer="${3:-selectionBandState}" seen deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        seen=$(ipc "$observer") || fail "marquee: observation failed: $observer"
        if jq -e "$expression" <<< "$seen" >/dev/null; then
            marquee_checks=$((marquee_checks + 1))
            printf 'MARQUEE_CHECK %s %s state=%s\n' "$marquee_checks" "$label" "$seen"
            return
        fi
        sleep 0.05
    done
    fail "marquee: $label: $seen"
}

marquee_glide() {
    local tx="$1" ty="$2" tolerance="${3:-4}" cx cy dx dy move_x move_y step
    [[ "$tx $ty" =~ ^-?[0-9]+\ -?[0-9]+$ ]] || fail "marquee: invalid target coordinates"
    assert_focus
    # libinput accelerates relative motion; re-read the actual position after every step.
    for ((step = 0; step < 16; step++)); do
        read -r cx cy <<< "$(hyprctl cursorpos | tr -d ',')"
        [[ "$cx $cy" =~ ^-?[0-9]+\ -?[0-9]+$ ]] || fail "marquee: actual pointer coordinates unavailable"
        dx=$((tx - cx)); dy=$((ty - cy))
        if (( ${dx#-} <= tolerance && ${dy#-} <= tolerance )); then return; fi
        move_x=$((dx / 2)); move_y=$((dy / 2))
        if (( dx != 0 && move_x == 0 )); then move_x=$((dx > 0 ? 1 : -1)); fi
        if (( dy != 0 && move_y == 0 )); then move_y=$((dy > 0 ? 1 : -1)); fi
        ydotool mousemove -x "$move_x" -y "$move_y" >/dev/null 2>&1 \
            || fail "marquee: relative pointer motion failed"
        sleep 0.05
    done
    fail "marquee: pointer did not reach $tx,$ty; observed $cx,$cy"
}

marquee_release() {
    if [[ "$marquee_button_down" == true ]]; then
        ydotool click 0x80 >/dev/null 2>&1 || fail "marquee: pointer release failed"
        marquee_button_down=false
    fi
    if [[ "$marquee_ctrl_down" == true ]]; then
        ydotool key 29:0 >/dev/null 2>&1 || fail "marquee: Ctrl release failed"
        marquee_ctrl_down=false
    fi
}

marquee_cleanup() {
    local result="$1"
    trap - EXIT HUP INT TERM
    (marquee_release) || result=1
    trash_cleanup "$result"
}

marquee_press() {
    local cx="$1" cy="$2" ctrl="${3:-false}" wx wy ww wh
    read -r wx wy ww wh < <(window_box) || fail "marquee: owned window is unavailable"
    (( cx >= 0 && cy >= 0 && cx < ww && cy < wh )) || fail "marquee: press is outside the owned client"
    marquee_glide "$((wx + cx))" "$((wy + cy))"
    if [[ "$ctrl" == true ]]; then
        ydotool key 29:1 >/dev/null 2>&1 || fail "marquee: Ctrl press failed"
        marquee_ctrl_down=true
    fi
    ydotool click 0x40 >/dev/null 2>&1 || fail "marquee: pointer press failed"
    marquee_button_down=true
}

marquee_to() {
    local wx wy ww wh
    read -r wx wy ww wh < <(window_box) || fail "marquee: owned window moved out of scope"
    marquee_glide "$((wx + $1))" "$((wy + $2))"
}

marquee_begin_below() {
    local last="$1" ctrl="${2:-false}" last_only="${3:-false}" ax ay aw ah rx ry rw rh cx cy
    local wx wy ww wh pointer_x pointer_y
    read -r ax ay aw ah <<< "$(ipc listAreaRect)"
    read -r rx ry rw rh <<< "$(ipc rowRect "$last")"
    [[ "$ax $ay $aw $ah $rx $ry $rw $rh" =~ ^[0-9]+(\ [0-9]+){7}$ ]] || fail "marquee: listing/row geometry unavailable"
    if [[ "$(ipc viewMode)" == columns ]]; then ax=$rx; aw=$rw; fi
    (( rh > 0 && ry + rh < ay + ah )) || fail "marquee: no empty space below the last row"
    cx=$((ax + aw - 12)); cy=$(((ry + rh + ay + ah) / 2))
    [[ "$last_only" == true ]] && cx=$((rx + rw * 3 / 4))
    read -r wx wy ww wh < <(window_box) || fail "marquee: owned window is unavailable"
    marquee_glide "$((wx + cx))" "$((wy + cy))" 1
    read -r pointer_x pointer_y <<< "$(hyprctl cursorpos | tr -d ',')"
    [[ "$pointer_x $pointer_y" =~ ^-?[0-9]+\ -?[0-9]+$ ]] || fail "marquee: final press position is unavailable"
    (( pointer_x > wx + ax && pointer_x < wx + ax + aw && pointer_y > wy + ry + rh && pointer_y < wy + ay + ah )) \
        || fail "marquee: actual pointer missed the measured empty tail; pointer=$pointer_x,$pointer_y row_bottom=$((wy + ry + rh)) view_bottom=$((wy + ay + ah))"
    marquee_press "$cx" "$cy" "$ctrl"
    marquee_state '.tracking and (.active | not)' "empty-space press owns a pending band"
}

marquee_four() {
    local label="$1" cx cy
    marquee_begin_below 3
    read -r cx cy <<< "$(ipc rowCentre 0)"
    marquee_to "$cx" "$cy"
    marquee_state '.active and .tracking' "$label has a live rubber band"
    marquee_expect selectedIndices '0,1,2,3' "$label marks four intersections before release"
    local footer
    footer=$(ipc statusFooterState) || fail "marquee: footer observation failed"
    jq -e '.selected == 4 and (.counts | contains("4 selected"))' <<< "$footer" >/dev/null \
        || fail "marquee: four live marks do not reach the footer: $footer"
    shot "marquee-$label-four-held"
    printf 'MARQUEE_SHOT_REQUIRES_INSPECTION %s\n' "$label-four-held"
}

marquee_interactions() {
    local label="$1" cx cy before
    click_row 0 left
    marquee_expect selectedIndices 0 "$label plain click marks one row"
    click_row 2 left --mods ctrl
    marquee_expect selectedIndices '0,2' "$label Ctrl-click preserves another mark"
    click_row 2 left --mods ctrl
    marquee_expect selectedIndices 0 "$label Ctrl-click toggles its mark off"
    click_row 3 left --mods shift
    marquee_expect selectedIndices '2,3' "$label Shift-click extends from cursor anchor"
    click_row 1 left
    marquee_four "$label"
    key -k Escape >/dev/null || fail "marquee: Escape delivery failed"
    marquee_expect selectedIndices 1 "$label Escape restores prior marks while pressed"
    marquee_expect cursor 1 "$label Escape restores prior cursor"
    marquee_state '(.tracking | not) and (.active | not)' "$label Escape releases band ownership"
    marquee_release
    marquee_expect selectedIndices 1 "$label physical release does not recommit a cancelled band"
    marquee_four "$label-repeat"
    marquee_release
    marquee_expect selectedIndices '0,1,2,3' "$label release retains four marks"
    marquee_expect cursor 0 "$label upward release chooses last entering row"
    click_row 0 left
    marquee_begin_below 3 true true
    read -r cx cy <<< "$(ipc rowCentre 3)"
    marquee_to "$cx" "$cy"
    marquee_expect selectedIndices '0,3' "$label Ctrl at press adds the band to existing marks"
    key -k Escape >/dev/null || fail "marquee: Ctrl-band Escape delivery failed"
    marquee_expect selectedIndices 0 "$label Escape cancels while Ctrl remains held"
    marquee_release
    marquee_begin_below 3 true true
    marquee_to "$cx" "$cy"
    marquee_release
    marquee_expect selectedIndices '0,3' "$label Ctrl band retains its union on release"
    printf 'MARQUEE_INTERACTIONS %s complete\n' "$label"
}

marquee_grid_zoom() {
    local before after x y width height before_height cx cy deadline
    settings_wait_value '.preview.thumbSize == "medium" and .preview.ctrlZoom == true'
    click_row 1 left
    marquee_expect selectedIndices 1 "Grid zoom rollback starts with one mark"
    marquee_expect cursor 1 "Grid zoom rollback starts with an identified cursor"
    before=$(ipc rowRect 0) || fail "marquee: Grid geometry before zoom is unavailable"
    [[ "$before" =~ ^[0-9]+(\ [0-9]+){3}$ ]] || fail "marquee: invalid Grid rectangle before zoom: $before"
    read -r x y width before_height <<< "$before"
    (( width > 0 && before_height > 0 )) || fail "marquee: Grid tile has no geometry before zoom"
    marquee_four default-grid-zoom
    ydotool key 29:1 >/dev/null 2>&1 || fail "marquee: zoom Ctrl press failed"
    marquee_ctrl_down=true
    omarchy-drive scroll up 1 >/dev/null || fail "marquee: native Ctrl-wheel zoom failed"
    settings_wait_value '.preview.thumbSize == "large"'
    deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        after=$(ipc rowRect 0) || fail "marquee: Grid geometry after zoom is unavailable"
        [[ "$after" =~ ^[0-9]+(\ [0-9]+){3}$ ]] || fail "marquee: invalid Grid rectangle after zoom: $after"
        read -r x y width height <<< "$after"
        (( width > 0 && height > before_height )) && break
        sleep 0.05
    done
    (( width > 0 && height > before_height )) || fail "marquee: Ctrl-wheel changed no actual tile geometry: $before -> $after"
    marquee_state '(.tracking | not) and (.active | not)' "Grid geometry change cancels the active band"
    marquee_expect selectedIndices 1 "Grid zoom restores the pre-band marks while pressed"
    marquee_expect cursor 1 "Grid zoom restores the pre-band cursor while pressed"
    shot marquee-grid-zoom-cancelled-held
    marquee_release
    marquee_expect selectedIndices 1 "physical release cannot recommit a zoom-cancelled band"
    marquee_expect cursor 1 "physical release preserves the zoom-restored cursor"
    read -r cx cy <<< "$(ipc rowCentre 0)"
    marquee_to "$cx" "$cy"
    ydotool key 29:1 >/dev/null 2>&1 || fail "marquee: zoom restoration Ctrl press failed"
    marquee_ctrl_down=true
    omarchy-drive scroll down 1 >/dev/null || fail "marquee: native zoom restoration failed"
    settings_wait_value '.preview.thumbSize == "medium"'
    marquee_release
    cardsize_expect rowRect "$before" 0
    marquee_expect selectedIndices 1 "restoring zoom preserves the rolled-back marks"
    marquee_expect cursor 1 "restoring zoom preserves the rolled-back cursor"
    printf 'MARQUEE_GRID_ZOOM before=%q enlarged=%q restored=%q\n' "$before" "$after" "$(ipc rowRect 0)"
}

marquee_filter() {
    local query="$1"
    key / >/dev/null || fail "marquee: filter entry delivery failed"
    marquee_state '.filterTyping and .filterQuery == ""' "filter entry is ready" keyDeliveryState
    key "$query" >/dev/null || fail "marquee: filter text delivery failed"
    marquee_state ".filterTyping and .filterQuery == \"$query\"" "filter receives exact text" keyDeliveryState
    key -k Return >/dev/null || fail "marquee: filter commit delivery failed"
    marquee_state "(.filterTyping | not) and .filterQuery == \"$query\"" "filter retains committed text" keyDeliveryState
}

marquee_scroll() {
    local dir="$marquee_box/scroll" i ax ay aw ah cx cy before after
    mkdir "$dir" || fail "marquee: scroll fixture creation failed"
    for i in $(seq -w 0 39); do printf 'scroll %s\n' "$i" > "$dir/file-$i.txt" || fail "marquee: scroll file creation failed"; done
    printf 'filtered out\n' > "$dir/other.txt" || fail "marquee: excluded file creation failed"
    seed_ui_state "$marquee_box/scroll-state" '{"keys":"default","view":"list","preview":{"thumbnails":"off"}}'
    HOME="$marquee_home" launch "$dir"
    wait_listing 41
    permissions_viewport 880 620
    marquee_filter file
    marquee_expect drawnCount 40 "scroll fixture filter retains forty rows"
    key -k End >/dev/null || fail "marquee: End delivery failed"
    read -r cx cy <<< "$(ipc rowCentre 39)"
    marquee_to "$cx" "$cy"
    omarchy-drive scroll down 1 >/dev/null || fail "marquee: footer could not be brought into view"
    marquee_begin_below 39
    read -r ax ay aw ah <<< "$(ipc listAreaRect)"
    marquee_to "$((ax + aw / 4))" "$((ay - 12))"
    marquee_state '.active and .contentY > 0' "top-edge drag starts from the bottom of a long filtered listing"
    before=$(ipc selectionBandState) || fail "marquee: initial autoscroll state unavailable"
    marquee_state ".active and .contentY < $(jq -r .contentY <<< "$before") and .anchor.y == $(jq -r .anchor.y <<< "$before")" \
        "top-edge autoscroll moves content without moving the band origin"
    shot marquee-autoscroll-up-held
    after=$(ipc selectionBandState) || fail "marquee: scrolled state unavailable"
    marquee_to "$((ax + aw / 4))" "$((ay + ah + 12))"
    marquee_state ".active and .contentY > $(jq -r .contentY <<< "$after") and .anchor.y == $(jq -r .anchor.y <<< "$before")" \
        "bottom-edge autoscroll reverses while retaining the band origin"
    key -k Escape >/dev/null || fail "marquee: scrolling-band Escape delivery failed"
    marquee_release
    marquee_expect selectedIndices '' "Escape restores the empty pre-scroll selection"
    marquee_expect cursor 39 "Escape restores the pre-scroll cursor"
    kill_flea
}

marquee_columns_boundary() {
    local dir="$marquee_box/columns-boundary" window_size total last last_name i name state original_held cross_before
    local ax ay aw ah rx ry rw rh cx cy band rate remaining deadline selected first
    marquee_guard "$dir"
    mkdir "$dir" || fail "marquee: Columns boundary fixture creation failed"
    printf 'column 0\n' > "$dir/file-000000.txt" || fail "marquee: Columns initial fixture failed"
    seed_ui_state "$marquee_box/columns-boundary-state" '{"keys":"default","view":"columns","preview":{"thumbnails":"off"}}'
    HOME="$marquee_home" launch "$dir"
    wait_listing 1
    permissions_viewport 880 620
    marquee_expect viewMode columns "boundary fixture enters native Columns"
    state=$(ipc listingWindowState) || fail "marquee: initial held-window observation failed"
    window_size=$(jq -er '.windowSize | select(. > 0)' <<< "$state") || fail "marquee: invalid held-window budget"
    total=$((window_size * 2 + 1)); last=$((total - 1))
    kill_flea
    for ((i = 1; i < total; i++)); do
        printf -v name 'file-%06d.txt' "$i"
        printf 'column %s\n' "$i" > "$dir/$name" || fail "marquee: Columns boundary file creation failed"
    done
    printf -v last_name 'file-%06d.txt' "$last"
    HOME="$marquee_home" launch "$dir"
    wait_listing "$total"
    permissions_viewport 880 620
    marquee_state '.total > .windowSize * 2' "Columns fixture exceeds two held windows" listingWindowState
    key -k End >/dev/null || fail "marquee: Columns End delivery failed"
    marquee_expect cursor "$last" "Columns End reaches the absolute final row"
    cardsize_expect visibleRowName "$last_name" "$last"
    marquee_state '.held > 0 and .loaded > 0 and .held + .loaded == .total and .loaded <= .windowSize' \
        "Columns End refills a bounded final held window" listingWindowState
    read -r cx cy <<< "$(ipc rowCentre "$last")"
    marquee_to "$cx" "$cy"
    omarchy-drive scroll down 1 >/dev/null || fail "marquee: Columns tail scroll failed"
    state=$(ipc listingWindowState) || fail "marquee: final held-window observation failed"
    original_held=$(jq -er .held <<< "$state") || fail "marquee: final held offset unavailable"
    read -r ax ay aw ah <<< "$(ipc listAreaRect)"
    read -r rx ry rw rh <<< "$(ipc rowRect "$last")"
    [[ "$ax $ay $aw $ah $rx $ry $rw $rh" =~ ^[0-9]+(\ [0-9]+){7}$ ]] || fail "marquee: Columns boundary geometry unavailable"
    (( rh > 0 )) || fail "marquee: Columns row height is zero"
    cross_before=$((original_held - (ah + rh - 1) / rh))
    (( cross_before >= 0 )) || fail "marquee: fixture cannot cross a complete old held boundary"
    marquee_begin_below "$last"
    marquee_to "$((rx + rw / 4))" "$((ay - rh / 2))"
    marquee_state '.active and .scrollRate > 0' "Columns band begins upward auto-scroll from its measured tail"
    band=$(ipc selectionBandState) || fail "marquee: Columns band observation failed"
    rate=$(jq -er '.scrollRate | select(. > 0)' <<< "$band") || fail "marquee: Columns auto-scroll rate unavailable"
    # Travel time comes from actual row geometry and the band's reported rate; retain the usual check window after it.
    remaining=$(jq -nr --argjson position "$(jq -r .contentY <<< "$band")" --argjson row "$cross_before" \
        --argjson height "$rh" --argjson rate "$rate" '([0, ($position - $row * $height) / $rate] | max | ceil) + 15') \
        || fail "marquee: Columns boundary travel time could not be derived"
    deadline=$((SECONDS + remaining))
    while (( SECONDS < deadline )); do
        state=$(ipc listingWindowState) || fail "marquee: Columns refill observation failed"
        jq -e '.held >= 0 and .loaded > 0 and .loaded <= .windowSize and .held + .loaded <= .total' <<< "$state" >/dev/null \
            || fail "marquee: Columns exceeded its held-window budget: $state"
        band=$(ipc selectionBandState) || fail "marquee: Columns active-band observation failed"
        jq -e '.active and .tracking' <<< "$band" >/dev/null || fail "marquee: a Columns refill cancelled the live band: $band"
        selected=$(ipc selectedIndices) || fail "marquee: Columns mark observation failed"
        first=${selected%%,*}
        if [[ "$first" =~ ^[0-9]+$ ]] && (( first <= cross_before )) \
            && jq -e --argjson old "$original_held" '.held < $old and .held + .loaded < .total' <<< "$state" >/dev/null; then break; fi
        sleep 0.05
    done
    if [[ ! "$first" =~ ^[0-9]+$ ]] || (( first > cross_before )) \
        || ! jq -e --argjson old "$original_held" '.held < $old and .held + .loaded < .total' <<< "$state" >/dev/null; then
        fail "marquee: Columns band did not cross its old held boundary: window=$state first=$first target=$cross_before"
    fi
    marquee_expect cursor "$last" "Columns auto-scroll leaves the cursor fixed until release"
    shot marquee-columns-boundary-held
    marquee_release
    selected=$(ipc selectedIndices) || fail "marquee: retained Columns marks unavailable"
    first=${selected%%,*}
    jq -ne --arg indices "$selected" --argjson old "$original_held" --argjson total "$total" \
        '($indices | split(",") | map(tonumber)) as $rows | ($rows | length) > 0 and $rows[0] < $old and $rows == [range($rows[0]; $total)]' >/dev/null \
        || fail "marquee: held-window replacement changed absolute marks: $selected"
    marquee_expect cursor "$first" "Columns release chooses the last absolute row that entered"
    printf -v name 'file-%06d.txt' "$first"
    cardsize_expect visibleRowName "$name" "$first"
    marquee_expect selectedIndices "$selected" "Columns marks survive the release cursor's viewport refill"
    printf 'MARQUEE_COLUMNS_BOUNDARY total=%s old_held=%s first_selected=%s retained=%s\n' "$total" "$original_held" "$first" "$selected"
    kill_flea
}

marquee_filtered() {
    local dir="$marquee_box/filtered" mode i name cx cy
    marquee_guard "$dir"
    mkdir "$dir" || fail "marquee: filtered fixture creation failed"
    for i in 0 1 2 3 4 5 6 7; do
        if (( i % 2 == 0 )); then name="file-$i-keep.txt"; else name="file-$i-skip.txt"; fi
        printf 'filter %s\n' "$i" > "$dir/$name" || fail "marquee: filtered file creation failed"
    done
    # Columns deliberately has no filter entry; List and Grid exercise their shared listing-index mapping natively.
    for mode in list grid; do
        seed_ui_state "$marquee_box/filtered-$mode-state" '{"keys":"default","view":"list","preview":{"thumbnails":"off"}}'
        HOME="$marquee_home" launch "$dir"
        wait_listing 8
        permissions_viewport 880 620
        click_chrome "$mode"
        marquee_expect viewMode "$mode" "filtered fixture selects native $mode"
        marquee_filter keep
        marquee_expect drawnCount 4 "$mode filter leaves four nonconsecutive listing rows"
        cardsize_expect visibleRowName file-6-keep.txt 6
        marquee_begin_below 6
        read -r cx cy <<< "$(ipc rowCentre 0)"
        marquee_to "$cx" "$cy"
        marquee_expect selectedIndices '0,2,4,6' "$mode filtered band stores the visible files' absolute indices"
        marquee_release
        marquee_expect selectedIndices '0,2,4,6' "$mode filtered release preserves only visible marks"
        marquee_expect cursor 0 "$mode filtered release restores the last entering absolute cursor"
        key -k Escape >/dev/null || fail "marquee: filter dismissal failed"
        marquee_expect drawnCount 8 "$mode filter dismissal restores the full listing"
        marquee_expect selectedIndices '0,2,4,6' "$mode filter dismissal cannot reinterpret marks as view positions"
        kill_flea
    done
}

marquee_targets() {
    local mode dir i state deadline cx cy ax ay aw ah
    for mode in list grid; do
        dir="$marquee_box/keyboard-$mode"
        marquee_guard "$dir"
        mkdir "$dir" || fail "marquee: keyboard fixture creation failed"
        for i in 0 1 2 3; do printf 'keyboard %s\n' "$i" > "$dir/file-$i.txt" || fail "marquee: keyboard file creation failed"; done
        seed_ui_state "$marquee_box/keyboard-$mode-state" '{"keys":"default","view":"list","preview":{"thumbnails":"off"}}'
        HOME="$marquee_home" launch "$dir"
        wait_listing 4
        permissions_viewport 880 620
        click_chrome "$mode"
        marquee_expect viewMode "$mode" "keyboard target fixture enters $mode"
        if [[ "$mode" == list ]]; then
            read -r cx cy <<< "$(ipc rowCentre 0)"
            marquee_press "$cx" "$cy"
            read -r ax ay aw ah <<< "$(ipc listAreaRect)"
            marquee_to "$((ax + aw / 2))" "$((ay + ah - 12))"
            marquee_state '(.tracking | not) and (.active | not)' "a row press keeps the existing file drag and cannot start a band"
            key -k Escape >/dev/null || fail "marquee: file-drag cancellation failed"
            marquee_release
            wait_listing 4
        fi
        marquee_four "keyboard-$mode"
        marquee_release
        key y >/dev/null || fail "marquee: copy key delivery failed"
        deadline=$((SECONDS + 15))
        while (( SECONDS < deadline )); do
            state=$(ipc keyDeliveryState) || fail "marquee: clipboard observation failed"
            if jq -e --arg dir "$dir" '.clipboard.paths == [$dir + "/file-0.txt", $dir + "/file-1.txt", $dir + "/file-2.txt", $dir + "/file-3.txt"]' <<< "$state" >/dev/null; then break; fi
            sleep 0.05
        done
        jq -e --arg dir "$dir" '.clipboard.paths == [$dir + "/file-0.txt", $dir + "/file-1.txt", $dir + "/file-2.txt", $dir + "/file-3.txt"]' <<< "$state" >/dev/null \
            || fail "marquee: y did not copy the banded set: $state"
        marquee_expect selectedIndices '0,1,2,3' "$mode copying retains the shared band marks"
        marquee_guard "$(ipc path)"
        [[ "$(ipc path)" == "$dir" ]] || fail "marquee: refusing deletion outside the current owned listing"
        for i in 0 1 2 3; do marquee_guard "$dir/file-$i.txt"; done
        marquee_guard "$XDG_DATA_HOME/Trash"
        trash_guard_store 0
        key dd >/dev/null || fail "marquee: protected trash pair delivery failed"
        wait_listing 0
        for i in 0 1 2 3; do [[ ! -e "$dir/file-$i.txt" ]] || fail "marquee: dd did not trash every banded file"; done
        trash_guard_store 4
        key z >/dev/null || fail "marquee: undo key delivery failed"
        wait_listing 4
        trash_guard_store 0
        for i in 0 1 2 3; do
            [[ "$(cat "$dir/file-$i.txt")" == "keyboard $i" ]] || fail "marquee: undo did not restore exact banded contents"
        done
        printf 'MARQUEE_KEYBOARD %s copy=4 trash=4 undo=4\n' "$mode"
        kill_flea
    done
}

case_marquee() (
    local group="${1:-all}"
    local marquee_checks=0 marquee_button_down=false marquee_ctrl_down=false preset mode i state other
    local marquee_box marquee_home dir
    local trash_box trash_checks=0 trash_parent_bus_id="" trash_private_bus_id="" trash_bus_address="" trash_bus_pid="" trash_provider_pid=""
    [[ "$(realpath -e "$(command -v gio)")" == /usr/bin/gio ]] || fail "marquee: product gio resolves to a stub"
    marquee_box=$(mktemp -d "$fixture_root/marquee.XXXXXXXX") || fail "marquee: sandbox creation failed"
    printf 'native mouse-selection fixture\n' > "$marquee_box/.flea-test-sandbox"
    marquee_home="$marquee_box/home"
    fixture_home_make "$marquee_home"
    export XDG_CONFIG_HOME="$marquee_home/.config" XDG_DATA_HOME="$marquee_box/data" XDG_CACHE_HOME="$marquee_box/cache"
    export YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket"
    mkdir "$XDG_DATA_HOME" "$XDG_CACHE_HOME" || fail "marquee: private state creation failed"
    trash_box=$marquee_box
    marquee_guard "$XDG_DATA_HOME"
    HOME="$marquee_home" trash_start_bus
    trap 'marquee_cleanup $?' EXIT
    if [[ "$group" != extended ]]; then
        for preset in default vim mac windows; do
            dir="$marquee_box/$preset"
            mkdir "$dir" || fail "marquee: listing creation failed"
            for i in 0 1 2 3; do printf 'mouse selection %s\n' "$i" > "$dir/file-$i.txt" || fail "marquee: file creation failed"; done
            seed_ui_state "$marquee_box/$preset-state" "{\"keys\":\"$preset\",\"view\":\"list\",\"preview\":{\"thumbnails\":\"off\"}}"
            HOME="$marquee_home" launch "$dir"
            wait_listing 4
            permissions_viewport 880 620
            marquee_expect keymapPreset "$preset" "native preset is identified"
            for mode in list grid columns; do
                click_chrome "$mode"
                marquee_expect viewMode "$mode" "native view button selects $mode"
                marquee_interactions "$preset-$mode"
                if [[ "$preset" == default && "$mode" == grid ]]; then marquee_grid_zoom; fi
            done
            click_chrome dual
            marquee_interactions "$preset-dual-left"
            other=$(ipc dualState | jq -c '.panes[0].selected') || fail "marquee: left-pane marks unavailable"
            key -k Tab >/dev/null || fail "marquee: dual-pane Tab delivery failed"
            wait_listing 4
            marquee_interactions "$preset-dual-right"
            state=$(ipc dualState) || fail "marquee: dual-pane observation failed"
            jq -e --argjson marks "$other" '.active and .focused == 1 and .panes[0].selected == $marks and .panes[1].selected == [0,3]' <<< "$state" >/dev/null \
                || fail "marquee: mouse marks leaked between dual panes: $state"
            kill_flea
        done
    fi
    if [[ "$group" != contexts ]]; then
        marquee_scroll
        marquee_columns_boundary
        marquee_filtered
        marquee_targets
    fi
    printf 'MARQUEE_NATIVE checks=%s group=%s\n' "$marquee_checks" "$group"
)

case_marqueecontexts() { case_marquee contexts; }
case_marqueeextended() { case_marquee extended; }
