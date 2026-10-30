#!/usr/bin/env bash
# Sourced by ui.sh, runs only by name: Tab walks the ring through the chrome bar and the tab strip, Ctrl+4 is Dual, Ctrl+Shift+Z is redo.
# shellcheck disable=SC2154 # ui.sh supplies the fixture root and the native driver helpers.
# One omarchy-drive ipc round trip costs 190 to 565 ms on minipc, so a call that outlives ten seconds is a hang and says so.
keyreach_ipc() {
    timeout 10 "$FLEA_IPC_HELPER" drive -p "$flea_ui/boot" flea "$@" || fail "keyreach: ipc $1 gave no answer in ten seconds"
}
# keyreach_until <seconds> <what> <command...>: polls on the wall clock and fails naming what it waited for.
keyreach_until() {
    local limit="$1" what="$2" deadline=$((SECONDS + $1))
    shift 2
    until "$@"; do
        (( SECONDS < deadline )) || fail "keyreach: waited $limit s for $what"
        sleep 0.25
    done
}
keyreach_has_path() { [[ -e "$1" ]]; }
keyreach_on_tab() { [[ "$(keyreach_ipc tabIndex)" == "$1" ]]; }
keyreach_lists() { [[ "$(keyreach_ipc path)" == "$1" ]]; }
keyreach_has_tabs() { [[ "$(keyreach_ipc tabCount)" == "$1" ]]; }
# Looks the row up among the fixture's three, so a listing that left the fixture fails at once instead of walking its rows.
keyreach_row_of() {
    local want="$1" total row i
    total=$(keyreach_ipc total)
    [[ "$total" == 3 ]] || fail "keyreach: the listing holds '$total' rows at '$(keyreach_ipc path)', not the fixture's 3, so $want is not looked up"
    for i in 0 1 2; do
        row=$(keyreach_ipc rowAt "$i")
        [[ "$row" == "$want|"* ]] && { printf '%s' "$i"; return; }
    done
    fail "keyreach: no row named $want among the fixture's 3"
}
# Which side of the dual view holds the keyboard, 0 left and 1 right, from the window's own ipc.
keyreach_side() { keyreach_ipc dualState | jq -r .focused; }
keyreach_tab() {
    key -k Tab >/dev/null
    settle
}
keyreach_expect() {
    local want="$1" why="$2" got
    got=$(keyreach_ipc focusStop)
    [[ "$got" == "$want" ]] || fail "keyreach: $why: the keyboard is on '$got', not '$want'"
}
case_keyreach() {
    local dir="$fixture_root/keyreach" tabs note_row
    sandbox_scratch "$dir"
    mkdir "$dir/sub"
    printf 'one\n' > "$dir/note.txt"
    printf 'two\n' > "$dir/other.txt"
    seed_ui_state "$fixture_root/keyreach-state" '{"keys":"default","view":"list"}'
    launch "$dir"
    wait_listing 3
    settle
    local shown
    shown=$(keyreach_ipc path)
    [[ "$shown" == "$dir" ]] || fail "keyreach: the window lists '$shown', not the fixture root '$dir', so no key is sent"
    [[ -e "$dir/note.txt" && -d "$dir/sub" ]] || fail "keyreach: the fixture root '$dir' lost its files"
    echo "-- one tab: list, rail, chrome bar, list --"
    keyreach_tab; keyreach_expect rail "Tab from the list"
    keyreach_tab; keyreach_expect "chrome:arrow-up" "Tab from the rail"
    shot keyreach-chrome-back
    keyreach_tab; keyreach_expect list "a strip of one tab is no stop"
    echo "-- two tabs: the strip joins the ring --"
    click_row 0 left
    settle
    key -M ctrl -k Return -m ctrl >/dev/null
    keyreach_until 10 "Ctrl+Return to open the second tab" keyreach_has_tabs 2
    tabs=$(keyreach_ipc tabCount)
    [[ "$tabs" == "2" ]] || fail "keyreach: Ctrl+Return opened $tabs tabs, not 2"
    settle
    keyreach_tab; keyreach_tab; keyreach_tab
    keyreach_expect "strip:$(keyreach_ipc tabIndex)" "Tab from the bar"
    shot keyreach-tab
    keyreach_tab; keyreach_expect list "Tab from the strip"
    key -M shift -k Tab -m shift >/dev/null
    settle
    keyreach_expect "strip:2" "Shift+Tab from the list"
    key -k Escape >/dev/null
    settle
    keyreach_expect list "Escape from the strip"
    # Ctrl+Return left the keyboard's tab on the new one, which lists sub; the rename below needs the fixture root's three rows.
    key 1 >/dev/null
    keyreach_until 10 "the digit 1 to select the first tab" keyreach_on_tab 0
    # The tab index flips at the switch request, so the path is waited on, not read once.
    keyreach_until 10 "tab 1 to list the fixture root $dir" keyreach_lists "$dir"
    echo "-- Ctrl+4 is Dual --"
    key -k Escape >/dev/null
    key -M ctrl -k 4 -m ctrl >/dev/null
    settle
    [[ "$(keyreach_ipc dualState | jq -r .active)" == "true" ]] || fail "keyreach: Ctrl+4 did not open the dual view"
    [[ "$(keyreach_side)" == 0 ]] || fail "keyreach: the dual view opened with the keyboard on side '$(keyreach_side)', not the left"
    keyreach_tab
    keyreach_expect list "Tab from the left list reaches the right one"
    [[ "$(keyreach_side)" == 1 ]] || fail "keyreach: Tab from the left list left the keyboard on side '$(keyreach_side)', not the right"
    # The rail is the left pane's, so the ring runs right list, rail, bar (tests/ui-railpointer.sh dualtab pins the first step).
    keyreach_tab
    keyreach_expect rail "Tab from the right list reaches the rail"
    keyreach_tab
    keyreach_expect "chrome:arrow-up" "Tab from the rail reaches the bar"
    shot keyreach-chrome-dual
    key -M ctrl -k 1 -m ctrl >/dev/null
    settle
    echo "-- Ctrl+Shift+Z redoes an undone rename --"
    note_row=$(keyreach_row_of note.txt) || fail "keyreach: note.txt has no row to click"
    click_row "$note_row" left
    settle
    key r >/dev/null
    settle
    printf '%s' "renamed" | omarchy-drive key --window com.thisisgm.flea - >/dev/null || fail "keyreach: typing the new name failed"
    key -k Return >/dev/null
    keyreach_until 10 "the rename to land as renamed.txt" keyreach_has_path "$dir/renamed.txt"
    [[ -e "$dir/renamed.txt" ]] || fail "keyreach: the rename never landed"
    key z >/dev/null
    keyreach_until 10 "z to put note.txt back" keyreach_has_path "$dir/note.txt"
    [[ -e "$dir/note.txt" && ! -e "$dir/renamed.txt" ]] || fail "keyreach: z did not undo the rename"
    key -M ctrl -M shift -k z -m shift -m ctrl >/dev/null
    keyreach_until 10 "Ctrl+Shift+Z to redo the rename" keyreach_has_path "$dir/renamed.txt"
    [[ -e "$dir/renamed.txt" && ! -e "$dir/note.txt" ]] || fail "keyreach: Ctrl+Shift+Z did not redo the rename"
    printf 'KEYREACH ok\n'
    kill_flea
}
