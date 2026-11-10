# GPUI overrides for UISH-13; seeded from the unchanged oracle at 2faf8d099.
case_unmounted() (
    local rail_box="$fixture_root/unmounted" rail_dir="$fixture_root/unmounted/home"
    local index before stamp
    sandbox_scratch "$rail_box"
    : > "$rail_box/.flea-test-sandbox" || fail 'rail: sandbox marker write failed'
    mkdir -p "$rail_box/state" "$rail_box/config" "$rail_box/cache" "$rail_box/data" || fail 'rail: private directories failed'
    fixture_home_make "$rail_dir"
    : > "$rail_box/calls.jsonl"
    rail_listing ""
    rail_path_box
    export HOME="$rail_dir" XDG_STATE_HOME="$rail_box/state" XDG_CONFIG_HOME="$rail_box/config"
    export XDG_CACHE_HOME="$rail_box/cache" XDG_DATA_HOME="$rail_box/data"
    export PATH="$rail_box/bin" FLEA_RAIL_BOX="$rail_box"

    echo "-- a 0.3.2 state file that stored the switch off reads on from the first 0.3.3 launch --"
    # Written raw, because any write through flea --ui-state already carries 0.3.3's stamp.
    local stored="$rail_box/state/flea/ui.json"
    mkdir -m 700 "$rail_box/state/flea" || fail 'rail: private state directory failed'
    # A key off its default beside the switch, so a migration that reset the file reads differently from one that did not.
    printf '{"view":"list","keys":"default","density":"comfortable","places":{"showUnmounted":false}}\n' > "$stored" \
        || fail 'rail: the 0.3.2 state file could not be written'
    # The stamp this binary ships: a read prints it and writes nothing, so the launch still owes the migration.
    stamp=$("$flea_bin" --ui-state | jq -r '.stateVersion') || fail 'rail: the shipped stamp could not be read'
    [[ "$stamp" =~ ^[0-9]+$ ]] || fail "rail: the shipped stamp reads as $stamp"
    launch "$rail_dir"
    rail_wait_entry false
    [[ "$(jq -c '[.places.showUnmounted, .stateVersion, .density]' "$stored")" == "[true,$stamp,\"comfortable\"]" ]] \
        || fail "rail: the launch did not write the migration down beside the file's own choice, ui.json holds $(jq -c '[.places.showUnmounted, .stateVersion, .density]' "$stored")"
    printf 'RAIL migrated=%s\n' "$(ipc deviceEntries | tr '\n' ' ')"
    kill_flea

    echo "-- switched off after the migration, unmounted rows stay off the rail --"
    "$flea_bin" --ui-state '{"places":{"showUnmounted":false}}' >/dev/null || fail 'rail: switch-off seed failed'
    launch "$rail_dir"
    wait_rail 2
    settle
    [[ "$(ipc deviceEntries)" != *"Archive"* ]] \
        || fail "rail: the switch is off and the rail still carries $(ipc deviceEntries)"
    printf 'RAIL off=%s\n' "$(ipc deviceEntries | tr '\n' ' ')"

    echo "-- but a mounted internal volume keeps Open and Unmount, and Ctrl+E releases it --"
    mkdir -p "$rail_box/mnt/Archive" || fail 'rail: outside mountpoint failed'
    rail_listing "$rail_box/mnt/Archive"
    rail_wait_entry true
    printf 'RAIL off-mounted=%s\n' "$(ipc deviceEntries | tr '\n' ' ')"
    index=$(rail_row_of Archive)
    click_rail_row "$index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount" ]] \
        || fail "rail: the mounted volume offers $(ipc contextMenuEntries), not Open then Unmount"
    [[ "$(ipc contextMenuGlyphs)" == "folder-open|-|drive" ]] \
        || fail "rail: the mounted volume draws $(ipc contextMenuGlyphs)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail 'rail: Escape left the menu open'
    [[ "$(ipc focusView)" == "rail" ]] || { key -k Tab >/dev/null; settle; }
    [[ "$(ipc focusView)" == "rail" ]] || fail "rail: Tab did not reach the rail, focus is $(ipc focusView)"
    rail_seek Archive
    key -M ctrl -k e -m ctrl >/dev/null
    for attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" != *"Archive"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" != *"Archive"* ]] \
        || fail "rail: Ctrl+E left $(ipc deviceEntries)"
    printf 'RAIL off-ctrl-e calls=%s\n' "$(rail_calls)"
    [[ "$(rail_calls)" == '["mount -u '"$rail_box"'/mnt/Archive"]' ]] \
        || fail "rail: Ctrl+E ran $(rail_calls)"
    kill_flea

    echo "-- and with the rail hidden, Ctrl+E from a listing inside the volume releases it through its own host --"
    # The double's 0.3 s stands in for gio's latency: a host unloaded at once would kill the unmount before it finished.
    "$flea_bin" --ui-state '{"places":{"showUnmounted":false,"rail":"hidden"}}' >/dev/null || fail 'rail: hidden-rail seed failed'
    rail_listing "$rail_box/mnt/Archive"
    launch "$rail_box/mnt/Archive"
    wait_path "$rail_box/mnt/Archive"
    [[ "$(ipc railCount)" == "0" ]] || fail "rail: the rail is not hidden, it carries $(ipc railCount) rows"
    key -M ctrl -k e -m ctrl >/dev/null
    for attempt in $(seq 1 200); do
        grep -q "$rail_box/mnt/Archive" "$rail_box/lsblk.json" || break
        sleep 0.05
    done
    ! grep -q "$rail_box/mnt/Archive" "$rail_box/lsblk.json" \
        || fail "rail: Ctrl+E with the rail hidden left the volume mounted, calls are $(rail_calls)"
    printf 'RAIL hidden-ctrl-e calls=%s\n' "$(rail_calls)"
    [[ "$(rail_calls)" == '["mount -u '"$rail_box"'/mnt/Archive","mount -u '"$rail_box"'/mnt/Archive"]' ]] \
        || fail "rail: Ctrl+E with the rail hidden ran $(rail_calls)"
    kill_flea
    "$flea_bin" --ui-state '{"places":{"rail":"shown"}}' >/dev/null || fail 'rail: shown-rail seed failed'
    # The Mount and Unmount checks below count only their own calls.
    : > "$rail_box/calls.jsonl"

    echo "-- switched on, the volume nothing mounted is a row of its own --"
    "$flea_bin" --ui-state '{"places":{"showUnmounted":true}}' >/dev/null || fail 'rail: switch seed failed'
    launch "$rail_dir"
    rail_wait_entry false
    printf 'RAIL on=%s\n' "$(ipc deviceEntries | tr '\n' ' ')"
    [[ "$(ipc deviceEntries)" != *"|volume|true"* ]] || fail 'rail: the fixture volume started out mounted'
    index=$(rail_row_of Archive)
    click_rail_row "$index" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail 'rail: the unmounted volume opened no menu'
    [[ "$(ipc contextMenuEntries)" == "Mount" ]] \
        || fail "rail: the unmounted volume offers $(ipc contextMenuEntries), not Mount alone"
    shot rail-unmounted-menu

    echo "-- Mount is the activation, and what comes up is opened --"
    before=$(rail_calls)
    key -k Return >/dev/null
    rail_wait_entry true
    wait_path "$rail_box/mnt/Archive"
    printf 'RAIL mounted calls=%s path=%s\n' "$(rail_calls)" "$(ipc path)"
    [[ "$(rail_calls)" == '["mount -d /dev/sdb1"]' ]] \
        || fail "rail: the Mount row ran $(rail_calls), not one gio mount by device (before: $before)"
    shot rail-mounted

    echo "-- and a mounted one offers the open beside its release --"
    index=$(rail_row_of Archive)
    click_rail_row "$index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount" ]] \
        || fail "rail: the mounted volume offers $(ipc contextMenuEntries), not Open then Unmount"
    [[ "$(ipc contextMenuGlyphs)" == "folder-open|-|drive" ]] \
        || fail "rail: the mounted volume draws $(ipc contextMenuGlyphs)"
    menu_seek Unmount
    key -k Return >/dev/null
    rail_wait_entry false
    printf 'RAIL unmounted calls=%s\n' "$(rail_calls)"
    [[ "$(rail_calls)" == '["mount -d /dev/sdb1","mount -u '"$rail_box"'/mnt/Archive"]' ]] \
        || fail "rail: Unmount ran $(rail_calls)"

    echo "-- Enter on the row mounts it the same way the menu row does --"
    # The chosen menu row handed the keyboard back to the listing, so the rail is asked for again.
    [[ "$(ipc focusView)" == "rail" ]] || { key -k Tab >/dev/null; settle; }
    [[ "$(ipc focusView)" == "rail" ]] || fail "rail: Tab did not reach the rail, focus is $(ipc focusView)"
    rail_seek Archive
    key -k Return >/dev/null
    rail_wait_entry true
    wait_path "$rail_box/mnt/Archive"
    [[ "$(rail_calls)" == *'"mount -d /dev/sdb1","mount -u '"$rail_box"'/mnt/Archive","mount -d /dev/sdb1"]' ]] \
        || fail "rail: Enter ran $(rail_calls)"
    printf 'RAIL enter calls=%s\n' "$(rail_calls)"

    # The board's second theme is not this harness's to shoot: launch() refuses a window painting
    # anything but the live theme, which is the guard that keeps a colour claim honest. The rail's
    # own squares are Theme.color.executable and muted, which tests/themes.sh sweeps on all 22.
    kill_flea
)

case_optical() {
    local dir="$fixture_root/optical" state="$fixture_root/optical-state"
    local label='MATSHITA DVD+/-RW UJ8FB' payload='optical-payload-143' gio_log="$dir/gio.log"
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    mkdir -p "$dir/bin" "$dir/files" "$dir/mnt/DVD" "$state/flea"
    printf 'keep\n' > "$dir/files/keep.txt"
    printf '%s' "$payload" > "$dir/mnt/DVD/disc-bytes.txt"
    : > "$gio_log"

    cat > "$dir/bin/lsblk" <<EOS
#!/bin/sh
if [ -f "$dir/malformed" ]; then
    printf 'not json at all\\n'
    exit 0
fi
if [ -f "$dir/mounted" ]; then
    optical_points='["$dir/mnt/DVD"]'
    optical_fs='"iso9660"'
elif [ -f "$dir/inserted" ]; then
    optical_points='[null]'
    optical_fs='"iso9660"'
else
    optical_points='[null]'
    optical_fs=null
fi
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",
"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},
{"name":"sr0","path":"/dev/sr0","label":null,"mountpoints":\$optical_points,"rm":true,"size":0,"type":"rom","fstype":\$optical_fs,"model":"$label"},
{"name":"sdb","path":"/dev/sdb","label":"USB","mountpoints":[null],"rm":true,"size":34359738368,"type":"disk","model":"USB Flash Disk","fstype":"vfat"}
]}
JSON
EOS
    chmod +x "$dir/bin/lsblk"

    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
printf '%s\\n' "\$*" >> "$gio_log"
if [ "\$1 \$2 \$3" = "mount -d /dev/sr0" ]; then
    [ -f "$dir/refuse" ] && exit 1
    : > "$dir/mounted"
fi
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/optical-home" real_home="$HOME" saved_path="$PATH" old_state="${XDG_STATE_HOME:-}"
    fixture_home_make "$fixture_home"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    export XDG_STATE_HOME="$state"
    launch "$dir/files"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 1

    local entries
    for _attempt in $(seq 1 200); do
        entries=$(ipc deviceEntries)
        [[ "$entries" == *"USB|device|volume|false"* ]] && break
        sleep 0.05
    done
    [[ "$entries" == *"USB|device|volume|false"* ]] \
        || fail "optical: the unmounted USB negative control is missing, got $entries"
    [[ "$entries" != *"$label|device|volume|"* ]] \
        || fail "optical: an empty optical drive was offered, got $entries"
    if grep -q '^mount -d /dev/sr0$' "$gio_log"; then
        fail "optical: an empty drive reached gio mount, log is $(cat "$gio_log")"
    fi
    shot optical-empty

    : > "$dir/inserted"
    wait_rail_label "$label"
    for _attempt in $(seq 1 200); do
        entries=$(ipc deviceEntries)
        [[ "$entries" == *"$label|device|volume|false"* ]] && break
        sleep 0.05
    done
    [[ "$entries" == *"$label|device|volume|false"* ]] \
        || fail "optical: inserted media did not remain unmounted, got $entries"
    : > "$dir/refuse"
    click_rail_row "$(rail_row_of "$label")" left
    wait_message "$label could not be mounted."
    [[ "$(ipc lastMessage)" != *unplug* ]] || fail "optical: refusal still advised unplugging: $(ipc lastMessage)"
    grep -q '^mount -d /dev/sr0$' "$gio_log" \
        || fail "optical: inserted media never reached gio mount, log is $(cat "$gio_log")"
    shot optical-refused

    rm -f "$dir/refuse"
    : > "$dir/mounted"
    for _attempt in $(seq 1 200); do
        entries=$(ipc deviceEntries)
        [[ "$entries" == *"$label|device|volume|true"* ]] && break
        sleep 0.05
    done
    [[ "$entries" == *"$label|device|volume|true"* ]] \
        || fail "optical: mounted media did not retain its row, got $entries"
    click_rail_row "$(rail_row_of "$label")" left
    wait_path "$dir/mnt/DVD"
    wait_listing 1
    [[ "$(ipc rowAt 0)" == disc-bytes.txt\|file\|* ]] \
        || fail "optical: mounted disc lists $(ipc rowAt 0), not disc-bytes.txt"
    shot optical-mounted
    open_row disc-bytes.txt
    for _attempt in $(seq 1 100); do
        [[ "$(ipc previewState)" == ready ]] && break
        sleep 0.05
    done
    [[ "$(ipc previewState)" == ready && "$(ipc previewText)" == "$payload" ]] \
        || fail "optical: mounted disc bytes are state=$(ipc previewState), bytes=$(ipc previewText | od -An -tx1)"
    key -k Escape >/dev/null
    settle

    : > "$dir/malformed"
    for _attempt in $(seq 1 200); do
        entries=$(ipc deviceEntries)
        [[ "$entries" != *"$label|device|volume|"* ]] && break
        sleep 0.05
    done
    [[ "$entries" != *"$label|device|volume|"* ]] \
        || fail "optical: malformed lsblk input retained a stale row, got $entries"
    rm -f "$dir/malformed"
    printf 'OPTICAL empty-hidden=ok usb-retained=ok inserted=ok refusal=observable mounted=ok bytes=exact malformed=clears\n'
    if [[ -n "$old_state" ]]; then export XDG_STATE_HOME="$old_state"; else unset XDG_STATE_HOME; fi
    kill_flea
    sandbox_remove "$fixture_home"
}

case_phones() {
    local dir="$fixture_root/phones" state="$fixture_root/phones-state"
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    mkdir -p "$state/flea"
    phones_gio_stub "$dir"
    local fuse="$phones_fuse" mtp_uri="$phones_mtp_uri" afc_fuse="$phones_afc_fuse" unmount_log="$phones_unmount_log"
    local gvfs_parent="$dir/gvfs" gvfs_count=1
    local -a flea_launch_wrap=()
    if [[ "$test_frontend" == gpui ]]; then
        gvfs_parent="/run/user/$(id -u)/gvfs"
        [[ -d "$gvfs_parent" ]] || fail 'phones: canonical GVFS mountpoint is unavailable'
        cp -a "$afc_fuse" "$dir/gvfs/"
        : > "$dir/gvfs/.flea-test-sandbox"
        local virtual_fuse="$gvfs_parent/${fuse##*/}" virtual_afc="$gvfs_parent/${afc_fuse##*/}"
        python3 "$repo/tests/lib/flea_phone_paths.py" "$dir/bin/gio" "$fuse" "$virtual_fuse" "$afc_fuse" "$virtual_afc" \
            || fail 'phones: private GIO paths could not be mapped'
        fuse=$virtual_fuse
        afc_fuse=$virtual_afc
        gvfs_count=2
        flea_launch_wrap=(/bin/bash "$repo/tests/lib/flea-gvfs-namespace.sh" --enter "$dir/gvfs" "$gvfs_parent"
            "$(readlink /proc/self/ns/user)" "$(readlink /proc/self/ns/mnt)")
    fi

    local fixture_home="$fixture_root/phones-home"
    fixture_home_make "$fixture_home"
    # Drive size on is what makes a phone row's detail reachable: a device row draws one when its
    # size is not null, and a row carrying no size at all wrote a type error into this run's log.
    printf '{"view":"list","places":{"driveSize":true,"trashCount":true}}\n' > "$state/flea/ui.json"
    local real_home="$HOME" saved_path="$PATH" old_state="${XDG_STATE_HOME:-}"
    export PATH="$dir/bin:$PATH"
    export XDG_STATE_HOME="$state"
    export HOME="$fixture_home"
    launch "$dir/files"
    export HOME="$real_home"
    wait_listing 1
    wait_rail 1

    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* \
        && "$(ipc deviceEntries)" == *"NIKON DSC D3500|device|phone|false"* \
        && "$(ipc deviceEntries)" == *"GM’s iPhone|device|phone|false"* ]] \
        || fail "phones: the three volumes are not three unmounted DEVICES rows, got $(ipc deviceEntries)"
    # Directive 44: the iPhone answers on GPhoto2 as well, and that leg is folded into the row above
    # rather than drawn beside it, so the phone is one row and its camera store is not a second.
    [[ "$(ipc deviceEntries)" != *"iPhone|device|phone|false|camera"* ]] \
        || fail "phones: the iPhone's camera leg drew its own row, got $(ipc deviceEntries)"
    # Behind the block devices, per the DEVICES rule: a block device leads and the phones are the tail.
    ipc railEntries | jq -e '[.[] | select(.group == "device") | .kind] | index("phone") as $i
        | ($i != null) and ($i > 0) and (.[$i:] | all(. == "phone"))' >/dev/null \
        || fail "phones: the phone rows do not sit behind the block devices, got $(ipc railEntries | jq -c '[.[]|select(.group=="device")|.kind]')"
    # PhoneMark rule 1: the monitor picks the mark, MTP the phone and GPhoto2 the camera.
    # AFC is a phone the same way MTP is: the mark names the transport, and only GPhoto2 is a camera.
    ipc railEntries | jq -e '[.[] | select(.kind == "phone") | .glyph] == ["smartphone", "camera", "smartphone"]' >/dev/null \
        || fail "phones: the marks are $(ipc railEntries | jq -c '[.[]|select(.kind=="phone")|.glyph]'), not phone, camera, phone"
    # RailDetails: a phone has no capacity to draw, and it keeps the fixed indicator slot a volume has.
    ipc railDetails | jq -e '[.rows[] | select(.kind == "phone")] | length == 3
        and all(.[]; .detail == "" and .indicatorVisible)' >/dev/null \
        || fail "phones: a phone row drew a size or lost its indicator, got $(ipc railDetails | jq -c '[.rows[]|select(.kind=="phone")|{detail,indicatorVisible}]')"

    # RailEject's row recipe: a phone label ends one row gap before its indicator slot.
    ipc railDetails | jq -e '.rowGap as $g | [.rows[] | select(.kind == "phone")] | length == 3
        and all(.[]; (.indicatorRect.x - .labelRect.x - .labelRect.width - $g) as $d | $d > -1 and $d < 1)' >/dev/null \
        || fail "phones: a phone label does not end one gap before its slot, got $(ipc railDetails | jq -c '{rowGap, rows: [.rows[]|select(.kind=="phone")|{label,labelRect,indicatorRect}]}')"
    # The same recipe cuts a label only when its text does not fit before that gap: a label that fits draws whole, one that does not elides.
    ipc railDetails | jq -e '.rowGap as $g | [.rows[] | select(.kind == "phone")] | length == 3
        and all(.[]; if .labelNeeds <= .indicatorRect.x - .labelRect.x - $g then (.elided | not) else .elided end)' >/dev/null \
        || fail "phones: a phone label is cut while it fits, or overruns its slot without eliding, got $(ipc railDetails | jq -c '[.rows[]|select(.kind=="phone")|{label,labelWidth,labelNeeds,elided,labelRect,indicatorRect}]')"
    # A label that does not fit keeps its start and ends in the ellipsis: Text.ElideRight is 1.
    ipc railDetails | jq -e '[.rows[] | select(.kind == "phone")] | length == 3 and all(.[]; .elide == 1)' >/dev/null \
        || fail "phones: a phone label does not elide at its end, got $(ipc railDetails | jq -c '[.rows[]|select(.kind=="phone")|{label,elide,elided}]')"

    # An unmounted volume offers the mount its own row does, per RailAdditions rule 2, and choosing it
    # is the row's own activation: this is the only thing that drives Sidebar.openPhone.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" && "$(ipc contextMenuEntries)" == "Mount" ]] \
        || fail "phones: an unmounted phone's menu is $(ipc contextMenuEntries), not Mount alone"
    key -k Escape >/dev/null
    settle

    # Activating the unmounted row mounts it, resolves the folder and opens it, the way a share does.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" left
    wait_path "$fuse"
    wait_listing 1
    [[ "$(ipc rowAt 0)" == "DCIM|"* ]] || fail "phones: the phone's own listing is $(ipc rowAt 0)"
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|true"* ]] \
        || fail "phones: the row did not survive its own mount, got $(ipc deviceEntries)"
    [[ -z "$(ipc networkEntries)" ]] \
        || fail "phones: gio's shadow mount became a NETWORK row as well, got $(ipc networkEntries)"
    shot phones-mounted

    # Issue 133: gio cannot trash into this mount, so neither the row nor the key is offered here.
    click_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" && "$(ipc contextMenuEntries)" == *"Move to Dropbox"* ]] \
        || fail "phones: the listing menu did not open on the phone's own row, got $(ipc contextMenuEntries)"
    [[ "$(ipc contextMenuEntries)" != *"Move to Trash"* ]] \
        || fail "phones: an MTP path still offers Move to Trash: $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle
    # The listing holds the keyboard here, so the silence below is the guard refusing and not a key
    # that went to the rail: asserted before the key rather than recovered from afterwards.
    focus_now=$(ipc focusView)
    [[ "$focus_now" == "list" ]] || fail "phones: the listing does not hold the keyboard, focus is $focus_now"
    key d >/dev/null
    # 50 ms apart cannot step over an arm that stands for four seconds, and a second of them outlasts the key's own round trip.
    for _attempt in $(seq 1 20); do
        [[ "$(ipc statusPrimary)" != *"Press d again"* ]] \
            || fail "phones: d armed a trash that can only fail, the bar reads $(ipc statusPrimary)"
        sleep 0.05
    done
    # One directory up is the folder the mount sits in rather than the mount, so the same key does arm
    # there: the silence above is the guard and not a keyboard that stopped answering.
    key h >/dev/null
    wait_path "$gvfs_parent"
    wait_listing "$gvfs_count"
    key j >/dev/null
    settle
    key d >/dev/null
    wait_message "Press d again to move it to Trash, or press Delete on its own."
    # The arm itself, not the hint: ui/Pane.qml escapePressed clears a selection before it clears the
    # bar, and coming up from the mount leaves the row we left selected (ui/js/Nav.js
    # applyPendingSelect), so the sentence stands for its four seconds while the arm is already gone.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc keyDeliveryState | jq -er '.trashArmedAt')" == "0" ]] \
        || fail "phones: Escape left the trash armed one directory up, the arm reads $(ipc keyDeliveryState | jq -c '.trashArmedAt')"
    key -k Return >/dev/null
    wait_path "$fuse"
    wait_listing 1

    # Unmount is the release a phone offers, never Eject, and the key that carries it is its uri.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount" ]] \
        || fail "phones: the mounted phone's menu is $(ipc contextMenuEntries), not Open then Unmount"
    menu_seek Unmount
    key -k Return >/dev/null
    for _attempt in $(seq 1 200); do
        [[ -s "$unmount_log" ]] && break
        sleep 0.05
    done
    [[ "$(cat "$unmount_log")" == "UNMOUNT $mtp_uri" ]] \
        || fail "phones: the menu row unmounted $(cat "$unmount_log"), not $mtp_uri"
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] \
        || fail "phones: the row never came back unmounted, got $(ipc deviceEntries)"

    # And the menu's own Mount drives the same activation, on the row the unmount above just released:
    # this is the only thing that reaches Sidebar.openPhone through the menu.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Mount" ]] \
        || fail "phones: the released phone's menu is $(ipc contextMenuEntries), not Mount alone"
    key -k Return >/dev/null
    wait_path "$fuse"
    wait_listing 1
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|true"* ]] \
        || fail "phones: the menu's Mount did not mount the row, got $(ipc deviceEntries)"

    # #181 from the keyboard: Enter on the released row only starts its mount, and focus follows the folder once it opens.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" right
    settle
    menu_seek Unmount
    : > "$unmount_log"
    key -k Return >/dev/null
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] \
        || fail "phones: the row did not come back unmounted for the keyboard leg, got $(ipc deviceEntries)"
    # Off the mount first, so the wait below can only be satisfied by this leg's own open.
    [[ "$(ipc path)" != "$fuse" ]] || { key h >/dev/null; wait_path "$gvfs_parent"; }
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "phones: Tab did not reach the rail, focus is $(ipc focusView)"
    key g >/dev/null
    for ((_step = 0; _step < $(rail_row_of 'SAMSUNG Android'); _step++)); do key j >/dev/null; done
    settle
    [[ "$(ipc railCursor)" == "$(rail_row_of 'SAMSUNG Android')" ]] || fail "phones: the rail cursor is on row $(ipc railCursor), not the phone"
    rm -f "$dir/mount-started"
    key -k Return >/dev/null
    wait_marker "$dir/mount-started" "phones: Enter on the unmounted phone never started its mount"
    [[ "$(ipc focusView)" == "rail" ]] \
        || fail "phones: Enter on the unmounted phone moved focus to $(ipc focusView) before its mount landed"
    wait_path "$fuse"
    wait_listing 1
    [[ "$(ipc focusView)" == "list" ]] \
        || fail "phones: Enter on the unmounted phone opened it but left focus on $(ipc focusView), not the list"

    # The same three legs on the iPhone's own row, which reaches its files over AFC: its menu offers
    # the mount, activating it resolves the root rather than the documents volume gvfs advertises,
    # and DCIM is what lists. Directive 44's live proof on the plugged phone is a manual check.
    click_rail_row "$(rail_row_of 'GM’s iPhone')" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" && "$(ipc contextMenuEntries)" == "Mount" ]] \
        || fail "phones: the unmounted iPhone's menu is $(ipc contextMenuEntries), not Mount alone"
    key -k Escape >/dev/null
    settle
    click_rail_row "$(rail_row_of 'GM’s iPhone')" left
    wait_path "$afc_fuse"
    wait_listing 1
    [[ "$(ipc rowAt 0)" == "DCIM|"* ]] || fail "phones: the iPhone's own listing is $(ipc rowAt 0)"
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"GM’s iPhone|device|phone|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"GM’s iPhone|device|phone|true"* ]] \
        || fail "phones: the iPhone row did not survive its own mount, got $(ipc deviceEntries)"
    [[ -z "$(ipc networkEntries)" ]] \
        || fail "phones: the AFC root mount became a NETWORK row as well, got $(ipc networkEntries)"
    shot phones-iphone-mounted
    printf 'PHONES rows=ok marks=ok mount=ok trash-guard=ok unmount=ok iphone=ok\n'
    export PATH="$saved_path"
    if [[ -n "$old_state" ]]; then export XDG_STATE_HOME="$old_state"; else unset XDG_STATE_HOME; fi
    kill_flea
    sandbox_remove "$fixture_home"
}

case_eject() {
    local dir="$fixture_root/eject"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin" "$dir/mnt/FLEASTICK"
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"

    local gio_log="$dir/gio.log"
    : > "$gio_log"
    # Every state this case needs, at zero privilege: one internal disk carrying /, and one removable
    # volume whose mountpoint goes away only once the gio stub has been told to really eject it.
    # The columns are the ones ui/DeviceMounts.qml asks for, MOUNTPOINTS and PATH included.
    cat > "$dir/bin/lsblk" <<EOS
#!/bin/sh
if [ -f "$dir/ejected" ]; then
  mp=null
else
  mp='"$dir/mnt/FLEASTICK"'
fi
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G",
"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":"238.5G","type":"part","model":null}]},
{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",
"children":[{"name":"sda1","path":"/dev/sda1","label":"FLEASTICK","mountpoints":[\$mp],"rm":true,"size":"116.1G","type":"part","model":null,"fstype":"vfat"}]}]}
JSON
EOS
    chmod +x "$dir/bin/lsblk"
    # Always exit 0, whatever it was asked to do: that is what the real gio did over a volume it
    # had not ejected, and it is why the verdict is never allowed to read an exit code.
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
printf '%s\n' "\$*" >> "$gio_log"
if [ "\$1 \$2" = "mount -e" ] && [ -f "$dir/really" ]; then
  : > "$dir/ejected"
fi
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/eject-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME" saved_path="$PATH" real_state="${XDG_STATE_HOME-}"
    # Show unmounted drives ships on from 0.3.3, so this seeds it on: a mounted volume
    # then offers Open, Unmount, Eject, and the case proves Ctrl+E and the menu both release it.
    seed_ui_state "$fixture_root/eject-state" '{"places":{"showUnmounted":true}}'
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/, mnt/ and gio.log are the stubs' own fixture entries beside the two files under test.
    wait_listing 5
    for _attempt in $(seq 1 100); do
        [[ "$(ipc deviceEntries)" == *"FLEASTICK|device|volume|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"FLEASTICK|device|volume|true" ]] \
        || fail "eject: the stub volume never appeared live, got $(ipc deviceEntries)"
    # The hostname prefix makes the disk row's label the box's own, so the shape is asserted, not the text.
    [[ "$(ipc deviceEntries | grep -c '|device|disk|true')" == "1" ]] \
        || fail "eject: the stub listing did not produce one internal disk row"
    [[ "$(ipc themeLoaded)" == "true" ]] || fail "eject: the fixture home did not load a real theme"

    # Network is empty here, so the last two rail rows are the internal disk and the volume.
    local rail_count volume_row disk_row
    rail_count=$(ipc railCount)
    volume_row=$((rail_count - 1))
    disk_row=$((rail_count - 2))

    # The internal disk is in this group and reads as mounted, and it must be offered nothing:
    # an eject on it would ask gio to spin the box's own system disk down.
    click_rail_row "$disk_row" right
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "eject: the internal disk was offered a menu"

    click_rail_row "$volume_row" right
    settle
    printf 'EJECT menu visible=%s entries=%s glyphs=%s\n' \
        "$(ipc contextMenuVisible)" "$(ipc contextMenuEntries)" "$(ipc contextMenuGlyphs)"
    shot eject-menu
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "eject: right click opened no menu on the volume"
    [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount|Eject|-|Flash Image" ]] \
        || fail "eject: the volume's menu is $(ipc contextMenuEntries), not Open|-|Unmount|Eject|-|Flash Image"
    [[ "$(ipc contextMenuGlyphs)" == "folder-open|-|drive|eject|-|file" ]] \
        || fail "eject: the volume's menu draws $(ipc contextMenuGlyphs), not folder-open|-|drive|eject|-|file"
    if grep -q '^mount -e' "$gio_log"; then
        fail "eject: opening the menu already ejected: $(cat "$gio_log")"
    fi

    # Eject is the third row, not the first: choosing rows[0] would open the stick, which is
    # the defect ui/js/Eject.js releaseFromRows exists for. Seek it by label, not by count.
    menu_seek Eject

    # The negative control, and the whole point of the case: gio exits 0 and the volume is still
    # mounted, so the sentence must refuse. A verdict read off the exit code would say safe here.
    key -k Return >/dev/null
    settle
    grep -q "^mount -e $dir/mnt/FLEASTICK\$" "$gio_log" \
        || fail "eject: the menu row did not run gio mount -e on the mount point, log is: $(cat "$gio_log")"
    local refusal="FLEASTICK is still mounted; close what is using it."
    local seen=""
    for _attempt in $(seq 1 250); do
        seen=$(ipc lastMessage)
        if [[ "$seen" == *"safe to unplug"* ]]; then
            fail "eject: gio exited 0 over a still-mounted volume and the status bar said: $seen"
        fi
        if [[ "$seen" == "$refusal" ]]; then
            break
        fi
        sleep 0.1
    done
    printf 'EJECT exit-zero-still-mounted message=%q\n' "$seen"
    [[ "$seen" == "$refusal" ]] || fail "eject: the refusal sentence is: $seen"
    shot eject-still-mounted

    # Now the same click with a stub that really unmounts it, and only then does the sentence change.
    : > "$dir/really"
    click_rail_row "$volume_row" right
    settle
    menu_seek Eject
    key -k Return >/dev/null
    wait_message "Ejected FLEASTICK. It is safe to unplug."
    printf 'EJECT really entries=%q\n' "$(ipc deviceEntries)"
    shot eject-safe

    # An unmounted volume with the switch on offers the mount its own activation does, so its
    # menu is a Mount row rather than no menu at all; choosing nothing leaves gio untouched.
    click_rail_row "$volume_row" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] \
        || fail "eject: an unmounted volume with unmounted drives shown was offered no menu"
    [[ "$(ipc contextMenuEntries)" == "Mount|-|Flash Image" ]] \
        || fail "eject: the unmounted volume's menu is $(ipc contextMenuEntries), not Mount|-|Flash Image"
    key -k Escape >/dev/null
    settle

    # gio's own -f is offered nowhere, and the eject never went through --device, which glib
    # dispatches before it ever reads --eject: see ui/DeviceMounts.qml "eject".
    if grep -qE '(^| )(-f|--force)( |$)' "$gio_log"; then
        fail "eject: a forced unmount reached gio: $(cat "$gio_log")"
    fi
    if grep -q '^mount -d' "$gio_log"; then
        fail "eject: an eject went through --device, which glib dispatches before --eject"
    fi
    [[ "$(grep -c '^mount -e ' "$gio_log")" == "2" ]] \
        || fail "eject: expected exactly two ejects, log is: $(cat "$gio_log")"

    printf 'EJECT menu=ok internal-disk-offers-nothing=ok exit-code-is-not-the-verdict=ok listing-is=ok no-force=ok\n'
    kill_flea
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    sandbox_remove "$fixture_home"
}

case_poweroff() {
    local dir="$fixture_root/poweroff"
    sandbox_scratch "$dir"
    # The fixture disk name must not exist on the host, or the eject chain watches a real disk.
    [[ ! -e /sys/block/sdflea ]] || fail "poweroff: fixture disk sdflea exists on this host"
    mkdir -p "$dir/bin" "$dir/mnt/DATA1" "$dir/mnt/DATA2"
    : > "$dir/0-one.txt"

    local gio_log="$dir/gio.log"
    : > "$gio_log"
    cat > "$dir/bin/lsblk" <<EOS
#!/bin/sh
mp1="$dir/mnt/DATA1"
mp2="$dir/mnt/DATA2"
[ -f "$dir/un-sdflea1" ] && mp1=""
[ -f "$dir/un-sdflea2" ] && mp2=""
if [ -n "\$mp1" ]; then j1="\"\$mp1\""; else j1=null; fi
if [ -n "\$mp2" ]; then j2="\"\$mp2\""; else j2=null; fi
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G",
"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":"238.5G","type":"part","model":null}]},
{"name":"sdflea","path":"/dev/sdflea","label":null,"mountpoints":[null],"rm":false,"tran":"usb","size":"1000.2G","type":"disk","model":"USB HDD",
"children":[{"name":"sdflea1","path":"/dev/sdflea1","label":"DATA1","mountpoints":[\$j1],"rm":false,"size":"500.1G","type":"part","model":null,"fstype":"ext4"},
{"name":"sdflea2","path":"/dev/sdflea2","label":"DATA2","mountpoints":[\$j2],"rm":false,"size":"500.1G","type":"part","model":null,"fstype":"ext4"}]}]}
JSON
EOS
    chmod +x "$dir/bin/lsblk"
    # A hung leg outlasts the deadline; slow legs pass the old whole-chain bound per leg.
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
printf '%s\n' "\$*" >> "$gio_log"
case "\$1 \$2" in
"mount -u")
  if [ -f "$dir/hang-unmount" ] && [ "\$3" = "$dir/mnt/DATA1" ]; then sleep 30; fi
  if [ -f "$dir/slowlegs" ]; then sleep 8; fi
  case "\$3" in
  "$dir/mnt/DATA1") : > "$dir/un-sdflea1" ;;
  "$dir/mnt/DATA2") : > "$dir/un-sdflea2" ;;
  esac
  exit 0 ;;
"mount -d")
  case "\$3" in
  /dev/sdflea1) rm -f "$dir/un-sdflea1" ;;
  /dev/sdflea2) rm -f "$dir/un-sdflea2" ;;
  esac
  exit 0 ;;
*) exit 0 ;;
esac
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/poweroff-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME" saved_path="$PATH" real_state="${XDG_STATE_HOME-}"
    seed_ui_state "$fixture_root/poweroff-state" '{"places":{"showUnmounted":true}}'
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/, mnt/ and gio.log are the stub entries beside the one file under test.
    wait_listing 4
    for _attempt in $(seq 1 100); do
        [[ "$(ipc deviceEntries)" == *"DATA1|device|volume|true"* ]] \
            && [[ "$(ipc deviceEntries)" == *"DATA2|device|volume|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"DATA1|device|volume|true"* ]] \
        || fail "poweroff: DATA1 never appeared live, got $(ipc deviceEntries)"
    [[ "$(ipc deviceEntries)" == *"DATA2|device|volume|true"* ]] \
        || fail "poweroff: DATA2 never appeared live, got $(ipc deviceEntries)"

    # The hung leg: the first unmount never answers, so the chain deadline ends it.
    : > "$dir/hang-unmount"
    click_rail_row "$(poweroff_row DATA1)" right
    settle
    menu_seek Eject
    key -k Return >/dev/null
    sleep 3
    # A user unmount of the other volume mid-chain is refused and never reaches gio.
    local unmounts_before unmounts_after
    unmounts_before=$(grep -c "^mount -u $dir/mnt/DATA2\$" "$gio_log" || true)
    click_rail_row "$(poweroff_row DATA2)" right
    settle
    menu_seek Unmount
    key -k Return >/dev/null
    wait_message "Still ejecting DATA1; wait for its result"
    unmounts_after=$(grep -c "^mount -u $dir/mnt/DATA2\$" "$gio_log" || true)
    [[ "$unmounts_after" == "$unmounts_before" ]] \
        || fail "poweroff: the refused unmount still ran gio mount -u on DATA2"
    # The deadline ends the hung leg with its own sentence, and the volume stays mounted.
    wait_message "DATA1 did not finish ejecting and is still mounted."
    [[ "$(ipc deviceEntries)" == *"DATA1|device|volume|true"* ]] \
        || fail "poweroff: the hung volume left the rail: $(ipc deviceEntries)"
    rm -f "$dir/hang-unmount"

    # Two slow legs finish past the old whole-chain bound and earn the safe sentence.
    : > "$dir/slowlegs"
    click_rail_row "$(poweroff_row DATA1)" right
    settle
    menu_seek Eject
    key -k Return >/dev/null
    # A slow-legs failure names its guard: the rail plus the eject chain's own state.
    wait_message "Ejected DATA1. It is safe to unplug." 'printf "rail: %s chain: %s" "$(ipc deviceEntries)" "$(ipc deviceEjectState)"'
    rm -f "$dir/slowlegs"

    # A later mount and unmount are answered with their own verdicts, not the chain's.
    click_rail_row "$(poweroff_row DATA2)" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Mount|-|Flash Image" ]] \
        || fail "poweroff: the unmounted volume offers $(ipc contextMenuEntries), not Mount|-|Flash Image"
    menu_seek Mount
    key -k Return >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc deviceEntries)" == *"DATA2|device|volume|true"* ]] && break
        sleep 0.1
    done
    [[ "$(ipc deviceEntries)" == *"DATA2|device|volume|true"* ]] \
        || fail "poweroff: DATA2 never remounted: $(ipc deviceEntries)"
    click_rail_row "$(poweroff_row DATA2)" right
    settle
    menu_seek Unmount
    key -k Return >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc deviceEntries)" == *"DATA2|device|volume|false"* ]] && break
        sleep 0.1
    done
    [[ "$(ipc deviceEntries)" == *"DATA2|device|volume|false"* ]] \
        || fail "poweroff: the later unmount never landed: $(ipc deviceEntries)"
    if grep -qE '(^| )(-f|--force)( |$)' "$gio_log"; then
        fail "poweroff: a forced unmount reached gio: $(cat "$gio_log")"
    fi

    printf 'POWEROFF deadline=ok still-ejecting-refused=ok slow-legs-safe=ok later-mount=ok later-unmount=ok no-force=ok\n'
    kill_flea
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    sandbox_remove "$fixture_home"
}
