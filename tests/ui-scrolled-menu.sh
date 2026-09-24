#!/usr/bin/env bash
# Sourced by ui.sh; menu routing is observed after native pointer input.

scrolled_menu_expect() {
    local has_row="$1" label="$2" state
    settle
    state=$(ipc menuState)
    jq -e --argjson row "$has_row" '.opened and .hasRow == $row' <<< "$state" >/dev/null \
        || fail "scrolledmenu: $label: $state"
    printf 'SCROLLEDMENU %s hasRow=%s scroll=%s\n' "$label" "$has_row" "$(ipc viewContentY)"
    key -k Escape >/dev/null
    settle
}

# Twenty files fit in the tall window but require scrolling after it shrinks. A native right click
# on the final delegate must keep the item menu: the background handler also sees that event, and
# adding contentY to a point already in content coordinates used to replace it with the directory
# menu. Each view repeats the click after growing again, and checks the measured empty tail and an
# empty directory so suppressing the background handler altogether cannot satisfy the case.
case_scrolledmenu() {
    local dir="$fixture_root/scrolledmenu" mode height addr i
    local wx wy ww wh ax ay aw ah rx ry rw rh cx cy
    sandbox_scratch "$dir"
    mkdir -p "$dir/files" "$dir/empty"
    for i in $(seq -w 1 20); do : > "$dir/files/file-$i.txt"; done

    for mode in list columns grid; do
        seed_ui_state "$fixture_root/scrolledmenu-state-$mode" "{\"view\":\"$mode\",\"keys\":\"default\",\"density\":\"compact\",\"display\":{\"textSize\":{\"mode\":14}},\"preview\":{\"column\":false,\"thumbnails\":\"off\"}}"
        launch "$dir/files"
        wait_listing 20
        [[ "$(ipc viewMode)" == "$mode" ]] || fail "scrolledmenu: wrong starting view"
        addr=$(hyprctl -j clients | jq -er --argjson pid "$(flea_pid)" '.[] | select(.pid == $pid) | .address')
        [[ "$addr" =~ ^0x[0-9a-fA-F]+$ ]] || fail "scrolledmenu: missing owned window"
        omarchy-drive window float "$addr" >/dev/null || fail "scrolledmenu: cannot float owned window"

        for height in 900 400 900; do
            hyprctl dispatch "hl.dsp.window.resize({ x = 900, y = $height, exact = true, window = \"address:$addr\" })" >/dev/null
            settle
            read -r wx wy ww wh < <(window_box) || fail "scrolledmenu: native geometry unavailable"
            [[ "$ww $wh" == "900 $height" ]] || fail "scrolledmenu: resize did not reach 900x$height"
            key -k Home >/dev/null
            settle
            click_row 0 right
            scrolled_menu_expect true "$mode/$height first item"
            # The row click also puts the pointer over this view before the real wheel events.
            omarchy-drive scroll down 20 >/dev/null
            settle
            if [[ "$height" == 400 ]]; then
                (( $(ipc viewContentY) > 0 )) || fail "scrolledmenu: short $mode did not scroll"
            else
                [[ "$(ipc viewContentY)" == 0 ]] || fail "scrolledmenu: tall $mode still needs scrolling"
            fi
            read -r rx ry rw rh <<< "$(ipc rowRect 19)"
            (( rw > 0 && rh > 0 && ry >= 0 && ry + rh <= wh )) \
                || fail "scrolledmenu: last item is not visible: $rx $ry $rw $rh"
            click_row 19 right
            scrolled_menu_expect true "$mode/$height last item"
            [[ "$(ipc cursor)" == 19 ]] || fail "scrolledmenu: right click did not target the last item"

            # Columns' listArea is the enclosing surface; the row identifies the active column's x.
            read -r ax ay aw ah <<< "$(ipc listAreaRect)"
            cx=$((rx + rw / 2)); cy=$(((ry + rh + ay + ah) / 2))
            (( ry + rh < cy && cy < ay + ah && cx < ww && cy < wh )) \
                || fail "scrolledmenu: no visible empty tail below the final item"
            omarchy-drive click "$((wx + cx))" "$((wy + cy))" right >/dev/null
            scrolled_menu_expect false "$mode/$height empty tail"
        done

        key -M ctrl -k l -m ctrl "$dir/empty" -k Return >/dev/null
        wait_listing 0
        read -r ax ay aw ah <<< "$(ipc listAreaRect)"
        # Keep the previous row's x inside the active column, including in columns mode.
        omarchy-drive click "$((wx + cx))" "$((wy + ay + ah / 2))" right >/dev/null
        scrolled_menu_expect false "$mode empty directory"
    done
}
