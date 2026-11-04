#!/usr/bin/env bash
# The ui:gvfsbridge case's environment scope and runtime preflight, and kill_flea's hyprctl failure line, against tool doubles that record what they see.
set -u
cd "$(dirname "$0")/.." || exit 1
repo=$PWD

failed=0
ok() { printf 'ok   %s\n' "$*"; }
bad() { printf 'FAIL %s\n' "$*"; failed=1; }

MARKER=.flea-test-sandbox
box=$(mktemp -d) || exit 1
: > "$box/$MARKER"
# Removes only an absolute, non-empty root that still carries its marker.
cleanup() {
    case $box in
        /?*) [ -f "$box/$MARKER" ] && rm -rf -- "$box" ;;
    esac
}
trap cleanup EXIT

# Sample input: gvfsbridge_launch() { ... } up to the first line that is a closing brace in column 0.
cut_out() { sed -n "/^$1() {/,/^}/p" "$2"; }
launch_def=$(cut_out launch tests/ui.sh)
window_def=$(cut_out launch_window tests/ui.sh)
homes_def=$(cut_out case_homes tests/ui.sh)
kill_def=$(cut_out kill_flea tests/ui.sh)
helper_def=$(cut_out gvfsbridge_launch tests/ui-gvfsbridge.sh)
fits_def=$(cut_out gvfsbridge_runtime_fits tests/ui-gvfsbridge.sh)
[ -n "$launch_def" ] && [ -n "$window_def" ] && [ -n "$kill_def" ] || { bad "launch, launch_window and kill_flea could not be cut out of tests/ui.sh"; exit 1; }

mkdir -p "$box/bin" "$box/operator-home" "$box/operator-rt" "$box/fixture-home" "$box/fixture-rt" "$box/dir/bin"
# One recorder for every double: the tool's name, then the four variables the case moves, then the first PATH entry.
record='printf "%s HOME=%s RT=%s FUSE=%s PATH1=%s\n" "$name" "$HOME" "$XDG_RUNTIME_DIR" "${FLEA_GVFS_FUSE-unset}" "${PATH%%:*}" >> "$REC/env.log"'
cat > "$box/bin/hyprctl" <<EOS
#!/bin/sh
name=hyprctl
$record
if [ -n "\${HYPRCTL_FAIL:-}" ]; then printf '%s\n' "Couldn't connect to /x/hypr/sig/.socket.sock. (3)" 'second line' >&2; exit 1; fi
printf '[]\n'
EOS
cat > "$box/bin/omarchy-drive" <<EOS
#!/bin/sh
name=omarchy-drive
$record
# Sample input: omarchy-drive wait window flea --timeout 15; the window exists once the flea double has written its record.
if [ "\$1 \$2" = "wait window" ]; then
    n=0
    while [ ! -f "\$REC/flea.env" ] && [ "\$n" -lt 100 ]; do sleep 0.05; n=\$((n + 1)); done
    [ -f "\$REC/flea.env" ] || exit 1
fi
exit 0
EOS
cat > "$box/bin/flea" <<EOS
#!/bin/sh
name=flea
$record
: > "\$REC/flea.env"
EOS
chmod +x "$box/bin/hyprctl" "$box/bin/omarchy-drive" "$box/bin/flea"

operator_path="$box/bin:$PATH"
operator_line="HOME=$box/operator-home RT=$box/operator-rt FUSE=unset PATH1=$box/bin"

# Runs gvfsbridge_launch under the operator's environment with the doubles first on PATH; prints what the shell holds afterwards.
run_launch() {
    local rec="$1" fuse="$2"
    mkdir -p "$rec"
    (
        eval "$launch_def"
        eval "$window_def"
        eval "$homes_def"
        test_frontend=qml
        eval "$kill_def"
        eval "$helper_def"
        fail() { printf 'FAIL: %s\n' "$*"; exit 1; }
        # The shared transport launches only worktree binaries, so the subshell spawns the bridge stub the way ui.sh did before it.
        . "$repo/tests/lib/fixture-launch.sh" || fail "cannot load the fixture launch double"
        flea_pids() { :; }
        backend_pids() { :; }
        owned_trash_monitors() { :; }
        flea_pid() { printf '123\n'; }
        assert_window() { omarchy-drive windows --json >/dev/null; }
        sleep() { :; }
        foreign_pids="" drain_wait_s=2 run_root="$box" fixture_root="$box/fixtures" run_log="$rec/run.log" flea_log="$rec/flea.log"
        flea_ui="$repo/ui" flea_bin="$box/bin/flea" FLEA_UNDO_DIR="$box/undo"
        export REC="$rec" HOME="$box/operator-home" XDG_RUNTIME_DIR="$box/operator-rt" PATH="$operator_path"
        unset FLEA_GVFS_FUSE
        gvfsbridge_launch "$box/dir" "$box/fixture-home" "$box/fixture-rt" "$fuse"
        printf 'AFTER HOME=%s RT=%s FUSE=%s PATH=%s\n' "$HOME" "$XDG_RUNTIME_DIR" "${FLEA_GVFS_FUSE-unset}" "$PATH"
    )
}

# Sample input: the "slow" bridge stub the case launches first, then the "fail" one.
if [ -z "$helper_def" ]; then
    bad "gvfsbridge_launch is not defined in tests/ui-gvfsbridge.sh"
else
    for stub in flea-gvfs-fuse-slow flea-gvfs-fuse-fail; do
        rec="$box/rec-$stub"
        out=$(run_launch "$rec" "$box/dir/bin/$stub" 2>&1)
        harness=$(grep -E '^(hyprctl|omarchy-drive) ' "$rec/env.log" | sed 's/^[^ ]* //' | sort -u)
        [ "$harness" = "$operator_line" ] \
            && ok "$stub: hyprctl and omarchy-drive see the operator's HOME, runtime dir and PATH, with no bridge stub" \
            || bad "$stub: the harness tools saw [$harness], wanted [$operator_line]"
        [ "$(grep -c '^hyprctl ' "$rec/env.log")" -ge 1 ] && [ "$(grep -c '^omarchy-drive ' "$rec/env.log")" -ge 2 ] \
            && ok "$stub: the launch ran hyprctl in kill_flea and omarchy-drive for the window" \
            || bad "$stub: the launch skipped a harness tool, so the environment check proved nothing"
        flea=$(grep '^flea ' "$rec/env.log" | sed 's/^[^ ]* //')
        want="HOME=$box/fixture-home RT=$box/fixture-rt FUSE=$box/dir/bin/$stub PATH1=$box/dir/bin"
        [ "$flea" = "$want" ] && ok "$stub: Flea sees the fixture's HOME, runtime dir, bridge stub and stub dir first on PATH" \
            || bad "$stub: Flea saw [$flea], wanted [$want]"
        after=$(grep '^AFTER ' <<< "$out")
        [ "$after" = "AFTER HOME=$box/operator-home RT=$box/operator-rt FUSE=unset PATH=$operator_path" ] \
            && ok "$stub: the caller's own environment is unchanged after the launch" \
            || bad "$stub: the caller's environment afterwards was [$after]"
    done
fi

# The case exports none of what the helper now scopes: an export would put the fixture back under the harness tools.
exports=$(grep -nE '^[[:space:]]*export (HOME|XDG_RUNTIME_DIR|FLEA_GVFS_FUSE|PATH|YDOTOOL_SOCKET)=' tests/ui-gvfsbridge.sh)
[ -z "$exports" ] && ok "tests/ui-gvfsbridge.sh exports none of HOME, XDG_RUNTIME_DIR, FLEA_GVFS_FUSE, PATH or YDOTOOL_SOCKET" \
    || bad "tests/ui-gvfsbridge.sh still exports: $exports"
launches=$(grep -c 'gvfsbridge_launch "' tests/ui-gvfsbridge.sh)
[ "$launches" -eq 2 ] && ok "the case launches both windows through gvfsbridge_launch" \
    || bad "the case calls gvfsbridge_launch $launches times, wanted 2"

# kill_flea names hyprctl's exit code and its first stderr line, and still fails.
kill_run() {
    local hyprctl_fail="$1"
    (
        eval "$kill_def"
        fail() { printf 'FAIL: %s\n' "$*"; exit 1; }
        flea_pids() { :; }
        backend_pids() { :; }
        owned_trash_monitors() { :; }
        sleep() { :; }
        foreign_pids="" drain_wait_s=2 run_root="$box"
        export REC="$box/rec-kill" PATH="$box/bin:$PATH" HOME="$box/operator-home" XDG_RUNTIME_DIR="$box/operator-rt" HYPRCTL_FAIL="$hyprctl_fail"
        mkdir -p "$REC"
        kill_flea
        printf 'drained\n'
    )
}
out=$(kill_run 1 2>&1)
want="FAIL: cannot inspect Hyprland clients while draining, because hyprctl exited 1 and its first stderr line was: Couldn't connect to /x/hypr/sig/.socket.sock. (3)"
[ "$out" = "$want" ] && ok "a failing hyprctl fails kill_flea with its exit code and first stderr line" \
    || bad "a failing hyprctl gave [$out], wanted [$want]"
out=$(kill_run "" 2>&1)
[ "$out" = drained ] && ok "a working hyprctl with no dead window drains" || bad "a working hyprctl gave [$out], wanted drained"

# The runtime dir must keep Quickshell's IPC socket and Hyprland's event socket, the longest path it reaches there, inside sun_path's 107 bytes.
if [ -z "$fits_def" ]; then
    bad "gvfsbridge_runtime_fits is not defined in tests/ui-gvfsbridge.sh"
else
    fits() (
        unset HYPRLAND_INSTANCE_SIGNATURE
        eval "$fits_def"
        fail() { printf 'FAIL: %s\n' "$*"; exit 1; }
        gvfsbridge_runtime_fits "$1"
        printf 'fits\n'
    )
    # Sample input: a runtime dir of 86 bytes, whose IPC socket path is 129 (86 plus the 43 bytes of /quickshell/by-id/<16 characters>/ipc.sock).
    long=/home/flea-sandbox/r6-net.W3z8dKGC/fixture/flea-ui-fixtures-1234567/gvfsbridge-runtime
    out=$(fits "$long")
    [ "${out#FAIL: gvfsbridge: the socket path $long/quickshell/by-id/}" != "$out" ] \
        && [ "${out%, so the runtime directory needs a shorter root}" != "$out" ] \
        && ok "a runtime dir whose IPC socket path passes 107 bytes fails with a whole sentence" \
        || bad "the 86 byte runtime dir gave [$out]"
    # Sample input: a runtime dir of 39 bytes, whose IPC socket path is 82, beside a Wayland display name of 78 bytes, which makes the Wayland socket path 118.
    short=/tmp/flea-ui-run.ZyXqhjfz/gvfsbridge-rt
    wayland=wayland-0123456789012345678901234567890123456789012345678901234567890123456789
    out=$(WAYLAND_DISPLAY=$wayland fits "$short")
    [ "${out#FAIL: gvfsbridge: the socket path $short/$wayland is 118 bytes, past the 107 }" != "$out" ] \
        && [ "${out%, so the runtime directory needs a shorter root}" != "$out" ] \
        && ok "a Wayland display name that passes 107 bytes fails with a whole sentence while the IPC socket path fits" \
        || bad "the long Wayland display name gave [$out]"
    # Sample input: the dir the case once made under the run root, whose Hyprland event socket path is 121 bytes (39 plus 6, 62 and 14).
    old=/tmp/flea-ui-run.ZyXqhjfz/gvfsbridge-rt
    out=$(fits "$old")
    [ "${out#FAIL: gvfsbridge: the socket path $old/hypr/}" != "$out" ] \
        && ok "a runtime dir under the run root fails on Hyprland's socket path" || bad "the run root's runtime dir gave [$out]"
    # Sample input: the root the case now makes, mktemp -d /tmp/fgb.XXXXXX.
    out=$(fits /tmp/fgb.Zq3x9a)
    [ "$out" = fits ] && ok "the case's own /tmp runtime root fits" || bad "the case's runtime root gave [$out]"
fi

[ "$failed" = 0 ] || exit 1
printf 'UIGVFSBRIDGEENV DONE\n'
