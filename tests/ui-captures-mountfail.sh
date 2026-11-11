# Mount-failure board capture, by name only; reuses cap_unlock_lsblk_shim, which lists a loop device as a removable stick.

# The scope polkit trusts: an ssh-started process is an inactive subject, so loop-setup, unmount and loop-delete run in the graphical slice like Flea.
cap_mountfail_wrap=(systemd-run --user --scope --quiet --slice=app-graphical.slice)

# The manager is on the real user bus; only its child keeps the fixture's isolated bus and runtime.
cap_mountfail_scope() {
    local runtime="/run/user/$(id -u)"
    local -a session=("--setenv=XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR")
    [[ -z "${DBUS_SESSION_BUS_ADDRESS-}" ]] || session+=("--setenv=DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS")
    env XDG_RUNTIME_DIR="$runtime" DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime/bus" \
        "${cap_mountfail_wrap[@]}" "${session[@]}" "$@"
}

# Builds the 64 MiB image at a new absolute IMG and prints its filesystem (btrfs, else ext4); blkid drops a bad checksum, so only what the kernel alone reads is broken.
cap_mountfail_image() {
    local img="$1" fs="" seen=""
    local PATH="$PATH:/usr/sbin:/sbin"
    [[ "$img" == /* && ! -e "$img" && ! -L "$img" ]] || { printf 'cap_mountfail_image: %s is not a new absolute path\n' "$img" >&2; return 1; }
    command -v blkid >/dev/null || { printf 'cap_mountfail_image: blkid is not installed\n' >&2; return 1; }
    truncate -s 64M "$img" || return 1
    if command -v mkfs.btrfs >/dev/null; then
        fs=btrfs
        # 64 MiB is under btrfs's default minimum, so the mixed layout (-M) is what fits.
        mkfs.btrfs -q -M -L 'DATA #2' "$img" >/dev/null 2>&1 || { printf 'cap_mountfail_image: mkfs.btrfs failed\n' >&2; return 1; }
        command -v python3 >/dev/null || { printf 'cap_mountfail_image: python3 is not installed, the superblock checksum cannot be redone\n' >&2; return 1; }
        # Sample superblock at 64 KiB: csum[32] fsid[16] bytenr[8] flags[8] magic "_BHRfS_M"; the csum is the CRC32C of bytes 32..4096.
        python3 -I - "$img" <<'PY' || return 1
import sys
with open(sys.argv[1], 'r+b') as f:
    f.seek(65536)
    sb = bytearray(f.read(4096))
    if sb[64:72] != b'_BHRfS_M':
        sys.exit('cap_mountfail_image: no btrfs superblock at 64 KiB')
    sb[48:56] = bytes(8)
    crc = 0xFFFFFFFF
    for byte in sb[32:]:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ (0x82F63B78 if crc & 1 else 0)
    sb[0:4] = (crc ^ 0xFFFFFFFF).to_bytes(4, 'little')
    f.seek(65536)
    f.write(sb)
PY
    else
        fs=ext4
        mkfs.ext4 -q -F -O ^metadata_csum -L 'DATA #2' "$img" >/dev/null 2>&1 || { printf 'cap_mountfail_image: mkfs.ext4 failed\n' >&2; return 1; }
        # The 56 bytes of geometry before s_magic at 1024; the magic, features and label stay, so the type still reads as ext4.
        dd if=/dev/zero of="$img" bs=1 count=56 seek=1024 conv=notrunc status=none || return 1
    fi
    seen=$(blkid -p -o value -s TYPE "$img" 2>/dev/null) || seen=""
    [[ "$seen" == "$fs" ]] || { printf 'cap_mountfail_image: blkid reads [%s] on the corrupted image, not %s\n' "$seen" "$fs" >&2; return 1; }
    printf '%s' "$fs"
}

# Attaches IMG read-only through udisks and prints its /dev/loopN.
cap_mountfail_attach() {
    local img="$1" out="" dev=""
    out=$(cap_mountfail_scope udisksctl loop-setup --no-user-interaction -r -f "$img" 2>&1) || { printf 'cap_mountfail_attach: loop-setup refused: %s\n' "$out" >&2; return 1; }
    # Sample input: "Mapped file /home/flea-sandbox/x/disk.img as /dev/loop3."
    dev=$(sed -n 's/.* as \(\/dev\/loop[0-9][0-9]*\)\.$/\1/p' <<< "$out" | head -n 1)
    [[ "$dev" =~ ^/dev/loop[0-9]+$ ]] || { printf 'cap_mountfail_attach: no loop device in [%s]\n' "$out" >&2; return 1; }
    printf '%s' "$dev"
}

# Every exit path: unmount if a bug mounted it, detach the loop device, remove the scratch root (the guard checks its marker).
cap_mountfail_cleanup() {
    local dev="$1" box="$2"
    if [[ "$dev" =~ ^/dev/loop[0-9]+$ ]]; then
        cap_mountfail_scope udisksctl unmount --no-user-interaction -b "$dev" >/dev/null 2>&1 || true
        cap_mountfail_scope udisksctl loop-delete --no-user-interaction -b "$dev" >/dev/null 2>&1 \
            || printf 'CAP NOTE mountfail: %s was not detached; run udisksctl loop-delete -b %s\n' "$dev" "$dev"
    fi
    sandbox_remove "$box"
}

# Sets cap_mountfail_at to the rail index of the volume with this device path, waited on the lsblk poll; no substitution, so its fail reaches the log.
cap_mountfail_row() {
    local dev="$1" index="" end=$((SECONDS + 30))
    while (( SECONDS < end )); do
        index=$(ipc railEntries 2>/dev/null | jq -r --arg dev "$dev" '[to_entries[] | select(.value.device == $dev) | .key][0] // ""' 2>/dev/null || printf "")
        [[ -n "$index" ]] && { cap_mountfail_at="$index"; return 0; }
        sleep 0.2
    done
    fail "cap_mountfail: no rail row for $dev, entries $(ipc railEntries | jq -c 'map({label,device,fstype,mounted})')"
}

# cap_resize returns on size alone while the centring move may still run, so no shot follows one until the box holds still.
cap_mountfail_still() {
    local width="$1" height="$2" box="" previous="" end=$((SECONDS + 20))
    while (( SECONDS < end )); do
        box=$(window_box) || fail "cap_mountfail: native window coordinates unavailable"
        [[ "$box" == "$previous" ]] && break
        previous="$box"
        sleep 0.2
    done
    [[ "$box" == "$previous" && "$box" == *" $width $height" ]] || fail "cap_mountfail: the window never held still at ${width}x${height}, last box [$box]"
}

# Mount failure: a corrupted volume clicked in DEVICES leaves the pane on its folder and says why.
case_cap_mountfail() {
    local dir="$fixture_root/cap-mountfail" box="$fixture_root/cap-mountfail-box" img="" fs="" dev="" shim="" index="" label="DATA #2"
    local headline="" detail="" end="" row=""
    local -a flea_launch_wrap=("${cap_mountfail_wrap[@]}")
    command -v udisksctl >/dev/null || fail "cap_mountfail: udisksctl is not on PATH, so the image cannot be attached"
    sandbox_scratch "$dir"
    sandbox_scratch "$box"
    img="$box/disk.img"
    fs=$(cap_mountfail_image "$img") || fail "cap_mountfail: the corrupted image could not be built"
    printf 'CAP NOTE mountfail: %s image, superblock rejected by the kernel and still read as %s by blkid\n' "$fs" "$fs"
    # The trap is armed before the attach, so a loop device is never left behind; the paths are expanded now because it runs after this function's locals are gone.
    trap "cap_mountfail_cleanup '' ${box@Q}" EXIT
    dev=$(cap_mountfail_attach "$img") || fail "cap_mountfail: the image could not be attached"
    trap "cap_mountfail_cleanup ${dev@Q} ${box@Q}" EXIT
    shim="$fixture_root/cap-mountfail-shim"
    sandbox_scratch "$shim"
    cap_unlock_lsblk_shim "$shim" "$dev" "$(command -v lsblk)" || fail "cap_mountfail: the lsblk shim for $dev could not be written"
    printf 'CAP NOTE mountfail: %s attached read-only, lsblk shim lists it as cap-%s\n' "$dev" "${dev#/dev/}"
    printf 'one\n' > "$dir/a.txt"
    printf 'two\n' > "$dir/b.txt"
    seed_ui_state "$fixture_root/cap-mountfail-state" '{"keys":"default","view":"list"}'
    PATH="$shim:$PATH" launch "$dir"
    wait_listing 2
    cap_resize 880 700
    cap_mountfail_still 880 700
    cap_mountfail_row "$dev"
    index="$cap_mountfail_at"
    row=$(ipc railEntries | jq -c --argjson at "$index" '.[$at] | {label,device,fstype,mounted}')
    ipc railEntries | jq -e --argjson at "$index" --arg label "$label" --arg fs "$fs" '.[$at] | .label == $label and .fstype == $fs and .mounted == false' >/dev/null \
        || fail "cap_mountfail: the rail row is $row, not an unmounted $fs volume named $label"
    click_rail_row "$index" left
    end=$((SECONDS + 40))
    while (( SECONDS < end )); do
        [[ "$(ipc statusError 2>/dev/null || printf false)" == "true" ]] && break
        [[ "$(ipc path 2>/dev/null || printf '%s' "$dir")" == "$dir" ]] || break
        sleep 0.1
    done
    [[ "$(ipc path)" == "$dir" ]] || fail "cap_mountfail: the pane moved to $(ipc path) off $dir, so a corrupted volume opened"
    [[ "$(ipc statusError)" == "true" ]] || fail "cap_mountfail: no error showed after the click, status [$(ipc statusPrimary)]"
    [[ "$(ipc total)" == "2" ]] || fail "cap_mountfail: the pane lost its listing, total $(ipc total)"
    headline=$(ipc statusPrimary)
    [[ "$headline" == *"$label"* && "$headline" == *"mount"* ]] || fail "cap_mountfail: the headline reads [$headline], not one naming $label and the failed mount"
    detail=$(ipc statusDetail)
    printf 'CAP NOTE mountfail: headline=[%s] detail=[%s]\n' "$headline" "$detail"
    settle
    shot mountfail-880
    cap_resize 500 700
    cap_mountfail_still 500 700
    [[ "$(ipc path)" == "$dir" && "$(ipc statusError)" == "true" ]] || fail "cap_mountfail: the narrow window lost the folder or the error"
    settle
    shot mountfail-500
    # Pinned for the MOUNTFAIL build: gio's own reason is the detail line. Red on stage4, whose message carries no detail, after both shots are kept.
    [[ "$detail" == *"superblock"* ]] || fail "cap_mountfail: the detail line is [$detail], not gio's reason (wrong fs type, bad superblock); expected red before MOUNTFAIL"
    printf 'CAP_MOUNTFAIL fs=%s headline=%q detail=%q shots=880,500\n' "$fs" "$headline" "$detail"
    kill_flea
    cap_mountfail_cleanup "$dev" "$box"
    trap - EXIT
}
