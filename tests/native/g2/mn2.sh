#!/bin/bash
# Native acceptance for agent, custom actions and plugin rows: the ui.sh cases and visual scenes in both arms, inside a marked sandbox.
set -euo pipefail
: "${W:?absolute staged candidate tree}" "${Q_BIN:?verified v0.3.12 launcher}" "${G_BIN:?verified GPUI flea binary}"
[[ $W == /* && -d $W ]] || { printf 'W must be an absolute tree\n' >&2; exit 2; }
cd "$W"
mkdir -p "$W/.superpowers/g2-mn2"
E=$(mktemp -d "$W/.superpowers/g2-mn2/native.XXXXXXXX")
: > "$E/.flea-test-sandbox"
case "$E" in /*/.superpowers/g2-mn2/native.*) ;; *) echo "refusing root $E" >&2; exit 2 ;; esac
seed_actions() {
    local root=$1
    mkdir -p "$root/config/flea/actions" "$root/config/flea/scripts" "$root/home/work/Photos"
    cat > "$root/config/flea/actions/inspect.toml" <<'EOF'
name="Inspect"
mime=["image/*"]
selection="one"
mode="preview"
run=["/usr/bin/file", "{path}"]
EOF
    cat > "$root/config/flea/actions/resize.toml" <<'EOF'
name="Resize to 1600 px"
mime=["image/*"]
selection="many"
mode="background"
run=["/usr/bin/true"]
EOF
    cat > "$root/config/flea/actions/runner.toml" <<'EOF'
name="Runner"
mime=["*/*"]
selection="many"
mode="terminal"
run=["/usr/bin/true"]
EOF
    cat > "$root/config/flea/actions/broken.toml" <<'EOF'
name="Broken"
EOF
    printf '#!/bin/sh\nexit 0\n' > "$root/config/flea/scripts/convert-to-webp"
    chmod 755 "$root/config/flea/scripts/convert-to-webp"
    for n in 1 2 3 4 5 6 7 8 9; do : > "$root/home/work/Photos/photo-$n.jpg"; done
    : > "$root/home/work/notes.txt"
}
cases=(runscript openterminal menu background)
runtime=${XDG_RUNTIME_DIR:?native compositor runtime}
for arm in qml gpui; do
    mkdir -p -m 700 "$E/$arm"/{config,state,cache,runtime,home}
    if [ -d "$runtime/hypr" ]; then ln -s "$runtime/hypr" "$E/$arm/runtime/hypr"; fi
    if [[ ${WAYLAND_DISPLAY:-} != /* && -S "$runtime/${WAYLAND_DISPLAY:-}" ]]; then
        ln -s "$runtime/$WAYLAND_DISPLAY" "$E/$arm/runtime/$WAYLAND_DISPLAY"
    fi
    if [ -S "$runtime/.ydotool_socket" ]; then ln -s "$runtime/.ydotool_socket" "$E/$arm/runtime/.ydotool_socket"; fi
    seed_actions "$E/$arm"
    bin=$Q_BIN
    [ "$arm" != gpui ] || bin=$G_BIN
    env WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" FLEA_FRONTEND="$arm" FLEA_BIN="$bin" HOME="$E/$arm/home" \
        XDG_CONFIG_HOME="$E/$arm/config" XDG_STATE_HOME="$E/$arm/state" \
        XDG_CACHE_HOME="$E/$arm/cache" XDG_RUNTIME_DIR="$E/$arm/runtime" \
        FLEA_FIXTURE_ROOT="$E/$arm/fixtures" bash tests/ui.sh "${cases[@]}" > "$E/$arm/ui.log" 2>&1 || printf '%s ui.sh exit %s\n' "$arm" "$?" >> "$E/failures"
done
# Actions ships hidden, so the --scene calls below seed a hidden set that shows actions and openAgent; the hashed scenes carry their own seed in the manifest setup.
test -x tools/flea-ci-visual || { printf 'BLOCKED: the GPUI scene CLI tools/flea-ci-visual is absent\n' >&2; exit 2; }
[ -n "${FLEA_VISUAL_DESIGN:-}" ] || { printf 'BLOCKED: FLEA_VISUAL_DESIGN unset\n' >&2; exit 2; }
shown='["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText"]'
off='["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText","openAgent","actions"]'
scene() {
    env WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" FLEA_FRONTEND=gpui FLEA_BIN="$G_BIN" HOME="$E/gpui/home" \
        XDG_CONFIG_HOME="$E/gpui/config" XDG_STATE_HOME="$E/gpui/state" \
        XDG_CACHE_HOME="$E/gpui/cache" XDG_RUNTIME_DIR="$E/gpui/runtime" \
        "$G_BIN" --scene "$1" > "$2" 2>&1 || printf 'gpui %s: --scene exited %s, log %s\n' "$1" "$?" "$2" >> "$E/failures"
}
set_hidden() {
    env HOME="$E/gpui/home" XDG_CONFIG_HOME="$E/gpui/config" XDG_STATE_HOME="$E/gpui/state" \
        XDG_DATA_HOME="$E/gpui/data" "$G_BIN" --ui-state "{\"menu\":{\"hidden\":$1}}" >/dev/null \
        || printf 'gpui --ui-state exited %s\n' "$?" >> "$E/failures"
}
# An absent row counts only beside a row the scene always draws, so an empty log is a failure, never a pass.
absent() {
    grep -qF "$2" "$1" || { printf '%s: no %s row drawn to anchor the check\n' "$4" "$2" >> "$E/failures"; return 0; }
    ! grep -qF "$3" "$1" || printf '%s: %s row while switched off\n' "$4" "$3" >> "$E/failures"
}
# Actions switch off (no row) then on: the row follows the Menus switch each way.
set_hidden "$shown"
scene g2.mn2.actions-menu.primary "$E/scene-actions.log"
grep -F 'Actions' "$E/scene-actions.log" || printf 'gpui actions-menu: no Actions row while shown\n' >> "$E/failures"
set_hidden "$off"
scene g2.mn2.actions-menu.primary "$E/scene-actions.log"
absent "$E/scene-actions.log" 'Tags' 'Actions' 'gpui actions-menu'
set_hidden "$shown"
scene g2.mn2.actions-menu.primary "$E/scene-actions.log"
grep -F 'Actions' "$E/scene-actions.log" || printf 'gpui actions-menu: no Actions row after switching back on\n' >> "$E/failures"
# The switched-off state lives in its own scene: shipped hidden, no agent row.
set_hidden "$off"
scene g2.mn2.extra.agent-off "$E/scene-agent-off.log"
absent "$E/scene-agent-off.log" 'Open in Terminal' 'Open in Agent' 'gpui agent-off'
set_hidden "$shown"
for arm in qml gpui; do
    tools/flea-ci-visual --frontend "$arm" --manifest tests/visual/g2/mn2.json --output "$E/visual/$arm" --design "$FLEA_VISUAL_DESIGN" \
        || printf '%s flea-ci-visual exited %s\n' "$arm" "$?" >> "$E/failures"
done
printf 'evidence %s\n' "$E"
[ ! -s "$E/failures" ]
