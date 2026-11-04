#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-magick-font"
. "$(dirname "$0")/headless-commons.sh"
# Exercise preview layout offscreen and fail on every engine binding loop, with no warning filter.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
for tool in qs magick; do
    command -v "$tool" >/dev/null || { echo "FAIL $tool is not installed"; exit 1; }
done
test_root="$FIXTURE_ROOT/flea-preview-layout-loop-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT
mkdir -p "$test_root"/{config,home,state,cache,runtime} || exit 1
chmod 700 "$test_root/runtime" || exit 1
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/preview-layout-loop.qml "$test_root/config/shell.qml" || exit 1
cat > "$test_root/mixed.md" <<'DOC'
# Overflow edge

A paragraph with `loadFile()` and enough words to wrap across the smaller preview viewport.

```js
var fenced = true;
A longer line of fenced content that wraps across the viewport. A longer line of fenced content that wraps across the viewport.
```

> A quoted paragraph with enough words to wrap across the smaller viewport more than once.

| Kind | Content |
| :--- | :--- |
| rows | a table cell |
| facts | another table cell |

![local](local.svg)
DOC
cat > "$test_root/local.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="180" height="120"><rect width="180" height="120" fill="#708090"/></svg>
SVG
# Hard-break lines yield adjacent documents on each side of the host's actual viewport height.
edge_documents=96
overflow_lines=96
wrap_repeats=8
for n in $(seq 1 "$edge_documents"); do
    { printf 'edge %d\n' "$n"; printf 'viewport edge  \n%.0s' $(seq 1 "$n"); } > "$test_root/edge-$n.md"
done
{ printf 'A long text line that wraps in the preview. %.0s' $(seq 1 "$wrap_repeats"); printf '\n'; printf 'plain text\n%.0s' $(seq 1 "$overflow_lines"); } \
    > "$test_root/plain.txt"
printf 'let long_line = "a long code line that wraps in Quick Look";\n%.0s' $(seq 1 "$overflow_lines") > "$test_root/code.rs"
magick -size 400x560 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 30 \
    -annotate +40+80 'LAYOUT' "$test_root/page.pdf" || exit 1
# Each scenario is a first visit in both hosts; relative Markdown images stay beside their documents.
for scenario in $(seq 0 5); do
    mkdir -p "$test_root/scenario-$scenario" || exit 1
    cp "$test_root/"*.md "$test_root/plain.txt" "$test_root/code.rs" "$test_root/page.pdf" "$test_root/local.svg" "$test_root/scenario-$scenario/" || exit 1
done
for theme in dark light; do
    theme_home="$test_root/home-$theme"
    palette_dir="$theme_home/.local/state/omarchy/current/theme"
    mkdir -p "$palette_dir" "$test_root/state-$theme" "$test_root/cache-$theme" || exit 1
    if [ "$theme" = dark ]; then
        background='#101315'; foreground='#c0caf5'; surface='#181825'
    else
        background='#ffffff'; foreground='#202020'; surface='#eeeeee'
    fi
    printf 'background = "%s"\nforeground = "%s"\ndark_background = "%s"\n' \
        "$background" "$foreground" "$surface" > "$palette_dir/colors.toml" || exit 1
    output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
        HOME="$theme_home" XDG_STATE_HOME="$test_root/state-$theme" XDG_CACHE_HOME="$test_root/cache-$theme" \
        XDG_RUNTIME_DIR="$test_root/runtime" FLEA_LAYOUT_DIR="$test_root" FLEA_REDUCED_MOTION=1 FLEA_LAYOUT_BACKGROUND="$background" \
        FLEA_BIN="${FLEA_BIN:-$PWD/target/debug/flea}" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
        timeout 75 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
    status=$?
    if [ -n "${FLEA_CI_SUITE_LOGS:-}" ]; then
        printf '%s\n' "$output" > "$FLEA_CI_SUITE_LOGS/preview-layout-loop-$theme-engine.log" || exit 1
    fi
    printf '%s\n' "$output" | grep -a 'PREVIEW_LAYOUT'
    loops=$(printf '%s\n' "$output" | grep -ac 'Binding loop detected')
    if [ "$loops" -ne 0 ]; then
        echo "FAIL $theme preview binding loops=$loops"
        printf '%s\n' "$output" | grep -a 'Binding loop detected'
        exit 1
    fi
    if [ "$status" -ne 143 ] || [ "$(printf '%s\n' "$output" | grep -c 'PREVIEW_LAYOUT DONE')" -ne 1 ] \
        || [ "$(printf '%s\n' "$output" | grep -c 'PREVIEW_LAYOUT CASE')" -ne 27 ] \
        || printf '%s\n' "$output" | grep -aqE 'PREVIEW_LAYOUT FAIL|ERROR|TypeError|ReferenceError'; then
        echo "FAIL $theme preview probe did not complete, qs exit=$status"
        printf '%s\n' "$output"
        exit 1
    fi
done
printf 'preview-layout-loop: 54 cases, 0 binding loops\n'
