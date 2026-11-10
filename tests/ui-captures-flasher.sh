# Flash image capture case, outside the default wanted list (Flasher, FlasherMenus, FlasherRun, FlasherLight).

# The zero-filled image fixture is one MiB.
cap_flasher_image_bytes=1048576

# One flashState read must satisfy a jq expression within a bound before a shot is taken.
cap_flasher_expect() {
    local expression="$1" label="$2" observed end=$((SECONDS + 15))
    while (( SECONDS < end )); do
        observed=$(ipc flashState) || fail "cap_flasher: the flash reader failed for $label"
        jq -e "$expression" <<< "$observed" >/dev/null && return 0
        sleep 0.05
    done
    fail "cap_flasher: $label: $observed"
}

# Every FlasherRun state that needs a real write, and every choosing state that needs hardware.
cap_flasher_skipped() {
    printf 'CAP SKIP flasher-ready-chosen-drive: needs a removable drive, driven by the controller\n'
    printf 'CAP SKIP flasher-two-drives-none-chosen: needs two fitting drives, driven by the controller\n'
    printf 'CAP SKIP flasher-refused-small: needs a drive smaller than the image, driven by the controller\n'
    printf 'CAP SKIP flasher-refused-readonly: needs a read-only drive, driven by the controller\n'
    printf 'CAP SKIP flasher-refused-holdsimage: needs the image stored on the drive itself, driven by the controller\n'
    printf 'CAP SKIP flasher-refused-member: needs a RAID or volume group member, driven by the controller\n'
    printf 'CAP SKIP flasher-twin-names: needs two drives with one label, driven by the controller\n'
    printf 'CAP SKIP flasher-flash-model-name: needs a labelless drive and the Flash step, driven by the controller\n'
    printf 'CAP SKIP flasher-choose-image-row: opened from a drive, needs a removable volume, driven by the controller\n'
    printf 'CAP SKIP flasher-rail-menu: Flash image needs a removable volume on the rail, driven by the controller\n'
    printf 'CAP SKIP flasher-flash-pressed: the button is unavailable with no drive and no IPC seam proves pressed\n'
    printf 'CAP SKIP flasher-run-writing: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-verifying: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-done: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-failed: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-cancelled-flashing: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-cancelled-verifying: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-password-wait: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-final-flush: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-unknown-size: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-after-back: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-after-eject: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-other-failures: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-run-two-jobs: needs a real write, driven by the controller\n'
    printf 'CAP SKIP flasher-light: the light theme is switched on minipc, driven by the controller\n'
    return 0
}

# Flasher from the menu way in to the menu refusal; the drive count picks the pane shot.
case_cap_flasher() {
    local dir="$fixture_root/cap-flasher" centre cx cy wx wy ww wh drives target cursor
    sandbox_scratch "$dir"
    # A .img needs no magic, so zeros probe as an image; a .txt probes refused.
    head -c "$cap_flasher_image_bytes" /dev/zero > "$dir/debian-13.img" || fail "cap_flasher: the image fixture failed"
    printf 'notes\n' > "$dir/notes.txt"
    seed_ui_state "$fixture_root/cap-flasher-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 2
    cap_resize 1100 700
    click_row "$(row_index_of debian-13.img)" left
    settle
    click_row "$(row_index_of debian-13.img)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_flasher: the file menu never opened"
    [[ "|$(ipc contextMenuEntries)|" == *"|Flash to Drive|"* ]] \
        || fail "cap_flasher: the image menu offers no Flash to Drive, got $(ipc contextMenuEntries)"
    shot flasher-menu-flash-to-drive
    menu_seek "Flash to Drive"
    key -k Return >/dev/null
    cap_flasher_expect '.opened == true' "the Flash pane never opened"
    cap_flasher_expect '.image != ""' "the image never landed in the Flash pane"
    settle
    shot flasher-strip-at-rest
    # minipc keeps its bench stick plugged in, so the drive count decides the shot, never a refusal.
    drives=$(ipc flashState | jq -r .drives)
    [[ "$drives" =~ ^[0-9]+$ ]] || fail "cap_flasher: the drive count never read, got $drives"
    if (( drives == 0 )); then
        shot flasher-image-no-drive
    else
        cursor=$(ipc flashState | jq -r .cursor)
        for _press in $(seq 1 3); do
            [[ "$cursor" == "0" ]] && break
            key k >/dev/null
            settle
            cursor=$(ipc flashState | jq -r .cursor)
        done
        cap_flasher_expect '.cursor == 0' "k never parked the keyboard on the image row"
        shot flasher-image-drive-listed
        printf 'CAP SKIP flasher-image-no-drive: a drive is plugged in, the controller shoots it with the stick detached\n'
    fi
    # The Flash row sits past every drive at n + 1; the empty-drive row takes no cursor stop.
    target=$((drives + 1))
    cursor=$(ipc flashState | jq -r .cursor)
    for _press in $(seq 1 $((drives + 2))); do
        [[ "$cursor" == "$target" ]] && break
        key j >/dev/null
        settle
        cursor=$(ipc flashState | jq -r .cursor)
    done
    cap_flasher_expect ".cursor == $target" "j never reached the Flash row"
    settle
    # A fitting stick is preselected and arms Flash, so the shot is named for the state it shows.
    flash_state=$(ipc flashState | jq -r 'if .live == true then "live" elif .live == false then "greyed" else "" end')
    [[ -n "$flash_state" ]] || fail "cap_flasher: the Flash row's state never read, got $(ipc flashState)"
    shot "flasher-flash-$flash_state-focus"
    centre=$(ipc flashControlCentre flash)
    read -r cx cy <<< "$centre"
    [[ "$cx $cy" =~ ^[0-9]+\ [0-9]+$ ]] || fail "cap_flasher: the Flash control has no centre, got $centre"
    read -r wx wy ww wh < <(window_box) || fail "cap_flasher: native window coordinates unavailable"
    assert_focus
    omarchy-drive move "$((wx + cx - 6))" "$((wy + cy))" >/dev/null || fail "cap_flasher: pointer approach to Flash failed"
    omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null || fail "cap_flasher: pointer move onto Flash failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1 || fail "cap_flasher: pointer nudge failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x -1 -y 0 >/dev/null 2>&1 || fail "cap_flasher: pointer nudge back failed"
    settle
    shot flasher-flash-hover
    key -k Escape >/dev/null
    cap_flasher_expect '.opened == false' "Escape never left the Flash pane"
    settle
    click_row "$(row_index_of notes.txt)" left
    settle
    click_row "$(row_index_of notes.txt)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "cap_flasher: the menu on notes.txt never opened"
    [[ "|$(ipc contextMenuEntries)|" != *"|Flash to Drive|"* ]] \
        || fail "cap_flasher: a non-image is offered Flash to Drive"
    shot flasher-menu-no-flash
    key -k Escape >/dev/null
    settle
    cap_flasher_skipped
    printf 'CAP_FLASHER menu=in refusal=menu-only pane=image-drives-%s flash=%s-focus,hover run=skipped\n' "$drives" "$flash_state"
    kill_flea
}
