#!/usr/bin/env bash
# The real ui/PreviewMarkdown.qml over a fixture document, grabbed offscreen and judged on pixel facts: the chrome chip behind inline code, the table's rules without verticals, no Qt default link blue, and the quote bar's muted ink.
set -u
script_path=$(readlink -f -- "$0") || exit 1
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

check_warnings() {
    local output=$1 platform_warning worker_warning maximum_worker_warnings worker_warnings warnings
    # Offscreen window-mask warning is expected; the caller bounds WorkerScript warnings for the active parse path.
    platform_warning='This plugin does not support setting window masks'
    worker_warning='QObject::connect(QJSEngine, QtObject): invalid nullptr parameter'
    maximum_worker_warnings=${2:-1}
    worker_warnings=$(printf '%s\n' "$output" | grep -cF "$worker_warning")
    if [ "$worker_warnings" -gt "$maximum_worker_warnings" ]; then
        printf 'FAIL at most %s WorkerScript warning allowed, got %s\n' "$maximum_worker_warnings" "$worker_warnings"
        return 1
    fi
    warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN' | grep -vF "$platform_warning" | grep -vF "$worker_warning")
    if [ -n "$warnings" ]; then
        printf 'FAIL the render harness logged a warning\n'
        printf '%s\n' "$warnings" | head -10
        return 1
    fi
}

if [ "${1:-}" = "--check-warnings" ]; then
    check_warnings "$(cat)" "${2:-1}"
    exit $?
fi

warning_control() {
    local name=$1 transcript=$2 expected_status=$3 expected_message=$4 output status
    output=$(printf '%s\n' "$transcript" | bash "$script_path" --check-warnings)
    status=$?
    if [ "$status" -ne "$expected_status" ] || [[ "$output" != *"$expected_message"* ]]; then
        printf 'FAIL warning control %s status=%s: %s\n' "$name" "$status" "$output"
        exit 1
    fi
    printf 'ok warning control %s status=%s\n' "$name" "$status"
}

worker_warning='WARN: QObject::connect(QJSEngine, QtObject): invalid nullptr parameter'
warning_control zero 'MARKDOWN_RENDER PASS chip, rules, bar and links all read' 0 ''
warning_control one "$worker_warning" 0 ''
warning_control two "$(printf '%s\n%s\n' "$worker_warning" "$worker_warning")" 1 'WorkerScript warning'
warning_control other 'WARN: probe unexpected warning' 1 'render harness logged a warning'
if check_warnings "$worker_warning" 0 >/dev/null; then
    echo "FAIL inline parse accepted a WorkerScript warning"; exit 1
fi
check_warnings "" 0 || exit 1
printf 'ok inline parse allows zero WorkerScript warnings\n'

if ! command -v qs >/dev/null; then
    echo "markdown-render.sh: qs is not installed, cannot render the preview"
    exit 1
fi

python3 tests/markdown-gates.py || exit 1

test_root="$FIXTURE_ROOT/flea-markdown-render-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/cache" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
ln -s "$(readlink -f ui/boot/Commons)" "$test_root/config/Commons" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/markdown-render.js tests/markdown-centre.js tests/markdown-board.js tests/markdown-bar.js "$test_root/config/" || exit 1
cp tests/markdown-render.qml "$test_root/config/shell.qml" || exit 1

cat > "$test_root/notes.md" <<'EOF'
# Rendered notes

## Second level

A paragraph with `loadFile()` inline code and [a guide](https://example.com/guide).

> A quoted line for the bar.

1. First item
2. Second item

- Alpha item
- Beta item

| Kind | Asks for | Cached |
| :--- | :--- | :--- |
| rows | the cursor | yes |
| facts | the table | yes |

```js
var fenced = true;
```

![shot](https://cdn.example.com/shot.png)
EOF

: > "$test_root/notes.md.empty.md"
printf '# Identical\n' > "$test_root/notes.md.first.md" || exit 1
cp "$test_root/notes.md.first.md" "$test_root/notes.md.second.md" || exit 1
printf 'before\n\n## \n\n>\n\nafter\n' > "$test_root/notes.md.gaps.md" || exit 1
# A name far past any bar width, so the bar must elide it and the room it may take is measured.
long_name_digits=200
long_fixture="$test_root/long-$(printf "%0${long_name_digits}d" 0).md"
cp "$test_root/notes.md" "$long_fixture" || exit 1

# The harness ends itself with a kill, so the subshell keeps bash's "Terminated" notice out of the report.
output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_MARKDOWN_FIXTURE="$test_root/notes.md" FLEA_MARKDOWN_LONG="$long_fixture" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 60 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )

# Sample input, the verdict line: "  INFO qml: MARKDOWN_RENDER PASS chip, rules, bar and links all read"
if [ "$(printf '%s\n' "$output" | grep -c 'MARKDOWN_RENDER PASS')" -ne 1 ] || printf '%s\n' "$output" | grep -q 'MARKDOWN_RENDER FAIL'; then
    printf 'FAIL the rendered preview missed a pixel fact\n'
    printf '%s\n' "$output" | grep -aE 'MARKDOWN_RENDER|ERROR|error'
    exit 1
fi
check_warnings "$output" 0 || exit 1
shot=$(ls "$test_root/runtime"/markdown-render-*-base.png 2>/dev/null | head -1)
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ] && [ -n "$shot" ]; then
    mkdir -p "$FLEA_CI_SUITE_LOGS" || exit 1
    cp "$shot" "$FLEA_CI_SUITE_LOGS/markdown-render.png" || exit 1
    large_shot=$(ls "$test_root/runtime"/markdown-render-*-large.png 2>/dev/null | head -1)
    if [ -z "$large_shot" ]; then
        printf 'FAIL markdown-render: large shot expected %s/runtime/markdown-render-*-large.png; arrived [<missing>]\n' "$test_root" >&2
        exit 1
    fi
    cp "$large_shot" "$FLEA_CI_SUITE_LOGS/markdown-render-large.png" || exit 1
    printf 'shot %s\n' "$FLEA_CI_SUITE_LOGS/markdown-render.png"
elif [ -n "$shot" ]; then
    printf 'shot %s\n' "$shot"
fi
printf '%s\n' "$output" | grep -oE 'MARKDOWN_RENDER (CHECK|body=|PASS).*'

# Keyboard scrolling reaches both Markdown views and the plain text pane through Quick Look.
cp tests/preview-scroll.qml "$test_root/config/shell.qml" || exit 1
for note in $(seq 1 16); do printf '## Section %s\n\n' "$note"; for word in $(seq 1 $((note % 9 + 15))); do printf 'A long wrapping paragraph with variable height. '; done; printf '\n\n'; done > "$test_root/scroll.md"
cp "$test_root/scroll.md" "$test_root/scroll.txt" || exit 1
scroll_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_SCROLL_DOC="$test_root/scroll.md" FLEA_SCROLL_TEXT="$test_root/scroll.txt" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$scroll_output" | grep -oE 'PREVIEW_SCROLL .*'
check_warnings "$scroll_output" 0 || exit 1
if ! printf '%s\n' "$scroll_output" | grep -qF 'PREVIEW_SCROLL 44 checks, 0 failed'; then
    printf 'FAIL keyboard scrolling: %s\n' "$scroll_output"
    exit 1
fi

cp tests/markdown-source-render.qml "$test_root/config/shell.qml" || exit 1
source_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_MARKDOWN_FIXTURE="$test_root/notes.md" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$source_output" | grep -oE 'MARKDOWN_SOURCE .*'
check_warnings "$source_output" 0 || exit 1
expected_source_checks=15
# Sample input: MARKDOWN_SOURCE 15 checks, 0 failed
if ! printf '%s\n' "$source_output" | grep -qF "MARKDOWN_SOURCE $expected_source_checks checks, 0 failed"; then
    printf 'FAIL markdown-render: Source expected %s checks, 0 failed for %s/notes.md; arrived [%s]\n' "$expected_source_checks" "$test_root" "${source_output:-<empty>}" >&2
    exit 1
fi
if [ -n "${FLEA_CI_SUITE_LOGS:-}" ]; then
    cp "$test_root/runtime/markdown-source.png" "$FLEA_CI_SUITE_LOGS/markdown-source.png" || exit 1
fi

# The heading ink as ui/Theme.qml derives it, over palettes where each source wins and where none does.
cp tests/markdown-headink.qml "$test_root/config/shell.qml" || exit 1
headink_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$headink_output" | grep -oE 'MARKDOWN_HEADINK .*'
check_warnings "$headink_output" 0 || exit 1
expected_headink_checks=9
# Sample input: MARKDOWN_HEADINK 9 checks, 0 failed
if ! printf '%s\n' "$headink_output" | grep -qF "MARKDOWN_HEADINK $expected_headink_checks checks, 0 failed"; then
    printf 'FAIL markdown-render: heading ink expected %s checks, 0 failed; arrived [%s]\n' "$expected_headink_checks" "${headink_output:-<empty>}" >&2
    exit 1
fi

# The last block's picture decodes late through a named pipe the harness feeds, over a document judged at text size 14, all in the sandbox root.
endfit_dir="$test_root/endfit"
mkdir -p "$endfit_dir" || exit 1
mkfifo "$endfit_dir/late.png" || exit 1
endfit_notes=60
{
    printf '# Field notes\n\n## What shipped\n\nA paragraph with `thumbCap` inline code and [a guide](https://example.com/guide).\n\n'
    printf '| Kind | Asks for |\n| :--- | :--- |\n| rows | the cursor |\n\n'
    printf '```js\nvar fenced = true;\n```\n'
    for _note in $(seq 1 "$endfit_notes"); do
        printf '\nNote %s of the fixture, plain text that only gives the document height.\n' "$_note"
    done
    printf '\n![late](./late.png)\n'
} > "$endfit_dir/endfit.md" || exit 1
# The board's 160 by 80 stand-in, written inside the sandbox only.
python3 - "$endfit_dir/late.bytes" <<'PY' || exit 1
import struct, sys, zlib
def chunk(tag, body):
    return struct.pack('>I', len(body)) + tag + body + struct.pack('>I', zlib.crc32(tag + body) & 0xffffffff)
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 160, 80, 8, 2, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress((b'\0' + b'\x40\x80\xc0' * 160) * 80)) + chunk(b'IEND', b'')
open(sys.argv[1], 'wb').write(png)
PY
cp tests/markdown-endfit.qml "$test_root/config/shell.qml" || exit 1
endfit_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_ENDFIT_DOC="$endfit_dir/endfit.md" \
    FLEA_ENDFIT_BYTES="$endfit_dir/late.bytes" FLEA_ENDFIT_FIFO="$endfit_dir/late.png" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 60 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$endfit_output" | grep -oE 'MARKDOWN_ENDFIT .*'
# Qt warns once that a pipe cannot seek, which is what holds the decode open, so that one line is not a defect.
check_warnings "$(printf '%s\n' "$endfit_output" | grep -vF 'QFile::at: Cannot set file position')" 0 || exit 1
expected_endfit_checks=12
# Sample input: MARKDOWN_ENDFIT 12 checks, 0 failed
if ! printf '%s\n' "$endfit_output" | grep -qF "MARKDOWN_ENDFIT $expected_endfit_checks checks, 0 failed"; then
    printf 'FAIL markdown-render: end of a late picture expected %s checks, 0 failed; arrived [%s]\n' "$expected_endfit_checks" "${endfit_output:-<empty>}" >&2
    exit 1
fi

# The preview's Loader is torn down while the list builds its cache, which must leave no incubation warning, over a document of pictures in items and quotes.
teardown_dir="$test_root/teardown"
mkdir -p "$teardown_dir" || exit 1
{
    printf '# Picture\n\nText above.\n\n![wide](./wide.png) by hand.\n\n- An item.\n- ![wide](./wide.png) by hand.\n- ![wide](./wide.png) by hand.\n\n'
    printf '> A quote.\n>\n> ![wide](./wide.png) by hand.\n\n1. ![wide](./wide.png) by hand.\n\n   Second paragraph.\n\n   ```js\n   var a = 1;\n   ```\n\n'
    printf '| a | b |\n|--|--|\n| 1 | 2 |\n'
    for _note in $(seq 1 40); do printf '\nNote %s plain text.\n\n- x ![wide](./wide.png)\n' "$_note"; done
} > "$teardown_dir/teardown.md" || exit 1
# A picture of 400 by 80, wider than the card is narrow, written inside the sandbox only.
python3 - "$teardown_dir/wide.png" <<'PY' || exit 1
import struct, sys, zlib
def chunk(tag, body):
    return struct.pack('>I', len(body)) + tag + body + struct.pack('>I', zlib.crc32(tag + body) & 0xffffffff)
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 400, 80, 8, 2, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress((b'\0' + b'\x40\x80\xc0' * 400) * 80)) + chunk(b'IEND', b'')
open(sys.argv[1], 'wb').write(png)
PY
cp tests/markdown-teardown.qml "$test_root/config/shell.qml" || exit 1
teardown_output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_TEARDOWN_DOC="$teardown_dir/teardown.md" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 60 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
printf '%s\n' "$teardown_output" | grep -oE 'MARKDOWN_TEARDOWN .*'
check_warnings "$teardown_output" 0 || exit 1
# Sample input: MARKDOWN_TEARDOWN 300 rounds, 300 torn down with a parsed list; the stage logs FAIL instead of this line when no round had one.
if ! printf '%s\n' "$teardown_output" | grep -qF "MARKDOWN_TEARDOWN 300 rounds"; then
    printf 'FAIL markdown-render: teardown expected 300 rounds; arrived [%s]\n' "${teardown_output:-<empty>}" >&2
    exit 1
fi
if printf '%s\n' "$teardown_output" | grep -qE 'Cannot create delegate|destroyed during incubation'; then
    printf 'FAIL markdown-render: the preview Loader was torn down mid-incubation\n' >&2
    exit 1
fi
