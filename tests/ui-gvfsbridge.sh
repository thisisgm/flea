# Quickshell binds its IPC socket under XDG_RUNTIME_DIR and reaches Hyprland's there, and a unix socket path has a fixed byte limit.
gvfsbridge_runtime_fits() {
    # sun_path holds 108 bytes with its NUL, so a socket path holds 107.
    local socket sun_path_max=107 signature="${HYPRLAND_INSTANCE_SIGNATURE:-}"
    # Sample input: /tmp/fgb.Zq3x9a; instance ids have run to 11 characters, so the sample id has 16, and a Hyprland signature has 62.
    (( ${#signature} >= 62 )) || signature=$(printf '%062d' 0)
    for socket in "$1/quickshell/by-id/0123456789abcdef/ipc.sock" "$1/${WAYLAND_DISPLAY:-wayland-1}" "$1/hypr/$signature/.socket2.sock"; do
        (( ${#socket} <= sun_path_max )) || fail "gvfsbridge: the socket path $socket is ${#socket} bytes, past the $sun_path_max a unix socket path holds, so the runtime directory needs a shorter root"
    done
}

# Only Flea takes the fixture's HOME and bridge stub (and under qml the runtime dir; GPUI's owned transport gives its own), so kill_flea, hyprctl and omarchy-drive keep the operator's.
gvfsbridge_launch() {
    local dir="$1" fixture_home="$2" runtime="$3" fuse="$4"
    local -a flea_launch_wrap=(env "HOME=$fixture_home" "PATH=$dir/bin:$PATH" "FLEA_GVFS_FUSE=$fuse")
    [[ "$test_frontend" != qml ]] || flea_launch_wrap+=("XDG_RUNTIME_DIR=$runtime")
    launch "$dir"
    gvfsbridge_runtime=$runtime
    if [[ "$test_frontend" == gpui ]]; then
        gvfsbridge_runtime=$(jq -er .runtime "$FLEA_TEST_WINDOW/window.json") || fail "gvfsbridge: owned runtime is unavailable"
    fi
}

# Removes the case's runtime root, whose entries link into the operator's, and the quickshell dir the case made there.
gvfsbridge_drop() {
    local runtime="$1" made="${2:-}"
    [[ -z "$made" ]] || rmdir -- "$made" 2>/dev/null || true
    # rm does not follow the links into the operator's runtime dir.
    [[ "$runtime" == /tmp/fgb.?* && -f "$runtime/.flea-test-sandbox" ]] && rm -rf -- "$runtime"
    return 0
}

# The GVFS bridge's Starting and failure lines, staged through the FLEA_GVFS_FUSE seam; runs only by name (ui:gvfsbridge).
case_gvfsbridge() {
    local dir="$fixture_root/gvfsbridge"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/one.txt"
    local fixture_home="$fixture_root/gvfsbridge-home"
    fixture_home_make "$fixture_home"
    local state="$fixture_root/gvfsbridge-state"
    # A root of its own, since the run root's /tmp name leaves Hyprland's socket path past the limit; the trap removes it on a failure too.
    local runtime
    runtime=$(mktemp -d /tmp/fgb.XXXXXX) || fail "gvfsbridge: cannot create a runtime directory under /tmp"
    : > "$runtime/.flea-test-sandbox"
    trap "gvfsbridge_drop ${runtime@Q}" EXIT
    gvfsbridge_runtime_fits "$runtime"
    mkdir "$runtime/gvfs"
    local leaf_slow="smb-share:server=bridge-slow.test,share=data"
    local leaf_fail="smb-share:server=bridge-fail.test,share=data"

    # Under qml every runtime entry but gvfs and flea-ipc is linked through, so the sockets resolve and the bridge dir starts unserved.
    local real_runtime="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    if [[ "$test_frontend" == qml && -d "$real_runtime" ]]; then
        # Quickshell registers the instance under the runtime dir, so the restored client only sees it through this link; a directory made here goes again at exit.
        [[ -d "$real_runtime/quickshell" ]] || trap "gvfsbridge_drop ${runtime@Q} ${real_runtime@Q}/quickshell" EXIT
        mkdir -p "$real_runtime/quickshell"
        local entry base
        shopt -s nullglob dotglob
        for entry in "$real_runtime"/*; do
            base="${entry##*/}"
            [[ "$base" == . || "$base" == .. || "$base" == gvfs || "$base" == flea-ipc ]] && continue
            ln -s "$entry" "$runtime/$base"
        done
        shopt -u nullglob dotglob
    fi
    mkdir -m 700 "$runtime/flea-ipc"

    # Empty listing, instant mount claim and info under the sandboxed bridge dir; trash keeps the real gio.
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
case "\$*" in
"info --attributes=trash::item-count trash:///"|"monitor --dir=trash:///") exec /usr/bin/gio "\$@" ;;
esac
case "\$1 \${2:-}" in
"mount -li") exit 0 ;;
"mount --anonymous") printf '%s\n' "\$*" >> "$dir/mount-calls" ;;
"info smb://bridge-slow.test/data") printf 'local path: %s\n' "\$XDG_RUNTIME_DIR/gvfs/$leaf_slow" ;;
"info smb://bridge-fail.test/data") printf 'local path: %s\n' "\$XDG_RUNTIME_DIR/gvfs/$leaf_fail" ;;
*) exit 0 ;;
esac
EOS
    chmod +x "$dir/bin/gio"
    : > "$dir/mount-calls"
    # Sample input: "<fusebin> <bridgedir> -f"; serves the leaf only after the 250 ms Starting line stands.
    cat > "$dir/bin/flea-gvfs-fuse-slow" <<EOS
#!/bin/sh
sleep 2
mkdir -p "\$1/$leaf_slow"
printf 'served\n' > "\$1/$leaf_slow/alpha.txt"
EOS
    chmod +x "$dir/bin/flea-gvfs-fuse-slow"
    # The failing bridge: serves nothing, so the whole-ensure 15 s deadline fails the wait.
    cat > "$dir/bin/flea-gvfs-fuse-fail" <<EOS
#!/bin/sh
exit 1
EOS
    chmod +x "$dir/bin/flea-gvfs-fuse-fail"

    seed_ui_state "$state" '{"view":"list","keys":"default","places":{"favourites":[{"label":"Bridge slow","path":"smb://bridge-slow.test/data"},{"label":"Bridge fail","path":"smb://bridge-fail.test/data"}]}}'
    gvfsbridge_launch "$dir" "$fixture_home" "$runtime" "$dir/bin/flea-gvfs-fuse-slow"
    wait_listing 3
    wait_rail_label "Bridge slow"

    local slow_index
    slow_index=$(ipc railEntries | jq -r 'map(.label) | index("Bridge slow")')
    [[ "$slow_index" != null ]] || fail "gvfsbridge: no rail row labelled Bridge slow"
    click_rail_row "$slow_index" left
    local seen_sticky=""
    for _attempt in $(seq 1 200); do
        seen_sticky=$(ipc stickyMessage)
        [[ "$seen_sticky" == "Opening Bridge slow" ]] && break
        sleep 0.05
    done
    [[ "$seen_sticky" == "Opening Bridge slow" ]] \
        || fail "gvfsbridge: the Starting line never stood, sticky says $seen_sticky"
    # Live this shows the mark walks with the line; tests/js/status.js tells the call from the sentence.
    [[ "$(ipc statusBusy)" == "true" ]] || fail "gvfsbridge: the busy mark did not walk while the Starting line stood"
    shot gvfsbridge-starting
    wait_path "$gvfsbridge_runtime/gvfs/$leaf_slow"
    printf 'GVFSBRIDGE slow=started-opened\n'

    # The fail flight swaps only the fuse stub.
    gvfsbridge_launch "$dir" "$fixture_home" "$runtime" "$dir/bin/flea-gvfs-fuse-fail"
    wait_listing 3
    wait_rail_label "Bridge fail"
    local fail_index
    fail_index=$(ipc railEntries | jq -r 'map(.label) | index("Bridge fail")')
    [[ "$fail_index" != null ]] || fail "gvfsbridge: no rail row labelled Bridge fail"
    click_rail_row "$fail_index" left
    wait_message "Bridge fail could not be opened because a system service did not start · waiting for the folder timed out"
    [[ "$(ipc statusError)" == "true" ]] || fail "gvfsbridge: the failure line is not in the error role"
    [[ "$(ipc statusBusy)" == "false" ]] || fail "gvfsbridge: the busy mark still walks after the bridge failed"
    shot gvfsbridge-failed
    printf 'GVFSBRIDGE fail=error-line\n'

    kill_flea
    sandbox_remove "$fixture_home"
    # The runtime dir holds links into the operator's, and rm does not follow them.
    [[ "$runtime" == /tmp/fgb.?* && -f "$runtime/.flea-test-sandbox" ]] \
        || fail "gvfsbridge: $runtime is not the case's marked runtime root, so it stays"
    rm -rf -- "$runtime"
}
