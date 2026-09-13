#!/usr/bin/env bash
# Source after tests/ui.sh helpers; case_trash owns a private D-Bus session and marked Trash fixture.
# All native actions use the candidate and read-only IPC; helper assertions do not approve pixels.

case_railorder() {
    local dir="$fixture_root/railorder" state="$fixture_root/railorder-state" config="$fixture_root/railorder-config"
    local entries trash_index home_index favourite_index cx trash_y home_y favourite_y end seed
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    sandbox_scratch "$config"
    export XDG_CONFIG_HOME="$config"
    seed=$(jq -cn --arg path "$dir" '{view:"list",places:{favourites:[{label:"Rail favorite",path:$path}],showTrash:true,showNetwork:true,showDevices:true}}')
    seed_ui_state "$state" "$seed"
    launch "$dir"
    wait_listing 0
    end=$((SECONDS + 20))
    while (( SECONDS < end )); do
        entries=$(ipc railEntries) || fail "railorder: rail entries unavailable"
        jq -e 'any(.[]; .group == "home") and any(.[]; .group == "trash")' <<< "$entries" >/dev/null && break
        sleep 0.05
    done
    jq -e '
        (map(.group) | index("trash")) as $trash |
        $trash != null and $trash > 0 and .[0].label == "Home" and
        ([.[] | select(.group == "trash")] | length) == 1 and
        .[$trash].path == "trash:///" and .[$trash - 1].group == "home" and
        all(.[:$trash][]; .group == "home") and .[$trash + 1].group == "favourite" and
        all(.[$trash + 2:][]; .group == "network" or .group == "device")
    ' <<< "$entries" >/dev/null || fail "railorder: expected Places, Favorites, Network, Devices: $entries"
    trash_index=$(jq -r 'map(.group) | index("trash")' <<< "$entries")
    home_index=$((trash_index - 1))
    favourite_index=$((trash_index + 1))
    read -r cx trash_y <<< "$(ipc railRowCentre "$trash_index")"
    read -r cx home_y <<< "$(ipc railRowCentre "$home_index")"
    read -r cx favourite_y <<< "$(ipc railRowCentre "$favourite_index")"
    [[ "$trash_y" =~ ^[0-9]+$ && "$home_y" =~ ^[0-9]+$ && "$trash_y" -gt "$home_y" ]] \
        || fail "railorder: live Trash row does not follow the Home/XDG rows"
    [[ "$favourite_y" =~ ^[0-9]+$ && "$favourite_y" -gt "$trash_y" ]] || fail "railorder: Favorites does not follow Trash"
    ipc railDetails | jq -e '[.headers[] | select(.visible) | .text] ==
        (["PLACES", "FAVORITES"] + (if any(.rows[]; .group == "network") then ["NETWORK"] else [] end)
        + (if any(.rows[]; .group == "device") then ["DEVICES"] else [] end))' >/dev/null \
        || fail "railorder: native section labels/order differ from the ruled rail"
    trash_shot trash-rail-order
    printf 'TRASH_RAIL_ORDER entries=%s home_y=%s trash_y=%s\n' "$entries" "$home_y" "$trash_y"
    kill_flea
}

trash_guard() {
    local path="$1" canonical
    [[ -n "$path" && "$path" == /* ]] || fail "trash: empty or relative mutation path"
    [[ -f "$trash_box/.flea-test-sandbox" ]] || fail "trash: missing owned fixture marker"
    canonical=$(realpath -m -- "$path") || fail "trash: could not resolve mutation path"
    [[ "$canonical" == "$trash_box/"* && "$canonical" != "$trash_box" ]] \
        || fail "trash: mutation path is outside this case's sandbox"
}

trash_bus_id() {
    gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.GetId \
        | python3 -c 'import ast,sys; print(ast.literal_eval(sys.stdin.read())[0])'
}

trash_owned_pid() {
    local pid="$1"
    [[ "$pid" =~ ^[0-9]+$ ]] || return 1
    trash_private_pids "$pid" | grep -Fx "$pid" >/dev/null
}

trash_private_pids() {
    python3 - "$trash_bus_address" "$XDG_DATA_HOME" "${1:-}" <<'PY'
import os, pathlib, sys
address, data = sys.argv[1].encode(), sys.argv[2].encode()
processes = [pathlib.Path("/proc", sys.argv[3])] if sys.argv[3] else pathlib.Path("/proc").iterdir()
for process in processes:
    if not process.name.isdigit() or int(process.name) in (os.getpid(), os.getppid()):
        continue
    try:
        values = dict(entry.split(b"=", 1) for entry in (process / "environ").read_bytes().split(b"\0") if b"=" in entry)
        # D-Bus activation may append the bus GUID to the same owned Unix socket address.
        bus, separator, guid = values.get(b"DBUS_SESSION_BUS_ADDRESS", b"").partition(b",guid=")
        same_bus = bus == address and (not separator or len(guid) == 32 and all(byte in b"0123456789abcdef" for byte in guid))
        if process.stat().st_uid == os.getuid() and same_bus and values.get(b"XDG_DATA_HOME") == data:
            print(process.name)
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        continue
PY
}

trash_cleanup() {
    local result="$1" pid end
    trap - EXIT HUP INT TERM
    (kill_flea) || result=1
    for pid in $(trash_private_pids); do
        trash_owned_pid "$pid" && kill -TERM "$pid" 2>/dev/null
    done
    end=$((SECONDS + 10))
    while (( SECONDS < end )); do
        [[ -z "$(trash_private_pids)" ]] && break
        sleep 0.05
    done
    for pid in $(trash_private_pids); do
        if trash_owned_pid "$pid"; then
            printf 'FAIL: trash: private process %s did not terminate\n' "$pid" >&2
            kill -KILL "$pid" 2>/dev/null
            result=1
        fi
    done
    [[ -z "$trash_bus_pid" ]] || wait "$trash_bus_pid" 2>/dev/null || true
    if [[ "$result" -ne 0 ]]; then
        trash_guard "$trash_box/payload"
        find "$trash_box" -type d -exec chmod u+rwx -- {} + || result=1
    fi
    printf 'TRASH_NATIVE checks=%s teardown_status=%s; screenshots require separate inspection.\n' "$trash_checks" "$result"
    exit "$result"
}

trash_start_bus() {
    local end pid providers
    trash_parent_bus_id=$(trash_bus_id) || fail "trash: parent session identity unavailable"
    trash_bus_address="unix:path=$trash_box/bus"
    export DBUS_SESSION_BUS_ADDRESS="$trash_bus_address"
    dbus-daemon --session --nofork --address="$trash_bus_address" > "$trash_box/dbus.log" 2>&1 &
    trash_bus_pid=$!
    trap 'trash_cleanup $?' EXIT
    trap 'exit 130' INT
    trap 'exit 143' HUP TERM
    end=$((SECONDS + 10))
    while (( SECONDS < end )); do
        if trash_private_bus_id=$(trash_bus_id 2>/dev/null); then break; fi
        kill -0 "$trash_bus_pid" 2>/dev/null || fail "trash: private dbus-daemon exited"
        sleep 0.05
    done
    [[ -n "$trash_private_bus_id" && "$trash_private_bus_id" != "$trash_parent_bus_id" ]] \
        || fail "trash: private session did not acquire a distinct identity"
    /usr/bin/gio list trash:/// > "$trash_box/initial-list.log" \
        || fail "trash: private GIO Trash mount failed"
    providers=0
    for pid in $(trash_private_pids); do
        if [[ "$(cat "/proc/$pid/comm")" == gvfsd-trash ]]; then
            trash_provider_pid="$pid"
            providers=$((providers + 1))
        fi
    done
    [[ "$providers" == 1 ]] || fail "trash: expected one private provider, found $providers"
    printf 'TRASH_SESSION parent=%s private=%s daemon=%s provider=%s root=%s\n' \
        "$trash_parent_bus_id" "$trash_private_bus_id" "$trash_bus_pid" "$trash_provider_pid" "$trash_box"
}

trash_private() {
    local actual pid="$trash_provider_pid" candidate
    [[ -n "$trash_private_bus_id" && -n "$trash_parent_bus_id" ]] \
        || fail "trash: private and parent D-Bus identities are required"
    actual=$(trash_bus_id) || fail "trash: cannot read the current D-Bus identity"
    [[ "$actual" == "$trash_private_bus_id" && "$actual" != "$trash_parent_bus_id" ]] \
        || fail "trash: this is not the owned private D-Bus session"
    [[ "$pid" =~ ^[0-9]+$ && -r "/proc/$pid/environ" ]] \
        || fail "trash: no attributable private gvfsd-trash process"
    [[ "$(cat "/proc/$pid/comm")" == gvfsd-trash ]] || fail "trash: wrong provider process"
    trash_owned_pid "$pid" || fail "trash: provider belongs to another bus, data root, or user"
    candidate=$(flea_pid)
    trash_owned_pid "$candidate" || fail "trash: candidate belongs to another bus, data root, or user"
}

trash_backing() {
    local uri="$1" info target
    info=$(/usr/bin/gio info --nofollow-symlinks --attributes=standard::target-uri "$uri") \
        || fail "trash: provider did not resolve a backing path"
    # Sample GIO line: "  standard::target-uri: file:///owned/fixture/data/Trash/files/a.txt".
    target=$(sed -n 's/^  standard::target-uri: //p' <<< "$info")
    python3 - "$target" <<'PY'
import sys
from urllib.parse import unquote, urlsplit
uri = urlsplit(sys.argv[1])
if uri.scheme != "file" or uri.netloc or uri.query or uri.fragment:
    raise SystemExit("REFUSED unsupported Trash backing URI")
print(unquote(uri.path, errors="strict"))
PY
}

trash_guard_store() {
    local expected="$1" listing uri original backing count=0
    trash_private
    trash_guard "$XDG_DATA_HOME"
    listing=$(/usr/bin/gio trash --list) || fail "trash: provider listing failed"
    if [[ -n "$listing" ]]; then
        # Sample list row: "trash:///a.txt<TAB>/owned/fixture/payload/a.txt".
        while IFS=$'\t' read -r uri original; do
            [[ "$uri" == trash:///* && -n "$original" ]] || fail "trash: malformed provider row"
            trash_guard "$original"
            backing=$(trash_backing "$uri") || fail "trash: unavailable backing path"
            trash_guard "$backing"
            [[ "$backing" == "$XDG_DATA_HOME/Trash/files/"* ]] \
                || fail "trash: provider backing is outside the private Trash store"
            count=$((count + 1))
        done <<< "$listing"
    fi
    [[ "$count" -eq "$expected" ]] || fail "trash: expected $expected private entries, found $count"
    printf 'TRASH_GUARD count=%s backing=%s/Trash/files bus=%s\n' "$count" "$XDG_DATA_HOME" "$trash_private_bus_id"
}

trash_wait() {
    local condition="$1" label="${2:-$1}" end=$((SECONDS + 20)) state
    while (( SECONDS < end )); do
        state=$(ipc trashState) || fail "trash: read-only state unavailable"
        if jq -e "$condition" <<< "$state" >/dev/null; then
            trash_checks=$((trash_checks + 1))
            printf 'TRASH_PASS %s expected=%q observed=%s\n' "$label" "$condition" "$state"
            return
        fi
        sleep 0.05
    done
    fail "trash: native state did not reach $condition; last state: $state"
}

trash_rail() {
    local index
    index=$(ipc railEntries | jq -er 'map(.label) | index("Trash")') \
        || fail "trash: no native Trash rail row"
    click_rail_row "$index" "${1:-left}"
}

trash_click() {
    local reader="$1" argument="${2:-}" button="${3:-left}" cx cy wx wy
    local -a modifiers=()
    if (( $# > 3 )); then modifiers=("${@:4}"); fi
    if [[ -n "$argument" ]]; then read -r cx cy <<< "$(ipc "$reader" "$argument")"
    else read -r cx cy <<< "$(ipc "$reader")"; fi
    [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "trash: missing native control centre"
    read -r wx wy _width _height < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" "$button" "${modifiers[@]}" >/dev/null \
        || fail "trash: native pointer activation failed"
}

trash_shot() {
    local name="$1" path="$evidence_dir/${trash_case_label:+$trash_case_label-}$1.png" canonical
    [[ -f "$run_root/.flea-test-sandbox" ]] || fail "trash: native evidence root is not marked"
    canonical=$(realpath -m -- "$path") || fail "trash: evidence path did not resolve"
    [[ "$canonical" == "$run_root/"* && "$canonical" != "$run_root" ]] || fail "trash: evidence escaped its sandbox"
    mkdir -p "$evidence_dir" || fail "trash: evidence directory creation failed"
    [[ ! -e "$path" ]] || fail "trash: refusing to reuse an old screenshot"
    omarchy-drive shot "$path" flea >/dev/null || fail "trash: screenshot failed"
    [[ -s "$path" ]] || fail "trash: screenshot is empty"
    printf 'TRASH_SHOT path=%s viewport=%q\n' "$path" "$(window_box)"
}

trash_empty_strip() {
    trash_rail right
    menu_seek "Empty Trash"
    key -k Return >/dev/null
    trash_wait '.confirmation.opened and (.confirmation.destructiveFocus == false)'
}

trash_empty_confirmed() {
    local count="$1"
    trash_empty_strip
    key l >/dev/null
    trash_wait ".confirmation.destructiveFocus and .confirmation.all and .confirmation.count == $count"
    trash_guard_store "$count"
    key -k Return >/dev/null
    trash_wait '.opened and .total == 0 and .count == 0 and (.busy == false)'
    trash_guard_store 0
}

trash_move() {
    local name="$1" previous_count="$2" remaining="$3" row selected
    trash_guard "$payload/$name"
    wait_path "$payload"
    trash_wait '(.opened == false)' "Move to Trash starts in fixture listing"
    row=$(row_index_of "$name")
    click_row "$row" left
    selected=$(ipc selectedIndices)
    [[ "$(ipc path)" == "$payload" && "$(ipc focusView)" == list && "$(ipc cursor)" == "$row" \
        && ( -z "$selected" || "$selected" == "$row" ) && "$(ipc contextMenuVisible)" == false ]] \
        || fail "trash: refusing Delete outside the intended fixture row"
    trash_guard_store "$previous_count"
    key -k Delete >/dev/null
    wait_listing "$remaining"
    trash_wait "(.opened == false) and .count == $((previous_count + 1))" "Move to Trash updates private count"
    trash_guard_store "$((previous_count + 1))"
}

case_trashsweep() { case_trash sweep; }
case_trashbasic() { case_trash basic; }
case_raildetails() { case_trash raildetails; }
case_trashcontrols() { case_trash controls; }
case_trashkeys() { case_trash keys; }
case_trashrestore() { case_trash restore; }
case_trashstale() { case_trash stale; }
case_trashfailure() { case_trash failure; }

trash_key_alternatives() {
    local preset="$1" binding bindings
    key m >/dev/null
    [[ "$(ipc contextMenuVisible)" == true ]] || fail "trash: $preset m did not open the selected menu"
    menu_seek "Delete permanently"
    trash_guard_store 1
    key -k Return >/dev/null
    trash_wait '.opened and .confirmation.opened and (.confirmation.all == false) and .confirmation.count == 1 and (.confirmation.destructiveFocus == false)' "$preset m then Enter opens selected confirmation"
    key -k Escape >/dev/null
    trash_wait '(.confirmation.opened == false) and .total == 1 and (.busy == false)'
    case "$preset" in
        default) bindings='dd' ;;
        vim) bindings=D ;;
        mac) bindings='dd ctrl-delete shift-delete' ;;
        windows) bindings='dd ctrl-d shift-delete' ;;
    esac
    for binding in $bindings; do
        trash_wait '.opened and .selectedCount == 1 and (.confirmation.opened == false) and (.busy == false)'
        trash_guard_store 1
        case "$binding" in
            dd|D) key "$binding" >/dev/null ;;
            ctrl-delete) key -M ctrl -k Delete -m ctrl >/dev/null ;;
            ctrl-d) key -M ctrl -k d -m ctrl >/dev/null ;;
            shift-delete) key -M shift -k Delete -m shift >/dev/null ;;
        esac
        trash_wait '.opened and .confirmation.opened and (.confirmation.all == false) and .confirmation.count == 1 and (.confirmation.destructiveFocus == false)' "$preset $binding requires selected confirmation"
        key -k Return >/dev/null
        trash_wait '(.confirmation.opened == false) and .total == 1 and (.busy == false)' "$preset $binding reflexive Return cancels"
        trash_guard_store 1
    done
}

trash_confirmation_controls() {
    local mode="${1:-controls}" preset move token
    for preset in default vim mac windows; do
        kill_flea
        trash_guard "$XDG_STATE_HOME"
        "$flea_bin" --ui-state "{\"keys\":\"$preset\"}" >/dev/null \
            || fail "trash: could not persist the $preset preset"
        launch "$payload"
        wait_listing 1
        [[ "$(ipc keymapPreset)" == "$preset" ]] || fail "trash: native preset differs from $preset"
        trash_guard_store 1
        trash_wait '(.opened == false) and .count == 1 and .rail.countText == "" and (.rail.current == false)' "$preset count defaults hidden outside Trash"
        trash_rail
        trash_wait '.opened and .total == 1 and (.busy == false) and .rail.current and .rail.countText == "" and .headerLabels == ["Name", "Original location", "Deleted"] and (.upEnabled == false)' "$preset dedicated view controls"
        trash_click trashControlCentre up
        trash_wait '.opened and .total == 1 and (.busy == false)' "$preset Up is inert"
        trash_click trashControlCentre back
        trash_wait '(.opened == false) and (.rail.current == false)' "$preset pointer Back"
        wait_path "$payload"
        for move in Backspace Escape; do
            trash_rail
            trash_wait '.opened and .total == 1 and (.busy == false)'
            key -k "$move" >/dev/null
            trash_wait '(.opened == false) and (.rail.current == false)' "$preset $move returns to listing"
            wait_path "$payload"
        done
        trash_rail
        trash_wait '.opened and .total == 1 and (.busy == false)'
        trash_click trashRowCentre 0 left
        trash_wait '.selectedCount == 1 and .rows[0].selected' "$preset pointer selection"
        trash_click trashRowCentre 0 left --mods ctrl
        trash_wait '.selectedCount == 0 and (.rows[0].selected == false)' "$preset Ctrl-click deselects"
        key -M ctrl -k a -m ctrl >/dev/null || fail "trash: Ctrl+A delivery failed"
        trash_wait '.selectedCount == 1 and .rows[0].selected' "$preset Ctrl+A selects snapshot"
        if [[ "$mode" != controls ]]; then trash_key_alternatives "$preset"; fi
        if [[ "$mode" == keys ]]; then continue; fi
        for move in Menu F10; do
            if [[ "$move" == F10 ]]; then key -M shift -k F10 -m shift >/dev/null
            else key -k Menu >/dev/null; fi
            ipc contextMenuModel | jq -e '[.[] | select(.separator != true) | .action] == ["restoreTrashSelection", "deletePermanently"]' >/dev/null \
                || fail "trash: $preset $move did not open the selected Trash menu"
            key -k Escape >/dev/null
            trash_wait '.opened and .selectedCount == 1' "$preset context dismissal preserves selection"
        done
        key -k Delete >/dev/null
        trash_wait '.confirmation.opened and (.confirmation.all == false) and .confirmation.count == 1 and (.confirmation.destructiveFocus == false)' "$preset Delete confirms selected identity"
        token=$(ipc trashState | jq -er '.confirmation.token')
        key -k Delete >/dev/null
        trash_wait ".confirmation.opened and .confirmation.token == $token and (.confirmation.destructiveFocus == false)" "$preset repeated Delete does not confirm"
        key -k Return >/dev/null
        trash_wait '(.confirmation.opened == false) and .total == 1 and (.busy == false)' "$preset reflexive Return cancels selected delete"

        trash_empty_strip
        trash_wait '.confirmation.all and .confirmation.title == "Empty Trash?" and .confirmation.cancel.enabled and .confirmation.danger.enabled' "$preset Empty Trash uses shared strip"
        token=$(ipc trashState | jq -er '.confirmation.token')
        key -k Tab >/dev/null
        trash_wait '.confirmation.destructiveFocus' "$preset Tab reaches danger"
        key -M shift -k Tab -m shift >/dev/null
        trash_wait '(.confirmation.destructiveFocus == false)' "$preset Backtab reaches Cancel"
        key l >/dev/null
        trash_wait '.confirmation.destructiveFocus' "$preset l reaches danger"
        key h >/dev/null
        trash_wait '(.confirmation.destructiveFocus == false)' "$preset h reaches Cancel"
        key -k Right >/dev/null
        trash_wait '.confirmation.destructiveFocus' "$preset Right reaches danger"
        key -k Left >/dev/null
        trash_wait '(.confirmation.destructiveFocus == false)' "$preset Left reaches Cancel"
        key -M shift -k Tab -m shift >/dev/null
        trash_wait '.confirmation.destructiveFocus' "$preset Backtab wraps to danger"
        key -k Tab >/dev/null
        trash_wait '(.confirmation.destructiveFocus == false)' "$preset Tab wraps to Cancel"
        key -M ctrl -k a -m ctrl >/dev/null || fail "trash: modified confirmation input failed"
        trash_wait ".confirmation.opened and .confirmation.token == $token and (.confirmation.destructiveFocus == false)" "$preset modified input stays in strip"
        key -k space >/dev/null
        trash_wait '(.confirmation.opened == false) and .total == 1 and (.busy == false)' "$preset Space on Cancel preserves Trash"
        trash_empty_strip
        key l >/dev/null
        key -k Escape >/dev/null
        trash_wait '(.confirmation.opened == false) and .total == 1 and (.busy == false)' "$preset Escape from danger cancels"
        trash_empty_strip
        trash_shot "trash-confirm-$preset"
        trash_click trashControlCentre cancel
        trash_wait '(.confirmation.opened == false) and .total == 1 and (.busy == false)' "$preset pointer Cancel"
        for move in left right; do
            trash_empty_strip
            key l >/dev/null
            trash_click trashControlCentre back "$move"
            trash_wait '.opened and (.confirmation.opened == false) and .total == 1 and (.busy == false)' "$preset outside $move click cancels without activating Back"
            [[ "$(ipc contextMenuVisible)" == false ]] || fail "trash: $preset outside $move click reached underlying menu"
        done
        trash_guard_store 1
    done
    kill_flea
    trash_guard "$XDG_STATE_HOME"
    "$flea_bin" --ui-state '{"keys":"default"}' >/dev/null || fail "trash: could not restore fixture preset"
    launch "$payload"
    wait_listing 1
    trash_rail
    trash_wait '.opened and .total == 1 and (.busy == false)'
}

trash_failure_status() {
    local headline="$1" detail="$2" end=$((SECONDS + 20)) primary observed
    while (( SECONDS < end )); do
        primary=$(ipc statusPrimary)
        observed=$(ipc statusDetail)
        if [[ "$(ipc statusError)" == true && "$primary" == "$headline" && "$observed" == *"$detail"* ]]; then
            trash_checks=$((trash_checks + 1))
            printf 'TRASH_PASS persistent failure expected=%q detail=%q observed=%q\n' "$headline" "$detail" "$observed"
            return
        fi
        sleep 0.05
    done
    fail "trash: expected persistent failure $headline with $detail, got $primary: $observed"
}

trash_restore_failures() {
    local uri backing row original="$trash_box/missing-parent/leaf.txt"
    trash_guard "$payload/beta.txt"
    (set -o noclobber; printf 'existing destination\n' > "$payload/beta.txt") \
        || fail "trash: refusing to overwrite the restore collision fixture"
    trash_click trashRowCentre 0 right
    row=$(menu_row_index Restore) || fail "trash: missing selected Restore row"
    trash_guard_store 1
    trash_click contextMenuRowCentre "$row"
    trash_failure_status 'Restored 0 of 1 · 1 failed' 'without overwriting'
    trash_wait '.opened and .total == 1 and .selectedCount == 1 and (.busy == false)'
    [[ "$(cat "$payload/beta.txt")" == 'existing destination' ]] || fail "trash: Restore overwrote an existing file"
    uri=$(/usr/bin/gio trash --list | cut -f1)
    backing=$(trash_backing "$uri") || fail "trash: failed restore has no backing"
    trash_guard "$backing"
    [[ "$(cat "$backing")" == beta ]] || fail "trash: collision changed the Trash survivor"
    trash_shot trash-restore-collision
    trash_guard "$trash_box/restore-blocker"
    mv --no-clobber -- "$payload/beta.txt" "$trash_box/restore-blocker" || fail "trash: could not preserve restore blocker"
    [[ ! -e "$payload/beta.txt" && "$(cat "$trash_box/restore-blocker")" == 'existing destination' ]] \
        || fail "trash: restore blocker was not safely moved"
    trash_rail right
    row=$(menu_row_index 'Restore all') || fail "trash: missing Restore all row"
    trash_guard_store 1
    trash_click contextMenuRowCentre "$row"
    trash_wait '.opened and .total == 0 and .count == 0 and (.busy == false)' 'explicit Restore all row restores survivor'
    trash_guard_store 0
    [[ "$(cat "$payload/beta.txt")" == beta ]] || fail "trash: Restore all lost the original content"
    [[ "$(ipc statusError)" == true ]] || fail "trash: successful Restore all silently acknowledged the collision"
    key -k Escape >/dev/null
    [[ "$(ipc statusError)" == false ]] || fail "trash: restore collision acknowledgement failed"

    trash_guard "$original"
    mkdir "$trash_box/missing-parent" || fail "trash: missing-parent fixture creation failed"
    printf 'missing parent survivor\n' > "$original"
    trash_guard_store 0
    /usr/bin/gio trash -- "$original" || fail "trash: owned missing-parent setup failed"
    trash_wait '.opened and .total == 1 and (.busy == false)'
    trash_guard "$trash_box/saved-parent"
    mv --no-clobber -- "$trash_box/missing-parent" "$trash_box/saved-parent" \
        || fail "trash: could not preserve original parent"
    [[ ! -e "$trash_box/missing-parent" && -d "$trash_box/saved-parent" ]] || fail "trash: original parent was not moved"
    trash_click trashRowCentre 0 right
    menu_seek Restore
    trash_guard_store 1
    key -k Return >/dev/null
    trash_failure_status 'Restored 0 of 1 · 1 failed' 'Could not open original location'
    trash_wait '.opened and .total == 1 and .selectedCount == 1 and (.busy == false)'
    [[ ! -e "$original" ]] || fail "trash: restore fabricated an unavailable parent"
    trash_shot trash-restore-missing-parent
    trash_guard "$trash_box/missing-parent"
    mv --no-clobber -- "$trash_box/saved-parent" "$trash_box/missing-parent" \
        || fail "trash: could not return original parent"
    [[ ! -e "$trash_box/saved-parent" && -d "$trash_box/missing-parent" ]] || fail "trash: original parent was not returned"
    trash_click trashRowCentre 0 right
    menu_seek Restore
    trash_guard_store 1
    key -k Return >/dev/null
    trash_wait '.opened and .total == 0 and .count == 0 and (.busy == false)'
    trash_guard_store 0
    [[ "$(cat "$original")" == 'missing parent survivor' ]] || fail "trash: retry after parent recovery lost content"
    [[ "$(ipc statusError)" == true ]] || fail "trash: retry silently acknowledged missing-parent failure"
    key -k Escape >/dev/null
    [[ "$(ipc statusError)" == false ]] || fail "trash: missing-parent acknowledgement failed"
    key -k Backspace >/dev/null
    trash_wait '(.opened == false)'
    wait_path "$payload"
    wait_listing 2
    trash_move beta.txt 0 1
    trash_rail
    trash_wait '.opened and .total == 1 and (.busy == false)'
}

trash_uri() {
    local original="$1" listing uri path found=""
    listing=$(/usr/bin/gio trash --list) || fail "trash: external identity listing failed"
    # Sample list row: "trash:///a.txt<TAB>/owned/fixture/payload/a.txt".
    while IFS=$'\t' read -r uri path; do
        [[ "$path" == "$original" ]] || continue
        [[ -z "$found" ]] || fail "trash: fixture original has ambiguous Trash identities"
        found="$uri"
    done <<< "$listing"
    [[ "$found" == trash:///* ]] || fail "trash: no provider identity for $original"
    printf '%s\n' "$found"
}

trash_stale_confirmations() {
    local token uri backing identity replacement="$trash_box/replacement-beta" retired="$trash_box/retired-beta"
    trash_empty_strip
    trash_shot trash-empty-confirm-cancel
    trash_guard_store 1
    key -k Return >/dev/null
    trash_wait '(.confirmation.opened == false) and .total == 1 and (.busy == false)'
    trash_empty_strip
    token=$(ipc trashState | jq -er '.confirmation.token')
    key l >/dev/null
    trash_wait '.confirmation.destructiveFocus'
    trash_guard "$payload/arrival.txt"
    (set -o noclobber; printf 'later arrival\n' > "$payload/arrival.txt") \
        || fail "trash: refusing to replace the arrival fixture"
    trash_guard_store 1
    /usr/bin/gio trash -- "$payload/arrival.txt" || fail "trash: external owned arrival failed"
    trash_wait ".confirmation.opened and .confirmation.count == 2 and .confirmation.token != $token and (.confirmation.destructiveFocus == false)" 'addition replaces stale all-items strip and resets Cancel focus'
    trash_guard_store 2
    trash_shot trash-confirm-added

    token=$(ipc trashState | jq -er '.confirmation.token')
    key l >/dev/null
    trash_wait '.confirmation.destructiveFocus'
    uri=$(trash_uri "$payload/arrival.txt")
    trash_guard_store 2
    trash_guard "$payload/arrival.txt"
    /usr/bin/gio trash --restore -- "$uri" || fail "trash: external owned restore failed"
    trash_wait ".confirmation.opened and .confirmation.count == 1 and .confirmation.token != $token and (.confirmation.destructiveFocus == false)" 'removal replaces stale all-items strip and resets Cancel focus'
    trash_guard_store 1
    [[ "$(cat "$payload/arrival.txt")" == 'later arrival' ]] || fail "trash: external restore changed contents"
    trash_shot trash-confirm-removed

    token=$(ipc trashState | jq -er '.confirmation.token')
    identity=$(ipc trashState | jq -c '.rows[0].identity')
    key l >/dev/null
    trash_wait '.confirmation.destructiveFocus'
    uri=$(trash_uri "$payload/beta.txt")
    backing=$(trash_backing "$uri") || fail "trash: missing replacement fixture backing"
    trash_guard "$backing"
    trash_guard "$replacement"
    trash_guard "$retired"
    (set -o noclobber; printf 'replacement contents\n' > "$replacement") \
        || fail "trash: refusing to reuse a replacement fixture"
    trash_guard_store 1
    ln -- "$backing" "$retired" || fail "trash: could not retain the replaced inode"
    # An atomic same-name replacement retains the old inode through its owned hard link.
    trash_guard "$backing"
    mv -T -- "$replacement" "$backing" || fail "trash: owned replacement failed"
    [[ "$(stat -c '%d:%i' "$backing")" != "$(stat -c '%d:%i' "$retired")" \
        && "$(cat "$backing")" == 'replacement contents' && "$(cat "$retired")" == beta ]] \
        || fail "trash: replacement did not preserve two distinct fixture identities"
    trash_wait ".confirmation.opened and .confirmation.count == 1 and .confirmation.token != $token and (.confirmation.destructiveFocus == false)" 'replacement invalidates old identity and resets Cancel focus'
    trash_guard_store 1
    trash_shot trash-confirm-replaced
    key -k Return >/dev/null
    trash_wait "(.confirmation.opened == false) and .total == 1 and (.busy == false) and .rows[0].identity != $identity" 'fresh replacement remains after reflexive Cancel'
    [[ "$(cat "$backing")" == 'replacement contents' ]] || fail "trash: stale strip deleted its replacement"

    trash_click trashRowCentre 0 left
    trash_wait '.opened and .selectedCount == 1 and (.busy == false)'
    trash_guard_store 1
    key -k Delete >/dev/null
    trash_wait '.confirmation.opened and (.confirmation.all == false) and .confirmation.count == 1'
    token=$(ipc trashState | jq -er '.confirmation.token')
    key l >/dev/null
    trash_wait '.confirmation.destructiveFocus'
    trash_guard "$payload/arrival.txt"
    trash_guard_store 1
    /usr/bin/gio trash -- "$payload/arrival.txt" || fail "trash: selected-set arrival failed"
    trash_wait ".confirmation.opened and (.confirmation.all == false) and .confirmation.count == 1 and .confirmation.token != $token and (.confirmation.destructiveFocus == false)" 'new arrival does not widen selected confirmation'
    trash_guard_store 2
    trash_shot trash-selected-arrival-excluded
    trash_click trashControlCentre danger
    trash_wait '.opened and .total == 1 and .count == 1 and (.busy == false) and (.rows[0].original | endswith("/arrival.txt"))' 'pointer confirmation deletes only reviewed selection'
    trash_guard_store 1
    uri=$(trash_uri "$payload/arrival.txt")
    backing=$(trash_backing "$uri") || fail "trash: unrelated arrival lost backing"
    trash_guard "$backing"
    [[ "$(cat "$backing")" == 'later arrival' && "$(cat "$retired")" == beta ]] \
        || fail "trash: selected deletion changed an unrelated fixture"
    trash_empty_confirmed 1
}

# The 30 day sweep, GM's ruling of 2026-09-11. It runs once, at startup, before the Trash window has
# ever existed, so this is its own launch with the setting already on and the fixture already holding
# items older than the threshold. Everything here is inside the marked fixture the caller made, and
# trash_guard refuses any path outside it.
trash_sweep_case() {
    local info recent old_one old_two swept day
    for name in keep.txt stale-one.txt stale-two.txt; do
        printf '%s\n' "$name" > "$payload/$name"
        /usr/bin/gio trash -- "$payload/$name" || fail "trash: sweep fixture could not be trashed"
    done
    trash_guard "$XDG_DATA_HOME/Trash/info"
    # gio writes DeletionDate into the .trashinfo beside each item, and it is the same field the
    # sweep reads back through trash::deletion-date, so backdating it here is backdating the item.
    for name in stale-one.txt stale-two.txt; do
        info="$XDG_DATA_HOME/Trash/info/$name.trashinfo"
        trash_guard "$info"
        [[ -f "$info" ]] || fail "trash: sweep fixture has no trashinfo for $name"
        sed -i "s/^DeletionDate=.*/DeletionDate=$(date -d '40 days ago' +%Y-%m-%dT%H:%M:%S)/" "$info" \
            || fail "trash: sweep fixture could not be backdated"
    done
    [[ "$(/usr/bin/gio trash --list | wc -l)" == 3 ]] || fail "trash: sweep fixture is not three items"
    "$flea_bin" --ui-state '{"trashAutoEmpty":true}' >/dev/null \
        || fail "trash: the sweep could not be switched on inside the fixture"
    launch "$payload"
    wait_listing 0
    # The two backdated items go and the recent one stays. This is the whole product promise, and it
    # is asserted against the provider rather than against Flea's own count alone.
    trash_wait '.count == 1' 'the sweep took the two items older than 30 days'
    [[ "$(/usr/bin/gio trash --list | wc -l)" == 1 ]] || fail "trash: the sweep left the wrong number of items"
    [[ "$(/usr/bin/gio trash --list | cut -f2)" == *keep.txt ]] || fail "trash: the sweep took the recent item"
    trash_guard_store 1
    trash_shot trash-sweep-done
    # The once-a-day guard: the day it ran is recorded, so a second launch today sweeps nothing.
    swept=$("$flea_bin" --ui-state 2>/dev/null | jq -er '.trashSweptOn') \
        || fail "trash: the sweep day could not be read back"
    # The same number ui/js/TrashDates.js dayNumber computes: whole days since the epoch at LOCAL
    # midnight, so a run either side of UTC midnight cannot disagree with the product.
    day=$(( $(date -d 'today 00:00:00' +%s) / 86400 ))
    [[ "$swept" == "$day" ]] || fail "trash: the sweep recorded day $swept, expected $day"
    trash_cleanup 0
}

case_trash() {
    local trash_box payload row uri backing root trash_checks=0
    local trash_case_label="${1:-full}"
    local trash_parent_bus_id="" trash_private_bus_id="" trash_bus_address="" trash_bus_pid="" trash_provider_pid=""
    [[ "$(realpath -e "$(command -v gio)")" == /usr/bin/gio ]] || fail "trash: product gio resolves to a stub"
    sandbox_require "$fixture_root"
    trash_box=$(mktemp -d "$fixture_root/trash.XXXXXXXX") || fail "trash: fixture creation failed"
    printf 'native private Trash\n' > "$trash_box/.flea-test-sandbox"
    [[ "$trash_box" == "$(realpath -e -- "$trash_box")" ]] || fail "trash: fixture root is not canonical"
    export XDG_DATA_HOME="$trash_box/data" XDG_CONFIG_HOME="$trash_box/config"
    export XDG_STATE_HOME="$trash_box/state" XDG_CACHE_HOME="$trash_box/cache"
    for root in "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"; do
        trash_guard "$root"
        mkdir -p "$root" || fail "trash: writable root creation failed"
    done
    "$flea_bin" --ui-state '{"view":"list","keys":"default","preview":{"column":false},"menu":{"hidden":[]}}' >/dev/null \
        || fail "trash: initial preferences could not be stored inside the fixture"
    payload="$trash_box/payload"
    trash_guard "$payload"
    [[ ! -e "$payload" ]] || fail "trash: refusing to reuse a prior payload"
    mkdir "$payload" || fail "trash: fixture creation failed"
    trash_start_bus
    if [[ "$trash_case_label" == sweep ]]; then trash_sweep_case; return; fi
    launch "$payload"
    wait_listing 0
    trash_guard_store 0
    trash_wait '.count == 0 and .rail.countText == "" and (.rail.current == false)'
    trash_rail
    trash_wait '.opened and .total == 0 and (.busy == false) and .rail.current and .rail.countText == ""'
    trash_shot trash-empty-current
    trash_rail right
    ipc contextMenuModel | jq -e '[.[] | select(.action == "restoreAll" or .action == "emptyTrash")] | (map(.action) | sort) == ["emptyTrash","restoreAll"] and all(.[]; .disabled == true)' >/dev/null \
        || fail "trash: empty actions must remain present and disabled"
    row=$(menu_row_index "Empty Trash") || fail "trash: missing Empty Trash row"
    trash_click contextMenuRowCentre "$row"
    trash_wait '.total == 0 and (.confirmation.opened == false) and (.operationActive == false)' 'disabled Empty Trash does nothing'
    if [[ "$(ipc contextMenuVisible)" == true ]]; then key -k Escape >/dev/null; fi
    trash_wait '.opened and .total == 0 and (.confirmation.opened == false)'
    key -k Backspace >/dev/null
    trash_wait '(.opened == false)'
    wait_path "$payload"

    if [[ "$trash_case_label" == raildetails ]]; then
        rail_details_native || fail "rail: native detail proof failed"
        trash_cleanup 0
    fi

    printf 'alpha\n' > "$payload/alpha.txt"
    printf 'beta\n' > "$payload/beta.txt"
    wait_listing 2
    trash_move alpha.txt 0 1
    trash_move beta.txt 1 0
    trash_wait '.count == 2'
    trash_guard_store 2
    trash_shot trash-full-not-current
    trash_rail
    trash_wait '.opened and .total == 2 and (.busy == false)'
    trash_shot trash-full-current
    row=$(ipc trashState | jq -er '.rows | map(.original | endswith("/alpha.txt")) | index(true)')
    trash_click trashRowCentre "$row" right
    menu_seek "Restore"
    trash_guard_store 2
    trash_guard "$payload/alpha.txt"
    key -k Return >/dev/null
    trash_wait '.total == 1 and (.busy == false)'
    [[ "$(cat "$payload/alpha.txt")" == alpha ]] || fail "trash: native Restore lost file contents"
    trash_guard_store 1
    if [[ "$trash_case_label" == basic ]]; then trash_cleanup 0; fi
    if [[ "$trash_case_label" == full || "$trash_case_label" == controls || "$trash_case_label" == keys ]]; then
        trash_confirmation_controls "$trash_case_label"
        if [[ "$trash_case_label" != full ]]; then trash_cleanup 0; fi
    fi
    if [[ "$trash_case_label" == full || "$trash_case_label" == restore ]]; then
        trash_restore_failures
        if [[ "$trash_case_label" == restore ]]; then trash_cleanup 0; fi
    fi
    if [[ "$trash_case_label" == failure ]]; then trash_empty_confirmed 1
    else trash_stale_confirmations; fi
    if [[ "$trash_case_label" == stale ]]; then trash_cleanup 0; fi

    key -k Backspace >/dev/null
    trash_guard "$payload/good.txt"
    trash_guard "$payload/locked"
    printf 'delete this\n' > "$payload/good.txt"
    mkdir "$payload/locked"
    printf 'survive failed delete\n' > "$payload/locked/child.txt"
    chmod 0555 "$payload/locked"
    wait_listing 3
    trash_move good.txt 0 2
    trash_move locked 1 1
    trash_rail
    trash_wait '.total == 2 and (.busy == false)'
    key -M ctrl -k a -m ctrl >/dev/null || fail "trash: Ctrl+A delivery failed"
    trash_wait '.selectedCount == 2 and (.busy == false)'
    trash_guard_store 2
    key -k Delete >/dev/null
    trash_wait '.confirmation.opened and (.confirmation.all == false) and .confirmation.count == 2'
    trash_shot trash-selected-confirm
    key l >/dev/null
    trash_guard_store 2
    key -k Return >/dev/null
    trash_wait '.total == 1 and .selectedCount == 1 and (.busy == false)'
    [[ "$(ipc statusPrimary)" == 'Deleted 1 of 2 · 1 failed' && "$(ipc statusError)" == true ]] \
        || fail "trash: partial deletion did not retain the named primary failure"
    [[ "$(ipc statusDetail)" == *locked* && "$(ipc statusDetail)" == *'Permission denied'* ]] \
        || fail "trash: partial deletion has no file-specific failure detail"
    trash_guard_store 1
    trash_shot trash-partial-failure
    uri=$(/usr/bin/gio trash --list | cut -f1)
    backing=$(trash_backing "$uri") || fail "trash: missing survivor backing"
    trash_guard "$backing"
    [[ "$(cat "$backing/child.txt")" == 'survive failed delete' ]] || fail "trash: failed survivor changed"
    chmod u+w "$backing"
    key -k F5 >/dev/null
    trash_wait '.total == 1 and (.busy == false)'
    trash_click trashRowCentre 0 left
    key -k Delete >/dev/null
    trash_wait '.confirmation.opened and .confirmation.count == 1'
    key l >/dev/null
    trash_guard_store 1
    key -k Return >/dev/null
    trash_wait '.total == 0 and (.busy == false)'
    trash_guard_store 0
    [[ "$(ipc statusError)" == true ]] || fail "trash: success silently acknowledged prior failure"
    key -k Escape >/dev/null
    [[ "$(ipc statusError)" == false ]] || fail "trash: explicit acknowledgement did not dismiss failure"
    trash_shot trash-recovered-empty
    trash_cleanup 0
}
