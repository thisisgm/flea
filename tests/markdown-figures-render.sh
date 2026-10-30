#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Render this checkout's Markdown figures with the real helper, checking geometry, ink, fallback and request history.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

if ! command -v qs >/dev/null; then
    echo "markdown-figures-render.sh: qs is not installed, cannot render the preview"
    exit 1
fi

resolve_qjs() {
    if [ -n "${FLEA_QJS:-}" ] && [ "${FLEA_QJS#/}" != "${FLEA_QJS}" ] && [ -x "${FLEA_QJS}" ]; then
        printf '%s\n' "${FLEA_QJS}"
    elif command -v qjs >/dev/null 2>&1; then
        command -v qjs
    elif [ -x "$PWD/.superpowers/tools/qjs" ]; then
        printf '%s\n' "$PWD/.superpowers/tools/qjs"
    else
        return 1
    fi
}
fleabin=""
for cand in "$PWD/target/debug/flea" "$PWD/target/release/flea"; do
    if [ -x "$cand" ]; then
        fleabin="$cand"
        break
    fi
done
if ! qjs=$(resolve_qjs); then
    echo 'FAIL missing qjs: the real figure helper requires quickjs-ng'
    exit 1
fi
if [ -z "$fleabin" ]; then
    echo 'FAIL missing flea binary: build this checkout before rendering figures'
    exit 1
fi
printf 'MODE=real\n'

test_root="$FIXTURE_ROOT/flea-markdown-figrender-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/cache" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/markdown-figures-render.js "$test_root/config/" || exit 1
cp tests/markdown-figures-render.qml "$test_root/config/shell.qml" || exit 1

# The real helper and QML must use the same checkout's assets.
export FLEA_UI="$PWD/ui"
# These checks are about drawing, so every helper start runs from source and no background build outlives the sandbox.
export FLEA_FIGURE_CACHE=off
# Sample input: {"id":1,"kind":"math","source":"x^2","display":false,"theme":{...}}.
helper_probe_out=$(printf '%s\n' '{"id":1,"kind":"math","source":"x^2","display":false,"theme":{"bg":"#101315","fg":"#c0caf5","accent":"#7aa2f7","font":"monospace","bodyPx":14}}' | FLEA_QJS="$qjs" "$fleabin" --figure-helper 2>&1)
if ! printf '%s\n' "$helper_probe_out" | grep -q '"svg"'; then
    printf 'FAIL the figure helper did not answer: %s\n' "$helper_probe_out"
    exit 1
fi

{
echo '# Figures'
echo ''
echo '```mermaid'
echo 'flowchart TD'
echo '    A --> B'
echo '```'
echo ''
echo '```math'
echo '\frac{a}{b}'
echo '```'
echo ''
echo '$$'
echo 'x^2'
echo '$$'
echo ''
echo '```mermaid'
echo 'not a diagram {{{'
echo '```'
echo ''
echo 'A paragraph with $x^2$ inline maths and `code`.'
echo ''
echo '```mermaid'
echo 'flowchart TD'
echo '    FAR --> AWAY'
echo '```'
} > "$test_root/notes.md"

# The harness ends itself with a kill, so the subshell keeps bash's "Terminated" notice out of the report.
output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_MARKDOWN_FIGURE_FIXTURE="$test_root/notes.md" \
    FLEA_FIG_MODE=real FLEA_BIN="$fleabin" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 120 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )

# Sample input: MARKDOWN_FIGRENDER PASS (real) three figures, one mono fallback, widths fit, far figure unasked
if [ "$(printf '%s\n' "$output" | grep -c 'MARKDOWN_FIGRENDER PASS')" -ne 1 ] || printf '%s\n' "$output" | grep -q 'MARKDOWN_FIGRENDER FAIL'; then
    printf 'FAIL the figure preview missed a check\n'
    printf '%s\n' "$output" | grep -aE 'MARKDOWN_FIGRENDER|FigureService|ERROR|error|flea:' | head -20
    exit 1
fi
# Sample input: This plugin does not support setting window masks
# Only the platform mask line is filtered; the QJSEngine connect line fails this harness.
platform_warning='This plugin does not support setting window masks'
warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN|invalid nullptr parameter' | grep -vF "$platform_warning")
if [ -n "$warnings" ]; then
    printf 'FAIL the figure render harness logged a warning\n'
    printf '%s\n' "$warnings" | head -10
    exit 1
fi
shot=$(ls "$test_root/runtime"/markdown-figrender-*.png 2>/dev/null | head -1)
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ] && [ -n "$shot" ]; then
    mkdir -p "$FLEA_CI_SUITE_LOGS" || exit 1
    cp "$shot" "$FLEA_CI_SUITE_LOGS/markdown-figures-render.png" || exit 1
    printf 'shot %s\n' "$FLEA_CI_SUITE_LOGS/markdown-figures-render.png"
elif [ -n "$shot" ]; then
    printf 'shot %s\n' "$shot"
fi
printf '%s\n' "$output" | grep -oE 'MARKDOWN_FIGRENDER (CHECK|far top=|x\^2 ink|PASS).*'

cat > "$test_root/mathgap.md" <<'EOF'
Before consecutive formulas.

$$x^2$$

$$
\frac{a}{b}
$$

```js
var next = true;
```

Before a single formula.

$$x^2$$

After a single formula.

```mermaid
flowchart TD
    A --> B
```

```mermaid
sequenceDiagram
    A->>B: hi
```
EOF
cp tests/markdown-mathgap.qml "$test_root/config/shell.qml" || exit 1
mathgap_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_MARKDOWN_FIGURE_FIXTURE="$test_root/mathgap.md" \
    FLEA_BIN="$fleabin" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 25 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$mathgap_output" | grep -oE 'MARKDOWN_MATHGAP .*'
warnings=$(printf '%s\n' "$mathgap_output" | grep -aE 'TypeError|ReferenceError|WARN|invalid nullptr parameter' | grep -vF "$platform_warning")
[ -z "$warnings" ] || { printf 'FAIL maths gap harness warning: %s\n' "$warnings"; exit 1; }
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ] && [ -f "$test_root/runtime/markdown-mathgap.png" ]; then
    cp "$test_root/runtime/markdown-mathgap.png" "$FLEA_CI_SUITE_LOGS/markdown-mathgap.png" || exit 1
fi
expected_mathgap_checks=34
# Sample input: MARKDOWN_MATHGAP 34 checks, 0 failed
if ! printf '%s\n' "$mathgap_output" | grep -qE "(^|: )MARKDOWN_MATHGAP $expected_mathgap_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: mathgap expected %s checks, 0 failed; arrived [%s]\n' "$expected_mathgap_checks" "${mathgap_output:-<empty>}" >&2
    exit 1
fi

# A display formula's ex equals the body font's x-height at text sizes 14 and 12, and a formula wider than the pane still fits.
wide_terms=$(for n in $(seq 1 60); do printf 'a_{%s}+' "$n"; done)
{
echo 'Display maths beside prose.'
echo ''
echo '$$x^2$$'
echo ''
echo '$$\frac{a}{b}$$'
echo ''
printf '$$%s0$$\n' "$wide_terms"
} > "$test_root/mathsize.md"
cp tests/markdown-mathsize.js "$test_root/config/" || exit 1
cp tests/markdown-mathsize.qml "$test_root/config/shell.qml" || exit 1
mathsize_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_MARKDOWN_FIGURE_FIXTURE="$test_root/mathsize.md" \
    FLEA_BIN="$fleabin" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 45 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$mathsize_output" | grep -oE 'MARKDOWN_MATHSIZE .*'
warnings=$(printf '%s\n' "$mathsize_output" | grep -aE 'TypeError|ReferenceError|WARN|invalid nullptr parameter' | grep -vF "$platform_warning")
[ -z "$warnings" ] || { printf 'FAIL maths size harness warning: %s\n' "$warnings"; exit 1; }
expected_mathsize_checks=10 # Per text size: the x-height read, two formula ex checks and one fit check, then two for the forced 9 px x-height.
# Sample input: MARKDOWN_MATHSIZE 10 checks, 0 failed
if ! printf '%s\n' "$mathsize_output" | grep -qE "(^|: )MARKDOWN_MATHSIZE $expected_mathsize_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: mathsize expected %s checks, 0 failed; arrived [%s]\n' "$expected_mathsize_checks" "${mathsize_output:-<empty>}" >&2
    exit 1
fi

# Every figure starts flush on the content column: a flowchart and a sequence diagram, whose own canvas padding the helper trims.
cat > "$test_root/figflush.md" <<'EOF'
# Flush figures

```mermaid
flowchart TD
    A --> B
```

```mermaid
sequenceDiagram
    A->>B: hi
```
EOF
cp tests/markdown-figflush.qml "$test_root/config/shell.qml" || exit 1
figflush_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_MARKDOWN_FIGURE_FIXTURE="$test_root/figflush.md" \
    FLEA_BIN="$fleabin" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 25 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$figflush_output" | grep -oE 'MARKDOWN_FIGFLUSH .*'
warnings=$(printf '%s\n' "$figflush_output" | grep -aE 'TypeError|ReferenceError|WARN|invalid nullptr parameter' | grep -vF "$platform_warning")
[ -z "$warnings" ] || { printf 'FAIL figure flush harness warning: %s\n' "$warnings"; exit 1; }
expected_figflush_checks=3 # One grab, then one first-painted-column check for each of the two figures.
# Sample input: MARKDOWN_FIGFLUSH 3 checks, 0 failed
if ! printf '%s\n' "$figflush_output" | grep -qE "(^|: )MARKDOWN_FIGFLUSH $expected_figflush_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: figflush expected %s checks, 0 failed; arrived [%s]\n' "$expected_figflush_checks" "${figflush_output:-<empty>}" >&2
    exit 1
fi
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ] && [ -f "$test_root/runtime/markdown-figflush.png" ]; then
    cp "$test_root/runtime/markdown-figflush.png" "$FLEA_CI_SUITE_LOGS/markdown-figflush.png" || exit 1
fi

# The post-pass over real library output and hand-built shapes: no broken viewBox, no label cut, every tspan line its own text.
tighten_output=$(timeout 45 "$qjs" tests/markdown-figtighten.mjs 2>&1)
tighten_status=$?
expected_tighten_checks=570 # Unreadable texts alone and beside a rect, tspan lines split with inherited anchor and size, real multi-line figures, wide and narrow labels, every ASCII glyph run at its measured maximum, a long CJK message, labels centred in their boxes with no dy left for QtSvg to drop.
# Sample input: MARKDOWN_FIGTIGHTEN 570 checks, 0 failed
if [ "$tighten_status" -ne 0 ] || ! printf '%s\n' "$tighten_output" | grep -qE "^MARKDOWN_FIGTIGHTEN $expected_tighten_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: figtighten expected %s checks, 0 failed; exited %s; arrived [%s]\n' "$expected_tighten_checks" "$tighten_status" "${tighten_output:-<empty>}" >&2
    exit 1
fi
printf '%s\n' "$tighten_output" | head -1

# Every link mark is a plain shape in the accent, or in the colour a user style gives its link; stderr goes to a file and prints on failure.
paths_output=$(timeout 45 "$qjs" tests/markdown-figures-render-paths.mjs 2>"$test_root/paths-stderr.log")
paths_status=$?
expected_paths_checks=125 # Three checks for each of 37 corpus and end-mark diagrams, a mark count for each of the five end-mark ones, four on a styled set and five on hand-built marks.
# Sample input: MARKDOWN_PATHS 125 checks, 0 failed
if [ "$paths_status" -ne 0 ] || ! printf '%s\n' "$paths_output" | grep -qE "^MARKDOWN_PATHS $expected_paths_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: paths expected %s checks, 0 failed; exited %s; arrived [%s] stderr [%s]\n' "$expected_paths_checks" "$paths_status" "${paths_output:-<empty>}" "$(cat "$test_root/paths-stderr.log")" >&2
    exit 1
fi
printf '%s\n' "$paths_output" | head -1
cp tests/markdown-figures-render-arrows.qml "$test_root/config/shell.qml" || exit 1
arrow_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_BIN="$fleabin" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 25 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$arrow_output" | grep -oE 'MARKDOWN_ARROWS .*'
warnings=$(printf '%s\n' "$arrow_output" | grep -aE 'TypeError|ReferenceError|WARN|invalid nullptr parameter' | grep -vF "$platform_warning")
[ -z "$warnings" ] || { printf 'FAIL arrow harness warning: %s\n' "$warnings"; exit 1; }
expected_arrow_checks=39 # Every head measured with a displaced-head control: five checks for each of four single heads, eight for the double-marked link, eleven for the back-link loop.
# Sample input: MARKDOWN_ARROWS 24 checks, 0 failed
if ! printf '%s\n' "$arrow_output" | grep -qE "(^|: )MARKDOWN_ARROWS $expected_arrow_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: arrows expected %s checks, 0 failed; arrived [%s]\n' "$expected_arrow_checks" "${arrow_output:-<empty>}" >&2
    exit 1
fi
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ]; then
    cp "$test_root/runtime/markdown-arrows.png" "$FLEA_CI_SUITE_LOGS/markdown-arrows.png" || exit 1
fi

# Every end mark of a link, flowchart and class, drawn at both ends of the first edge.
cp tests/markdown-figures-render-ends.qml "$test_root/config/shell.qml" || exit 1
ends_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_BIN="$fleabin" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 25 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$ends_output" | grep -oE 'MARKDOWN_ENDS (MARKS|FAIL).*'
warnings=$(printf '%s\n' "$ends_output" | grep -aE 'TypeError|ReferenceError|WARN|invalid nullptr parameter' | grep -vF "$platform_warning")
[ -z "$warnings" ] || { printf 'FAIL end marks harness warning: %s\n' "$warnings"; exit 1; }
expected_ends_checks=36 # Three checks for each of 12 link end cases.
# Sample input: MARKDOWN_ENDS 36 checks, 0 failed
if ! printf '%s\n' "$ends_output" | grep -qE "(^|: )MARKDOWN_ENDS $expected_ends_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: end marks expected %s checks, 0 failed; arrived [%s]\n' "$expected_ends_checks" "${ends_output:-<empty>}" >&2
    exit 1
fi
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ]; then
    cp "$test_root/runtime/markdown-ends.png" "$FLEA_CI_SUITE_LOGS/markdown-ends.png" || exit 1
fi

md3u_output=$(timeout 45 "$qjs" tests/markdown-advfix-md3u.mjs) || { printf '%s\n' "$md3u_output"; exit 1; }
printf '%s\n' "$md3u_output" | head -1
printf '%s\n' "$md3u_output" | tail -1 > "$test_root/md3u-cases.json" || exit 1
md3u_native=$(QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QML_XHR_ALLOW_FILE_READ=1 QT_FORCE_STDERR_LOGGING=1 \
    XDG_RUNTIME_DIR="$test_root/runtime" timeout 25 qml6 tests/markdown-advfix-md3u.qml -- "file://$test_root/md3u-cases.json" 2>&1)
md3u_status=$?
printf '%s\n' "$md3u_native" | grep -E 'MD3U_GEOMETRY|MARKDOWN_MD3U_NATIVE|FAIL'
expected_md3u_native_checks=12 # Four heading checks, six F2 geometry checks, decode and capture across six cases.
expected_md3u_native_cases=6 # Three body sizes for each of F2 and F7.
# Sample input: MD3U_GEOMETRY {"id":"F2","bodyPx":12,"labelInkWidth":86}
md3u_native_cases=$(printf '%s\n' "$md3u_native" | grep -cE '(^|: )MD3U_GEOMETRY ')
# Sample input: MARKDOWN_MD3U_NATIVE 12 checks, 0 failed
if [ "$md3u_status" -ne 0 ] || [ "$md3u_native_cases" -ne "$expected_md3u_native_cases" ] || ! printf '%s\n' "$md3u_native" | grep -qE "(^|: )MARKDOWN_MD3U_NATIVE $expected_md3u_native_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: md3u native expected %s cases, %s checks, 0 failed; exited %s; arrived %s cases [%s]\n' "$expected_md3u_native_cases" "$expected_md3u_native_checks" "$md3u_status" "$md3u_native_cases" "${md3u_native:-<empty>}" >&2
    exit 1
fi
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ]; then
    cp "$test_root/md3u-cases.png" "$FLEA_CI_SUITE_LOGS/markdown-md3u-geometry.png" || exit 1
fi

# Nesting and inline maths: the ql-markdown-nesting fixture, a formula pair for ink, a failing formula, a repeat and one far below the cache.
draw_dir="$test_root/draw"
mkdir -p "$draw_dir" || exit 1
cat > "$draw_dir/nesting.md" <<'EOF'
# Nesting

- one
  - two
    - three
- [ ] task
  - [x] nested done

1. first

2. loose second

> outer
>> inner
>>> innermost

Inline maths $x^2 + y^2$ in a line.
EOF
cat > "$draw_dir/maths.md" <<'EOF'
# One

x $x$ x

# Two

y $y$ y

# Three

bad $\badmacro$ here

# Four

twice $a$ and $a$ again
EOF
{
    for _filler in $(seq 1 60); do printf '## Filler %s\n\nA paragraph that only gives the document height.\n\n' "$_filler"; done
    printf '## Far\n\nfar $z$ end\n'
} > "$draw_dir/far.md"
cp tests/markdown-draw.js "$test_root/config/" || exit 1
cp tests/markdown-draw.qml "$test_root/config/shell.qml" || exit 1
draw_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_DRAW_DIR="$draw_dir" \
    FLEA_BIN="$fleabin" FLEA_QJS="$qjs" FLEA_UI="$FLEA_UI" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 90 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$draw_output" | grep -oE 'MARKDOWN_DRAW .*'
warnings=$(printf '%s\n' "$draw_output" | grep -aE 'TypeError|ReferenceError|WARN|invalid nullptr parameter' | grep -vF "$platform_warning")
[ -z "$warnings" ] || { printf 'FAIL nesting and inline maths harness warning: %s\n' "$warnings"; exit 1; }
expected_draw_checks=39 # Nesting visits 13 each, maths visits 4 each and 3 for the grab and ink, the far formula 2.
# Sample input: MARKDOWN_DRAW 39 checks, 0 failed
if ! printf '%s\n' "$draw_output" | grep -qE "(^|: )MARKDOWN_DRAW $expected_draw_checks checks, 0 failed$"; then
    printf 'FAIL markdown-figures-render: nesting and inline maths expected %s checks, 0 failed; arrived [%s]\n' "$expected_draw_checks" "${draw_output:-<empty>}" >&2
    exit 1
fi
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ] && ls "$test_root/runtime"/markdown-draw-*.png >/dev/null 2>&1; then
    cp "$(ls "$test_root/runtime"/markdown-draw-*.png | head -1)" "$FLEA_CI_SUITE_LOGS/markdown-draw.png" || exit 1
fi
