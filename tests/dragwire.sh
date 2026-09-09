#!/bin/bash
# Guards the one declaration that decides what an external application sees when Flea drags a file
# out. tests/drag.sh proves the behaviour but needs the display and a real pointer, so it never runs
# in the headless battery: put Qt.MoveAction back and every other suite stays green while Chromium
# reports dropEffect move again and Google refuses the upload. That regression shipped in 0.1.4.
set -u
cd "$(dirname "$0")/.." || exit 1

pass=0
fail=0
ok()  { printf 'ok   %s\n' "$*"; pass=$((pass+1)); }
bad() { printf 'FAIL %s\n' "$*"; fail=$((fail+1)); }

# Comments legitimately name Qt.MoveAction to explain why it is absent, so every check below reads
# code only. Sample input, ui/List.qml: '            Drag.supportedActions: Qt.CopyAction  // note'
code_of() { sed -e 's://.*::' "$1"; }

# Sample input, ui/List.qml: '            Drag.supportedActions: Qt.CopyAction'
advertised=$(for f in ui/*.qml; do code_of "$f" | grep -H --label="$f" -n 'Drag\.supportedActions'; done)

# Two views lift a row: the window's list and the picker's, each on a ghost of the same shape.
views=$(printf '%s\n' "$advertised" | cut -d: -f1 | sort | tr '\n' ' ')
if [ "$views" = "ui/List.qml ui/PickerList.qml " ]; then
    ok "exactly the two views advertise a drag: $views"
else
    bad "expected Drag.supportedActions in ui/List.qml and ui/PickerList.qml only, found: $views"
    printf '%s\n' "$advertised" | sed 's/^/     /'
fi

# Qt hands effectAllowed straight from this, so anything but Copy alone tells the receiver it may move.
wider=$(printf '%s\n' "$advertised" | grep -v 'Drag\.supportedActions:[[:space:]]*Qt\.CopyAction[[:space:]]*$')
if [ -z "$wider" ]; then
    ok "every advertised action is Qt.CopyAction alone"
else
    bad "Drag.supportedActions must be exactly Qt.CopyAction, got: $(printf '%s' "$wider" | cut -d: -f3-)"
fi

# Move ranks above Copy, so Chromium prefers it the moment it is offered.
offered=$(for f in ui/*.qml ui/js/*.js; do code_of "$f" | grep -H --label="$f" -n 'Qt\.MoveAction\|Qt\.LinkAction'; done)
if [ -z "$offered" ]; then
    ok "no Qt.MoveAction or Qt.LinkAction reaches an external client"
else
    bad "a drag still offers an action above Copy:"
    printf '%s\n' "$offered" | sed 's/^/     /'
fi

# The ctrl signal used to ride on the DragEvent's proposedAction, which Qt clamps to what the source
# advertised: once the source is copy-only that read is always Copy, so a move silently became a copy.
side=$(for f in ui/*.qml ui/js/*.js; do code_of "$f" | grep -H --label="$f" -n '\bdrag\.proposedAction'; done)
if [ -z "$side" ]; then
    ok "the copy signal does not ride on the DragEvent's proposedAction"
else
    bad "the internal copy signal is back on proposedAction, which a copy-only source pins to Copy:"
    printf '%s\n' "$side" | sed 's/^/     /'
fi

printf 'dragwire: %s check(s), %s failed\n' "$((pass + fail))" "$fail"
[ "$fail" -eq 0 ]
