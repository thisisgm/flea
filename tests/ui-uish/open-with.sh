# GPUI overrides for UISH-22; seeded from the unchanged oracle at 2faf8d099.
case_openterminal() {
    local dir="$fixture_root/openterminal"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    printf 'abc' > "$dir/target.txt"
    local ran="$dir/ran.log" opened="$dir/opened.log" real_bin="$flea_bin"
    : > "$ran"
    : > "$opened"
    # FLEA_BIN is the backend's binary as well as the opener's, so only --terminal is intercepted
    # and every other mode execs the real one: a stub that swallowed --backend would leave the
    # window with no listing to press a key in. The sleep is what makes the single-flight guard and
    # the two-paths-at-once check observable at all.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = --terminal ] || exec %q "$@"\n' "$real_bin"
      printf 'sleep 1\n'
      printf 'printf "TERMINAL %%s\\n" "$2" >> %q\n' "$ran"
      printf 'exit 0\n'
    } > "$dir/bin/flea"
    chmod +x "$dir/bin/flea"
    # The real route, stubbed too: if anything ever reaches src/terminal.rs this records it instead
    # of opening a terminal on the operator's display, which is what used to leak a window per run.
    {
      printf '#!/bin/sh\n'
      printf '# Sample input: xdg-terminal-exec --dir=/home/flea-sandbox/fixtures/openterminal\n'
      printf 'printf "REAL-TERMINAL %%s\\n" "${1#--dir=}" >> %q\n' "$ran"
      printf 'exit 0\n'
    } > "$dir/bin/xdg-terminal-exec"
    chmod +x "$dir/bin/xdg-terminal-exec"
    # Only the open subcommand is intercepted, so stubbing the opener leaves the gio mount calls
    # ui/NetworkMounts.qml makes on every launch answering from the real gio. That name is the mount
    # tool's own and is spelled by hand here; the stub's name is derived from src/open.rs instead.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$dir/bin/$open_handoff"
    chmod +x "$dir/bin/$open_handoff"

    # Open in terminal ships switched off in the Menus section, so the menu half below says which
    # state it is driving instead of reading whatever the operator's own ui.json holds.
    local real_state="${XDG_STATE_HOME-}"
    seed_ui_state "$fixture_root/openterminal-state" "{\"menu\":{\"hidden\":$terminal_shown}}"
    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    flea_bin="$dir/bin/flea"
    launch "$dir"
    export PATH="$saved_path"
    flea_bin="$real_bin"
    # bin, opened.log, ran.log, target.txt.
    wait_listing 4

    # The pointer half: the context-menu row, which is the route SettingsMenus.html specifies and
    # the only one left now that the chrome carries no terminal button.
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Open in Terminal"* ]] \
        || fail "openterminal: the menu offers no terminal row, got $(ipc contextMenuEntries)"
    menu_seek "Open in Terminal"
    key -k Return >/dev/null
    # The cursor row is the folder bin, and a folder row's menu opens its terminal in that folder (FIX-373, v0.4.0).
    [[ "$(ipc rowAt "$(ipc cursor)")" == "bin|"* ]] || fail "openterminal: the menu half did not start on the folder bin, row is $(ipc rowAt "$(ipc cursor)")"
    wait_terminal "$ran" "$dir/bin" "the context menu on the folder bin"
    printf 'OPENTERMINAL menu log=%q\n' "$(cat "$ran")"

    # The keyboard half, from the list.
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    wait_terminal "$ran" "$dir" "ctrl+t in the list"

    # And from the rail, which owns its own keys and would otherwise swallow the chord.
    : > "$ran"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "openterminal: tab did not reach the rail"
    hotkey --global ctrl t flea >/dev/null
    wait_terminal "$ran" "$dir" "ctrl+t on the rail"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "openterminal: escape did not return to the list"

    # A second request while the first terminal is still starting is dropped, and the third, once it
    # has exited, is not: without the second half this check passes for a key that does nothing.
    # The drop is announced, because a swallowed keypress with nothing on screen is the defect.
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    settle
    hotkey --global ctrl t flea >/dev/null
    wait_message "Still opening the last terminal; try again in a moment."
    wait_terminal "$ran" "$dir" "the single-flight guard"
    sleep 1
    [[ "$(grep -c . "$ran")" == "1" ]] || fail "openterminal: two terminals were started, log is $(cat "$ran")"
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    wait_terminal "$ran" "$dir" "a request after the child exited"

    # Both at once: the terminal child is still sleeping when Enter opens a file, so a shared path or
    # a shared Process would show up as one of the two logs carrying the other's argument.
    : > "$ran"
    : > "$opened"
    hotkey --global ctrl t flea >/dev/null
    seek_row_named target.txt
    key -k Return >/dev/null
    local waited
    for waited in $(seq 1 100); do
        grep -q "^OPENED $dir/target.txt$" "$opened" && break
        sleep 0.05
    done
    grep -q "^OPENED $dir/target.txt$" "$opened" \
        || fail "openterminal: a file open during a terminal launch never reached $open_handoff, log is $(cat "$opened")"
    wait_terminal "$ran" "$dir" "the terminal launched beside a file open"
    printf 'OPENTERMINAL crossed terminal=%q opened=%q\n' "$(cat "$ran")" "$(cat "$opened")"
    [[ "$(grep -c . "$opened")" == "1" ]] || fail "openterminal: the opener ran twice, log is $(cat "$opened")"

    # A nonzero exit reaches the status line as one sentence. The stub is rewritten rather than
    # relaunched, because a fresh exec reads the file again.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = --terminal ] || exec %q "$@"\n' "$real_bin"
      printf 'exit 2\n'
    } > "$dir/bin/flea"
    chmod +x "$dir/bin/flea"
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    wait_message "No terminal on this system opened that folder."
    shot openterminal-failed
    [[ ! -s "$ran" ]] || fail "openterminal: the failing stub still logged $(cat "$ran")"

    printf 'OPENTERMINAL menu=ok list=ok rail=ok single-flight=ok crossed=ok failure=ok\n'
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    kill_flea
}

case_makeexec() {
    local dir="$fixture_root/makeexec"
    sandbox_scratch "$dir"
    printf '#!/bin/sh\necho built\n' > "$dir/build.sh"
    printf 'plain notes\n' > "$dir/notes.txt"
    chmod 644 "$dir/build.sh" "$dir/notes.txt"
    local real_state="${XDG_STATE_HOME-}"
    seed_ui_state "$fixture_root/makeexec-state" '{"view":"list","keys":"default"}'
    launch "$dir"
    wait_listing 2

    # build.sh: the row sits with Open, before Move to Trash's group (CTL2 2026-10-31); the menu-row board hides Add to Favorites on files.
    seek_row_named "build.sh"
    click_row "$(ipc cursor)" right
    local entries="" deadline=$(( $(date +%s%3N) + async_wait_ms ))
    while (( $(date +%s%3N) < deadline )); do
        entries=$(ipc contextMenuEntries)
        [[ "$entries" == *"Make Executable"* ]] && break
        sleep 0.1
    done
    printf 'MAKEEXEC build entries=%s\n' "$entries"
    [[ "$entries" == *"Make Executable"* ]] \
        || fail "makeexec: build.sh offers no Make Executable row, got $entries"
    [[ "${entries%%Make Executable*}" != *"Move to Trash"* && "${entries%%Make Executable*}" == "Open|"* ]] \
        || fail "makeexec: Make Executable is not in Open's group, before Move to Trash, in $entries"
    menu_seek "Make Executable"
    key -k Return >/dev/null
    local mode="" deadline2=$(( $(date +%s%3N) + async_wait_ms ))
    while (( $(date +%s%3N) < deadline2 )); do
        mode=$(stat -c '%a' "$dir/build.sh")
        [[ "$mode" == "744" ]] && break
        sleep 0.1
    done
    [[ "$(stat -c '%a' "$dir/build.sh")" == "744" ]] \
        || fail "makeexec: build.sh stayed $(stat -c '%a' "$dir/build.sh"), not 744"
    wait_message "Made it executable. · z undoes"
    key z >/dev/null
    local back="" deadline3=$(( $(date +%s%3N) + async_wait_ms ))
    while (( $(date +%s%3N) < deadline3 )); do
        back=$(stat -c '%a' "$dir/build.sh")
        [[ "$back" == "644" ]] && break
        sleep 0.1
    done
    [[ "$(stat -c '%a' "$dir/build.sh")" == "644" ]] \
        || fail "makeexec: undo left $(stat -c '%a' "$dir/build.sh"), not 644"
    wait_message "Undid the permissions"

    # notes.txt: the same menu with no shebang offers no such row.
    key -k Escape >/dev/null
    settle
    seek_row_named "notes.txt"
    local before_probe
    before_probe=$(ipc menuState | jq -er '.shebangId') || fail "makeexec: could not read the probe id"
    click_row "$(ipc cursor)" right
    makeexec_wait_shebang "$dir/notes.txt" "$before_probe"
    local plain=""
    plain=$(ipc contextMenuEntries)
    printf 'MAKEEXEC notes entries=%s\n' "$plain"
    [[ "$plain" != *"Make Executable"* ]] \
        || fail "makeexec: notes.txt offers Make Executable in $plain"
    key -k Escape >/dev/null
    settle
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
}

case_openwithdesign() (
    local menu_box listing config data state name frame flyout mime_before menus_checks=0 openwith_flyout_rows=0
    local -a numbers
    sandbox_require "$fixture_root"
    menu_box=$(mktemp -d "$fixture_root/openwith-design.XXXXXXXX") || fail 'openwith: owned fixture creation failed'
    printf 'native Open with fixture\n' > "$menu_box/.flea-test-sandbox"
    listing="$menu_box/listing"; config="$menu_box/config"; data="$menu_box/data"; state="$menu_box/state"
    for name in "$listing" "$config" "$data" "$data/applications"; do
        menus_guard "$name"
        mkdir -p "$name" || fail 'openwith: fixture directory creation failed'
    done
    # The fixture entry execs /bin/true, so an Open that reaches the launcher starts nothing.
    menus_guard "$data/applications/zzflea-openwith.desktop"
    printf '[Desktop Entry]\nType=Application\nName=Zzflea Fixture\nExec=/bin/true %%f\nIcon=text-x-generic\nMimeType=text/plain;\n' \
        > "$data/applications/zzflea-openwith.desktop"
    # gio resolves the id named below through the desktop database, and an applications directory
    # with no cache answers for nothing, so the default silently did not take.
    update-desktop-database "$data/applications" 2>/dev/null \
        || fail 'openwith: the fixture desktop database could not be built'
    # A default the desktop really reads back, so the board's "default" caption has something to name.
    menus_guard "$config/mimeapps.list"
    printf '[Default Applications]\ntext/plain=zzflea-openwith.desktop\n' > "$config/mimeapps.list"
    export XDG_CONFIG_HOME="$config" XDG_DATA_HOME="$data"
    # The system roots stay on the path: the shared mime database names the kind, and dropping it
    # would leave the eyebrow reading "text/plain" instead of the type's own name.
    export XDG_DATA_DIRS="$data:/usr/local/share:/usr/share"
    menus_guard "$listing/notes.txt"
    printf 'a note with real bytes in it\n' > "$listing/notes.txt"
    seed_ui_state "$state" '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"off"},"menu":{"hidden":[]}}'
    mime_before=$(stat -c %Y "$config/mimeapps.list")
    trap 'kill_flea' EXIT
    launch "$listing"
    wait_listing 1

    # Rule 2: the flyout is the parent's own width, flush against its right edge, its frame top on
    # the opening row's top. Rule 3: the default leads and carries the muted caption.
    openwith_open_flyout notes.txt
    frame=$(ipc menuState | jq -r .frame)
    flyout=$(ipc menuState | jq -r .flyout)
    read -r -a numbers <<< "$frame $flyout"
    [[ "${numbers[2]}" == "${numbers[6]}" ]] \
        || fail "openwith: the flyout is ${numbers[6]} wide, the menu is ${numbers[2]}"
    (( numbers[4] == numbers[0] + numbers[2] )) \
        || fail "openwith: the flyout starts at ${numbers[4]}, not flush at $(( numbers[0] + numbers[2] ))"
    menus_expect menuState '[.entries[] | select(.action == "openWith") | .submenu[]][0] | .id == "zzflea-openwith.desktop" and .hint == "default"' \
        'the desktop default leads the flyout and says so'
    menus_expect menuState '[.entries[] | select(.action == "openWith") | .submenu[]] | (.[-2].separator == true) and (.[-1].glyph == "app-window")' \
        'the tail row sits under its own separator with the app-window glyph'
    # Both rungs of AppLibrary.qml's ladder: the app and device index answers with a file where it
    # can, and a name it cannot place is passed through for the themed lookup rather than dropped.
    # The fixture names a mimetype icon on purpose, which is exactly what that index does not hold.
    menus_expect menuState '[.entries[] | select(.action == "openWith") | .submenu[] | .icon // empty] | any(startswith("/"))' \
        "an entry the app icon index places arrives as a file"
    menus_expect menuState '[.entries[] | select(.action == "openWith") | .submenu[] | select(.id == "zzflea-openwith.desktop")][0].icon == "text-x-generic"' \
        "a name that index cannot place is left for the themed lookup"
    shot openwith-flyout
    # Two: the first closes the flyout, the second the menu holding it open behind it.
    key -k Escape >/dev/null
    key -k Escape >/dev/null

    # Rules 4 and 5: the Convert popup family's width, two eyebrows naming the kind, and a list that
    # ends on a row boundary at seven applications plus the eyebrows standing over them.
    openwith_open_dialog notes.txt
    menus_expect openWithState '.kind == "Plain text document" and .name == "notes.txt"' 'the card names the file and its kind'
    menus_expect openWithState '[.rows[] | select(.eyebrow) | .eyebrow] == ["Registered for Plain text document", "All applications"]' \
        'both groups draw under their own eyebrow'
    menus_expect openWithState '.rows[1].id == "zzflea-openwith.desktop" and .rows[1].isDefault' 'the registered group leads with the default'
    # Assert HIG geometry from painted rects and live tokens, allowing one pixel of rounding (CTL2 10-31 15:1x).
    menus_expect openWithState '(.listRect | split(" ")[3] | tonumber) - (7 * .rowHeight + 2 * .eyebrowHeight + .eyebrowAir[0] + .eyebrowAir[1] + 2) | fabs <= 1' \
        'the list holds seven applications, both eyebrows with their air and two hairlines'
    menus_expect openWithState '(.rect | split(" ")[2] | tonumber) > 0' "the card takes the Convert family's own width"
    menus_expect openWithState '(.titleRuleRect | split(" ") | map(tonumber)) as $t | (.searchRect | split(" ") | map(tonumber)) as $f | $f[1] - ($t[1] + $t[3]) - .gap | fabs <= 1' \
        'the search field sits one gap under the title rule'
    menus_expect openWithState '(.searchRect | split(" ") | map(tonumber)) as $f | (.listRect | split(" ") | map(tonumber)) as $l | $l[1] - ($f[1] + $f[3]) - .searchTrail | fabs <= 1' \
        'the list starts one gap under the search field'
    menus_expect openWithState '(.listRect | split(" ") | map(tonumber)) as $l | (.eyebrowRects[0] | split(" ") | map(tonumber)) as $e | $e[1] - $l[1] - .eyebrowAir[0] | fabs <= 1' \
        'the first eyebrow stands its air below the list top'
    menus_expect openWithState '(.groupRuleRect | split(" ") | map(tonumber)) as $r | (.eyebrowRects[1] | split(" ") | map(tonumber)) as $e | ($r | length) == 4 and ($e[1] - ($r[1] + $r[3]) - .eyebrowAir[1] | fabs <= 1)' \
        'a hairline closes the registered group and the next eyebrow stands its air below it'
    shot openwith-dialog

    # Rule 5: the search filters both groups, and a group that matches nothing takes its eyebrow with it.
    openwith_search Zzflea
    menus_expect openWithState '[.rows[] | .label // .eyebrow] == ["Registered for Plain text document", "Zzflea Fixture", "All applications", "Zzflea Fixture"]' \
        'the search filters both groups and keeps both eyebrows'
    openwith_clear_search
    openwith_search zzNoMatchingApplication
    # HIG leaves no-match to v0.3.12: retain the viewport and centre its caption.
    menus_expect openWithState '(.rows | length) == 0 and ((.listRect | split(" ") | map(tonumber)) as $l | (.noMatchRect | split(" ") | map(tonumber)) as $n | ($l | length) == 4 and ($n | length) == 4 and ($l[3] - (7 * .rowHeight + 2 * .eyebrowHeight + .eyebrowAir[0] + .eyebrowAir[1] + 2) | fabs <= 1) and ($n[0] + $n[2]/2 - ($l[0] + $l[2]/2) | fabs <= 1) and ($n[1] + $n[3]/2 - ($l[1] + ($l[3]-1)/2) | fabs <= 1))' \
        'no-match keeps the viewport height and centres the caption above its closing rule'
    menus_expect openWithState '(.listRect | split(" ") | map(tonumber)) as $l | (.noMatchRect | split(" ") | map(tonumber)) as $n | $n[3] >= 30 and $n[0] >= $l[0] and $n[0] + $n[2] <= $l[0] + $l[2]' \
        'the long no-match caption wraps inside the viewport'
    menus_expect openWithState 'any(.controls[]; .name == "Open" and (.enabled | not)) and any(.controls[]; .name == "Cancel" and .enabled)' \
        'rule 6 dims Open and leaves Cancel live'
    shot openwith-nomatch
    openwith_clear_search

    # Rule 7: Space toggles the always box from an empty search line, and the write lands once.
    key -k space >/dev/null
    menus_expect openWithState '.always' 'Space toggles the always box'
    openwith_search Zzflea
    menus_expect openWithState '.cursor == 0 and .rows[1].id == "zzflea-openwith.desktop"' 'the cursor holds the only match'
    key -k Return >/dev/null
    menus_expect openWithState '.opened | not' 'Open closes the card'
    grep -q '^text/plain=zzflea-openwith.desktop' "$config/mimeapps.list" \
        || fail "openwith: the default was not written: $(cat "$config/mimeapps.list")"
    [[ "$(stat -c %Y "$config/mimeapps.list")" != "$mime_before" ]] \
        || fail 'openwith: mimeapps.list was never rewritten'
    menus_equal 'the card restores listing focus' list "$(ipc focusView)"
    printf 'OPENWITH flyout=flush default=first card=630 list=315 write=own-config\n'
)
