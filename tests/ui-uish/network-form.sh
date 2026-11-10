# GPUI overrides for UISH-11; seeded from the unchanged oracle at 2faf8d099.
case_network() {
    assert_network_attempt_reset _infoOutput infoProcess \
        || fail "network: info output is not cleared immediately before infoProcess starts"
    assert_network_attempt_reset _listSharesOutput listSharesProcess \
        || fail "network: share output is not cleared immediately before listSharesProcess starts"
    local dir="$fixture_root/network"
    sandbox_scratch "$dir"
    local fake_root="$fixture_root/network-fake"
    sandbox_scratch "$fake_root"
    mkdir -p "$fake_root/bin"
    # Three rows so a cursor that did not move is distinguishable from one clamped to a short listing.
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"
    : > "$dir/apple.txt"
    local fixture_home="$fixture_root/network-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME" mount_log="$fake_root/mount.log" mount_calls="$fake_root/mount-calls"
    local state="$fixture_root/network-state" stored bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    local real_state="${XDG_STATE_HOME-}" real_config="${XDG_CONFIG_HOME-}"
    export XDG_CONFIG_HOME="$fixture_home/.config"
    seed_ui_state "$state" '{"view":"list","keys":"default","places":{"favourites":[]}}'
    stored="$state/flea/ui.json"
    : > "$mount_log"
    : > "$mount_calls"
    mkfifo "$fake_root/mount-release"
    local races="$fixture_root/network-races" race_state="$fixture_root/network-race-state" name
    sandbox_scratch "$races"
    mkdir -p "$races/left" "$races/right/child" "$races/mounted" "$races/share-mounted"
    for name in a b c; do
        : > "$races/left/$name.txt"
        : > "$races/right/$name.txt"
    done
    for name in a b; do
        : > "$races/right/child/$name.txt"
        : > "$races/mounted/$name.txt"
        : > "$races/share-mounted/$name.txt"
    done
    for name in late origin shares child; do mkfifo "$fake_root/$name-release"; done

    network_cleanup() {
        local name fifo fd result=0
        local -a release_fds=()
        sandbox_require "$fake_root"
        sandbox_owned "$fake_root" || fail "network: refusing cleanup of an unowned fixture"
        for name in mount late origin shares child; do
            sandbox_require "$fake_root/$name-release"
            fifo="$SANDBOX_PATH"
            [[ -p "$fifo" && ! -L "$fifo" ]] || fail "network: cleanup barrier is not an owned FIFO: $fifo"
            # Keep both ends open through the reap so even a helper arriving late receives its release.
            exec {fd}<>"$fifo" || fail "network: could not open cleanup barrier: $fifo"
            release_fds+=("$fd")
            printf 'release\n' >&"$fd" || fail "network: could not release cleanup barrier: $fifo"
        done
        ( kill_flea ) || result=1
        for fd in "${release_fds[@]}"; do exec {fd}>&-; done
        return "$result"
    }
    trap 'network_cleanup || exit 1' EXIT

    network_wait_favourites() {
        settings_wait_value "$1"
        local expected
        expected=$(ipc uiSettings | jq -c '.places.favourites')
        ipc railEntries | jq -e --argjson expected "$expected" '[.[] | select(.kind == "favourite") | .original] == $expected' >/dev/null \
            || fail "network: saved favourites and their visible rail rows differ"
    }
    network_wait_closed() {
        local attempt
        for attempt in $(seq 1 100); do
            [[ "$(ipc dialogOpen)" == false ]] && return
            sleep 0.05
        done
        fail "network: successful persistence did not close the form: $(ipc networkStatus)"
    }
    network_wait_panes() {
        local filter="$1" attempt seen
        for attempt in $(seq 1 100); do
            seen=$(ipc dualState)
            jq -e "$filter" <<< "$seen" >/dev/null && return
            sleep 0.05
        done
        printf 'NETWORK_PANE_FAILURE cursor=%q key=%s network=%s geometry=%s\n' \
            "$(hyprctl cursorpos)" "$(ipc keyDeliveryState)" "$(ipc networkFocusState)" "$(ipc shareBrowserState)"
        shot network-pane-focus-failure
        fail "network: pane transition did not satisfy $filter: $seen"
    }
    network_click_pane() {
        local side="$1" x y width height wx wy
        read -r x y width height <<< "$(ipc shareBrowserState | jq -r --argjson side "$side" '.paneRects[$side]')"
        [[ "$width" -gt 0 && "$height" -gt 0 ]] || fail "network: pane $side has no clickable listing"
        read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
        printf 'NETWORK_PANE_CLICK side=%s target=%s,%s rectangle=%s,%s,%s,%s before=%s\n' \
            "$side" "$((wx + x + width / 2))" "$((wy + y + height / 2))" "$x" "$y" "$width" "$height" "$(ipc dualState)"
        omarchy-drive click "$((wx + x + width / 2))" "$((wy + y + height / 2))" >/dev/null
        network_wait_panes ".focused == $side"
    }
    network_wait_tabs() {
        local count="$1" visible="$2" attempt
        for attempt in $(seq 1 100); do
            [[ "$(ipc tabCount)" == "$count" && "$(ipc tabBarVisible)" == "$visible" ]] && return
            sleep 0.05
        done
        fail "network: expected $count tabs with strip visible=$visible, got $(ipc tabCount)/$(ipc tabBarVisible)"
    }
    network_share_geometry() {
        local label="$1" expected="${2:-}" attempt seen
        for attempt in $(seq 1 100); do
            seen=$(ipc shareBrowserState) || fail 'network: share geometry observer failed'
            if jq -e --arg expected "$expected" '.active and .owner == 1 and .baseUri == "smb://shares-second.test/"
                and .rect == .paneRects[1] and ($expected == "" or .rect == $expected)' <<< "$seen" >/dev/null; then
                network_geometry_rect=$(jq -r .rect <<< "$seen")
                network_geometry_checks=$((network_geometry_checks + 1))
                printf 'NETWORK_SHARE_GEOMETRY %s %s expected=%q observed=%s\n' "$network_geometry_checks" "$label" "$expected" "$seen"
                return
            fi
            sleep 0.05
        done
        fail "network: $label geometry expected=$expected observed=$seen panes=$(ipc dualState)"
    }
    network_share_reflow() {
        local hidden_rect tabs_rect x y width height chrome font_before font_after direction stops
        network_wait_tabs 1 false
        network_share_geometry 'second-pane shares start without tabs'
        hidden_rect="$network_geometry_rect"
        network_click_pane 0
        network_wait_tabs 1 false
        network_share_geometry 'pointer focus in the other pane retains owner geometry' "$hidden_rect"
        key t >/dev/null || fail 'network: native new-tab key failed'
        network_wait_tabs 2 true
        read -r x y width height <<< "$hidden_rect"
        chrome=$(ipc chromeHeight)
        tabs_rect="$x $((y + chrome)) $width $((height - chrome))"
        network_share_geometry 'new tab adds exactly the live tab-strip height' "$tabs_rect"
        [[ "$(ipc focusView)" == list ]] || fail 'network: non-owner pane lacks listing focus before Tab'
        key -k Tab >/dev/null || fail 'network: native pane focus key failed'
        network_wait_panes '.focused == 1'
        network_wait_tabs 1 false
        network_share_geometry 'keyboard focus to one-tab owner removes the tab strip' "$hidden_rect"
        network_click_pane 0
        network_wait_tabs 2 true
        network_share_geometry 'pointer focus to two-tab pane restores the tab strip' "$tabs_rect"
        shot network-shares-tabs-reflow
        key w >/dev/null || fail 'network: native close-tab key failed'
        network_wait_tabs 1 false
        network_share_geometry 'closing the other pane tab restores the original rectangle' "$hidden_rect"

        permissions_viewport 880 620
        network_share_geometry 'shares match the owner at 880x620'
        hidden_rect="$network_geometry_rect"
        permissions_viewport 1100 800
        network_share_geometry 'shares match the owner after native resize to 1100x800'
        [[ "$network_geometry_rect" != "$hidden_rect" ]] || fail 'network: window resize did not change the owner rectangle'
        shot network-shares-resized

        settings_wait_value '.display.textSize.mode == "system"'
        font_before=$(token_of baseSize)
        # Sample source: var STOPS = [9, 10, 11, 12, 14, 16, 20]
        stops=$(grep '^var STOPS = ' "$flea_ui/js/TextSize.js" | cut -d= -f2-) || fail 'network: text-size stops are unavailable'
        font_after=$(jq -r --argjson size "$font_before" 'map(select(. > $size)) | first' <<< "$stops") \
            || fail 'network: text-size stops could not be read'
        direction=equal
        if [[ -z "$font_after" || "$font_after" == null ]]; then
            font_after=$(jq -er --argjson size "$font_before" 'map(select(. < $size)) | last' <<< "$stops") \
                || fail 'network: no alternate text-size stop is available'
            direction=minus
        fi
        hidden_rect="$network_geometry_rect"
        key -M ctrl -M shift -k "$direction" -m shift -m ctrl >/dev/null || fail 'network: text-size chord failed'
        settings_wait_value ".display.textSize.mode == $font_after"
        [[ "$(token_of baseSize)" == "$font_after" ]] || fail 'network: persisted text size did not reach the live theme'
        network_share_geometry 'shares follow the native text-size change'
        [[ "$network_geometry_rect" != "$hidden_rect" ]] || fail 'network: text-size change did not change owner geometry'
        shot network-shares-font-reflow
        key -M ctrl -M shift -k 0 -m shift -m ctrl >/dev/null || fail 'network: text-size reset chord failed'
        settings_wait_value '.display.textSize.mode == "system"'
        [[ "$(token_of baseSize)" == "$font_before" ]] || fail 'network: Follow Omarchy did not restore the original font'
        network_share_geometry 'font reset restores the original owner rectangle' "$hidden_rect"
        printf 'NETWORK shares-tab-focus=exact shares-resize=exact shares-font-reflow=exact checks=%s\n' "$network_geometry_checks"
    }
    network_click_favourite() {
        local label="$1" index
        index=$(ipc railEntries | jq -r --arg label "$label" 'map(.label) | index($label)')
        [[ "$index" != null ]] || fail "network: missing fixture favourite $label"
        click_rail_row "$index" left
    }
    network_release() {
        timeout 2 sh -c 'printf "release\n" > "$1"' sh "$fake_root/$1-release" \
            || fail "network: $1 fixture was no longer waiting at its release barrier"
    }

    # One ipc read into network_ipc_out; a failed read fails by name, so it is never mistaken for an empty answer.
    local network_ipc_out=""
    network_ipc() {
        local status=0
        network_ipc_out=$(ipc "$@" 2>&1) || status=$?
        (( status == 0 )) || fail "network: ipc $1 failed with status $status: $network_ipc_out"
    }
    # networkBuilt (ui/Ipc.qml) is true only after the rail rebuilt from a gio listing and a mountinfo read, and an empty group is also what the unbuilt rail shows.
    local network_built_attempts=400
    network_wait_built() {
        local attempt
        for attempt in $(seq 1 "$network_built_attempts"); do
            network_ipc networkBuilt
            [[ "$network_ipc_out" == true ]] && return
            sleep 0.05
        done
        fail "network: the rail never built its Network group from a mount listing (networkBuilt is $network_ipc_out)"
    }
    network_assert_empty() {
        local what="$1"
        network_ipc networkEntries
        [[ -z "$network_ipc_out" ]] \
            || fail "network: $what: the Network group shows [$network_ipc_out], not empty (the gio double isolates the gio mount table only; a kernel network mount Flea lists from /proc/self/mountinfo shows as its own row)"
    }

    # This case proves form/bookmark behavior, not a network route; a bounded local gio double keeps
    # the newly functional Save action from dialing TEST-NET-2 or reopening on its later timeout.
    cat > "$fake_root/bin/gio" <<EOS
#!/bin/sh
$gio_stub_args
case "\$*" in
"info --attributes=trash::item-count trash:///"|"monitor --dir=trash:///") exec /usr/bin/gio "\$@" ;;
esac
case "\$1 \${2:-}" in
"mount -li")
    if [ -e "$fake_root/friendly-live" ]; then
        printf 'Mount(0): GVfs generated on 198.51.100.1 -> smb://198.51.100.1/\n  Type: GDaemonMount\n'
    fi
    ;;
"mount nfs://cancel.test/export")
    : > "$fake_root/cancel-started"
    read release < "$fake_root/mount-release"
    ;;
"mount nfs://late-retry.test/export")
    : > "$fake_root/late-started"
    read release < "$fake_root/late-release"
    exit 2
    ;;
"mount nfs://origin.test/export")
    : > "$fake_root/origin-started"
    read release < "$fake_root/origin-release"
    ;;
"mount nfs://stale-one.test/export") printf 'Location is already mounted\n' >&2; exit 2 ;;
"mount nfs://stale-two.test/export") exit 2 ;;
"mount --anonymous")
    [ "\$3" != 'smb://198.51.100.1/' ] || : > "$fake_root/friendly-live"
    printf '%s\n' "\$*" > "$mount_log"
    printf '%s\n' "\$*" >> "$mount_calls"
    ;;
"mount "*) printf '%s\n' "\$*" > "$mount_log"; printf '%s\n' "\$*" >> "$mount_calls" ;;
"info nfs://stale-two.test/export") exit 1 ;;
"info nfs://late-retry.test/export") exit 1 ;;
"info nfs://origin.test/export") printf 'local path: %s\n' "$races/mounted" ;;
"info smb://shares-origin.test/alpha")
    : > "$fake_root/child-started"
    read release < "$fake_root/child-release"
    printf 'local path: %s\n' "$races/share-mounted"
    ;;
"info smb://shares-one.test/"|"info smb://shares-two.test/") exit 1 ;;
"info smb://shares-origin.test/"|"info smb://shares-second.test/") exit 1 ;;
"info "*) printf 'local path: %s\n' "$dir" ;;
"list smb://shares-one.test/") printf 'old-share\n' ;;
"list smb://shares-two.test/") exit 0 ;;
"list smb://shares-origin.test/")
    : > "$fake_root/shares-started"
    read release < "$fake_root/shares-release"
    printf 'alpha\nbeta\n'
    ;;
"list smb://shares-second.test/") printf 'second-alpha\nsecond-beta\n' ;;
*) exit 0 ;;
esac
EOS
    chmod +x "$fake_root/bin/gio"
    local saved_path="$PATH"
    export PATH="$fake_root/bin:$PATH"

    # No .config/gtk-3.0/ at all yet: the exact "absent at launch" shape the dialog's write must survive.
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 3
    network_wait_built
    network_assert_empty "no bookmarks, no listed mounts and no Dropbox"
    shot network-empty

    # The current keymap binds "a" to add network from both listing and rail contexts.
    [[ "$(ipc focusView)" == "list" ]] || fail "network: did not start on the list"
    [[ "$(ipc cursor)" == "0" ]] || fail "network: did not start on row 0"
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: a from the list did not open the add-location dialog"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: list add-location dialog did not close"
    [[ "$(ipc cursor)" == "0" ]] || fail "network: list a changed the cursor to $(ipc cursor)"

    # The real submit path: Tab to the rail, "a" opens the dialog there, type a location, Enter submits.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "network: Tab did not reach the rail"
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: a from the rail did not open the add-location dialog"
    shot network-dialog-open
    # A saved name unlike both the host and gio's name proves the breadcrumb's source; Shift+Tab from Host reaches Label.
    key -M shift -k Tab -m shift >/dev/null
    settle
    [[ "$(ipc networkFocus)" == Label ]] || fail "network: Shift+Tab did not reach Label"
    key "Saved personal" >/dev/null
    key -k Tab >/dev/null
    # TEST-NET-2 (RFC 5737): guaranteed non-routable, so this never actually dials out.
    key "198.51.100.1" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "smb://198.51.100.1/" ]] \
        || fail "network: the Mounts-as line reads $(ipc networkUri)"
    key -k Return >/dev/null
    wait_network_result mounted 5
    network_wait_closed
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: Enter did not submit and close the dialog"
    [[ "$(cat "$mount_log")" == 'mount --anonymous smb://198.51.100.1/' ]] \
        || fail "network: guest SMB did not use gio mount --anonymous"
    network_wait_favourites '.places.favourites == [{"label":"Saved personal","path":"smb://198.51.100.1/"}]'
    network_wait_closed
    # Info completion triggers the poll's own rebuild, and its live row carries another label under the same normalized URI.
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "GVfs generated|network|share|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "GVfs generated|network|share|true" ]] \
        || fail "network: the live mount control did not reach the rail: $(ipc networkEntries)"
    [[ "$(ipc crumbText)" == "Saved personal" ]] \
        || fail "network: the live poll changed the saved breadcrumb to $(ipc crumbText)"
    [[ ! -e "$bookmarks" ]] || fail "network: the new favourite created shared GTK bookmarks"
    shot network-appeared
    # Retire this live-row control before the later legacy-bookmark fixture expects its rail alone.
    sandbox_require "$fake_root/friendly-live"
    rm -f -- "$SANDBOX_PATH"

    local invalid_port invalid_port_failures=0 snapshot network_shot_port=0
    # The GPUI port field takes digits only, so 22/path can never reach a submit; 0 and 65536 are the refused ports.
    assert_network_port_filters "22/path" 22 || fail "network: the port field kept a non-digit from 22/path"
    for invalid_port in "0" "65536"; do
        snapshot="$fixture_root/network-state-${invalid_port//\//-}"
        if ! assert_invalid_network_port "$invalid_port" "$stored" "$snapshot"; then
            invalid_port_failures=$((invalid_port_failures + 1))
        fi
    done
    [[ "$invalid_port_failures" -eq 0 ]] \
        || fail "network: $invalid_port_failures invalid-port submit checks failed"

    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: Escape did not close the dialog"
    # g resets to row 0 first, so this reads the same whatever the dialog left the cursor on.
    key g >/dev/null
    settle
    key j >/dev/null
    settle
    [[ "$(ipc cursor)" == "1" ]] || fail "network: keyboard nav is dead after the dialog closed, cursor is $(ipc cursor)"

    # The five protocols the canvas draws, each prefilling its own port and naming its own path row.
    key -k Tab >/dev/null
    settle
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: the dialog did not reopen for the protocol pass"
    local want protocol port pathlabel
    for want in "SMB 445 Share" "SFTP 22 Path" "FTPS 21 Path" "WebDAV 443 Path" "NFS 2049 Export"; do
        read -r protocol port pathlabel <<< "$want"
        click_chip "$protocol"
        settle
        [[ "$(ipc networkProtocol)" == "$protocol" ]] \
            || fail "network: the chip did not pick $protocol, it is $(ipc networkProtocol)"
        [[ "$(ipc networkPort)" == "$port" ]] \
            || fail "network: $protocol prefilled port $(ipc networkPort), not $port"
        [[ "$(ipc networkPathLabel)" == "$pathlabel" ]] \
            || fail "network: $protocol names its path row $(ipc networkPathLabel), not $pathlabel"
    done
    printf 'NETWORK protocols=ok ports=445,22,21,443,2049\n'
    shot network-protocols
    key -k Escape >/dev/null
    settle

    # The traversal, driven rather than read. No new IPC is needed: the Mounts-as line is an exact
    # projection of protocol, host, port, path, domain and user, so where a typed pair LANDS is
    # observable, and a Tab that went wrong puts its pair somewhere else in the same string.
    # Port is tabbed through and never typed into, because it is the one prefilled field and the
    # caret position on programmatic focus is not something this case should depend on.
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: the dialog did not reopen for the traversal walk"
    key "hh" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "ss" >/dev/null
    key -k Tab >/dev/null
    key "dd" >/dev/null
    key -k Tab >/dev/null
    key "uu" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "smb://dd;uu@hh/ss" ]] \
        || fail "network: Tab did not walk host, port, share, domain, username in order, URI is $(ipc networkUri)"

    # Under SMB, Tab from Username walks Password and the drawn buttons (Install Dropbox only without Dropbox) to the first chip.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc networkFocus)" == Password ]] || fail "network: Tab from Username did not reach Password"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc networkFocus)" == Cancel ]] || fail "network: Tab from Password did not reach Cancel"
    key -k Tab >/dev/null
    settle
    local primary
    primary="$(ipc networkAction)"
    [[ -n "$primary" && "$(ipc networkFocus)" == "$primary" ]] || fail "network: Tab from Cancel did not reach the primary under SMB, focus is $(ipc networkFocus)"
    key -k Tab >/dev/null
    settle
    if [[ "$(ipc networkFocus)" == "Install Dropbox" ]]; then key -k Tab >/dev/null; settle; fi
    [[ -z "$(ipc networkFocus)" ]] || fail "network: the buttons did not wrap to the first chip, focus is $(ipc networkFocus)"
    local walk
    key -k Tab >/dev/null
    settle
    # networkFocus names fields and buttons only, so the chip's focus is shot, not asserted.
    shot network-chip-focus
    key -k Return >/dev/null
    settle
    shot network-chip-picked-focus
    [[ "$(ipc networkUri)" == "sftp://uu@hh/ss" ]] \
        || fail "network: Enter on a tabbed-to chip did not pick SFTP, URI is $(ipc networkUri)"

    # SFTP hides Domain and TLS: Backtab crosses SMB, the buttons, Password and Username, then skips Domain to Path.
    for ((walk = 0; walk < 8; walk++)); do
        key -M shift -k Tab -m shift >/dev/null
        settle
        [[ "$(ipc networkFocus)" == Username ]] && break
    done
    [[ "$(ipc networkFocus)" == Username ]] || fail "network: Backtab never reached Username, focus is $(ipc networkFocus)"
    key -M shift -k Tab -m shift >/dev/null
    key "XX" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh/ssXX" ]] \
        || fail "network: Shift-Tab did not skip the hidden Domain and reach Path, URI is $(ipc networkUri)"
    # Repeat the reverse walk with a physical Shift+Tab chord, which can arrive as Tab plus Shift.
    key -k Tab -k Tab -k Tab >/dev/null
    settle
    [[ "$(ipc networkFocus)" == Cancel ]] || fail "network: Tab from Password did not reach Cancel under SFTP"
    local expected_focus
    for expected_focus in Password Username Path; do
        key -M shift -k Tab -m shift >/dev/null
        settle
        [[ "$(ipc networkFocus)" == "$expected_focus" ]] \
            || fail "network: Shift+Tab expected '$expected_focus', got '$(ipc networkFocus)'"
    done
    key "YY" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh/ssXXYY" ]] \
        || fail "network: Shift+Tab did not return typing to Path, URI is $(ipc networkUri)"
    click_chip FTPS
    settle
    [[ "$(ipc networkFocus)" == Path ]] || fail "network: protocol change moved the visible Path focus"
    key -k Tab -k Tab -k Tab >/dev/null
    settle
    [[ "$(ipc networkFocus)" == TLS ]] || fail "network: forward traversal did not reach TLS"
    key -M shift -k Tab -m shift >/dev/null
    settle
    [[ "$(ipc networkFocus)" == Password ]] || fail "network: Shift+Tab from TLS did not reach Password"
    key -k Tab -M shift -k Tab -m shift >/dev/null
    settle
    [[ "$(ipc networkFocus)" == Password ]] || fail "network: Backtab from TLS did not reach Password"
    # From Password the walk names Cancel, then the primary's label; both focus stops are shot.
    for ((walk = 0; walk < 4; walk++)); do
        [[ "$(ipc networkFocus)" == Cancel ]] && break
        key -k Tab >/dev/null
        settle
    done
    [[ "$(ipc networkFocus)" == Cancel ]] || fail "network: Tab from TLS did not reach Cancel"
    shot network-focus-cancel
    primary="$(ipc networkAction)"
    for ((walk = 0; walk < 4; walk++)); do
        [[ -n "$primary" && "$(ipc networkFocus)" == "$primary" ]] && break
        key -k Tab >/dev/null
        settle
    done
    [[ -n "$primary" && "$(ipc networkFocus)" == "$primary" ]] \
        || fail "network: Tab from Cancel did not reach the primary, focus is $(ipc networkFocus)"
    shot network-focus-connect
    printf 'NETWORK traversal=ok wrap=ok skip=ok chip-enter=ok shift-tab=ok tls-reverse=ok buttons=ok\n'
    shot network-traversal
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: escape on the focused primary did not close the dialog"

    # Finding 17: picking a chip that hides the field holding the caret must re-home the caret to
    # the next visible field, or the next keystroke lands in a field the form never draws while
    # Enter still submits from it. Domain is SMB-only, so SFTP hides it.
    # The URI drops a domain that carries no username, so the walk is proven by the share instead,
    # and a caret one field early or late puts "uu" where the last assertion would catch it.
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: the dialog did not reopen for the re-home walk"
    key "hh" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "ss" >/dev/null
    key -k Tab >/dev/null
    key "dd" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "smb://hh/ss" ]] \
        || fail "network: the re-home walk did not reach Domain, URI is $(ipc networkUri)"
    click_chip SFTP
    settle
    [[ "$(ipc networkProtocol)" == "SFTP" ]] \
        || fail "network: the chip click did not pick SFTP, it is $(ipc networkProtocol)"
    key "uu" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh/ss" ]] \
        || fail "network: a chip click left the caret in the hidden Domain, URI is $(ipc networkUri)"
    printf 'NETWORK rehome=ok\n'
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] \
        || fail "network: escape did not close the dialog after the re-home walk"

    # Favourites owns the new row; Remove edits that store without touching GTK or unmounting.
    local added_favourite_index
    added_favourite_index=$(ipc railEntries | jq -er 'map(.kind) | index("favourite")') \
        || fail "network: the added favorite is not present in the rail"
    click_rail_row "$added_favourite_index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Remove from Favorites" ]] \
        || fail "network: the added favourite offers $(ipc contextMenuEntries), not Remove from Favorites"
    key -k Return >/dev/null
    network_wait_favourites '.places.favourites == []'
    [[ ! -e "$bookmarks" ]] || fail "network: Remove created shared GTK bookmarks"
    [[ "$(wc -l < "$mount_calls")" -eq 1 ]] || fail "network: Remove called the mount helper"
    printf 'NETWORK add-then-remove=ok gtk-absent=ok\n'

    local host
    for host in 198.51.100.2 198.51.100.3; do
        rail_focus
        key a >/dev/null
        settle
        [[ "$(ipc dialogOpen)" == true ]] || fail "network: the form did not reopen for $host"
        key "$host" >/dev/null
        key -k Return >/dev/null
        network_wait_closed
    done
    network_wait_favourites '.places.favourites == [{"label":"198.51.100.2","path":"smb://198.51.100.2/"},{"label":"198.51.100.3","path":"smb://198.51.100.3/"}]'

    # A real unreadable-but-writable store must refuse its asynchronous append without losing inputs.
    local before mount_count
    before=$(cat "$stored")
    mount_count=$(wc -l < "$mount_calls")
    chmod 200 "$stored"
    rail_focus
    key a >/dev/null
    settle
    key "198.51.100.4" >/dev/null
    key -k Return -k Return >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkStatus)" == *"could not be read"* ]] && break
        sleep 0.05
    done
    chmod 600 "$stored"
    [[ "$(ipc dialogOpen)" == true && "$(ipc networkStatus)" == *"could not be read"* ]] \
        || fail "network: persistence refusal did not keep its error and form visible: $(ipc networkStatus)"
    [[ "$(ipc networkUri)" == 'smb://198.51.100.4/' && "$(ipc networkAction)" == Retry ]] \
        || fail "network: save refusal lost the entered endpoint or retry action"
    [[ "$(cat "$stored")" == "$before" ]] || fail "network: unreadable state was overwritten"
    [[ "$(wc -l < "$mount_calls")" -eq $((mount_count + 1)) ]] \
        || fail "network: repeated submit mounted more than once"
    shot network-save-refused
    key -k Return >/dev/null
    network_wait_closed
    network_wait_favourites '.places.favourites | length == 3 and .[2] == {"label":"198.51.100.4","path":"smb://198.51.100.4/"}'
    [[ "$(wc -l < "$mount_calls")" -eq $((mount_count + 1)) ]] \
        || fail "network: persistence retry remounted an already connected endpoint"
    [[ ! -e "$bookmarks" ]] || fail "network: persistence retry created GTK bookmarks"
    printf 'NETWORK save-refusal=retained repeat-submit=single retry=save-only\n'

    # A mount blocked at a fixture-owned FIFO provides an observable, cancellable in-flight request.
    before=$(cat "$stored")
    rail_focus
    key a >/dev/null
    settle
    click_chip NFS
    key "cancel.test" >/dev/null
    key -k Tab -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    for _attempt in $(seq 1 100); do
        [[ -e "$fake_root/cancel-started" ]] && break
        sleep 0.05
    done
    [[ -e "$fake_root/cancel-started" && "$(ipc dialogOpen)" == true ]] \
        || fail "network: cancellable mount did not remain in the open form"
    [[ "$(cat "$stored")" == "$before" ]] || fail "network: favourite was saved before mount completion"
    local pending_state pending_key
    pending_state=$(ipc dualState)
    ipc networkFocusState | jq -e '.inside and .busy' >/dev/null \
        || fail "network: pending mount did not own keyboard focus"
    for pending_key in Tab Backtab ShiftTab; do
        if [[ "$pending_key" == ShiftTab ]]; then
            key -M shift -k Tab -m shift >/dev/null
        elif [[ "$pending_key" == Backtab ]]; then
            key -k ISO_Left_Tab >/dev/null
        else
            key -k "$pending_key" >/dev/null
        fi
        settle
        ipc networkFocusState | jq -e '.inside and .busy' >/dev/null \
            || fail "network: pending $pending_key escaped the dialog: $(ipc networkFocusState)"
        [[ "$(ipc dualState)" == "$pending_state" ]] \
            || fail "network: pending $pending_key changed the listing, selection or pane focus"
    done
    shot network-mount-pending
    key -k Escape >/dev/null
    wait_network_result cancelled 5
    [[ "$(ipc dialogOpen)" == false && "$(cat "$stored")" == "$before" ]] \
        || fail "network: Cancel reopened the form or saved an unfinished mount"
    printf 'NETWORK mount-first=ok pending-Tab=contained pending-Backtab=contained pending-ShiftTab=contained cancel=inflight no-place=ok\n'

    # Plain WebDAV is port 80, so the tick that picks the scheme has to pick the number with it, or
    # the dialog offers a port that scheme does not use while the rail dedups against the one it
    # does. The arm above closed its own dialog, so this one opens a fresh one.
    rail_focus
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: a from the rail did not reopen the dialog for the TLS pass"
    click_chip WebDAV
    settle
    [[ "$(ipc networkPort)" == "443" ]] || fail "network: WebDAV opened on port $(ipc networkPort), not 443"
    # WebDAV picked: the Path field and the TLS box at port 443.
    shot network-webdav-picked
    key "wd.example" >/dev/null
    # Domain is hidden under WebDAV and the walk skips it, Password is shown and the walk crosses it,
    # so the walk reads where it is rather than counting: the traversal arm above pins the order.
    for _attempt in $(seq 1 8); do
        [[ "$(ipc networkFocus)" == "TLS" ]] && break
        key -k Tab >/dev/null
        settle
    done
    [[ "$(ipc networkFocus)" == "TLS" ]] \
        || fail "network: the walk never reached the TLS row, it is on $(ipc networkFocus)"
    # The TLS row focused under WebDAV, before the tick.
    shot network-webdav-tls
    key -k Space >/dev/null
    settle
    [[ "$(ipc networkPort)" == "80" ]] \
        || fail "network: unticking TLS left the port at $(ipc networkPort), not plain dav's own 80"
    [[ "$(ipc networkUri)" == "dav://wd.example/" ]] \
        || fail "network: the Mounts-as line reads $(ipc networkUri) after the tick"
    printf 'NETWORK tls-port=ok\n'
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: escape did not close the dialog after the TLS pass"

    # Legacy GTK rows stay readable and byte-for-byte intact alongside persisted Flea favourites.
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf 'smb://legacy.test/data Legacy share\nfile:///missing/legacy Local legacy\n' > "$bookmarks"
    local legacy_before
    legacy_before=$(cat "$bookmarks")
    kill_flea
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 3
    network_wait_favourites '.places.favourites | length == 3'
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == 'Legacy share|network|share|false' ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == 'Legacy share|network|share|false' ]] \
        || fail "network: restarting did not preserve the readable legacy Network row"
    assert_network_mark_alignment "the live text size" true
    local legacy_index
    legacy_index=$(ipc railEntries | jq -r 'map(.label) | index("Legacy share")')
    click_rail_row "$legacy_index" right
    settle
    # The rail menu is offered, the same as 0.1.6: opening it writes nothing, only activating a row does.
    [[ "$(ipc contextMenuVisible)" == true ]] || fail "network: a legacy GTK row offered no rail menu"
    printf 'NETWORK legacy-rail-menu=%s\n' "$(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle
    [[ "$(cat "$bookmarks")" == "$legacy_before" ]] || fail "network: legacy GTK bytes changed"
    printf 'NETWORK restart=favourites-and-legacy gtk=unchanged\n'

    rail_focus
    key a >/dev/null
    settle
    click_chip NFS
    key "nfs.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted 5
    network_wait_closed
    [[ "$(cat "$mount_log")" == 'mount nfs://nfs.test/export' ]] \
        || fail "network: NFS did not retain plain gio mount"

    # Two direct opens in a row, the second refused. A mount exit code no longer decides on its own
    # (ui/NetworkMounts.qml "_mountFailed"), so the refused location refuses its info call too, and
    # what this arm proves is that the second open reports its own verdict and not the first's.
    [[ "$(ipc focusView)" == "rail" ]] || fail "network: direct-cache setup did not return to rail"
    key a >/dev/null
    click_chip NFS
    key "stale-one.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted 5
    network_wait_closed
    key a >/dev/null
    click_chip NFS
    key "stale-two.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    wait_network_result failed 5
    network_wait_favourites '.places.favourites | all(.[]; .path != "nfs://stale-two.test/export")'
    [[ "$(ipc dialogOpen)" == true ]] || fail "network: failed mount discarded its draft"
    [[ "$(ipc networkStatus)" == "Could not connect: the server refused the connection." ]] \
        || fail "network: the refused direct open reported the previous open's verdict"
    key -k Escape >/dev/null
    settle

    [[ "$(ipc focusView)" == "rail" ]] || fail "network: share-cache setup did not return to rail"
    key a >/dev/null
    key "shares-one.test" >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted 5
    network_wait_closed
    [[ "$(ipc shareBrowserOpen)" == "true" && "$(ipc shareBrowserEntries)" == "old-share" ]] \
        || fail "network: share-cache setup did not list first root"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "network: first share browser did not return to rail"
    key a >/dev/null
    key "shares-two.test" >/dev/null
    key -k Return >/dev/null
    wait_network_result missing-credential 5
    shot network-guest-empty-signin
    [[ "$(ipc shareBrowserOpen)" == "false" \
        && "$(ipc networkStatus)" == "This server shows a guest nothing. Sign in to see its shares." ]] \
        || fail "network: bare root reused stale share output"

    network_wait_favourites '.places.favourites | all(.[]; .path != "smb://shares-two.test/")'
    [[ "$(cat "$bookmarks")" == "$legacy_before" ]] || fail "network: later Network actions changed GTK bookmarks"
    key -k Escape >/dev/null
    settle
    printf 'NETWORK empty=ok a-scoped=ok dialog=ok submit-path=ok keyboard-after=ok guest-smb=anonymous nfs=plain caches=isolated\n'

    # Separate persisted fixtures keep the mount-origin races independent of the save inventory above.
    kill_flea
    seed_ui_state "$race_state" "$(jq -n --arg left "$races/left" --arg right "$races/right" \
        '{view:"list",keys:"default",dual:{paths:[$left,$right],focus:0},places:{favourites:[
            {label:"Late retry",path:"nfs://late-retry.test/export"},
            {label:"Origin mount",path:"nfs://origin.test/export"},
            {label:"Origin shares",path:"smb://shares-origin.test/"},
            {label:"Second shares",path:"smb://shares-second.test/"}]}}')"
    HOME="$fixture_home" launch "$races/left"
    wait_listing 3
    network_wait_favourites '.places.favourites | length == 4'

    network_click_favourite "Late retry"
    wait_marker "$fake_root/late-started" "network: late retry did not reach its mount barrier"
    [[ "$(ipc keymapPreset)" == default && "$(ipc networkResult)" == mounting ]] \
        || fail "network: direct mount context differs: preset=$(ipc keymapPreset) focus=$(ipc focusView) result=$(ipc networkResult)"
    key -M ctrl -k k -m ctrl >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == false ]] || fail "network: Mac-only Ctrl+K opened a draft in the Default preset"
    if [[ "$(ipc focusView)" == list ]]; then key -k Tab >/dev/null; fi
    [[ "$(ipc focusView)" == rail ]] || fail "network: native Tab did not focus the rail during mounting"
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == true ]] \
        || fail "network: rail a did not open a newer draft: preset=$(ipc keymapPreset) focus=$(ipc focusView) dialog=$(ipc dialogOpen) mount=$(ipc networkResult) dialogFocus=$(ipc networkFocusState)"
    printf 'NETWORK direct-mount preset=%s focus=%s dialog=%s mount=%s default-CtrlK=refused rail-a=opened\n' \
        "$(ipc keymapPreset)" "$(ipc networkFocusState)" "$(ipc dialogOpen)" "$(ipc networkResult)"
    click_chip NFS
    key "newer-draft.test" >/dev/null
    key -k Tab -k Tab >/dev/null
    key "/kept" >/dev/null
    local draft_uri draft_title draft_action draft_focus
    draft_uri=$(ipc networkUri)
    draft_title=$(ipc networkTitle)
    draft_action=$(ipc networkAction)
    draft_focus=$(ipc networkFocusState)
    [[ "$draft_uri" == 'nfs://newer-draft.test/kept' ]] || fail "network: newer draft did not receive native typing"
    network_release late
    wait_network_result failed 5
    [[ "$(ipc dialogOpen)" == true && "$(ipc networkUri)" == "$draft_uri" \
        && "$(ipc networkTitle)" == "$draft_title" && "$(ipc networkAction)" == "$draft_action" \
        && "$(ipc networkFocusState)" == "$draft_focus" && -z "$(ipc networkStatus)" ]] \
        || fail "network: late failure replaced or refocused the newer draft"
    [[ "$(ipc statusError)" == true && "$(ipc lastMessage)" == 'Could not connect: the server refused the connection.' ]] \
        || fail "network: preserving the newer draft hid the older mount failure: statusError=$(ipc statusError) message=$(ipc lastMessage) footer=$(ipc statusFooterState) result=$(ipc networkResult)"
    network_wait_favourites '.places.favourites | length == 4'
    shot network-late-retry-newer-draft
    key -k Escape >/dev/null
    settle
    printf 'NETWORK late-retry=newer-draft-retained focus=retained failure=visible\n'

    click_chrome dual
    network_wait_panes '.active and .focused == 0 and .panes[1].total == 4 and (.panes[1].loading | not)'
    network_click_favourite "Origin mount"
    wait_marker "$fake_root/origin-started" "network: origin mount did not reach its barrier"
    network_click_pane 1
    key j >/dev/null
    settle
    local other_before
    other_before=$(ipc dualState | jq -c '.panes[1]')
    network_release origin
    wait_network_result mounted 5
    network_wait_panes ".focused == 1 and .panes[0].path == \"$races/mounted\" and .panes[0].total == 2 and (.panes[0].loading | not)"
    [[ "$(ipc paneCrumbText 0)" == "Origin mount" ]] \
        || fail "network: the dual-pane mount breadcrumb reads $(ipc paneCrumbText 0), not its saved label"
    [[ "$(ipc dualState | jq -c '.panes[1]')" == "$other_before" ]] \
        || fail "network: completing the first pane's mount changed the second pane"
    shot network-dual-origin-mount

    network_click_pane 0
    network_click_favourite "Origin shares"
    wait_marker "$fake_root/shares-started" "network: share listing did not reach its barrier"
    network_click_pane 1
    other_before=$(ipc dualState | jq -c '.panes[1]')
    network_release shares
    wait_network_result mounted 5
    ipc shareBrowserState | jq -e '.active and .owner == 0 and .rect == .paneRects[0] and .baseUri == "smb://shares-origin.test/"' >/dev/null \
        || fail "network: completed shares did not cover only their originating pane: $(ipc shareBrowserState)"
    [[ "$(ipc dualState | jq -c '.panes[1]')" == "$other_before" ]] \
        || fail "network: completed shares changed the other pane"
    local other_cursor
    other_cursor=$(ipc cursor)
    key j >/dev/null
    settle
    [[ "$(ipc cursor)" -eq $((other_cursor + 1)) && "$(ipc shareBrowserCursor)" == 0 ]] \
        || fail "network: keys in the other pane moved the share cursor"
    goto_row 0
    key l >/dev/null
    network_wait_panes ".focused == 1 and .panes[1].path == \"$races/right/child\" and .panes[1].total == 2 and (.panes[1].loading | not)"
    [[ "$(ipc shareBrowserOpen)" == true ]] || fail "network: other-pane navigation dismissed the originating pane's shares"
    shot network-dual-origin-shares

    local sx sy sw sh wx wy row_height
    read -r sx sy sw sh <<< "$(ipc shareBrowserRect)"
    read -r _body _caption _padding row_height <<< "$(ipc metrics)"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((wx + sx + sw / 2))" "$((wy + sy + row_height / 2))" >/dev/null
    wait_marker "$fake_root/child-started" "network: share row pointer activation did not mount its child"
    network_wait_panes '.focused == 0'
    network_click_pane 1
    other_before=$(ipc dualState | jq -c '.panes[1]')
    network_release child
    wait_network_result mounted 5
    network_wait_panes ".focused == 1 and .panes[0].path == \"$races/share-mounted\" and .panes[0].total == 2 and (.panes[0].loading | not)"
    [[ "$(ipc shareBrowserOpen)" == false && "$(ipc dualState | jq -c '.panes[1]')" == "$other_before" ]] \
        || fail "network: child mount lost its origin, stole focus or left its shares visible"
    shot network-dual-origin-child
    printf 'NETWORK dual-mount=origin-preserved shares=origin-preserved other-pane-keys=independent other-pane-navigation=independent child-mount=origin-preserved\n'

    printf 'NETWORK_SECOND_SHARES before=%s result=%s panes=%s\n' "$(ipc shareBrowserState)" "$(ipc networkResult)" "$(ipc dualState)"
    network_click_favourite "Second shares"
    wait_network_result mounted 5
    printf 'NETWORK_SECOND_SHARES after-result=%s result=%s panes=%s\n' "$(ipc shareBrowserState)" "$(ipc networkResult)" "$(ipc dualState)"
    local second_shares attempt
    for attempt in $(seq 1 100); do
        second_shares=$(ipc shareBrowserState)
        jq -e '.active and .baseUri == "smb://shares-second.test/"' <<< "$second_shares" >/dev/null && break
        sleep 0.05
    done
    jq -e '.active and .baseUri == "smb://shares-second.test/" and .owner == 1 and .rect == .paneRects[1]' <<< "$second_shares" >/dev/null \
        || fail "network: second-pane shares did not retain their owner: $second_shares; panes=$(ipc dualState); result=$(ipc networkResult)"
    shot network-second-pane-shares
    local network_geometry_rect network_geometry_checks=0
    network_share_reflow
    local exit_log_start retained_secondary retained_shares retained_entries retained_rect rx ry rw rh menus_checks=0
    retained_secondary=$(ipc dualState | jq -c '.panes[1] | del(.focused)')
    retained_shares=$(ipc shareBrowserState | jq -c '{active,owner,baseUri,cursor}')
    retained_entries=$(ipc shareBrowserEntries)
    retained_rect="$network_geometry_rect"
    exit_log_start=$(wc -l < "$flea_log")
    click_chrome list
    network_wait_panes "(.active | not) and .focused == 0 and (.panes[1].focused | not) and ((.panes[1] | del(.focused)) == $retained_secondary)"
    menus_expect shareBrowserState "{active,owner,baseUri,cursor} == $retained_shares" 'leaving dual retains the secondary share listing session'
    key -k Home >/dev/null || fail 'network: primary Home delivery failed after dual exit'
    network_wait_panes '.focused == 0 and .panes[0].cursor == 0'
    key j >/dev/null || fail 'network: primary j delivery failed after dual exit'
    network_wait_panes ".focused == 0 and .panes[0].cursor == 1 and ((.panes[1] | del(.focused)) == $retained_secondary)"
    menus_expect shareBrowserState "{active,owner,baseUri,cursor} == $retained_shares" 'hidden secondary shares cannot consume primary-list keys'
    read -r rx ry rw rh <<< "$(ipc rowRect 0)"
    [[ "$rw" -gt 0 && "$rh" -gt 0 ]] || fail 'network: primary row has no pointer target after dual exit'
    menus_point "$((rx + rw * 3 / 4)) $((ry + rh / 2))"
    network_wait_panes ".focused == 0 and .panes[0].cursor == 0 and .panes[0].selected == [0] and ((.panes[1] | del(.focused)) == $retained_secondary)"
    menus_expect shareBrowserState "{active,owner,baseUri,cursor} == $retained_shares" 'pointer selects the expanded primary listing without activating hidden shares'
    # The race fixture's queued network error owns the first Escape, so it is acknowledged before selection Escape is tested.
    local standing_errors
    standing_errors=$(ipc statusActivityState | jq -er '.errors | numbers') \
        || fail "network: statusActivityState gave no error count: $(ipc statusActivityState)"
    printf 'NETWORK_STANDING_ERRORS %s\n' "$standing_errors"
    while [[ "$standing_errors" != 0 ]]; do
        key -k Escape >/dev/null || fail 'network: error acknowledgement Escape delivery failed'
        menus_expect statusActivityState ".errors < $standing_errors" 'Escape acknowledges a standing error first'
        network_wait_panes '.focused == 0 and .panes[0].selected == [0]'
        standing_errors=$(ipc statusActivityState | jq -er '.errors | numbers') \
            || fail "network: statusActivityState gave no error count: $(ipc statusActivityState)"
    done
    key -k Escape >/dev/null || fail 'network: primary Escape delivery failed after dual exit'
    network_wait_panes '.focused == 0 and .panes[0].selected == []'
    menus_expect shareBrowserState "{active,owner,baseUri,cursor} == $retained_shares" 'primary Escape clears selection without dismissing the retained secondary session'
    shot network-shares-owner-hidden

    click_chrome dual
    network_wait_panes ".active and .focused == 0 and ((.panes[1] | del(.focused)) == $retained_secondary)"
    network_share_geometry 'reopening dual restores retained shares geometry' "$retained_rect"
    menus_equal 'reopening dual preserves the original share names' "$retained_entries" "$(ipc shareBrowserEntries)"
    key -k Tab >/dev/null || fail 'network: reopened secondary focus delivery failed'
    network_wait_panes '.focused == 1'
    key j >/dev/null || fail 'network: reopened share cursor delivery failed'
    menus_expect shareBrowserState '.active and .owner == 1 and .cursor == 1' 'reopened secondary shares receive their own cursor key'
    network_wait_panes ".focused == 1 and .panes[0].cursor == 0 and ((.panes[1] | del(.focused)) == $retained_secondary)"
    shot network-shares-owner-reopened
    key -k Escape >/dev/null || fail 'network: reopened share Escape delivery failed'
    menus_expect shareBrowserState '(.active | not) and .owner == 1' 'Escape in the owning pane dismisses shares without destroying its pane'
    key j >/dev/null || fail 'network: secondary listing j failed after dismissing shares'
    network_wait_panes '.focused == 1 and .panes[1].focused and .panes[1].cursor == 1 and .panes[0].cursor == 0'
    shot network-shares-owner-dismissed
    click_chrome list
    network_wait_panes '(.active | not) and .focused == 0 and (.panes[1].focused | not)'
    tail -n "+$((exit_log_start + 1))" "$flea_log" > "$fake_root/second-pane-exit.log"
    # Qt 6.12's Wayland text input warns when focus leaves with no surface focused; the log check below skips it too.
    if grep -F -v 'QWaylandTextInputv3::disableSurface(wl_surface*) Trying to disable ' "$fake_root/second-pane-exit.log" \
        | grep -E 'WARN|ERROR|TypeError|ReferenceError|Cannot'; then
        fail "network: second-pane exit or reopen produced native QML errors"
    fi
    [[ "$(cat "$bookmarks")" == "$legacy_before" ]] || fail "network: origin races changed GTK bookmarks"
    printf 'NETWORK second-pane-session=retained hidden-keys-pointer=primary reopen=owner-and-geometry Escape=focused-owner focus=restored qml-log=clean checks=%s\n' "$menus_checks"
    export PATH="$saved_path"
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    if [[ -n "$real_config" ]]; then export XDG_CONFIG_HOME="$real_config"; else unset XDG_CONFIG_HOME; fi
    network_cleanup || fail "network: owned process did not drain after releasing its barriers"
    trap - EXIT
    sandbox_remove "$fixture_home"
    sandbox_remove "$fake_root"
}

case_netmark() {
    local dir="$fixture_root/netmark"
    sandbox_scratch "$dir"
    : > "$dir/one.txt"
    local fake_root="$fixture_root/netmark-fake"
    sandbox_scratch "$fake_root"
    mkdir -p "$fake_root/bin"
    # This case measures geometry, so the rail must not depend on whatever the box has mounted.
    printf '#!/bin/sh\nexit 0\n' > "$fake_root/bin/gio"
    chmod +x "$fake_root/bin/gio"
    local saved_path="$PATH"
    export PATH="$fake_root/bin:$PATH"

    local fixture_home="$fixture_root/netmark-home" real_home="$HOME"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0" "$fixture_home/.config/omarchy"
    # One bookmark is one NETWORK row, and a NETWORK row is the only thing that draws an indicator dot.
    printf 'smb://198.51.100.1/ 198.51.100.1\n' > "$fixture_home/.config/gtk-3.0/bookmarks"

    local base first=true caption smallest="" largest=""
    for base in 9 10 11 12 14 16 20; do
        printf '[font]\nbase-size = %s\n' "$base" > "$fixture_home/.config/omarchy/shell.toml"
        export HOME="$fixture_home"
        launch "$dir"
        export HOME="$real_home"
        wait_rail 1
        # The probes cost four clicks, so they run once, at the stop with the widest overhang, and
        # before the invariant: a measured box has to be real before its centre is worth arguing about.
        if [[ "$first" == true ]]; then
            probe_network_mark_target
            first=false
        fi
        assert_network_mark_alignment "text size $base"
        read -r _body caption _pad _rowheight <<< "$(ipc metrics)"
        [[ -n "$smallest" ]] || smallest="$caption"
        largest="$caption"
    done
    # A sweep whose stops all render the same caption would pass without testing anything.
    [[ "$largest" -gt "$smallest" ]] \
        || fail "netmark: caption stayed at $smallest across every text size, so no stop took effect"

    # The text-size chords are the second live lever on this base, keys.toml textSizeUp/textSizeDown,
    # and their top stop is the only place the caption grows past Theme.hitMin, where the hit target
    # and the caption slot become the same box. Those stops are read back the way the base sizes are.
    local step zoom_small zoom_large
    for step in 1 2; do
        key -M ctrl -M shift -k minus -m shift -m ctrl >/dev/null
    done
    settle
    assert_network_mark_alignment "text size 20 at the smallest zoom"
    read -r _body zoom_small _pad _rowheight <<< "$(ipc metrics)"
    for step in $(seq 1 12); do
        key -M ctrl -M shift -k equal -m shift -m ctrl >/dev/null
    done
    settle
    assert_network_mark_alignment "text size 20 at the largest zoom"
    read -r _body zoom_large _pad _rowheight <<< "$(ipc metrics)"
    key -M ctrl -M shift -k 0 -m shift -m ctrl >/dev/null
    settle
    # The chords are keystrokes into a live window, so the zoom ends are proven the same way: a pair
    # that never moved the caption would assert the invariant twice against one layout.
    [[ "$zoom_large" -gt "$zoom_small" ]] \
        || fail "netmark: caption stayed at $zoom_small across both zoom ends, so the chords did nothing"

    printf 'NETMARK stops=7 caption=%s..%s probes=4 zoom=%s..%s\n' \
        "$smallest" "$largest" "$zoom_small" "$zoom_large"
    export PATH="$saved_path"
    kill_flea
    sandbox_remove "$fixture_home"
    sandbox_remove "$fake_root"
    sandbox_remove "$dir"
}

case_networkauth() {
    local dir="$fixture_root/network-auth" fixture_home="$fixture_root/network-auth-home"
    local state="$dir/state" runtime_canary
    runtime_canary=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
    local helper_log="$state/helper.log" attempt_log="$state/attempt.log" helper_attempts
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    local retry_dir="$state/retry-root"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin" "$state" "$retry_dir"
    : > "$dir/local.txt"
    : > "$retry_dir/alpha.txt"
    : > "$state/mounted"
    : > "$helper_log"
    : > "$attempt_log"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf '%s\n' 'sftp://tester@slot.test/home42/tester Own slot' > "$bookmarks"
    printf '%s\n' 'sftp://tester@slot.test/' > "$state/mounted"

    assert_runtime_canary_absent() {
        local surface method value
        for surface in "$fixture_root" "$thumb_fixture" "$hash_fixture" "$stale_fixture" \
            "$evidence_dir" "$flea_log" "$run_log" "$case_log" \
            "$repo/.superpowers/flea/release-014-20260904/reports/baseline-authenticated-ui.md"; do
            [[ -e "$surface" ]] || continue
            # -D skip and -r, not -R: case_network leaves FIFOs in its fixture, and a recursive read
            # of one blocks in the kernel forever waiting for a writer that never comes.
            ! printf '%s\n' "$runtime_canary" | grep -r -D skip -a -F -q -f - "$surface" 2>/dev/null \
                || fail "networkauth: runtime canary reached byte-addressable surface"
        done
        for method in themeForeground selectedFill palette metrics tokens selectedIndices focusView \
            path lastMessage stickyMessage firstRowsAt inputToRows mode state stateMessage \
            contextMenuEntries contextMenuGlyphs contextMenuSubmenuGlyphs contextMenuSubmenuEntries \
            renameEditorText railRenameEditorText previewKind previewState previewPdfZoom previewExpanded \
            previewPdfPage headerTitles sortMark previewSliderCentre headerLeft viewMode archiveFormats \
            keymapSheetRows keymapSheetResults convertFormat previewFacts previewColumnState columnPlayCentre columnStripCentre \
            tabLabels pathBarText pathCentre \
            headerTop networkProtocol networkPort networkUri networkPathLabel networkTitle networkFields \
            networkFocus networkHostPortWidths networkPasswordState networkNote networkAction networkStatus \
            networkDialogMetrics networkDialogMetricTargets networkResult networkPasswordEyeCentre \
            shareBrowserEntries networkEntries deviceEntries; do
            value=$(ipc "$method" 2>/dev/null || true)
            [[ "$value" != *"$runtime_canary"* ]] \
                || fail "networkauth: runtime canary reached IPC $method"
        done
    }

    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
$gio_stub_args
case "\$1 \${2:-}" in
"mount -li")
    if [ -s "$state/mounted" ]; then
        uri=\$(cat "$state/mounted")
        printf 'Mount(0): auth-test -> %s\n' "\$uri"
    fi
    ;;
"mount -u")
    printf '%s\n' "\$3" > "$state/unmount-uri"
    : > "$state/mounted"
    ;;
"info "*)
    mounted=\$(cat "$state/mounted")
    if [ "\$mounted" != "\$2" ]; then
        [ "\$mounted" = "sftp://tester@slot.test/" ] \
            && [ "\$2" = "sftp://tester@slot.test/home42/tester" ] || exit 1
    fi
    printf '%s\n' "\$2" > "$state/info-uri"
    if [ "\$2" = "ftps://tester@slot.test/retry" ]; then
        printf 'local path: %s\n' "$retry_dir"
    else
        printf 'local path: %s\n' "$dir"
    fi
    ;;
"mount "*) printf '%s\n' "\$2" > "$state/mounted" ;;
*) exit 1 ;;
esac
EOS
    cat > "$dir/bin/timeout" <<EOS
#!/bin/sh
if [ "\${1:-}" = -- ] && [ "\${2:-}" = 35 ] && [ "\${3:-}" = "$dir/bin/flea-gio-auth" ]; then
    /usr/bin/timeout "\$@"
    result=\$?
    printf '%s\n' "\$result" >> "$attempt_log"
    exit "\$result"
fi
exec /usr/bin/timeout "\$@"
EOS
    # Sample attempt log: "127\n126\n"; each line records one completed real auth timeout invocation.
    networkauth_wait_attempt() {
        local count="$1" code="$2" attempt
        for attempt in $(seq 1 100); do
            [[ "$(wc -l < "$attempt_log")" -eq "$count" && "$(tail -n 1 "$attempt_log")" == "$code" ]] && return
            sleep 0.05
        done
        fail "networkauth: expected completed auth attempt $count with exit $code"
    }
    cat > "$dir/bin/flea-gio-auth" <<'EOS'
#!/bin/sh
[ "$#" -eq 1 ] || exit 2
IFS= read -r secret || exit 3
[ -n "$secret" ] || exit 4
cmdline=$(tr '\0' '\n' < "/proc/$$/cmdline")
environment=$(env)
case "$cmdline$environment" in *"$secret"*) exit 5 ;; esac
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
state=${script_dir%/bin}/state
printf 'argc=1 uri=%s stdin-lines=1\n' "$1" >> "$state/helper.log"
[ ! -e "$state/fail" ] || exit 60
[ ! -e "$state/refuse" ] || exit 77
printf '%s\n' "$1" > "$state/mounted"
secret=
EOS
    chmod +x "$dir/bin/gio" "$dir/bin/flea-gio-auth" "$dir/bin/timeout"

    local real_home="$HOME" saved_path="$PATH"
    export HOME="$fixture_home"
    export PATH="$dir/bin:$PATH"
    export FLEA_GIO_AUTH="$dir/bin/flea-gio-auth"
    launch "$dir"
    export HOME="$real_home"
    wait_listing_wall 3

    # GIO exposes an authority-root mount for SFTP even when the saved product location is an
    # addressable descendant. The rail must keep the saved URI, mark it mounted, and emit one row.
    local want_entries='Own slot|network|share|true' seen_entries="" entries_deadline
    entries_deadline=$(( $(date +%s%3N) + 12000 ))
    while (( $(date +%s%3N) < entries_deadline )); do
        seen_entries=$(timeout 1 "$FLEA_IPC_HELPER" drive -p "$flea_ui/boot" flea networkEntries 2>/dev/null || true)
        [[ "$seen_entries" == "$want_entries" ]] && break
        sleep 0.1
    done
    [[ "$seen_entries" == "$want_entries" ]] \
        || fail "networkauth: authority-root mount duplicated descendant bookmark ($seen_entries)"
    local network_index step
    network_index=$(ipc networkStartIndex)
    key -k Tab >/dev/null
    key g >/dev/null
    for ((step = 0; step < network_index; step++)); do key j >/dev/null; done
    key -k Return >/dev/null
    wait_network_result mounted 5
    [[ "$(cat "$state/info-uri")" == 'sftp://tester@slot.test/home42/tester' ]] \
        || fail "networkauth: mounted saved row did not resolve its descendant URI"
    [[ ! -s "$helper_log" ]] || fail "networkauth: already-mounted descendant launched helper"
    click_rail_row "$network_index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount|-|Rename|Edit Address…|-|Remove from Network" ]] \
        || fail "networkauth: the projected mounted row offers $(ipc contextMenuEntries), not Open first"
    menu_seek Unmount
    key -k Return >/dev/null
    wait_network_result unmounted 5
    [[ "$(cat "$state/unmount-uri")" == 'sftp://tester@slot.test/' ]] \
        || fail "networkauth: projected row unmounted its saved descendant URI"
    printf 'NETWORKAUTH descendant-dedup=ok saved-uri=ok mount-uri=ok\n'
    kill_flea
    : > "$state/mounted"
    printf '%s\n' \
        'dav://tester@plain-default.test/ Plain 80' \
        'dav://tester@plain-secure-port.test:443/ Plain 443' > "$bookmarks"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_network_entry_state false
    network_index=$(ipc networkStartIndex)
    key -k Tab >/dev/null
    key g >/dev/null
    for ((step = 0; step < network_index; step++)); do key j >/dev/null; done
    key -k Return >/dev/null
    settle
    [[ "$(ipc networkPort)" == 80 && "$(ipc networkUri)" == 'dav://tester@plain-default.test/' ]] \
        || fail "networkauth: dav default reparsed as port $(ipc networkPort), URI $(ipc networkUri)"
    key -k Escape >/dev/null
    key j >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc networkPort)" == 443 && "$(ipc networkUri)" == 'dav://tester@plain-secure-port.test:443/' ]] \
        || fail "networkauth: dav :443 reparsed as port $(ipc networkPort), URI $(ipc networkUri)"
    printf 'NETWORKAUTH dav-default=80 dav-explicit=443\n'
    kill_flea
    : > "$bookmarks"
    : > "$state/info-uri"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing_wall 3

    key -k Tab >/dev/null
    key a >/dev/null
    settle
    # Dialogs rule 6: the title says the task, so it is the same sentence under every chip below.
    [[ "$(ipc networkTitle)" == "Add a Network Share" ]] \
        || fail "networkauth: the add dialog does not name its own task, got $(ipc networkTitle)"
    [[ "$(ipc networkFields)" == "Label|Host|Port|Share|Domain|Username|Password" ]] \
        || fail "networkauth: SMB fields are $(ipc networkFields)"
    [[ "$(ipc networkHostPortWidths)" == *"|"* ]] || fail "networkauth: no Host/Port geometry"
    local host_width port_width
    IFS='|' read -r host_width port_width <<< "$(ipc networkHostPortWidths)"
    [[ "$host_width" == "$port_width" ]] || fail "networkauth: Host/Port widths differ ($host_width/$port_width)"
    [[ "$(ipc networkPasswordState)" == "masked|empty" ]] \
        || fail "networkauth: fresh password state is $(ipc networkPasswordState)"
    [[ "$(ipc networkPasswordHint)" == "The password is not saved." ]] \
        || fail "networkauth: SMB password hint is $(ipc networkPasswordHint)"
    [[ "$(ipc networkDialogMetrics)" == "$(ipc networkDialogMetricTargets)" ]] \
        || fail "networkauth: card padding/gap $(ipc networkDialogMetrics) differs from scaled 16/12 target $(ipc networkDialogMetricTargets)"

    local title_case want_protocol want_fields
    for title_case in \
        "SFTP|Label|Host|Port|Path|Username|Password" \
        "FTPS|Label|Host|Port|Path|Username|Password|TLS" \
        "WebDAV|Label|Host|Port|Path|Username|Password|TLS"; do
        IFS='|' read -r want_protocol want_fields <<< "$title_case"
        click_chip "$want_protocol"
        [[ "$(ipc networkTitle)" == "Add a Network Share" ]] \
            || fail "networkauth: $want_protocol moved the title to $(ipc networkTitle)"
        [[ "$(ipc networkFields)" == "$want_fields" ]] \
            || fail "networkauth: $want_protocol fields are $(ipc networkFields)"
    done

    click_chip NFS
    settle
    [[ "$(ipc networkTitle)" == "Add a Network Share" ]] || fail "networkauth: NFS moved the title"
    [[ "$(ipc networkFields)" == "Label|Host|Port|Export" ]] || fail "networkauth: NFS fields are $(ipc networkFields)"
    [[ "$(ipc networkNote)" == "No password needed. The NFS server trusts this computer." ]] \
        || fail "networkauth: NFS note is $(ipc networkNote)"
    [[ -z "$(ipc networkPasswordHint)" ]] || fail "networkauth: NFS shows the password hint $(ipc networkPasswordHint)"

    click_chip SMB
    key "slot.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "data" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "tester" >/dev/null
    key -k Tab >/dev/null
    [[ "$(ipc networkFocus)" == "Password" ]] || fail "networkauth: Password did not follow Username"
    printf '%s' "$runtime_canary" | omarchy-drive key --window com.thisisgm.flea - >/dev/null
    [[ "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: typed password is not masked"
    shot networkauth-password-masked
    assert_runtime_canary_absent
    local eye_centre eye_x eye_y wx wy held_password_state released_password_state
    eye_centre=$(ipc networkPasswordEyeCentre 2>/dev/null) \
        || fail "networkauth: password eye has no IPC centre"
    read -r eye_x eye_y <<< "$eye_centre"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive move "$((wx + eye_x))" "$((wy + eye_y))" >/dev/null
    ydotool click 0x40 >/dev/null 2>&1
    held_password_state=$(ipc networkPasswordState)
    ydotool click 0x80 >/dev/null 2>&1
    released_password_state=$(ipc networkPasswordState)
    [[ "$held_password_state" == "visible|set" ]] \
        || fail "networkauth: password eye did not reveal only while held"
    [[ "$released_password_state" == "masked|set" ]] \
        || fail "networkauth: password eye did not remask on release"
    key -k Return >/dev/null
    wait_network_result mounted
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "networkauth: successful save left dialog open"
    [[ "$(cat "$helper_log")" == "argc=1 uri=smb://tester@slot.test/data stdin-lines=1" ]] \
        || fail "networkauth: SMB helper route is $(cat "$helper_log")"
    # A save lands in Flea Favorites, never the shared GTK file, and it carries no secret.
    local saved_favourite
    saved_favourite=$(ipc uiSettings | jq -c '.places.favourites[-1]')
    [[ "$saved_favourite" == '{"label":"data","path":"smb://tester@slot.test/data"}' ]] \
        || fail "networkauth: the saved favourite is $saved_favourite"
    [[ -z "$(cat "$bookmarks")" ]] \
        || fail "networkauth: a save wrote the shared GTK file, it now reads: $(cat "$bookmarks")"
    ipc uiSettings | grep -F -q -- "$runtime_canary" \
        && fail "networkauth: the saved favourite carries the password"

    # An already-mounted authenticated row resolves directly and never needs the process password.
    local helper_calls
    helper_calls=$(wc -l < "$helper_log")
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_network_entry_state true
    key -k Tab >/dev/null
    key g >/dev/null
    settle
    rail_seek auth-test
    key -k Return >/dev/null
    wait_network_result mounted
    [[ "$(ipc dialogOpen)" == "false" ]] \
        || fail "networkauth: already-mounted row asked for a password"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: already-mounted row launched helper"

    # A restart erases the process map. The saved credentialed row must reopen populated and must
    # not launch the helper until the user supplies a new password.
    : > "$state/mounted"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_rail_label data
    key -k Tab >/dev/null
    key g >/dev/null
    settle
    rail_seek data
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" && "$(ipc networkUri)" == "smb://tester@slot.test/data" ]] \
        || fail "networkauth: missing session credential did not reopen populated"
    [[ "$(ipc networkAction)" == "Retry" && "$(ipc networkPasswordState)" == "masked|empty" ]] \
        || fail "networkauth: missing credential state is $(ipc networkAction)/$(ipc networkPasswordState)"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: missing credential launched helper"

    # Missing and non-executable helpers both fail closed, retain the secret in process memory for
    # Retry, and never turn the password into an argument, environment value or log line.
    local _field
    for _field in Port Share Domain Username Password; do key -k Tab >/dev/null; done
    [[ "$(ipc networkFocus)" == "Password" ]] \
        || fail "networkauth: missing-helper setup did not reach Password"
    printf '%s' "$runtime_canary" | omarchy-drive key --window com.thisisgm.flea - >/dev/null
    mv "$dir/bin/flea-gio-auth" "$dir/bin/flea-gio-auth.real"
    helper_attempts=$(wc -l < "$attempt_log")
    key -k Return >/dev/null
    networkauth_wait_attempt "$((helper_attempts + 1))" 127
    wait_network_status "Could not connect: Flea could not ask for the password." 10
    [[ "$(ipc dialogOpen)" == "true" \
        && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: missing helper did not fail closed and retain fields"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: missing helper wrote helper output"

    cp "$dir/bin/flea-gio-auth.real" "$dir/bin/flea-gio-auth"
    chmod 0644 "$dir/bin/flea-gio-auth"
    helper_attempts=$(wc -l < "$attempt_log")
    key -k Return >/dev/null
    networkauth_wait_attempt "$((helper_attempts + 1))" 126
    wait_network_status "Could not connect: Flea could not ask for the password." 10
    [[ "$(ipc dialogOpen)" == "true" \
        && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: permission-denied helper did not fail closed and retain fields"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: permission-denied helper wrote helper output"

    # A helper that never exits must be stopped by the product deadline, not by this driver's wait.
    cat > "$dir/bin/flea-gio-auth" <<'EOS'
#!/bin/sh
IFS= read -r _password || exit 3
sleep 120
EOS
    chmod +x "$dir/bin/flea-gio-auth"
    key -k Return >/dev/null
    wait_network_result mounting 5
    wait_network_result failed 40
    [[ "$(ipc dialogOpen)" == "true" \
        && "$(ipc networkStatus)" == "Could not connect: the server did not respond." \
        && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: helper timeout did not retain one sanitized Retry artifact"

    # After another restart the same saved row has no map entry. Submitting its populated form with
    # an empty password keeps it open and does not launch any helper.
    cp "$dir/bin/flea-gio-auth.real" "$dir/bin/flea-gio-auth"
    chmod +x "$dir/bin/flea-gio-auth"
    key -k Escape >/dev/null
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_rail_label data
    key -k Tab >/dev/null
    key g >/dev/null
    settle
    rail_seek data
    key -k Return >/dev/null
    settle
    helper_calls=$(wc -l < "$helper_log")
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" && "$(ipc networkResult)" == "missing-credential" \
        && "$(ipc networkAction)" == "Retry" && "$(ipc networkPasswordState)" == "masked|empty" ]] \
        || fail "networkauth: empty password did not remain a missing credential"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: empty password launched helper"

    for _field in Port Share Domain Username Password; do key -k Tab >/dev/null; done
    [[ "$(ipc networkFocus)" == "Password" ]] \
        || fail "networkauth: corrected-retry setup did not reach Password"
    printf '%s' "$runtime_canary" | omarchy-drive key --window com.thisisgm.flea - >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted
    [[ "$(cat "$state/info-uri")" == "smb://tester@slot.test/data" ]] \
        || fail "networkauth: corrected-retry setup did not resolve first location"

    # #181: the share opened once its credential landed, and focus followed that open into it.
    [[ "$(ipc focusView)" == "list" ]] \
        || fail "networkauth: the corrected retry's open did not take focus into the share, focus is $(ipc focusView)"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] \
        || fail "networkauth: corrected-retry setup did not return to rail"

    # The approved failed-connect artifact is FTPS: keep every field, mask the password, say one
    # sentence and replace Save with Retry.
    key a >/dev/null
    click_chip FTPS
    key "slot.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "retry" >/dev/null
    key -k Tab >/dev/null
    key "tester" >/dev/null
    key -k Tab >/dev/null
    printf '%s' "$runtime_canary" | omarchy-drive key --window com.thisisgm.flea - >/dev/null
    : > "$state/fail"
    key -k Return >/dev/null
    wait_network_result failed
    # Rule 6 again: a reopen after a failed connect is a connect, not an add, and it says which failed.
    [[ "$(ipc dialogOpen)" == "true" && "$(ipc networkTitle)" == "Could not connect to the network share" ]] \
        || fail "networkauth: failure did not reopen approved FTPS artifact, title $(ipc networkTitle)"
    [[ "$(ipc networkStatus)" == "Could not connect: the network location could not be opened." ]] \
        || fail "networkauth: failure said $(ipc networkStatus)"
    [[ "$(ipc networkAction)" == "Retry" && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: failure lost Retry or masked credential state"
    [[ "$(ipc networkUri)" == "ftps://tester@slot.test/retry" ]] \
        || fail "networkauth: failure lost fields, URI is $(ipc networkUri)"
    assert_runtime_canary_absent

    # The helper's refusal status (a server that asked again) names the refusal and keeps the same Retry.
    helper_calls=$(wc -l < "$helper_log")
    rm -f "$state/fail"
    : > "$state/refuse"
    key -k Return >/dev/null
    wait_network_status "Could not connect: the server did not accept the password."
    [[ "$(wc -l < "$helper_log")" -eq $((helper_calls + 1)) ]] \
        || fail "networkauth: refused Retry did not launch helper once"
    [[ "$(ipc networkResult)" == "failed" && "$(ipc dialogOpen)" == "true" \
        && "$(ipc networkTitle)" == "Could not connect to the network share" ]] \
        || fail "networkauth: refusal did not keep the failed connect, title $(ipc networkTitle)"
    [[ "$(ipc networkAction)" == "Retry" && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: refusal lost Retry or masked credential state"
    [[ "$(ipc networkUri)" == "ftps://tester@slot.test/retry" ]] \
        || fail "networkauth: refusal lost fields, URI is $(ipc networkUri)"
    assert_runtime_canary_absent

    helper_calls=$(wc -l < "$helper_log")
    rm -f "$state/refuse"
    key -k Return >/dev/null
    wait_network_result mounted
    [[ "$(wc -l < "$helper_log")" -eq $((helper_calls + 1)) ]] \
        || fail "networkauth: corrected Retry did not launch helper once"
    [[ "$(tail -n 1 "$helper_log")" == "argc=1 uri=ftps://tester@slot.test/retry stdin-lines=1" ]] \
        || fail "networkauth: corrected Retry used stale helper URI"
    [[ "$(cat "$state/info-uri")" == "ftps://tester@slot.test/retry" ]] \
        || fail "networkauth: corrected Retry resolved stale info URI"
    [[ "$(cat "$state/mounted")" == "ftps://tester@slot.test/retry" ]] \
        || fail "networkauth: corrected Retry did not mount current URI"
    wait_path_wall "$retry_dir" 10
    wait_listing_wall 1 10
    [[ "$(ipc rowAt 0)" == "alpha.txt|"* ]] \
        || fail "networkauth: corrected Retry browsed stale child"
    [[ "$(ipc dialogOpen)" == "false" && "$(ipc shareBrowserOpen)" == "false" ]] \
        || fail "networkauth: corrected Retry left stale failure UI (dialog $(ipc dialogOpen), shares $(ipc shareBrowserOpen), status $(ipc networkStatus), result $(ipc networkResult))"
    assert_runtime_canary_absent
    sleep "$transient_clear_s"
    [[ -z "$(ipc lastMessage)" ]] \
        || fail "networkauth: corrected Retry retained stale error text"

    printf 'NETWORKAUTH artifact=ok stdin=one persistence=none retry=corrected refused=named eye=held missing-session=no-launch\n'
    unset FLEA_GIO_AUTH
    export PATH="$saved_path"
    kill_flea
    sandbox_remove "$fixture_home"
}
