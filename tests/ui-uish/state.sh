# GPUI overrides for UISH-06; seeded from the unchanged oracle at 2faf8d099.
case_xwsettings() {
    local root="$fixture_root/xwsettings" dir config state
    dir="$root/files"
    config="$root/config"
    state="$root/state"
    sandbox_scratch "$root"
    [[ -f "$fixture_root/.flea-test-sandbox" ]] \
        || fail "xwsettings: the sandbox root carries no marker, so nothing here is deletable"
    mkdir -p "$dir/sub" "$config" "$state" || fail "xwsettings: the sandbox could not be made"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/.dot"
    : > "$dir/sub/c.txt"
    local real_config="${XDG_CONFIG_HOME-}" real_state="${XDG_STATE_HOME-}"
    export XDG_CONFIG_HOME="$config"
    export XDG_STATE_HOME="$state"
    env XDG_STATE_HOME="$state" "$flea_bin" --ui-state \
        '{"hidden":false,"density":"compact","view":"list"}' >/dev/null \
        || fail "xwsettings: the seeding write failed"
    printf 'XWSETTINGS setup=ok\n'

    # Window A through the owned transport, so assert_window and every read name it by its owned PID.
    launch_window "$dir"
    local addrA addrB pidA pidB
    pidA=$ipc_pid
    addrA=$(omarchy-drive windows --json | jq -r '[.windows[] | select(.title == "Flea")] | .[0].address // empty')
    [[ -n "$addrA" ]] || fail "xwsettings: no Flea window after launching A"
    [[ "$(hyprctl clients -j | jq -r --arg a "$addrA" '.[] | select(.address == $a) | .pid')" == "$pidA" ]] \
        || fail "xwsettings: the window at $addrA does not belong to owned pid $pidA"
    omarchy-drive focus "$addrA" >/dev/null
    assert_window
    [[ "$pidA" =~ ^[1-9][0-9]*$ ]] || fail "xwsettings: A has no owned qs pid"
    local -a ipcA=(timeout --kill-after="$ipc_call_kill_after" "$ipc_call_timeout" "$FLEA_IPC_HELPER" --pid "$pidA" call flea)
    wait_listing 3
    [[ "$("${ipcA[@]}" path)" == "$dir" ]] || fail "xwsettings: A opened $("${ipcA[@]}" path), not $dir"

    # Window B is a second owned launch over the same state file; flea-gui needs no second ui tree.
    local b_log="$run_root/flea-xwsettings-b.log" before pid
    before=$(flea_pids | tr '\n' ' ')
    flea_test_launch "$dir" "$b_log" --ui "$flea_ui" --binary "$flea_bin" \
        || fail "xwsettings: the second owned window launch failed"
    local attempt count
    for (( attempt = 0; attempt < 150; attempt++ )); do
        count=$(omarchy-drive windows --json | jq '[.windows[] | select(.title == "Flea")] | length')
        [[ "$count" == 2 ]] && break
        sleep 0.1
    done
    [[ "$count" == 2 ]] || fail "xwsettings: the second window never arrived, got $count"
    addrB=$(omarchy-drive windows --json \
        | jq -r --arg a "$addrA" '[.windows[] | select(.title == "Flea" and .address != $a)] | .[0].address // empty')
    [[ -n "$addrB" ]] || fail "xwsettings: the second window has no address of its own"
    pidB=""
    for pid in $(flea_pids); do
        [[ " $before " == *" $pid "* ]] || pidB=$pid
    done
    [[ -n "$pidB" ]] || fail "xwsettings: B has no owned qs pid"
    flea_process_owned "$pidB" || fail "xwsettings: B has no owned qs pid"
    [[ "$pidB" != "$pidA" ]] || fail "xwsettings: A and B share pid $pidA"
    [[ "$(hyprctl clients -j | jq -r --arg a "$addrB" '.[] | select(.address == $a) | .pid')" == "$pidB" ]] \
        || fail "xwsettings: the second window at $addrB does not belong to owned pid $pidB"
    local -a ipcB=(timeout --kill-after="$ipc_call_kill_after" "$ipc_call_timeout" "$FLEA_IPC_HELPER" --pid "$pidB" call flea)
    omarchy-drive focus "$addrB" >/dev/null
    for (( attempt = 0; attempt < 150; attempt++ )); do
        [[ "$("${ipcB[@]}" path 2>/dev/null)" == "$dir" ]] && break
        sleep 0.1
    done
    [[ "$("${ipcB[@]}" path)" == "$dir" ]] || fail "xwsettings: B never listed $dir"

    # Both windows list $dir, so only a field that must differ tells the routes apart: A listed before B launched.
    local firstA firstB
    firstA=$("${ipcA[@]}" firstRowsAt)
    firstB=$("${ipcB[@]}" firstRowsAt)
    [[ "$firstA" =~ ^[0-9]+$ && "$firstB" =~ ^[0-9]+$ ]] \
        || fail "xwsettings: no first-rows stamp over the pid routes, A (pid $pidA) gave '$firstA' and B (pid $pidB) gave '$firstB'"
    (( firstB > firstA )) \
        || fail "xwsettings: the pid routes reach one instance, A (pid $pidA) and B (pid $pidB) both answered first-rows $firstA and $firstB"

    # Focus once, then poll only the active address before injecting the routing proof.
    local active="" focus_wait_tries=50 focus_poll_s=0.1
    omarchy-drive focus "$addrA" >/dev/null
    for (( attempt = 0; attempt < focus_wait_tries; attempt++ )); do
        active=$(hyprctl activewindow -j 2>/dev/null | jq -r 'select(.address != null) | .address')
        [[ "$active" == "$addrA" ]] && break
        sleep "$focus_poll_s"
    done
    if [[ "$active" != "$addrA" ]]; then
        xwsettings_route_snapshot
        fail "xwsettings: A never became the active window, active is $active"
    fi
    omarchy-drive key --window "$addrA" j >/dev/null
    settle
    if [[ "$("${ipcA[@]}" cursor)" != "1" ]]; then
        xwsettings_route_snapshot
        fail "xwsettings: A's cursor did not step, it is $("${ipcA[@]}" cursor)"
    fi
    [[ "$("${ipcB[@]}" cursor)" == "0" ]] || fail "xwsettings: the step addressed to A moved B to $("${ipcB[@]}" cursor)"
    omarchy-drive key --window "$addrA" k >/dev/null
    settle
    printf 'XWSETTINGS route=ok\n'

    # Views stay their own and non-vacuous: A in list, B in grid, so the shared hidden change below
    # proves each kept its view rather than two list windows agreeing by doing nothing.
    omarchy-drive focus "$addrB" >/dev/null
    omarchy-drive key --window "$addrB" -M ctrl -k 3 -m ctrl >/dev/null
    settle
    [[ "$("${ipcB[@]}" viewMode)" == "grid" ]] || fail "xwsettings: B did not switch to grid, it is $("${ipcB[@]}" viewMode)"
    [[ "$("${ipcA[@]}" viewMode)" == "list" ]] || fail "xwsettings: A did not stay in list, it is $("${ipcA[@]}" viewMode)"

    # Hidden files, toggled in A and applied in B: the '.' and Ctrl+> keys and the menu row all
    # write the one global hidden preference, so one keypress stands in for all three entrances.
    # The fixture holds sub, a.txt, b.txt and .dot, so 3 rows without dotfiles and 4 with.
    omarchy-drive focus "$addrA" >/dev/null
    local hid_start hid_ms hid_total=0
    hid_start=$(date +%s%3N)
    omarchy-drive key --window "$addrA" . >/dev/null
    for (( attempt = 0; attempt < 20; attempt++ )); do
        hid_total=$("${ipcB[@]}" total 2>/dev/null || printf 0)
        [[ "$hid_total" == 4 ]] && break
        sleep 0.05
    done
    hid_ms=$(( $(date +%s%3N) - hid_start ))
    [[ "$hid_total" == 4 ]] || fail "xwsettings: B never showed the dotfiles, total is $hid_total"
    printf 'XWSETTINGS hidden=ok elapsed_ms=%s (information only, no bound)\n' "$hid_ms"
    [[ "$("${ipcB[@]}" showHidden)" == "true" ]] || fail "xwsettings: B lists four rows but reports showHidden $("${ipcB[@]}" showHidden)"
    [[ "$("${ipcA[@]}" showHidden)" == "true" ]] || fail "xwsettings: A's own toggle did not take"
    [[ "$("${ipcA[@]}" viewMode)" == "list" ]] || fail "xwsettings: the shared hidden change moved A to $("${ipcA[@]}" viewMode)"
    [[ "$("${ipcB[@]}" viewMode)" == "grid" ]] || fail "xwsettings: the shared hidden change moved B to $("${ipcB[@]}" viewMode)"

    # Density, stepped in B's own Settings panel and applied in A: compact is the default, so one
    # step right lands on Normal, the same move settings_view drives on one window.
    omarchy-drive focus "$addrB" >/dev/null
    local beforeA afterA sections down target cursor count step
    beforeA=$("${ipcA[@]}" fileRowHeight)
    omarchy-drive key --window "$addrB" , >/dev/null
    settle
    sections=$("${ipcB[@]}" settingsSections)
    down=$(printf '%s' "$sections" | jq -r 'map(.id) | index("view") // empty')
    count=$(printf '%s' "$sections" | jq 'length')
    [[ "$down" =~ ^[0-9]+$ ]] || fail "xwsettings: B's rail has no view section, it carries $sections"
    [[ "$("${ipcB[@]}" settingsSide)" == "rail" ]] \
        || { omarchy-drive key --window "$addrB" -k Tab >/dev/null; settle; }
    for (( step = 1; step < count; step++ )); do omarchy-drive key --window "$addrB" -k k >/dev/null; done
    settle
    [[ "$("${ipcB[@]}" settingsSection)" == "view" ]] || fail "xwsettings: B's rail did not reach View"
    for (( step = 0; step < down; step++ )); do omarchy-drive key --window "$addrB" -k j >/dev/null; settle; done
    target=$("${ipcB[@]}" settingsModel | jq -r 'map(.id) | index("density") // empty')
    count=$("${ipcB[@]}" settingsModel | jq 'length')
    [[ "$target" =~ ^[0-9]+$ ]] || fail "xwsettings: B's View has no density control"
    [[ "$("${ipcB[@]}" settingsSide)" == "pane" ]] \
        || { omarchy-drive key --window "$addrB" -k Tab >/dev/null; settle; }
    for (( attempt = 0; attempt <= count; attempt++ )); do
        cursor=$("${ipcB[@]}" settingsCursor)
        [[ "$cursor" == "$target" ]] && break
        if (( cursor < target )); then omarchy-drive key --window "$addrB" j >/dev/null
        else omarchy-drive key --window "$addrB" -k k >/dev/null; fi
        settle
    done
    [[ "$cursor" == "$target" ]] || fail "xwsettings: B's cursor never reached density, it is $cursor"
    omarchy-drive key --window "$addrB" l >/dev/null
    for (( attempt = 0; attempt < 30; attempt++ )); do
        if "${ipcB[@]}" uiSettings 2>/dev/null | jq -e '.density == "normal"' >/dev/null \
            && jq -e '.density == "normal"' "$state/flea/ui.json" >/dev/null; then break; fi
        sleep 0.1
    done
    "${ipcB[@]}" uiSettings | jq -e '.density == "normal"' >/dev/null \
        || fail "xwsettings: B's density step never landed"
    afterA=$("${ipcA[@]}" fileRowHeight)
    [[ "$afterA" != "$beforeA" ]] || fail "xwsettings: A's rows never changed height, still $afterA"
    [[ "$afterA" == "$("${ipcB[@]}" fileRowHeight)" ]] \
        || fail "xwsettings: A draws $afterA while B draws $("${ipcB[@]}" fileRowHeight)"
    omarchy-drive key --window "$addrB" -k Escape >/dev/null
    settle
    printf 'XWSETTINGS density=ok\n'

    # Per-window state stays put: B navigates into sub while A keeps its folder and its view.
    omarchy-drive focus "$addrB" >/dev/null
    omarchy-drive key --window "$addrB" -k Return >/dev/null
    for (( attempt = 0; attempt < 100; attempt++ )); do
        [[ "$("${ipcB[@]}" path 2>/dev/null)" == "$dir/sub" ]] && break
        sleep 0.1
    done
    [[ "$("${ipcB[@]}" path)" == "$dir/sub" ]] || fail "xwsettings: B never opened sub, it is at $("${ipcB[@]}" path)"
    [[ "$("${ipcA[@]}" path)" == "$dir" ]] || fail "xwsettings: A left $dir for $("${ipcA[@]}" path)"
    [[ "$("${ipcA[@]}" viewMode)" == "list" ]] || fail "xwsettings: A's view moved to $("${ipcA[@]}" viewMode)"
    [[ "$("${ipcB[@]}" viewMode)" == "grid" ]] || fail "xwsettings: B's view moved to $("${ipcB[@]}" viewMode)"
    [[ "$("${ipcA[@]}" total)" == "4" ]] || fail "xwsettings: A lost the applied toggle, total is $("${ipcA[@]}" total)"
    printf 'XWSETTINGS pinned=ok\n'

    if [[ -n "$real_config" ]]; then export XDG_CONFIG_HOME="$real_config"; else unset XDG_CONFIG_HOME; fi
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    cat "$b_log" >> "$run_log" 2>/dev/null || true
    kill_flea
}

# Find the second owned GPUI launch by its new pid, as case_xwsettings does.
xw_second_window_gpui() {
    local start_path="$1" before pid fresh="" attempt
    before=" $(flea_pids | tr '\n' ' ') "
    flea_test_launch "$start_path" "$run_root/flea.A.log" --ui "$flea_ui" --binary "$flea_bin" \
        || fail "xwwatch: the second owned window launch failed"
    for (( attempt = 0; attempt < xw_window_poll_tries; attempt++ )); do
        for pid in $(flea_pids); do
            [[ "$before" == *" $pid "* ]] || fresh=$pid
        done
        [[ -n "$fresh" ]] && break
        sleep "$xw_ui_poll_s"
    done
    [[ -n "$fresh" ]] || fail "xwwatch: the second window's process never appeared"
    xw_addr_for_pid "$fresh"
}

# GPUI's New File card owns a text input; menuDialogState observes only Open With and Run cards.
xw_wait_new_file_gpui() {
    local pid="$1" want="$2" step="$3" file="${4-}" n state rename
    for ((n = 0; n < xw_ui_poll_tries; n++)); do
        state=$(xw_ipc "$pid" keyDeliveryState 2>/dev/null || true)
        if [[ "$want" == open ]]; then
            rename=$(xw_ipc "$pid" renameState 2>/dev/null || true)
            if [[ "$(xw_ipc "$pid" contextMenuVisible)" == false ]] \
                && jq -e '.index == -1' <<< "$rename" >/dev/null \
                && jq -e '.activeFocusItem == "QQuickTextInput" and .searchMode == "" and .filterTyping == false' <<< "$state" >/dev/null; then
                return 0
            fi
        elif [[ -f "$file" ]] && jq -e '.listFocus == true' <<< "$state" >/dev/null; then
            return 0
        fi
        sleep "$xw_ui_poll_s"
    done
    xw_editor_diagnostics "$pid" "$step" "${rename-}" "$state"
    fail "xwwatch: $step: New File $want state was not reached"
}

case_xwwatch() {
    local dir="$fixture_root/xwwatch" ui_copy="$fixture_root/xwwatch-ui" addrB addrA row pidA pidB cleanup_command entries marksA marksB
    printf -v cleanup_command 'xw_cleanup %q || exit 1' "$ui_copy"
    trap "$cleanup_command" EXIT
    trap 'exit 1' HUP INT TERM
    xw_sweep_stale
    sandbox_scratch "$dir"
    sandbox_scratch "$ui_copy"
    printf 'one\n' > "$dir/sel-one.txt"
    printf 'two\n' > "$dir/sel-two.txt"
    printf 'three\n' > "$dir/sel-three.txt"
    printf 'four\n' > "$dir/renamed-later.txt"
    printf 'five\n' > "$dir/untouched.txt"
    launch "$dir"
    wait_listing 5
    pidB=$(flea_pid)
    addrB=$(xw_addr_for_pid "$pidB") || fail "xwwatch: no B window address"
    [[ -n "$addrB" ]] || fail "xwwatch: no B window address"

    # B selects three files and parks its cursor on the last of them.
    seek_row_named sel-one.txt
    key v >/dev/null
    key j >/dev/null
    key v >/dev/null
    key j >/dev/null
    key v >/dev/null
    settle
    [[ "$(xw_ipc "$pidB" selectionCount)" == "3" ]] || fail "xwwatch: B selected $(xw_ipc "$pidB" selectionCount), not 3"
    [[ "$(xw_ipc "$pidB" rowAt "$(xw_ipc "$pidB" cursor)")" == sel-two.txt\|* ]] \
        || fail "xwwatch: B cursor is on $(xw_ipc "$pidB" rowAt "$(xw_ipc "$pidB" cursor)"), not sel-two.txt"

    # A opens beside it on the same folder.
    addrA=$(xw_second_window_gpui "$dir") || fail "xwwatch: no A window address"
    [[ -n "$addrA" ]] || fail "xwwatch: no A window address"
    pidA=$(hyprctl clients -j | jq -er --arg addr "$addrA" \
        '[.[] | select(.address == $addr)] | select(length == 1) | .[0].pid
         | select(type == "number" and . == floor and . > 0)') \
        || fail "xwwatch: no single A pid at $addrA"
    flea_process_owned "$pidA" || fail "xwwatch: A pid $pidA is not owned by this run"
    [[ "$pidA" != "$pidB" ]] || fail "xwwatch: A and B share pid $pidA"
    xw_wait_total "$pidA" 5 "second window listing"
    xw_settled "$pidA"

    # A has no marks; B holds three. Equal answers expose routes reaching the same instance.
    marksA=$(xw_ipc "$pidA" selectionCount) || fail "xwwatch: A IPC route pid $pidA failed"
    marksB=$(xw_ipc "$pidB" selectionCount) || fail "xwwatch: B IPC route pid $pidB failed"
    [[ "$marksA" != "$marksB" ]] \
        || fail "xwwatch: A IPC route pid $pidA and B IPC route pid $pidB both answered $marksA marks"
    [[ "$marksA" == 0 ]] || fail "xwwatch: A IPC route pid $pidA answered $marksA marks, expected 0"
    [[ "$marksB" == 3 ]] || fail "xwwatch: B IPC route pid $pidB answered $marksB marks, expected 3"

    # A creates a file through its own New File row; B shows it with the same three marked.
    xw_click_background "$addrA" "$pidA"
    sleep "$settle_s"
    entries=$(xw_ipc "$pidA" contextMenuEntries)
    [[ "$entries" == 'New Folder|New File'* ]] || fail "xwwatch: A did not open its background menu: $entries"
    xw_menu_seek "$addrA" "$pidA" "New File"
    xw_key "$addrA" -k Return
    xw_wait_new_file_gpui "$pidA" open "New File after menu Return"
    xw_key "$addrA" -M ctrl -k a -m ctrl -k BackSpace
    xw_key "$addrA" created-by-a.txt
    xw_key "$addrA" -k Return
    xw_wait_new_file_gpui "$pidA" closed "New File after submit" "$dir/created-by-a.txt"
    xw_wait_total "$pidA" 6 "create in A"
    xw_wait_total "$pidB" 6 "create"
    xw_settled "$pidA"
    xw_settled "$pidB"
    xw_wait_row "$pidA" created-by-a.txt "create row in A"
    xw_wait_row "$pidB" created-by-a.txt "create row in B"
    [[ "$(xw_ipc "$pidB" selectionCount)" == "3" ]] || fail "xwwatch: B lost a mark after create"
    marksB=$(xw_ipc "$pidB" selectedIndices) || fail "xwwatch: B marks unreadable after create"
    local marked_names="" marked_row
    for row in ${marksB//,/ }; do
        marked_row=$(xw_ipc "$pidB" rowAt "$row") || fail "xwwatch: B marked row $row unreadable after create"
        marked_names+="${marked_row%%|*}"$'\n'
    done
    [[ "$(printf '%s' "$marked_names" | LC_ALL=C sort)" == $'sel-one.txt\nsel-three.txt\nsel-two.txt' ]] \
        || fail "xwwatch: B marks after create name $marked_names instead of the same three files"

    # A renames a file B never marked; B follows the name with its marks untouched.
    xw_goto "$addrA" "$pidA" 1
    [[ "$(xw_ipc "$pidA" rowAt "$(xw_ipc "$pidA" cursor)")" == renamed-later.txt\|* ]] \
        || fail "xwwatch: A cursor is on $(xw_ipc "$pidA" rowAt "$(xw_ipc "$pidA" cursor)"), not renamed-later.txt"
    xw_key "$addrA" -k F2
    xw_wait_editor "$pidA" "rename after F2"
    xw_key "$addrA" -M ctrl -k a -m ctrl -k BackSpace
    xw_key "$addrA" renamed-by-a.txt
    xw_key "$addrA" -k Return
    xw_wait_row "$pidB" renamed-by-a.txt "rename"
    xw_settled "$pidB"

    # A deletes a file B holds marked; B drops that mark and keeps the other two.
    row=0
    local found=-1
    for ((row = 0; row < 6; row++)); do
        if [[ "$(xw_ipc "$pidA" rowAt "$row")" == sel-one.txt\|* ]]; then found=$row; break; fi
    done
    [[ "$found" -ge 0 ]] || fail "xwwatch: A never listed sel-one.txt"
    xw_goto "$addrA" "$pidA" "$found"
    xw_key "$addrA" -k Delete
    xw_wait_total "$pidB" 5 "delete"
    xw_settled "$pidB"
    [[ "$(xw_ipc "$pidB" selectionCount)" == "2" ]] \
        || fail "xwwatch: B holds $(xw_ipc "$pidB" selectionCount) marks, not the 2 survivors"
    [[ "$(xw_ipc "$pidB" rowAt "$(xw_ipc "$pidB" cursor)")" == sel-two.txt\|* ]] \
        || fail "xwwatch: B cursor is on $(xw_ipc "$pidB" rowAt "$(xw_ipc "$pidB" cursor)"), not sel-two.txt"
    local survivor_rows
    survivor_rows=$(xw_ipc "$pidB" selectedIndices)
    [[ -n "$survivor_rows" ]] || fail "xwwatch: B holds no readable marks"
    for row in ${survivor_rows//,/ }; do
        case "$(xw_ipc "$pidB" rowAt "$row")" in
            sel-two.txt\|* | sel-three.txt\|*) ;;
            *) fail "xwwatch: B mark on row $row is $(xw_ipc "$pidB" rowAt "$row"), not a survivor" ;;
        esac
    done
    printf 'XWWATCH survivors ok\n'
    xw_cleanup "$ui_copy" || fail "xwwatch: owned windows or backends did not drain"
    trap - EXIT HUP INT TERM
}

case_makedefault() {
    local dir="$fixture_root/makedefault" box="$fixture_root/makedefault-box" real_bin="$flea_bin"
    sandbox_scratch "$dir"
    sandbox_scratch "$box"
    : > "$dir/a.txt"
    mkdir -p "$box/bin" "$box/data/applications"
    local queries="$box/queries.log" handler="$box/handler" mode="$box/mode" restart="$box/restart" refused="$box/refused.log" runs
    local no_key='type == "object" and ([.. | objects | has("makeDefault")] | any | not)'
    makedefault_log="$box/ran.log"
    makedefault_restart_log="$box/systemctl.log"
    makedefault_events_log="$box/events.log"
    : > "$makedefault_log"
    : > "$queries"
    : > "$makedefault_restart_log"
    : > "$makedefault_events_log"
    : > "$refused"
    printf 'org.gnome.Nautilus.desktop\n' > "$handler"
    printf 'ok\n' > "$mode"
    printf 'ok\n' > "$restart"
    makedefault_from_source "$box"
    # The row looks for the packaged entry on the XDG data ladder before it offers a claim, as flea --default does.
    printf '[Desktop Entry]\nType=Application\nName=Flea\nExec=flea %%U\n' > "$box/data/applications/com.thisisgm.flea.desktop"

    # The two --default shapes are answered, other argv of a desktop-writing mode is refused, and the rest runs the real binary.
    cat > "$box/bin/flea" <<'STUB'
#!/bin/sh
# Sample input: flea --default off
box="$FLEA_MAKEDEFAULT_BOX"
if [ "$#" -eq 1 ] && [ "$1" = --default ]; then
    shape=claim
elif [ "$#" -eq 2 ] && [ "$1" = --default ] && [ "$2" = off ]; then
    shape=release
elif [ "$#" -gt 0 ] && grep -q -x -F -e "$1" "$box/guarded"; then
    printf 'REFUSED flea %s\n' "$*" >> "$box/refused.log"
    exit 64
else
    exec "$FLEA_MAKEDEFAULT_REAL" "$@"
fi
printf 'RAN %s\n' "$*" >> "$box/ran.log"
printf 'RAN %s\n' "$*" >> "$box/events.log"
trap 'echo EXITED >> "$box/events.log"' EXIT
# Long enough to read the working state and to press again while it lasts.
sleep 2
mode=$(cat "$box/mode")
case "$mode" in
    refuse) cat "$box/refusal" >&2; exit "$(cat "$box/refusal-status")" ;;
    fail) echo "flea: xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop" >&2; exit 1 ;;
esac
if [ "$shape" = claim ]; then
    echo com.thisisgm.flea.desktop > "$box/handler"
    [ "$mode" != partly ] || cat "$box/skipped" >&2
    echo "undo both with: flea --default off"
else
    echo org.gnome.Nautilus.desktop > "$box/handler"
fi
STUB
    chmod +x "$box/bin/flea"
    cat > "$box/bin/xdg-mime" <<'STUB'
#!/bin/sh
# Sample input: xdg-mime query default inode/directory
box="$FLEA_MAKEDEFAULT_BOX"
if [ "$#" -ne 3 ] || [ "$1" != query ] || [ "$2" != default ] || [ "$3" != inode/directory ]; then
    printf 'REFUSED xdg-mime %s\n' "$*" >> "$box/refused.log"
    exit 1
fi
echo query >> "$box/queries.log"
answer=$(cat "$box/handler")
echo ASKED >> "$box/events.log"
# A held query answers what the handler was when it was asked, once the case opens the gate or 20 s have passed.
waited=0
while [ -e "$box/hold" ] && [ ! -e "$box/gate" ] && [ "$waited" -lt 400 ]; do
    sleep 0.05
    waited=$((waited + 1))
done
echo "ANSWERED $answer" >> "$box/events.log"
echo "$answer"
STUB
    chmod +x "$box/bin/xdg-mime"
    # Every call is logged and only the exact portal restart is answered, so no argv reaches the operator's systemd.
    cat > "$box/bin/systemctl" <<'STUB'
#!/bin/sh
# Sample input: systemctl --user try-restart xdg-desktop-portal.service
printf 'SYSTEMCTL %s\n' "$*" >> "$FLEA_MAKEDEFAULT_BOX/systemctl.log"
[ "$*" = "--user try-restart xdg-desktop-portal.service" ] || exit 1
[ "$(cat "$FLEA_MAKEDEFAULT_BOX/restart")" = ok ]
STUB
    chmod +x "$box/bin/systemctl"

    makedefault_launch "$dir" "$box" "$real_bin"

    settings_open_key
    settle
    settings_section about
    makedefault_wait_handler org.gnome.Nautilus.desktop
    # The entry probe runs beside the handler read, and a miss would grey the row after this point.
    settle
    makedefault_wait "false|false||" "About did not open unticked and live over a Nautilus handler"
    ipc settingsModel | jq -e '(map(.id) | index("makeDefault")) as $at | .[$at - 1].label == "File manager" and .[$at].caption == null' >/dev/null \
        || fail "makedefault: the row is not directly under the File manager fact with no caption, got $(ipc settingsModel | jq -c 'map(.label)')"
    shot makedefault-off

    # Keyboard: Space claims, and a second Space while the run lasts runs nothing.
    settings_focus_row makeDefault
    key -k space >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Space did not start the claim"
    key -k space >/dev/null
    makedefault_wait "true|false|Folders, Show in folder and file dialogs open Flea.|foreground" "the claim did not settle ticked"
    [[ "$(makedefault_ran)" == "RAN --default;" ]] \
        || fail "makedefault: the stub ran '$(makedefault_ran)', so a press during the run was not ignored"
    makedefault_wait_handler com.thisisgm.flea.desktop
    [[ "$(makedefault_restarts)" == 1 ]] || fail "makedefault: a claim that went through did not restart the portal once"
    [[ "$(grep -c . "$queries")" == 2 ]] \
        || fail "makedefault: xdg-mime was asked $(grep -c . "$queries") times, so the handler was not read again after the run"
    ipc settingsModel | jq -e '(map(.id) | index("makeDefault")) as $at | .[$at + 1].elide == "right"' >/dev/null \
        || fail "makedefault: the note is not the one eliding line"
    shot makedefault-on

    # Pointer: a click on the row hands folders back, and the box follows the handler read after it.
    settings_click_control makeDefault
    makedefault_wait "true|true|Handing folders back|muted" "a click did not start handing folders back"
    makedefault_wait "false|false||" "handing back did not settle unticked with no note"
    [[ "$(makedefault_ran)" == "RAN --default;RAN --default off;" ]] || fail "makedefault: the stub ran '$(makedefault_ran)'"
    makedefault_wait_handler org.gnome.Nautilus.desktop
    [[ "$(makedefault_restarts)" == 2 ]] || fail "makedefault: a release that went through did not restart the portal"

    # Enter claims too; a build with no portal files says the file dialogs are left out.
    printf 'partly\n' > "$mode"
    key -k Return >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Enter did not start the claim"
    makedefault_wait "true|false|File dialogs need the flea package's portal files.|foreground" "a claim that skipped the chooser step did not say so"
    shot makedefault-partly
    printf 'ok\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|false||" "the partly claim was not handed back"
    [[ "$(makedefault_restarts)" == 4 ]] || fail "makedefault: the partly claim and its release did not each restart the portal"

    # A portal that will not restart keeps the switch as it went and says when file dialogs follow.
    printf 'fail\n' > "$restart"
    key -k space >/dev/null
    makedefault_wait "true|false|File dialogs follow after xdg-desktop-portal restarts.|foreground" "a failed portal restart did not say so"
    shot makedefault-portal
    printf 'ok\n' > "$restart"
    key -k space >/dev/null
    makedefault_wait "false|false||" "the claim with the late portal was not handed back"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: the portal was not restarted after each switch that went through"

    # A failed claim shows flea's own line in the error role, and the box stays what xdg-mime answers.
    printf 'fail\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|false|xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop|error" \
        "a failed claim did not show flea's own line"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: a failed run restarted the portal"
    shot makedefault-failed

    # A refused claim, the build with no desktop entry, leaves the row inert to Space and to a click.
    printf 'refuse\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|true|Install a Flea package to make it the default.|foreground" "a refused claim did not make the row inert"
    runs=$(grep -c . "$makedefault_log")
    key -k space >/dev/null
    settings_click_control makeDefault
    sleep 1
    [[ "$(grep -c . "$makedefault_log")" == "$runs" ]] || fail "makedefault: the inert row still ran '$(makedefault_ran)'"
    makedefault_wait "false|true|Install a Flea package to make it the default.|foreground" "the inert row changed under a press"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: a refused run restarted the portal"
    shot makedefault-unpackaged

    # An action with a live state, never a setting: nothing reached ui.json, at any depth, and no save was refused.
    ipc uiSettings | jq -e "$no_key" >/dev/null \
        || fail "makedefault: the session state carries makeDefault, or did not read as an object: $(ipc uiSettings)"
    [[ ! -e "$XDG_STATE_HOME/flea/ui.json" ]] || jq -e "$no_key" "$XDG_STATE_HOME/flea/ui.json" >/dev/null \
        || fail "makedefault: ui.json carries makeDefault, or does not parse"
    [[ "$(ipc lastMessage)" != "That setting could not be saved." ]] || fail "makedefault: a press was sent to the settings writer"

    # A fresh window's first read, held past a claim's exit, answers from before it; only the read begun after may tick the box.
    printf 'ok\n' > "$mode"
    : > "$makedefault_events_log"
    : > "$box/hold"
    makedefault_launch "$dir" "$box" "$real_bin"
    settings_open_key
    settle
    settings_section about
    settings_focus_row makeDefault
    key -k space >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Space did not start the claim over a held read"
    makedefault_wait_event EXITED
    # Time for the window to take the exit in before the held answer lands.
    settle
    [[ "$(makedefault_events)" == "ASKED;RAN --default;EXITED;" ]] \
        || fail "makedefault: the claim did not exit inside the held read, the stubs logged '$(makedefault_events)'"
    : > "$box/gate"
    makedefault_wait "true|false|Folders, Show in folder and file dialogs open Flea.|foreground" \
        "the answer from before the claim settled it" 200
    [[ "$(makedefault_events)" == "ASKED;RAN --default;EXITED;ANSWERED org.gnome.Nautilus.desktop;ASKED;ANSWERED com.thisisgm.flea.desktop;" ]] \
        || fail "makedefault: the handler was not read again behind the held read, the stubs logged '$(makedefault_events)'"
    [[ "$(makedefault_restarts)" == 7 ]] || fail "makedefault: the claim behind the held read did not restart the portal"
    shot makedefault-held

    # With xdg-mime nowhere on PATH no read can start, and neither About's read nor a claim's re-read may leave the row working.
    makedefault_path_without_mime "$box"
    # GPUI handles a missing xdg-mime without Qt's process-start warning.
    : > "$makedefault_events_log"
    makedefault_launch "$dir" "$box" "$real_bin" "$box/nomime"
    settings_open_key
    settle
    settings_section about
    # The entry probe lands beside the read that never started.
    settle
    makedefault_wait "false|false||" "About on a PATH with no xdg-mime did not open live and unticked"
    [[ "$(makedefault_handler)" == "Not reported" ]] || fail "makedefault: with no xdg-mime the File manager row states '$(makedefault_handler)'"
    settings_focus_row makeDefault
    key -k space >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Space did not start the claim with no xdg-mime"
    makedefault_wait "false|false||" "the claim stayed working behind a re-read whose xdg-mime could not start"
    [[ "$(makedefault_events)" == "RAN --default;EXITED;" ]] \
        || fail "makedefault: with no xdg-mime on PATH the stubs logged '$(makedefault_events)'"
    [[ "$(makedefault_handler)" == "Not reported" ]] || fail "makedefault: the re-read with no xdg-mime stated '$(makedefault_handler)'"
    [[ "$(makedefault_restarts)" == 8 ]] || fail "makedefault: the claim with no xdg-mime did not restart the portal"
    shot makedefault-nomime

    [[ ! -s "$refused" ]] || fail "makedefault: a stub was asked something it does not answer: $(tr '\n' ';' < "$refused")"
    key -k Escape >/dev/null
    settle

    printf 'MAKEDEFAULT keyboard=ok pointer=ok enter=ok partly=ok portal=ok failed=ok unpackaged=ok stored=none held=ok nomime=ok runs=%s restarts=%s\n' \
        "$runs" "$(makedefault_restarts)"
    kill_flea
}
