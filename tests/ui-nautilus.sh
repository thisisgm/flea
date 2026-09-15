# Native regression checks, sourced by ui.sh; all files and state are private fixtures.
case_nautilus() {
    local dir="$fixture_root/nautilus" state="$fixture_root/nautilus-state" before pid
    sandbox_scratch "$dir"
    mkdir -p "$dir/child" "$dir/.hidden"
    : > "$dir/note.txt"
    seed_ui_state "$state" '{"keys":"nautilus","hidden":true,"view":"list","newTab":"current"}'
    launch "$dir"
    wait_listing 3
    [[ "$(ipc keymapPreset)" == nautilus ]] || fail "nautilus: stored preset was not loaded"
    [[ "$(ipc showHidden)" == true ]] || fail "nautilus: hidden files were not shown"

    key -M ctrl -k h -m ctrl >/dev/null
    wait_listing 2
    [[ "$(ipc showHidden)" == false ]] || fail "nautilus: Ctrl+H did not hide dotfiles"
    key -M ctrl -k h -m ctrl >/dev/null
    wait_listing 3
    [[ "$(ipc showHidden)" == true ]] || fail "nautilus: Ctrl+H did not restore dotfiles"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == rail ]] || fail "nautilus: Tab did not focus Places"
    key -M ctrl -k h -m ctrl >/dev/null
    wait_listing 2
    key -M ctrl -k h -m ctrl >/dev/null
    wait_listing 3
    key -k Tab >/dev/null
    settle

    key -M ctrl -k t -m ctrl >/dev/null
    settle
    [[ "$(ipc tabCount)" == 2 ]] || fail "nautilus: Ctrl+T did not create a tab"
    key -M alt -k 1 -m alt >/dev/null
    settle
    [[ "$(ipc tabIndex)" == 0 ]] || fail "nautilus: Alt+1 did not select the first tab"
    key -M ctrl -k w -m ctrl >/dev/null
    wait_path "$dir"
    [[ "$(ipc tabCount)" == 1 ]] || fail "nautilus: Ctrl+W did not close only the current tab"
    key -M ctrl -k 2 -m ctrl >/dev/null
    settle
    [[ "$(ipc viewMode)" == grid ]] || fail "nautilus: Ctrl+2 did not select grid"
    key -M ctrl -k 1 -m ctrl >/dev/null
    settle
    [[ "$(ipc viewMode)" == list ]] || fail "nautilus: Ctrl+1 did not select list"

    key -M ctrl -k l -m ctrl >/dev/null
    settle
    [[ "$(ipc pathBarOpen)" == true ]] || fail "nautilus: Ctrl+L did not open location"
    key "$dir/child" -k Return >/dev/null
    wait_path "$dir/child"
    key -M alt -k Up -m alt >/dev/null
    wait_path "$dir"
    key -M alt -k Left -m alt >/dev/null
    wait_path "$dir/child"
    key -M alt -k Right -m alt >/dev/null
    wait_path "$dir"
    key / >/dev/null
    settle
    [[ "$(ipc pathBarOpen)" == true && "$(ipc pathBarText)" == / ]] || fail "nautilus: slash did not prefill root"
    key -k Escape >/dev/null
    settle
    key d >/dev/null
    settle
    ipc keyDeliveryState | jq -e '.searchMode == "typing" and .searchQuery == "d"' >/dev/null \
        || fail "nautilus: plain typing did not start search"
    key -k Escape >/dev/null
    settle
    before=$(ipc listRequests)
    key -k F5 >/dev/null
    settle
    [[ "$(ipc listRequests)" -gt "$before" ]] || fail "nautilus: F5 did not refresh"
    wait_listing 3

    # First attribute observation may refresh; repeated no-op chmods must not keep re-listing.
    chmod 755 "$dir/.hidden"
    sleep 1
    before=$(ipc listRequests)
    for _attempt in 1 2 3 4 5; do chmod 755 "$dir/.hidden"; sleep 0.3; done
    settle
    [[ "$(ipc listRequests)" == "$before" ]] || fail "nautilus: unchanged hidden attributes restarted listing"
    : > "$dir/.new-hidden"
    wait_listing 4
    [[ "$(ipc listRequests)" -gt "$before" ]] || fail "nautilus: a real hidden file change was missed"

    # Exercise the QML window host and its normal backend drain, not just the JS dispatch.
    pid=$(flea_pid)
    key -M ctrl -k w -m ctrl >/dev/null
    for _attempt in $(seq 1 100); do [[ ! -d "/proc/$pid" ]] && break; sleep 0.1; done
    [[ ! -d "/proc/$pid" ]] || fail "nautilus: Ctrl+W on the last tab did not close its window"
    printf 'NAUTILUS hidden=list+rail tabs=ok views=ok navigation=ok typing=ok refresh=ok no-op-attributes=quiet real-change=visible close=drained\n'
    kill_flea
}
