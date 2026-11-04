#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Gate off-thread parsing and viewport delegates for a generated README of about 560 KiB.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
. "$(dirname "$0")/qslog-gate.sh"
cd "$(dirname "$0")/.." || exit 1

check_report() {
    local output=$1 line blocks delegates offthread minimum_blocks minimum_delegates maximum_delegates delegate_fraction_divisor
    # Reject a fixture too small to establish viewport-bounded rendering.
    minimum_blocks=800
    # A blank pane establishes no rendered viewport.
    minimum_delegates=1
    # Bound live delegates independently of the document's total block count.
    maximum_delegates=150
    # Require fewer than one quarter of the document's blocks to have live delegates.
    delegate_fraction_divisor=4
    # Sample input: MARKDOWN_LAZY blocks=12500 delegates=12 offthread=true.
    line=$(printf '%s\n' "$output" | grep -aE 'MARKDOWN_LAZY blocks=' | head -1)
    if [ -z "$line" ]; then
        printf 'FAIL the lazy harness never reported (no live preview ran)\n'
        printf '%s\n' "$output" | grep -aE 'MARKDOWN_LAZY|ERROR|error' | head -20
        return 1
    fi
    blocks=$(printf '%s\n' "$line" | grep -aoE 'blocks=[0-9]+' | grep -aoE '[0-9]+')
    delegates=$(printf '%s\n' "$line" | grep -aoE 'delegates=[0-9]+' | grep -aoE '[0-9]+')
    offthread=$(printf '%s\n' "$line" | grep -aoE 'offthread=[a-z]+' | cut -d= -f2)
    [ "$offthread" = "true" ] || {
        echo "FAIL the parse never left the UI thread"
        return 1
    }
    [ "$blocks" -ge "$minimum_blocks" ] || {
        echo "FAIL only $blocks blocks, the fixture is no test"
        return 1
    }
    [ "$delegates" -ge "$minimum_delegates" ] || {
        echo "FAIL zero delegates, the pane is blank"
        return 1
    }
    [ "$delegates" -le "$maximum_delegates" ] || {
        echo "FAIL $delegates delegates for $blocks blocks, nothing is lazy"
        return 1
    }
    [ "$delegates" -lt "$((blocks / delegate_fraction_divisor))" ] || {
        echo "FAIL $delegates delegates approach $blocks blocks"
        return 1
    }
    printf 'PASS %s blocks draw through %s delegates, parsed off thread\n' "$blocks" "$delegates"
}

if check_report "MARKDOWN_LAZY blocks=12500 delegates=0 offthread=true" >/dev/null; then
    echo "FAIL zero delegate report was accepted"
    exit 1
fi
check_report "MARKDOWN_LAZY blocks=12500 delegates=12 offthread=true" >/dev/null || exit 1
printf "ok lazy wrapper rejects zero and accepts drawn delegates\n"

# A hang bound for each qs run, not a speed budget: the harness quits itself after its last line.
qs_bound_s=600
# Sample input: 124 is the status timeout gives a run it stopped; 143 is the harness's own kill after its last line.
check_finished() {
    [ "$1" -ne 124 ] || { printf 'FAIL the %s run did not finish inside %ss\n' "$2" "$qs_bound_s"; return 1; }
}
if check_finished 124 test >/dev/null; then
    echo "FAIL a run stopped at the bound was accepted"
    exit 1
fi
check_finished 143 test >/dev/null || exit 1

if ! command -v qs >/dev/null; then
    echo "markdown-lazy.sh: qs is not installed, cannot render the preview"
    exit 1
fi

test_root="$FIXTURE_ROOT/flea-markdown-lazy-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/cache" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1
cp -a ui "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/markdown-lazy.qml "$test_root/config/shell.qml" || exit 1

# Each section has five source blocks; the suite prints the parser's actual emitted count.
fixture_sections=2500
for ((section = 0; section < fixture_sections; section++)); do
    printf '## Section %s\n\nParagraph %s carries enough words to wrap a couple of lines in the frame.\n\n- item %s alpha\n- item %s beta\n\n| Kind | Asks for |\n| :--- | :--- |\n| rows %s | the cursor |\n\n```js\nvar section%s = true;\n```\n\n' \
        "$section" "$section" "$section" "$section" "$section" "$section"
done > "$test_root/notes.md" || exit 1
# The final fence ends in exactly one newline; only the separators between sections keep their blank line.
python3 -I -c 'import sys; p = sys.argv[1]; data = open(p, "rb").read(); assert data.endswith(b"\n\n"); open(p, "wb").write(data[:-1])' "$test_root/notes.md" || exit 1
printf 'readme bytes: %s\n' "$(stat -c %s "$test_root/notes.md")"
# Require more than half a MiB so the fixture exercises a substantial document.
minimum_fixture_bytes=524288
[ "$(stat -c %s "$test_root/notes.md")" -gt "$minimum_fixture_bytes" ] || { echo "FAIL the README fixture is too small"; exit 1; }

output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_MARKDOWN_FIXTURE="$test_root/notes.md" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="$(qslog_rules "${QT_LOGGING_RULES:-}")" \
    timeout "$qs_bound_s" qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
check_finished "$?" logged || exit 1

printf '%s\n' "$output" | qslog_nullptr markdown-lazy || exit 1
# The same document with no logging rules or qtlogging.ini: the announcement stays off, and the null connect lines prove a worker did start.
quiet=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u QT_LOGGING_RULES -u QT_LOGGING_CONF \
    HOME="$test_root/home" XDG_CONFIG_HOME="$test_root/home/.config" XDG_CONFIG_DIRS="$test_root/xdg" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_MARKDOWN_FIXTURE="$test_root/notes.md" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout "$qs_bound_s" qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
check_finished "$?" default || exit 1
printf '%s\n' "$quiet" | qslog_silent markdown-lazy || exit 1
if printf '%s\n' "$output" | grep -q 'MARKDOWN_LAZY FAIL'; then
    printf 'FAIL the lazy harness refused its fixture\n'
    printf '%s\n' "$output" | grep -aE 'MARKDOWN_LAZY|ERROR' | head -10
    exit 1
fi
check_report "$output" || exit 1
# Sample input: MARKDOWN_LAZY source chars=8190 total=573019 endlaid=12286 lastline=true.
check_source() {
    local line chars total endlaid reached
    # The Source view lays out a few chunks of 4096 characters, never the whole file, at the top and at the end.
    local max_chars=24576
    line=$(printf '%s\n' "$1" | grep -aE 'MARKDOWN_LAZY source chars=' | head -1)
    [ -n "$line" ] || { echo "FAIL the Source view never reported"; return 1; }
    chars=$(printf '%s\n' "$line" | grep -aoE 'chars=[0-9]+' | grep -aoE '[0-9]+')
    total=$(printf '%s\n' "$line" | grep -aoE 'total=[0-9]+' | grep -aoE '[0-9]+')
    endlaid=$(printf '%s\n' "$line" | grep -aoE 'endlaid=[0-9]+' | grep -aoE '[0-9]+')
    reached=$(printf '%s\n' "$line" | grep -aoE 'lastline=[a-z]+' | cut -d= -f2)
    [ "$chars" -gt 0 ] && [ "$chars" -le "$max_chars" ] && [ "$endlaid" -le "$max_chars" ] || { echo "FAIL the Source view laid out $chars then $endlaid of $total characters, bound $max_chars"; return 1; }
    [ "$reached" = true ] || { echo "FAIL the end of the Source list never showed the file's last line"; return 1; }
    printf 'PASS Source lays out %s then %s of %s characters and reaches the last line\n' "$chars" "$endlaid" "$total"
}
if check_source "MARKDOWN_LAZY source chars=573019 total=573019 endlaid=573019 lastline=true" >/dev/null; then
    echo "FAIL a Source view that laid out the whole file was accepted"
    exit 1
fi
check_source "$output" || exit 1
# Sample input: MARKDOWN_LAZY source roundtrip chunks=140 same=true.
check_roundtrip() {
    local line chunks same
    line=$(printf '%s\n' "$1" | grep -aE 'MARKDOWN_LAZY source roundtrip' | head -1)
    [ -n "$line" ] || { echo "FAIL the Source chunks never reported their round trip"; return 1; }
    chunks=$(printf '%s\n' "$line" | grep -aoE 'chunks=[0-9]+' | grep -aoE '[0-9]+')
    same=$(printf '%s\n' "$line" | grep -aoE 'same=[a-z]+' | cut -d= -f2)
    [ "$chunks" -gt 1 ] && [ "$same" = true ] || { echo "FAIL $chunks Source chunks do not put the file back (same=$same)"; return 1; }
    printf 'PASS %s Source chunks put the file back exactly\n' "$chunks"
}
if check_roundtrip "MARKDOWN_LAZY source roundtrip chunks=140 same=false" >/dev/null; then
    echo "FAIL chunks that do not put the file back were accepted"
    exit 1
fi
if check_roundtrip "MARKDOWN_LAZY source chars=8190 total=573019 endlaid=12286 lastline=true" >/dev/null; then
    echo "FAIL a run that never reported its round trip was accepted"
    exit 1
fi
check_roundtrip "$output"
