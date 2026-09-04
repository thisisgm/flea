#!/usr/bin/env bash
# Proves the wheel arithmetic through the real QML component, then pins every vertical viewport to
# that shared policy so a newly-unreachable surface cannot hide behind the arithmetic tests.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

D=$(mktemp -d /tmp/flea-scroll.XXXXXX)
trap 'rm -rf "$D"' EXIT
ln -s "$PWD/tests/scroll.qml" "$D/shell.qml"
ln -s "$PWD/ui/FastScrollHandler.qml" "$D/FastScrollHandler.qml"

out=$(QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME= QT_STYLE_OVERRIDE=Basic \
      QT_FORCE_STDERR_LOGGING=1 timeout 30 qs -p "$D" 2>&1)
code=$?
printf '%s\n' "$out"
[[ "$code" -ne 124 ]] || { printf 'scroll.sh: QML harness timed out\n'; exit 1; }
printf '%s' "$out" | grep -qE '9 checks, 0 failed' || exit 1
# The harness ends its own Quickshell process with SIGTERM after printing the authoritative tally.
[[ "$code" -eq 0 || "$code" -eq 143 ]] || exit "$code"

declare -A want=(
    [ui/List.qml]=1
    [ui/GridArea.qml]=1
    [ui/ColumnPane.qml]=1
    [ui/Sidebar.qml]=1
    [ui/PreviewText.qml]=1
    [ui/PdfViewer.qml]=1
    [ui/ShareBrowser.qml]=1
    [ui/ContextMenu.qml]=2
)

for file in "${!want[@]}"; do
    seen=$(grep -c 'FastScrollHandler {' "$file")
    [[ "$seen" -eq "${want[$file]}" ]] || {
        printf 'scroll.sh: %s has %s handlers, expected %s\n' "$file" "$seen" "${want[$file]}"
        exit 1
    }
done

grep -q 'id: shareView' ui/ShareBrowser.qml || { printf 'scroll.sh: share browser is not a view\n'; exit 1; }
grep -q 'height: Menu.boundedExtent' ui/ContextMenu.qml || { printf 'scroll.sh: menus are not viewport-bounded\n'; exit 1; }
printf 'scroll: accelerated arithmetic and 9 vertical surfaces passed\n'
