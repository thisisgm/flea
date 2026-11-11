#!/usr/bin/env bash
# Replays capture assertions against controlled replies, including scaled controls and both outcomes of the open-file unmount.
set -uo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)" || exit 1
bash -n "$repo/tests/ui-captures-cohesion.sh" || exit 1
bash -n "$repo/tests/capture-port.sh" || exit 1
printf 'CAPTUREPORT SYNTAX both shell scripts passed\n'
. "$repo/tools/flea-sandbox-guard"
sandbox_root_ok
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-capture-port.XXXXXXXX") || exit 1
: > "$scratch/$SANDBOX_MARKER"
holder=""
group_fixture="" detached_launcher=""
cleanup() {
    if [[ "$group_fixture" == "$scratch/"* && -f "$group_fixture/$SANDBOX_MARKER" ]]; then
        local file pid
        for file in "$group_fixture/child.pid" "$group_fixture/detached.pid"; do
            [[ -f "$file" ]] || continue
            # Sample input: child.pid and detached.pid each hold one decimal pid, "4242".
            read -r pid < "$file"
            [[ "$pid" =~ ^[0-9]+$ ]] && kill -KILL "$pid" 2>/dev/null || true
        done
        [[ -z "$detached_launcher" ]] || wait "$detached_launcher" 2>/dev/null || true
    fi
    if [[ -n "$holder" ]]; then kill "$holder" 2>/dev/null || true; wait "$holder" 2>/dev/null || true; fi
    sandbox_remove "$scratch"
}
trap cleanup EXIT
. "$repo/tests/ui-captures-cohesion.sh"
. "$repo/tests/ui-trash.sh"
. "$repo/tests/ui-captures-b72.sh"
. "$repo/tests/ui-captures-unlock.sh"
. "$repo/tests/ui-captures-archives.sh"
checks=0
failed=0
coh_theme=replay
check() {
    checks=$((checks + 1))
    if [[ "$2" == "$3" ]]; then printf 'CAPTUREPORT PASS %s\n' "$1"; else
        printf 'FAIL capture-port %s: got [%s], expected [%s]\n' "$1" "$2" "$3"
        failed=$((failed + 1))
    fi
}
fail() { printf 'refused capture: %s\n' "$*" >&2; exit 1; }
settle() { :; }
# Real holder processes retain their cwd; only polling advances the clock to bound negative controls.
sleep() { if [[ "$1" == 60 ]]; then command sleep "$@"; else SECONDS=$((SECONDS + 5)); command sleep 0.01; fi; }
ipc() {
    case "$1" in
        cohesionControls) printf '%s\n' "$control_json" ;;
        railEntries) printf '%s\n' "$rail_json" ;;
        statusPrimary) printf '%s\n' "$footer" ;;
    esac
}
# Escape clears a standing footer unless escape_clears=false; Return (Unmount) shows footer_after when one is set.
key() {
    actions=$((actions + 1))
    case "$2" in
        Escape) [[ "${escape_clears:-true}" == true ]] && footer='' ;;
        Return) [[ -n "${footer_after:-}" ]] && footer=$footer_after ;;
    esac
    return 0
}
click_rail_row() { actions=$((actions + 1)); clicked_row="$1"; }
menu_seek() { :; }
shot() { shots=$((shots + 1)); }
mountpoint() { [[ "$mount_live" == true ]]; }

python3 "$repo/tests/capture-lifecycle.py" "$repo" "$scratch" "$SANDBOX_MARKER"
check "isolated signal and Trash teardown probes pass" "$?" 0

target='{"kind":"DialogButton","name":"Open","x":100,"y":100,"w":80,"h":30,"targetX":140,"targetY":115}'
control_json='{"controls":[{"kind":"DialogButton","name":"Open","x":102,"y":101,"w":80,"h":30,"pressed":true}]}'
coh_wait_flag "$target" pressed >/dev/null; check "a scaled held button keeps its identity at the hit point" "$?" 0
control_json='{"controls":[{"kind":"DialogButton","name":"Cancel","x":102,"y":101,"w":80,"h":30,"pressed":true}]}'
coh_wait_flag "$target" pressed >/dev/null; check "another control at the same point cannot satisfy the press" "$?" 1
control_json='{"controls":[{"kind":"DialogButton","name":"Open","x":250,"y":101,"w":80,"h":30,"pressed":true}]}'
coh_wait_flag "$target" pressed >/dev/null; check "the same name at another point cannot satisfy the press" "$?" 1
control_json='{"controls":[{"kind":"DialogButton","name":"Open","x":102,"y":101,"w":80,"h":30,"pressed":false}]}'
coh_wait_flag "$target" pressed >/dev/null; check "an unpressed control cannot satisfy the press" "$?" 1
control_json='{"controls":[{"kind":"MediaPlay","name":"Play","x":100,"y":100,"w":24,"h":24,"exposed":true,"hoverable":false,"pressable":true,"pressed":true}]}'
b72_flag_at 112 112 hovered media >/dev/null; check "media hover requires its actual exposed play target" "$?" 0
b72_flag_at 112 112 pressed media >/dev/null; check "media held press requires the actual TapHandler state" "$?" 0
control_json='{"controls":[]}'
b72_flag_at 112 112 hovered media >/dev/null; check "an absent media target never passes" "$?" 1

mount="$scratch/S1LUKS"
label="Locked container"
actions=0 shots=0 inuse_receipt=in-use mount_live=false rail_json='[]' footer=''
cap_unlock_unmount_in_use "$mount" Unmount > "$scratch/row-gone.log"
check "a removed unlocked row skips without navigating or starting a cwd holder" "$? $actions $shots $holder $inuse_receipt" '0 0 0  skipped'
grep -q '^CAP SKIP unlock-unmount-in-use:' "$scratch/row-gone.log"; check "the removed-row skip records its reason" "$?" 0
rail_json=$(jq -cn --arg mount "$mount" '[{path:$mount,device:"/dev/mapper/fixture",label:"S1LUKS",locked:false}]')
cap_unlock_unmount_in_use "$mount" Unmount > "$scratch/mount-gone.log"
check "a stale row with no actual mount also skips before cd" "$? $actions $shots $holder $inuse_receipt" '0 0 0  skipped'
mkdir "$mount"
busy='S1LUKS is busy; close what is using it and try again.'
mount_live=true inuse_receipt=in-use footer='' footer_after=$busy
rail_json=$(jq -cn --arg mount "$mount" '[{path:$mount,label:"Favourite",device:""},{path:$mount,device:"/dev/mapper/fixture",label:"S1LUKS",locked:false}]')
cap_unlock_unmount_in_use "$mount" Unmount > "$scratch/in-use.log"
check "a mounted row uses its filesystem name and captures the refusal" "$? $actions $shots $holder $inuse_receipt" '0 2 1  in-use'
check "a favourite for the mount cannot replace the actual unlocked device row" "$clicked_row" 1
# The open-file leg can leave the same busy sentence standing; only a new one after Unmount counts.
actions=0 shots=0 footer=$busy footer_after=$busy
cap_unlock_unmount_in_use "$mount" Unmount > "$scratch/stale-cleared.log"
check "a standing busy sentence is cleared by Escape before Unmount" "$? $actions $shots $holder $footer" "0 3 1  $busy"
unlock_refused() {
    (
        trap 'if [[ -n "$holder" ]]; then kill "$holder" 2>/dev/null || true; wait "$holder" 2>/dev/null || true; fi' EXIT
        cap_unlock_unmount_in_use "$mount" Unmount
    ) > "$scratch/$1.log" 2>&1
    printf '%s %s' "$?" "$(grep -c -F -- "$2" "$scratch/$1.log")"
}
footer=$busy footer_after=''
check "a standing busy sentence with no new one after Unmount is refused" \
    "$(unlock_refused stale-only 'did not say it was busy')" '1 1'
footer=$busy footer_after=$busy escape_clears=false
check "a busy sentence that Escape never clears is refused before Unmount" \
    "$(unlock_refused stale-stuck 'the earlier busy sentence never cleared')" '1 1'
escape_clears=true footer='' footer_after='Locked container is busy; close what is using it and try again.'
check "the locked container's name cannot satisfy the mounted-row refusal" \
    "$(unlock_refused wrong-name 'did not say it was busy')" '1 1'
footer_after=''

# Drive the actual themed archive case with its UI steps stubbed; the fixture_home_make stub keeps its real sandbox_scratch guard.
fixture_root="$scratch/fixtures"
sandbox_make "$fixture_root"
flea_bin=/usr/bin/true
test_frontend=gpui
# The replay owns its 7z and theme, so the leg never depends on what the host or CI image installs.
mkdir -p "$scratch/bin" && printf '#!/bin/sh\nexit 0\n' > "$scratch/bin/7z" && chmod +x "$scratch/bin/7z"
PATH="$scratch/bin:$PATH"
printf 'foreground = "#dcd7ba"\n' > "$scratch/kanagawa-colors.toml"
kanagawa_colors="$scratch/kanagawa-colors.toml"
fixture_home_make() { sandbox_scratch "$1"; mkdir -p "$1/.local/state/omarchy/current/theme"; }
cap_archives_format_hue() { :; }
cap_archives_format_open() { :; }
cap_archives_format_walk() { :; }
assert_theme() { :; }
kill_flea() { :; }
case_cap_archives_format_kanagawa > "$scratch/archive.log" 2>&1
check "the nested theme HOME passes the unchanged guard in the real archive case" "$?" 0
check "the theme HOME has a marked direct parent" "$([[ -f "$fixture_root/cap-archives-format-kanagawa-home/$SANDBOX_MARKER" ]] && echo yes || echo no)" yes

# Replay WindowDrag's displaced origin through the real cancel and parking helpers.
. "$repo/tests/lib/hypr-dispatch.sh"
# Command substitution runs the compositor stub in a subshell, so its geometry must live in a file.
native_box_file="$scratch/native-box"
printf '701 370 1100 700\n' > "$native_box_file"
replay_outputs='[{"name":"fixture","x":0,"y":0,"width":2560,"height":1440,"scale":1,"transform":0}]'
window_box() { cat "$native_box_file"; }
hyprctl() {
    local x y w h
    if [[ "$*" == 'monitors -j' ]]; then
        printf '%s\n' "$replay_outputs"
        return 0
    fi
    # Sample input: hl.dsp.window.move({ x = 701, y = 370, window = "address:0xabc" }).
    local move='[.]window[.]move[(][{] x = (-?[0-9]+), y = (-?[0-9]+), window = "address:0xabc" [}][)]$'
    [[ $# == 2 && "$2" =~ $move ]] || return 1
    if [[ "${replay_restore:-true}" == true ]]; then
        # Sample input: the state file holds "701 752 900 600" before the position is restored.
        read -r x y w h < "$native_box_file" || return 1
        printf '%s %s %s %s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "$w" "$h" > "$native_box_file"
    fi
    printf 'ok\n'
}
pointer_glide() {
    printf '%s %s\n' "$1" "$2" >> "$scratch/moves.log"
    if (( coh_held )); then printf '701 752 900 600\n' > "$native_box_file"; fi
}
coh_ydotool() { printf '%s\n' "$*" >> "$scratch/buttons.log"; }
resizes=0
cap_resize() {
    local x y w h
    # Sample input: the state file holds "701 370 1100 700", the window's x, y, width and height.
    read -r x y w h < "$native_box_file"
    resizes=$((resizes + 1))
    printf '%s %s %s %s\n' "$x" "$y" "$1" "$2" > "$native_box_file"
}
flea_pid() { printf '4242\n'; }
window_addr_of_pid() { [[ "$1" == 4242 ]] && printf '0xabc\n'; }
name=window
coh_held=1
( coh_cancel_press; coh_park; printf 'REPLAY_DRAG %s %s %s\n' "$resizes" "$coh_held" "$(window_box)" ) > "$scratch/drag.log" 2>&1
check "a held chrome cancel restores geometry before the next park" "$?" 0
check "the cancel restores position and size after releasing the button" "$(sed -n 's/^REPLAY_DRAG //p' "$scratch/drag.log")" '1 0 701 370 1100 700'
check "both pointer moves stay inside the output" "$(tr '\n' '|' < "$scratch/moves.log")" '707 1064|707 1064|'
grep -q '^COH_DRAG_RESET replay window .*displaced=701 752 900 600 restored=701 370 1100 700' "$scratch/drag.log"
check "a chrome drag records original, displaced and restored geometry" "$?" 0
coh_held=0
( coh_held=1; replay_restore=false; pointer_glide() { printf '701 752 900 600\n' > "$native_box_file"; }; coh_cancel_press ) > "$scratch/unrestored.log" 2>&1
check "a compositor that does not restore position fails the cancel" "$?" 1
grep -q 'window geometry was not restored' "$scratch/unrestored.log"
check "a failed restoration reports the geometry mismatch" "$?" 0
printf '701 752 1100 700\n' > "$native_box_file"
( coh_pointer_to 6 694 ) > "$scratch/outside.log" 2>&1
check "an off-output target is refused before motion" "$? $(wc -l < "$scratch/moves.log")" '1 2'
grep -q '^COH_POINTER_OUTSIDE replay window .*target=707,1446 .*"height":1440' "$scratch/outside.log"
check "the refusal names the target and physical output size" "$?" 0
replay_outputs='[{"name":"fixture","x":0,"y":0,"width":2560,"height":1440,"scale":2,"transform":0}]'
printf '100 100 300 200\n' > "$native_box_file"
coh_pointer_to 150 100
check "scaled outputs use logical pointer coordinates" "$?" 0
( coh_pointer_to 150 650 ) > "$scratch/scaled-outside.log" 2>&1
check "a point inside physical pixels but outside logical pixels is refused" "$?" 1
replay_outputs='[{"name":"fixture","x":0,"y":0,"width":1440,"height":2560,"scale":1,"transform":1}]'
printf '701 370 1100 700\n' > "$native_box_file"
coh_pointer_to 1000 694
check "rotated outputs swap their physical dimensions" "$?" 0
check "the rotated replay reaches a point outside the unswapped width" "$(tail -n 1 "$scratch/moves.log")" '1701 1064'
# Sample function fragments: the rotated width uses height and the rotated height uses width.
rotated_width='if ((.transform // 0) % 2) == 1 then .height else .width end'
rotated_height='if ((.transform // 0) % 2) == 1 then .width else .height end'
unrotated=$(declare -f coh_pointer_to)
unrotated=${unrotated//"$rotated_width"/.width}
unrotated=${unrotated//"$rotated_height"/.height}
moves_before=$(wc -l < "$scratch/moves.log")
( eval "$unrotated"; coh_pointer_to 1000 694 ) > "$scratch/unrotated.log" 2>&1
check "removing the rotation swap refuses the same target before motion" "$? $(wc -l < "$scratch/moves.log")" "1 $moves_before"

# Three distinct preview controls must each retain their own hit point through the actual surface loop.
(
    real_state_dir="$scratch/live-state"
    coh_launch() { :; }
    coh_park() { :; }
    coh_cancel_press() { coh_held=0; }
    coh_pointer_to() { point_x="$1"; point_y="$2"; }
    coh_wake_colpdf() { coh_pdf_bar=$(jq -c '.controls[] | select(.kind == "ViewportScrollBar")' <<< "$control_json"); }
    coh_shot() { printf 'REPLAY_TARGET %s %s %s,%s\n' "$3" "$5" "$point_x" "$point_y"; }
    point_x=0 point_y=0
    # Sample input: each control is exposed in PreviewColumn and has a distinct kind and target.
    control_json='{"controls":[
        {"kind":"ChromeButton","name":"Zoom","x":700,"y":50,"w":30,"h":30,"targetX":715,"targetY":65},
        {"kind":"DialogButton","name":"Open","x":800,"y":50,"w":60,"h":30,"targetX":830,"targetY":65},
        {"kind":"ViewportScrollBar","name":"Vertical","value":"vertical","x":1080,"y":40,"w":12,"h":300,"targetX":1086,"targetY":115}]}'
    control_json=$(jq -c '.controls |= map(. + {surface:"PreviewColumn",exposed:true,enabled:true,hoverLive:true,pressLive:true})' <<< "$control_json")
    ipc() {
        if [[ "$1" == pdfState ]]; then
            printf '%s\n' '{"frame":"640 40 460 630"}'
            return
        fi
        jq -c --argjson x "$point_x" --argjson y "$point_y" --argjson held "$coh_held" '.controls |= map(. + {
            hovered:(.targetX == $x and .targetY == $y),pressed:(.targetX == $x and .targetY == $y and $held == 1)})' <<< "$control_json"
    }
    coh_run_surface cols-pdf coh_open_main '{}' none '640 40 1100 670'
) > "$scratch/preview-targets.log" 2>&1
walk_rc=$?
(( walk_rc == 0 )) || cat "$scratch/preview-targets.log"
check "the PDF surface finishes its multi-control walk" "$walk_rc" 0
for target in 'ChromeButton|Zoom|700|50 715,65' 'DialogButton|Open|800|50 830,65' 'ViewportScrollBar|Vertical|1080|40 1086,115'; do
    for phase in hover press; do
        check "the PDF surface drives $target once at $phase" "$(grep -Fxc "REPLAY_TARGET $phase $target" "$scratch/preview-targets.log")" 1
    done
done

# The full aggregate refuses before fixture work; each existing selector still schedules every surface.
real_foreground='#ffffff'
( COH_THEMES='live gruvbox' case_cap_cohesion ) > "$scratch/split.log" 2>&1
check "the aggregate directs full captures to bounded per-theme cases" "$? $(grep -c 'ui:cap_cohesion_live ui:cap_cohesion_gruvbox ui:cap_cohesion_latte ui:cap_cohesion_kanagawa' "$scratch/split.log")" '1 1'
for themes in $'live\tgruvbox' $'live\ngruvbox'; do
    ( COH_THEMES="$themes" case_cap_cohesion ) > "$scratch/split-whitespace.log" 2>&1
    check "all shell whitespace splits full themes before fixture work" "$? $(grep -c 'split full capture' "$scratch/split-whitespace.log")" '1 1'
done
coh_capture_ui() { printf '%s\n' "$repo/ui"; }
coh_fixture() { :; }
coh_surface() { printf 'REPLAY_SURFACE %s %s %s\n' "$coh_theme" "$1" "$4"; }
coh_picker() { printf 'REPLAY_SURFACE %s picker none\n' "$coh_theme"; }
evidence_dir="$scratch/evidence"
for selector in live gruvbox latte kanagawa; do
    ( "case_cap_cohesion_$selector" ) > "$scratch/theme-$selector.log" 2>&1
    check "$selector schedules all 22 surfaces within its own case" "$? $(grep -c '^REPLAY_SURFACE ' "$scratch/theme-$selector.log")" '0 22'
    grep -q '^REPLAY_SURFACE .* keymap none$' "$scratch/theme-$selector.log"
    check "$selector keeps the keymap sheet open instead of sending Tab" "$?" 0
done

# Reload the wrapper and return from its worker before EXIT scans and terminates a real owned process.
. "$repo/tests/ui-captures-cohesion.sh"
coh_ydotool() { printf '%s\n' "$*" >> "$scratch/buttons.log"; }
coh_failed=''
coh_deadline=$((SECONDS + coh_budget_s))
coh_run_surface() {
    trash_box="$scratch/trash-lifetime-$1"
    sandbox_make "$trash_box"
    export XDG_DATA_HOME="$trash_box/data"
    trash_bus_address="unix:path=$trash_box/bus"
    trash_private_bus_id=replay-private
    mkdir "$XDG_DATA_HOME"
    trap 'coh_trash_cleanup $?' EXIT
    env DBUS_SESSION_BUS_ADDRESS="$trash_bus_address" XDG_DATA_HOME="$XDG_DATA_HOME" \
        bash -c 'printf ready > "$1"; exec sleep 60' bash "$trash_box/ready" &
    trash_bus_pid=$!
    printf '%s\n' "$trash_bus_pid" > "$trash_box/pid"
    local deadline=$((SECONDS + 5))
    until [[ -f "$trash_box/ready" ]]; do
        kill -0 "$trash_bus_pid" 2>/dev/null && (( SECONDS < deadline )) || fail "owned teardown replay process did not start"
        command sleep 0.01
    done
    if [[ "$1" == failed ]]; then coh_held=1; exit 7; fi
}
coh_surface passed > "$scratch/trash-passed.log" 2>&1
check "successful surface teardown retains ownership after its worker returns" "$? $(grep -c '^TRASH_TEARDOWN .*remaining=none status=0$' "$scratch/trash-passed.log")" '0 1'
! kill -0 "$(cat "$scratch/trash-lifetime-passed/pid")" 2>/dev/null
check "successful teardown reaps its private process" "$?" 0
: > "$scratch/buttons.log"
coh_surface failed > "$scratch/trash-failed.log" 2>&1
grep -q '^COH_TIMING replay failed .*exit=7$' "$scratch/trash-failed.log"
check "failure teardown preserves the original surface exit" "$?" 0
! kill -0 "$(cat "$scratch/trash-lifetime-failed/pid")" 2>/dev/null
check "failure teardown also reaps its private process" "$?" 0
check "failure teardown releases a held pointer button" "$(cat "$scratch/buttons.log")" 'click 0x80'

# A real blocking child must be interrupted, while the surface's existing EXIT trap still releases its held button.
coh_run_surface() {
    coh_held=1
    bash -c 'printf "%s\n" "$$" > "$1"; exec sleep 60' bash "$scratch/budget-pid"
}
: > "$scratch/buttons.log"
coh_failed=''
coh_deadline=$((SECONDS + 1))
budget_started=$SECONDS
coh_surface late > "$scratch/surface-budget.log" 2>&1
check "a late surface fails by its remaining theme budget" "$? $coh_failed" '0  replay/late'
grep -q '^COH_SKIPPED replay late cut after 1s (theme remaining 1s)$' "$scratch/surface-budget.log"
check "the budget cut names the interrupted surface" "$?" 0
grep -q '^COH_TIMING replay late .*exit=124$' "$scratch/surface-budget.log"
check "the interrupted surface reports its timeout exit" "$?" 0
! kill -0 "$(cat "$scratch/budget-pid")" 2>/dev/null
check "a budget cut stops a blocking surface child" "$?" 0
check "a budget cut releases the current held button" "$(cat "$scratch/buttons.log")" 'click 0x80'
(( SECONDS - budget_started <= 3 )); check "a late surface cannot spend its original long wait" "$?" 0

# The actual picker wrapper bounds setup and its process by the same remaining theme second, rather than its own 180s.
coh_home() { printf '%s\n' "$scratch/picker-home"; }
python3() { bash -c 'printf "%s\n" "$$" > "$1"; exec sleep 60' bash "$scratch/picker-budget-pid"; }
flea_ui="$repo/ui"
coh_failed=''
coh_deadline=$((SECONDS + 1))
budget_started=$SECONDS
coh_picker > "$scratch/picker-budget.log" 2>&1
check "the picker fails by the remaining theme budget" "$? $coh_failed" '0  replay/picker'
grep -q '^COH_SKIPPED replay picker cut after 1s (theme remaining 1s)$' "$scratch/picker-budget.log"
check "the picker budget cut names the picker" "$?" 0
grep -q '^COH_TIMING replay picker .*exit=124$' "$scratch/picker-budget.log"
check "the picker reports its timeout exit" "$?" 0
! kill -0 "$(cat "$scratch/picker-budget-pid")" 2>/dev/null
check "a budget cut stops the blocking picker child" "$?" 0
(( SECONDS - budget_started <= 3 )); check "the picker cannot take its fixed 180s past the theme budget" "$?" 0
unset -f python3

coh_run_surface() { printf 'UNEXPECTED_START\n'; }
coh_failed=''
coh_deadline=$SECONDS
coh_surface expired > "$scratch/expired-budget.log" 2>&1
coh_picker >> "$scratch/expired-budget.log" 2>&1
check "expired budgets retain both failed surface names" "$? $coh_failed" '0  replay/expired replay/picker'
check "expired budgets skip both workers before setup" "$(grep -c '^COH_SKIPPED ' "$scratch/expired-budget.log") $(grep -c '^UNEXPECTED_START$' "$scratch/expired-budget.log")" '2 0'

# Fix only the budget's logical second; the watchdog still enforces its real one-second timeout.
(
    unset SECONDS
    SECONDS=0
    coh_deadline=8
    coh_bounded own-limit 1 command sleep 60
) > "$scratch/own-limit.log" 2>&1
check "a shorter own timeout remains capped within the theme budget" "$?" 124
grep -q '^COH_SKIPPED replay own-limit cut after 1s (theme remaining 8s)$' "$scratch/own-limit.log"
check "the runner retains an own timeout shorter than the remaining budget" "$?" 0

# Advance only the whole-second clock at worker completion to model crossing a boundary in under one second.
(
    wait() { builtin wait "$@"; local status=$?; SECONDS=$((SECONDS + 1)); return "$status"; }
    coh_deadline=$((SECONDS + 8))
    coh_bounded subsecond 1 true
) > "$scratch/subsecond.log" 2>&1
check "an under-limit success survives a whole-second clock boundary" "$?" 0

coh_run_surface() { trap 'command sleep 60' EXIT; command sleep 60; }
coh_failed=''
coh_deadline=$((SECONDS + 1))
coh_teardown_s=1
budget_started=$SECONDS
coh_surface slow-cleanup > "$scratch/cleanup-budget.log" 2>&1
coh_teardown_s=30
check "a stuck teardown still retains the failed surface name" "$? $coh_failed" '0  replay/slow-cleanup'
grep -q '^COH_TIMING replay slow-cleanup .*exit=137$' "$scratch/cleanup-budget.log"
check "a stuck teardown is forcibly stopped after its grace" "$?" 0
(( SECONDS - budget_started <= 4 )); check "teardown cannot hold the native slot indefinitely" "$?" 0
coh_run_surface() { trap 'exit 0' TERM; command sleep 60; }
coh_failed=''
coh_deadline=$((SECONDS + 1))
coh_surface swallowed-cut > "$scratch/swallowed-budget.log" 2>&1
check "a cleanup that returns success cannot swallow budget expiry" "$? $coh_failed" '0  replay/swallowed-cut'
grep -q '^COH_TIMING replay swallowed-cut .*exit=124$' "$scratch/swallowed-budget.log"
check "a swallowed signal still reports the budget failure" "$?" 0
check "budget runners leave no live watchdog job" "$(jobs -pr)" ''

# Both stand-ins stay in one marked root; only the worker's group belongs to the bounded runner.
group_fixture=$(mktemp -d "$scratch/worker-group.XXXXXXXX") || exit 1
: > "$group_fixture/$SANDBOX_MARKER"
cat > "$group_fixture/ignore-term.sh" <<'SH'
#!/usr/bin/env bash
set -eu
trap - TERM
[[ "$2" != ignore ]] || trap '' TERM
printf '%s\n' "$BASHPID" > "$1.pid"
ps -o pgid= -p "$BASHPID" > "$1.group"
: > "$1.ready"
exec sleep 60
SH
setsid bash "$group_fixture/ignore-term.sh" "$group_fixture/detached" normal > /dev/null 2>&1 < /dev/null &
detached_launcher=$!
replay_ready() {
    local end=$((SECONDS + 3))
    until [[ -f "$1.ready" ]]; do
        (( SECONDS < end )) || fail "process-group stand-in did not become ready"
        command sleep 0.01
    done
}
replay_ready "$group_fixture/detached"
group_worker() {
    printf '%s\n' "$BASHPID" > "$group_fixture/worker.pid"
    bash "$group_fixture/ignore-term.sh" "$group_fixture/child" ignore &
    local child=$!
    replay_ready "$group_fixture/child"
    wait "$child"
}
coh_deadline=$((SECONDS + 8))
coh_teardown_s=1
group_started=$SECONDS
coh_bounded ignored-term 1 group_worker > "$group_fixture/bounded.log" 2>&1
group_status=$?
coh_teardown_s=30
read -r group_child < "$group_fixture/child.pid"
read -r group_leader < "$group_fixture/worker.pid"
read -r child_group < "$group_fixture/child.group"
read -r detached_pid < "$group_fixture/detached.pid"
read -r detached_group < "$group_fixture/detached.group"
check "the TERM-ignoring child belongs to the recorded worker group" "$child_group" "$group_leader"
[[ "$detached_group" == "$detached_pid" && "$detached_group" != "$group_leader" ]]
check "the detached stand-in belongs to its own recorded group" "$?" 0
child_alive=false detached_alive=false
kill -0 "$group_child" 2>/dev/null && child_alive=true
kill -0 "$detached_pid" 2>/dev/null && detached_alive=true
printf 'COH_GROUP_STANDIN limit=1 grace=1 exit=%s child_alive=%s detached_alive=%s\n' "$group_status" "$child_alive" "$detached_alive"
check "teardown removes a TERM-ignoring child after the worker leader exits" "$child_alive" false
check "worker teardown leaves the detached stand-in untouched" "$detached_alive" true
check "worker-group draining preserves the original timeout exit" "$group_status" 124
(( SECONDS - group_started <= 4 )); check "worker-group draining stays bounded" "$?" 0
kill -KILL "$group_child" 2>/dev/null || true
kill -KILL "$detached_pid" 2>/dev/null || true
wait "$detached_launcher" 2>/dev/null || true
detached_launcher=""
group_fixture=""

# An asleep bar wakes only through the wheel, and the actual helper parks in its lane before capture.
replay_wheels=0 replay_awake=false replay_in_lane=false replay_wheel_works=true
coh_pdf_bar=''
ipc() {
    if [[ "$1" == columnFrameRect ]]; then printf '200 40 300 300\n'; return; fi
    jq -cn --argjson exposed "$replay_awake" --argjson hovered "$replay_in_lane" '{captureReader:"porttp4d",controls:[{
        kind:"ViewportScrollBar",name:"Vertical scroll bar",surface:"PreviewColumn",value:"vertical",
        x:488,y:40,w:12,h:300,targetX:494,targetY:115,exposed:$exposed,hovered:$hovered,enabled:true}]}'
}
coh_pointer_to() {
    printf '%s %s\n' "$1" "$2" >> "$scratch/wake-moves.log"
    replay_in_lane=false
    [[ "$*" != '494 115' ]] || replay_in_lane=true
}
omarchy-drive() {
    [[ "$*" == 'scroll down 1' ]] || return 1
    replay_wheels=$((replay_wheels + 1))
    [[ "$replay_wheel_works" != true ]] || replay_awake=true
    return 0
}
coh_wake_colpdf > "$scratch/wake.log"
check "the PDF wakes through exactly one wheel and is held in its own lane" "$? $replay_wheels $replay_in_lane" '0 1 true'
check "the wheel targets the PDF body before the actual knob" "$(tr '\n' '|' < "$scratch/wake-moves.log")" '350 190|494 115|'
grep -q '^COH_WAKE replay cols-pdf bar=true after=[0-9]' "$scratch/wake.log"
check "a real wake prints its exposed state and elapsed milliseconds" "$?" 0
grep -q 'frame="200 40 300 300" found=true reader=porttp4d' "$scratch/wake.log"
check "a wake receipt records the real frame, discovery and reader revision" "$?" 0
coh_wake_colpdf > "$scratch/wake-held.log"
check "a pointer held in the lane needs no extra wheel between hover and press" "$? $replay_wheels" '0 1'
replay_awake=false replay_in_lane=false
coh_wake_colpdf > "$scratch/wake-again.log"
check "a bar that sleeps again is reawakened before another capture" "$? $replay_wheels $replay_in_lane" '0 2 true'
replay_awake=false replay_in_lane=false replay_wheel_works=false
( coh_wait_s=0; coh_wake_colpdf ) > "$scratch/wake-refused.log" 2>&1
check "a wheel that leaves the bar asleep cannot satisfy target discovery" "$?" 1
grep -q '^COH_WAKE replay cols-pdf bar=false after=' "$scratch/wake-refused.log"
check "the unsuccessful wake prints a decisive non-exposed receipt" "$?" 0
( ipc() { if [[ "$1" == columnFrameRect ]]; then printf '200 40 300 300\n'; else printf '{"controls":[]}\n'; fi; }; coh_wake_colpdf ) > "$scratch/stale-reader.log" 2>&1
check "an older capture reader is refused before the wheel" "$?" 1
grep -q 'frame="200 40 300 300" found=false reader=missing' "$scratch/stale-reader.log"
check "an older reader gets a distinct missing-reader diagnosis" "$?" 0
printf 'CAPTUREPORT DONE checks=%s failed=%s\n' "$checks" "$failed"
[[ "$failed" == 0 ]]
