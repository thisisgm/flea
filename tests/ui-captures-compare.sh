# Compare board captures, sourced by ui.sh and run only by name on the native display.
. "$repo/tests/lib/pointer-glide.sh"

# One byte past OVER_BYTES in src/backend/compare.rs:13 (1 MiB), so Compare refuses the large pair.
readonly cap_compare_over_bytes=1048577

# The clipboard helper starts a child process, so allow its ownership change a bounded wait.
readonly cap_compare_clipboard_wait_s=15
# Poll at 50 ms to wait for clipboard ownership without spinning.
readonly cap_compare_clipboard_poll_s=0.05
# A one-second hold catches clipboard changes a Copy child delivers after the release event.
readonly cap_compare_clipboard_hold_ms=1000
# Four attempts bound the native search for the Copy patch Tab stop.
readonly cap_compare_tab_tries=4

# The twenty-line pair differs only at lines 12 to 17, so diff -u prints the board's one hunk.
cap_compare_fixture() {
    local dir="$1" pair n
    sandbox_require "$dir"
    mkdir -p "$dir/diff" "$dir/identical" "$dir/binary" "$dir/large"
    for pair in a b; do
        for n in 1 2 3 4 5 6 7 8; do printf 'Line %s of the notes.\n' "$n"; done > "$dir/diff/notes-$pair.md"
        printf '## Previews\n\nEach kind asks for what it shows.\n' >> "$dir/diff/notes-$pair.md"
    done
    cat >> "$dir/diff/notes-a.md" <<'TEXT'
Thumbnails never load on phones.
The map holds every row.
A taller preview costs nothing.
Ports keep their slots.
Marks ship one size.
Facts fold into two lines.
TEXT
    cat >> "$dir/diff/notes-b.md" <<'TEXT'
Thumbnails stay cache-only on phones.
The map holds the visible rows.
A taller preview costs what a shorter one did.
Ports keep their slots while scrolling.
Marks share one optical size.
Facts fold into one line.
TEXT
    for pair in a b; do
        printf 'Only the visible lines are asked for.\nA newer request cancels an older one.\nThe frame runs to the column bottom.\n' >> "$dir/diff/notes-$pair.md"
        cp "$dir/diff/notes-a.md" "$dir/identical/notes-$pair.md"
        printf '\211PNG\r\n\032\n\0image %s\n' "$pair" > "$dir/binary/shot-$pair.png"
        head -c "$cap_compare_over_bytes" /dev/zero | tr '\0' 'a' > "$dir/large/dump-$pair.log"
    done
}

# One IPC sample must satisfy the whole state immediately before its shot.
cap_compare_shot() {
    local name="$1" filter="$2"
    menus_expect compareState ".opened and .visible and .landed and .ready and (if .controls[0].visible then .controls[0].label == \"Copy Patch\" else .controls[0].label == \"\" end) and .controls[1].label == \"Close\" and ($filter)" "$name"
    shot "$name"
}

# Every specimen enters through two listing marks and the menu's Compare row.
cap_compare_open() {
    local dir="$1" first="$2" second="$3" answer="$4" title="$5" menu_state
    seed_ui_state "${dir%/*}/state-$answer" '{"keys":"default","view":"list","display":{"textSize":{"mode":14}},"menu":{"hidden":[]}}'
    launch "$dir"
    wait_listing 2
    cap_resize 1100 700
    cap_unlock_still
    menus_expect bodyPx '. == 14' "the board's body text size is set"
    seek_row_named "$first"
    key v >/dev/null
    seek_row_named "$second"
    key v >/dev/null
    [[ "$(ipc selectionCount)" == 2 ]] || fail "compare: the two files were not marked"
    key m >/dev/null
    menus_expect menuState '.opened' "the two-file menu opens"
    menu_state=$(ipc menuState)
    if ! jq -e 'any(.entries[]; .label == "Compare")' <<< "$menu_state" >/dev/null; then
        printf 'compare: menuState before missing Compare: %s\n' "$menu_state" >&2
        fail "compare: no row labelled Compare"
    fi
    menu_seek Compare
    key -k Return >/dev/null
    menus_expect compareState ".opened and .visible and .landed and .ready and .answer == \"$answer\"" "the $answer specimen lands"
    [[ "$(ipc compareState | jq -r '.title')" == "$title" ]] || fail "compare: the title does not name the board's pair"
}

# The card reports its own control centres; pointer_glide supplies the pointer frames Qt hears.
cap_compare_point() {
    local name="$1" centre cx cy wx wy ww wh
    if [[ "$name" == title ]]; then
        centre=$(ipc compareState | jq -er '.titleCentre') || fail "compare: no title centre"
    else
        centre=$(ipc compareState | jq -er --arg name "$name" '.controls[] | select(.name == $name and .visible) | .centre') \
            || fail "compare: no visible $name control"
    fi
    # Sample input: "640 360", the control centre in window pixels from compareState.
    read -r cx cy <<< "$centre"
    [[ "$cx $cy" =~ ^[0-9]+\ [0-9]+$ ]] || fail "compare: invalid centre for $name: [$centre]"
    # Sample input: "120 80 1100 700", the window's layout x, y, width and height from window_box.
    read -r wx wy ww wh < <(window_box) || fail "compare: native window coordinates unavailable"
    pointer_glide "$((wx + cx))" "$((wy + cy))" || fail "compare: pointer never reached $name"
    settle
}

# Copy sends the entire patch, including its path headers, without changing the overlay.
cap_compare_clipboard() {
    local want="$1" got="" end=$((SECONDS + cap_compare_clipboard_wait_s))
    while (( SECONDS < end )); do
        got=$(wl-paste --no-newline 2>/dev/null || true)
        [[ "$got" == "$want" ]] && return 0
        sleep "$cap_compare_clipboard_poll_s"
    done
    fail "compare: the clipboard never held the complete patch"
}

# Every poll must keep the sentinel through the whole window, including its final sample.
cap_compare_clipboard_hold() {
    # Sample input: date +%s%3N prints "1791913200123", wall-clock milliseconds.
    local want="$1" got="" now end=$(( $(date +%s%3N) + cap_compare_clipboard_hold_ms ))
    while :; do
        now=$(date +%s%3N)
        got=$(wl-paste --no-newline 2>/dev/null || true)
        [[ "$got" == "$want" ]] || fail "compare: release off Copy patch changed the clipboard"
        (( now >= end )) && return 0
        sleep "$cap_compare_clipboard_poll_s"
    done
}

case_cap_compare() {
    local dir menus_checks=0 patch state diff_rc=0 _attempt
    sandbox_require "$fixture_root/compare"
    dir=$(mktemp -d "$fixture_root/compare.XXXXXXXX") || fail "compare: cannot create the specimen sandbox"
    : > "$dir/$SANDBOX_MARKER"
    cap_compare_fixture "$dir"
    patch=$(diff -u --label "$dir/diff/notes-a.md" --label "$dir/diff/notes-b.md" -- "$dir/diff/notes-a.md" "$dir/diff/notes-b.md") || diff_rc=$?
    [[ "$diff_rc" == 1 && "$patch" == *'@@ -9,12 +9,12 @@'* ]] || fail "compare: the fixture does not give the board's hunk"
    cap_compare_open "$dir/diff" notes-a.md notes-b.md diff 'notes-a.md and notes-b.md · 12 lines differ'
    cap_compare_point title
    state=$(ipc compareState)
    [[ "$(jq -r '.patch' <<< "$state")" == "$patch" ]] || fail "compare: the displayed patch differs from diff -u"
    menus_expect compareState '.mark == "markdown" and (.rows | length) == 19 and .rows[0].text == "@@ -9,12 +9,12 @@" and [.rows[] | select(.kind == "context") | [.oldNo, .newNo]] == [[9,9],[10,10],[11,11],[18,18],[19,19],[20,20]] and [.rows[] | select(.kind == "removed") | .oldNo] == [12,13,14,15,16,17] and [.rows[] | select(.kind == "added") | .newNo] == [12,13,14,15,16,17]' "the board's hunk and both number gutters are present"
    cap_compare_shot compare-diff '.answer == "diff" and all(.controls[]; .visible and .enabled and (.hovered or .pressed or .focused or .ring | not))'
    cap_compare_shot compare-copy-rest '.answer == "diff" and [.controls[] | select(.name == "copy") | [.hovered,.pressed,.focused,.ring]] == [[false,false,false,false]]'
    cap_compare_shot compare-close-rest '.answer == "diff" and [.controls[] | select(.name == "close") | [.hovered,.pressed,.focused,.ring]] == [[false,false,false,false]]'
    printf 'Compare capture clipboard sentinel\n' | wl-copy
    cap_compare_clipboard 'Compare capture clipboard sentinel'
    key y >/dev/null
    cap_compare_clipboard "$patch"
    cap_compare_shot compare-copy-key '.answer == "diff" and all(.controls[]; .pressed | not)'
    cap_compare_point copy
    cap_compare_shot compare-copy-hover '.answer == "diff" and [.controls[] | select(.name == "copy") | [.hovered,.pressed,.focused,.ring]] == [[true,false,false,false]]'
    printf 'Compare capture clipboard sentinel\n' | wl-copy
    cap_compare_clipboard 'Compare capture clipboard sentinel'
    trap 'YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || true' EXIT
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 || fail "compare: Copy patch press failed"
    cap_compare_shot compare-copy-pressed '.answer == "diff" and [.controls[] | select(.name == "copy") | [.hovered,.pressed]] == [[true,true]]'
    cap_compare_point title
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || fail "compare: Copy patch release failed"
    trap - EXIT
    menus_expect compareState '.opened and all(.controls[]; .pressed | not)' "release off the button leaves the overlay open"
    cap_compare_clipboard_hold 'Compare capture clipboard sentinel'
    cap_compare_point close
    cap_compare_shot compare-close-hover '.answer == "diff" and [.controls[] | select(.name == "close") | [.hovered,.pressed,.focused,.ring]] == [[true,false,false,false]]'
    cap_compare_point title
    for (( _attempt=0; _attempt<cap_compare_tab_tries; _attempt++ )); do
        key -k Tab >/dev/null
        state=$(ipc compareState)
        jq -e '[.controls[] | select(.name == "copy") | .focused] == [true]' <<< "$state" >/dev/null && break
    done
    cap_compare_shot compare-copy-focus '.answer == "diff" and [.controls[] | select(.name == "copy") | [.focused,.ring,.hovered,.pressed]] == [[true,true,false,false]]'
    printf 'Compare capture clipboard sentinel\n' | wl-copy
    cap_compare_clipboard 'Compare capture clipboard sentinel'
    key -k Return >/dev/null
    cap_compare_clipboard "$patch"
    menus_expect compareState '.opened and .answer == "diff"' "Return on Copy patch keeps the overlay"
    cap_compare_open "$dir/identical" notes-a.md notes-b.md identical 'notes-a.md and notes-b.md'
    cap_compare_point title
    cap_compare_shot compare-identical '.answer == "identical" and .sentence == "The two files are identical." and .rows == [] and [.controls[] | select(.name == "copy") | .visible] == [false]'
    cap_compare_open "$dir/binary" shot-a.png shot-b.png binary 'shot-a.png and shot-b.png'
    cap_compare_point title
    cap_compare_shot compare-binary '.answer == "binary" and .sentence == "These files are binary, so there is nothing to compare." and .rows == [] and [.controls[] | select(.name == "copy") | .visible] == [false]'
    cap_compare_open "$dir/large" dump-a.log dump-b.log over 'dump-a.log and dump-b.log'
    cap_compare_point title
    cap_compare_shot compare-large '.answer == "over" and .sentence == "These files are over 1.0 MB, too large to compare." and .rows == [] and [.controls[] | select(.name == "copy") | .visible] == [false]'
    key -k Escape >/dev/null
    menus_expect compareState '.opened | not' "Escape closes Compare"
    printf 'CAP_COMPARE specimens=diff,identical,binary,large copy=rest,hover,focus,pressed,key close=rest,hover\n'
    kill_flea
}
