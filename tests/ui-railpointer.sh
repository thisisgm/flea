#!/usr/bin/env bash
# Sourced by ui.sh; a listing click takes the keyboard back from the rail.
# shellcheck disable=SC2154 # ui.sh supplies the fixture root and the native driver helpers.
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
# Dual view with Auto-hide on: both listings stay equal and steady while the overlay rail is revealed (ui/WindowBody.qml).
dual_list_width() {
    local geo area w
    geo=$(ipc dragPaneGeometry "$1" 0) || fail "dualautohidereveal: geometry read failed for pane $1"
    # area is rectOf's "x y width height" string, rounded to whole pixels.
    area=$(jq -er '.area' <<< "$geo") || fail "dualautohidereveal: pane $1 has no list area (geometry=$geo)"
    read -r _ _ w _ <<< "$area"
    [[ "$w" =~ ^[0-9]+$ ]] || fail "dualautohidereveal: pane $1 list area '$area' has no whole width"
    printf '%s\n' "$w"
}

# Dual view Tab walks the left pane's rail, the bar, the left list and the right list, Shift+Tab walks it back, and Tab reaching an auto-hide rail reveals it.
dual_tab_at() {
    local label=$1 side=$2 view=$3
    settle
    ipc dualState | jq -e --argjson side "$side" '.focused == $side' >/dev/null || fail "dualtab: $label left focus on pane $(ipc dualState | jq -r '.focused'), want $side"
    [[ "$(ipc focusView)" == "$view" ]] || fail "dualtab: $label left the keyboard on $(ipc focusView), want $view"
}
dual_tab_launch() {
    local dir=$1 state=$2 extra=$3 dual_json dual_ready=false
    seed_ui_state "$state" "$(jq -cn --arg l "$dir/left" --arg r "$dir/right" --argjson extra "$extra" '{view:"dual",dual:{paths:[$l,$r],focus:0}} + $extra')"
    launch "$dir/left"
    for _attempt in $(seq 1 100); do
        dual_json=$(ipc dualState)
        [[ "$(jq -r '(.panes | length == 2) and all(.panes[]; . != null and .loading == false)' 2>/dev/null <<< "$dual_json")" == "true" ]] && { dual_ready=true; break; }
        sleep 0.05
    done
    [[ "$dual_ready" == true ]] || fail "dualtab: two listed dual panes never appeared in 5 s (dualState=$dual_json)"
    dual_tab_at "the launch" 0 list
}
case_dualtab() {
    local dir="$fixture_root/dualtab" suite_home="$XDG_STATE_HOME" i
    sandbox_scratch "$dir"
    mkdir -p "$dir/left" "$dir/right"
    for i in $(seq -f '%02g' 1 6); do : > "$dir/left/l$i.txt"; : > "$dir/right/r$i.txt"; done
    dual_tab_launch "$dir" "$fixture_root/dualtab-state" '{}'
    key -k Tab >/dev/null
    dual_tab_at "Tab from the left list" 1 list
    key -k Tab >/dev/null
    dual_tab_at "Tab from the right list" 0 rail
    shot dualtab-rail
    key -k Tab >/dev/null
    dual_tab_at "Tab from the rail" 0 chrome
    key -k Tab >/dev/null
    dual_tab_at "Tab from the bar, a window of one tab having no strip" 0 list
    key -M shift -k Tab -m shift >/dev/null
    dual_tab_at "Shift+Tab from the left list" 0 chrome
    key -M shift -k Tab -m shift >/dev/null
    dual_tab_at "Shift+Tab from the bar" 0 rail
    key -M shift -k Tab -m shift >/dev/null
    dual_tab_at "Shift+Tab from the rail" 1 list
    key -M shift -k Tab -m shift >/dev/null
    dual_tab_at "Shift+Tab from the right list" 0 list
    kill_flea
    dual_tab_launch "$dir" "$fixture_root/dualtab-hide-state" '{"places":{"autoHide":true}}'
    local hid=false hid_json
    for _attempt in $(seq 1 60); do
        hid_json=$(ipc railState)
        [[ "$(jq -r '.hidden' 2>/dev/null <<< "$hid_json")" == "true" ]] && { hid=true; break; }
        sleep 0.05
    done
    [[ "$hid" == true ]] || fail "dualtab: the auto-hide rail never hid before the Tab (railState=$hid_json)"
    key -k Tab >/dev/null
    dual_tab_at "Tab from the left list with the rail hidden" 1 list
    key -k Tab >/dev/null
    dual_tab_at "Tab from the right list with the rail hidden" 0 rail
    local revealed=false rail_json
    for _attempt in $(seq 1 60); do
        rail_json=$(ipc railState)
        [[ "$(jq -r '.hidden' 2>/dev/null <<< "$rail_json")" == "false" ]] && { revealed=true; break; }
        sleep 0.05
    done
    [[ "$revealed" == true ]] || fail "dualtab: Tab from the right list never revealed the auto-hide rail (railState=$rail_json)"
    printf 'DUALTAB cycle=ok autohide=revealed\n'
    export XDG_STATE_HOME="$suite_home"
    kill_flea
}
# The divider's x in window pixels: the right edge of the left listing, where its hairline sits.
dual_divider_x() {
    local geo area x w
    geo=$(ipc dragPaneGeometry 0 0) || fail "dualdivider: geometry read failed for pane 0"
    area=$(jq -er '.area' <<< "$geo") || fail "dualdivider: pane 0 has no list area (geometry=$geo)"
    # area is "x y w h" in window pixels, e.g. "12 40 520 600"
    read -r x _ w _ <<< "$area"
    [[ "$x" =~ ^-?[0-9]+$ && "$w" =~ ^[0-9]+$ ]] || fail "dualdivider: pane 0 list area '$area' has no whole x and width"
    printf '%s\n' "$((x + w))"
}
dualdivider_launch() {
    launch "$1"
    local ready=false json
    for _attempt in $(seq 1 100); do
        # dualState is {"active":true,"focused":0,"panes":[{"path":"/x","loading":false,...},{...}]}
        json=$(ipc dualState)
        [[ "$(jq -r '(.panes | length == 2) and all(.panes[]; . != null and .loading == false)' 2>/dev/null <<< "$json")" == "true" ]] && { ready=true; break; }
        sleep 0.05
    done
    [[ "$ready" == true ]] || fail "dualdivider: two listed dual panes never appeared in 5 s (dualState=$json)"
    settle
}
# Asserts the two listing widths differ by at most 1 px, then prints them.
dualdivider_equal() {
    local a b d
    a=$(dual_list_width 0) || exit 1
    b=$(dual_list_width 1) || exit 1
    d=$(( a > b ? a - b : b - a ))
    (( d <= 1 )) || fail "dualdivider: $1: the listings are not equal ($a vs $b)"
    printf '%s %s\n' "$a" "$b"
}

# The auto-hidden rail's slide, driven for a recording: dwell, leave, re-enter mid-hide, Tab in and Tab out, single view then Dual (runs by name only).
railmotion_state() { printf 'RAILMOTION %s %s: %s\n' "$1" "$2" "$(ipc railState)"; sleep "${railmotion_pause:-1}"; }
# A railmotion failure names the rail and the pointer, so a log shows whether the pointer rested on the reveal edge.
railmotion_fail() { fail "$1 (railState=$(ipc railState 2>&1 || true) cursorpos=$(hyprctl cursorpos 2>&1 || true))"; }
railmotion_point() {
    omarchy-drive move "$1" "$2" >/dev/null || railmotion_fail "cap_railmotion: the pointer move to $1,$2 failed"
    # Hyprland's cursor move sends no pointer frame, so a one pixel seat nudge makes Qt hear the hover.
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null || railmotion_fail "cap_railmotion: ydotool mousemove failed"
}
# Scheduling slack added to each rail wait: it absorbs the frame and event-loop delay after a timer fires.
railmotion_slack_ms=100
# The rail's own times, read from the code that owns them: the dwell and the open and close slides in ui/js/Motion.js, the settle in ui/PaneRail.qml.
railmotion_times() {
    local dwell settle open close
    # Motion.js line "var dwellMs = 120", the dwell in ms.
    dwell=$(sed -nE 's/^var dwellMs = ([0-9]+).*/\1/p' "$flea_ui/js/Motion.js")
    # Motion.js line "    open: 180,", the open slide in ms (a trailing comma).
    open=$(sed -nE 's/^ *open: ([0-9]+),.*/\1/p' "$flea_ui/js/Motion.js")
    # Motion.js line "    close: 140", the close slide in ms (the last key, no comma).
    close=$(sed -nE 's/^ *close: ([0-9]+)$/\1/p' "$flea_ui/js/Motion.js")
    # PaneRail.qml line "    readonly property int settleMs: 220", the settle in ms.
    settle=$(sed -nE 's/^ *readonly property int settleMs: ([0-9]+).*/\1/p' "$flea_ui/PaneRail.qml")
    [[ "$dwell" =~ ^[0-9]+$ && "$open" =~ ^[0-9]+$ && "$close" =~ ^[0-9]+$ && "$settle" =~ ^[0-9]+$ ]] \
        || fail "cap_railmotion: the rail times are not readable (dwell=$dwell open=$open close=$close settle=$settle)"
    # The reveal is the dwell then the open slide, the hide the settle then the close slide, each plus the slack.
    rail_reveal_ms=$((dwell + open + railmotion_slack_ms)); rail_hide_ms=$((settle + close + railmotion_slack_ms))
}
railmotion_wait() { sleep "$(printf '%d.%03d' $(($1 / 1000)) $(($1 % 1000)))"; }
railmotion_walk() {
    local mode="$1" wx wy ww wh edge_x away_x y hid=false rail_reveal_ms rail_hide_ms
    railmotion_times
    read -r wx wy ww wh < <(window_box) || fail "cap_railmotion: native window coordinates unavailable"
    edge_x=$((wx + 1)); away_x=$((wx + ww * 6 / 10)); y=$((wy + wh / 2))
    # A relaunched window can open under a pointer on the reveal edge, which keeps the rail up.
    railmotion_point "$away_x" "$y"
    railmotion_wait "$rail_hide_ms"
    for _attempt in $(seq 1 60); do
        [[ "$(ipc railState | jq -r '.hidden')" == "true" ]] && { hid=true; break; }
        sleep 0.05
    done
    [[ "$hid" == true ]] || railmotion_fail "cap_railmotion: $mode: the rail never hid before the walk"
    railmotion_state "$mode" rest
    railmotion_point "$edge_x" "$y"
    railmotion_wait "$rail_reveal_ms"
    railmotion_state "$mode" dwell
    railmotion_point "$away_x" "$y"
    railmotion_wait "$rail_hide_ms"
    railmotion_state "$mode" leave
    railmotion_point "$edge_x" "$y"
    railmotion_wait "$rail_reveal_ms"
    railmotion_state "$mode" reveal-again
    railmotion_point "$away_x" "$y"
    # The settle is 220 ms and the hide slide 140 ms, so the pointer returns while the rail is part way out.
    sleep 0.3
    railmotion_point "$edge_x" "$y"
    railmotion_state "$mode" re-enter-mid-hide
    railmotion_point "$away_x" "$y"
    railmotion_wait "$rail_hide_ms"
    key -k Tab >/dev/null
    railmotion_state "$mode" tab-in
    key -k Tab >/dev/null
    railmotion_state "$mode" tab-out
}
case_cap_railmotion() {
    local dir="$fixture_root/railmotion" state="$fixture_root/railmotion-state" suite_home="$XDG_STATE_HOME" i
    command -v ydotool >/dev/null || fail "cap_railmotion: no ydotool on PATH"
    [[ -S "${XDG_RUNTIME_DIR:-}/.ydotool_socket" ]] || fail "cap_railmotion: no ydotool socket at ${XDG_RUNTIME_DIR:-}/.ydotool_socket"
    sandbox_scratch "$dir"
    mkdir -p "$dir/left" "$dir/right"
    for i in $(seq -f '%02g' 1 20); do : > "$dir/left/l$i.txt"; : > "$dir/right/r$i.txt"; done
    seed_ui_state "$state" "$(jq -cn --arg l "$dir/left" '{places:{autoHide:true},dual:{paths:[$l,$l],focus:0}}')"
    launch "$dir/left"
    wait_listing 20
    railmotion_walk single
    # The seed wipes the state directory first, so the Dual leg names auto-hide again.
    seed_ui_state "$state" "$(jq -cn --arg l "$dir/left" '{view:"dual",places:{autoHide:true},dual:{paths:[$l,$l],focus:0}}')"
    launch "$dir/left"
    wait_listing 20
    railmotion_walk dual
    export XDG_STATE_HOME="$suite_home"
    kill_flea
}
