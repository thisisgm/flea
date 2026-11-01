# GPUI overrides for UISH-23; seeded from the unchanged oracle at 2faf8d099.
case_griddrop() {
    local dir="$fixture_root/griddrop" hov="$fixture_root/griddrop-hover" name i ax ay aw ah wx wy ww wh cx cy
    local folder="Move 1 item to a folder · ctrl copies, shift moves" lit after first pt
    sandbox_scratch "$dir"
    sandbox_scratch "$hov"
    mkdir -p "$dir/box" "$dir/side"
    for name in a b c d e f g; do printf 'body %s\n' "$name" > "$dir/$name.txt"; done
    for i in $(seq -w 1 80); do printf 'pad\n' > "$hov/hov-$i.txt"; done
    seed_ui_state "$fixture_root/griddrop-state" '{"keys":"default","view":"grid"}'
    launch "$dir"
    wait_listing 9
    [[ "$(ipc viewMode)" == grid ]] || fail "griddrop: the seeded view is $(ipc viewMode), not grid"

    echo "-- a file dropped on a folder tile moves, the tile named while held --"
    griddrop_hold a.txt box "Move 1 item to box · ctrl copies, shift moves"
    cap_drops_release
    wait_message "Moved 1 item · z undoes"
    [[ ! -e "$dir/a.txt" && -f "$dir/box/a.txt" ]] || fail "griddrop: a.txt did not move into box"
    wait_listing 8

    echo "-- the target follows the pointer over a file tile and two folders, and the drop lands on the last --"
    griddrop_hold b.txt c.txt "$folder" box "Move 1 item to box · ctrl copies, shift moves" d.txt "$folder" side "Move 1 item to side · ctrl copies, shift moves"
    cap_drops_release
    wait_message "Moved 1 item · z undoes"
    [[ ! -e "$dir/b.txt" && -f "$dir/side/b.txt" && ! -e "$dir/box/b.txt" ]] || fail "griddrop: b.txt did not land on side alone"
    wait_listing 7

    echo "-- a drop past the last tile takes nothing, as it takes nothing in List --"
    griddrop_hold e.txt c.txt "$folder"
    read -r ax ay aw ah <<< "$(ipc listAreaRect)"
    [[ "$ax $ay $aw $ah" =~ ^-?[0-9]+(\ -?[0-9]+){3}$ ]] || fail "griddrop: no list area, ipc answered '$ax $ay $aw $ah'"
    cap_drops_steer "$((ax + aw / 2))" "$((ay + ah - 30))"
    cap_drops_wait_target "$folder"
    cap_drops_release
    # The pane's floor under the grid takes the drop and refuses it for the file's own folder, as List's floor does.
    wait_message "Already in this folder."
    [[ -f "$dir/e.txt" && ! -e "$dir/box/e.txt" && ! -e "$dir/side/e.txt" ]] || fail "griddrop: the empty-space drop moved e.txt"
    [[ "$(ipc total)" == "7" ]] || fail "griddrop: the empty-space drop left $(ipc total) rows, not 7"
    [[ "$(ipc collideState | jq -r .opened)" == "false" ]] || fail "griddrop: the empty-space drop opened a collide card"

    echo "-- ctrl held from the lift copies --"
    # Chain the Ctrl release ahead of the EXIT command in force, as cap_drops_arm does, and put that command back after the drop.
    eval "set -- $(trap -p EXIT)"
    griddrop_saved_exit=${3-}
    trap 'griddrop_status=$?; ( YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 29:0 ) >/dev/null 2>&1 || true; set +e; (exit "$griddrop_status"); '"$griddrop_saved_exit" EXIT
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 29:1 >/dev/null 2>&1 || fail "griddrop: ctrl down refused"
    sleep 0.3
    griddrop_hold f.txt box "Copy 1 item to box"
    shot "griddrop-ctrl-held"
    # v0.3.12 Drag.js:27 and dropHeld read keys again at drop, so Copy keeps Ctrl through the release.
    cap_drops_release
    wait_message "Copied 1 item · z undoes"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 29:0 >/dev/null 2>&1 || true
    [[ -f "$dir/f.txt" && -f "$dir/box/f.txt" ]] || fail "griddrop: ctrl at the lift did not copy f.txt"

    echo "-- ctrl released before the drop clears the lift's copy bit --"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 29:1 >/dev/null 2>&1 || fail "griddrop: second ctrl down refused"
    sleep 0.3
    griddrop_hold g.txt side "Copy 1 item to side"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 29:0 >/dev/null 2>&1 || fail "griddrop: ctrl release refused"
    cap_drops_release
    wait_message "Moved 1 item · z undoes"
    [[ ! -e "$dir/g.txt" && -f "$dir/side/g.txt" && ! -e "$dir/box/g.txt" ]] || fail "griddrop: released Ctrl did not move g.txt into side alone"
    shot "griddrop-ctrl-released-result"
    if [[ -n "$griddrop_saved_exit" ]]; then trap -- "$griddrop_saved_exit" EXIT; else trap - EXIT; fi
    printf 'GRIDDROP move=box follow=side empty=refused copy=ctrl released-ctrl=move\n'
    kill_flea

    echo "-- hover: a resting pointer lights its tile, the wheel moves the light, leaving the tiles puts it out --"
    launch "$hov"
    wait_listing 80
    cap_resize 760 480
    settle
    read -r wx wy ww wh < <(window_box) || fail "griddrop: native window coordinates unavailable"
    first=$(row_index_of hov-01.txt)
    pt=$(griddrop_pt hov-01.txt) || fail "griddrop: no point on hov-01.txt"
    read -r cx cy <<< "$pt"
    pointer_glide "$((wx + cx))" "$((wy + cy))" || fail "griddrop: the pointer glide onto hov-01.txt failed"
    settle
    lit=$(griddrop_hovered)
    [[ "$lit" == "$first" ]] || fail "griddrop: a pointer resting on hov-01.txt lights [$lit], not $first"
    [[ "$(ipc viewContentY)" == "0" ]] || fail "griddrop: the grid did not start at the top"
    omarchy-drive scroll down 2 >/dev/null
    settle
    after=$(ipc viewContentY)
    [[ "$after" != "0" ]] || fail "griddrop: the wheel under the pointer did not scroll the grid"
    lit=$(griddrop_hovered)
    [[ "$lit" =~ ^[0-9]+$ && "$lit" != "$first" ]] || fail "griddrop: after the wheel the lit tiles are [$lit], not one tile other than $first"
    read -r ax ay aw ah <<< "$(ipc rowRect "$lit")"
    (( cx >= ax && cx < ax + aw && cy >= ay && cy < ay + ah )) || fail "griddrop: lit tile $lit at $ax,$ay,$aw,$ah does not hold the pointer at $cx,$cy"
    read -r ax ay aw ah <<< "$(ipc listAreaRect)"
    [[ "$ax $ay $aw $ah" =~ ^-?[0-9]+(\ -?[0-9]+){3}$ ]] || fail "griddrop: no list area, ipc answered '$ax $ay $aw $ah'"
    pointer_glide "$((wx + ax + aw / 2))" "$((wy + ay / 2))" || fail "griddrop: the pointer glide off the tiles failed"
    settle
    lit=$(griddrop_hovered)
    [[ -z "$lit" ]] || fail "griddrop: with the pointer off the tiles [$lit] stay lit"
    printf 'GRIDDROP hover=rest,wheel,off\n'
    kill_flea
}

case_railpointer() {
    local dir="$fixture_root/railpointer" mode before after suite_home="$XDG_STATE_HOME"
    sandbox_scratch "$dir"
    local i
    for i in $(seq -w 1 32); do : > "$dir/f$i.txt"; done
    launch "$dir"
    wait_listing 32
    for mode in list grid columns; do
        switch_view "$mode"
        # Click row with a real lower neighbour from actual total/stride.
        local total stride click expected
        total=$(ipc total) || fail "railpointer: $mode: total unavailable"
        stride=1
        [[ "$mode" == grid ]] && stride=$(ipc gridColumns)
        [[ "$stride" =~ ^[0-9]+$ && "$stride" -ge 1 ]] || fail "railpointer: $mode: bad stride $stride"
        click=8
        (( click + stride < total )) || click=2
        (( click + stride < total )) || click=0
        (( click + stride < total )) || fail "railpointer: $mode: no row with a lower neighbour (total=$total stride=$stride)"
        expected=$((click + stride))
        click_row 5 left
        settle
        key -k Tab >/dev/null
        settle
        [[ "$(ipc focusView)" == "rail" ]] || fail "railpointer: $mode: Tab did not reach the rail"
        click_row "$click" left
        settle
        [[ "$(ipc focusView)" == "list" ]] || fail "railpointer: $mode: a click on row $click left the keyboard on the rail (focusView=$(ipc focusView))"
        [[ "$(ipc cursor)" == "$click" ]] || fail "railpointer: $mode: a click on row $click left the cursor on $(ipc cursor)"
        before=$(ipc cursor)
        # Witness, not a gate: stride versus miss needs geometry on the failing j.
        printf 'RAILPOINTER_WITNESS mode=%s phase=before-j cursor=%s stride=%s centre=%s rect=%s focus=%s view=%s total=%s click=%s expected=%s\n' "$mode" "$before" "$stride" "$(ipc rowCentre "$click")" "$(ipc rowRect "$click")" "$(ipc focusView)" "$(ipc viewMode)" "$total" "$click" "$expected"
        key j >/dev/null
        settle
        after=$(ipc cursor)
        printf 'RAILPOINTER_WITNESS mode=%s phase=after-j cursor=%s stride=%s focus=%s\n' "$mode" "$after" "$stride" "$(ipc focusView)"
        [[ "$after" != "$before" ]] || fail "railpointer: $mode: j after a row click stayed on $before"
        [[ "$after" == "$expected" ]] || fail "railpointer: $mode: j after a row click reached $after, want $expected (total=$total stride=$stride)"
        key -k Tab >/dev/null
        settle
        [[ "$(ipc focusView)" == "rail" ]] || fail "railpointer: $mode: the second Tab did not reach the rail (focusView=$(ipc focusView))"
        click_row "$click" right
        settle
        [[ "$(ipc focusView)" == "list" ]] || fail "railpointer: $mode: a right click left the keyboard on $(ipc focusView) instead of returning it to the list"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc focusView)" == "list" ]] || fail "railpointer: $mode: a right click plus Escape left the keyboard on the rail (focusView=$(ipc focusView))"
        before=$(ipc cursor)
        printf 'RAILPOINTER_WITNESS mode=%s phase=before-right-j cursor=%s stride=%s centre=%s rect=%s focus=%s view=%s total=%s click=%s expected=%s\n' "$mode" "$before" "$stride" "$(ipc rowCentre "$click")" "$(ipc rowRect "$click")" "$(ipc focusView)" "$(ipc viewMode)" "$total" "$click" "$expected"
        key j >/dev/null
        settle
        after=$(ipc cursor)
        printf 'RAILPOINTER_WITNESS mode=%s phase=after-right-j cursor=%s stride=%s focus=%s\n' "$mode" "$after" "$stride" "$(ipc focusView)"
        [[ "$after" != "$before" ]] || fail "railpointer: $mode: j after right click stayed on $before"
        [[ "$after" == "$expected" ]] || fail "railpointer: $mode: j after right click reached $after, want $expected (total=$total stride=$stride)"
        printf 'RAILPOINTER %s left=ok right=ok\n' "$mode"
    done
    # A press on the revealed auto-hide rail is a rail press, so it takes the rail keyboard even from the list.
    seed_ui_state "$fixture_root/railpointer-hide" '{"view":"list","places":{"autoHide":true}}'
    launch "$dir"
    wait_listing 32
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "railpointer: auto-hide Tab did not reach the rail"
    wait_rail 1
    local rail_total
    rail_total=$(ipc railCount)
    [[ "$rail_total" =~ ^[0-9]+$ ]] || fail "railpointer: the revealed rail has no numeric count (railCount=$rail_total railState=$(ipc railState))"
    (( rail_total >= 2 )) || fail "railpointer: the revealed rail has $rail_total rows, need two for the landed-press proof (railState=$(ipc railState))"
    # Rail j still steps before the pointer path.
    if [[ "$(ipc railCursor)" == "0" ]]; then
        key j >/dev/null
        settle
        [[ "$(ipc railCursor)" != "0" ]] || fail "railpointer: j never left rail row 0 (railCount=$rail_total)"
    fi
    # Return focus to the list: Tab leaves the rail for the bar and one more reaches the list (a one-tab window has no strip); the hide unloads the rail Loader and resets its cursor.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "chrome" ]] || fail "railpointer: Tab out of the rail left the keyboard on $(ipc focusView) instead of the bar"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "railpointer: Tab back to the list left the keyboard on $(ipc focusView) instead of the list"
    local hid=false hid_json
    for _attempt in $(seq 1 60); do
        hid_json=$(ipc railState)
        [[ "$(jq -r '.hidden' 2>/dev/null <<< "$hid_json")" == "true" ]] && { hid=true; break; }
        sleep 0.05
    done
    [[ "$hid" == true ]] || fail "railpointer: the rail never hid before the reveal (railState=$hid_json)"
    # Without these the reveal gesture below is silent, and the rail assertions after it are vacuous.
    command -v ydotool >/dev/null || fail "railpointer: no ydotool on PATH, so the auto-hide rail cannot be revealed"
    [[ -S "${XDG_RUNTIME_DIR:-}/.ydotool_socket" ]] || fail "railpointer: no ydotool socket at ${XDG_RUNTIME_DIR:-}/.ydotool_socket, so the auto-hide rail cannot be revealed"
    local wx wy wh
    read -r wx wy _ww wh < <(window_box) || fail "railpointer: native window coordinates unavailable"
    omarchy-drive move "$((wx + 1))" "$((wy + wh / 2))" >/dev/null || fail "railpointer: the pointer move to the left edge failed, so the rail never revealed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null || fail "railpointer: ydotool mousemove failed, so the rail never revealed"
    settle
    # The rail Loader unloads while hidden, so a polled hidden=false proves the reveal happened.
    local revealed=false rail_json
    for _attempt in $(seq 1 60); do
        rail_json=$(ipc railState)
        [[ "$(jq -r '.hidden' 2>/dev/null <<< "$rail_json")" == "false" ]] && { revealed=true; break; }
        sleep 0.05
    done
    [[ "$revealed" == true ]] || fail "railpointer: the auto-hide rail never revealed (railState=$rail_json)"
    # Real mouse path: the pointer reveals while the keyboard stays in the list.
    [[ "$(ipc focusView)" == "list" ]] || fail "railpointer: the pointer reveal left the keyboard on $(ipc focusView) instead of the list"
    [[ "$(jq -r '.hidden' 2>/dev/null <<< "$(ipc railState)")" == "false" ]] || fail "railpointer: the rail is not shown before the press (railState=$(ipc railState))"
    # A hide unloads the rail Loader and resets its cursor to 0, so press the row it does not hold.
    local rail_before rail_target
    rail_before=$(ipc railCursor)
    [[ "$rail_before" =~ ^[0-9]+$ ]] || fail "railpointer: the revealed rail has no numeric cursor (railCursor=$rail_before)"
    if [[ "$rail_before" == "0" ]]; then rail_target=1; else rail_target=0; fi
    [[ -n "$(ipc railRowCentre "$rail_target")" ]] || fail "railpointer: rail row $rail_target has no on-screen centre while revealed, so the press has no target"
    click_rail_row "$rail_target" left
    settle
    [[ "$(ipc railCursor)" == "$rail_target" ]] || fail "railpointer: a press on revealed rail row $rail_target left the rail cursor on $(ipc railCursor) (was $rail_before)"
    [[ "$(ipc focusView)" == "rail" ]] || fail "railpointer: a press on the revealed rail left the keyboard on $(ipc focusView)"
    printf 'RAILPOINTER autohide=ok\n'
    # In dual view Tab still switches panes, and a row click focuses that pane with the list keyboard.
    local dual="$fixture_root/railpointer-dual" dstate="$fixture_root/railpointer-dual-state" rect rx ry rw rh cx cy ww hh dual_json
    sandbox_scratch "$dual"
    mkdir -p "$dual/left" "$dual/right"
    for i in $(seq -f '%02g' 1 6); do : > "$dual/left/l$i.txt"; : > "$dual/right/r$i.txt"; done
    seed_ui_state "$dstate" "$(jq -cn --arg l "$dual/left" --arg r "$dual/right" '{view:"dual",dual:{paths:[$l,$r],focus:0}}')"
    launch "$dual/left"
    # An empty panes array reads as not-loading, so the wait requires a pane to exist first.
    local dual_ready=false
    for _attempt in $(seq 1 100); do
        dual_json=$(ipc dualState)
        [[ "$(jq -r '(.panes | length > 0) and ([.panes[].loading] | any | not)' 2>/dev/null <<< "$dual_json")" == "true" ]] && { dual_ready=true; break; }
        sleep 0.05
    done
    [[ "$dual_ready" == true ]] || fail "railpointer: dual panes still loading after 5 s (dualState=$dual_json)"
    ipc dualState | jq -e '.active and .focused == 0' >/dev/null || fail "railpointer: dual did not open focused on pane 0"
    key -k Tab >/dev/null
    settle
    ipc dualState | jq -e '.focused == 1' >/dev/null || fail "railpointer: dual Tab did not switch panes"
    # .folder is pane 0 row 0 (l01.txt), so its rect centre is a pinned row and never pane chrome.
    local geo
    geo=$(ipc dragPaneGeometry 0 0) || fail "railpointer: dual geometry read failed for pane 0 row 0"
    rect=$(jq -er '.folder.rect' <<< "$geo") || fail "railpointer: pane 0 row 0 has no rect (geometry=$geo)"
    [[ -n "$rect" ]] || fail "railpointer: pane 0 row 0 read an empty rect (geometry=$geo)"
    read -r rx ry rw rh <<< "$rect"
    [[ "$rx" =~ ^-?[0-9]+$ && "$ry" =~ ^-?[0-9]+$ && "$rw" =~ ^-?[0-9]+$ && "$rh" =~ ^-?[0-9]+$ ]] || fail "railpointer: pane 0 row 0 read no four numbers (rect=$rect)"
    (( rw > 0 && rh > 0 )) || fail "railpointer: pane 0 row 0 read an empty rect (rect=$rect)"
    [[ "$(jq -er '.folder.name' <<< "$geo")" == "l01.txt" ]] || fail "railpointer: pane 0 row 0 names $(jq -r '.folder.name' <<< "$geo"), want l01.txt, so the click is not pinned to a row (geometry=$geo)"
    read -r wx wy ww hh < <(window_box) || fail "railpointer: native window coordinates unavailable"
    omarchy-drive click "$((wx + rx + rw / 2))" "$((wy + ry + rh / 2))" left >/dev/null || fail "railpointer: the click on pane 0 row 0 failed"
    settle
    ipc dualState | jq -e '.focused == 0' >/dev/null || fail "railpointer: a row click on the other pane left focus on pane $(ipc dualState | jq -r '.focused')"
    [[ "$(ipc focusView)" == "list" ]] || fail "railpointer: a row click on the other pane left the keyboard on $(ipc focusView)"
    printf 'RAILPOINTER dual=ok\n'
    export XDG_STATE_HOME="$suite_home"
    kill_flea
}

case_dualautohidereveal() {
    local dir="$fixture_root/dualautohidereveal" dstate="$fixture_root/dualautohidereveal-state" suite_home="$XDG_STATE_HOME"
    sandbox_scratch "$dir"
    mkdir -p "$dir/left" "$dir/right"
    local i
    for i in $(seq -f '%02g' 1 6); do : > "$dir/left/l$i.txt"; : > "$dir/right/r$i.txt"; done
    seed_ui_state "$dstate" "$(jq -cn --arg l "$dir/left" --arg r "$dir/right" '{view:"dual",places:{autoHide:true},dual:{paths:[$l,$r],focus:0}}')"
    launch "$dir/left"
    local dual_ready=false dual_json
    for _attempt in $(seq 1 100); do
        dual_json=$(ipc dualState)
        [[ "$(jq -r '(.panes | length == 2) and all(.panes[]; . != null and .loading == false)' 2>/dev/null <<< "$dual_json")" == "true" ]] && { dual_ready=true; break; }
        sleep 0.05
    done
    [[ "$dual_ready" == true ]] || fail "dualautohidereveal: two listed dual panes never appeared in 5 s (dualState=$dual_json)"
    local hid=false hid_json
    for _attempt in $(seq 1 60); do
        hid_json=$(ipc railState)
        [[ "$(jq -r '.hidden' 2>/dev/null <<< "$hid_json")" == "true" ]] && { hid=true; break; }
        sleep 0.05
    done
    [[ "$hid" == true ]] || fail "dualautohidereveal: the rail never hid before the reveal (railState=$hid_json)"
    local hidden0 hidden1 shown0 shown1 d
    hidden0=$(dual_list_width 0) || exit 1
    hidden1=$(dual_list_width 1) || exit 1
    d=$(( hidden0 > hidden1 ? hidden0 - hidden1 : hidden1 - hidden0 ))
    (( d <= 1 )) || fail "dualautohidereveal: hidden listing widths differ ($hidden0 vs $hidden1)"
    command -v ydotool >/dev/null || fail "dualautohidereveal: no ydotool on PATH, so the auto-hide rail cannot be revealed"
    [[ -S "${XDG_RUNTIME_DIR:-}/.ydotool_socket" ]] || fail "dualautohidereveal: no ydotool socket at ${XDG_RUNTIME_DIR:-}/.ydotool_socket, so the auto-hide rail cannot be revealed"
    local wx wy wh
    read -r wx wy _ww wh < <(window_box) || fail "dualautohidereveal: native window coordinates unavailable"
    omarchy-drive move "$((wx + 1))" "$((wy + wh / 2))" >/dev/null || fail "dualautohidereveal: the pointer move to the left edge failed"
    # Hyprland's cursor move sends no pointer frame, so a one pixel seat nudge makes Qt hear the hover.
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null || fail "dualautohidereveal: ydotool mousemove failed"
    settle
    local revealed=false rail_json
    for _attempt in $(seq 1 60); do
        rail_json=$(ipc railState)
        [[ "$(jq -r '.hidden' 2>/dev/null <<< "$rail_json")" == "false" ]] && { revealed=true; break; }
        sleep 0.05
    done
    [[ "$revealed" == true ]] || fail "dualautohidereveal: the auto-hide rail never revealed (railState=$rail_json)"
    shown0=$(dual_list_width 0) || exit 1
    shown1=$(dual_list_width 1) || exit 1
    d=$(( shown0 > shown1 ? shown0 - shown1 : shown1 - shown0 ))
    (( d <= 1 )) || fail "dualautohidereveal: revealed listing widths differ ($shown0 vs $shown1)"
    d=$(( shown0 > hidden0 ? shown0 - hidden0 : hidden0 - shown0 ))
    (( d <= 1 )) || fail "dualautohidereveal: the reveal moved the left listing ($hidden0 to $shown0)"
    printf 'DUALAUTOHIDEREVEAL hidden=%s/%s revealed=%s/%s\n' "$hidden0" "$hidden1" "$shown0" "$shown1"
    export XDG_STATE_HOME="$suite_home"
    kill_flea
}

case_dualdivider() {
    local dir="$fixture_root/dualdivider" suite_home="$XDG_STATE_HOME" rail
    sandbox_scratch "$dir"
    mkdir -p "$dir/left" "$dir/right"
    local i
    for i in $(seq -f '%02g' 1 40); do : > "$dir/left/l$i.txt"; : > "$dir/right/r$i.txt"; done
    command -v ydotool >/dev/null || fail "dualdivider: no ydotool on PATH"
    [[ -S "${XDG_RUNTIME_DIR:-}/.ydotool_socket" ]] || fail "dualdivider: no ydotool socket at ${XDG_RUNTIME_DIR:-}/.ydotool_socket"
    local drag=120
    # No IPC read names the divider's begin, move, end and cancel counts, so the handle logs each event it hears to the flea log when this is set (ui/ResizeHandle.qml trace).
    export FLEA_TRACE_RESIZE=1
    for rail in shown hidden; do
        seed_ui_state "$fixture_root/dualdivider-state-$rail" "$(jq -cn --arg l "$dir/left" --arg r "$dir/right" --arg rail "$rail" '{view:"dual",places:{rail:$rail},dual:{paths:[$l,$r],focus:0}}')"
        dualdivider_launch "$dir/left"
        local w0 w1 wx wy ww wh div y moved0 moved1 stored
        read -r w0 w1 <<< "$(dualdivider_equal "$rail default")"
        read -r wx wy ww wh < <(window_box) || fail "dualdivider: native window coordinates unavailable"
        div=$(dual_divider_x) || exit 1
        y=$(( wy + wh / 2 ))
        # The zone starts on the divider pixel and runs 9 px right, so its middle is 4 px past it.
        omarchy-drive move "$(( wx + div + 4 ))" "$y" >/dev/null || fail "dualdivider: $rail: the move onto the divider failed"
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null || fail "dualdivider: $rail: ydotool mousemove failed"
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 || fail "dualdivider: $rail: the press failed"
        column_drag_absolute "$(( wx + div + 4 ))" "$y" "$drag" 4
        # Nothing re-lays while the pointer is down; the handle log says whether a press, move, release or cancel reached the divider.
        local trace="" event
        for event in press move release cancel; do trace+=" $event=$(grep -acw "RESIZE divider $event" "$flea_log" || true)"; done
        printf 'DUALDIVIDER %s events%s\n' "$rail" "$trace"
        read -r moved0 moved1 <<< "$(dual_list_width 0) $(dual_list_width 1)"
        (( moved0 == w0 && moved1 == w1 )) || fail "dualdivider: $rail: the listings moved during the drag ($w0/$w1 to $moved0/$moved1), divider events:$trace"
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || fail "dualdivider: $rail: the release failed"
        settle
        read -r moved0 moved1 <<< "$(dual_list_width 0) $(dual_list_width 1)"
        (( moved0 - w0 >= drag - 3 && moved0 - w0 <= drag + 3 && w1 - moved1 >= drag - 3 && w1 - moved1 <= drag + 3 )) \
            || fail "dualdivider: $rail: a $drag px drag moved the listings $w0/$w1 to $moved0/$moved1"
        stored=$(jq -er '.dual.split' "$XDG_STATE_HOME/flea/ui.json") || fail "dualdivider: $rail: ui.json holds no dual.split"
        (( stored > 5000 )) || fail "dualdivider: $rail: ui.json remembers split $stored"
        printf 'DUALDIVIDER %s drag=%s/%s split=%s\n' "$rail" "$moved0" "$moved1" "$stored"
        dualdivider_launch "$dir/left"
        local r0 r1 d
        read -r r0 r1 <<< "$(dual_list_width 0) $(dual_list_width 1)"
        d=$(( r0 > moved0 ? r0 - moved0 : moved0 - r0 ))
        (( d <= 1 )) || fail "dualdivider: $rail: the relaunch lays the left listing at $r0, the drag left it at $moved0"
        read -r wx wy ww wh < <(window_box) || fail "dualdivider: native window coordinates unavailable"
        div=$(dual_divider_x) || exit 1
        omarchy-drive click "$(( wx + div + 4 ))" "$(( wy + wh / 2 ))" left --double >/dev/null || fail "dualdivider: $rail: the double click failed"
        settle
        read -r r0 r1 <<< "$(dualdivider_equal "$rail double click")"
        d=$(( r0 > w0 ? r0 - w0 : w0 - r0 ))
        (( d <= 1 )) || fail "dualdivider: $rail: the double click lays the listings at $r0/$r1, the default was $w0/$w1"
        [[ "$(jq -er '.dual.split' "$XDG_STATE_HOME/flea/ui.json")" == 5000 ]] || fail "dualdivider: $rail: the double click left split $(jq -r '.dual.split' "$XDG_STATE_HOME/flea/ui.json")"
        printf 'DUALDIVIDER %s double=%s/%s\n' "$rail" "$r0" "$r1"
    done
    unset FLEA_TRACE_RESIZE
    export XDG_STATE_HOME="$suite_home"
    kill_flea
}
