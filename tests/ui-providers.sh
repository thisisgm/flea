#!/usr/bin/env bash
# Sourced by ui.sh; provider doubles record calls, and all application changes enter through native input.
# shellcheck disable=SC2154 # ui.sh supplies the candidate paths and fixture root.
providers_write() {
    menus_guard "$menu_box/$1"
    printf '%s' "$2" > "$menu_box/$1" || fail "providers: cannot write $1"
}

providers_expect() {
    local expression="$1" label="$2" observed deadline=$((SECONDS + 25))
    while (( SECONDS < deadline )); do
        observed=$(ipc providerState) || fail "providers: state observer failed"
        if jq -e "$expression" <<< "$observed" >/dev/null; then
            menus_checks=$((menus_checks + 1))
            printf 'PROVIDERS_CHECK %s %s state=%s\n' "$menus_checks" "$label" "$observed"
            return
        fi
        sleep 0.05
    done
    fail "providers: $label: $observed"
}

providers_seek() {
    local action="$1" state cursor target step index
    local -a events=()
    state=$(ipc menuState) || fail "providers: menu observer failed"
    target=$(jq -er --arg action "$action" '.entries | to_entries[] | select(.value.action == $action and (.value.disabled | not)) | .key' <<< "$state") \
        || fail "providers: unavailable action $action: $state"
    cursor=$(jq -r .cursor <<< "$state")
    if (( target > cursor )); then step=1; else step=-1; fi
    for ((index = cursor + step; index != target + step; index += step)); do
        if jq -e --argjson index "$index" '.entries[$index] | (.separator | not) and (.disabled | not)' <<< "$state" >/dev/null; then
            if (( step > 0 )); then events+=(-k Down); else events+=(-k Up); fi
        fi
    done
    if (( ${#events[@]} )); then key "${events[@]}" >/dev/null || fail "providers: menu keys failed"; fi
    menus_expect menuState ".entries[.cursor].action == \"$action\"" "keyboard reaches $action"
}

providers_calls() {
    jq -s --arg helper "$1" '[.[] | select(.helper == $helper)] | length' "$menu_box/calls.jsonl"
}

providers_call() {
    local helper="$1" expected="$2" before="$3" seen deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        seen=$(jq -sc --arg helper "$helper" '[.[] | select(.helper == $helper)]' "$menu_box/calls.jsonl") \
            || fail "providers: invalid invocation record"
        if jq -e --argjson before "$before" --argjson expected "$expected" \
            'length == ($before + 1) and .[-1].args == $expected' <<< "$seen" >/dev/null; then
            menus_checks=$((menus_checks + 1))
            printf 'PROVIDERS_CALL %s %s\n' "$helper" "$seen"
            return
        fi
        sleep 0.05
    done
    fail "providers: expected one $helper call with $expected after $before calls: $seen"
}

providers_install() {
    local name="$1" installed="$2" source destination
    if [[ "$installed" == yes ]]; then source="$menu_box/absent/$name"; destination="$menu_box/bin/$name"
    else source="$menu_box/bin/$name"; destination="$menu_box/absent/$name"; fi
    if [[ -e "$destination" ]]; then return; fi
    menus_guard "$source"
    menus_guard "$destination"
    [[ -e "$source" ]] || fail "providers: missing owned helper $source"
    mv -- "$source" "$destination" || fail "providers: cannot change fixture installation $name"
}

providers_mode() {
    providers_write "$1-mode" "$2"
    providers_write "$1-output" "$3"
    providers_write "$1-error" "${4:-}"
    providers_write "$1-exit" "${5:-0}"
}

providers_release() {
    local provider="$1"
    menus_guard "$menu_box/$provider-release"
    [[ -p "$menu_box/$provider-release" && ! -L "$menu_box/$provider-release" ]] || fail "providers: release is not an owned FIFO"
    if [[ "$provider" == tailscale ]]; then printf 'release\n' >&"$taildrop_fd" || fail 'providers: Tailscale gate write failed'
    elif [[ "$provider" == dropbox-cli ]]; then printf 'release\n' >&"$dropbox_fd" || fail 'providers: Dropbox gate write failed'
    else fail "providers: unknown release $provider"; fi
}

providers_close() {
    key -k Escape >/dev/null || fail "providers: menu Escape failed"
    menus_expect menuState '(.opened | not) and (.submenu | not)' 'Escape closes provider menu'
    providers_expect '.listFocus' 'provider menu restores listing focus'
}

providers_ready() {
    providers_mode tailscale ready "$provider_ready"
    providers_mode dropbox-cli ready 'Up to date'
    providers_mode sharelink ready 'https://fixture.invalid/share'
    providers_mode wl-copy ready ''
    providers_write home/.dropbox/info.json "$(jq -cn --arg path "$menu_box/Dropbox" '{personal:{path:$path}}')"
}

providers_open() {
    menus_file_menu "${1:-b-cursor.txt}" "${2:-key}"
    providers_expect '(.refreshing | not) and (.taildrop.checking | not) and (.dropbox.checking | not)' 'provider entry refresh finishes'
}

providers_disabled() {
    local action="$1" reason="$2" state target before
    # GM ruled a provider that cannot answer reads as red, so the row carries no reason at all.
    menus_expect menuState "any(.entries[]; .action == \"$action\" and .disabled and .errored and (.hint == null))" "$action reads as an error ($reason)"
    state=$(ipc menuState)
    target=$(jq -er --arg action "$action" '.entries | to_entries[] | select(.value.action == $action) | .key' <<< "$state")
    before=$(providers_calls omarchy-tailscale-send)
    providers_seek copy
    menus_point "$(ipc contextMenuRowCentre "$target")"
    menus_expect menuState '.opened and .entries[.cursor].action == "copy"' 'disabled provider pointer activation preserves the current action'
    menus_equal 'disabled row sends nothing' "$before" "$(providers_calls omarchy-tailscale-send)"
}

providers_geometry() {
    local state x y width height area_x area_y area_width area_height
    state=$(ipc menuState)
    read -r x y width height <<< "$(jq -r .frame <<< "$state")"
    read -r area_x area_y area_width area_height <<< "$(jq -r '.workArea | [.x,.y,.width,.height] | join(" ")' <<< "$state")"
    jq -en --argjson x "$x" --argjson y "$y" --argjson width "$width" --argjson height "$height" \
        --argjson ax "$area_x" --argjson ay "$area_y" --argjson aw "$area_width" --argjson ah "$area_height" \
        '$width > 0 and $height > 0 and $x >= $ax and $y >= $ay and ($x+$width) <= ($ax+$aw) and ($y+$height) <= ($ay+$ah)' >/dev/null \
        || fail "providers: refreshed menu escaped work area: $state"
    providers_expect '.menuFocus' 'provider refresh retains menu keyboard focus'
    printf 'PROVIDERS_GEOMETRY %s\n' "$state"
}

providers_fixture() {
    local part file name target
    local -a parts
    sandbox_scratch "$menu_box"
    : > "$menu_box/.flea-test-sandbox" || fail 'providers: sandbox marker write failed'
    for target in list Dropbox retired bin absent doubles state config cache data; do
        menus_guard "$menu_box/$target"
        mkdir -p "$menu_box/$target" || fail "providers: fixture directory $target failed"
    done
    fixture_home_make "$menu_box/home"
    menus_guard "$menu_box/home/.dropbox"
    mkdir "$menu_box/home/.dropbox" || fail 'providers: private account directory failed'
    for target in list/a-marked.txt list/b-cursor.txt Dropbox/a-marked.txt Dropbox/b-cursor.txt; do
        providers_write "$target" "$target original"
    done
    providers_write calls.jsonl ''
    for name in tailscale dropbox-cli; do
        menus_guard "$menu_box/$name-release"
        mkfifo "$menu_box/$name-release" || fail "providers: cannot make $name gate"
    done
    exec {taildrop_fd}<>"$menu_box/tailscale-release" || fail 'providers: cannot open Tailscale gate'
    exec {dropbox_fd}<>"$menu_box/dropbox-cli-release" || fail 'providers: cannot open Dropbox gate'
    # Excluding every real provider prevents removal of a double from falling through to GM's account.
    IFS=: read -r -a parts <<< "$PATH"
    for part in "${parts[@]}"; do
        [[ -d "$part" ]] || continue
        for file in "$part"/*; do
            [[ -f "$file" && -x "$file" ]] || continue
            name=${file##*/}
            case "$name" in tailscale|omarchy-tailscale-send|dropbox-cli|wl-copy) continue ;; esac
            [[ ! -e "$menu_box/bin/$name" ]] || continue
            menus_guard "$menu_box/bin/$name"
            ln -s -- "$file" "$menu_box/bin/$name" || fail "providers: cannot retain required command $name"
        done
    done
    menus_guard "$menu_box/doubles/provider"
    cat > "$menu_box/doubles/provider" <<'SH'
#!/usr/bin/env bash
set -eu
box=${FLEA_PROVIDERS_BOX:?}
[[ "$box" == /* && -f "$box/.flea-test-sandbox" ]] || exit 90
guard() {
    [[ -n "$1" && "$1" == /* ]] || exit 91
    local resolved
    resolved=$(realpath -m -- "$1") || exit 91
    [[ "$resolved" == "$box/"* && "$resolved" != "$box" ]] || exit 91
}
name=${0##*/}
guard "$box/calls.jsonl"
response=''
case "$name:$#:${1:-}:${2:-}" in
    tailscale:2:status:--json|dropbox-cli:1:status:)
        response=$name
        ;;
    omarchy-tailscale-send:2:fixture.invalid:*)
        guard "$2"
        [[ -f "$2" && ! -L "$2" ]] || exit 94
        ;;
    dropbox-cli:2:sharelink:*)
        guard "$2"
        [[ -f "$2" && ! -L "$2" ]] || exit 94
        response=sharelink
        ;;
    wl-copy:1:https://fixture.invalid/share:) response=wl-copy ;;
    *) printf 'REFUSED: provider fixture received unexpected arguments: %s\n' "$name" >&2; exit 95 ;;
esac
jq -cn --arg helper "$name" --args '{helper:$helper,args:$ARGS.positional}' -- "$@" >> "$box/calls.jsonl"
if [[ -n "$response" ]]; then
    guard "$box/$response-mode"
    mode=$(cat "$box/$response-mode")
    if [[ "$mode" == socket-timeout && "$response" == sharelink ]]; then
        python3 - <<'PY_SOCKET'
import socket

# Installed DropboxCommand(timeout=5) reports BadConnection as an exit-zero sentence.
socket_timeout_seconds = 5
client, server = socket.socketpair()
with client, server:
    client.settimeout(socket_timeout_seconds)
    try:
        client.recv(1)
    except socket.timeout:
        print("Dropbox isn't responding!")
    else:
        raise SystemExit("provider fixture: expected a real socket timeout")
PY_SOCKET
        exit $?
    elif [[ "$mode" == gate ]]; then
        gate=$name
        # Share Link and clipboard run after status, so they reuse the now-idle Dropbox gate.
        [[ "$response" == sharelink || "$response" == wl-copy ]] && gate=dropbox-cli
        guard "$box/$gate-release"
        [[ -p "$box/$gate-release" && ! -L "$box/$gate-release" ]] || exit 92
        read -r release < "$box/$gate-release"
        [[ "$release" == release ]] || exit 93
    elif [[ "$mode" == vanish && "$response" == dropbox-cli ]]; then
        guard "$box/bin/dropbox-cli"
        guard "$box/absent/dropbox-cli"
        [[ ! -e "$box/absent/dropbox-cli" ]] || exit 96
        mv -- "$box/bin/dropbox-cli" "$box/absent/dropbox-cli"
    fi
    for field in output error exit; do guard "$box/$response-$field"; done
    cat "$box/$response-output"
    cat "$box/$response-error" >&2
    exit "$(cat "$box/$response-exit")"
fi
SH
    chmod 700 "$menu_box/doubles/provider" || fail 'providers: cannot make dispatcher executable'
    for name in tailscale omarchy-tailscale-send dropbox-cli wl-copy; do
        menus_guard "$menu_box/doubles/$name"
        cp "$menu_box/doubles/provider" "$menu_box/doubles/$name" || fail "providers: double copy failed: $name"
        menus_guard "$menu_box/absent/$name"
        ln -s "$menu_box/doubles/$name" "$menu_box/absent/$name" || fail "providers: fixture link failed: $name"
    done
    providers_install wl-copy yes
    providers_ready
}

providers_cleanup() {
    providers_release tailscale || return 1
    providers_release dropbox-cli || return 1
    kill_flea || return 1
    exec {taildrop_fd}>&-
    exec {dropbox_fd}>&-
}

providers_selection() {
    local path="$1" marked cursor
    menus_visit "$path" 2
    menus_expect listInFlight '. == false' 'provider listing settles before selection'
    marked=$(row_index_of a-marked.txt)
    cursor=$(row_index_of b-cursor.txt)
    click_row "$marked" left
    # The mark has to be observed BEFORE the cursor moves. Without this the case pressed
    # Down into whatever state it found, and a re-list landing between the two clears the
    # selection and resets the cursor to row 0; Down then lands on row 1, which is
    # b-cursor.txt by arithmetic, so the cursor half of the assertion below passed and only
    # the selection half failed. That reported a selection defect that was really a listing
    # the case had never waited for.
    providers_expect ".selected == [$marked] and .refreshing == false" \
        'the click marked the row, and nothing is refreshing, before the cursor moves'
    key -k Down >/dev/null || fail 'providers: cursor movement failed'
    providers_expect ".cursor == $cursor and .selected == [$marked] and .cursorPath == \"$path/b-cursor.txt\" and .selectedPaths == [\"$path/a-marked.txt\"]" 'cursor remains outside the marked selection'
    key -k Menu >/dev/null || fail 'providers: native Menu delivery failed'
    menus_expect menuState '.opened and .snapshotReady' 'native Menu captures distinct cursor and marks'
    providers_expect '(.refreshing | not) and .menuFocus' 'provider selection refresh settles'
}

providers_choose() {
    local action="$1"
    providers_seek "$action"
    if [[ "$action" == taildrop ]]; then
        key -k Right >/dev/null || fail 'providers: submenu key failed'
        menus_expect menuState '.submenu and .submenuEntries[.submenuCursor].id == "fixture-peer"' 'Taildrop submenu retains the selected peer identity'
    fi
    key -k Return >/dev/null || fail 'providers: activation key failed'
}

providers_sharelink_checks() {
    local scenario reason before clipboard_calls share_before share_requests warning
    for scenario in share-failed share-invalid share-socket-timeout share-missing clipboard-failed clipboard-missing; do
        providers_ready
        providers_selection "$menu_box/Dropbox"
        before=$(providers_calls wl-copy)
        clipboard_calls=0
        warning=""
        case "$scenario" in
            share-failed)
                providers_mode sharelink ready '' 'fixture share-link refusal' 7
                reason='Dropbox could not make a share link for that file.' ;;
            share-invalid)
                providers_mode sharelink ready 'http is not a share link'
                reason='Dropbox could not make a share link for that file.' ;;
            share-socket-timeout)
                providers_mode sharelink socket-timeout ''
                reason='Dropbox could not make a share link for that file.' ;;
            share-missing)
                providers_mode dropbox-cli vanish 'Up to date'
                warning="Process failed to start, likely because the binary could not be found. Command: QList(\"dropbox-cli\", \"sharelink\", \"$menu_box/Dropbox/b-cursor.txt\")"
                reason='The Dropbox share link helper could not start.' ;;
            clipboard-failed)
                providers_mode wl-copy ready '' 'fixture clipboard refusal' 7
                reason='The share link could not be copied to the clipboard.'
                clipboard_calls=1 ;;
            clipboard-missing)
                providers_install wl-copy no
                warning='Process failed to start, likely because the binary could not be found. Command: QList("wl-copy", "https://fixture.invalid/share")'
                reason='The clipboard helper could not start; the share link was not copied.' ;;
        esac
        providers_choose sharelink
        menus_error "$reason" "$scenario reports its own plain failure"
        menus_expect statusActivityState '.errors == 1' "$scenario records one persistent error"
        menus_equal "$scenario clipboard dispatch count" "$((before + clipboard_calls))" "$(providers_calls wl-copy)"
        providers_expect '.listFocus and (.pendingActivation | not)' "$scenario returns menu ownership to the listing"
        if [[ -n "$warning" ]]; then printf '%s\n' "$warning" >> "$expected_warnings"; fi
        menus_shot "providers-$scenario"
        providers_install dropbox-cli yes
        providers_install wl-copy yes
        menus_acknowledge
    done

    providers_ready
    providers_selection "$menu_box/Dropbox"
    providers_mode sharelink gate 'https://fixture.invalid/share'
    providers_mode wl-copy gate ''
    share_before=$(providers_calls dropbox-cli)
    before=$(providers_calls wl-copy)
    providers_choose sharelink
    providers_call dropbox-cli "$(jq -cn --arg path "$menu_box/Dropbox/b-cursor.txt" '["sharelink",$path]')" "$((share_before + 1))"
    menus_expect statusActivityState '.errors == 0 and .notice != "Share link copied to the clipboard."' 'a pending Dropbox reply cannot claim clipboard success'
    key -k Up >/dev/null || fail 'providers: pending-share cursor movement failed'
    providers_release dropbox-cli
    providers_call wl-copy '["https://fixture.invalid/share"]' "$before"
    menus_expect statusActivityState '.errors == 0 and .notice != "Share link copied to the clipboard."' 'starting wl-copy cannot claim clipboard success'
    share_requests=$(jq -s '[.[] | select(.helper == "dropbox-cli" and .args[0] == "sharelink")] | length' "$menu_box/calls.jsonl")
    providers_open a-marked.txt
    providers_choose sharelink
    menus_error 'A share link is still being copied; try again when it finishes.' 'another file cannot replace the pending clipboard request'
    menus_equal 'busy refusal starts no second share-link request' "$share_requests" \
        "$(jq -s '[.[] | select(.helper == "dropbox-cli" and .args[0] == "sharelink")] | length' "$menu_box/calls.jsonl")"
    menus_equal 'busy refusal starts no second clipboard request' "$((before + 1))" "$(providers_calls wl-copy)"
    menus_acknowledge
    providers_release dropbox-cli
    menus_message 'Share link copied to the clipboard.' 'only successful clipboard completion acknowledges the retained share link'
    providers_ready
}

providers_dropbox_move_checks() {
    providers_selection "$menu_dir"
    providers_choose dropbox
    menus_error 'Move failed: a-marked.txt' 'Move to Dropbox reports the real destination collision'
    menus_expect statusActivityState '(.activities | length) == 0 and .errors == 1' 'failed Dropbox move finishes without hiding its error'
    menus_expect statusFooterState '.secondary.text == " · esc dismisses"' 'unacknowledged Dropbox error keeps the informational error specimen'
    menus_equal 'collision keeps the marked source bytes' 'list/a-marked.txt original' "$(cat "$menu_dir/a-marked.txt")"
    menus_equal 'collision keeps the existing destination bytes' 'Dropbox/a-marked.txt original' "$(cat "$menu_box/Dropbox/a-marked.txt")"
    menus_equal 'Dropbox retry selects only the failed marked file' "$(row_index_of a-marked.txt)" "$(ipc selectedIndices)"
    menus_shot providers-dropbox-collision

    menus_guard "$menu_box/Dropbox/a-marked.txt"
    menus_guard "$menu_box/retired/dropbox-collision.txt"
    mv -- "$menu_box/Dropbox/a-marked.txt" "$menu_box/retired/dropbox-collision.txt" || fail 'providers: cannot preserve the collision before retry'
    menus_acknowledge
    menus_expect statusFooterState '.secondary.text | contains("a-marked.txt selected for retry")' 'acknowledged Dropbox failure names the identity-checked source for retry'
    key -k Menu >/dev/null || fail 'providers: retained-selection retry menu failed'
    menus_expect menuState '.opened and .snapshotReady' 'native retry captures the retained original selection'
    providers_expect '(.refreshing | not) and .menuFocus' 'Dropbox retry refresh settles'
    providers_choose dropbox
    menus_message 'Moved 1 item' 'Move to Dropbox retries the retained original through the real transfer'
    wait_listing 1
    [[ ! -e "$menu_dir/a-marked.txt" ]] || fail 'providers: successful move retained its source'
    menus_equal 'successful Dropbox move commits the original bytes' 'list/a-marked.txt original' "$(cat "$menu_box/Dropbox/a-marked.txt")"
    menus_equal 'successful Dropbox move leaves the unmarked cursor file alone' replacement "$(cat "$menu_dir/b-cursor.txt")"
    menus_expect statusActivityState '.undoAvailable and .errors == 0 and (.activities | length) == 0' 'successful Dropbox move is undoable'
    menus_shot providers-dropbox-moved

    menus_guard "$menu_dir/a-marked.txt"
    menus_guard "$menu_box/Dropbox/a-marked.txt"
    key z >/dev/null || fail 'providers: native Dropbox Undo failed'
    menus_message 'Undid the move.' 'native Undo restores the same Dropbox source'
    wait_listing 2
    [[ ! -e "$menu_box/Dropbox/a-marked.txt" ]] || fail 'providers: Undo retained its moved destination'
    menus_equal 'Dropbox Undo restores original source bytes' 'list/a-marked.txt original' "$(cat "$menu_dir/a-marked.txt")"
    menus_equal 'Dropbox Undo preserves the pre-existing collision' 'Dropbox/a-marked.txt original' "$(cat "$menu_box/retired/dropbox-collision.txt")"
    menus_shot providers-dropbox-undone
}

case_providers() (
    local menu_box="$fixture_root/providers" menu_dir="$fixture_root/providers/list" menus_checks=0
    local taildrop_fd dropbox_fd name before action record reason path saved
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
    menus_expect menuState 'all(.entries[]; .action != "taildrop" and .action != "dropbox" and .action != "sharelink")' 'absent providers are not built'
    menus_equal 'absent providers spawn no helper' 0 "$(jq -s length "$menu_box/calls.jsonl")"
    providers_close

    for name in tailscale dropbox-cli; do
        providers_install "$name" yes
        menus_guard "$menu_box/doubles/$name"
        chmod 600 "$menu_box/doubles/$name" || fail 'providers: cannot make installed helper unusable'
    done
    providers_install omarchy-tailscale-send yes
    providers_open
    providers_disabled taildrop 'not executable'
    providers_disabled dropbox 'not executable'
    menus_equal 'unusable providers spawn no helper' 0 "$(jq -s length "$menu_box/calls.jsonl")"
    menus_shot providers-unusable
    providers_close
    for name in tailscale dropbox-cli; do menus_guard "$menu_box/doubles/$name"; chmod 700 "$menu_box/doubles/$name" || fail 'providers: executable recovery failed'; done

    while IFS='|' read -r record reason; do
        providers_mode tailscale ready "$record"
        providers_open
        providers_disabled taildrop "$reason"
        providers_close
    done <<'STATES'
{"BackendState":"NeedsLogin"}|signed out
{"BackendState":"Stopped"}|stopped
{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{}}|no peers
{"BackendState":"Running","Self":{},"Peer":{}}|disabled for this account
malformed|invalid status
STATES
    providers_mode tailscale ready '' 'fixture Tailscale failure' 7
    providers_open
    providers_disabled taildrop 'fixture Tailscale failure'
    providers_close
    providers_mode tailscale gate "$provider_ready"
    providers_open
    providers_disabled taildrop 'timed out'
    menus_shot providers-timeout
    providers_close
    providers_ready
    providers_install omarchy-tailscale-send no
    providers_open
    providers_disabled taildrop 'not installed'
    providers_close
    providers_install omarchy-tailscale-send yes

    for record in '{}' malformed '{"personal":{"path":"relative"}}'; do
        providers_write home/.dropbox/info.json "$record"
        providers_open
        if [[ "$record" == '{}' ]]; then reason='signed out'; else reason='invalid'; fi
        providers_disabled dropbox "$reason"
        providers_close
    done
    providers_ready
    for record in '' "Dropbox isn't running!" "Dropbox isn't responding!" 'Dropbox daemon stopped.' "Couldn't get status: fixture refusal"; do
        providers_mode dropbox-cli ready "$record"
        providers_open
        reason="$record"
        [[ -n "$reason" ]] || reason='empty status'
        providers_disabled dropbox "$reason"
        providers_close
    done
    providers_mode dropbox-cli ready '' 'fixture Dropbox failure' 7
    providers_open
    providers_disabled dropbox 'fixture Dropbox failure'
    providers_close
    providers_mode dropbox-cli gate 'Up to date'
    providers_open
    providers_disabled dropbox 'timed out'
    providers_close
    providers_ready

    providers_install tailscale no
    providers_open
    menus_expect menuState 'all(.entries[]; .action != "taildrop")' 'fresh menu removes uninstalled Tailscale'
    providers_close
    providers_install tailscale yes
    providers_mode tailscale gate "$provider_ready"
    menus_file_menu b-cursor.txt key
    providers_expect '.refreshing and .taildrop.checking' 'late installed provider refresh is in flight'
    providers_seek properties
    before=$(ipc listContentY)
    providers_release tailscale
    providers_expect '(.refreshing | not) and .menuFocus' 'late provider insertion finishes with keyboard focus'
    menus_expect menuState '.entries[.cursor].action == "properties" and any(.entries[]; .action == "taildrop" and (.disabled | not))' 'inserted provider preserves current action'
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
        menus_error 'Selected item changed' "$action refuses an asynchronously replaced unmarked cursor"
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
    menus_error 'Dropbox account folder changed' 'Move to Dropbox refuses a replaced account directory'
    providers_expect '(.refreshing | not) and (.pendingActivation | not)' 'destination refusal clears pending activation'
    menus_equal 'refused destination remains empty' 0 "$(find "$menu_box/Dropbox" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
    menus_equal 'destination refusal preserves marked source' 'list/a-marked.txt original' "$(cat "$menu_dir/a-marked.txt")"
    menus_equal 'destination refusal preserves cursor replacement' replacement "$(cat "$menu_dir/b-cursor.txt")"
    menus_shot providers-destination-refusal
    printf 'PROVIDERS_NATIVE_CHECKS=%s\n' "$menus_checks"
    printf 'PROVIDERS_UNVERIFIED worker-stage Dropbox retry, offline daemon wording, helper launch race, concurrent panes, all-preset provider combinations, matched-size pixels\n'
)

# Opt-in: real provider discovery and menu cancellation, with only Flea's own persistence redirected.
case_providersinstalled() (
    local menu_box="$fixture_root/providersinstalled" menu_dir="$fixture_root/providersinstalled/list" menus_checks=0
    local name target state peer_count clipboard_before
    sandbox_scratch "$menu_box"
    : > "$menu_box/.flea-test-sandbox" || fail 'providersinstalled: sandbox marker write failed'
    for target in list state config cache data; do
        menus_guard "$menu_box/$target"
        mkdir "$menu_box/$target" || fail 'providersinstalled: private directory creation failed'
    done
    providers_write list/provider.txt 'provider observation only'
    for name in tailscale omarchy-tailscale-send dropbox-cli; do
        target=$(command -v "$name") || fail "providersinstalled: $name is not installed"
        [[ "$target" == /* && -x "$target" ]] || fail "providersinstalled: $name is not an absolute executable"
        printf 'PROVIDERS_INSTALLED_COMMAND %s %s\n' "$name" "$target"
    done
    export XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config"
    export XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    "$flea_bin" --ui-state '{"view":"list","keys":"default"}' >/dev/null \
        || fail 'providersinstalled: private settings seed failed'
    trap 'kill_flea || exit 1' EXIT
    launch "$menu_dir"
    wait_listing 1
    clipboard_before=$(ipc keyDeliveryState | jq -c .clipboard) || fail 'providersinstalled: internal clipboard observer failed'
    providers_open provider.txt pointer
    providers_expect '.facts.taildrop.installed and .facts.taildropSend.installed and .facts.dropbox.installed and .dropbox.ready' 'native menu resolves the actual installed providers and ready Dropbox account'
    menus_expect menuState 'any(.entries[]; .action == "dropbox" and (.disabled | not) and .mark == "dropbox")' 'real Dropbox account exposes its official menu mark and enabled move'
    menus_expect menuState 'any(.entries[]; .action == "taildrop" and .mark == "tailscale")' 'real Tailscale installation exposes its official menu mark'
    providers_geometry
    menus_shot providers-installed-menu
    state=$(ipc providerState) || fail 'providersinstalled: actual peer observer failed'
    peer_count=$(jq -er '.taildrop.peers | length' <<< "$state") || fail 'providersinstalled: actual peers invalid'
    if (( peer_count > 0 )); then
        providers_seek taildrop
        key -k Right >/dev/null || fail 'providersinstalled: real-peer flyout failed'
        menus_expect menuState ".submenu and (.submenuEntries | length) == $peer_count and .submenuCursor == 0" 'real eligible peer identities populate the native flyout'
        menus_equal 'flyout keeps the actual provider peer identities' \
            "$(jq -c '[.taildrop.peers[].id]' <<< "$state")" "$(ipc menuState | jq -c '[.submenuEntries[].id]')"
        menus_shot providers-installed-flyout
        key -k Escape >/dev/null || fail 'providersinstalled: peer cancellation failed'
        menus_expect menuState '.opened and (.submenu | not)' 'Escape cancels the real peer flyout without selecting a receiver'
    else
        menus_expect menuState 'any(.entries[]; .action == "taildrop" and .disabled and (.hint | length) > 0)' 'no eligible real peer leaves a visible reason'
        printf 'PROVIDERS_INSTALLED_UNVERIFIED no currently eligible real peer; populated flyout not exercised\n'
    fi
    providers_close
    providers_expect '(.pendingActivation | not) and (.refreshing | not) and .listFocus' 'provider observation leaves no queued activation and restores listing focus'
    menus_expect statusActivityState '.errors == 0 and (.activities | length) == 0 and (.notice | test("Sending|Moved|Share link") | not)' 'cancelled provider menus produce no operation or dispatch confirmation'
    menus_equal 'cancelled provider menus preserve the internal clipboard' "$clipboard_before" "$(ipc keyDeliveryState | jq -c .clipboard)"
    menus_equal 'cancelled provider menus preserve fixture bytes' 'provider observation only' "$(cat "$menu_dir/provider.txt")"
    printf 'PROVIDERS_INSTALLED checks=%s eligible_peers=%s input=menu,flyout,Escape no_send_move_sharelink_activation=true visual_inspection=pending\n' "$menus_checks" "$peer_count"
)
