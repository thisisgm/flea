# GPUI overrides for UISH-10; seeded from the unchanged oracle at 2faf8d099.
case_editplace() {
    local dir="$fixture_root/editplace"
    sandbox_scratch "$dir"
    : > "$dir/one.txt"
    local fake_root="$fixture_root/editplace-fake"
    sandbox_scratch "$fake_root"
    mkdir -p "$fake_root/bin"
    local fixture_home="$fixture_root/editplace-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME" mount_log="$fake_root/mount.log"
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    export XDG_CONFIG_HOME="$fixture_home/.config"
    seed_ui_state "$fixture_root/editplace-state" '{"view":"list","keys":"default","places":{"favourites":[]}}'
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf 'smb://legacy.test/data Legacy share\nfile:///missing/legacy Local legacy\n' > "$bookmarks"
    : > "$mount_log"

    # The same shape case_network's own fake has: this box's real mounts are the operator's, so the
    # rail under test is exactly the two seeded lines and nothing the box happens to have open.
    cat > "$fake_root/bin/gio" <<EOS
#!/bin/sh
$gio_stub_args
case "\$1 \${2:-}" in
"mount -li") exit 0 ;;
"mount "*) printf '%s\n' "\$*" > "$mount_log" ;;
"info "*) printf 'local path: %s\n' "$dir" ;;
*) exit 0 ;;
esac
EOS
    chmod +x "$fake_root/bin/gio"
    local saved_path="$PATH"
    export PATH="$fake_root/bin:$PATH"

    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 1
    local _attempt
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == 'Legacy share|network|share|false' ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == 'Legacy share|network|share|false' ]] \
        || fail "editplace: the rail reads $(ipc networkEntries), not the one saved place"

    local legacy_index
    legacy_index=$(ipc railEntries | jq -r 'map(.label) | index("Legacy share")')
    [[ -n "$legacy_index" && "$legacy_index" != "null" ]] || fail "editplace: the saved place is not on the rail"

    click_rail_row "$legacy_index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Rename|Edit Address…|-|Remove from Network" ]] \
        || fail "editplace: the saved place offers $(ipc contextMenuEntries), not Rename, Edit address and Remove from Network"
    menu_seek "Edit address"
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "editplace: Edit opened no dialog"
    [[ "$(ipc networkUri)" == "smb://legacy.test/data" ]] \
        || fail "editplace: the dialog opened on $(ipc networkUri), not the place Edit named"
    # The host holds the caret on open, so the old one is taken out and the corrected one typed in.
    local back
    for back in $(seq 1 24); do key -k BackSpace >/dev/null; done
    key "legacy2.test" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "smb://legacy2.test/data" ]] \
        || fail "editplace: the Mounts-as line reads $(ipc networkUri) after the address was corrected"
    key -k Return >/dev/null
    for _attempt in $(seq 1 200); do
        [[ "$(ipc dialogOpen)" == "false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "editplace: the dialog stayed open, it says $(ipc networkStatus)"
    # SMB mounts go through --anonymous, the same argv ui/NetworkMounts.qml sends for every share.
    [[ "$(cat "$mount_log")" == 'mount --anonymous smb://legacy2.test/data' ]] \
        || fail "editplace: the edit mounted $(cat "$mount_log")"
    local edited_marks
    edited_marks=$(cat "$bookmarks")
    [[ "$edited_marks" == 'smb://legacy2.test/data Legacy share
file:///missing/legacy Local legacy' ]] \
        || fail "editplace: the bookmarks file reads $edited_marks after the edit"
    # The rail reads the file it was rewritten in, so the place is one row carrying the new address.
    local rail_uri
    for _attempt in $(seq 1 100); do
        rail_uri=$(ipc railEntries | jq -r '[.[] | select(.group == "network")] | map(.uri) | join(",")')
        [[ "$rail_uri" == "smb://legacy2.test/data" ]] && break
        sleep 0.05
    done
    [[ "$rail_uri" == "smb://legacy2.test/data" ]] \
        || fail "editplace: the rail's network rows are $rail_uri, not the one corrected place"
    # The edit rewrites the place it came from, so Flea's own favourites gain nothing.
    [[ "$(ipc railEntries | jq -r '[.[] | select(.group == "favorite")] | length')" == "0" ]] \
        || fail "editplace: the edit added a favourite as well as rewriting the place"
    printf 'EDITPLACE edit=%s rail=%s\n' "$(head -1 "$bookmarks")" "$rail_uri"

    echo "-- an Edit nobody finished leaves the place alone when the next place is saved --"
    legacy_index=$(ipc railEntries | jq -r 'map(.label) | index("Legacy share")')
    [[ -n "$legacy_index" && "$legacy_index" != "null" ]] \
        || fail "editplace: the edited place left the rail, which carries $(ipc railEntries)"
    click_rail_row "$legacy_index" right
    settle
    menu_seek "Edit address"
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "editplace: the second Edit opened no dialog"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "editplace: Escape left the dialog open"
    # Without the dialog's own disarm this next mount rewrites the line the abandoned Edit armed.
    rail_focus
    key a >/dev/null
    settle
    key "other.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/share" >/dev/null
    key -k Return >/dev/null
    for _attempt in $(seq 1 200); do
        [[ "$(ipc dialogOpen)" == "false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "editplace: the second place never saved, it says $(ipc networkStatus)"
    [[ "$(cat "$mount_log")" == 'mount --anonymous smb://other.test/share' ]] \
        || fail "editplace: the second place mounted $(cat "$mount_log")"
    settle
    [[ "$(cat "$bookmarks")" == "$edited_marks" ]] \
        || fail "editplace: the abandoned Edit rewrote the saved place, the file now reads $(cat "$bookmarks")"
    printf 'EDITPLACE abandoned=unchanged marks=%s\n' "$(head -1 "$bookmarks")"

    export PATH="$saved_path"
    kill_flea
}

case_placemenu() {
    local dir="$fixture_root/placemenu"
    sandbox_scratch "$dir"
    mkdir -p "$dir/Work"
    : > "$dir/Work/one.txt"
    : > "$dir/plain.txt"
    local state="$fixture_root/placemenu-state"
    # The place menu on over one favourite, with Open in terminal and Copy path (the Copy as switch) on: a row switched off in Settings is off on this menu too.
    seed_ui_state "$state" "$(printf '{"menu":{"hidden":["delete","moveto","copyto","properties","permissions","runScript","pasteAs","invertSelection","extThumbs"]},"places":{"favourites":[{"label":"Work","path":"%s/Work"}]}}' "$dir")"

    launch "$dir"
    wait_listing 2
    local favourite_index
    favourite_index=$(ipc railEntries | jq -r 'map(.label) | index("Work")')
    [[ -n "$favourite_index" && "$favourite_index" != "null" ]] \
        || fail "placemenu: the seeded favourite is not on the rail, which carries $(ipc railEntries)"

    echo "-- a Favorites row ends on Remove, and a Places row on Add --"
    click_rail_row "$favourite_index" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "placemenu: the favourite's right click opened no menu"
    [[ "$(ipc contextMenuEntries)" == "Open|Open in New Tab|-|Open in Terminal|Copy Path|Remove from Favorites" ]] \
        || fail "placemenu: the favourite offers $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    for _attempt in $(seq 1 20); do
        [[ "$(ipc contextMenuVisible)" == "false" ]] && break
        sleep 0.25
    done
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "placemenu: escape left the favourite's menu open"
    local home_index
    home_index=$(ipc railEntries | jq -r 'map(.label) | index("Home")')
    [[ -n "$home_index" && "$home_index" != "null" ]] \
        || fail "placemenu: the rail has no Home row, it carries $(ipc railEntries | jq -r 'map(.label) | join(",")')"
    click_rail_row "$home_index" right
    settle
    # Read visible before entries: the menu keeps its last rows, so a row that opens nothing would
    # otherwise answer with the menu before it, which is exactly how this case first read green.
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "placemenu: the Home row's right click opened no menu"
    [[ "$(ipc contextMenuEntries)" == "Open|Open in New Tab|-|Open in Terminal|Copy Path|Add to Favorites" ]] \
        || fail "placemenu: the Home row offers $(ipc contextMenuEntries)"

    echo "-- and a row acts on its own path, not on the listing's cursor --"
    local tabs_before
    tabs_before=$(ipc tabCount)
    menu_seek "Open in New Tab"
    key -k Return >/dev/null
    for _attempt in $(seq 1 40); do
        [[ "$(ipc tabCount)" == "$((tabs_before + 1))" ]] && break
        sleep 0.25
    done
    [[ "$(ipc tabCount)" == "$((tabs_before + 1))" ]] \
        || fail "placemenu: New tab left $(ipc tabCount) tabs"
    wait_path "$(window_home)"
    printf 'PLACEMENU tabs=%s path=%s labels=%s\n' "$(ipc tabCount)" "$(ipc path)" "$(ipc tabLabels)"

    echo "-- with the switch off it is the menu it was --"
    kill_flea
    seed_ui_state "$fixture_root/placemenu-off" "$(printf '{"places":{"favourites":[{"label":"Work","path":"%s/Work"}]}}' "$dir")"
    launch "$dir"
    wait_listing 2
    favourite_index=$(ipc railEntries | jq -r 'map(.label) | index("Work")')
    click_rail_row "$favourite_index" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "placemenu: with the switch off the favourite opened no menu"
    [[ "$(ipc contextMenuEntries)" == "Remove from Favorites" ]] \
        || fail "placemenu: with the switch off the favourite offers $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle
    local home_off
    home_off=$(ipc railEntries | jq -r 'map(.label) | index("Home")')
    click_rail_row "$home_off" right
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] \
        || fail "placemenu: with the switch off the Home row opened $(ipc contextMenuEntries)"
    printf 'PLACEMENU off=%s\n' "$(ipc contextMenuVisible)"
    kill_flea
}

case_rename() {
    local dir="$fixture_root/rename"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/0-one.txt"

    printf 'Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n' > "$dir/bin/gio-out"
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
case "\$1 \$2" in
  "mount -li") cat "$dir/bin/gio-out"; exit 0 ;;
esac
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/rename-home"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    printf 'smb://192.168.1.10/data NAS\n' > "$bookmarks"
    local real_home="$HOME" saved_path="$PATH"

    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/ is the gio stub's own fixture entry, alongside the one file under test.
    wait_listing 2
    wait_rail 3
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"NAS|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"NAS|network|share|false" ]] \
        || fail "rename: unexpected starting entries, got $(ipc networkEntries)"

    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "rename: Tab did not reach the rail"

    # The rail also carries Trash and this box's own devices, so the bookmark-only entry is sought
    # by its label rather than counted: j from the top until the cursor row says NAS.
    rail_seek NAS

    # F2 starts the field pre-filled and pre-selected; typing replaces the whole label.
    key -k F2 >/dev/null
    settle
    [[ "$(ipc railRenamingIndex)" == "$(ipc railCursor)" ]] \
        || fail "rename: F2 did not start renaming on the cursor row, railRenamingIndex is $(ipc railRenamingIndex)"
    shot rename-editing

    # Escape cancels first, proving it before the real rename below: no write, state unwound.
    key "garbage" >/dev/null
    key -k Escape >/dev/null
    settle
    [[ "$(ipc railRenamingIndex)" == "-1" ]] || fail "rename: Escape did not close the field"
    [[ "$(cat "$bookmarks")" == "smb://192.168.1.10/data NAS" ]] \
        || fail "rename: Escape wrote to the bookmarks file, it now reads: $(cat "$bookmarks")"

    # The real rename: a line-scoped label rewrite, byte-identical apart from that one field.
    key -k F2 >/dev/null
    settle
    key "Homelab" >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc railRenamingIndex)" == "-1" ]] || fail "rename: Return did not close the field"
    [[ "$(cat "$bookmarks")" == "smb://192.168.1.10/data Homelab" ]] \
        || fail "rename: the bookmarks line was not rewritten in place, it now reads: $(cat "$bookmarks")"
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"Homelab|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"Homelab|network|share|false" ]] \
        || fail "rename: the rail did not pick up the new label, got $(ipc networkEntries)"
    shot rename-relabelled

    # An empty submitted name reverts: no write, the previous label stands. The field selects its
    # whole text on focus (see ui/SidebarRow.qml), so one Backspace clears it.
    key -k F2 >/dev/null
    settle
    key -k Backspace >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(cat "$bookmarks")" == "smb://192.168.1.10/data Homelab" ]] \
        || fail "rename: an empty submit changed the file, it now reads: $(cat "$bookmarks")"

    # isos has no bookmark line at all yet: renaming it must create one, not fail silently.
    key k >/dev/null
    settle
    [[ "$(ipc railLabel "$(ipc railCursor)")" == "isos" ]] \
        || fail "rename: k did not reach isos, the cursor row is $(ipc railLabel "$(ipc railCursor)")"
    key -k F2 >/dev/null
    settle
    key "ISOs Archive" >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(cat "$bookmarks")" == "smb://192.168.1.10/data Homelab"$'\n'"smb://192.168.1.10/isos ISOs Archive" ]] \
        || fail "rename: a mount-only entry did not gain a bookmark line, file now reads: $(cat "$bookmarks")"
    # PR #21's rule, which this case used to assert the other way round: the operator's own name wins
    # on the live row too, or the rename just typed is written to the file and never drawn again.
    # The live mount still wins the row itself, and its "true" here says so; see ui/js/Mounts.js
    # "railLabel" and ui/NetworkMounts.qml "rebuild".
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "ISOs Archive|network|share|true"$'\n'"Homelab|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "ISOs Archive|network|share|true"$'\n'"Homelab|network|share|false" ]] \
        || fail "rename: a mounted entry did not take the name just typed, got $(ipc networkEntries)"

    # A poll that finds the same shares must hand the Repeater nothing, or every rail row rebinds on
    # a five second timer and an open editor loses what was typed into it.
    key -k F2 >/dev/null
    settle
    key "Surviving" >/dev/null
    settle
    [[ "$(ipc railRenameEditorText)" == "Surviving" ]] \
        || fail "rename: the rail editor holds $(ipc railRenameEditorText), not what was typed"
    sleep "$rail_poll_wait_s"
    [[ "$(ipc railRenameEditorText)" == "Surviving" ]] \
        || fail "rename: the mount poll emptied the rail editor, it now holds $(ipc railRenameEditorText)"
    [[ "$(ipc railRenameFieldShown)" == "true" ]] || fail "rename: the mount poll closed the rail editor"

    # A poll that finds a DIFFERENT share at that position is a different row under the editor, so
    # the rename is void: it used to stand there empty, one keystroke from relabelling the new share.
    printf 'Mount(0): photos on 192.168.1.10 -> smb://192.168.1.10/photos/\n' > "$dir/bin/gio-out"
    for _attempt in $(seq 1 $((rail_poll_wait_s * 20))); do
        [[ "$(ipc networkEntries)" == "photos|"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "photos|"* ]] \
        || fail "rename: the rail never picked up the swapped share, got $(ipc networkEntries)"
    [[ "$(ipc railRenamingIndex)" == "-1" ]] \
        || fail "rename: a swapped share left the rail editor open over it, index $(ipc railRenamingIndex)"
    [[ "$(ipc railRenameEditorLive)" == "false" ]] || fail "rename: a rail editor is live over a share nobody chose"
    # Put the stub back: the relaunch below asserts the share list this case started with.
    printf 'Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n' > "$dir/bin/gio-out"
    printf 'RENAME poll-survives=ok swap-closes=ok\n'

    kill_flea

    # Persistence across a real relaunch: the whole point of writing to the bookmarks file at all.
    export HOME="$fixture_home"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 2
    wait_rail 3
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "ISOs Archive|network|share|true"$'\n'"Homelab|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "ISOs Archive|network|share|true"$'\n'"Homelab|network|share|false" ]] \
        || fail "rename: the label did not survive a relaunch, got $(ipc networkEntries)"

    printf 'RENAME relabel=ok escape=ok empty=ok create-bookmark=ok persists=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
}

case_recent() {
    local dir="$fixture_root/recent"
    sandbox_scratch "$dir"
    printf 'first file\n' > "$dir/alpha.txt"
    printf 'second file\n' > "$dir/beta.txt"
    local deep="$dir/a-very-long-parent-directory/another-long-parent-directory/deeply-nested-recent-location"
    mkdir -p "$deep"
    printf 'third file\n' > "$deep/gamma.txt"

    local fixture_home="$fixture_root/recent-home"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.local/share"
    cat > "$fixture_home/.local/share/recently-used.xbel" <<EOS
<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0">
  <bookmark href="file://$deep/gamma.txt" added="2026-09-25T10:00:00Z" modified="2026-09-25T10:00:00Z" visited="2026-09-25T10:00:00Z"/>
  <bookmark href="file://$dir/beta.txt" added="2026-09-26T10:00:00Z" modified="2026-09-26T10:00:00Z" visited="2026-09-26T10:00:00Z"/>
  <bookmark href="file://$dir/alpha.txt" added="2026-09-26T09:00:00Z" modified="2026-09-26T09:00:00Z" visited="2026-09-27T10:00:00Z"/>
</xbel>
EOS
    # Only visited favors alpha: beta leads in file order, added, modified and mtime alike.
    touch -d '2026-09-26 10:00:00' "$dir/beta.txt"
    touch -d '2026-09-26 09:00:00' "$dir/alpha.txt"
    touch -d '2026-09-25 10:00:00' "$deep/gamma.txt"

    local state="$fixture_root/recent-state"
    seed_ui_state "$state" '{"places":{"showRecent":true}}'
    # GPUI reads the history from XDG_DATA_HOME, so the fixture HOME's own data folder is passed for this launch.
    local real_home="$HOME" real_data="${XDG_DATA_HOME-}"
    export HOME="$fixture_home" XDG_DATA_HOME="$fixture_home/.local/share"
    launch "$dir"
    export HOME="$real_home"
    if [[ -n "$real_data" ]]; then export XDG_DATA_HOME="$real_data"; else unset XDG_DATA_HOME; fi
    wait_listing 3
    local labels=
    for _attempt in $(seq 1 100); do
        labels=$(ipc railLabels 2>/dev/null || printf unavailable)
        if [[ "|$labels|" == *"|Recent|"* ]]; then break; fi
        sleep 0.05
    done
    [[ "|$labels|" == *"|Recent|"* ]] \
        || fail "recent: the rail has no Recent row: $labels"

    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "recent: Tab did not reach the rail"
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "recent: Recent is not the second rail row"
    key -k Return >/dev/null
    local mode= total=
    for _attempt in $(seq 1 200); do
        mode=$(ipc recentMode 2>/dev/null || printf unavailable)
        total=$(ipc total 2>/dev/null || printf unavailable)
        if [[ "$mode" == "results" && "$total" == "3" && "$(ipc listInFlight 2>/dev/null)" == "false" ]]; then break; fi
        sleep 0.05
    done
    [[ "$mode" == "results" ]] || fail "recent: Enter on the rail row never listed the history"
    [[ "$total" == "3" ]] || fail "recent: the history listed $total rows, not 3"
    [[ "$(ipc headerTitles)" == "Name|Location|Size|Used" ]] \
        || fail "recent: the header does not read Name|Location|Size|Used: $(ipc headerTitles)"
    [[ "$(ipc rowAt 0)" == *"/alpha.txt|file|"* ]] \
        || fail "recent: the newest visited bookmark is not first: $(ipc rowAt 0)"
    [[ "$(ipc rowAt 1)" == *"/beta.txt|file|"* ]] \
        || fail "recent: the older visited bookmark is not second: $(ipc rowAt 1)"
    [[ "$(ipc rowAt 2)" == *"${deep#/}/gamma.txt|file|"* ]] \
        || fail "recent: the oldest visited bookmark with a deep location is not third: $(ipc rowAt 2)"
    shot recent-listing

    key -k Escape >/dev/null
    wait_path "$dir"
    [[ "$(ipc recentMode)" == "" ]] || fail "recent: Esc left the mode standing"

    printf 'RECENT listing=newest-first used-mark=ok esc-returns=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
}
