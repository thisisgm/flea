# #259's native case, run only by name on minipc; legs a pin the lift verb in one window (its QDrag.exec loop starves QML of keys, ROOTCAUSE.md), b flip it across two, c pin the frozen offer to a foreign DropArea.
. "$repo/tests/lib/pointer-glide.sh"
cap_dropmods_box() {
    [[ "$#" -eq 1 ]] || fail "cap_dropmods: box needs a qs pid, got $#"
    local geometry
    geometry=$(hyprctl clients -j | jq -er --argjson pid "$1" '
        [.[] | select(.class == "com.thisisgm.flea" and .pid == $pid)]
        | select(length == 1) | .[0] | [.at[0], .at[1], .size[0], .size[1]]
        | select(all(.[]; type == "number" and . == floor)) | @tsv') \
        || fail "cap_dropmods: no native geometry for qs pid $1"
    printf '%s\n' "$geometry"
}
# Press, hold and steer in absolute screen coordinates, so two windows need no shared origin.
cap_dropmods_press_at() {
    [[ "$#" -eq 2 ]] || fail "cap_dropmods: press needs x and y, got $#"
    # Through ydotool, never a compositor move, which sends Qt no pointer frame.
    pointer_glide "$1" "$2" || fail "cap_dropmods: pointer move to $1,$2 failed"
    sleep 0.2
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 || fail "cap_dropmods: pointer press failed"
}
cap_dropmods_steer_to() {
    [[ "$#" -eq 2 ]] || fail "cap_dropmods: steer needs x and y, got $#"
    pointer_glide "$1" "$2" || { cap_dropmods_abandon; fail "cap_dropmods: the drag never reached [$1,$2]"; }
}
cap_dropmods_abandon() {
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || true
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 29:0 42:0 >/dev/null 2>&1 || true
}
# The case's one exit trap: a fail anywhere lets go of the pointer and keys and closes the probe window if it runs.
cap_dropmods_cleanup() {
    ipc_pid=""
    cap_drops_wait_abort=""
    cap_dropmods_abandon
    [[ -z "$cap_dropmods_probe_pid" ]] || kill "$cap_dropmods_probe_pid" 2>/dev/null || true
    cap_dropmods_probe_pid=""
}
cap_dropmods_release() {
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || fail "cap_dropmods: pointer release failed"
    settle
}
# A press that lands on the parked probe starts no drag, so a probe covering the source point fails the case before the press.
cap_dropmods_probe_clear_of() {
    [[ "$#" -eq 2 ]] || fail "cap_dropmods: probe_clear_of needs x and y, got $#"
    local clients found
    clients=$(hyprctl clients -j 2>/dev/null) \
        || fail "cap_dropmods: hyprctl clients was unreadable, so the parked probe could not be located"
    found=$(jq -er '[.[] | select(.title == "DropProbe")] | length' <<< "$clients") \
        || fail "cap_dropmods: hyprctl clients was unreadable, so the parked probe could not be located"
    [[ "$found" == "1" ]] || fail "cap_dropmods: expected one parked DropProbe window, found $found"
    jq -e --argjson x "$1" --argjson y "$2" '[.[] | select(.title == "DropProbe")] | .[0]
        | (.at[0] <= $x and $x < .at[0] + .size[0] and .at[1] <= $y and $y < .at[1] + .size[1]) | not' <<< "$clients" >/dev/null \
        || fail "cap_dropmods: the parked probe covers the source point $1,$2, so the press would miss the row"
}
# ydotool key 29 is left Ctrl and 42 left Shift.
cap_dropmods_key() {
    [[ "$#" -eq 2 ]] || fail "cap_dropmods: key needs a code and 1 or 0, got $#"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key "$1:$2" >/dev/null 2>&1 || fail "cap_dropmods: key $1:$2 refused"
    sleep 0.3
}
cap_dropmods_wait_file() {
    [[ "$#" -eq 1 ]] || fail "cap_dropmods: wait_file needs a path, got $#"
    local end=$((SECONDS + 15))
    while (( SECONDS < end )); do [[ -e "$1" ]] && return 0; sleep 0.1; done
    fail "cap_dropmods: $1 never landed (said $(ipc lastMessage 2>/dev/null); found at $(find "$fixture_root" -maxdepth 3 -name "$(basename "$1")" | tr '\n' ' '))"
}
cap_dropmods_wait_gone() {
    [[ "$#" -eq 1 ]] || fail "cap_dropmods: wait_gone needs a path, got $#"
    local end=$((SECONDS + 15))
    while (( SECONDS < end )); do [[ ! -e "$1" ]] && return 0; sleep 0.1; done
    fail "cap_dropmods: $1 survived the drop"
}
# launch()'s twin that keeps the first window; pids come from flea_pids as window_box's do, never $!, which names the setsid shim.
cap_dropmods_launch_second() {
    [[ "$#" -eq 1 ]] || fail "cap_dropmods: launch_second needs a path, got $#"
    FLEA_UI="$flea_ui" FLEA_BIN="$flea_bin" \
        flea_test_exec "$flea_bin" --gui "$1" >"$flea_log.second" 2>&1 </dev/null &
    local count="" end=$((SECONDS + 20))
    while (( SECONDS < end )); do
        count=$(hyprctl clients -j 2>/dev/null | jq '[.[] | select(.class == "com.thisisgm.flea")] | length' 2>/dev/null || printf "")
        [[ "$count" == "2" ]] && return 0
        sleep 0.2
    done
    fail "cap_dropmods: the second window never arrived (saw $count flea clients)"
}
# Focuses a window and points ipc at it, since the path route reaches only the newest of two windows.
cap_dropmods_focus_pid() {
    [[ "$#" -eq 2 ]] || fail "cap_dropmods: focus needs a pid and a path, got $#"
    local addr seen
    addr=$(window_addr_of_pid "$1") || exit 1
    hypr_window_focus "$addr" >/dev/null || fail "cap_dropmods: focuswindow $addr refused"
    sleep 0.4
    seen=$(focused_pid)
    [[ "$seen" == "$1" ]] || fail "cap_dropmods: focus did not land on qs pid $1 (the active window is $seen)"
    ipc_pid="$1"
    seen=$(ipc path 2>/dev/null || printf unavailable)
    [[ "$seen" == "$2" ]] || fail "cap_dropmods: qs pid $1 does not show $2 (ipc says $seen)"
}
case_cap_dropmods() {
    local dir="$fixture_root/cap-dropmods" dir2="$fixture_root/cap-dropmods-two"
    local src_pt dst_pt cx cy tx ty wx wy ww wh seen win1 win2 probe_log probe_box
    cap_dropmods_probe_pid=""
    trap cap_dropmods_cleanup EXIT
    cap_drops_wait_abort=cap_dropmods_abandon
    sandbox_scratch "$dir"
    sandbox_scratch "$dir2"
    mkdir -p "$dir/sub" "$dir2/sub2"
    printf 'a one\n' > "$dir/a1.txt"
    printf 'a two\n' > "$dir/a2.txt"
    printf 'b one\n' > "$dir/b1.txt"
    printf 'b two\n' > "$dir/b2.txt"
    printf 'pad\n' > "$dir2/pad.txt"
    printf 'probe me\n' > "$dir/probe1.txt"
    printf 'probe me too\n' > "$dir/probe2.txt"
    seed_ui_state "$fixture_root/cap-dropmods-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 7
    seek_row_named "a1.txt" || fail "cap_dropmods: a1.txt never came under the cursor"
    src_pt=$(ipc rowCentre "$(row_index_of a1.txt)"); read -r cx cy <<< "$src_pt"
    dst_pt=$(ipc rowCentre "$(row_index_of sub)"); read -r tx ty <<< "$dst_pt"
    cap_drops_press "$cx" "$cy"
    cap_drops_steer "$tx" "$ty"
    cap_drops_wait_target "Move 1 item to sub · ctrl copies, shift moves"
    cap_dropmods_key 29 1
    cap_drops_release
    cap_dropmods_key 29 0
    wait_message "Moved 1 item · z undoes"
    cap_dropmods_wait_gone "$dir/a1.txt"
    cap_dropmods_wait_file "$dir/sub/a1.txt"
    printf 'CAP_DROPMODS a1: same-window ctrl at drop keeps the lift move\n'
    wait_listing 6
    seek_row_named "a2.txt" || fail "cap_dropmods: a2.txt never came under the cursor"
    src_pt=$(ipc rowCentre "$(row_index_of a2.txt)"); read -r cx cy <<< "$src_pt"
    dst_pt=$(ipc rowCentre "$(row_index_of sub)"); read -r tx ty <<< "$dst_pt"
    cap_dropmods_key 29 1
    cap_drops_press "$cx" "$cy"
    cap_drops_steer "$tx" "$ty"
    cap_drops_wait_target "Copy 1 item to sub"
    cap_dropmods_key 29 0
    cap_dropmods_key 42 1
    cap_drops_release
    cap_dropmods_key 42 0
    wait_message "Copied 1 item · z undoes"
    cap_dropmods_wait_file "$dir/a2.txt"
    cap_dropmods_wait_file "$dir/sub/a2.txt"
    rm -f "$dir/sub/a2.txt"
    printf 'CAP_DROPMODS a2: same-window shift at drop keeps the ctrl-lift copy\n'
    wait_listing 6
    win1=$(flea_pid)
    cap_dropmods_launch_second "$dir2"
    win2=$(flea_pids | grep -vx "$win1" | head -n 1)
    [[ -n "$win2" ]] || fail "cap_dropmods: the second window has no surviving qs pid"
    cap_dropmods_focus_pid "$win2" "$dir2"
    wait_listing 2
    dst_pt=$(ipc rowCentre "$(row_index_of sub2)"); read -r tx ty <<< "$dst_pt"
    read -r wx wy ww wh < <(cap_dropmods_box "$win2") || exit 1
    tx=$((tx + wx)); ty=$((ty + wy))
    cap_dropmods_focus_pid "$win1" "$dir"
    wait_listing 6
    seek_row_named "b1.txt" || fail "cap_dropmods: b1.txt never came under the cursor"
    src_pt=$(ipc rowCentre "$(row_index_of b1.txt)"); read -r cx cy <<< "$src_pt"
    read -r wx wy ww wh < <(cap_dropmods_box "$win1") || exit 1
    cap_dropmods_press_at "$((cx + wx))" "$((cy + wy))"
    cap_dropmods_steer_to "$tx" "$ty"
    sleep 0.5
    seen=$(focused_pid)
    [[ "$seen" == "$win2" ]] || { cap_dropmods_abandon; fail "cap_dropmods: focus never followed the drag to qs pid $win2 (the active window is $seen), so no mid-drag key could reach it"; }
    # Win2 must show the hover line before Ctrl, or the release can land on its listing floor.
    ipc_pid="$win2"; cap_drops_wait_target "Move 1 item to sub2 · ctrl copies, shift moves"; ipc_pid="$win1"
    cap_dropmods_key 29 1
    cap_dropmods_release
    cap_dropmods_key 29 0
    cap_dropmods_focus_pid "$win2" "$dir2"
    wait_message "Copied 1 item · z undoes"
    cap_dropmods_wait_file "$dir/b1.txt"
    cap_dropmods_wait_file "$dir2/sub2/b1.txt"
    printf 'CAP_DROPMODS b1: cross-window ctrl at drop copies a plain lift\n'
    rm -f "$dir2/sub2/b1.txt"
    cap_dropmods_focus_pid "$win1" "$dir"
    seek_row_named "b2.txt" || fail "cap_dropmods: b2.txt never came under the cursor"
    src_pt=$(ipc rowCentre "$(row_index_of b2.txt)"); read -r cx cy <<< "$src_pt"
    read -r wx wy ww wh < <(cap_dropmods_box "$win1") || exit 1
    cx=$((cx + wx)); cy=$((cy + wy))
    cap_dropmods_focus_pid "$win2" "$dir2"
    dst_pt=$(ipc rowCentre "$(row_index_of sub2)"); read -r tx ty <<< "$dst_pt"
    read -r wx wy ww wh < <(cap_dropmods_box "$win2") || exit 1
    tx=$((tx + wx)); ty=$((ty + wy))
    cap_dropmods_focus_pid "$win1" "$dir"
    cap_dropmods_key 29 1
    cap_dropmods_press_at "$cx" "$cy"
    cap_dropmods_steer_to "$tx" "$ty"
    sleep 0.5
    seen=$(focused_pid)
    [[ "$seen" == "$win2" ]] || { cap_dropmods_abandon; fail "cap_dropmods: focus never followed the drag to qs pid $win2 (the active window is $seen), so no mid-drag key could reach it"; }
    ipc_pid="$win2"; cap_drops_wait_target "Copy 1 item to sub2"; ipc_pid="$win1"
    cap_dropmods_key 29 0
    cap_dropmods_key 42 1
    cap_dropmods_release
    cap_dropmods_key 42 0
    cap_dropmods_focus_pid "$win2" "$dir2"
    wait_message "Moved 1 item · z undoes"
    cap_dropmods_wait_file "$dir2/sub2/b2.txt"
    cap_dropmods_wait_gone "$dir/b2.txt"
    printf 'CAP_DROPMODS b2: cross-window shift at drop moves a ctrl lift\n'
    kill_flea
    ipc_pid=""
    launch "$dir"
    wait_listing 5
    probe_log="$run_root/dropprobe.log"
    QT_FORCE_STDERR_LOGGING=1 qml6 "$repo/tests/dropmods-probe.qml" >"$probe_log" 2>&1 &
    cap_dropmods_probe_pid=$!
    local probe_end=$((SECONDS + 15)) probe_addr=""
    while (( SECONDS < probe_end )); do
        probe_addr=$(hyprctl clients -j 2>/dev/null | jq -er '[.[] | select(.title == "DropProbe")] | select(length == 1) | .[0].address' 2>/dev/null || printf "")
        [[ -n "$probe_addr" ]] && break
        sleep 0.2
    done
    [[ -n "$probe_addr" ]] || fail "cap_dropmods: the probe window never arrived"
    hypr_window_focus "$probe_addr" >/dev/null || fail "cap_dropmods: the probe window would not focus"
    sleep 0.3
    hypr_window_float "$probe_addr" on >/dev/null || fail "cap_dropmods: the probe window would not float"
    sleep 0.3
    # A floated window keeps its tiled width, which parks its centre past the screen edge; give it its own size first.
    hypr_window_resize_absolute "$probe_addr" 420 300 >/dev/null || fail "cap_dropmods: the probe window would not take its size"
    sleep 0.3
    read -r wx wy ww wh <<< "$(window_box)"
    hypr_window_move_absolute "$probe_addr" "$((wx + ww - 480))" "$((wy + wh - 360))" >/dev/null \
        || fail "cap_dropmods: the probe window would not park"
    sleep 0.5
    probe_box=$(hyprctl clients -j 2>/dev/null | jq -er '[.[] | select(.title == "DropProbe")] | select(length == 1) | .[0] | [.at[0] + .size[0] / 2, .at[1] + .size[1] / 2] | select(all(.[]; . == floor)) | @tsv' 2>/dev/null || printf "")
    [[ -n "$probe_box" ]] || fail "cap_dropmods: the probe window has no geometry to aim at"
    read -r tx ty <<< "$probe_box"
    (( tx > wx && tx < wx + ww && ty > wy && ty < wy + wh )) \
        || fail "cap_dropmods: the probe centre $tx,$ty lies outside the Flea window $wx,$wy,$ww,$wh, so the pointer cannot reach it"
    cap_dropmods_focus_pid "$(flea_pid)" "$dir"
    seek_row_named "probe1.txt" || fail "cap_dropmods: probe1.txt never came under the cursor"
    src_pt=$(ipc rowCentre "$(row_index_of probe1.txt)"); read -r cx cy <<< "$src_pt"
    read -r wx wy ww wh <<< "$(window_box)"
    cap_dropmods_probe_clear_of "$((cx + wx))" "$((cy + wy))"
    cap_dropmods_press_at "$((cx + wx))" "$((cy + wy))"
    cap_dropmods_steer_to "$tx" "$ty"
    printf 'CAP_DROPMODS c1 aim %s,%s cursor %s windows %s\n' "$tx" "$ty" "$(hyprctl cursorpos)" "$(hyprctl clients -j | jq -c '[.[] | select(.title == "DropProbe" or .class == "com.thisisgm.flea") | [.title, .floating, .at, .size]]')"
    cap_drops_wait_target "Move 1 item to a folder · ctrl copies, shift moves"
    sleep 0.5
    cap_dropmods_release
    local offered1="" offered2=""
    probe_end=$((SECONDS + 10))
    while (( SECONDS < probe_end )); do
        offered1=$(grep -a "PROBE drop#1" "$probe_log" 2>/dev/null | sed -n 's/.*offered=\([0-9]*\) proposed=\([0-9]*\).*/\1 \2/p' | head -n 1 || printf "")
        [[ -n "$offered1" ]] && break
        sleep 0.2
    done
    [[ -n "$offered1" ]] || fail "cap_dropmods: the probe never logged the plain drop (log: $(head -c 500 "$probe_log" 2>/dev/null))"
    grep -aq "PROBE drop#1 .*formats=.*text/uri-list" "$probe_log" || fail "cap_dropmods: the plain drop carried no uri-list"
    grep -aq "PROBE drop#1 .*formats=.*application/x-flea-rows" "$probe_log" || fail "cap_dropmods: the plain drop carried no Flea marker"
    grep -aq "PROBE drop#1 chose=copy" "$probe_log" || fail "cap_dropmods: the plain drop chose no copy"
    printf 'CAP_DROPMODS c1: foreign drop offers %s\n' "$offered1"
    # Focus followed the drag onto the probe and stays there, so key() would refuse the seek's keys.
    cap_dropmods_focus_pid "$(flea_pid)" "$dir"
    seek_row_named "probe2.txt" || fail "cap_dropmods: probe2.txt never came under the cursor"
    src_pt=$(ipc rowCentre "$(row_index_of probe2.txt)"); read -r cx cy <<< "$src_pt"
    read -r wx wy ww wh <<< "$(window_box)"
    cap_dropmods_probe_clear_of "$((cx + wx))" "$((cy + wy))"
    cap_dropmods_press_at "$((cx + wx))" "$((cy + wy))"
    cap_dropmods_steer_to "$tx" "$ty"
    cap_drops_wait_target "Move 1 item to a folder · ctrl copies, shift moves"
    sleep 0.5
    cap_dropmods_key 29 1
    cap_dropmods_release
    cap_dropmods_key 29 0
    probe_end=$((SECONDS + 10))
    while (( SECONDS < probe_end )); do
        offered2=$(grep -a "PROBE drop#2" "$probe_log" 2>/dev/null | sed -n 's/.*offered=\([0-9]*\) proposed=\([0-9]*\).*/\1 \2/p' | head -n 1 || printf "")
        [[ -n "$offered2" ]] && break
        sleep 0.2
    done
    [[ -n "$offered2" ]] || fail "cap_dropmods: the probe never logged the ctrl drop"
    printf 'CAP_DROPMODS c2: ctrl at drop offers %s\n' "$offered2"
    [[ "$offered1" == "1 1" ]] || fail "cap_dropmods: the plain lift offered $offered1, not the copy singleton 1 1"
    [[ "$offered2" == "$offered1" ]] || fail "cap_dropmods: the ctrl drop changed the platform offer to $offered2, so the freeze the root cause states did not hold"
    grep -aq "PROBE drop#2 chose=copy" "$probe_log" || fail "cap_dropmods: the ctrl drop chose no copy"
    printf 'CAP_DROPMODS settings=off a1=lift-move a2=lift-copy b1=drop-copy b2=drop-move c=frozen-offer\n'
    cap_dropmods_cleanup
    trap - EXIT
    kill_flea
}
