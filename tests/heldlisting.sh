#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
flea_ipc_init qs
. "$(dirname "$0")/headless-commons.sh"
# A held first listing reads loading in List and Columns while the IPC seam stays unbuilt, and ui-captures-states.sh proves the reading shot by a live shell's "Target not found." for that absent seam.
set -u
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"

failed=0
ok() { printf 'ok   %s\n' "$*"; }
bad() { printf 'FAIL %s\n' "$*"; failed=1; }

# Sample input, ipc ready with the seam unbuilt in a live shell: "Target not found."; built: "true"; no shell at all: 'No running instances for "/path/shell.qml"' on stderr.
helper() {
    (
        eval "$(sed -n '/^heldcolumns_expect_loading()/,/^}/p;/^heldcolumns_launch()/,/^}/p' tests/ui-captures-states.sh)"
        for name in heldcolumns_expect_loading heldcolumns_launch; do
            declare -F "$name" > /dev/null || { printf 'FAIL: %s is not defined by tests/ui-captures-states.sh\n' "$name"; exit 1; }
        done
        fail() { printf 'FAIL: %s\n' "$*"; exit 1; }
        eval "$@"
    )
}

out=$(helper 'ipc() { printf "Target not found.\n"; return 1; }; heldcolumns_expect_loading 2 "before its reading shot"; printf "loading\n"')
[ "$out" = loading ] && ok "a live shell's absent seam proves the pane still reads loading" || bad "a live shell's absent seam must pass, got: $out"

out=$(helper 'ipc() { printf "Target not found.\n"; }; heldcolumns_expect_loading 2 "before its reading shot"; printf "loading\n"')
[ "$out" = loading ] && ok "the absent seam proves loading whatever the call's status" || bad "the absent seam must pass at status 0 too, got: $out"

out=$(helper 'ipc() { return 1; }; heldcolumns_expect_loading 2 "before its reading shot"; printf "loading\n"')
[ "$out" = "FAIL: heldcolumns: limit 2 got no live answer before its reading shot, ipc ready said 'nothing'" ] \
    && ok "a failed call with no answer fails: nothing proves the pane loading" || bad "a failed call must fail, got: $out"

out=$(helper 'ipc() { printf "No running instances for \"/x/shell.qml\"\n" >&2; return 255; }; heldcolumns_expect_loading 4 "after its reading shot"; printf "loading\n"')
case $out in
    "FAIL: heldcolumns: limit 4 got no live answer after its reading shot, ipc ready said 'No running instances"*) ok "a dead shell's answer fails, stderr read too" ;;
    *) bad "a dead shell must fail, got: $out" ;;
esac

out=$(helper 'ipc() { return 124; }; heldcolumns_expect_loading 5 "before its reading shot"; printf "loading\n"')
[ "$out" = "FAIL: heldcolumns: limit 5 got no live answer before its reading shot, ipc ready said 'nothing'" ] \
    && ok "a timed-out call fails" || bad "a timed-out call must fail, got: $out"

out=$(helper 'ipc() { printf "true\n"; }; heldcolumns_expect_loading 3 "after its reading shot"; printf "loading\n"')
[ "$out" = 'FAIL: heldcolumns: limit 3 left loading after its reading shot' ] \
    && ok "a built seam fails: the pane left loading" || bad "a built seam must fail, got: $out"

out=$(helper 'calls=0; assert_theme() { calls=$((calls + 1)); }; launch() { assert_theme; launched=$1; }; heldcolumns_launch /held; during=$calls; assert_theme; printf "launch=%s during=%s after=%s\n" "$launched" "$during" "$calls"')
[ "$out" = 'launch=/held during=0 after=1' ] \
    && ok "a held launch skips the seam wait and gives it back" || bad "a held launch must skip assert_theme once, got: $out"

# Sample input, the case body: the held launch, then the reads that can only run with a seam come after wait_pane_waiting.
body=$(sed -n '/^case_cap_heldcolumns()/,/^}/p' tests/ui-captures-states.sh)
before_waiting=$(printf '%s\n' "$body" | sed -n '/heldcolumns_launch "\$listing"/,/wait_pane_waiting heldcolumns/p')
if printf '%s\n' "$body" | grep -q 'heldcolumns_launch "\$listing"' && ! printf '%s\n' "$body" | grep -q '^ *launch "\$listing"' \
    && [ -n "$before_waiting" ] && ! printf '%s\n' "$before_waiting" | grep -qE 'ipc (state|viewMode)|assert_theme'; then
    ok "the held Columns case launches without the seam wait and reads no IPC state or theme before waiting"
else
    bad "the held Columns case must launch through heldcolumns_launch and read no ipc state, viewMode or assert_theme before wait_pane_waiting"
fi

# Sample input, the case body: the seam exists from wait_pane_waiting on, so the held launch's theme is asserted there and before the waiting shot.
after_waiting=$(printf '%s\n' "$body" | sed -n '/wait_pane_waiting heldcolumns/,/shot "heldcolumns-\$n-waiting"/p')
if printf '%s\n' "$after_waiting" | grep -q '^ *assert_theme$' && [ "$(printf '%s\n' "$body" | grep -c '^ *assert_theme$')" = 1 ]; then
    ok "the held Columns case asserts the theme once, after the seam exists and before the waiting shot"
else
    bad "the held Columns case must call assert_theme once between wait_pane_waiting and the waiting shot"
fi

[ "$failed" = 0 ] || exit 1


for tool in qs dbus-run-session; do
    command -v "$tool" > /dev/null || { printf 'FAIL heldlisting needs %s\n' "$tool"; exit 1; }
done
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[ -x "$bin" ] || { printf 'FAIL heldlisting needs the built backend: %s\n' "$bin"; exit 1; }
eval "$(sed -n '/^hanglisting_write_proxy()/,/^}/p' tests/ui.sh)"
declare -F hanglisting_write_proxy > /dev/null || { printf 'FAIL: hanglisting_write_proxy is not defined by tests/ui.sh\n'; exit 1; }

runner=""
probe_root=""
cleanup() {
    [ -n "$runner" ] && kill -TERM -- "-$runner" 2> /dev/null
    [ -n "$runner" ] && wait "$runner" 2> /dev/null
    [ -n "$probe_root" ] && [ -d "$probe_root/w" ] && sandbox_remove "$probe_root/w"
    # The root holds only its marker now; rmdir refuses anything else.
    case "$probe_root" in
        /tmp/flea-hl.?*) [ -f "$probe_root/$SANDBOX_MARKER" ] && rm -f -- "$probe_root/$SANDBOX_MARKER" && rmdir -- "$probe_root" ;;
    esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# A short root under /tmp: the seam's socket path must fit the 108-byte sun_path.
sandbox_forbidden /tmp && sandbox_refuse "heldlisting: /tmp is inside a forbidden test target"
probe_root=$(mktemp -d /tmp/flea-hl.XXXXXX) || exit 1
FIXTURE_ROOT=$probe_root
sandbox_root_ok
probe_root=$SANDBOX_ROOT
: > "$probe_root/$SANDBOX_MARKER" || exit 1
work=$probe_root/w
mkdir -p "$work" || exit 1
hold_s=4
now() { date +%s%3N; }

# leg_dirs NAME: a probe directory with the two-file folder the proxy holds, a private runtime dir and the proxy.
leg_dirs() {
    local probe="$work/$1"
    mkdir -p "$probe"/{config,home/fixture,state,data,cache,runtime} || return 1
    chmod 700 "$probe/runtime"
    : > "$probe/home/fixture/one.txt"
    : > "$probe/home/fixture/two.txt"
    hanglisting_write_proxy "$probe/flea-proxy"
}

# One qs ipc call against the control shell's runtime dir, the same two answers the harness reads: "Target not found." for a missing target, a refusal for a missing shell.
ctl_qs() {
    env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE XDG_RUNTIME_DIR="$ctl/runtime" QT_QPA_PLATFORM=offscreen \
        timeout 5 "$FLEA_IPC_HELPER" -p "$ctl/config" call "$@"
}

# The real qs against a shell that serves one target, other, and never flea: "Target not found." while it lives, "No running instances" once it is gone.
ipc_control() {
    local up_by=$(( $(now) + 15000 )) out up=""
    ctl=$work/ctl
    leg_dirs ctl || { bad "control: the probe directories could not be made"; return; }
    cat > "$ctl/config/shell.qml" << 'QML'
import Quickshell
import Quickshell.Io

ShellRoot {
    IpcHandler {
        target: "other"
        function ping(): bool { return true }
    }
}
QML
    out=$(helper 'ipc() { ctl_qs flea "$@"; }; heldcolumns_expect_loading 2 "with no shell"; printf "loading\n"')
    case $out in
        "FAIL: heldcolumns: limit 2 got no live answer with no shell, ipc ready said 'No running instances"*) ok "control: no shell fails the loading check" ;;
        *) bad "control: no shell must fail the loading check, got: $out" ;;
    esac
    setsid env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE XDG_RUNTIME_DIR="$ctl/runtime" QT_QPA_PLATFORM=offscreen \
        qs -p "$ctl/config" > "$ctl/qs.log" 2>&1 < /dev/null &
    runner=$!
    while (( $(now) < up_by )); do
        [ "$(ctl_qs other ping 2> /dev/null)" = true ] && { up=1; break; }
        sleep 0.1
    done
    if [ -z "$up" ]; then
        bad "control: the shell never answered; log tail: $(tail -3 "$ctl/qs.log")"
    else
        out=$(helper 'ipc() { ctl_qs flea "$@"; }; heldcolumns_expect_loading 2 "with a live shell"; printf "loading\n"')
        [ "$out" = loading ] && ok "control: a live shell without the seam passes the loading check" || bad "control: a live shell without the seam must pass, got: $out"
    fi
    kill -TERM -- "-$runner" 2> /dev/null
    wait "$runner" 2> /dev/null
    runner=""
    out=$(helper 'ipc() { ctl_qs flea "$@"; }; heldcolumns_expect_loading 2 "after the shell died"; printf "loading\n"')
    case $out in
        "FAIL: heldcolumns: limit 2 got no live answer after the shell died, ipc ready said 'No running instances"*) ok "control: a shell that died fails the loading check" ;;
        *) bad "control: a dead shell must fail the loading check, got: $out" ;;
    esac
}

# The Pane over the real backend, offscreen: loading with the list in flight for the whole hold, then ready with both rows.
pane_leg() {
    local view=$1 probe="$work/pane-$1" out noisy
    leg_dirs "pane-$view" || { bad "pane $view: the probe directories could not be made"; return; }
    ln -s "$PWD/ui" "$probe/config/flea"
    headless_commons "$work" "$probe/config/Commons" || exit 1
    ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui"
    cp tests/heldlisting.qml "$probe/config/shell.qml"
    out=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
        HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_RUNTIME_DIR="$probe/runtime" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
        FLEA_BIN="$probe/flea-proxy" FLEA_HANGLISTING_REAL="$bin" FLEA_HANGLISTING_DELAY="$hold_s" FLEA_HANGLISTING_MARKER="$probe/held" \
        HELDLISTING_VIEW="$view" HELDLISTING_MS=$((hold_s * 1000)) HELDLISTING_DIR="$probe/home/fixture" timeout 30 qs -p "$probe/config" 2>&1)
    noisy=$(printf '%s\n' "$out" | grep -av 'does not support setting window masks' | grep -aiE 'WARN|ERROR|Binding loop|TypeError|ReferenceError|is not a function|is not defined')
    if [ "$(printf '%s\n' "$out" | grep -ac 'HELDLISTING PASS')" = 1 ] && [ "$(printf '%s\n' "$out" | grep -ac 'HELDLISTING DONE')" = 1 ] \
        && ! printf '%s\n' "$out" | grep -aq 'HELDLISTING FAIL' && [ -f "$probe/held" ] && [ -z "$noisy" ]; then
        ok "pane $view: $(printf '%s\n' "$out" | grep -ao 'HELDLISTING PASS.*')"
    else
        bad "pane $view did not read loading through the hold: $(printf '%s\n' "$out" | grep -a 'HELDLISTING') held=$([ -f "$probe/held" ] && echo yes || echo no) $noisy"
    fi
}

# One qs ipc call against the boot shell of a leg's runtime dir; empty for any failure.
boot_ipc() {
    local runtime=$1
    shift
    env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE XDG_RUNTIME_DIR="$runtime" QT_QPA_PLATFORM=offscreen timeout 5 "$FLEA_IPC_HELPER" -p "$PWD/ui/boot" call flea "$@" 2> /dev/null
}

# The real entry offscreen: no seam answers while the list is held, and the seam answers once the rows have landed in the view the leg is named for.
boot_leg() {
    local view=$1 limit=$2 probe="$work/boot-$1" held_at="" unbuilt=0 early="" built_at="" answer t seen_view=""
    leg_dirs "boot-$view" || { bad "boot $view: the probe directories could not be made"; return; }
    HOME="$probe/home" XDG_STATE_HOME="$probe/state" env -- "$bin" --ui-state '{"view":"'"$view"'","columnsLimit":'"$limit"',"updates":{"autoCheck":false}}' > /dev/null \
        || { bad "boot $view: the state seed failed"; return; }
    setsid env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
        HOME="$probe/home" XDG_CONFIG_HOME="$probe/config" XDG_STATE_HOME="$probe/state" \
        XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
        FLEA_PATH="$probe/home/fixture" FLEA_BIN="$probe/flea-proxy" GVFS_DISABLE_FUSE=1 \
        FLEA_HANGLISTING_REAL="$bin" FLEA_HANGLISTING_DELAY="$hold_s" FLEA_HANGLISTING_MARKER="$probe/held" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
        dbus-run-session -- qs -p "$PWD/ui/boot" > "$probe/qs.log" 2>&1 < /dev/null &
    runner=$!
    local by=$(( $(now) + 30000 ))
    while (( $(now) < by )); do
        t=$(now)
        answer=$(boot_ipc "$probe/runtime" ready) || answer=""
        [ -z "$held_at" ] && [ -f "$probe/held" ] && held_at=$t
        if [ "$answer" = true ]; then
            built_at=$t
            # The seam is the first thing that can say which view the pane drew, so the view is read the moment it exists.
            seen_view=$(boot_ipc "$probe/runtime" viewMode)
            [ -n "$held_at" ] && (( t < held_at + hold_s * 1000 - 400 )) && early="seam answered $(( t - held_at )) ms into the hold"
            break
        fi
        [ -n "$held_at" ] && (( t < held_at + hold_s * 1000 - 400 )) && unbuilt=$(( unbuilt + 1 ))
        sleep 0.05
    done
    kill -TERM -- "-$runner" 2> /dev/null
    wait "$runner" 2> /dev/null
    runner=""
    if [ -n "$early" ] || [ -z "$built_at" ] || [ -z "$held_at" ] || (( unbuilt < 10 )) || [ "$seen_view" != "$view" ]; then
        bad "boot $view: unbuilt polls $unbuilt of the hold, held=${held_at:-never}, built=${built_at:-never}, view=${seen_view:-unread}, $early; log tail: $(tail -3 "$probe/qs.log")"
    else
        ok "boot $view: view $seen_view, no seam for $unbuilt polls through the hold, built $(( built_at - held_at )) ms after the list was held"
    fi
}

ipc_control
pane_leg list
pane_leg columns
boot_leg list 2
boot_leg columns 2

[ "$failed" = 0 ] || exit 1
printf 'HELDLISTING helpers=ok pane=list,columns boot=list,columns\n'
