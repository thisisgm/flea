#!/bin/bash
# Guards what an external application sees when Flea drags a file out. tests/drag.sh proves the
# gesture but needs the display and a real pointer, so it never runs in the headless battery.
# A leaving drag offers copy and move, the way Files does, and still offers text/uri-list.
# Offering move is what made Chromium report dropEffect move in 0.1.4; the offer stays, and the
# note in AGENTS.md records that an uploader may refuse it. The shelf drag stays copy only.
set -u
cd "$(dirname "$0")/.." || exit 1

pass=0
fail=0
ok()  { printf 'ok   %s\n' "$*"; pass=$((pass+1)); }
bad() { printf 'FAIL %s\n' "$*"; fail=$((fail+1)); }

# Comments may name an action to explain it, so every check below reads code only.
code_of() { sed -e 's://.*::' "$1"; }

advertised=$(for f in ui/*.qml; do code_of "$f" | grep -H --label="$f" -n 'Drag\.supportedActions'; done)
count=$(printf '%s' "$advertised" | grep -c . )
if [ "$count" -eq 1 ]; then
    ok "exactly one view advertises a drag: $(printf '%s' "$advertised" | cut -d: -f1)"
else
    bad "expected exactly 1 Drag.supportedActions in ui/, found $count"
    printf '%s\n' "$advertised" | sed 's/^/     /'
fi

# Qt hands effectAllowed straight from this line. A plain lift names both actions.
# Ctrl names copy alone and Shift names move alone, so a receiver that prefers move
# whenever move is offered still copies when Ctrl was held at the lift.
if printf '%s' "$advertised" | grep -q 'Qt\.CopyAction' && printf '%s' "$advertised" | grep -q 'Qt\.MoveAction'; then
    ok "a leaving drag offers both copy and move"
else
    bad "a leaving drag must offer Qt.CopyAction and Qt.MoveAction, got: $(printf '%s' "$advertised" | cut -d: -f3-)"
fi
if printf '%s' "$advertised" | grep -q 'dragCopy' && printf '%s' "$advertised" | grep -q 'dragShift'; then
    ok "ctrl offers copy alone and shift offers move alone"
else
    bad "the offer must narrow on dragCopy and dragShift, got: $(printf '%s' "$advertised" | cut -d: -f3-)"
fi

if printf '%s' "$advertised" | grep -q 'Qt\.LinkAction'; then
    bad "a leaving drag offers a link"
else
    ok "a leaving drag does not offer a link"
fi

if grep -q 'text/uri-list' ui/js/Drag.js; then
    ok "a leaving drag still offers text/uri-list"
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

# Flea's own verb is the marker, not the DragEvent field Qt clamps to the advertised actions.
side=$(for f in ui/*.qml ui/js/*.js; do code_of "$f" | grep -H --label="$f" -n '\bdrag\.proposedAction'; done)
if [ -z "$side" ]; then
    ok "the internal verb does not ride on drag.proposedAction"
else
    bad "the internal verb is back on drag.proposedAction:"
    printf '%s\n' "$side" | sed 's/^/     /'
fi

printf 'dragwire: %s check(s), %s failed\n' "$((pass + fail))" "$fail"
[ "$fail" -eq 0 ]
