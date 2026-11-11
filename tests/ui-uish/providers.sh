# GPUI overrides for UISH-14; seeded from the unchanged oracle at 2faf8d099.
case_sharebrowser() {
    local dir="$fixture_root/sharebrowser"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"

    local share1_dir="$fixture_root/sharebrowser-share1"
    local share2_dir="$fixture_root/sharebrowser-share2"
    sandbox_remove "$share1_dir"; sandbox_remove "$share2_dir"
    mkdir -p "$share1_dir" "$share2_dir"
    : > "$share1_dir/one.txt"
    : > "$share2_dir/one.txt"
    : > "$share2_dir/two.txt"

    local base_uri="smb://stubhost/"
    local share1_uri="smb://stubhost/share1/"
    local share2_uri="smb://stubhost/share2/"
    local share3_uri="smb://stubhost/share3/"

    # A plain dispatcher, not a canned fixture: it answers exactly the four gio subcommands
    # ui/NetworkMounts.qml issues, keyed on the exact uri each entry activates.
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
# The gio client translates its own output, so it answers English only where the caller pinned C.
local_path_label='ruta local'
[ "\$LC_ALL" = C ] && local_path_label='local path'
case "\$1" in
  mount)
    if [ "\$2" = "-l" ] || [ "\$2" = "-u" ]; then
      exit 0
    fi
    # The product passes --anonymous on smb, so the location is the last argument, not the second.
    shift \$(( \$# - 1 ))
    # Sample last argument: smb://stubhost/share3 or smb://stubhost/share3/.
    asked="\${1%/}/"
    if [ "\$asked" = "$share2_uri" ]; then
      # gvfsd composes this refusal, so no client locale makes it English: it stays Spanish.
      echo "gio: \$1: La ubicacion ya esta montada" >&2
      exit 2
    fi
    if [ "\$asked" = "$share3_uri" ]; then
      # A genuine failure, translated by the same daemon, and the control the already-mounted half
      # never had: same nonzero exit, opposite verdict, told apart only by the gio info below.
      echo "gio: \$1: No se pudo conectar con el servidor" >&2
      exit 1
    fi
    exit 0
    ;;
  info)
    # The URI follows gio options; accept either trailing-slash spelling.
    shift \$(( \$# - 1 ))
    asked="\${1%/}/"
    case "\$asked" in
      "$share1_uri") printf '%s: %s\n' "\$local_path_label" "$share1_dir" ;;
      "$share2_uri") printf '%s: %s\n' "\$local_path_label" "$share2_dir" ;;
      # A location gio could not describe at all: no line, nonzero, which is the whole difference.
      "$share3_uri") exit 1 ;;
    esac
    exit 0
    ;;
  list)
    printf 'share1\nshare2\nshare3\n'
    exit 0
    ;;
esac
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/sharebrowser-home"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf '%s StubNAS\n' "$base_uri" > "$fixture_home/.config/gtk-3.0/bookmarks"
    local real_home="$HOME"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/ is the gio stub's own fixture entry, alongside the two files under test.
    wait_listing 3
    wait_rail 2
    [[ "$(ipc networkEntries)" == "StubNAS|network|share|false" ]] \
        || fail "sharebrowser: the stub NAS bookmark did not appear, got $(ipc networkEntries)"

    # The overlay's box in the lazy views first: placed on a lazy view's item it sat at the Loader's origin, and that is where the old coordinates put it.
    local mode
    for mode in grid columns; do
        switch_view "$mode"
        key -k Tab >/dev/null
        settle
        key g >/dev/null
        settle
        rail_seek StubNAS
        key l >/dev/null
        for _attempt in $(seq 1 100); do
            [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
            sleep 0.05
        done
        [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: $mode: l on the bare root never opened the overlay"
        rect_is "$(ipc shareBrowserRect)" $(ipc listAreaRect) 1 \
            || fail "sharebrowser: $mode: the overlay's box is $(ipc shareBrowserRect), not the listing slot $(ipc listAreaRect)"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc shareBrowserOpen)" == "false" ]] || fail "sharebrowser: $mode: Escape did not close the overlay"
        key -k Escape >/dev/null
        settle
    done
    switch_view list

    # Tab to the rail and l the bare-root entry: it lists shares, it does not open anything.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "sharebrowser: Tab did not reach the rail"
    key g >/dev/null
    settle
    rail_seek StubNAS
    key l >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: l on the bare root never opened the overlay"
    rect_is "$(ipc shareBrowserRect)" $(ipc listAreaRect) 1 \
        || fail "sharebrowser: list: the overlay's box is $(ipc shareBrowserRect), not the listing slot $(ipc listAreaRect)"
    [[ "$(ipc shareBrowserEntries)" == "$(printf 'share1\nshare2\nshare3')" ]] \
        || fail "sharebrowser: the overlay's own shares over IPC are wrong: $(ipc shareBrowserEntries)"
    [[ "$(ipc path)" == "$dir" ]] || fail "sharebrowser: listing the shares navigated away from $dir"
    shot sharebrowser-open

    # Escape returns to the listing underneath without mounting or navigating anywhere.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc shareBrowserOpen)" == "false" ]] || fail "sharebrowser: Escape did not close the overlay"
    [[ "$(ipc path)" == "$dir" ]] || fail "sharebrowser: Escape navigated to $(ipc path)"
    [[ "$(ipc total)" == "3" ]] || fail "sharebrowser: Escape changed the listing underneath"
    # A second Escape hands focus back to the list, ui/js/RailKeys.js "act"'s own escape case;
    # case_network's own post-dialog check relies on the exact same mechanism.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "sharebrowser: focus never returned to the list"
    key j >/dev/null
    settle
    [[ "$(ipc cursor)" == "1" ]] || fail "sharebrowser: keyboard nav is dead after Escape, cursor is $(ipc cursor)"

    # GPUI gives the active overlay its own focus handle; l must route through that observed owner.
    click_rail_row "$(rail_row_of StubNAS)" left
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: pointer reactivation of StubNAS did not reopen the overlay"
    ipc shareBrowserState | jq -e '.active and .focused' >/dev/null \
        || fail "sharebrowser: pointer activation did not focus the active overlay: $(ipc shareBrowserState)"
    [[ "$(ipc shareBrowserCursor)" == "0" ]] || fail "sharebrowser: the overlay did not reset its cursor to 0, got $(ipc shareBrowserCursor)"
    key l >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc path)" == "$share1_dir" ]] && break
        sleep 0.05
    done
    [[ "$(ipc path)" == "$share1_dir" ]] || fail "sharebrowser: l on share1 never opened $share1_dir, path is $(ipc path)"
    [[ "$(ipc shareBrowserOpen)" == "false" ]] || fail "sharebrowser: opening share1 did not close the overlay"
    [[ "$(ipc total)" == "1" ]] || fail "sharebrowser: share1's own listing did not load, total is $(ipc total)"
    shot sharebrowser-opened-share1

    # Pointer reopens the focused overlay; Return drives the already-mounted share.
    [[ "$(ipc focusView)" == "list" ]] || fail "sharebrowser: opening share1 moved focus to $(ipc focusView)"
    click_rail_row "$(rail_row_of StubNAS)" left
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: second pointer reactivation of StubNAS did not reopen the overlay"
    ipc shareBrowserState | jq -e '.active and .focused' >/dev/null \
        || fail "sharebrowser: second pointer activation did not focus the overlay: $(ipc shareBrowserState)"
    key j >/dev/null
    settle
    [[ "$(ipc shareBrowserCursor)" == "1" ]] || fail "sharebrowser: cursor did not move to share2, got $(ipc shareBrowserCursor)"
    key -k Return >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc path)" == "$share2_dir" ]] && break
        sleep 0.05
    done
    [[ "$(ipc path)" == "$share2_dir" ]] \
        || fail "sharebrowser: the already-mounted quirk was misreported as a failure, path is $(ipc path), message=$(ipc lastMessage)"
    [[ "$(ipc total)" == "2" ]] || fail "sharebrowser: share2's own listing did not load, total is $(ipc total)"

    # share3 fails the same way share2 refused, and the opposite thing has to happen: the bar names
    # the failure and the pane stays where it is. Without this arm a product that read any nonzero
    # mount exit as the harmless already-mounted case would pass every assertion above.
    click_rail_row "$(rail_row_of StubNAS)" left
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: third pointer reactivation of StubNAS did not reopen the overlay"
    key j >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc shareBrowserCursor)" == "2" ]] || fail "sharebrowser: cursor did not move to share3, got $(ipc shareBrowserCursor)"
    key -k Return >/dev/null
    wait_message "Could not connect: the server refused the connection."
    [[ "$(ipc path)" == "$share2_dir" ]] \
        || fail "sharebrowser: a mount that genuinely failed navigated to $(ipc path)"

    printf 'SHAREBROWSER list=ok escape=ok mount-open=ok already-mounted-quirk=ok mount-failure=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"; sandbox_remove "$share1_dir"; sandbox_remove "$share2_dir"
}

case_taildrop() (
    local menu_box="$fixture_root/taildrop" menu_dir="$fixture_root/taildrop/list" menus_checks=0
    local taildrop_fd dropbox_fd before provider_ready
    provider_ready=$(jq -cn '{BackendState:"Running",Self:{UserID:1001,Capabilities:["https://tailscale.com/cap/file-sharing"]},Peer:{
        "first-peer":{HostName:"Alpha",DNSName:"unused.invalid.",Online:true,TaildropTarget:1,UserID:1001},
        "second-peer":{HostName:"Bravo",DNSName:"fixture.invalid.",Online:true,TaildropTarget:1,UserID:1001}}}') \
        || fail 'taildrop: cannot construct private peer fixture'
    providers_fixture
    trap 'providers_cleanup || exit 1' EXIT
    menus_guard "$menu_dir/adir"
    mkdir "$menu_dir/adir" || fail 'taildrop: cannot create directory eligibility fixture'
    menus_guard "$menu_box/open-bin"
    mkdir "$menu_box/open-bin" || fail 'taildrop: cannot create private opener directory'
    [[ "$open_handoff" == gio ]] || fail 'taildrop: opener contract changed; isolate the new handoff before running'
    menus_guard "$menu_box/open-bin/$open_handoff"
    cat > "$menu_box/open-bin/$open_handoff" <<'EOS'
#!/usr/bin/env bash
set -eu
[[ "${1:-}" == open ]] || exec /usr/bin/gio "$@"
box=${FLEA_PROVIDERS_BOX:?}
[[ "$box" == /* && -f "$box/.flea-test-sandbox" ]] || exit 90
log=$(realpath -m -- "$box/calls.jsonl")
[[ "$log" == "$box/"* && "$log" != "$box" ]] || exit 91
jq -cn --arg helper gio --args '{helper:$helper,args:$ARGS.positional}' -- "$@" >> "$log"
EOS
    chmod 700 "$menu_box/open-bin/$open_handoff" || fail 'taildrop: cannot make the private opener executable'
    providers_install tailscale yes
    providers_install omarchy-tailscale-send yes
    export HOME="$menu_box/home" XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config"
    export XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    export PATH="$menu_box/open-bin:$menu_box/bin" FLEA_PROVIDERS_BOX="$menu_box"
    "$flea_bin" --ui-state '{"view":"list","keys":"default","menu":{"hidden":[]}}' >/dev/null \
        || fail 'taildrop: private settings seed failed'
    launch "$menu_dir"
    wait_listing 3
    providers_open adir pointer
    providers_disabled taildrop 'Taildrop sends files only' 'files only'
    menus_expect menuState 'any(.entries[]; .action == "plugin-send:flea.tailscale" and .disabled and .submenu == [])' 'directory keeps its installed provider but offers no file targets'
    providers_close

    providers_open b-cursor.txt pointer
    menus_expect menuState 'any(.entries[]; .action == "plugin-send:flea.tailscale" and (.disabled | not) and .mark == "tailscale")' 'file menu uses the Tailscale brand mark'
    menus_shot taildrop-menu
    providers_seek taildrop
    key -k Return >/dev/null || fail 'taildrop: submenu Enter failed'
    menus_expect menuState '.submenu and .submenuCursor == 0 and (.submenuEntries | map(.label)) == ["Alpha","Bravo"]' 'Enter opens both fixture peers in name order'
    menus_equal 'each peer retains the machine glyph' 'server|server' "$(ipc contextMenuSubmenuGlyphs)"
    menus_shot taildrop-flyout
    key -k Down >/dev/null || fail 'taildrop: second-peer movement failed'
    menus_expect menuState '.submenu and .submenuCursor == 1 and .submenuEntries[.submenuCursor].id == "second-peer"' 'Down selects the second peer identity'
    before=$(providers_calls omarchy-tailscale-send)
    key -k Return >/dev/null || fail 'taildrop: second-peer Enter failed'
    providers_call omarchy-tailscale-send "$(jq -cn --arg path "$menu_dir/b-cursor.txt" '["fixture.invalid",$path]')" "$before"
    menus_expect menuState '(.opened | not) and (.submenu | not)' 'the send closes both native menus'
    providers_expect '.listFocus' 'the send restores listing focus'
    menus_message 'Sending b-cursor.txt to Bravo.' 'the send names the chosen peer and exact cursor file'
    menus_equal 'the send preserves source bytes' 'list/b-cursor.txt original' "$(cat "$menu_dir/b-cursor.txt")"
    menus_shot taildrop-sent

    providers_mode tailscale ready '{"BackendState":"NeedsLogin","Peer":{}}'
    providers_open b-cursor.txt pointer
    providers_disabled taildrop 'signed out' 'signed out'
    menus_expect menuState 'any(.entries[]; .action == "plugin-send:flea.tailscale" and .disabled and .submenu == [])' 'signed-out installation offers no cached peer'
    menus_shot taildrop-signed-out
    providers_close
    providers_install tailscale no
    before=$(providers_calls tailscale)
    providers_open b-cursor.txt pointer
    menus_expect menuState 'all(.entries[]; .action != "plugin-send:flea.tailscale")' 'missing installation removes the provider row'
    providers_expect '.facts.taildrop.installed == false and (.taildrop.checking | not)' 'private PATH proves the provider is absent'
    menus_equal 'absence does not attempt a missing status helper' "$before" "$(providers_calls tailscale)"
    menus_shot taildrop-absent
    providers_close
    menus_equal 'native menu input never opens a file' 0 "$(providers_calls gio)"
    menus_equal 'Taildrop never touches the clipboard recorder' 0 "$(providers_calls wl-copy)"
    printf 'TAILDROP checks=%s directory-disabled=ok brand=ok submenu=ok second-peer-recorded=ok signed-out-disabled=ok absent=ok isolated=ok\n' "$menus_checks"
)

case_providers() (
    local menu_box="$fixture_root/providers" menu_dir="$fixture_root/providers/list" menus_checks=0
    local taildrop_fd dropbox_fd localsend_fd name before action record reason word path saved
    local provider_ready='{"BackendState":"Running","Self":{"UserID":"fixture-owner","Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{"fixture-peer":{"HostName":"Fixture","DNSName":"fixture.invalid.","Online":true,"TaildropTarget":1,"UserID":"fixture-owner"}}}'
    providers_fixture
    trap 'providers_cleanup || exit 1' EXIT
    export HOME="$menu_box/home" XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config"
    export XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data" PATH="$menu_box/bin" FLEA_PROVIDERS_BOX="$menu_box"
    "$flea_bin" --ui-state '{"view":"list","keys":"default","menu":{"hidden":[]}}' >/dev/null || fail 'providers: private settings seed failed'
    launch "$menu_dir"
    wait_listing 2
    menus_expect listInFlight '. == false' 'provider fixture initial listing settles'
    providers_open
    menus_expect menuState 'all(.entries[]; .action != "plugin-send:flea.tailscale" and .action != "dropbox" and .action != "sharelink" and .action != "localsend")' 'absent providers are not built'
    menus_equal 'absent providers spawn no helper' 0 "$(jq -s length "$menu_box/calls.jsonl")"
    providers_close

    for name in tailscale dropbox-cli; do
        providers_install "$name" yes
        menus_guard "$menu_box/doubles/$name"
        chmod 600 "$menu_box/doubles/$name" || fail 'providers: cannot make installed helper unusable'
    done
    providers_install omarchy-tailscale-send yes
    providers_open
    providers_disabled taildrop 'not executable' offline
    providers_disabled dropbox 'not executable' 'not running'
    menus_equal 'unusable providers spawn no helper' 0 "$(jq -s length "$menu_box/calls.jsonl")"
    menus_shot providers-unusable
    providers_close
    for name in tailscale dropbox-cli; do menus_guard "$menu_box/doubles/$name"; chmod 700 "$menu_box/doubles/$name" || fail 'providers: executable recovery failed'; done

    while IFS='|' read -r record reason word; do
        providers_mode tailscale ready "$record"
        providers_open
        providers_disabled taildrop "$reason" "$word"
        providers_close
    done <<'STATES'
{"BackendState":"NeedsLogin"}|signed out|signed out
{"BackendState":"Stopped"}|stopped|offline
{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{}}|no peers|no devices
{"BackendState":"Running","Self":{},"Peer":{}}|disabled for this account|offline
malformed|invalid status|offline
STATES
    providers_mode tailscale ready '' 'fixture Tailscale failure' 7
    providers_open
    providers_disabled taildrop 'fixture Tailscale failure' offline
    providers_close
    providers_mode tailscale gate "$provider_ready"
    providers_open
    providers_disabled taildrop 'timed out' offline
    menus_shot providers-timeout
    providers_close
    providers_ready
    providers_install omarchy-tailscale-send no
    providers_open
    providers_disabled taildrop 'not installed' offline
    providers_close
    providers_install omarchy-tailscale-send yes

    for record in '{}' malformed '{"personal":{"path":"relative"}}'; do
        providers_write home/.dropbox/info.json "$record"
        providers_open
        if [[ "$record" == '{}' ]]; then reason='signed out'; word='signed out'; else reason='invalid'; word='not running'; fi
        providers_disabled dropbox "$reason" "$word"
        providers_close
    done
    providers_ready
    for record in '' "Dropbox isn't running!" "Dropbox isn't responding!" 'Dropbox daemon stopped.' "Couldn't get status: fixture refusal"; do
        providers_mode dropbox-cli ready "$record"
        providers_open
        reason="$record"
        [[ -n "$reason" ]] || reason='empty status'
        providers_disabled dropbox "$reason" 'not running'
        providers_close
    done
    providers_mode dropbox-cli ready '' 'fixture Dropbox failure' 7
    providers_open
    providers_disabled dropbox 'fixture Dropbox failure' 'not running'
    providers_close
    providers_mode dropbox-cli gate 'Up to date'
    providers_open
    providers_disabled dropbox 'timed out' 'not running'
    providers_close
    providers_ready

    providers_install tailscale no
    providers_open
    menus_expect menuState 'all(.entries[]; .action != "plugin-send:flea.tailscale")' 'fresh menu removes uninstalled Tailscale'
    providers_close
    providers_install tailscale yes
    providers_mode tailscale gate "$provider_ready"
    menus_file_menu b-cursor.txt key
    providers_expect '.refreshing and .taildrop.checking' 'late installed provider refresh is in flight'
    providers_seek properties
    before=$(ipc listContentY)
    providers_release tailscale
    providers_expect '(.refreshing | not) and .menuFocus' 'late provider insertion finishes with keyboard focus'
    menus_expect menuState '.entries[.cursor].action == "properties" and any(.entries[]; .action == "plugin-send:flea.tailscale" and (.disabled | not))' 'inserted provider preserves current action'
    providers_geometry
    menus_equal 'provider insertion leaves covered listing scroll unchanged' "$before" "$(ipc listContentY)"
    menus_shot providers-inserted
    providers_close
    providers_install dropbox-cli no
    menus_file_menu b-cursor.txt key
    providers_expect '.refreshing and .taildrop.checking' 'provider removal waits for the other fresh query'
    providers_seek properties
    providers_release tailscale
    providers_expect '(.refreshing | not) and .menuFocus' 'provider removal finishes with keyboard focus'
    menus_expect menuState '.entries[.cursor].action == "properties" and all(.entries[]; .action != "dropbox")' 'removed provider preserves current action'
    providers_geometry
    providers_close
    providers_install dropbox-cli yes
    providers_ready

    providers_selection "$menu_dir"
    before=$(providers_calls omarchy-tailscale-send)
    providers_choose taildrop
    providers_call omarchy-tailscale-send "$(jq -cn --arg path "$menu_dir/b-cursor.txt" '["fixture.invalid",$path]')" "$before"
    menus_message 'Sending b-cursor.txt to Fixture.' 'native Taildrop passes only the unmarked cursor to the local recorder'
    menus_equal 'Taildrop recorder preserves cursor file' 'list/b-cursor.txt original' "$(cat "$menu_dir/b-cursor.txt")"
    menus_equal 'Taildrop recorder preserves marked file' 'list/a-marked.txt original' "$(cat "$menu_dir/a-marked.txt")"

    providers_selection "$menu_dir"
    before=$(providers_calls omarchy-tailscale-send)
    providers_mode tailscale gate '{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{}}'
    providers_choose taildrop
    providers_expect '.refreshing and .pendingActivation and .taildrop.checking' 'chosen peer awaits fresh availability'
    providers_release tailscale
    menus_error 'That action is no longer available' 'a disappeared peer cannot inherit the queued send'
    providers_expect '(.refreshing | not) and (.pendingActivation | not)' 'peer refusal clears pending activation'
    menus_equal 'disappeared peer never reaches sender' "$before" "$(providers_calls omarchy-tailscale-send)"
    menus_acknowledge
    providers_ready

    for action in taildrop sharelink; do
        if [[ "$action" == taildrop ]]; then path="$menu_dir"; name=tailscale
        else path="$menu_box/Dropbox"; name=dropbox-cli; fi
        providers_selection "$path"
        before=$(providers_calls "$([[ "$action" == taildrop ]] && printf omarchy-tailscale-send || printf dropbox-cli)")
        providers_mode "$name" gate "$([[ "$name" == tailscale ]] && printf '%s' "$provider_ready" || printf 'Up to date')"
        providers_choose "$action"
        providers_expect '.refreshing and .pendingActivation' 'provider activation waits for a fresh status reply'
        saved="$menu_box/retired/$action-original"
        menus_guard "$path/b-cursor.txt"
        menus_guard "$saved"
        mv -- "$path/b-cursor.txt" "$saved" || fail 'providers: cannot retain cursor original'
        providers_write "${path#"$menu_box/"}/b-cursor.txt" replacement
        providers_release "$name"
        # Validation retains the backend's identity refusal; DEF2 also preserves this detail in the status line.
        menus_error 'Selected item changed; reopen the menu.' "$action refuses an asynchronously replaced unmarked cursor"
        providers_expect '(.refreshing | not) and (.pendingActivation | not)' 'refused activation has no pending provider work'
        if [[ "$action" == taildrop ]]; then menus_equal 'stale Taildrop never reaches sender' "$before" "$(providers_calls omarchy-tailscale-send)"
        else menus_equal 'stale Share Link makes only its fresh status query' "$((before + 1))" "$(providers_calls dropbox-cli)"; fi
        menus_equal 'cursor refusal preserves replacement' replacement "$(cat "$path/b-cursor.txt")"
        menus_equal 'cursor refusal preserves original' "${path#"$menu_box/"}/b-cursor.txt original" "$(cat "$saved")"
        menus_acknowledge
        providers_ready
    done

    providers_sharelink_checks
    providers_dropbox_move_checks
    providers_localsend_checks

    providers_selection "$menu_dir"
    providers_mode dropbox-cli gate 'Up to date'
    providers_choose dropbox
    providers_expect '.refreshing and .pendingActivation and .dropbox.checking' 'Dropbox activation refresh is in flight'
    menus_guard "$menu_box/Dropbox"
    menus_guard "$menu_box/retired/Dropbox"
    mv -- "$menu_box/Dropbox" "$menu_box/retired/Dropbox" || fail 'providers: cannot retain destination original'
    menus_guard "$menu_box/Dropbox"
    mkdir "$menu_box/Dropbox" || fail 'providers: cannot replace fixture destination'
    providers_release dropbox-cli
    # The destination must name Dropbox as v0.3.12 did; the cargo regression also pins its complete backend sentence.
    menus_error 'Dropbox account folder changed' 'Move to Dropbox refuses a replaced account directory'
    providers_expect '(.refreshing | not) and (.pendingActivation | not)' 'destination refusal clears pending activation'
    menus_equal 'refused destination remains empty' 0 "$(find "$menu_box/Dropbox" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
    menus_equal 'destination refusal preserves marked source' 'list/a-marked.txt original' "$(cat "$menu_dir/a-marked.txt")"
    menus_equal 'destination refusal preserves cursor replacement' replacement "$(cat "$menu_dir/b-cursor.txt")"
    menus_shot providers-destination-refusal
    printf 'PROVIDERS_NATIVE_CHECKS=%s\n' "$menus_checks"
    printf 'PROVIDERS_UNVERIFIED worker-stage Dropbox retry, offline daemon wording, helper launch race, concurrent panes, all-preset provider combinations, matched-size pixels\n'
)
