#!/usr/bin/env bash
# Render-property tests of the real QML components, offscreen at both device scales. The sandbox
# owns every setting and input; no display, system setting or installed UI is changed. Killing the
# probe is its completion path because Quickshell does not implement Qt.quit(). A tally is mandatory.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v qs >/dev/null || { echo 'typography: qs is required' >&2; exit 1; }
probe=$(mktemp -d)
trap 'rm -rf "$probe"' EXIT
cp -a ui "$probe/ui"
cp tests/typography.qml "$probe/ui/shell.qml"
mkdir -p "$probe/home/.config" "$probe/home/.local/state"
# Keep the real font alias, but not a user's text overrides or Flea settings.
if [[ -d "$HOME/.config/fontconfig" ]]; then
    cp -a "$HOME/.config/fontconfig" "$probe/home/.config/"
fi
for scale in 1 2; do
    status=0
    { env -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
        HOME="$probe/home" XDG_CONFIG_HOME="$probe/home/.config" \
        XDG_STATE_HOME="$probe/home/.local/state" XDG_CACHE_HOME="$probe/cache" \
        QT_QPA_PLATFORM=offscreen QT_SCALE_FACTOR="$scale" QT_FORCE_STDERR_LOGGING=1 \
        FLEA_REDUCED_MOTION=1 \
        timeout 20s qs -p "$probe/ui"; } >"$probe/log" 2>&1 || status=$?
    if ! grep -qE 'TYPOGRAPHY [1-9][0-9]* checks, 0 failed' "$probe/log" \
        || grep -qE 'FAIL|TypeError|ReferenceError|Error:|ERROR|Cannot assign|Binding loop' "$probe/log" \
        || [[ $status != 0 && $status != 143 ]]; then
        printf 'typography: scale %s failed (status %s)\n' "$scale" "$status"
        tail -100 "$probe/log"
        exit 1
    fi
    printf 'typography: scale %s — %s\n' "$scale" "$(grep 'TYPOGRAPHY' "$probe/log")"
done
