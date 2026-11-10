#!/usr/bin/env bash
# Sourced by ui.sh; RailAdditions rule 1 and rule 2 driven against doubles on a private PATH.
# shellcheck disable=SC2154 # ui.sh supplies the fixture root, the candidate paths and the helpers.

# The box has no unmounted internal volume of its own and nothing here may create one: a loop device
# is a udisks operation behind polkit, and a polkit prompt on this display swallows the very keys the
# case is driving. So lsblk answers from a file the case owns, and gio records what it was asked to
# do and rewrites that file the way udisks would, which is the same shape tests/ui-providers.sh uses
# for Tailscale and Dropbox.
rail_listing() {
    local mountpoint="$1" point
    point=$([[ -n "$mountpoint" ]] && printf '"%s"' "$mountpoint" || printf 'null')
    cat > "$rail_box/lsblk.json" <<JSON
{"blockdevices":[
 {"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"tran":"nvme","size":256060514304,"type":"disk","model":"KBG40ZNS256G","fstype":null,"parttypename":null,
  "children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"tran":null,"size":256060514304,"type":"part","model":null,"fstype":"btrfs","parttypename":"Linux filesystem"}]},
 {"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"tran":"sata","size":2000398934016,"type":"disk","model":"Samsung SSD 870","fstype":null,"parttypename":null,
  "children":[{"name":"sdb1","path":"/dev/sdb1","label":"Archive","mountpoints":[$point],"rm":false,"tran":null,"size":1000398934016,"type":"part","model":null,"fstype":"ext4","parttypename":"Linux filesystem"},
   {"name":"sdb2","path":"/dev/sdb2","label":null,"mountpoints":[null],"rm":false,"tran":null,"size":536870912,"type":"part","model":null,"fstype":"vfat","parttypename":"EFI System"},
   {"name":"sdb3","path":"/dev/sdb3","label":null,"mountpoints":["[SWAP]"],"rm":false,"tran":null,"size":8589934592,"type":"part","model":null,"fstype":"swap","parttypename":"Linux swap"}]}
]}
JSON
}

# Every command the app reaches for stays reachable; lsblk and gio are the two this case answers for.
rail_path_box() {
    local part file name
    local -a parts
    mkdir -p "$rail_box/bin" || fail 'rail: private bin failed'
    IFS=: read -r -a parts <<< "$PATH"
    for part in "${parts[@]}"; do
        [[ -d "$part" ]] || continue
        for file in "$part"/*; do
            [[ -f "$file" && -x "$file" ]] || continue
            name=${file##*/}
            case "$name" in lsblk|gio) continue ;; esac
            [[ ! -e "$rail_box/bin/$name" ]] || continue
            ln -s -- "$file" "$rail_box/bin/$name" || fail "rail: cannot retain required command $name"
        done
    done
    cat > "$rail_box/bin/lsblk" <<'SH'
#!/usr/bin/env bash
set -eu
box=${FLEA_RAIL_BOX:?}
[[ "$box" == /* && -f "$box/.flea-test-sandbox" ]] || exit 90
cat "$box/lsblk.json"
SH
    # Only the two calls this case owns are answered here; the trash count, the mount listing and the
    # trash monitor are the real gio's, because a double that refused them would be testing itself.
    local real_gio
    real_gio=$(command -v gio) || fail 'rail: the box has no gio to stand behind the double'
    cat > "$rail_box/bin/gio" <<SH
#!/usr/bin/env bash
set -eu
box=\${FLEA_RAIL_BOX:?}
[[ "\$box" == /* && -f "\$box/.flea-test-sandbox" ]] || exit 90
jq -cn --args '{args:\$ARGS.positional}' -- "\$@" >> "\$box/calls.jsonl"
case "\$*" in
    "mount -d /dev/sdb1")
        mkdir -p "\$box/mnt/Archive"
        sed -i "s|\[null\],\\"rm\\":false,\\"tran\\":null,\\"size\\":1000398934016|[\\"\$box/mnt/Archive\\"],\\"rm\\":false,\\"tran\\":null,\\"size\\":1000398934016|" "\$box/lsblk.json"
        ;;
    "mount -u \$box/mnt/Archive")
        sleep 0.3
        sed -i "s|\[\\"\$box/mnt/Archive\\"\],\\"rm\\":false,\\"tran\\":null,\\"size\\":1000398934016|[null],\\"rm\\":false,\\"tran\\":null,\\"size\\":1000398934016|" "\$box/lsblk.json"
        ;;
    *) exec REAL_GIO "\$@" ;;
esac
SH
    sed -i "s|REAL_GIO|$real_gio|" "$rail_box/bin/gio" || fail 'rail: the double could not be pointed at the real gio'
    chmod 700 "$rail_box/bin/lsblk" "$rail_box/bin/gio" || fail 'rail: cannot make the doubles executable'
}

# The mounts this case asked for, in order; every other gio call belongs to the trash and the rail.
rail_calls() {
    jq -sc '[.[] | .args | join(" ")] | map(select(startswith("mount -d") or startswith("mount -u")))' "$rail_box/calls.jsonl"
}

rail_entry() {
    ipc deviceEntries | grep -c "^Archive|device|volume|$1$" || true
}

rail_wait_entry() {
    local want="$1" attempt
    for attempt in $(seq 1 200); do
        [[ "$(rail_entry "$want")" == "1" ]] && return
        sleep 0.05
    done
    fail "rail: the Archive row never read mounted=$want, the rail carries $(ipc deviceEntries)"
}

# RailAdditions rules 1 and 2: a volume nothing mounted is a rail row behind its own switch, its menu
# is Mount, and choosing it mounts through gio and opens what came up. The switch off is 0.2.1's rail.
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

# The window this case owns, floated so its width is this case's to set and nobody else's.
sidebar_resize() {
    local width="$1" height="${2:-800}" addr
    addr=$(xwdrag_addr "$(flea_pid)") || fail "sidebar: missing owned window"
    hypr_window_resize "$addr" "$width" "$height" \
        || fail "sidebar: the window would not resize to $width"
    settle
    settle
}

sidebar_state() {
    ipc railState | jq -er "$1"
}

# The state file is written asynchronously, so the choice is waited for rather than read at once.
sidebar_stored() {
    local want="$1" file="$2" attempt
    for attempt in $(seq 1 60); do
        [[ "$(jq -r '.places.rail // "shown"' "$file")" == "$want" ]] && return
        sleep 0.1
    done
    fail "sidebar: the state file never held rail=$want, it holds $(jq -c .places "$file")"
}

sidebar_edge() {
    local wx wy _ww wh
    read -r wx wy _ww wh < <(window_box) || fail "sidebar: native window coordinates unavailable"
    omarchy-drive move "$((wx + 1))" "$((wy + wh / 2))" >/dev/null
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
    settle
}
sidebar_away() {
    local wx wy ww wh
    read -r wx wy ww wh < <(window_box) || fail "sidebar: native window coordinates unavailable"
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
    settle
}
sidebar_wait() {
    local want="$1" attempt
    for attempt in $(seq 1 60); do
        [[ "$(sidebar_state .hidden)" == "$want" ]] && return
        sleep 0.1
    done
    fail "sidebar: the rail never read hidden=$want, its state is $(ipc railState)"
}

# RailAdditions rule 4 (issue 112, muellan): ctrl-b hides the rail and brings it back, the choice
# outlives the window, and a window narrower than Theme.space(640) hides it on its own without
# touching what was remembered.
case_sidebar() (
    local dir="$fixture_root/sidebar" state="$fixture_root/sidebar-state" stored width addr
    sandbox_scratch "$dir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    seed_ui_state "$state" '{"view":"list"}'
    stored="$state/flea/ui.json"

    launch "$dir"
    wait_listing 2
    addr=$(xwdrag_addr "$(flea_pid)") || fail "sidebar: missing owned window"
    hypr_window_float "$addr" "on" || fail 'sidebar: the window would not float'
    settle
    sidebar_resize 1200
    [[ "$(sidebar_state .hidden)" == "false" ]] || fail "sidebar: a fresh home opened with the rail hidden, $(ipc railState)"
    width=$(sidebar_state .width)
    (( width > 0 )) || fail "sidebar: the rail is shown and has no width, $(ipc railState)"
    printf 'SIDEBAR shown=%s\n' "$(ipc railState)"

    echo "-- ctrl-b hides it, and the pane takes its width --"
    key -M ctrl -k b -m ctrl >/dev/null
    sidebar_wait true
    [[ "$(sidebar_state .width)" == "0" ]] || fail "sidebar: the hidden rail still takes $(sidebar_state .width) px"
    [[ "$(ipc railCount)" == "0" ]] || fail "sidebar: the hidden rail still has rows"
    shot sidebar-hidden
    sidebar_stored hidden "$stored"

    echo "-- and it is remembered, because a state is not a setting --"
    kill_flea
    launch "$dir"
    wait_listing 2
    sidebar_wait true
    key -M ctrl -k b -m ctrl >/dev/null
    sidebar_wait false
    sidebar_stored shown "$stored"

    echo "-- directive 74: with auto-hide off a narrow window keeps the rail --"
    addr=$(xwdrag_addr "$(flea_pid)") || fail "sidebar: missing owned window"
    hypr_window_float "$addr" "on" || fail 'sidebar: the window would not float'
    settle
    sidebar_resize 1200
    sidebar_wait false
    sidebar_resize 520
    settle
    [[ "$(sidebar_state .hidden)" == "false" ]] \
        || fail "sidebar: the rail hid itself with auto-hide off, $(ipc railState)"
    sidebar_resize 1200

    echo "-- and the two Places rows are the other handle on the same state --"
    settings_open_key; settle
    settings_section places
    settings_focus_row places.rail
    key -k Space >/dev/null; settle
    sidebar_wait true
    sidebar_stored hidden "$stored"
    key -k Space >/dev/null; settle
    sidebar_wait false
    settings_focus_row places.autoHide
    key -k Space >/dev/null; settle
    settings_wait_value '.places.autoHide == true'
    shot sidebar-settings
    key -k Escape >/dev/null; settle

    echo "-- directive 77: switched on, the rail withdraws and the pane keeps the width --"
    sidebar_wait true
    [[ "$(sidebar_state .inset)" == "0" ]] || fail "sidebar: the withdrawn rail still holds $(ipc railState)"
    [[ "$(jq -r '.places.rail' "$stored")" == "shown" ]] \
        || fail "sidebar: withdrawing wrote the remembered choice, the file holds $(jq -c .places "$stored")"
    sidebar_resize 520
    sidebar_wait true
    sidebar_resize 1200
    sidebar_wait true

    echo "-- and the pointer at the window's own left edge reveals it, over the pane and not beside it --"
    sidebar_edge
    sidebar_wait false
    [[ "$(sidebar_state .inset)" == "0" ]] \
        || fail "sidebar: the revealed rail reflowed the pane, $(ipc railState)"
    [[ "$(sidebar_state '.width > 0')" == "true" ]] || fail "sidebar: the revealed rail has no width, $(ipc railState)"
    shot sidebar-overlay

    echo "-- it withdraws a moment after the pointer leaves --"
    sidebar_away
    sidebar_wait true

    echo "-- Tab reveals it the same way, and the keyboard leaving withdraws it --"
    key -k Tab >/dev/null; settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "sidebar: Tab did not reach the withdrawn rail, it is on $(ipc focusView)"
    sidebar_wait false
    key -k Tab >/dev/null; settle
    [[ "$(ipc focusView)" == "chrome" ]] || fail "sidebar: Tab out of the rail left the keyboard on $(ipc focusView) instead of the bar"
    sidebar_wait true
    key -k Tab >/dev/null; settle
    [[ "$(ipc focusView)" == "list" ]] || fail "sidebar: a further Tab left the keyboard on $(ipc focusView) instead of the list"

    echo "-- and the switch goes off again from its own row --"
    settings_open_key; settle
    settings_section places
    shot sidebar-greyed
    settings_focus_row places.autoHide
    key -k Space >/dev/null; settle
    settings_wait_value '.places.autoHide == false'
    key -k Escape >/dev/null; settle

    echo "-- switched off, Show sidebar and ctrl-b govern again --"
    sidebar_wait false
    [[ "$(sidebar_state '.inset > 0')" == "true" ]] || fail "sidebar: the rail came back as an overlay, $(ipc railState)"
    key -M ctrl -k b -m ctrl >/dev/null
    sidebar_wait true
    key -M ctrl -k b -m ctrl >/dev/null
    sidebar_wait false
    printf 'SIDEBAR autohide=ok remembered=%s\n' "$(jq -r '.places.rail' "$stored")"
    kill_flea
)

# The two in-between eject sentences, "Ejecting ..." while gio is held and "Could not confirm ..." off one unreadable listing.
case_ejectlines() (
    local dir="$fixture_root/ejectlines"
    local gio_log="$dir/gio.log" fixture_home="$fixture_root/ejectlines-home"
    local volume_row refusal seen attempt
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin" "$dir/mnt/FLEASTICK"
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"

    : > "$gio_log"
    # Columns as ui/DeviceMounts.qml asks; while unreadable exists the body is no listing, which Eject.js verdict() reads as unknown.
    cat > "$dir/bin/lsblk" <<EOS
#!/bin/sh
if [ -f "$dir/unreadable" ]; then
  printf 'not a listing\n'
  exit 0
fi
mp='"$dir/mnt/FLEASTICK"'
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G",
"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":"238.5G","type":"part","model":null}]},
{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",
"children":[{"name":"sda1","path":"/dev/sda1","label":"FLEASTICK","mountpoints":[\$mp],"rm":true,"size":"116.1G","type":"part","model":null,"fstype":"vfat"}]}]}
JSON
EOS
    chmod +x "$dir/bin/lsblk"
    # The first mount -e waits on the release flag so the Ejecting line stands for its shot; every call exits 0 as the real tool did.
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
printf '%s\n' "\$*" >> "$gio_log"
if [ "\$1 \$2" = "mount -e" ] && [ -f "$dir/release" ]; then
  : > "$dir/unreadable"
fi
if [ "\$1 \$2" = "mount -e" ] && [ -f "$dir/hold" ] && [ ! -f "$dir/held" ]; then
  : > "$dir/held"
  # 600 polls of 0.05 s bound the held eject's wait.
  hold_polls=600
  n=0
  while [ ! -f "$dir/release" ]; do
    sleep 0.05
    n=\$((n + 1))
    [ "\$n" -lt "\$hold_polls" ] || break
  done
fi
exit 0
EOS
    chmod +x "$dir/bin/gio"

    fixture_home_make "$fixture_home"
    seed_ui_state "$fixture_root/ejectlines-state" '{"places":{"showUnmounted":true}}'
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    wait_listing 5
    for attempt in $(seq 1 100); do
        [[ "$(ipc deviceEntries)" == *"FLEASTICK|device|volume|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"FLEASTICK|device|volume|true"* ]] \
        || fail "ejectlines: the stub volume never appeared live, got $(ipc deviceEntries)"

    volume_row=$(rail_row_of FLEASTICK)

    # Held open: the Ejecting line stands while gio waits on the release flag.
    : > "$dir/hold"
    rm -f "$dir/release"
    click_rail_row "$volume_row" right
    settle
    menu_seek Eject
    key -k Return >/dev/null
    for attempt in $(seq 1 60); do
        [[ "$(grep -c '^mount -e ' "$gio_log")" == "1" ]] && break
        sleep 0.05
    done
    [[ "$(grep -c '^mount -e ' "$gio_log")" == "1" ]] \
        || fail "ejectlines: the held Eject never reached gio, log is: $(cat "$gio_log")"
    wait_message "Ejecting FLEASTICK. Do not unplug it yet."
    printf 'EJECTLINES ejecting message=%q\n' "$(ipc lastMessage)"
    shot eject-ejecting
    : > "$dir/release"

    # Released, gio exits 0 over the still-mounted volume, so the refusal ends the leg.
    refusal="FLEASTICK is still mounted; close what is using it."
    seen=""
    for attempt in $(seq 1 250); do
        seen=$(ipc lastMessage)
        [[ "$seen" == "$refusal" ]] && break
        sleep 0.1
    done
    [[ "$seen" == "$refusal" ]] || fail "ejectlines: the held leg never refused, last said: $seen"

    # Unreadable: every listing after gio exits is no listing, so the 15 s verdict wait ends in the unconfirmed sentence.
    volume_row=$(rail_row_of FLEASTICK)
    click_rail_row "$volume_row" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "ejectlines: right click opened no menu on the volume"
    menu_seek Eject
    key -k Return >/dev/null
    for attempt in $(seq 1 60); do
        [[ "$(grep -c '^mount -e ' "$gio_log")" == "2" ]] && break
        sleep 0.05
    done
    [[ "$(grep -c '^mount -e ' "$gio_log")" == "2" ]] \
        || fail "ejectlines: the second Eject never reached gio, log is: $(cat "$gio_log")"
    wait_message "Could not confirm FLEASTICK was ejected; do not unplug it yet."
    printf 'EJECTLINES unconfirmed message=%q\n' "$(ipc lastMessage)"
    shot eject-unconfirmed
    rm -f "$dir/unreadable"

    if grep -qE '(^| )(-f|--force)( |$)' "$gio_log"; then
        fail "ejectlines: a forced unmount reached gio: $(cat "$gio_log")"
    fi
    if grep -q '^mount -d' "$gio_log"; then
        fail "ejectlines: an eject went through --device, which glib dispatches before --eject"
    fi
    [[ "$(grep -c '^mount -e ' "$gio_log")" == "2" ]] \
        || fail "ejectlines: expected exactly two ejects, log is: $(cat "$gio_log")"

    printf 'EJECTLINES ejecting=ok unconfirmed=ok no-force=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
)

# A locked LUKS container's row menu is Unlock alone; the lsblk shape is tests/ui-captures-unlock.sh's locked device.
case_rail_locked() (
    local rail_box="$fixture_root/rail-locked" rail_dir="$fixture_root/rail-locked-home"
    local label="" index attempt
    sandbox_scratch "$rail_box"
    mkdir -p "$rail_box/bin" || fail 'rail-locked: private bin failed'
    cat > "$rail_box/bin/lsblk" <<'EOS'
#!/bin/sh
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G",
"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":"238.5G","type":"part","model":null,"fstype":"btrfs"}]},
{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"size":"2000398934016","type":"disk","model":"Samsung SSD 870","fstype":null,
"children":[{"name":"sdb1","path":"/dev/sdb1","label":"VAULT","mountpoints":[null],"rm":false,"size":1000398934016,"type":"part","model":null,"fstype":"crypto_LUKS","fsver":"2"}]}]}
JSON
EOS
    chmod +x "$rail_box/bin/lsblk" || fail 'rail-locked: cannot make the lsblk double executable'
    fixture_home_make "$rail_dir"
    seed_ui_state "$fixture_root/rail-locked-state" '{"places":{"showUnmounted":true}}'
    export PATH="$rail_box/bin:$PATH"
    export HOME="$rail_dir"
    launch "$rail_dir"
    for attempt in $(seq 1 200); do
        label=$(ipc railEntries 2>/dev/null | jq -r '[.[] | select(.device == "/dev/sdb1" and .locked == true)][0].label // ""' 2>/dev/null || printf "")
        [[ -n "$label" ]] && break
        sleep 0.05
    done
    [[ -n "$label" ]] || fail "rail-locked: no locked rail row for /dev/sdb1, entries $(ipc railEntries)"
    index=$(rail_row_of "$label")
    click_rail_row "$index" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail 'rail-locked: the locked row opened no menu'
    [[ "$(ipc contextMenuEntries)" == "Unlock" ]] \
        || fail "rail-locked: the locked row offers $(ipc contextMenuEntries), not Unlock alone"
    [[ "$(ipc contextMenuGlyphs)" == "lock" ]] \
        || fail "rail-locked: the locked row draws $(ipc contextMenuGlyphs), not the lock glyph"
    shot rail-locked-menu
    key -k Escape >/dev/null
    settle
    printf 'RAIL-LOCKED label=%s menu=ok\n' "$label"
    kill_flea
)

# The mark hover, the kernel network rows with their menus and the composite rail, from fed mountinfo lines; nothing is mounted.
case_kernelrows() (
    local dir="$fixture_root/kernelrows"
    local gio_log="$dir/gio.log" fixture_home="$fixture_root/kernelrows-home"
    local attempt idx centre dets dxdy dx dy cx cy wx wy ww wh feed feed_result nets devs label hovered
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin" "$dir/mnt/FLEASTICK" "$dir/mnt/BACKUP" "$dir/mnt/nas" "$dir/mnt/nfs"
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"

    : > "$gio_log"
    # The device stub of case_eject plus a second marked removable drive and one unmounted drive.
    cat > "$dir/bin/lsblk" <<EOS
#!/bin/sh
mp='"$dir/mnt/FLEASTICK"'
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G",
"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":"238.5G","type":"part","model":null}]},
{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",
"children":[{"name":"sda1","path":"/dev/sda1","label":"FLEASTICK","mountpoints":[\$mp],"rm":true,"size":"116.1G","type":"part","model":null,"fstype":"vfat"},
{"name":"sda2","path":"/dev/sda2","label":"SPARE","mountpoints":[null],"rm":true,"size":"8.0G","type":"part","model":null,"fstype":"vfat"}]},
{"name":"sdc","path":"/dev/sdc","label":null,"mountpoints":[null],"rm":true,"size":"64.0G","type":"disk","model":"Backup Disk",
"children":[{"name":"sdc1","path":"/dev/sdc1","label":"BACKUP","mountpoints":["$dir/mnt/BACKUP"],"rm":true,"size":"64.0G","type":"part","model":null,"fstype":"vfat"}]}]}
JSON
EOS
    chmod +x "$dir/bin/lsblk"
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
printf '%s\n' "\$*" >> "$gio_log"
exit 0
EOS
    chmod +x "$dir/bin/gio"

    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/gdrive"
    seed_ui_state "$fixture_root/kernelrows-state" '{"places":{"showUnmounted":true}}'
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    wait_listing 5
    for attempt in $(seq 1 100); do
        devs=$(ipc deviceEntries)
        [[ "$devs" == *"FLEASTICK|device|volume|true"* && "$devs" == *"BACKUP|device|volume|true"* && "$devs" == *"SPARE|device|volume|false"* ]] && break
        sleep 0.05
    done
    devs=$(ipc deviceEntries)
    [[ "$devs" == *"FLEASTICK|device|volume|true"* && "$devs" == *"BACKUP|device|volume|true"* && "$devs" == *"SPARE|device|volume|false"* ]] \
        || fail "kernelrows: the stub devices never appeared live, got $devs"
    for attempt in $(seq 1 200); do
        [[ "$(ipc networkBuilt)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkBuilt)" == "true" ]] || fail "kernelrows: the network group never built"

    # The pointer glides to the slot centre railDetails gives, and railMarkHovered must read true before the shot.
    command -v ydotool >/dev/null || fail "kernelrows: no ydotool on PATH, so the mark cannot be hovered"
    [[ -S "${XDG_RUNTIME_DIR:-}/.ydotool_socket" ]] || fail "kernelrows: no ydotool socket, so the mark cannot be hovered"
    ipc railEntries | jq -e '[.[] | select(.label == "FLEASTICK" and .group == "device" and .kind == "volume" and .mounted == true and .releasable == true)] | length == 1' >/dev/null \
        || fail "kernelrows: FLEASTICK is not a mounted releasable volume, entries $(ipc railEntries | jq -c 'map({label,mounted,releasable})')"
    idx=$(rail_row_of FLEASTICK)
    centre=$(ipc railRowCentre "$idx")
    [[ -n "$centre" ]] || fail "kernelrows: rail row $idx has no on-screen centre"
    dets=$(ipc railDetails)
    read -r cx cy <<< "$centre"
    dxdy=$(jq -r --argjson i "$idx" '.rows[$i] | "\((.indicatorRect.x + .indicatorRect.width / 2) - (.rect.x + .rect.width / 2)) \(((.indicatorRect.y + .indicatorRect.height / 2) - (.rect.y + .rect.height / 2)))"' <<< "$dets") \
        || fail "kernelrows: railDetails names no indicator slot for row $idx"
    read -r dx dy <<< "$dxdy"
    dx=$(printf '%.0f' "$dx")
    dy=$(printf '%.0f' "$dy")
    read -r wx wy ww wh < <(window_box) || fail "kernelrows: native window coordinates unavailable"
    omarchy-drive move "$((wx + cx + dx))" "$((wy + cy + dy))" >/dev/null \
        || fail "kernelrows: the pointer move onto the mark failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1 \
        || fail "kernelrows: the pointer nudge onto the mark failed"
    settle
    hovered=""
    for attempt in $(seq 1 50); do
        hovered=$(ipc railMarkHovered "$idx")
        [[ "$hovered" == "true" ]] && break
        sleep 0.1
    done
    [[ "$hovered" == "true" ]] \
        || fail "kernelrows: the mark never took its hover state, railMarkHovered said $hovered"
    shot rail-eject-hover

    # Three mountinfo lines, cifs at a sandbox path, nfs4 and fuse.rclone under the fixture home, each a NETWORK row with Unmount.
    feed="30 30 0:30 / $dir/mnt/nas rw,nosuid,nodev - cifs //nas/share rw
31 31 0:31 / $dir/mnt/nfs rw,nosuid,nodev - nfs4 nas:/export rw
32 32 0:32 / $fixture_home/gdrive rw,nosuid,nodev - fuse.rclone remote: rw"
    feed_result=$(ipc networkFeedMountinfo "$feed")
    [[ "$feed_result" == "fed" ]] \
        || fail "kernelrows: the mountinfo feed said $feed_result"
    for attempt in $(seq 1 100); do
        nets=$(ipc networkEntries)
        [[ "$nets" == *"nas|network|share|true"* && "$nets" == *"nfs|network|share|true"* && "$nets" == *"gdrive|network|share|true"* ]] && break
        sleep 0.05
    done
    nets=$(ipc networkEntries)
    [[ "$nets" == *"nas|network|share|true"* ]] || fail "kernelrows: no cifs row, network carries $nets"
    [[ "$nets" == *"nfs|network|share|true"* ]] || fail "kernelrows: no nfs row, network carries $nets"
    [[ "$nets" == *"gdrive|network|share|true"* ]] || fail "kernelrows: no rclone row, network carries $nets"
    shot rail-kernel-rows

    for label in nas nfs gdrive; do
        idx=$(rail_row_of "$label")
        click_rail_row "$idx" right
        settle
        [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "kernelrows: $label opened no menu"
        [[ "$(ipc contextMenuEntries)" == "Open|-|Unmount" ]] \
            || fail "kernelrows: $label offers $(ipc contextMenuEntries), not Open then Unmount"
        [[ "$(ipc contextMenuGlyphs)" == "folder-open|-|drive" ]] \
            || fail "kernelrows: $label draws $(ipc contextMenuGlyphs)"
        shot "rail-kernel-$label-menu"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "kernelrows: Escape left the $label menu open"
    done

    # Composite: the SMB share with its mark, the NFS share with its square, two marked drives and one unmounted drive.
    nets=$(ipc networkEntries)
    devs=$(ipc deviceEntries)
    [[ "$nets" == *"nas|network|share|true"* && "$nets" == *"nfs|network|share|true"* && "$nets" == *"gdrive|network|share|true"* ]] \
        || fail "kernelrows: the composite lost a network row: $nets"
    [[ "$devs" == *"FLEASTICK|device|volume|true"* && "$devs" == *"BACKUP|device|volume|true"* && "$devs" == *"SPARE|device|volume|false"* ]] \
        || fail "kernelrows: the composite lost a device row: $devs"
    dets=$(ipc railDetails)
    for label in nas nfs gdrive FLEASTICK BACKUP SPARE; do
        [[ "$(jq -r --arg l "$label" '[.rows[] | select(.label == $l and .indicatorVisible == true)] | length' <<< "$dets")" == "1" ]] \
            || fail "kernelrows: the composite lost the mark slot on $label"
    done
    shot rail-composite

    printf 'KERNELROWS hover=ok cifs=ok nfs=ok rclone=ok composite=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
)
