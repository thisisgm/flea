# GPUI overrides for UISH-12; seeded from the unchanged oracle at 2faf8d099.
case_networktimeout() {
    local dir="$fixture_root/network-timeout"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/one.txt"
    : > "$dir/two.txt"
    local calls="$dir/calls"
    printf '0\n' > "$calls"

    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
$gio_stub_args
if [ "\$1 \$2" != "mount -li" ]; then
  exec /usr/bin/gio "\$@"
fi
count=\$(cat "$calls")
count=\$((count + 1))
printf '%s\n' "\$count" > "$calls"
case "\$count" in
  1) printf 'Mount(0): First -> smb://stub/first\n' ;;
  2) exec sleep 20 ;;
  *) printf 'Mount(0): Recovered -> smb://stub/recovered\n' ;;
esac
EOS
    chmod +x "$dir/bin/gio"
    # The credentialed leg exits with the status file's code, so one stub drives both connect failure sentences.
    cat > "$dir/bin/flea-gio-auth" <<EOS
#!/bin/sh
[ "\$#" -eq 1 ] || exit 2
IFS= read -r secret || exit 3
[ -n "\$secret" ] || exit 4
printf 'uri=%s stdin-lines=1\n' "\$1" >> "$dir/auth-calls"
code=\$(cat "$dir/auth-status")
exit "\$code"
EOS
    chmod +x "$dir/bin/flea-gio-auth"
    : > "$dir/auth-calls"
    printf '124\n' > "$dir/auth-status"

    local fixture_home="$fixture_root/network-timeout-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME"
    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    export FLEA_GIO_AUTH="$dir/bin/flea-gio-auth"
    launch "$dir"
    export HOME="$real_home"

    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "First|network|share|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "First|network|share|true" ]] \
        || fail "networktimeout: first listing never appeared, got $(ipc networkEntries)"
    sleep "$rail_poll_wait_s"
    [[ "$(ipc networkEntries)" == "First|network|share|true" ]] \
        || fail "networktimeout: a wedged listing erased the last good row"

    for _attempt in $(seq 1 220); do
        [[ "$(ipc networkEntries)" == "Recovered|network|share|true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc networkEntries)" == "Recovered|network|share|true" ]] \
        || fail "networktimeout: polling never recovered, got $(ipc networkEntries)"
    [[ "$(cat "$calls")" -ge 3 ]] || fail "networktimeout: expected three list attempts"

    printf '%s\n' 'flea-gui: device probe gio mount -li failed: timed out after 10000 ms' >> "$expected_warnings"
    printf 'NETWORKTIMEOUT retained=ok retry=ok\n'

    # One connect failure sentence per stub status, each shot with its draft held.
    rail_focus
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "networktimeout: a did not open the add-location dialog"
    key "fail.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "data" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "tester" >/dev/null
    key -k Tab >/dev/null
    settle
    [[ "$(ipc networkFocus)" == "Password" ]] \
        || fail "networktimeout: Password did not follow Username, focus is $(ipc networkFocus)"
    printf '%s' "fixture-password" | omarchy-drive key --window com.thisisgm.flea - >/dev/null
    [[ "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networktimeout: typed password is not masked"
    key -k Return >/dev/null
    wait_network_status "Could not connect: the server did not respond." 20
    [[ "$(cat "$dir/auth-calls")" == "uri=smb://tester@fail.test/data stdin-lines=1" ]] \
        || fail "networktimeout: the 124 sentence is not one credentialed call, got $(cat "$dir/auth-calls")"
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "networktimeout: the failed connect discarded its draft"
    shot network-connect-failed
    # GPUI consumes the Secret on each mount attempt, so the retry receives a fresh field value.
    if [[ "$test_frontend" == gpui && "$(ipc networkPasswordState)" == "masked|empty" ]]; then
        local retry_step
        for retry_step in $(seq 1 12); do
            [[ "$(ipc networkFocus)" == Password ]] && break
            key -k Tab >/dev/null
            settle
        done
        [[ "$(ipc networkFocus)" == Password ]] || fail "networktimeout: retry could not focus Password"
        printf '%s' "fixture-password" | omarchy-drive key --window com.thisisgm.flea - >/dev/null
        [[ "$(ipc networkPasswordState)" == "masked|set" ]] || fail "networktimeout: retry password was not accepted"
    fi
    printf '60\n' > "$dir/auth-status"
    key -k Return >/dev/null
    wait_network_status "Could not connect: the network location could not be opened." 20
    [[ "$(wc -l < "$dir/auth-calls")" -eq 2 ]] \
        || fail "networktimeout: the second sentence is not a second credentialed call"
    [[ "$(tail -n 1 "$dir/auth-calls")" == "uri=smb://tester@fail.test/data stdin-lines=1" ]] \
        || fail "networktimeout: the retry called $(tail -n 1 "$dir/auth-calls"), not the credentialed URI"
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "networktimeout: the second failed connect discarded its draft"
    shot network-connect-failed-open
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "networktimeout: Escape did not close the failed dialog"
    printf 'NETWORKTIMEOUT connect-failure=124-then-open\n'
    unset FLEA_GIO_AUTH
    export PATH="$saved_path"
    kill_flea
    sandbox_remove "$fixture_home"
}

case_gvfs() {
    local dir="$fixture_root/gvfs-local"
    sandbox_scratch "$dir"
    : > "$dir/one.txt"
    : > "$dir/two.txt"
    local share_dir="$fixture_root/gvfs-share"
    local fixture_home="$fixture_root/gvfs-home"
    fixture_home_make "$fixture_home"
    FLEA_GVFS_CASE_DIR="$share_dir"
    FLEA_GVFS_CASE_HOME="$fixture_home"
    trap case_gvfs_cleanup EXIT HUP INT TERM
    local real_home="$HOME"
    local uri local_path

    uri=$(DIR="$share_dir" ./tools/flea-gvfs-fixture make)
    DIR="$share_dir" ./tools/flea-gvfs-fixture mount
    local_path=$(gio info "$uri" | sed -n 's/^local path: //p')
    [[ "$local_path" == "/run/user/$(id -u)/gvfs/"* ]] \
        || fail "gvfs: fixture has no FUSE path, got $local_path"

    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 2
    wait_rail 2
    # The rail count is satisfied by Home and Trash, and ui/NetworkMounts.qml polls mounts every five
    # seconds, so the fixture's own row is waited for. The operator's own shares sit on this rail too
    # and are none of this case's business, which is why the row is looked for rather than counted.
    for _attempt in $(seq 1 300); do
        [[ "$(ipc networkEntries)" == *"share.zip|network|share|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == *"share.zip|network|share|true"* ]] \
        || fail "gvfs: live mount never appeared, the rail carries $(ipc networkEntries)"

    key -k Tab >/dev/null
    key g >/dev/null
    settle
    rail_seek share.zip
    key -k Return >/dev/null
    wait_path "$local_path"
    wait_listing 2
    [[ "$(ipc rowAt 0)" == "alpha.txt|"* || "$(ipc rowAt 1)" == "alpha.txt|"* ]] \
        || fail "gvfs: alpha.txt absent from Flea listing"

    key -k Escape >/dev/null
    open_row alpha.txt
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "text" ]] \
        || fail "gvfs: alpha.txt did not open as a text preview"
    key -k Escape >/dev/null
    key -k Tab >/dev/null
    key g >/dev/null
    key -k Return >/dev/null
    wait_path "$fixture_home"

    click_rail_row "$(rail_row_of share.zip)" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount|-|Rename|Edit Address…|-|Remove from Network" ]] \
        || fail "gvfs: mounted share menu is $(ipc contextMenuEntries)"
    menu_seek Unmount
    key -k Return >/dev/null
    wait_message "Unmounted share.zip"
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" != *"share.zip"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" != *"share.zip"* ]] \
        || fail "gvfs: the row survived unmount, the rail carries $(ipc networkEntries)"
    ! gio mount -l | grep -Fq -- "-> $uri" || fail "gvfs: GIO mount survived Flea unmount"

    printf 'GVFS rail=ok browse=ok preview=ok unmount=ok\n'
    case_gvfs_cleanup
    trap - EXIT HUP INT TERM
}

case_unmount() {
    local dir="$fixture_root/unmount"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"

    local unmount_log="$dir/unmount.log"
    : > "$unmount_log"
    local share_uri="smb://stubhost/stubshare/"
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
$gio_stub_args
case "\$1 \$2" in
  "mount -li")
    # gvfsd composes this label and translates the word between share and host, so the stub speaks
    # Spanish here whatever the client locale is. What that proves is that the parser is robust to a
    # translated connector, and nothing about the C pin: live matrix step 0b ran this case in both
    # arms of the pin and it passed in both, because ui/js/Mounts.js's regex never reads the
    # connector word, ui/js/Protocols.js "shareName" strips the host plus one word in any language,
    # and this stub answers no gio info, so localPath() is empty either way. case_sharebrowser is
    # where the pin reddens, measured twice in the same step.
    printf 'Mount(0): stubshare en stubhost -> $share_uri\n  Type: GDaemonMount\n'
    exit 0
    ;;
  "mount -u")
    printf 'UNMOUNT %s\n' "\$3" >> "$unmount_log"
    exit 0
    ;;
esac
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/unmount-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    # Decided 2026-09-08: the machine's rail row is the bare hostname, no device suffix, so it fits at the body size.
    [[ "|$(ipc railLabels)|" == *"|$(cat /etc/hostname | tr -d '[:space:]')|"* && "$(ipc railLabels)" != *" · "* ]] \
        || fail "unmount: the rail's machine row is not the bare hostname: $(ipc railLabels)"
    export PATH="$saved_path"
    # bin/ and unmount.log are the gio stub's own fixture entries, alongside the two files under test.
    wait_listing 4
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] \
        || fail "unmount: the stub mount never appeared live, got $(ipc networkEntries)"
    [[ "$(ipc themeLoaded)" == "true" ]] || fail "unmount: the fixture home did not load a real theme"

    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "unmount: Tab did not reach the rail"

    # Right click raises the menu over the row and nothing else: the release row first, then the three
    # rows the saved place itself owns, and no unmount has run. The old two-right-click arm is gone,
    # see ui/Sidebar.qml "openRailMenu" and ui/js/Mounts.js "rowMenu".
    click_rail_row "$(rail_row_of stubshare)" right
    settle
    printf 'UNMOUNT menu visible=%s entries=%s glyphs=%s\n' \
        "$(ipc contextMenuVisible)" "$(ipc contextMenuEntries)" "$(ipc contextMenuGlyphs)"
    shot unmount-menu
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "unmount: right click opened no menu on the share"
    [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount|-|Rename|Edit Address…|-|Remove from Network" ]] \
        || fail "unmount: the share's menu is $(ipc contextMenuEntries), not Open, Unmount, Rename, Edit address then Remove from Network"
    [[ "$(ipc contextMenuGlyphs)" == "folder-open|-|drive|-|rename|sliders|-|minus" ]] \
        || fail "unmount: the share's rows draw $(ipc contextMenuGlyphs), not folder-open, drive, rename, sliders and minus"
    [[ -z "$(cat "$unmount_log")" ]] || fail "unmount: opening the menu already unmounted: $(cat "$unmount_log")"

    # Escape closes it and still nothing has run, which is what makes the menu the confirmation.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "unmount: Escape did not close the rail menu"
    [[ -z "$(cat "$unmount_log")" ]] || fail "unmount: Escape unmounted anyway: $(cat "$unmount_log")"

    # Choosing the row is what unmounts, and the row's key is what says which share, not its index.
    click_rail_row "$(rail_row_of stubshare)" right
    settle
    menu_seek Unmount
    key -k Return >/dev/null
    wait_message "Unmounted stubshare"
    [[ "$(cat "$unmount_log")" == "UNMOUNT ${share_uri%/}" ]] \
        || fail "unmount: the menu row did not unmount $share_uri, log is: $(cat "$unmount_log")"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "unmount: the menu stayed open after its action ran"

    # A rail row with nothing to release opens no menu at all, rather than an empty frame. Home is
    # the one favourite this fixture home has, and it is not a mount.
    click_rail_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] \
        || fail "unmount: a favourite opened $(ipc contextMenuEntries)"

    # The one instance is shared with the listing, so the keyboard must come back to it afterwards:
    # a second ContextMenu in this tree once killed every key in the window, see AGENTS.md.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "chrome" ]] || fail "unmount: Tab out of the rail left the keyboard on $(ipc focusView) instead of the bar"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "unmount: Tab did not reach the list after the rail menu closed"
    local before after
    before=$(ipc cursor)
    key j >/dev/null
    settle
    after=$(ipc cursor)
    [[ "$after" != "$before" ]] || fail "unmount: the list stopped taking keys after the rail menu, cursor stuck at $before"

    # PR #21's Remove row, driven at last: three of the states ui/NetworkMounts.qml "forget" answers
    # for, each with its own sentence. This home has no bookmarks file, so the live share is unsaved.
    click_rail_row "$(rail_row_of stubshare)" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount|-|Rename|Edit Address…|-|Remove from Network" ]] \
        || fail "unmount: the share's menu is $(ipc contextMenuEntries) before Remove"
    menu_seek "Remove from Network"
    key -k Return >/dev/null
    wait_message "stubshare is not saved, and stays in the sidebar until it is unmounted"
    [[ ! -e "$fixture_home/.config/gtk-3.0/bookmarks" ]] \
        || fail "unmount: Remove on a place nothing saved wrote a bookmarks file"
    [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] \
        || fail "unmount: Remove took a live mount off the rail, got $(ipc networkEntries)"

    printf 'UNMOUNT menu=ok escape=ok fire=ok no-menu-on-favourite=ok keyboard=ok unsaved=ok\n'
    kill_flea

    # The next two need a saved place, so the file goes in before the launch that reads it: one line
    # for the share the stub reports live, one for a place nothing mounts.
    mkdir -p "$fixture_home/.config/gtk-3.0"
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    printf 'smb://stubhost/stubshare Saved Share\nsmb://stubhost/ghost Ghost Place\n' > "$bookmarks"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 4
    wait_rail 3
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "Saved Share|network|share|true"$'\n'"Ghost Place|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "Saved Share|network|share|true"$'\n'"Ghost Place|network|share|false" ]] \
        || fail "unmount: the two saved places did not reach the rail, got $(ipc networkEntries)"

    # Mounted and saved: the one line goes, the row stays as the live mount it still is, and its name
    # falls back to gio's own because the bookmark that was winning it is gone. The stub reports the
    # share live whatever happens, so the rail cannot show a Remove that also unmounted; the log the
    # stub keeps is the only thing that can, and it already carries the deliberate unmount above.
    local unmount_log_before
    unmount_log_before=$(cat "$unmount_log")
    click_rail_row "$(rail_row_of 'Saved Share')" right
    settle
    menu_seek "Remove from Network"
    key -k Return >/dev/null
    wait_message "Saved Share is forgotten, and stays in the sidebar until it is unmounted"
    [[ "$(cat "$bookmarks")" == "smb://stubhost/ghost Ghost Place" ]] \
        || fail "unmount: Remove did not drop just its own line, the file reads: $(cat "$bookmarks")"
    [[ "$(cat "$unmount_log")" == "$unmount_log_before" ]] \
        || fail "unmount: Remove also unmounted the share, the log now reads: $(cat "$unmount_log")"
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "stubshare|network|share|true"$'\n'"Ghost Place|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "stubshare|network|share|true"$'\n'"Ghost Place|network|share|false" ]] \
        || fail "unmount: the live row kept a name nothing saves any more, got $(ipc networkEntries)"

    # Saved and nothing mounted: the line and the row both go, and no unmount clause is offered.
    click_rail_row "$(rail_row_of 'Ghost Place')" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Rename|Edit Address…|-|Remove from Network" ]] \
        || fail "unmount: an unmounted place offers $(ipc contextMenuEntries), not Rename, Edit address then Remove from Network"
    menu_seek "Remove from Network"
    key -k Return >/dev/null
    wait_message "Ghost Place is forgotten"
    [[ -f "$bookmarks" ]] || fail "unmount: Remove unlinked the bookmarks file instead of emptying it"
    [[ -z "$(cat "$bookmarks")" ]] \
        || fail "unmount: the last saved line survived Remove, the file reads: $(cat "$bookmarks")"
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] \
        || fail "unmount: the forgotten place stayed on the rail, got $(ipc networkEntries)"

    local before_press4
    before_press4=$(stat -c %y "$bookmarks")
    # The second press on the row that is still there, which is the state one sentence used to blame
    # on a file nobody had read: the file has been read, and this share is simply not in it.
    click_rail_row "$(rail_row_of stubshare)" right
    settle
    menu_seek "Remove from Network"
    key -k Return >/dev/null
    wait_message "stubshare is not saved, and stays in the sidebar until it is unmounted"
    # Both stat and cat print nothing for a path that is gone, so neither assertion below means
    # anything until the file is known to be there.
    [[ -f "$bookmarks" ]] || fail "unmount: the refused press unlinked the bookmarks file"
    [[ -z "$(cat "$bookmarks")" ]] \
        || fail "unmount: the refused press wrote to the file, it reads: $(cat "$bookmarks")"
    [[ "$(stat -c %y "$bookmarks")" == "$before_press4" ]] \
        || fail "unmount: the refused press rewrote the file, mtime moved from $before_press4"

    printf 'UNMOUNT remove saved-mounted=ok saved-only=ok pressed-again=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
}

case_hangshare() {
    local dir="$fixture_root/hangshare"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/0-one.txt"

    local good_dir="$fixture_root/hangshare-good"
    sandbox_remove "$good_dir"
    mkdir -p "$good_dir"
    : > "$good_dir/one.txt"

    local hang_uri="smb://stubhost/hang/"
    local good_uri="smb://stubhost/good/"
    local root_uri="smb://stubhost/"
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
$gio_stub_args
# Every call is logged, because a case that hangs on purpose has no other way to say which leg it
# reached; the fail messages below quote it. Same idea as case_unmount's own stub log.
printf '%s\n' "\$*" >> "$dir/bin/calls"
if [ "\$1 \$2" = "mount -li" ]; then
  printf 'Mount(0): hang en stubhost -> $hang_uri\n  Type: GDaemonMount\n'
  printf 'Mount(1): good en stubhost -> $good_uri\n  Type: GDaemonMount\n'
  exit 0
fi
if [ "\$1" = info ]; then
  # A bare server root has no FUSE path of its own, so the product goes on to gio list.
  [ "\$2" = "$root_uri" ] && exit 0
  # The dead-server shape ui/NetworkMounts.qml documents: gio info on a location gvfs cannot reach
  # never returns. The marker is what tells the test the guard has actually closed, so the busy
  # refusal below is never asserted against an open that has not started. exec so the product's own
  # terminate reaches the sleep and leaves nothing behind.
  if [ "\${2%/}" = "${hang_uri%/}" ]; then : > "$dir/bin/info-started"; exec sleep 25; fi
  printf 'local path: %s\n' "$good_dir"
fi
# gio list on a server gvfs cannot reach hangs the same way gio info does, and this is the third
# leg of an open: without its own deadline nothing in the chain is left to end the share browser.
if [ "\$1" = list ]; then : > "$dir/bin/list-started"; exec sleep 25; fi
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/hangshare-home"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf '%s StubRoot\n' "$root_uri" > "$fixture_home/.config/gtk-3.0/bookmarks"
    local real_home="$HOME" saved_path="$PATH"

    local want_rail="hang|network|share|true"$'\n'"good|network|share|true"$'\n'"StubRoot|network|share|false"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/ is the gio stub's own fixture entry, alongside the one file under test.
    wait_listing 2
    wait_rail 4
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "$want_rail" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "$want_rail" ]] \
        || fail "hangshare: the stub mounts and bare root did not reach the rail, got $(ipc networkEntries)"

    # Home(0), hang(1), good(2). Opening hang starts the info that never answers.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "hangshare: Tab did not reach the rail"
    key g >/dev/null
    settle
    rail_seek hang
    settle
    key l >/dev/null
    wait_marker "$dir/bin/info-started" "hangshare: the hang share's gio info never started, the stub saw: $(grep -v '^mount -l$' "$dir/bin/calls" 2>/dev/null | sort -u | tr '\n' ';')"

    # An open that has not landed keeps focus on the rail, so the next key still moves the rail's cursor (#181).
    [[ "$(ipc focusView)" == "rail" ]] || fail "hangshare: focus left the rail for $(ipc focusView) while the share had not opened"

    # The guard is closed now, proven by the marker above rather than by a sleep, and a second share
    # must say so rather than swallow the keypress.
    key j >/dev/null
    settle
    [[ "$(ipc railLabel "$(ipc railCursor)")" == "good" ]] \
        || fail "hangshare: expected the rail cursor on good, got $(ipc railLabel "$(ipc railCursor)")"
    key l >/dev/null
    wait_message "Another network location is still opening. Try again when it opens"
    [[ "$(ipc path)" == "$dir" ]] || fail "hangshare: the refused open navigated to $(ipc path)"

    # And the deadline is what reopens it, with the same sentence a mount that never answers gets.
    # The sentence is emitted in the same handler that terminates the leg, and Process.running only
    # clears when onExited is delivered (measured on quickshell 0.3.1), so an l pressed the instant
    # it appears can still meet the guard. Press until it takes: with no deadline none ever does.
    wait_message "Could not connect: the server did not respond."
    for _attempt in $(seq 1 200); do
        [[ "$(ipc path)" == "$good_dir" ]] && break
        key l >/dev/null
        sleep 0.05
    done
    [[ "$(ipc path)" == "$good_dir" ]] \
        || fail "hangshare: good never opened after the deadline, path is $(ipc path)"
    [[ "$(ipc total)" == "1" ]] || fail "hangshare: good's own listing did not load, total is $(ipc total)"

    # The third leg, driven from a fresh window: the deadline's sentence is the same one the info
    # leg already produced, and lastMessage still carries it, so a stale match would green this arm
    # against a product that never ended the listing at all. A bare server root ends in gio list,
    # which hangs here exactly as gio info did, and nothing else in the chain is left to end it.
    # The stub and the fixture home are exported around this launch exactly as they were around the
    # first: a relaunch that inherits the restored HOME starts Flea on the operator's own rail.
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 2
    wait_rail 4
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "$want_rail" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "$want_rail" ]] \
        || fail "hangshare: the fresh window's rail is $(ipc networkEntries)"
    [[ -z "$(ipc lastMessage)" ]] || fail "hangshare: the fresh window already says $(ipc lastMessage)"
    # Home(0), hang(1), good(2), StubRoot(3).
    rail_focus
    key g >/dev/null
    settle
    rail_seek StubRoot
    settle
    key l >/dev/null
    wait_marker "$dir/bin/list-started" "hangshare: the bare root's gio list never started, the stub saw: $(grep -v '^mount -l$' "$dir/bin/calls" 2>/dev/null | sort -u | tr '\n' ';')"
    wait_message "Could not connect: the server did not respond."
    [[ "$(ipc shareBrowserOpen)" == "false" ]] \
        || fail "hangshare: the overlay opened on a listing that never answered"
    # The relaunch above started this window at $dir, so that is where a listing that ended in the
    # deadline has to leave it.
    [[ "$(ipc path)" == "$dir" ]] || fail "hangshare: the timed-out listing navigated to $(ipc path)"

    printf 'HANGSHARE busy-refusal=ok deadline=ok next-share-opens=ok list-deadline=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"; sandbox_remove "$good_dir"
}

case_hanglisting() {
    local dir="$fixture_root/hanglisting"
    sandbox_scratch "$dir"
    : > "$dir/one.txt"
    : > "$dir/two.txt"
    hanglisting_write_proxy "$dir/flea-proxy"
    local real_bin="$flea_bin"
    export FLEA_HANGLISTING_REAL="$real_bin" FLEA_HANGLISTING_DELAY=15 FLEA_HANGLISTING_MARKER="$dir/delayed"
    flea_bin="$dir/flea-proxy"
    launch "$dir"
    flea_bin="$real_bin"
    wait_marker "$dir/delayed" "hanglisting: the proxy never saw the listing"
    local seen="" deadline=$(( $(date +%s%3N) + 25000 ))
    while (( $(date +%s%3N) < deadline )); do
        seen=$(ipc stateMessage 2>/dev/null || true)
        [[ "$seen" == "That folder is not responding." ]] && break
        sleep 0.2
    done
    [[ "$seen" == "That folder is not responding." ]] \
        || fail "hanglisting: the deadline never spoke, stateMessage is $seen, state $(ipc state), proxy marker $(tr "\n" ";" < "$dir/delayed")"
    [[ "$(ipc state)" == "waiting" ]] \
        || fail "hanglisting: the pane is not waiting, state is $(ipc state)"
    shot hanglisting-waiting
    # flea-proxy and delayed are the stub entries beside the two files under test.
    wait_listing 4
    [[ "$(ipc path)" == "$dir" ]] || fail "hanglisting: the landed listing navigated to $(ipc path)"
    [[ -z "$(ipc stateMessage)" ]] || fail "hanglisting: the landed listing kept $(ipc stateMessage)"

    # Past the deadline a second held listing escapes through the rail's Home row, and its late rows never draw over Home.
    export FLEA_HANGLISTING_MARKER="$dir/escape-delayed"
    flea_bin="$dir/flea-proxy"
    launch "$dir"
    flea_bin="$real_bin"
    wait_marker "$dir/escape-delayed" "hanglisting: the proxy never held the second listing"
    local held_at=$(date +%s%3N) home_path seen_path="" escape_by
    home_path=$(window_home) || fail "hanglisting: the window's isolated Home is unavailable"
    [[ "$home_path" == "$FLEA_TEST_WINDOWS_ROOT/"*/home && -f "$FLEA_TEST_WINDOWS_ROOT/.flea-test-sandbox" && -O "$home_path" ]] \
        || fail "hanglisting: Home is outside the owned window sandbox"
    : > "$home_path/hanglisting-home.txt"
    wait_pane_waiting hanglisting
    click_rail_row "$(rail_row_of Home)" left
    escape_by=$(( $(date +%s%3N) + 10000 ))
    while (( $(date +%s%3N) < escape_by )); do
        seen_path=$(ipc path 2>/dev/null || true)
        [[ "$seen_path" == "$home_path" ]] && break
        sleep 0.2
    done
    [[ "$seen_path" == "$home_path" ]] \
        || fail "hanglisting: Home never opened past the wait, path is $seen_path"
    # Home's list sends the held one first, so its stale rows arrive while Home's listing is out; the wait spans the hold too.
    local late_by=$(( held_at + FLEA_HANGLISTING_DELAY * 1000 + 2000 ))
    while (( $(date +%s%3N) < late_by )); do sleep 0.2; done
    [[ "$(ipc path)" == "$home_path" ]] \
        || fail "hanglisting: the late rows drew over Home: $(ipc path)"
    # Only a list of another folder writes "flushed", so the proxy sent the held list ahead of Home's own list.
    grep -qx flushed "$dir/escape-delayed" 2>/dev/null \
        || fail "hanglisting: Home's list never flushed the held one, the proxy wrote $(tr '\n' ' ' 2>/dev/null < "$dir/escape-delayed" || echo 'no marker file')"
    wait_listing 1
    seek_row_named hanglisting-home.txt
    [[ -z "$(ipc stateMessage)" ]] \
        || fail "hanglisting: Home kept $(ipc stateMessage)"
    shot hanglisting-escaped

    # F5 on a waiting pane re-lists the folder instead of refusing; the proxy holds only a session's first list.
    export FLEA_HANGLISTING_MARKER="$dir/reload-delayed"
    flea_bin="$dir/flea-proxy"
    launch "$dir"
    flea_bin="$real_bin"
    wait_marker "$dir/reload-delayed" "hanglisting: the proxy never held the third listing"
    wait_pane_waiting hanglisting
    key -k F5 >/dev/null
    wait_listing 6
    [[ "$(ipc path)" == "$dir" ]] || fail "hanglisting: F5 past the wait navigated to $(ipc path)"
    [[ "$(ipc state)" != "waiting" && -z "$(ipc stateMessage)" ]] \
        || fail "hanglisting: F5 left the pane $(ipc state) with $(ipc stateMessage)"
    # The proxy dropped the held list when the re-list reached it, so the rows here came from the re-list.
    grep -qx dropped "$dir/reload-delayed" \
        || fail "hanglisting: F5 rows landed only with the held reply, not the re-list"
    shot hanglisting-reloaded

    # Up on a waiting pane climbs from the folder it waited on, with the waited folder's row as cursor.
    export FLEA_HANGLISTING_MARKER="$dir/up-delayed"
    flea_bin="$dir/flea-proxy"
    launch "$dir"
    flea_bin="$real_bin"
    wait_marker "$dir/up-delayed" "hanglisting: the proxy never held the fourth listing"
    wait_pane_waiting hanglisting
    key -k BackSpace >/dev/null
    wait_path "$fixture_root"
    local up_name="" up_by=$(( $(date +%s%3N) + 15000 ))
    while (( $(date +%s%3N) < up_by )); do
        up_name=$(ipc renameState 2>/dev/null | jq -er .cursorName 2>/dev/null || true)
        [[ "$up_name" == "hanglisting" && "$(ipc listInFlight 2>/dev/null)" == "false" ]] && break
        sleep 0.2
    done
    [[ "$(ipc path)" == "$fixture_root" ]] \
        || fail "hanglisting: Up left the pane at $(ipc path), not the parent $fixture_root"
    [[ "$up_name" == "hanglisting" ]] \
        || fail "hanglisting: Up put the cursor on $up_name, not the waited folder"
    shot hanglisting-up

    # A rail click inside the wait is the newest request, taken not refused; the proxy holds the Up listing of the parent for 8 s, below the 10 s wait.
    export FLEA_HANGLISTING_MARKER="$dir/newest-delayed" FLEA_HANGLISTING_DELAY=8 FLEA_HANGLISTING_PATH="$fixture_root"
    flea_bin="$dir/flea-proxy"
    launch "$dir"
    flea_bin="$real_bin"
    home_path=$(window_home) || fail "hanglisting: the fifth window's isolated Home is unavailable"
    [[ "$home_path" == "$FLEA_TEST_WINDOWS_ROOT/"*/home && -f "$FLEA_TEST_WINDOWS_ROOT/.flea-test-sandbox" && -O "$home_path" ]] \
        || fail "hanglisting: the fifth Home is outside the owned window sandbox"
    : > "$home_path/hanglisting-home.txt"
    key -k BackSpace >/dev/null
    wait_marker "$dir/newest-delayed" "hanglisting: the proxy never held the fifth listing"
    held_at=$(date +%s%3N)
    printf 'HELD path=%s age_ms=%s\n' "$(cat "$dir/newest-delayed")" "$(( held_at - $(date -r "$dir/newest-delayed" +%s%3N) ))"
    sleep 1
    local flight state_now
    flight=$(ipc listInFlight); state_now=$(ipc state)
    [[ "$flight" == "true" && "$state_now" != "waiting" ]] \
        || fail "hanglisting: the fifth listing is not out inside the wait, in flight $flight, state $state_now, $(( $(date +%s%3N) - held_at )) ms after the hold"
    click_rail_row "$(rail_row_of Home)" left
    sleep 0.3
    [[ "$(ipc stateMessage)" != "A directory is already loading." ]] \
        || fail "hanglisting: a rail click inside the wait was refused with $(ipc stateMessage)"
    escape_by=$(( $(date +%s%3N) + 10000 ))
    seen_path=""
    while (( $(date +%s%3N) < escape_by )); do
        seen_path=$(ipc path 2>/dev/null || true)
        [[ "$seen_path" == "$home_path" ]] && break
        sleep 0.2
    done
    [[ "$seen_path" == "$home_path" ]] \
        || fail "hanglisting: Home never landed inside the wait, path is $seen_path"
    late_by=$(( held_at + FLEA_HANGLISTING_DELAY * 1000 + 2000 ))
    while (( $(date +%s%3N) < late_by )); do sleep 0.2; done
    [[ "$(ipc path)" == "$home_path" && -z "$(ipc stateMessage)" ]] \
        || fail "hanglisting: the late rows drew over Home: $(ipc path) with $(ipc stateMessage)"
    wait_listing 1
    seek_row_named hanglisting-home.txt
    # As in the second listing, Home's list sent the held one ahead of its own, so its stale rows came while Home was out.
    grep -qx flushed "$dir/newest-delayed" 2>/dev/null \
        || fail "hanglisting: Home's list never flushed the held fifth listing, the proxy wrote $(tr '\n' ' ' 2>/dev/null < "$dir/newest-delayed" || echo 'no marker file')"
    unset FLEA_HANGLISTING_PATH
    export FLEA_HANGLISTING_DELAY=15

    printf 'HANGLISTING deadline=ok rows-land-after=ok wait-escape=ok late-rows-dropped=ok reload-escape=ok up-climb=ok newest-wins=ok\n'
    kill_flea
}
