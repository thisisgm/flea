# Issue 370 on a real rclone mount, by name only: extract, compress and convert by keyboard and pointer; 100 kB/s uploads keep staged files in flight.
cap_rclone_unit=""
cap_rclone_job_polls=240 cap_rclone_dialog_polls=120 cap_rclone_mount_polls=100 cap_rclone_poll_s=0.25 cap_rclone_quiet_s=6
cap_rclone_cleanup() {
    local mnt="$1"
    mountpoint -q "$mnt" && fusermount3 -u "$mnt" >/dev/null 2>&1
    [[ -n "$cap_rclone_unit" ]] && systemctl --user stop "$cap_rclone_unit" >/dev/null 2>&1
    return 0
}
# One menu row by action, through the keyboard (Return) or the pointer (a click on the row).
cap_rclone_menu() {
    local name="$1" action="$2" input="$3"
    menus_file_menu "$name" "$([[ "$input" == key ]] && printf key || printf pointer)"
    settle
    menus_choose "$action" "$([[ "$input" == key ]] && printf key || printf pointer)"
    settle
}
# Compress's flyout row for .zip, through the keyboard (Right, Down, Return) or the pointer.
cap_rclone_compress() {
    local name="$1" input="$2" index
    menus_file_menu "$name" "$([[ "$input" == key ]] && printf key || printf pointer)"
    settle
    menus_seek compress
    if [[ "$input" == key ]]; then key -k Right >/dev/null; else menus_point "$(ipc contextMenuRowCentre "$(ipc contextMenuCursor)")"; fi
    settle
    menus_expect menuState '.submenu and ([.submenuEntries[] | select(.label == ".zip")] | length) == 1' "the Compress flyout offers .zip"
    if [[ "$input" == key ]]; then
        menu_seek_submenu .zip
        key -k Return >/dev/null
    else
        index=$(ipc menuState | jq -er '[.submenuEntries[] | .label] | index(".zip")') || fail "cap_rclone: no .zip row"
        menus_point "$(ipc contextMenuSubmenuRowCentre "$index")"
    fi
    settle
}
# Sample output: {"notice":"Archive written.","running":0}
cap_rclone_job_state() {
    local state
    # jq -e exits non-zero when the reply is not a lane, so a failed or partial read never passes for a state.
    state=$(ipc statusActivityState | jq -ce 'select((.activities | type) == "array" and (.notice | type) == "string") | {notice, running: ([.activities[] | select(.running)] | length)}') || return 1
    printf '%s\n' "$state"
}
cap_rclone_convert() {
    local name="$1" input="$2" before now quiet='{"notice":"","running":0}'
    cap_rclone_menu "$name" convert "$input"
    for _ in $(seq 1 "$cap_rclone_dialog_polls"); do [[ "$(ipc convertOpen)" == true ]] && break; sleep "$cap_rclone_poll_s"; done
    [[ "$(ipc convertOpen)" == true ]] || fail "cap_rclone: Convert over $name by $input never opened"
    # A png opens on jpg; GPUI reads out no centre for the popup's Convert button, so Return commits it in both arms.
    [[ "$(ipc convertFormat)" == jpg ]] || fail "cap_rclone: Convert over $name opened on $(ipc convertFormat), not jpg"
    before=$(cap_rclone_job_state) || fail "cap_rclone: the status lane could not be read before Convert over $name"
    for _ in $(seq 1 "$cap_rclone_dialog_polls"); do
        [[ "$(ipc convertOpen)" == false ]] && return 0
        now=$(cap_rclone_job_state) || fail "cap_rclone: the status lane could not be read during Convert over $name"
        # The card drops Return while it still probes its output name, so it is sent again until a conversion shows.
        [[ "$now" == "$before" || "$now" == "$quiet" ]] && key -k Return >/dev/null
        sleep "$cap_rclone_poll_s"
    done
    fail "cap_rclone: Convert over $name by $input never closed"
}
# A job has ended once nothing runs and its output shows on the mount or a new error is queued.
cap_rclone_idle() {
    local label="$1" output="$2" before="$3" state="" running=1 errors="$3" landed=0
    for _ in $(seq 1 "$cap_rclone_job_polls"); do
        state=$(ipc statusActivityState)
        running=$(jq -r '[.activities[] | select(.running)] | length' <<< "$state")
        errors=$(jq -r .errors <<< "$state")
        compgen -G "$mnt/$output" >/dev/null && landed=1
        [[ "$running" == 0 ]] && { [[ "$landed" == 1 ]] || (( errors > before )); } && break
        sleep "$cap_rclone_poll_s"
    done
    printf 'CAP rclone %s activity=%s\n' "$label" "$(jq -c '{errors, notice, activities: [.activities[] | {text, running}]}' <<< "$state")"
    [[ "$running" == 0 ]] || fail "cap_rclone: $label still running after $cap_rclone_job_polls polls"
    (( errors <= before )) || fail "cap_rclone: $label ended in an error: $(jq -r .notice <<< "$state")"
    [[ "$landed" == 1 ]] || fail "cap_rclone: $label wrote no $output on the mount"
}
# Runs one drive command, then judges its job against the error count from before it started.
cap_rclone_step() {
    local label="$1" output="$2" before
    shift 2
    before=$(ipc statusActivityState | jq -r .errors)
    "$@"
    cap_rclone_idle "$label" "$output" "$before"
}
case_cap_archives_rclone() {
    local back="$fixture_root/cap-rclone-back" mnt="$fixture_root/cap-rclone-mnt" cache="$fixture_root/cap-rclone-cache"
    local src="$fixture_root/cap-rclone-src" log="$fixture_root/cap-rclone.log" out all left missing=""
    local member_bytes=150000 blob_bytes=200000 upload_cap=100k drain_s=150 work_prefix=.flea-work- menus_checks=0
    command -v rclone >/dev/null && command -v fusermount3 >/dev/null || { printf 'CAP SKIP cap-archives-rclone: rclone or fusermount3 is not installed\n'; return 0; }
    command -v bsdtar >/dev/null && command -v magick >/dev/null || { printf 'CAP SKIP cap-archives-rclone: bsdtar or magick is not installed\n'; return 0; }
    sandbox_scratch "$back"; sandbox_scratch "$mnt"; sandbox_scratch "$cache"; sandbox_scratch "$src"
    mkdir -p "$src/flea-teszt-370/almappa"
    for f in flea-teszt-370/szamok.csv flea-teszt-370/olvass-el.txt flea-teszt-370/almappa/note.txt; do
        head -c "$member_bytes" /dev/urandom > "$src/$f"
    done
    bsdtar -a -cf "$back/mouse370.zip" -C "$src" flea-teszt-370 || fail "cap_rclone: no zip fixture"
    bsdtar -a -cf "$back/keys370.zip" -C "$src" flea-teszt-370 || fail "cap_rclone: no zip fixture"
    head -c "$blob_bytes" /dev/urandom > "$back/mouse.bin"
    head -c "$blob_bytes" /dev/urandom > "$back/keys.bin"
    magick -size 300x300 plasma:fractal "$back/mouse.png" && magick -size 300x300 plasma:fractal "$back/keys.png" || fail "cap_rclone: no image fixture"
    : > "$fixture_root/cap-rclone.conf"
    cap_rclone_unit="flea-uish-rclone-$$"
    trap "cap_rclone_cleanup ${mnt@Q}" EXIT
    systemd-run --user --quiet --unit="$cap_rclone_unit" --collect rclone mount ":local:$back" "$mnt" \
        --config "$fixture_root/cap-rclone.conf" --vfs-cache-mode full --cache-dir "$cache" --vfs-write-back 0s --bwlimit "$upload_cap" \
        --log-file "$log" --log-level INFO || fail "cap_rclone: rclone mount did not start"
    for _ in $(seq 1 "$cap_rclone_mount_polls"); do mountpoint -q "$mnt" && break; sleep "$cap_rclone_poll_s"; done
    grep -q " $mnt .* - fuse.rclone " /proc/self/mountinfo || fail "cap_rclone: $mnt is not a fuse.rclone mount"
    seed_ui_state "$fixture_root/cap-rclone-state" '{"keys":"default","view":"list"}'
    launch "$mnt"
    wait_listing 6
    cap_resize 1100 700
    cap_rclone_step "extract by pointer" mouse370/flea-teszt-370/olvass-el.txt cap_rclone_menu mouse370.zip extract pointer
    cap_rclone_step "extract by keyboard" keys370/flea-teszt-370/olvass-el.txt cap_rclone_menu keys370.zip extract key
    shot archives-rclone-extracted
    cap_rclone_step "compress by pointer" mouse.zip cap_rclone_compress mouse.bin pointer
    cap_rclone_step "compress by keyboard" keys.zip cap_rclone_compress keys.bin key
    cap_rclone_step "convert by pointer" 'mouse*.jpg' cap_rclone_convert mouse.png pointer
    cap_rclone_step "convert by keyboard" 'keys*.jpg' cap_rclone_convert keys.png key
    settle
    shot archives-rclone-done
    printf 'CAP rclone mount=%s\n' "$(ls -A "$mnt" | tr '\n' '|')"
    kill_flea
    # The remote is read once every output has uploaded or drain_s passed, so a missing one is what the drive really lacks.
    for _ in $(seq 1 "$drain_s"); do
        all=1
        for out in mouse370/flea-teszt-370/olvass-el.txt keys370/flea-teszt-370/olvass-el.txt mouse.zip keys.zip 'mouse*.jpg' 'keys*.jpg'; do
            compgen -G "$back/$out" >/dev/null || all=0
        done
        [[ "$all" == 1 ]] && break
        sleep 1
    done
    sleep "$cap_rclone_quiet_s"
    printf 'CAP rclone remote=%s\n' "$(ls -A "$back" | tr '\n' '|')"
    { grep -E 'ERROR' "$log" || true; } | cut -c1-200 | head -8 | sed 's/^/CAP rclone log: /'
    for out in mouse370 keys370; do
        for f in flea-teszt-370/szamok.csv flea-teszt-370/olvass-el.txt flea-teszt-370/almappa/note.txt; do
            cmp -s "$src/$f" "$back/$out/$f" || missing+=" $out/$f"
        done
    done
    for out in mouse.zip keys.zip 'mouse*.jpg' 'keys*.jpg'; do
        all=$(compgen -G "$back/$out" | head -1 || true)
        [[ -n "$all" && -s "$all" ]] || missing+=" $out"
    done
    left=$(find "$back" -name "${work_prefix}*" | sed "s|^$back/||" | tr '\n' ' ')
    cap_rclone_cleanup "$mnt"
    trap - EXIT
    [[ -z "$missing" ]] || fail "cap_rclone: the remote lacks or differs on:$missing"
    [[ -z "$left" ]] || fail "cap_rclone: Flea work folders stayed on the remote: $left"
    printf 'CAP rclone remote whole, no work folder left\n'
}
