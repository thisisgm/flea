#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
# The render sweep half of the theme battery: the candidate launched once per stock theme, with the
# surfaces the colour rules govern shot and measured. tests/js/themes.js asserts the rules over the
# palettes; this asserts that the roles the window really carries still satisfy them, which is what
# catches ui/Theme.qml and that suite drifting apart, and it leaves one PNG per theme per surface.
set -u
set -o pipefail
# Hard rule 9's own guard owns every create and delete here, the way tests/mount-listing.sh does it:
# a fixture lives outside $HOME and carries a marker, and nothing unmarked is ever removed.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
repo=$PWD
themes_dir=${THEMES_DIR:-/usr/share/omarchy/themes}
flea_ui="$repo/ui"
flea_bin=${FLEA_BIN:-$repo/target/debug/flea}
class=com.thisisgm.flea
failures=0
# The same floors tests/js/themes.js names, and the same rules behind them.
text_min=4.5      # body text on its own ground, WCAG AA
mark_min=3        # a drawn mark or frame is a graphical object, AA at 3:1
role_steps=4      # how far a screenshot's round trip may move a role, measured over all 22 themes

command -v omarchy-drive >/dev/null || { echo "themes.sh: omarchy-drive is not installed"; exit 1; }
command -v magick >/dev/null || { echo "themes.sh: magick is not installed, and the pixel reads need it"; exit 1; }
[ -x "$flea_bin" ] || { echo "themes.sh: no candidate at $flea_bin"; exit 1; }
eval "$(omarchy-drive env)"

. "$repo/tests/lib/stock-themes.sh"
# The pure suite pins the same list; a theme shipped since then reddens here rather than being skipped.
installed=$(stock_themes_installed "$themes_dir" | tr '\n' ' ')
pinned=$(stock_themes_pinned tests/js/themes.js | tr '\n' ' ')
if [ "$installed" != "$pinned" ]; then
    printf 'FAIL the installed themes are not the ones tests/js/themes.js pins\n  installed: %s\n  pinned:    %s\n' "$installed" "$pinned"
    failures=$((failures + 1))
fi

sandbox="$FIXTURE_ROOT/flea-themes-$$"
sandbox_make "$sandbox"
# The PNGs outlive the run as its record, so they take one path the next run replaces rather than a
# fresh mktemp nobody ever removes.
shots="$FIXTURE_ROOT/flea-themeshots"
sandbox_make "$shots"
# Only what this run launched: the candidate is started with setsid, so its own process group holds
# it and the qs it spawns, and nothing the operator started is signalled.
launched=""
# How long a stopped candidate may take to leave before it is failed and killed.
stop_wait_s=10
# How many composited frames the edge check samples, and how far apart, before it calls the edge missing.
edge_shots=10
edge_shot_gap_s=0.3
# How often group_gone looks, and how long a KILLed group may take to go before the run gives up.
group_poll_s=0.1
kill_wait_s=2
# True once no live member of process group $1 remains, polled for at most $2 seconds.
group_gone() {
    local deadline=$((SECONDS + $2)) pid alive
    while :; do
        alive=0
        for pid in $(pgrep -g "$1"); do
            # Sample input, ps -o stat=: "Sl", and a leading Z is a dead member nobody has reaped yet.
            [ "$(ps -o stat= -p "$pid" | cut -c1)" = Z ] || alive=1
        done
        [ "$alive" = 0 ] && return 0
        (( SECONDS < deadline )) || return 1
        sleep "$group_poll_s"
    done
}
# The next theme must not overlap this one: a path-addressed ipc answers from the oldest instance.
stop() {
    [ -n "$launched" ] || return 0
    kill -TERM -- "-$launched" 2>/dev/null
    if ! group_gone "$launched" "$stop_wait_s"; then
        fail "the candidate outlived TERM by $stop_wait_s s, so it was killed"
        kill -KILL -- "-$launched" 2>/dev/null
        group_gone "$launched" "$kill_wait_s" || { printf 'FAIL the candidate group %s survived KILL\n' "$launched"; exit 1; }
    fi
    wait "$launched" 2>/dev/null
    launched=""
}
cleanup() {
    stop
    sandbox_remove "$sandbox"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$sandbox/files" "$sandbox/state/flea" "$sandbox/config"
printf 'one\n' > "$sandbox/files/alpha.txt"
printf 'two\n' > "$sandbox/files/beta.txt"
printf 'three\n' > "$sandbox/files/gamma-needle.txt"
printf '{"view":"list","places":{"driveSize":true,"trashCount":true}}\n' > "$sandbox/state/flea/ui.json"
# A palette that sets no muted of its own, which no installed theme does and a third-party theme can:
# it is the one way to drive ui/Theme.qml's darkened-foreground fallback through the real window.
synthetic="$sandbox/no-muted/colors.toml"
# The name the loop will read back out of the path, so the rule below is bound to the palette this
# wrote rather than to a second copy of its name, and a sweep that never reached it says so.
no_muted_theme=$(basename "$(dirname "$synthetic")")
no_muted_seen=0
mkdir -p "$(dirname "$synthetic")"
printf 'background = "#1e1e2e"\nforeground = "#cdd6f4"\naccent = "#89b4fa"\nred = "#f38ba8"\n' > "$synthetic"
# Qt.darker(#cdd6f4, 1.4) as ui/Theme.qml applies it, which tests/js/themes.js pins as the same
# literal: the two halves meeting on this value is what binds that mirror to the running window.
no_muted_caption="#9299ae"

ipc() { flea_ipc_drive -p "$flea_ui/boot" flea "$@"; }
# Filtered to the window under test, as tests/ui.sh does: an unfiltered send goes wherever focus is.
key() { omarchy-drive key --window com.thisisgm.flea "$@" >/dev/null; }
window_xy() { hyprctl clients -j | jq -r --arg c "$class" '[.[]|select(.class==$c)][0] | "\(.at[0]) \(.at[1])"'; }
fail() { printf 'FAIL %s\n' "$1"; failures=$((failures + 1)); }

# WCAG 2.1 relative luminance, the same arithmetic ui/js/Contrast.js does, so a rule can be asserted
# on the colours the running window reports rather than on a screenshot somebody looked at.
ratio() {
python3 - "$1" "$2" <<'PY'
import sys
def channel(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
def luminance(hex_colour):
    h = hex_colour.lstrip('#')
    h = h[2:] if len(h) == 8 else h
    r, g, b = (int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
a, b = luminance(sys.argv[1]), luminance(sys.argv[2])
print(round((max(a, b) + 0.05) / (min(a, b) + 0.05), 3))
PY
}
at_least() {
    local theme="$1" rule="$2" got="$3" floor="$4"
    awk -v g="$got" -v f="$floor" 'BEGIN { exit !(g >= f) }' \
        || fail "$theme: $rule is $got, under $floor"
}
# Does this ratio clear that floor? A ratio that came back empty is a measurement this sweep did not
# take, so it reddens here rather than skipping the rule it gates in silence.
clears() {
    local theme="$1" rule="$2" got="$3" floor="$4"
    if [ -z "$got" ]; then
        fail "$theme: no ratio came back for $rule"
        return 1
    fi
    awk -v g="$got" -v f="$floor" 'BEGIN { exit !(g >= f) }'
}
# The first of these keys colors.toml sets, as "#rrggbb". Sample input, one line of the file:
#   background = "#1e1e2e"   # the canvas
colour_for() {
    local file="$1" key line hex
    shift
    for key in "$@"; do
        line=$(grep -m1 -E "^$key[[:space:]]*=" "$file")
        # The first hex on the line and no other: a trailing comment on that line carries its own.
        hex=$(printf '%s\n' "$line" | grep -o '#[0-9A-Fa-f]\{6\}' | head -1 | tr 'A-F' 'a-f')
        if [ -n "$hex" ]; then
            printf '%s' "$hex"
            return
        fi
    done
}
# One pixel of a shot, as "#rrggbb": a fill and a frame are solid, so a pixel is the colour itself.
pixel_at() {
    magick "$1" -format "%[hex:p{$2,$3}]" info: | tr 'A-F' 'a-f' | cut -c1-6
}
# A screenshot is not bit exact, up to three steps a channel over all 22 themes, so a role matches within role_steps and never by string.
near_colour() {
    python3 -c '
import sys
def channels(value):
    h = value.lstrip("#")
    h = h[2:] if len(h) == 8 else h
    return [int(h[i:i + 2], 16) for i in (0, 2, 4)]
got, want, steps = channels(sys.argv[1]), channels(sys.argv[2]), int(sys.argv[3])
print("near" if all(abs(a - b) <= steps for a, b in zip(got, want)) else "off")
' "${1#\#}" "$2" "$role_steps"
}
same_colour() {
    local theme="$1" rule="$2" got="${3#\#}" want="$4"
    [ "$(near_colour "$got" "$want")" = "near" ] || fail "$theme: $rule draws #$got, not $want"
}

for colours in "$themes_dir"/*/colors.toml "$synthetic"; do
    theme=$(basename "$(dirname "$colours")")
    # One theme at a time while a rule is being written; the battery runs them all.
    [ -n "${THEMES_ONLY:-}" ] && [ "$theme" != "${THEMES_ONLY}" ] && continue
    home="$sandbox/home"
    sandbox_scratch "$home"
    mkdir -p "$home/.local/state/omarchy/current/theme" "$home/.config/omarchy"
    cp "$colours" "$home/.local/state/omarchy/current/theme/colors.toml"
    [ -f "$(dirname "$colours")/shell.toml" ] && cp "$(dirname "$colours")/shell.toml" "$home/.local/state/omarchy/current/theme/"
    printf '%s\n' "$theme" > "$home/.local/state/omarchy/current/theme.name"
    [ -f "$HOME/.config/omarchy/shell.toml" ] && cp "$HOME/.config/omarchy/shell.toml" "$home/.config/omarchy/"

    stop
    real_home=$HOME
    export HOME="$home"
    FLEA_UI="$flea_ui" FLEA_BIN="$flea_bin" XDG_STATE_HOME="$sandbox/state" XDG_CONFIG_HOME="$sandbox/config" \
        setsid nohup "$flea_bin" --gui "$sandbox/files" >"$sandbox/flea-$theme.log" 2>&1 </dev/null &
    launched=$!
    export HOME="$real_home"
    if ! omarchy-drive wait window "$class" --timeout 20 >/dev/null; then
        fail "$theme: the candidate never opened a window"
        continue
    fi
    omarchy-drive focus "$class" >/dev/null
    # The seam is built after the first listing, so the window has mapped before it answers.
    flea_ipc_wait -p "$flea_ui/boot" flea ready true --timeout 20 >/dev/null \
        || { fail "$theme: the IPC seam never answered ready"; continue; }
    sleep 3
    [ "$(ipc themeLoaded)" = "true" ] || fail "$theme: the window did not load a palette"
    read -r bg surface fg muted accent error symlink executable accentframe mutedframe <<< "$(ipc palette)"
    case "$bg$fg$muted$accent$accentframe" in *"#"*) ;; *) fail "$theme: the window reports no palette"; continue ;; esac
    case "$accentframe" in "#"*) ;; *) fail "$theme: the window reports no accentFrame field"; continue ;; esac
    case "$mutedframe" in "#"*) ;; *) fail "$theme: the window reports no mutedFrame field"; continue ;; esac
    # The keys Commons/Color.qml maps, first match wins, each anchored to its own key so background_dim
    # cannot answer for background. Sample input, one line of colors.toml: background = "#1e1e2e"
    want_bg=$(colour_for "$colours" background color0)
    want_fg=$(colour_for "$colours" foreground color7)
    want_muted=$(colour_for "$colours" muted)
    # An unreadable file is the sweep losing its binding, so it reddens rather than skipping quietly.
    [ -n "$want_bg" ] && [ -n "$want_fg" ] \
        || fail "$theme: colors.toml names no background or foreground this sweep can bind to"
    [ -z "$want_bg" ] || [ "$want_bg" = "$bg" ] \
        || fail "$theme: the window is on $bg, not this theme's own $want_bg"
    [ -z "$want_fg" ] || [ "$want_fg" = "$fg" ] \
        || fail "$theme: the window draws $fg, not this theme's own $want_fg"

    # The rules, on the roles the running window carries rather than on the file it read.
    at_least "$theme" "foreground on background" "$(ratio "$fg" "$bg")" "$text_min"
    at_least "$theme" "foreground on surface" "$(ratio "$fg" "$surface")" "$text_min"
    at_least "$theme" "muted text on background" "$(ratio "$muted" "$bg")" "$text_min"
    at_least "$theme" "muted text on surface" "$(ratio "$muted" "$surface")" "$text_min"
    at_least "$theme" "muted frame on background" "$(ratio "$mutedframe" "$bg")" "$mark_min"
    at_least "$theme" "error on background" "$(ratio "$error" "$bg")" "$text_min"
    at_least "$theme" "symlink on background" "$(ratio "$symlink" "$bg")" "$text_min"
    at_least "$theme" "executable on background" "$(ratio "$executable" "$bg")" "$text_min"
    at_least "$theme" "the primary frame on the card's surface" "$(ratio "$accentframe" "$surface")" "$mark_min"
    # The role is the accent lifted onto the card, so a theme whose accent already clears the floor
    # reports that accent itself, exactly: both come from one palette read, with no shot between them.
    if clears "$theme" "the accent on the card's surface" "$(ratio "$accent" "$surface")" "$mark_min"; then
        [ "$accentframe" = "$accent" ] \
            || fail "$theme: the primary frame is $accentframe, not this theme's own accent $accent"
    fi
    # The text role is the palette's own muted lifted to the text floor on both grounds, so a theme already clearing
    # it there reports that muted itself: this is what binds the role tests/js/themes.js mirrors to the file.
    if [ -n "$want_muted" ] && clears "$theme" "the file's own muted on the background" "$(ratio "$want_muted" "$bg")" "$text_min" \
        && clears "$theme" "the file's own muted on the surface" "$(ratio "$want_muted" "$surface")" "$text_min"; then
        [ "$want_muted" = "$muted" ] \
            || fail "$theme: the caption draws $muted, not this theme's own $want_muted"
    fi
    # And with no muted key at all the window darkens the foreground rather than drawing it, which is
    # the fallback tests/js/themes.js mirrors. The value is this palette's own, so the rule is its own:
    # an installed theme that ships no muted would darken a different foreground and is not this case.
    if [ "$theme" = "$no_muted_theme" ]; then
        no_muted_seen=1
        [ "$muted" = "$no_muted_caption" ] \
            || fail "$theme: with no muted of its own the caption is $muted, not the darkened $no_muted_caption"
    fi

    # The cursor row's own accent edge, marked and hovered rows beside it: a shot of all three.
    # Waited for, not slept over: a fixed sleep sampled a frame predating the last key, 1 run in 6.
    key j
    key v
    key j
    if ! flea_ipc_wait -p "$flea_ui/boot" flea cursor 2 --timeout 10 >/dev/null; then
        fail "$theme: the cursor never reached row 2, so no shot of it can be measured"
        continue
    fi
    read -r wx wy <<< "$(window_xy)"
    read -r cx cy <<< "$(ipc rowCentre 2)"
    [ -n "${cx:-}" ] && omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null
    sleep 1
    omarchy-drive shot "$shots/$theme-list.png" "$class" >/dev/null
    # The edge is a solid bar at the cursor row's leading corner, so its own pixel is the accent itself.
    read -r rx ry rw rh <<< "$(ipc rowRect "$(ipc cursor)")"
    if [ -n "${rx:-}" ]; then
        edge=$(pixel_at "$shots/$theme-list.png" "$((rx + 1))" "$((ry + rh / 2))")
        shot_n=1
        # ipc confirms the model, not a composited frame: one run shot its first frame 1.5 s after cursor 2.
        while [ "$(near_colour "$edge" "$accent")" != near ] && [ "$shot_n" -lt "$edge_shots" ]; do
            sleep "$edge_shot_gap_s"
            shot_n=$((shot_n + 1))
            omarchy-drive shot "$shots/$theme-list.png" "$class" >/dev/null
            edge=$(pixel_at "$shots/$theme-list.png" "$((rx + 1))" "$((ry + rh / 2))")
        done
        if [ "$shot_n" -gt 1 ] && [ "$(near_colour "$edge" "$accent")" = near ]; then
            printf 'NOTE %s: the accent edge was composited on shot %s of %s\n' "$theme" "$shot_n" "$edge_shots"
        fi
        fails_before=$failures
        same_colour "$theme" "the cursor's accent edge" "$edge" "$accent"
        if [ "$failures" -gt "$fails_before" ]; then
            # Sample input, clients -j: [{"class":"com.thisisgm.flea","workspace":{"id":1},"mapped":true,"hidden":false,"focusHistoryID":0}], activeworkspace -j: {"id":1,"name":"1"}
            printf 'NOTE %s: model cursor %s, window %s, active workspace %s\n' "$theme" "$(ipc cursor)" \
                "$(hyprctl clients -j | jq -c --arg c "$class" '[.[] | select(.class == $c) | {ws: .workspace.id, mapped, hidden, focus: .focusHistoryID}]')" \
                "$(hyprctl activeworkspace -j | jq .id)"
        fi
    else
        fail "$theme: the cursor row has no rectangle to measure"
    fi

    # The settings card, whose checkbox draws in the foreground and muted roles the rules above already
    # measure; it is shot for the reviewer rather than measured a second time here.
    key ,; sleep 2
    omarchy-drive shot "$shots/$theme-settings.png" "$class" >/dev/null
    key -k Escape; sleep 1

    # The dialog is the reviewer's record: the seam publishes no rectangle for a button, and a search
    # of the whole window passes on any accent pixel in it, so the role check above is the rule.
    key a; sleep 2
    omarchy-drive key --window "$class" nas >/dev/null
    sleep 1
    omarchy-drive shot "$shots/$theme-dialog.png" "$class" >/dev/null
    key -k Escape; sleep 1

    # A search run, whose matches carry the one wash, and the strip that reports it.
    key f; sleep 1
    omarchy-drive key --window "$class" needle >/dev/null
    key -k Return; sleep 3
    omarchy-drive shot "$shots/$theme-search.png" "$class" >/dev/null
    key -k Escape; sleep 1

    printf 'THEME %s bg=%s fg=%s muted=%s accent=%s error=%s\n' "$theme" "$bg" "$fg" "$muted" "$accent" "$error"
done

# One palette owns that rule, so a sweep that never ran it has not checked the fallback at all, and
# the 22 themes that do ship a muted key would carry the suite green past a missing synthetic palette.
[ -n "${THEMES_ONLY:-}" ] || [ "$no_muted_seen" = 1 ] \
    || fail "the $no_muted_theme palette never ran, so the darkened-foreground fallback went unchecked"

stop
printf 'SHOTS %s\n' "$shots"
printf '%s theme checks failed\n' "$failures"
exit "$((failures > 0))"
