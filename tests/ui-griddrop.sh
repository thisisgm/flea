# Sourced by ui.sh: the Grid's one shared DropArea and one HoverHandler under a real compositor drag and a real pointer.

# The icon box of a tile as a window-relative point, the part of a tile a press and a drop both hit.
griddrop_pt() {
    [[ "$#" -eq 1 ]] || fail "griddrop: pt needs a name, got $#"
    local x y w h
    read -r x y w h <<< "$(ipc rowThumbRect "$(row_index_of "$1")")"
    [[ "$x $y $w $h" =~ ^-?[0-9]+(\ -?[0-9]+){3}$ ]] || fail "griddrop: $1 has no icon box, ipc answered '$x $y $w $h'"
    printf '%s %s\n' "$((x + w / 2))" "$((y + h / 2))"
}
# One drag leg: press on the first tile, steer through the rest and wait for each one's line; the caller releases.
griddrop_hold() {
    local from="$1" pt cx cy
    shift
    pt=$(griddrop_pt "$from") || fail "griddrop: no point on $from"
    read -r cx cy <<< "$pt"
    cap_drops_wait_abort=cap_drops_abandon
    cap_drops_press "$cx" "$cy"
    while [[ "$#" -ge 2 ]]; do
        pt=$(griddrop_pt "$1") || fail "griddrop: no point on $1"
        read -r cx cy <<< "$pt"
        cap_drops_steer "$cx" "$cy"
        cap_drops_wait_target "$2"
        shift 2
    done
}
# Every listing index the shown Grid draws lit as hovered, as a comma list.
griddrop_hovered() {
    local i total lit=""
    total=$(ipc total)
    for (( i = 0; i < total; i++ )); do
        [[ "$(ipc rowHovered "$i")" == "true" ]] && lit+="${lit:+,}$i"
    done
    printf '%s\n' "$lit"
}
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
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 29:0 >/dev/null 2>&1 || true
    cap_drops_release
    if [[ -n "$griddrop_saved_exit" ]]; then trap -- "$griddrop_saved_exit" EXIT; else trap - EXIT; fi
    wait_message "Copied 1 item · z undoes"
    [[ -f "$dir/f.txt" && -f "$dir/box/f.txt" ]] || fail "griddrop: ctrl at the lift did not copy f.txt"
    printf 'GRIDDROP move=box follow=side empty=refused copy=ctrl\n'
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
