#!/bin/bash
# Headless guard for what an external drop target sees, unlike tests/drag.sh which needs a display: plain offers copy alone since a browser uploader refuses a move, Ctrl copy, Shift move, Ctrl with Shift link, shelf copy only, a tab drag Move alone with only the private tab type.
set -u
cd "$(dirname "$0")/.." || exit 1

pass=0
fail=0
ok()  { printf 'ok   %s\n' "$*"; pass=$((pass+1)); }
bad() { printf 'FAIL %s\n' "$*"; fail=$((fail+1)); }

# Comments may name an action, so every check reads code only. Sample input: Drag.supportedActions: Qt.MoveAction // explanatory comment, which is ignored.
code_of() { sed -e 's://.*::' "$1"; }

# Sample input, ui/FileDrag.qml:27: '    Drag.supportedActions: root.dragLink ? Qt.LinkAction : ...'
advertised=$(while IFS= read -r f; do
    code_of "$f" | grep -H --label="$f" -n 'Drag\.supportedActions'
done < <(find ui -type f -name '*.qml'))

advertiser_files=$(printf '%s\n' "$advertised" | cut -d: -f1 | sort -u)
expected_advertisers=$(printf '%s\n' ui/FileDrag.qml ui/TabBar.qml | sort -u)
if [[ "$advertiser_files" == "$expected_advertisers" ]]; then
    ok "only ui/FileDrag.qml and ui/TabBar.qml advertise drag actions: one view per drag kind"
else
    bad "unexpected Drag.supportedActions files: $advertiser_files"
fi

# Each advertiser holds exactly one line, so a second offer in either file cannot hide behind the set check above.
file_lines=$(printf '%s\n' "$advertised" | grep -c '^ui/FileDrag.qml:')
tab_lines=$(printf '%s\n' "$advertised" | grep -c '^ui/TabBar.qml:')
if [ "$file_lines" -eq 1 ] && [ "$tab_lines" -eq 1 ]; then
    ok "exactly one Drag.supportedActions line in each of ui/FileDrag.qml and ui/TabBar.qml"
else
    bad "expected one Drag.supportedActions line in each advertiser, found FileDrag=$file_lines TabBar=$tab_lines"
fi

# Sample input: "Drag.supportedActions: root.dragLink ? Qt.LinkAction : Qt.CopyAction"; extract the whole offer.
offer=$(code_of ui/FileDrag.qml | grep 'Drag\.supportedActions:' | sed 's/.*Drag\.supportedActions:[[:space:]]*//')
[ -n "$offer" ] || bad "no Drag.supportedActions line left in ui/FileDrag.qml to pin"
# Sample input: "root.dragShift ? Qt.MoveAction : Qt.CopyAction;" ends with the plain copy arm.
final=$(printf '%s\n' "$offer" | sed 's/.*://;s/[[:space:];]//g')
if [ "$final" = "Qt.CopyAction" ]; then
    ok "a plain lift offers copy alone"
else
    bad "a plain lift must end on Qt.CopyAction alone, got: $final"
fi
# Sample input: root.dragLink ? Qt.LinkAction : root.dragCopy ? Qt.CopyAction : root.dragShift ? Qt.MoveAction : Qt.CopyAction
offer_seq=$(printf '%s\n' "$offer" | tr -d '[:space:];' | sed -e 's/root\.//g' -e 's/?/ /g' -e 's/:/;/g')
# Link precedes copy because a link lift carries ctrl, so order decides the verb.
expected_seq='dragLink Qt.LinkAction;dragCopy Qt.CopyAction;dragShift Qt.MoveAction;Qt.CopyAction'
if [ "$offer_seq" = "$expected_seq" ]; then
    ok "the offer narrows arm by arm: link alone, Ctrl copy alone, Shift move alone, plain copy alone"
else
    bad "the offer must read $expected_seq, got: $offer_seq"
fi

# FileDrag and Drag.js offer uri-list; DropInto, RowDrag and the grid's one shared area consume it, with no other ui users.
uri_files=$(while IFS= read -r f; do
    code_of "$f" | grep -H --label="$f" 'text/uri-list'
done < <(find ui -type f \( -name '*.qml' -o -name '*.js' \)) | cut -d: -f1 | sort -u)
expected_uri_files=$(printf '%s\n' ui/DropInto.qml ui/FileDrag.qml ui/GridArea.qml ui/RailDrop.qml ui/RowDrag.qml ui/js/Drag.js | sort -u)
if [[ "$uri_files" == "$expected_uri_files" ]]; then
    ok "uri-list is confined to the file payload producers and their four receivers"
else
    bad "unexpected text/uri-list files: $uri_files"
fi

# Exactly one file-drag advertiser of copy, the file lift, plus the tab drag's own Move.
copy_files=$(printf '%s' "$advertised" | grep 'CopyAction' | cut -d: -f1 | sort -u)
if [ "$(printf '%s' "$copy_files" | grep -c .)" -eq 1 ] && [ "$copy_files" = "ui/FileDrag.qml" ]; then
    ok "exactly one file-drag advertiser of copy: ui/FileDrag.qml"
else
    bad "expected the one copy advertiser to be ui/FileDrag.qml alone, found: $(printf '%s' "$copy_files" | tr '\n' ' ')"
fi
move_line=$(printf '%s' "$advertised" | grep '^ui/TabBar.qml' | cut -d: -f3-)
if printf '%s' "$move_line" | grep -q 'Drag\.supportedActions:[[:space:]]*Qt\.MoveAction[[:space:]]*$'; then
    ok "the tab drag advertises Move alone"
else
    bad "the tab drag must advertise Qt.MoveAction alone, got: $move_line"
fi

# Qt hands effectAllowed from this expression; combined Copy and Move violates the plain offer.
scratch=$(mktemp -d) || exit 1
trap 'rm -rf "$scratch"' EXIT
# Load the exact component outside ui's qmldir, which eagerly imports unrelated Quickshell singletons.
cp ui/FileDrag.qml "$scratch/FileDrag.qml" || exit 1
ln -s "$PWD/ui/js" "$scratch/js" || exit 1
offer_timeout_seconds=15
file_offer=$(env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 timeout "$offer_timeout_seconds" qml6 tests/dragwire-offer.qml -- "$scratch/FileDrag.qml" 2>&1)
offer_status=$?
if [[ "$offer_status" == 0 ]] && grep -q 'file offers: 5 checks, 0 failed' <<< "$file_offer"; then
    ok "file lift offers copy for plain, ctrl and ctrl plus shift without link; shift offers move and link takes priority"
else
    bad "file lift offers failed (status=$offer_status): $file_offer"
fi

# Exercise the shipped floor bindings and handler while a listing is held and after it settles.
floor_probe_seconds=15
floor_output=$(env QML_XHR_ALLOW_FILE_READ=1 QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
    timeout "$floor_probe_seconds" qml6 tests/dragwire-floor.qml 2>&1)
floor_status=$?
if [[ "$floor_status" == 0 ]] && grep -q 'floor drops: 56 checks, 0 failed' <<< "$floor_output"; then
    ok "list, grid and columns floors refuse held listings, target the shown directory after settlement and follow the held keys"
else
    bad "floor drops failed (status=$floor_status): $floor_output"
fi

# The Move-alone advertiser is the tab drag: it carries the private type alone, since Files moves a folder whenever Move is offered.
if grep -q 'text/uri-list' ui/TabBar.qml; then
    bad "the tab drag must not offer text/uri-list with Move"
else
    ok "no tab drag offers text/uri-list with Move"
fi
if code_of ui/js/Tabs.js | grep -q 'text/uri-list'; then
    bad "the tab payload must not offer text/uri-list"
else
    ok "the tab payload carries only the private tab type"
fi

if grep -q 'text/uri-list' ui/js/Drag.js; then
    ok "a leaving file drag still offers text/uri-list"
else
    bad "text/uri-list is gone from ui/js/Drag.js"
fi

# The shelf is a copy offer of its own and is not the leaving-file drag.
shelf=$(code_of shelf/ShelfCard.qml | grep -n 'Drag\.supportedActions')
if printf '%s' "$shelf" | grep -q 'Drag\.supportedActions:[[:space:]]*Qt\.CopyAction[[:space:]]*$'; then
    ok "the shelf drag stays a copy offer"
else
    bad "the shelf drag must stay Qt.CopyAction alone, got: $shelf"
fi

# Own verb rides the marker: proposedAction reaches only the Drag.js helper plus pass-through call sites (dropInto, dropVerb, feedbackFor, enterTarget, dropped).
if ! grep -q '^function foreignHeld' ui/js/Drag.js || ! grep -q '^function dropVerb' ui/js/Drag.js; then
    bad "ui/js/Drag.js must hold the single proposedAction helper (foreignHeld and dropVerb)"
else
    ok "the single proposedAction helper lives in ui/js/Drag.js"
fi
if ! grep -q 'dropVerb(marker, proposed' ui/js/Drag.js; then
    bad "dropInto must choose its verb through dropVerb"
else
    ok "dropInto chooses its verb through dropVerb"
fi
side=$(for f in ui/*.qml ui/js/*.js; do code_of "$f" | grep -H --label="$f" -n 'proposed'; done)
badside=""
# Sample input: the proposed-action scan emits "ui/DropInto.qml:88: Drag.dropInto(drop.proposedAction)".
while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    # Sample input: "ui/DropInto.qml:88: Drag.dropInto(drop.proposedAction)" names ui/DropInto.qml.
    file=$(printf '%s' "$hit" | cut -d: -f1)
    # Sample input: "ui/DropInto.qml:88: Drag.dropInto(drop.proposedAction)" keeps the code after field 2.
    text=$(printf '%s' "$hit" | cut -d: -f3-)
    case "$file" in
        ui/js/Drag.js) continue ;;
        ui/DropInto.qml|ui/RowDrag.qml|ui/FileDrag.qml|ui/RailDrop.qml|ui/js/RailDrop.js)
            # Pass-through only (offer setting and argument forwarding); a bitwise read or comparison here would decide the verb outside the helper.
            case "$text" in
                *"&"*|*"=="*|*"!="*) ;;
                *) continue ;;
            esac
            ;;
    esac
    badside="${badside}${hit}
"
done <<< "$side"
if [ -z "$badside" ] && [ -n "$side" ]; then
    ok "proposedAction reaches only the helper and its pass-through call sites"
else
    bad "the internal verb is back on proposedAction outside the helper:"
    printf '%s\n' "${badside:-$side}" | sed 's/^/     /'
fi
# Helper ignores proposed for any Flea marker: no bitwise proposed read outside foreignHeld.
bites=$(grep -n 'proposed &' ui/js/Drag.js)
# Sample input: "9:function foreignHeld(proposed) {" starts the helper at line 9.
start=$(grep -n '^function foreignHeld' ui/js/Drag.js | cut -d: -f1)
if [ -z "$start" ]; then
    bad "ui/js/Drag.js has no ^function foreignHeld line, so the single-helper range is unbounded"
fi
finish=""
# Sample input: helper boundaries arrive as one line number per line, "9\n16\n".
while IFS= read -r n; do
    if [ "$n" -gt "${start:-0}" ]; then
        finish=$n
        break
    fi
    # Sample input: "16:function dropVerb(marker, proposed) {" supplies the next function's line number.
done <<< "$(grep -n '^function ' ui/js/Drag.js | cut -d: -f1)"
if [ -z "$finish" ]; then
    finish=$(($(wc -l < ui/js/Drag.js) + 1))
fi
outside=""
# Sample input: the bitwise scan emits "10: return proposed & Qt.CopyAction".
while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    # Sample input: "10: return proposed & Qt.CopyAction" locates the read at line 10.
    n=$(printf '%s' "$hit" | cut -d: -f1)
    if [ -n "$start" ] && [ -n "$finish" ] && [ "$n" -ge "$start" ] && [ "$n" -lt "$finish" ]; then
        continue
    fi
    outside="${outside}${hit}
"
done <<< "$bites"
if [ -z "$outside" ] && [ -n "$bites" ]; then
    ok "only foreignHeld reads the proposedAction bits"
else
    bad "proposedAction bits are read outside foreignHeld lines $start-$finish:"
    printf '%s\n' "${outside:-no bitwise read left to place}" | sed 's/^/     /'
fi

# The shortened bound, and the fewest stub calls that show the wait kept polling.
short_wait_ns=300000000
min_poll_calls=2
# Sample input: "xwdrag_wait_row_gone() {" and "wait_ns=10000000000"; extract the wait and count guard, then shorten the bound.
eval "$(sed -n '/^xwdrag_count()/,/^}/p;/^xwdrag_wait_row_gone()/,/^}/p' tests/ui.sh | sed "s/wait_ns=[0-9][0-9]*/wait_ns=$short_wait_ns/")"
# A stub qs that fails every call, so the wait must keep polling to the bound.
xwdrag_qs() {
    printf 'call\n' >&3
    return 255
}
calls=$(
    {
        xwdrag_wait_row_gone stub-id "move.txt" >/dev/null 2>&1
        printf 'rc=%s\n' "$?"
    } 3>&1
)
# Sample input: "call\ncall\nrc=1\n" reports exit status 1 after two failed polls.
wait_rc=$(printf '%s\n' "$calls" | sed -n 's/^rc=//p')
poll_calls=$(printf '%s\n' "$calls" | grep -c '^call$')
# A wait that saw no row and no total answers 1 only after polling for it.
if [ "$wait_rc" -eq 1 ] && [ "$poll_calls" -ge "$min_poll_calls" ]; then
    ok "a failing total call keeps waiting and answers 1 at the bound"
else
    bad "a failing total call must wait and answer 1, got rc=$wait_rc calls=$poll_calls"
fi

# The catcher must never steal focus and release the platform drag's held button.
if code_of ui/boot/tabtearoff.qml | grep -q 'WlrLayershell.keyboardFocus: WlrKeyboardFocus.None' \
    && ! code_of ui/boot/tabtearoff.qml | grep -Eq 'focus:|Keys\.|forceActiveFocus|WlrKeyboardFocus\.(OnDemand|Exclusive)'; then
    ok "tear-off catcher never requests keyboard focus"
else
    bad "tear-off catcher must use None without a focused Escape item"
fi
# A sibling source bypasses QQuickDropArea's ancestor rejection (QTBUG-64128).
if code_of ui/TabBar.qml | grep -q 'Drag.source: dragOrigin' \
    && code_of ui/TabBar.qml | grep -q 'Item { id: dragOrigin; width: 0; height: 0; visible: false }'; then
    ok "tab drag source is an invisible sibling of the strip DropArea"
else
    bad "tab drag source must not be the strip DropArea's ancestor"
fi

if python3 -B tests/drag-read-check.py; then
    ok "the receiver's asynchronous reads pass the headless self-check"
else
    bad "the receiver's asynchronous read self-check failed"
fi

# The failed-read evidence line and the after-drop line of tests/drag.sh, read against stub observers (that suite itself needs the display).
evidence_pid=4242
# Sample input: tests/drag.sh function bodies, each from its name line to the closing brace on a line of its own.
eval "$(sed -n '/^evidence_json()/,/^}/p;/^expect_evidence()/,/^}/p;/^after_drop_line()/,/^}/p' tests/drag.sh)"
MYPID=$evidence_pid
# Sample input, ipc statusActivityState: {"activities":[{"id":0,"text":"Copy 2 items to a","running":false}],"errors":0,"notice":""}
ipc() {
    case "$1" in
        statusActivityState) printf '%s\n' '{"activities":[{"id":0,"text":"Copy 2 items to a","running":false}],"errors":0,"notice":"","undoAvailable":false}' ;;
        dualState) printf '%s\n' '{"active":false,"focused":0,"panes":[{"path":"/home/p"}]}' ;;
        path) printf '%s\n' /home/p ;;
        tabCount) printf '%s\n' 2 ;;
        tabIndex) printf '%s\n' 1 ;;
        collideState) return 3 ;;
        *) printf '\n' ;;
    esac
}
# Sample input, hyprctl clients -j: [{"pid": 4242, "address": "0xabc", "at": [12, 42], "size": [2536, 1386], "floating": false, "focusHistoryID": 0}]
hyprctl() {
    case "$1" in
        clients) printf '%s\n' '[{"pid":4242,"address":"0xabc","at":[12,42],"size":[2536,1386],"floating":false,"focusHistoryID":0},{"pid":7,"address":"0xdef"}]' ;;
        activewindow) printf '%s\n' '{"address":"0xabc","class":"com.thisisgm.flea","title":"x"}' ;;
    esac
}
evidence=$(expect_evidence lastMessage 'Copied 2 items · z undoes' '')
# Sample input: 'DRAG_EXPECT_FAIL {"reader":"lastMessage",...}', one line, JSON after the prefix.
if [ "$(printf '%s\n' "$evidence" | wc -l)" -eq 1 ] && [ "${evidence%% *}" = DRAG_EXPECT_FAIL ] \
    && printf '%s\n' "${evidence#DRAG_EXPECT_FAIL }" | jq -e '
        .reader == "lastMessage" and .observed == ""
        and .statusActivityState.notice == "" and .statusActivityState.errors == 0
        and (.statusActivityState.activities | length) == 1
        and .path == "/home/p" and .tabCount == 2 and .tabIndex == 1 and .dualState.focused == 0
        and (.collideState | test("observer exit 3"))
        and (.clients | length) == 1 and .clients[0].address == "0xabc" and .clients[0].floating == false
        and .clients[0].focus == 0 and .clients[0].at == [12, 42] and .clients[0].size == [2536, 1386]
        and .activeWindow.class == "com.thisisgm.flea"' >/dev/null; then
    ok "a failed read prints one JSON line with the transfer, status, tab, pane and window facts"
else
    bad "the failed-read evidence is not one complete JSON line: $evidence"
fi
after=$(after_drop_line)
# Sample input: 'DRAG_R9_AFTER_DROP {"notice":"","errors":0,"running":[false],"currentPane":0,...}'.
if [ "$(printf '%s\n' "$after" | wc -l)" -eq 1 ] && [ "${after%% *}" = DRAG_R9_AFTER_DROP ] \
    && printf '%s\n' "${after#DRAG_R9_AFTER_DROP }" | jq -e '
        .notice == "" and .errors == 0 and .running == [false] and .currentPane == 0
        and .path == "/home/p" and .tabIndex == "1"' >/dev/null; then
    ok "the after-drop line carries the notice and the current pane"
else
    bad "the after-drop line is not one complete JSON line: $after"
fi
# Sample input: expect_ipc's body, whose two die calls (observer failure, mismatch) each follow the evidence call.
dies=0
unprinted=0
previous=
while IFS= read -r body_line; do
    if [[ "$body_line" == *'die "'* ]]; then
        dies=$((dies + 1))
        [[ "$previous" == *expect_evidence* || "$body_line" == *expect_evidence* ]] || unprinted=$((unprinted + 1))
    fi
    previous=$body_line
done < <(sed -n '/^expect_ipc()/,/^}/p' tests/drag.sh)
if [ "$dies" -eq 2 ] && [ "$unprinted" -eq 0 ]; then
    ok "expect_ipc prints the evidence line before each of its two failures"
else
    bad "expect_ipc must call expect_evidence before each die"
fi
# Sample input: the lines around the call in cross_view_pair, release then the line then the file wait.
around=$(sed -n '/^cross_view_pair()/,/^}/p' tests/drag.sh | grep -B1 -A1 -x '[[:space:]]*after_drop_line' | sed 's/^[[:space:]]*//')
if [ "$around" = $'release; ctrl_up\nafter_drop_line\npair_result "$source" "$drop" "$name" Copy' ]; then
    ok "the after-drop line is read right after the release and before the file wait"
else
    bad "after_drop_line must sit between the commit release and its pair_result, got: $around"
fi

# Sample input, tests/drag.sh: cleanup_drop_events='wl_data_(device|source)#[0-9]+\.(drop|...)'.
eval "$(grep '^cleanup_drop_events=' tests/drag.sh)"
# Sample input, WAYLAND_DEBUG: '[06:32:03.100000] {Default Queue} wl_data_source#80.dnd_finished()' and the request 'wl_data_device#6.start_drag(...)'.
drop_events=$(printf '%s\n' '[1.0] {Default Queue} wl_data_device#6.drop()' '[1.1] {Default Queue} wl_data_source#80.dnd_finished()' \
    '[1.2] {Default Queue} wl_data_source#80.cancelled()' '[1.3] {Default Queue}  -> wl_data_device#6.start_drag(wl_data_source#80)' \
    | grep -c -E "${cleanup_drop_events:-unset}")
if [ "$drop_events" -eq 3 ]; then
    ok "the teardown keeps the compositor's drop, finished and cancelled events and not the lift request"
else
    bad "cleanup_drop_events must match drop, dnd_finished and cancelled only, matched $drop_events of 3"
fi

# A row or tab centre is awaited through one helper, and a bare substitution of it is the unbound $1 that ended R3 silently.
bare=$(grep -n -E 'set -- \$\(screen_(tab_)?centre|point=\$\(screen_(tab_)?centre' tests/drag.sh)
if [ -z "$bare" ]; then
    ok "no pointer position is read from a bare screen_centre or screen_tab_centre substitution"
else
    bad "a pointer position bypasses centre_into and tab_centre_into:"
    printf '%s\n' "$bare" | sed 's/^/     /'
fi

# The centre helper of tests/drag.sh against doubles whose rowCentre answers late, or never (that suite needs the display).
centre_tmp=$(mktemp -d)
[ -n "$centre_tmp" ] && [ "${centre_tmp#/}" != "$centre_tmp" ] || { bad "no scratch root for the centre doubles"; exit 1; }
printf 'marker\n' > "$centre_tmp/marker"
# Sample input: tests/drag.sh function bodies, each from its name line to the closing brace on a line of its own, and one-line wrappers.
eval "$(sed -n '/^rowidx()/,/^}/p;/^screen_centre()/,/^}/p;/^screen_tab_centre()/,/^}/p;/^centre_fail_line()/,/^}/p;/^await_centre()/,/^}/p;/^centre_into()/p;/^tab_centre_into()/p;/^die()/p' tests/drag.sh)"
# The poll bound is shrunk and its gap removed so the never-answering case ends at once.
centre_poll_attempts=4
centre_poll_seconds=0
WX=12; WY=42; WW=2560; WH=1440
centre_mode=late
printf '0\n' > "$centre_tmp/calls"
# Sample input, ipc rowCentre 1: "300 220", the row's centre inside the window; an empty answer is a listing mid-swap.
ipc() {
    local calls
    case "$1" in
        total) printf '%s\n' 3 ;;
        visibleRowName) case "$2" in 0) printf '%s\n' aaa ;; 1) printf '%s\n' r3.txt ;; *) printf '%s\n' zzz ;; esac ;;
        rowCentre)
            calls=$(( $(cat "$centre_tmp/calls") + 1 )); printf '%s\n' "$calls" > "$centre_tmp/calls"
            if [ "$centre_mode" = late ] && [ "$calls" -gt 2 ]; then printf '%s\n' '300 220'; else printf '\n'; fi ;;
        tabCentre) printf '\n' ;;
        listInFlight) printf '%s\n' true ;;
        viewMode) printf '%s\n' list ;;
        path) printf '%s\n' /home/p ;;
    esac
}
late=$( { centre_into sx sy r3.txt; printf 'got=%s,%s calls=%s\n' "${sx:-unset}" "${sy:-unset}" "$(cat "$centre_tmp/calls")"; } 2>&1 )
if [ "$late" = 'got=312,262 calls=3' ]; then
    ok "a centre that answers nothing twice and then a point is returned into the caller's variables"
else
    bad "the late centre must be returned after 3 reads, got: $late"
fi
centre_mode=never
never=$( (centre_into sx sy r3.txt; printf 'reached\n') 2>&1 ); never_rc=$?
# Sample input: 'DRAG_CENTRE_FAIL {"reader":"screen_centre","name":"r3.txt","rowidx":"1","centre":"",...}', one line.
evidence=$(printf '%s\n' "$never" | grep '^DRAG_CENTRE_FAIL ')
if [ "$never_rc" -eq 1 ] && [ "$(printf '%s\n' "$evidence" | wc -l)" -eq 1 ] \
    && printf '%s\n' "${evidence#DRAG_CENTRE_FAIL }" | jq -e '
        .reader == "screen_centre" and .name == "r3.txt" and .rowidx == "1" and .centre == ""
        and .window == {"width": 2560, "height": 1440}
        and .listing == {"inFlight": "true", "total": "3", "view": "list", "path": "/home/p"}' >/dev/null; then
    ok "a centre that never answers prints one DRAG_CENTRE_FAIL line with the lookup, read, window and listing"
else
    bad "the never-answering centre must exit 1 after one DRAG_CENTRE_FAIL line, got rc=$never_rc: $never"
fi
if printf '%s\n' "$never" | grep -q '^FAIL the row r3.txt has no visible screen centre$' \
    && ! printf '%s\n' "$never" | grep -q -e 'unbound variable' -e '^reached$'; then
    ok "and the suite ends on a die that names the row, not on an unbound variable"
else
    bad "the never-answering centre must die naming r3.txt, got: $never"
fi
tab=$( (tab_centre_into tx ty 2; printf 'reached\n') 2>&1 ); tab_rc=$?
if [ "$tab_rc" -eq 1 ] && printf '%s\n' "$tab" | grep -q '^FAIL the tab 2 has no visible screen centre$' \
    && printf '%s\n' "$tab" | grep -q '^DRAG_CENTRE_FAIL .*"reader":"screen_tab_centre"' && ! printf '%s\n' "$tab" | grep -q '^reached$'; then
    ok "a tab centre that never answers ends the suite the same way"
else
    bad "the never-answering tab centre must die naming tab 2, got rc=$tab_rc: $tab"
fi

# R7's gate against a double (two row counts a poll): the payload's row appears on the third poll and its listing lands on the fifth, it never appears, or the observer fails.
r7_gate=$( (
    eval "$(sed -n '/^rowidx()/,/^}/p;/^r7_payload_listed()/,/^}/p' tests/drag.sh)"
    ok() { printf 'OK %s\n' "$*"; }
    walk_state() { printf 'WALK %s\n' "$*"; }
    r7_poll_attempts=6; r7_poll_seconds=0
    printf '0\n' > "$centre_tmp/polls"
    # Sample input, ipc total: "2", the row count; ipc visibleRowName 1: "r7.txt"; ipc listInFlight: "true" while a listing is out.
    ipc() {
        local polls
        polls=$(cat "$centre_tmp/polls")
        case "$1" in
            total) polls=$((polls + 1)); printf '%s\n' "$polls" > "$centre_tmp/polls"
                if [ "$r7_mode" = dead ]; then printf 'no such target\n'; return 1; fi
                if [ "$r7_mode" = garbled ]; then printf 'no such target\n'; return 0; fi; printf '2\n' ;;
            visibleRowName) if [ "$2" = 1 ] && [ "$r7_mode" = listed ] && [ "$polls" -gt 4 ]; then printf 'r7.txt\n'; else printf 'aaa\n'; fi ;;
            listInFlight) if [ "$polls" -le 8 ]; then printf 'true\n'; else printf 'false\n'; fi ;;
        esac
    }
    r7_mode=listed; r7_payload_listed; printf 'POLLS %s\n' "$(cat "$centre_tmp/polls")"
    printf '0\n' > "$centre_tmp/polls"
    r7_mode=never; (r7_payload_listed; printf 'reached\n'); printf 'RC %s\n' "$?"
    printf '0\n' > "$centre_tmp/polls"
    r7_mode=dead; (r7_payload_listed; printf 'reached\n'); printf 'DEAD %s after %s\n' "$?" "$(cat "$centre_tmp/polls")"
    printf '0\n' > "$centre_tmp/polls"
    r7_mode=garbled; (r7_payload_listed; printf 'reached\n'); printf 'GARBLED %s after %s\n' "$?" "$(cat "$centre_tmp/polls")"
) 2>&1 )
if printf '%s\n' "$r7_gate" | grep -q '^POLLS 10$' && printf '%s\n' "$r7_gate" | grep -q '^OK R7 the payload is listed and no listing is out$'; then
    ok "R7's gate passes only once the payload's row is listed and its listing has landed (poll 5, not the row alone at poll 3)"
else
    bad "R7's gate must wait for the payload's row and a landed listing, got: $r7_gate"
fi
if printf '%s\n' "$r7_gate" | grep -q '^RC 1$' && printf '%s\n' "$r7_gate" | grep -q '^WALK R7 unsettled$' \
    && printf '%s\n' "$r7_gate" | grep -q 'the payload never settled in the listing: listed no, in flight unread$' && ! printf '%s\n' "$r7_gate" | grep -q '^reached$'; then
    ok "and a payload that never reaches the listing ends the suite with the walk state and what was read"
else
    bad "R7's gate must die with its evidence when the payload never lists, got: $r7_gate"
fi
if printf '%s\n' "$r7_gate" | grep -q '^DEAD 1 after 1$' && printf '%s\n' "$r7_gate" | grep -q '^GARBLED 1 after 1$' \
    && [ "$(printf '%s\n' "$r7_gate" | grep -c 'R7 row count unavailable: no such target$')" -eq 2 ]; then
    ok "and a failed observer ends the suite on its first read, by name, whether it exits nonzero or answers an error with status 0"
else
    bad "R7's gate must die on the first failed row count, got: $r7_gate"
fi
[ -f "$centre_tmp/marker" ] && [ -n "$centre_tmp" ] && [ "${centre_tmp#/}" != "$centre_tmp" ] && rm -rf -- "$centre_tmp"

printf 'dragwire: %s check(s), %s failed\n' "$((pass + fail))" "$fail"
[ "$fail" -eq 0 ]
