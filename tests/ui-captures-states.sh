#!/usr/bin/env bash
# Native captures for held Columns heroes, system refusals, grid reveal and Columns rename; sourced by ui.sh.
# shellcheck disable=SC2154 # ui.sh supplies the fixture root, the native driver and the shared helpers.

# The entrance fade's own duration token is ui/js/Motion.js durMs.open, so a shot never lands mid fade.
heldcolumns_fade_wait() {
    local open_ms wait_ms wait_s
    # Sample input, ui/js/Motion.js:7 inside var durMs at :5: "    open: 180,".
    open_ms=$(sed -n '/durMs/,/^}/s/^ *open: \([0-9][0-9]*\).*/\1/p' "$repo/ui/js/Motion.js" | head -1)
    [[ "$open_ms" =~ ^[0-9]+$ ]] || fail "heldcolumns: Motion durMs.open is unreadable"
    wait_ms=$(( open_ms * 2 + 200 ))
    wait_s="$(( wait_ms / 1000 )).$(printf '%03d' "$(( wait_ms % 1000 ))")"
    sleep "$wait_s"
}

# A held first listing has no IPC seam yet (AGENTS.md "The first window"): only a live shell answers "Target not found." then.
heldcolumns_expect_loading() {
    local answer
    answer=$(ipc ready 2>&1 || true)
    [[ "$answer" != true ]] || fail "heldcolumns: limit $1 left loading $2"
    [[ "$answer" == *"Target not found."* ]] || fail "heldcolumns: limit $1 got no live answer $2, ipc ready said '${answer:-nothing}'"
}

# launch() waits for the seam in assert_theme, which a held first listing builds only at its 10 s deadline, after the reading shot.
heldcolumns_launch() {
    local assert_theme_def
    assert_theme_def=$(declare -f assert_theme)
    assert_theme() { :; }
    launch "$1"
    eval "$assert_theme_def"
}

# Recording stubs for the release tools: release verbs log their argv and refuse, listing calls exec the real tool by absolute path, so the case fails only if a release runs.
cap_sysrefusal_write_stubs() {
    local bindir="$1" log="$2" tool verbs
    for tool in gio udisksctl umount fusermount3; do
        case "$tool" in
            gio) verbs='"mount -u"|"mount -e"|"mount -t"' ;;
            udisksctl) verbs='"unmount "*|"power-off "*' ;;
            umount) verbs='*' ;;
            fusermount3) verbs='"-u"*' ;;
        esac
        cat > "$bindir/$tool" <<EOS
#!/bin/sh
# Release verbs refuse (gio mount -u/-e/-t, udisksctl unmount/power-off, any umount, fusermount3 -u); the rest exec the real tool past this bindir or fail naming the tool.
case "\$1 \$2" in
$verbs)
printf '%s\n' "\$*" >> "$log"
exit 1
;;
esac
IFS=:
for d in \$PATH; do
if [ "\$d" != "$bindir" ] && [ -x "\$d/$tool" ]; then
exec "\$d/$tool" "\$@"
fi
done
printf '%s: no real tool past this bindir\n' "$tool" >&2
exit 127
EOS
        chmod +x "$bindir/$tool"
    done
}

# Columns heroes for a held listing and a denied folder, at 2 to 5 columns.
case_cap_heldcolumns() {
    local dir listing markers real_bin n estate err_by denied denied_sentence
    # Sample input: '    denied: "This folder cannot be opened: you do not have permission.",' yields the quoted sentence.
    denied_sentence=$(sed -n 's/^    denied: "\([^"\\]*\)",$/\1/p' "$repo/ui/js/Errors.js")
    [[ "$(grep -c '^    denied: ' "$repo/ui/js/Errors.js")" -eq 1 ]] || fail "heldcolumns: Errors.WORDS.denied must name one string"
    [[ -n "$denied_sentence" ]] || fail "heldcolumns: Errors.WORDS.denied is empty"
    dir="$fixture_root/cap-heldcolumns"
    listing="$dir/listing"
    markers="$dir/markers"
    real_bin="$flea_bin"
    sandbox_scratch "$dir"
    mkdir -p "$listing" "$markers"
    : > "$listing/one.txt"
    : > "$listing/two.txt"
    hanglisting_write_proxy "$dir/flea-proxy"
    trap 'chmod 700 "$dir"/denied-* 2>/dev/null; kill_flea' EXIT
    for n in 2 3 4 5; do
        seed_ui_state "$fixture_root/cap-heldcolumns-state-$n" '{"view":"columns","columnsLimit":'"$n"'}'
        export FLEA_HANGLISTING_REAL="$real_bin" FLEA_HANGLISTING_DELAY=15 FLEA_HANGLISTING_MARKER="$markers/held-$n"
        flea_bin="$dir/flea-proxy"
        heldcolumns_launch "$listing"
        flea_bin="$real_bin"
        wait_marker "$markers/held-$n" "heldcolumns: limit $n never held its listing"
        # Past the 150 ms hold-off, before the 10 s deadline.
        sleep 1
        heldcolumns_expect_loading "$n" "before its reading shot"
        heldcolumns_fade_wait
        shot "heldcolumns-$n-reading"
        heldcolumns_expect_loading "$n" "after its reading shot"
        wait_pane_waiting heldcolumns
        # The seam exists only now, so the theme of this launch, reading shot included, is checked here.
        assert_theme
        [[ "$(ipc viewMode)" == "columns" ]] || fail "heldcolumns: limit $n opened in $(ipc viewMode), not columns"
        [[ "$(ipc stateMessage)" == "That folder is not responding." ]] || fail "heldcolumns: limit $n spoke $(ipc stateMessage)"
        heldcolumns_fade_wait
        shot "heldcolumns-$n-waiting"
        wait_listing 2
        [[ "$(ipc path)" == "$listing" ]] || fail "heldcolumns: limit $n landed at $(ipc path)"
        [[ -z "$(ipc stateMessage)" ]] || fail "heldcolumns: limit $n kept $(ipc stateMessage)"
        kill_flea
        denied="$dir/denied-$n"
        mkdir -p "$denied"
        chmod 000 "$denied" || fail "heldcolumns: limit $n denied setup failed"
        seed_ui_state "$fixture_root/cap-heldcolumns-denied-$n" '{"view":"columns","columnsLimit":'"$n"'}'
        launch "$denied"
        estate=""
        err_by=$(( $(date +%s%3N) + 15000 ))
        while (( $(date +%s%3N) < err_by )); do
            estate=$(ipc state 2>/dev/null || true)
            [[ "$estate" == "locked" || "$estate" == "error" ]] && break
            sleep 0.2
        done
        # A denied scan answers locked, never error, per ui/js/Errors.js denied.
        [[ "$estate" == "locked" ]] || fail "heldcolumns: limit $n denied state is $estate"
        [[ "$(ipc stateMessage)" == "$denied_sentence" ]] || fail "heldcolumns: limit $n denied sentence is $(ipc stateMessage)"
        heldcolumns_fade_wait
        shot "heldcolumns-$n-error"
        chmod 700 "$denied" || fail "heldcolumns: limit $n denied restore failed"
        kill_flea
        printf 'HELDCOLUMNS limit=%s reading=ok waiting=ok error-denied=ok\n' "$n"
    done
    printf 'HELDCOLUMNS locked-dropped: the denied main-pane hero is shot per limit, and the child-column Locked tile exposes no IPC reader\n'
    trap - EXIT
    kill_flea
}

# System refusals through Ctrl+E: the volume sentence in a listing, the drive sentence on the rail.
case_cap_sysrefusal() {
    local dir listing stub_log saved_path entries label holders diag
    dir="$fixture_root/cap-sysrefusal"
    listing="$dir/listing"
    stub_log="$dir/stub.log"
    saved_path="$PATH"
    sandbox_scratch "$dir"
    mkdir -p "$listing" "$dir/bin"
    : > "$listing/one.txt"
    : > "$listing/two.txt"
    : > "$stub_log"
    cap_sysrefusal_write_stubs "$dir/bin" "$stub_log"
    trap 'export PATH="$saved_path"' EXIT
    export PATH="$dir/bin:$PATH"
    seed_ui_state "$fixture_root/cap-sysrefusal-state" '{"view":"list"}'
    launch "$listing"
    wait_listing 2
    key -M ctrl -k e -m ctrl >/dev/null
    # The sentence stands 4 s and the wait ends 25 s on, so the last read is blank either way; this names the branch Ctrl+E took.
    holders='[.[] | select((.path // "") != "") | select(.path as $m | $p == $m or ($p | startswith($m + "/"))) | {group, kind, label, path, mounted}]'
    diag='printf "release stubs ran: [%s], rail rows holding %s: %s, keys: %s" "$(cat "$stub_log")" "$listing" "$(ipc railEntries | jq -c --arg p "$listing" "$holders")" "$(ipc keyDeliveryState)"'
    wait_message "This folder is on a system volume, which Flea never unmounts" "$diag"
    shot sysrefusal-listing
    entries=$(ipc railEntries) || fail "sysrefusal: the rail has no entries"
    label=$(jq -r '[.[] | select(.group == "device" and .kind == "disk" and .path == "/") | .label] | first // empty' <<< "$entries") || fail "sysrefusal: the system disk row is unreadable"
    # The disk row carries path "/" by construction (ui/js/Devices.js:25), so an empty label is a broken rail, never a skip.
    [[ -n "$label" ]] || fail "sysrefusal: no system disk row in the rail entries"
    rail_focus
    key g >/dev/null
    settle
    rail_seek "$label"
    key -M ctrl -k e -m ctrl >/dev/null
    wait_message "$label is the system drive, which Flea never unmounts"
    shot sysrefusal-rail
    [[ ! -s "$stub_log" ]] || fail "sysrefusal: a release stub ran: $(cat "$stub_log")"
    export PATH="$saved_path"
    trap - EXIT
    printf 'SYSREFUSAL listing=ok rail=ok stubs=silent\n'
    kill_flea
}

# Grid cursor reveal by plain key moves past the fold, ending on End.
case_cap_gridreveal() {
    local dir menus_checks=0 index total guard
    dir="$fixture_root/cap-gridreveal"
    sandbox_scratch "$dir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    for ((index = 0; index < 1200; index++)); do
        printf -v total '%s/f%04d.txt' "$dir" "$index"
        : > "$total"
    done
    seed_ui_state "$fixture_root/cap-gridreveal-state" '{"view":"list"}'
    launch "$dir"
    wait_listing 1202
    switch_view grid
    key -k Home >/dev/null
    menus_expect renameState '.cursor == 0' "gridreveal starts at the top"
    guard=0
    while [[ "$(ipc viewContentY)" == "0" ]]; do
        (( guard >= 400 )) && fail "gridreveal: 400 Downs never left the first screen"
        key -k Down >/dev/null
        guard=$((guard + 1))
    done
    rename_far_whole "gridreveal-down" "$(ipc cursor)"
    shot gridreveal-down
    key -k End >/dev/null
    rename_design_still
    menus_expect renameState '.cursorName == "f1199.txt"' "gridreveal End reaches the last tile"
    shot gridreveal-end
    printf 'GRIDREVEAL down=ok end=ok\n'
    kill_flea
}

# Rename ending in Columns with focus back on the active column.
case_cap_columnsrename() {
    local menu_box menu_dir menus_checks=0
    menu_box="$fixture_root/cap-columnsrename-box"
    menu_dir="$menu_box/listing"
    sandbox_scratch "$menu_box"
    printf 'native rename fixture\n' > "$menu_box/.flea-test-sandbox"
    mkdir -p "$menu_dir"
    printf 'original bytes\n' > "$menu_dir/a-original.md"
    printf 'existing bytes\n' > "$menu_dir/b-existing.md"
    : > "$menu_dir/c-third.md"
    seed_ui_state "$fixture_root/cap-columnsrename-state" '{"view":"columns"}'
    launch "$menu_dir"
    wait_listing 3
    [[ "$(ipc viewMode)" == "columns" ]] || fail "columnsrename: opened in $(ipc viewMode), not columns"
    key -k Home >/dev/null
    menus_expect renameState '.cursor == 0 and .cursorName == "a-original.md"' "columnsrename starts on the first file"
    key -k F2 >/dev/null
    menus_expect renameState '.index >= 0 and .focused and (.pending | not) and .error == ""' "columnsrename F2 opens the inline editor"
    key -M ctrl -k a -m ctrl -k BackSpace >/dev/null
    key "a-renamed.md" >/dev/null
    menus_guard "$menu_dir/a-original.md"
    menus_guard "$menu_dir/a-renamed.md"
    key -k Return >/dev/null
    wait_marker "$menu_dir/a-renamed.md" "columnsrename: the rename never wrote a-renamed.md"
    menus_expect renameState '.index == -1 and (.pending | not) and (.loading | not) and .cursorName == "a-renamed.md"' "columnsrename lands on the renamed file"
    # The columns area takes focus back on editor close: QML names it ColumnsArea_QMLTYPE_n(0x...), GPUI names its focus stop "list"; // keeps a null focus a named failure.
    menus_expect keyDeliveryState '.listFocus and ((.activeFocusItem // "") | test("^(ColumnsArea_QML|list$)"))' "columnsrename hands focus back to the active column"
    [[ -n "$(ipc columnActiveRect)" ]] || fail "columnsrename: the active column has no rectangle"
    shot columnsrename-ended
    printf 'COLUMNSRENAME landed=ok focus=ok\n'
    kill_flea
}
