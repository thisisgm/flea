#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
flea_ipc_init qs
test_frontend=qml
# tests/ui.sh and tests/ui-captures-dropmods.sh window helpers against hyprctl, omarchy-drive and ipc doubles: geometry, focus, keys and the cursor seek are per qs pid, and a lookup or a key that fails ends the case.
set -u
cd "$(dirname "$0")/.." || exit 1
repo=$(pwd)

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

# The helpers, the focus readers, the keys and the pid route, cut out of the files that ship them.
eval "$(sed -n -e '/^cap_dropmods_box()/,/^}/p' -e '/^cap_dropmods_focus_pid()/,/^}/p' tests/ui-captures-dropmods.sh)"
eval "$(sed -n -e '/^ipc_pid=""$/,/^}$/p' -e '/^focused_class()/,/^}/p' -e '/^focused_pid()/,/^}/p' -e '/^window_addr_of_pid()/,/^}/p' \
    -e '/^assert_focus()/,/^}/p' -e '/^key()/,/^}/p' -e '/^hotkey()/,/^}/p' -e '/^seek_row_named()/,/^}/p' tests/ui.sh)"

# Sample input, hyprctl clients -j: [{"class":"com.thisisgm.flea","pid":11,"address":"0xa1","at":[0,40],"size":[900,700]}], one entry per window.
clients='[{"class":"com.thisisgm.flea","pid":11,"address":"0xa1","at":[0,40],"size":[900,700]},{"class":"com.thisisgm.flea","pid":22,"address":"0xb2","at":[900,40],"size":[950,700]},{"class":"other","pid":33,"address":"0xc3","at":[1,1],"size":[2,2]}]'
active=11
active_class=com.thisisgm.flea
# The newest window answers the path route whichever one has focus, as two windows over one ShellId do.
route_reply=/two
flea_ui=/none
flea_window_class=com.thisisgm.flea
# ambiguous answers a class match with the driver's refusal, as two Flea windows do; refuse_key refuses one key outright.
ambiguous=no
refuse_key=""
rows=(a.txt b1.txt b2.txt sub)
hyprctl() {
    case "$*" in
        "clients -j") printf '%s\n' "$clients" ;;
        "activewindow -j") printf '{"class":"%s","pid":%s}\n' "$active_class" "$active" ;;
        *'focus({ window = "address:0x'*) printf 'ok\n' ;;
        *) return 1 ;;
    esac
}
. "$repo/tests/lib/hypr-dispatch.sh" || exit 1
sleep() { :; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
# The listing answers a seek reads: the cursor is the j keys the driver took since its last g.
rows_reply() {
    local cursor=0 key
    while IFS= read -r key; do
        case $key in g) cursor=0 ;; j) cursor=$((cursor + 1)) ;; esac
    done < "$box/keys.ok"
    case "$1" in
        total) printf '%s\n' "${#rows[@]}" ;;
        cursor) printf '%s\n' "$cursor" ;;
        rowAt) if [ "$2" -lt "${#rows[@]}" ]; then printf '%s|file\n' "${rows[$2]}"; else printf 'loading\n'; fi ;;
        visibleRowName) printf '\n' ;;
        *) return 1 ;;
    esac
}
# Records every argv; a --window com.thisisgm.flea key is refused with exit 3 and the driver's own text while two windows match.
omarchy-drive() {
    printf '%s\n' "$*" >> "$box/argv"
    case "$1" in
        ipc)
            shift 4
            if [ "$1" = path ]; then printf '%s\n' "$route_reply"; else rows_reply "$@"; fi ;;
        key)
            [ "$2" = --window ] || return 1
            if [ "$3" = com.thisisgm.flea ] && [ "$ambiguous" = yes ]; then
                printf "omarchy-drive: 'com.thisisgm.flea' matches 2 windows: com.thisisgm.flea,com.thisisgm.flea, narrow it\n" >&2
                return 3
            fi
            case "$3" in com.thisisgm.flea | 0xa1 | 0xb2) ;; *) return 1 ;; esac
            if [ "$4" = "$refuse_key" ]; then printf 'omarchy-drive: key refused\n' >&2; return 3; fi
            printf '%s\n' "$4" >> "$box/keys.ok" ;;
        hotkey) ;;
        *) return 1 ;;
    esac
}
xw_ipc() {
    case "$1/$2" in
        11/path) printf '/one\n' ;;
        22/path) printf '/two\n' ;;
        22/*) shift; rows_reply "$@" ;;
        *) return 1 ;;
    esac
}
reset_drive() { : > "$box/argv"; : > "$box/keys.ok"; }

out=$(cap_dropmods_box 22 2>&1)
[ "$out" = "900	40	950	700" ] && ok "the box is read for the qs pid asked for" || bad "box 22 must read the second window, got: $out"

out=$(cap_dropmods_box 99 2>&1)
[ "$out" = "FAIL: cap_dropmods: no native geometry for qs pid 99" ] && ok "a pid with no client fails by name" || bad "an unknown pid must fail by name, got: $out"

clients='[{"class":"com.thisisgm.flea","pid":11,"address":"0xa1","at":[0.5,40],"size":[900,700]}]'
out=$(cap_dropmods_box 11 2>&1)
[ "$out" = "FAIL: cap_dropmods: no native geometry for qs pid 11" ] && ok "a fractional geometry is no geometry" || bad "a fractional box must fail, got: $out"
clients='[{"class":"com.thisisgm.flea","pid":11,"address":"0xa1","at":[0,40],"size":[900,700]},{"class":"com.thisisgm.flea","pid":22,"address":"0xb2","at":[900,40],"size":[950,700]},{"class":"other","pid":33,"address":"0xc3","at":[1,1],"size":[2,2]}]'

# A caller's own read, as the case writes it: a failed lookup must end it, not hand it empty numbers.
site() { local wx wy ww wh; read -r wx wy ww wh < <(cap_dropmods_box "$1") || exit 1; printf 'reached %s\n' "$wx"; }
out=$( (site 99) 2>&1)
[ "$out" = "FAIL: cap_dropmods: no native geometry for qs pid 99" ] && ok "a caller reading a failed box stops there" || bad "a failed box must end the caller, got: $out"
[ -z "$(grep -n '<<< "\$(cap_dropmods_box' tests/ui-captures-dropmods.sh)" ] \
    && ok "no caller reads a box through a here-string, which swallows the failure" || bad "a here-string caller still hides a failed box"

active=11
ipc_pid=""
out=$( { cap_dropmods_focus_pid 11 /one && printf 'landed pid=%s path=%s\n' "$ipc_pid" "$(ipc path)"; } 2>&1)
[ "$out" = "landed pid=11 path=/one" ] && ok "the older window is read through its own pid, not the newest" || bad "focusing window 11 must read /one, got: $out"

active=22
out=$( { cap_dropmods_focus_pid 22 /two && printf 'landed pid=%s path=%s\n' "$ipc_pid" "$(ipc path)"; } 2>&1)
[ "$out" = "landed pid=22 path=/two" ] && ok "the newer window is read through its own pid" || bad "focusing window 22 must read /two, got: $out"

active=22
out=$(cap_dropmods_focus_pid 11 /one 2>&1)
[ "$out" = "FAIL: cap_dropmods: focus did not land on qs pid 11 (the active window is 22)" ] \
    && ok "a focus that stays on the other window fails by pid" || bad "a focus that did not land must fail, got: $out"

active=11
out=$(cap_dropmods_focus_pid 11 /two 2>&1)
[ "$out" = "FAIL: cap_dropmods: qs pid 11 does not show /two (ipc says /one)" ] \
    && ok "a window showing another path fails by pid" || bad "a wrong path must fail, got: $out"

out=$(cap_dropmods_focus_pid 99 /one 2>&1)
[ "$out" = "FAIL: no client address for qs pid 99" ] \
    && ok "a pid with no client ends the focus on one failure line" || bad "an unknown pid must end the focus alone, got: $out"

ipc_pid=""
[ "$(ipc path)" = "$route_reply" ] && ok "with no pid set ipc takes the path route" || bad "an empty ipc_pid must take the path route"

# Keys: two windows share a class, so a pinned pid names its window by address and the focus check reads the pid.
ambiguous=yes
active=22
ipc_pid=22
reset_drive
out=$(seek_row_named b1.txt 2>&1)
[ -z "$out" ] && [ "$(cat "$box/argv")" = "key --window 0xb2 g
key --window 0xb2 j" ] \
    && ok "a pinned pid seeks its row through its own address while the class matches two windows" || bad "a pinned pid must reach b1.txt with --window 0xb2, got: $out / $(paste -sd'|' "$box/argv")"

reset_drive
active=11
out=$(seek_row_named b1.txt 2>&1)
[ "$out" = "FAIL: focus is on qs pid '11', not 22, so this case sends no more keys" ] && [ ! -s "$box/argv" ] \
    && ok "a pinned pid refuses a key while another Flea window has focus" || bad "focus on the other window must refuse the key, got: $out / $(paste -sd'|' "$box/argv")"

reset_drive
active=22
ipc_pid=99
out=$(seek_row_named b1.txt 2>&1)
[ "$out" = "FAIL: focus is on qs pid '22', not 99, so this case sends no more keys" ] && [ ! -s "$box/argv" ] \
    && ok "a pid that is not the focused window sends nothing" || bad "a wrong pinned pid must send nothing, got: $out"

ipc_pid=22
reset_drive
refuse_key=j
out=$(seek_row_named b1.txt 2>&1)
[ "$out" = "omarchy-drive: key refused
FAIL: seek_row_named: key j was refused on the way to b1.txt, so no key can move the cursor" ] \
    && [ "$(grep -c '^key ' "$box/argv")" = 2 ] \
    && ok "a refused key ends the seek at once and names the key and the row" || bad "a refused j must end the seek, got: $out / $(paste -sd'|' "$box/argv")"
refuse_key=""

# With no pid set the calls are today's, and a refusal is no longer forty silent tries.
ipc_pid=""
active=11
ambiguous=no
reset_drive
out=$(seek_row_named b1.txt 2>&1)
[ -z "$out" ] && [ "$(grep '^key ' "$box/argv")" = "key --window com.thisisgm.flea g
key --window com.thisisgm.flea j" ] \
    && ok "with no pid set the seek sends today's class route" || bad "an unset pid must keep the --window com.thisisgm.flea route, got: $out / $(paste -sd'|' "$box/argv")"

ambiguous=yes
reset_drive
out=$(seek_row_named b1.txt 2>&1)
[ "$out" = "omarchy-drive: 'com.thisisgm.flea' matches 2 windows: com.thisisgm.flea,com.thisisgm.flea, narrow it
FAIL: seek_row_named: key g was refused on the way to b1.txt, so no key can move the cursor" ] && [ "$(grep -c '^key ' "$box/argv")" = 1 ] \
    && ok "an ambiguous class ends the seek on its first key, not after forty tries" || bad "an ambiguous class must end the seek at once, got: $out / $(grep -c '^key ' "$box/argv") keys"

active_class=other
out=$(assert_focus 2>&1)
[ "$out" = "FAIL: focus is on class 'other', not com.thisisgm.flea, so this case sends no more keys" ] \
    && ok "with no pid set the focus check reads the class" || bad "an unset pid must check the class, got: $out"
active_class=com.thisisgm.flea

ipc_pid=99
out=$( (key g) 2>&1)
[ "$out" = "FAIL: focus is on qs pid '11', not 99, so this case sends no more keys" ] \
    && ok "the focus check comes before any address lookup" || bad "key must check focus first, got: $out"
ipc_pid=22
active=22
reset_drive
clients='[{"class":"com.thisisgm.flea","pid":11,"address":"0xa1","at":[0,40],"size":[900,700]}]'
out=$( (key g) 2>&1)
[ "$out" = "FAIL: no client address for qs pid 22" ] && [ ! -s "$box/argv" ] \
    && ok "a pinned pid whose window is gone sends no key" || bad "a pid with no client must send no key, got: $out"
clients='[{"class":"com.thisisgm.flea","pid":11,"address":"0xa1","at":[0,40],"size":[900,700]},{"class":"com.thisisgm.flea","pid":22,"address":"0xb2","at":[900,40],"size":[950,700]},{"class":"other","pid":33,"address":"0xc3","at":[1,1],"size":[2,2]}]'

# hotkey takes no --window, so a pinned pid is held to its focus check and the call is otherwise today's.
reset_drive
active=11
out=$( (hotkey --global ctrl t flea) 2>&1)
[ "$out" = "FAIL: focus is on qs pid '11', not 22, so this case sends no more keys" ] && [ ! -s "$box/argv" ] \
    && ok "a pinned pid refuses a hotkey while another Flea window has focus" || bad "a hotkey on the wrong window must be refused, got: $out"
active=22
hotkey --global ctrl t flea
[ "$(cat "$box/argv")" = "hotkey --global ctrl t flea" ] \
    && ok "a hotkey on the pinned window is sent as written" || bad "the hotkey argv must be unchanged, got: $(paste -sd'|' "$box/argv")"
ipc_pid=""

# Every glide ends with a 1 px out-and-back from inside the target, so the crossing move is never the last motion.
. tests/lib/pointer-glide.sh
glide_pos="0, 0"
hyprctl() {
    case "$*" in
        "cursorpos") printf '%s\n' "$glide_pos" ;;
        "clients -j") printf '%s\n' "$clients" ;;
        "activewindow -j") printf '{"class":"%s","pid":%s}\n' "$active_class" "$active" ;;
        *) return 1 ;;
    esac
}
ydotool() {
    printf '%s\n' "$*" >> "$box/glide-argv"
    # The first move crosses into the target, as libinput's gain does mid-drag.
    glide_pos="100, 100"
    return 0
}
glide_red() { pointer_glide 100 100; }
: > "$box/glide-argv"
glide_red
[ "$(tail -n 2 "$box/glide-argv")" = "$(printf 'mousemove -x 1 -y 0\nmousemove -x -1 -y 0')" ] \
    && ok "a glide landing on its crossing move still ends with the out-and-back" \
    || bad "the argv log must end with the 1 px out-and-back, got: $(paste -sd'|' "$box/glide-argv")"

reset_drive
out=$( (test_frontend=gpui; ipc_pid=""; key g) 2>&1)
[ "$out" = 'FAIL: native input has no owned PID' ] && [ ! -s "$box/argv" ] \
    && ok "the native route refuses input before sending a key without an owned PID" \
    || bad "unowned native input must be refused without a key, got: $out"

[ "$failed" = 0 ] || exit 1
printf 'DROPMODSWINDOWS DONE\n'
