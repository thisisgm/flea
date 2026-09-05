#!/usr/bin/env bash
# Drives the real Quickshell window with omarchy-drive and asserts through the read-only IPC seam.
# Usage: ./tests/ui.sh [cursor|terminal|open|openterminal|click|menu|hidden|selection|select|colour|lifted|icons|thumbs|hashcache|stale|nosweep|oem|header|overflow|focus|preview|network|sharebrowser|unmount|eject|rename|renamelife|taildrop|grid|columns|operations|tabs ...]; no argument runs all thirty-two.
set -u
set -o pipefail
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete this suite makes.
. "$(dirname "$0")/../tools/flea-sandbox-guard"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

export PATH="$HOME/.local/bin:$PATH"
eval "$(omarchy-drive env)"
# omarchy-drive env omits the Qt platform theme, without which no icon name resolves; the session publishes it here.
# Omarchy sets this with hl.env in /usr/share/omarchy/default/hypr/envs.lua, so it reaches a client
# Hyprland launched and is not necessarily in the systemd user environment at all. Both are read, and
# the last resort is a real client's own environ, which is the only authoritative answer.
platform_theme() {
    systemctl --user show-environment | grep '^QT_QPA_PLATFORMTHEME=' | cut -d= -f2-
}

platform_theme_from_a_live_client() {
    local pid
    pid=$(pgrep -x quickshell | head -1)
    [[ -n "$pid" ]] || return 0
    tr '\0' '\n' < "/proc/$pid/environ" | grep '^QT_QPA_PLATFORMTHEME=' | cut -d= -f2-
}

if [[ -z "${QT_QPA_PLATFORMTHEME:-}" ]]; then
    export QT_QPA_PLATFORMTHEME="$(platform_theme)"
fi
if [[ -z "$QT_QPA_PLATFORMTHEME" ]]; then
    export QT_QPA_PLATFORMTHEME="$(platform_theme_from_a_live_client)"
fi
[[ -n "$QT_QPA_PLATFORMTHEME" ]] || fail "no session, and no running client, published QT_QPA_PLATFORMTHEME, so every row would draw no icon at all"

repo="$(cd "$(dirname "$0")/.." && pwd)"
flea_ui="$repo/ui"
flea_bin="${FLEA_BIN:-$repo/target/release/flea}"
# Sample input: let finished = Command::new("gio")
# Every opener stub below is named from the product's own exec target, the same derivation
# tests/modes.sh makes: a stub named by hand goes stale the day the target is renamed, and the run
# then resolves the operator's real launcher instead. That is what left three editor pairs resident
# on this box, and this suite is the one that did it.
handoff_in() {
    grep -ho 'Command::new("[a-z0-9-]\+")' "$1" | cut -d'"' -f2 | sort -u
}
open_handoff=$(handoff_in "$repo/src/open.rs")
# Fail closed rather than write a stub nothing calls, which is the fall-through this prevents: a
# suite that stubs the wrong name reports green having handed the operator's own launcher a file.
case "$open_handoff" in
    ''|*[!a-z0-9-]*) fail "src/open.rs must name exactly one handoff; got '$open_handoff'" ;;
esac
bench_dir="${FLEA_BENCH_DIR:-$FIXTURE_ROOT/flea-bench-btrfs}"
# Every root sits under the fixture root and takes no override, because a root the environment can
# replace is a root nothing checks: these four are deleted whole on every exit path.
fixture_root="$FIXTURE_ROOT/flea-ui-fixtures-$$"
# Hard links need the media fixture's filesystem, and the pid keeps a previous run's cache entries out of this delta.
thumb_fixture="$FIXTURE_ROOT/flea-ui-thumbs-$$"
# Its own tree because case_hashcache redirects the whole cache root into it.
hash_fixture="$FIXTURE_ROOT/flea-ui-hash-$$"
# Its own tree again, because case_stale redirects the cache root as well and regenerates an entry inside it.
stale_fixture="$FIXTURE_ROOT/flea-ui-stale-$$"
thumb_rows=200
# A settle is 120 ms and a round trip through the pool is tens of ms, so a screen has a second.
thumb_fill_s=20
# ydotool delivers 200 detents in 0.15 s, so a fling has to be this long to outlast one IPC sample.
fling_clicks=1500
# The backend's own DRAIN_LIMIT is 25 s, so anything alive past this is wedged rather than draining.
drain_wait_s=30
# Hard rule 9 covers writes, not only deletes: an overridable path that is truncated or written into
# is the same hazard as one that is deleted, so both of these are pinned rather than taken from the
# environment. Neither override had a caller.
evidence_dir=/tmp/flea-ui-evidence
# Quickshell truncates nothing, so each case gets a fresh log and every log lands in the run log.
flea_log=/tmp/flea.log
run_log=/tmp/flea-ui-run.log
# One case's own output, re-read for the refusal check rather than piped. Pid-scoped like every
# fixture root here, because two runs sharing it would read each other's output, and truncated before
# each case because a failed redirect would otherwise leave the previous case's bytes for the
# refusal grep to find and report a refusal for a case that never ran.
case_log=/tmp/flea-ui-case-$$.log

# Ten bursts of twelve clicks moved the 100k viewport about eleven rows when measured.
scroll_bursts=10
wheel_clicks=12
# The status bar clears a transient message after 4000 ms, so a persistent state must outlive that.
transient_clear_s=5
# ui/NetworkMounts.qml polls mounts every 5000 ms, so outlasting one poll needs more than that.
rail_poll_wait_s=7
# The window coalescer is 16 ms and a refill is a round trip, so injected input needs a moment.
settle_s=0.4
# The Hyprland corner arc shows wallpaper through the window's own top-left pixels, so start past it.
header_sample_x=16
header_sample_width=600
# Nothing else in the header palette is warm, so red well ahead of blue is the accent role and only that.
accent_warmth=0.04
# Anything brighter than the background is a header label or the header rule.
lit_level=0.13
# The "Name" label alone measured about 150 lit pixels, so this floor survives a darker theme.
header_lit_floor=100
# Qt's Image.status enum: Null 0, Ready 1, Loading 2, Error 3.
image_ready=1
image_loading=2
# A flat red thumbnail replaced by a flat blue one, so one chroma test tells the old frame from the new.
icon_red='u.r > 0.5 && u.b < 0.3'
icon_blue='u.b > 0.5 && u.r < 0.3'
# A day back, so the mtime certainly differs and the freedesktop Thumb::MTime stamp certainly misses.
stale_mtime_back_s=86400
# A local file playing through the ffmpeg backend starts well inside a second; twenty times that is generous.
preview_play_wait_s=5

ipc() {
    omarchy-drive ipc -p "$flea_ui" flea "$@"
}

settle() {
    sleep "$settle_s"
}

flea_pids() {
    local pid
    for pid in $(pgrep -x qs || true); do
        [[ -r "/proc/$pid/cmdline" ]] || continue
        # Redirections apply left to right, so the silencer has to precede the read it is silencing.
        if tr '\0' ' ' 2>/dev/null < "/proc/$pid/cmdline" | grep -Fq "$flea_ui"; then
            printf '%s\n' "$pid"
        fi
    done
}

# Any Flea from this checkout that we did not start, captured once before anything is killed. The
# operator works at this box, and flea_pids cannot tell their window from ours: both match "$flea_ui".
foreign_pids=$(flea_pids | tr '\n' ' ')
if [[ -n "${foreign_pids// /}" ]]; then
    printf 'REFUSED a Flea from %s is already running (pid%s %s)\n' \
        "$flea_ui" "$( [[ $(wc -w <<< "$foreign_pids") -gt 1 ]] && printf s )" "${foreign_pids% }"
    printf 'REFUSED this suite kills every Flea it finds, so it will not run beside one it did not start.\n'
    printf 'REFUSED close it, or run the suite against a git archive export at another path.\n'
    exit 1
fi

# The backend outlives the qs that spawned it, and only its own drain may publish or remove its temps.
backend_pids() {
    local pid
    for pid in $(pgrep -x flea || true); do
        [[ -r "/proc/$pid/cmdline" ]] || continue
        if tr '\0' ' ' 2>/dev/null < "/proc/$pid/cmdline" | grep -Fq -- "$flea_bin --backend"; then
            printf '%s\n' "$pid"
        fi
    done
}

flea_pid() {
    local -a pids
    mapfile -t pids < <(flea_pids)
    [[ ${#pids[@]} -eq 1 ]] || fail "expected one exact Flea qs pid, got ${#pids[@]}"
    printf '%s\n' "${pids[0]}"
}

kill_flea() {
    local pid found waited
    for pid in $(flea_pids); do
        [[ " $foreign_pids " == *" $pid "* ]] && continue
        kill "$pid"
    done
    while :; do
        found=0
        for pid in $(flea_pids); do
            [[ " $foreign_pids " == *" $pid "* ]] && continue
            found=1
        done
        [[ "$found" -eq 0 ]] && break
        sleep 0.05
    done
    # Killing qs closes the backend's stdin, and it keeps publishing into the shared cache until its drain ends.
    for waited in $(seq 1 $((drain_wait_s * 20))); do
        found=0
        for pid in $(backend_pids); do
            found=1
        done
        [[ "$found" -eq 0 ]] && return
        sleep 0.05
    done
    fail "a backend was still draining after $drain_wait_s s, so no cache count can be trusted"
}

# The four roots carry the markers; every per-case directory inside them is a scratch, so a listing
# a case asserts on holds exactly what that case put there.
sandbox_make "$fixture_root"
sandbox_make "$thumb_fixture"
sandbox_make "$hash_fixture"
sandbox_make "$stale_fixture"

cleanup() {
    local wedged=0
    # fail is an exit that || true cannot catch, so the reap runs in a subshell and its status is re-raised below.
    ( kill_flea ) || wedged=1
    local root
    for root in "$fixture_root" "$thumb_fixture" "$hash_fixture" "$stale_fixture"; do
        sandbox_remove "$root"
    done
    rm -f "$case_log"
    cache_restore
    if [[ "$wedged" -eq 1 ]]; then
        printf 'FAIL drain at exit\n'
        exit 1
    fi
}

# src/backend/thumbcache.rs honours XDG_CACHE_HOME, so the whole run's thumbnails land inside the
# fixture root and the operator's real cache is never written to. Two cases already redirect into
# their own sandbox; this is the same move, made once for the run.
export XDG_CACHE_HOME="$fixture_root/cache"
cache_large="$fixture_root/cache/thumbnails/large"
# The operator's own, read twice and never written, only to prove the redirect actually held.
real_cache_large="$HOME/.cache/thumbnails/large"
sandbox_cache_require "$real_cache_large"
real_cache_before=$(ls -A "$real_cache_large" 2>/dev/null | wc -l)

cache_snapshot() { :; }

# ls -A, never ls: a cache audited with plain ls is blind to every dotfile temp in it.
cache_restore() {
    printf 'CACHE scratch=%s operator before=%s after=%s\n' \
        "$(ls -A "$cache_large" 2>/dev/null | wc -l)" \
        "$real_cache_before" "$(ls -A "$real_cache_large" 2>/dev/null | wc -l)"
}

# The class every keystroke in this suite is aimed at.
flea_window_class=com.thisisgm.flea

command -v hyprctl >/dev/null || fail "no hyprctl on PATH, so no keystroke could be checked against the focused window"
command -v jq >/dev/null || fail "no jq on PATH, so no keystroke could be checked against the focused window"

# Empty when hyprctl errors, when the instance signature is unset, and when nothing at all is focused.
focused_class() {
    hyprctl activewindow -j 2>/dev/null | jq -r 'select(.class != null) | .class' 2>/dev/null
}

# Positive equality against the expected class: a negated test reads an empty class as a pass.
assert_focus() {
    local seen
    seen=$(focused_class || true)
    [[ "$seen" == "$flea_window_class" ]] \
        || fail "focus is on class '$seen', not $flea_window_class, so this case sends no more keys"
}

# Every keystroke goes through here, because a rule each case has to remember is not a gate.
key() {
    assert_focus
    omarchy-drive key --window flea "$@"
}

hotkey() {
    assert_focus
    omarchy-drive hotkey "$@"
}

# Captured while HOME is still the operator's, because four cases fake it for fixture isolation.
real_state_dir="$HOME/.local/state/omarchy/current"
# Color.qml layers this over the theme's own shell.toml, and [font] base-size lives here, so a
# fixture home without it renders at Omarchy's stock 12 while this box runs 14.
real_user_shell_toml="$HOME/.config/omarchy/shell.toml"

# Sample input, one line of the live colors.toml:
# foreground        = "#c8ccd0"   # content ink
real_foreground=$(grep -E '^foreground' "$real_state_dir/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
[[ -n "$real_foreground" ]] || fail "no foreground in $real_state_dir/theme/colors.toml, so no shot could be checked against the live palette"

# Theme.stateDir is built from $HOME, so a window under a fixture HOME paints Theme's own fallback
# palette rather than the live theme, which is how two README shots shipped Catppuccin colours.
# themeLoaded is asserted too but cannot carry this alone: it says only that some palette parsed, and
# the foreground comparison below is what pins the shot to this box's own live one.
assert_theme() {
    local loaded seen
    for _attempt in $(seq 1 100); do
        seen=$(ipc themeForeground 2>/dev/null || true)
        [[ -n "$seen" ]] && break
        sleep 0.05
    done
    loaded=$(ipc themeLoaded 2>/dev/null || true)
    [[ "$loaded" == "true" ]] || fail "themeLoaded is '$loaded', not true, so this window has no parsed palette"
    [[ "${seen,,}" == "${real_foreground,,}" ]] \
        || fail "this window paints foreground '$seen', not the live theme's $real_foreground, so no shot or colour claim from it is real"
}

# Theme reads exactly these four files, and they are copied rather than symlinked so that nothing
# inside a sandbox this suite rm -rf's ever points back out at the operator's home.
fixture_home_make() {
    local home="$1" theme="$1/.local/state/omarchy/current"
    sandbox_scratch "$home"
    mkdir -p "$theme/theme"
    cp "$real_state_dir/theme/colors.toml" "$theme/theme/colors.toml"
    cp "$real_state_dir/theme/shell.toml" "$theme/theme/shell.toml"
    cp "$real_state_dir/theme.name" "$theme/theme.name"
    # A box with no user shell.toml really does render at 12, so its absence is copied faithfully too.
    if [[ -f "$real_user_shell_toml" ]]; then
        mkdir -p "$home/.config/omarchy"
        cp "$real_user_shell_toml" "$home/.config/omarchy/shell.toml"
    fi
}

assert_window() {
    local count class
    count=$(omarchy-drive windows --json | jq '[.windows[] | select(.title == "Flea")] | length')
    [[ "$count" == "1" ]] || fail "expected one Flea window, got $count"
    class=$(omarchy-drive windows --json | jq -r '.windows[] | select(.title == "Flea") | .class')
    [[ "$class" == "$flea_window_class" ]] || fail "unexpected Flea class '$class'"
    assert_theme
}

launch() {
    local start_path="$1"
    kill_flea
    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    : > "$flea_log"
    FLEA_PATH="$start_path" FLEA_BIN="$flea_bin" \
        setsid nohup qs -p "$flea_ui" >"$flea_log" 2>&1 </dev/null &
    omarchy-drive wait window flea --timeout 15 >/dev/null
    omarchy-drive focus flea >/dev/null
    assert_window
    printf 'LAUNCH path=%q pid=%s\n' "$start_path" "$(flea_pid)"
}

wait_listing() {
    local want_total="$1"
    local total row
    for _attempt in $(seq 1 300); do
        total=$(ipc total 2>/dev/null || printf unavailable)
        row=$(ipc rowAt 0 2>/dev/null || printf loading)
        if [[ "$total" == "$want_total" && "$row" != "loading" ]]; then
            return
        fi
        sleep 0.05
    done
    fail "listing did not reach total $want_total, got total=$total row=$row"
}

wait_path() {
    local want="$1" seen
    for _attempt in $(seq 1 300); do
        seen=$(ipc path 2>/dev/null || printf unavailable)
        [[ "$seen" == "$want" ]] && return
        sleep 0.05
    done
    fail "the pane never opened $want, it is at $seen"
}

# The rail's two FileViews load asynchronously, so a key pressed right after launch can race them.
wait_rail() {
    local want="$1" count
    for _attempt in $(seq 1 300); do
        count=$(ipc railCount 2>/dev/null || printf 0)
        [[ "$count" -ge "$want" ]] && return
        sleep 0.05
    done
    fail "the rail never reached $want entries, it has $count"
}

# The Flea window is tiled here, so a pane coordinate needs its origin added before a click.
window_box() {
    omarchy-drive windows --json \
        | jq -r '.windows[] | select(.title == "Flea") | "\(.at[0]) \(.at[1]) \(.size[0]) \(.size[1])"'
}

click_row() {
    local index="$1"; shift
    local centre cx cy wx wy ww wh
    centre=$(ipc rowCentre "$index")
    [[ -n "$centre" ]] || fail "row $index has no on-screen centre"
    read -r cx cy <<< "$centre"
    read -r wx wy ww wh < <(window_box)
    # Everything after the index goes straight to omarchy-drive: the button, --double, --mods.
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" "$@" >/dev/null
}

# Steps the menu cursor onto a row by its label rather than by a hardcoded number of Downs, so a
# case survives the operations design's own rows landing between the ones it cares about.
menu_seek() {
    local want="$1" entries target i cursor
    entries=$(ipc contextMenuEntries)
    target=-1
    i=0
    local IFS='|'
    for label in $entries; do
        [[ "$label" == "$want" ]] && { target=$i; break; }
        i=$((i + 1))
    done
    unset IFS
    [[ "$target" -ge 0 ]] || fail "menu_seek: no row labelled $want in $entries"
    for _ in $(seq 1 12); do
        cursor=$(ipc contextMenuCursor)
        [[ "$cursor" == "$target" ]] && return 0
        key -k Down >/dev/null
        settle
    done
    fail "menu_seek: could not reach $want, cursor stalled at $(ipc contextMenuCursor)"
}

# "Kind=PNG image|Size=346 B" becomes "Kind|Size": the labels are the canvas's contract, and the
# values move with the fixture.
fact_labels() {
    printf '%s' "$1" | tr '|' '\n' | cut -d= -f1 | paste -sd'|' -
}

# Walks the cursor to a row by name, from the top, so no case depends on an index the sort could move.
seek_row_named() {
    local want="$1" i
    key g >/dev/null
    for i in $(seq 1 40); do
        [[ "$(ipc rowAt "$(ipc cursor)")" == "$want|"* ]] && return 0
        key j >/dev/null
    done
    fail "could not put the cursor on $want"
}

# A protocol chip carries a label and no tree, so it is reached by name and clicked at its centre.
click_chip() {
    local name="$1" centre cx cy wx wy
    centre=$(ipc networkChipCentre "$name")
    [[ -n "$centre" ]] || fail "the network form has no chip called $name"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box)
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null
}

click_chrome() {
    local glyph="$1" centre cx cy wx wy
    centre=$(ipc chromeButtonCentre "$glyph")
    [[ -n "$centre" ]] || fail "the chrome has no button called $glyph"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box)
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null
}

# Same idiom as click_row, but for a rail row: right click raises the rail's own context menu over
# it, which is the only route to unmount and eject, see ui/Sidebar.qml "openRailMenu".
click_rail_row() {
    local index="$1" button="$2"
    local centre cx cy wx wy ww wh
    centre=$(ipc railRowCentre "$index")
    [[ -n "$centre" ]] || fail "rail row $index has no on-screen centre"
    read -r cx cy <<< "$centre"
    read -r wx wy ww wh < <(window_box)
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" "$button" >/dev/null
}

# ui/Opener.qml spawns flea --terminal and returns, so the stub's line lands after the key has
# been answered: waited for, never slept at, and the path it carries is checked here rather than
# by the caller so a wrong directory reads as a timeout with the log printed.
wait_terminal() {
    local log="$1" want="$2" what="$3" waited
    for waited in $(seq 1 200); do
        grep -q "^TERMINAL $want$" "$log" && return 0
        sleep 0.05
    done
    fail "openterminal: $what started no terminal in $want, log is $(cat "$log")"
}

# The status bar clears a transient after 4000 ms and an eject verdict arrives on the rail's own
# 5000 ms poll, so a sentence that lands after a poll has to be caught as it lands, never slept for.
wait_message() {
    local want="$1" seen=""
    for _attempt in $(seq 1 250); do
        seen=$(ipc lastMessage)
        if [[ "$seen" == "$want" ]]; then
            return 0
        fi
        sleep 0.1
    done
    fail "the status bar never said: $want (the last thing it said was: $seen)"
}

# Reaches a known row index from wherever the cursor is, without assuming a predicted sort order.
goto_row() {
    local target="$1" n
    key g >/dev/null
    for ((n = 0; n < target; n++)); do
        key j >/dev/null
    done
    settle
}

# Counts the pixels in one crop that satisfy a channel expression, see AGENTS.md "Testing".
count_pixels() {
    local png="$1" geometry="$2" expression="$3"
    magick "$png" -crop "$geometry" +repage -fx "$expression ? 1.0 : 0.0" \
        -format "%[fx:int(mean*w*h+0.5)]" info:
}

# Finds a row by name rather than by a predicted sort order, which has been wrong here before.
icon_of() {
    local want="$1" i total
    total=$(ipc total)
    for (( i = 0; i < total; i++ )); do
        [[ "$(ipc rowAt "$i")" == "$want|"* ]] && { ipc rowIcon "$i"; return; }
    done
    fail "no row named $want in a listing of $total"
}

# Same lookup as icon_of, but reads the row's glyph name rather than a thumbnail URL.
glyph_of() {
    local want="$1" i total
    total=$(ipc total)
    for (( i = 0; i < total; i++ )); do
        [[ "$(ipc rowAt "$i")" == "$want|"* ]] && { ipc rowGlyph "$i"; return; }
    done
    fail "no row named $want in a listing of $total"
}

# The icon square starts at the row's own left edge plus rowPaddingX, not the window's: the sidebar
# sits to the left of the row and shifts that edge right by its own width.
icon_crop() {
    local pad rowheight side cy rx
    read -r _body _caption pad rowheight <<< "$(ipc metrics)"
    side=$(( rowheight / 4 ))
    read -r _centre_x cy <<< "$(ipc rowCentre 0)"
    rx=$(ipc rowLeft 0)
    [[ -n "$cy" && -n "$rx" ]] || fail "row 0 reported no on-screen position, so no icon was ever rendered to sample"
    printf '%sx%s+%s+%s' "$side" "$side" "$(( rx + pad + side ))" "$(( cy - side / 2 ))"
}

# Same lookup as icon_of, but the row's own index, for a case that needs to seek to it by keyboard.
row_index_of() {
    local want="$1" i total
    total=$(ipc total)
    for (( i = 0; i < total; i++ )); do
        [[ "$(ipc rowAt "$i")" == "$want|"* ]] && { printf '%s' "$i"; return; }
    done
    fail "no row named $want in a listing of $total"
}

# Seeks to the named row and presses Space, leaving the result for the caller to assert.
open_row() {
    goto_row "$(row_index_of "$1")"
    key -k space >/dev/null
    settle
}

# Same seek, but skips the trailing settle, so the poll below races the clip's play window and not the settle too.
open_row_fast() {
    goto_row "$(row_index_of "$1")"
    key -k space >/dev/null
}

# The player starts asynchronously, so a state read taken at once would catch loading and prove nothing.
wait_preview_state() {
    local want="$1" waited state
    for waited in $(seq 1 $((preview_play_wait_s * 20))); do
        state=$(ipc previewState)
        [[ "$state" == "$want" ]] && return
        sleep 0.05
    done
    fail "preview state never reached $want, it is $state"
}

# The slot must be showing the thumbnail and not the themed icon, and the decode is asynchronous.
wait_thumb_ready() {
    local waited icon status
    for waited in $(seq 1 $((thumb_fill_s * 20))); do
        icon=$(ipc rowIcon 0)
        status=$(ipc rowIconStatus 0)
        [[ "$icon" == "file://"* && "$status" == "$image_ready" ]] && return
        sleep 0.05
    done
    fail "row 0 never drew a ready thumbnail: icon=$icon status=$status"
}

shot() {
    local name="$1"
    mkdir -p "$evidence_dir"
    omarchy-drive shot "$evidence_dir/$name.png" flea >/dev/null
    printf 'SHOT %s\n' "$evidence_dir/$name.png"
}

# Catches removing the cursor clamp from ListView.onContentYChanged in ui/Pane.qml.
case_cursor() {
    [[ -d "$bench_dir" ]] || fail "the 100,000-file fixture is missing at $bench_dir"
    launch "$bench_dir"
    wait_listing 100000
    local wx wy ww wh burst cursor centre first_row
    read -r wx wy ww wh < <(window_box)
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    for burst in $(seq 1 "$scroll_bursts"); do
        omarchy-drive scroll down "$wheel_clicks" >/dev/null
    done
    settle
    cursor=$(ipc cursor)
    centre=$(ipc rowCentre "$cursor")
    first_row=$(ipc rowAt 0)
    printf 'CURSOR cursor=%s centre=%q rowAt0=%q total=%s\n' \
        "$cursor" "$centre" "$first_row" "$(ipc total)"
    shot cursor-after-scroll
    command -v magick >/dev/null || fail "ImageMagick is needed to read the header pixels"
    [[ "$first_row" == "loading" ]] \
        || fail "row 0 is still on screen: the wheel did not scroll, or the clamp snapped the view back"
    (( cursor > 0 )) || fail "the wheel scrolled the viewport away but the cursor stayed on row $cursor"
    [[ -n "$centre" ]] || fail "cursor row $cursor is not in the viewport"
    local row_height band warm lit centre_y header_x header_y
    read -r _centre_x centre_y <<< "$centre"
    row_height=$(ipc metrics | cut -d' ' -f4)
    # The sidebar sits to the header's left and the window chrome sits above it, so the sample band
    # is placed from the header's own origin rather than the window's.
    header_x=$(ipc headerLeft)
    header_y=$(ipc headerTop)
    band="${header_sample_width}x${row_height}+$(( header_x + header_sample_x ))+${header_y}"
    warm=$(count_pixels "$evidence_dir/cursor-after-scroll.png" "$band" "(r - b) > $accent_warmth")
    lit=$(count_pixels "$evidence_dir/cursor-after-scroll.png" "$band" "((r+g+b)/3) > $lit_level")
    printf 'CURSOR header band=%s warm=%s lit=%s rowHeight=%s cursorRowTop=%s\n' \
        "$band" "$warm" "$lit" "$row_height" "$((centre_y - row_height / 2))"
    (( centre_y - row_height / 2 < header_y + row_height )) \
        || fail "the cursor row does not straddle the header, so the overpaint check proves nothing"
    (( warm == 0 )) || fail "the cursor row painted $warm accent pixels over the column header"
    (( lit >= header_lit_floor )) || fail "the header drew only $lit lit pixels, so its labels are gone"
}

# Catches removing the terminal branch from Pane.onFailed in ui/Pane.qml.
case_terminal() {
    local dir="$fixture_root/terminal"
    sandbox_scratch "$dir"
    : > "$dir/only.txt"
    launch "$dir"
    wait_listing 1
    local qs_pid backend_pid
    qs_pid=$(flea_pid)
    backend_pid=$(pgrep -P "$qs_pid" -x flea)
    [[ -n "$backend_pid" ]] || fail "no flea backend child of qs $qs_pid"
    printf 'TERMINAL qs=%s backend=%s before state=%s total=%s row=%q\n' \
        "$qs_pid" "$backend_pid" "$(ipc state)" "$(ipc total)" "$(ipc rowAt 0)"
    kill "$backend_pid"
    omarchy-drive wait ipc -p "$flea_ui" flea state error --timeout 15 >/dev/null \
        || fail "the pane stayed in state '$(ipc state)' after the backend died"
    [[ "$(ipc total)" == "0" ]] || fail "the stale total $(ipc total) survived the backend"
    [[ "$(ipc rowAt 0)" == "loading" ]] || fail "the stale row $(ipc rowAt 0) survived the backend"
    [[ -n "$(ipc stateMessage)" ]] || fail "no recovery sentence after the backend died"
    sleep "$transient_clear_s"
    printf 'TERMINAL after state=%s total=%s row=%q message=%q transient=%q\n' \
        "$(ipc state)" "$(ipc total)" "$(ipc rowAt 0)" "$(ipc stateMessage)" "$(ipc lastMessage)"
    shot terminal-after-death
    [[ "$(ipc state)" == "error" ]] || fail "the error state did not outlive the transient timer"
    [[ -n "$(ipc stateMessage)" ]] || fail "the recovery sentence did not outlive the transient timer"
    [[ "$(ipc total)" == "0" ]] || fail "the total came back after the transient timer"
    assert_window
}

# Catches removing the exit-status branches from ui/Opener.qml or the dispatch from Pane.openCursor.
case_open() {
    local dir="$fixture_root/open"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir" "$dir/bin"
    printf 'abc' > "$dir/target.txt"
    ln -s "$dir/target.txt" "$dir/linkfile"
    ln -s "$dir/subdir" "$dir/linkdir"
    ln -s "$dir/nowhere" "$dir/broken"
    local opened="$dir/opened.log"
    : > "$opened"
    # Only the open subcommand is intercepted, so stubbing the opener leaves the gio mount calls
    # ui/NetworkMounts.qml makes on every launch answering from the real gio. That name is the mount
    # tool's own and is spelled by hand here; the stub's name is derived from src/open.rs instead.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$dir/bin/$open_handoff"
    chmod +x "$dir/bin/$open_handoff"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export PATH="$saved_path"
    # Measured row order: bin, subdir, broken, linkdir, linkfile, opened.log, target.txt.
    wait_listing 7

    # Bare l enters a real directory and stays silent when its empty listing has no row.
    seek_row_named subdir
    key l >/dev/null
    wait_path "$dir/subdir"
    omarchy-drive wait ipc -p "$flea_ui" flea state empty --timeout 10 >/dev/null \
        || fail "open: subdir never reached its empty listing, state is $(ipc state)"
    [[ ! -s "$opened" ]] || fail "l on a directory handed $(cat "$opened") to $open_handoff open"
    [[ -z "$(ipc lastMessage)" ]] || fail "open: entering the empty subdir said $(ipc lastMessage)"
    key l >/dev/null
    settle
    [[ -z "$(ipc lastMessage)" ]] || fail "open: l on an empty directory said $(ipc lastMessage)"
    key q >/dev/null
    settle
    [[ "$(ipc lastMessage)" == "Press / to filter this listing by name." ]] \
        || fail "open: unbound q said $(ipc lastMessage), so the empty-row l check has no negative control"

    # Bare l enters a symlink to a directory without handing it to the opener.
    key -k Backspace >/dev/null
    wait_path "$dir"
    seek_row_named linkdir
    key l >/dev/null
    wait_path "$dir/linkdir"
    [[ ! -s "$opened" ]] || fail "l on a symlink directory handed $(cat "$opened") to $open_handoff open"

    # Return on a symlink to a directory keeps its existing navigation and opener coverage.
    key -k Backspace >/dev/null
    wait_path "$dir"
    seek_row_named linkdir
    key -k Return >/dev/null
    wait_path "$dir/linkdir"
    [[ ! -s "$opened" ]] || fail "Enter on a symlink directory handed $(cat "$opened") to $open_handoff open"

    # Return on a symlink to a file still resolves and opens its target.
    key -k Backspace >/dev/null
    wait_path "$dir"
    seek_row_named linkfile
    settle
    [[ "$(ipc rowAt "$(ipc cursor)")" == "linkfile|"* ]] || fail "the cursor is on $(ipc rowAt "$(ipc cursor)"), not linkfile"
    key -k Return >/dev/null
    local waited
    for waited in $(seq 1 100); do
        grep -q "^OPENED $dir/target.txt$" "$opened" && break
        sleep 0.05
    done
    printf 'OPEN log=%q path=%q message=%q\n' "$(cat "$opened")" "$(ipc path)" "$(ipc lastMessage)"
    grep -q "^OPENED $dir/target.txt$" "$opened" || fail "Enter on a symlink did not open its target"
    [[ "$(ipc path)" == "$dir" ]] || fail "Enter on a file left the directory for $(ipc path)"

    # A broken symlink is one sentence and nothing else.
    key g >/dev/null
    key -k Down >/dev/null
    key -k Down >/dev/null
    settle
    [[ "$(ipc rowAt "$(ipc cursor)")" == "broken|"* ]] || fail "the cursor is on $(ipc rowAt "$(ipc cursor)"), not broken"
    key -k Return >/dev/null
    settle
    printf 'OPEN broken message=%q path=%q log=%q\n' "$(ipc lastMessage)" "$(ipc path)" "$(cat "$opened")"
    shot open-broken
    [[ -n "$(ipc lastMessage)" ]] || fail "Enter on a broken symlink said nothing"
    [[ "$(ipc path)" == "$dir" ]] || fail "Enter on a broken symlink moved to $(ipc path)"
    [[ "$(grep -c OPENED "$opened")" == "1" ]] || fail "a broken symlink was handed to $open_handoff open"
    [[ "$(ipc total)" == "7" ]] || fail "the listing did not survive Enter on a broken symlink"
}

# PR 34's terminal route. Nothing else in this suite reaches it: before this case, openTerminal,
# terminalRequested, terminalFailed and --terminal appeared nowhere in this file, so the topbar
# button, its wiring in ui/shell.qml and the chord's interception in ui/js/Focus.js were all
# deletable with the display suite still green.
case_openterminal() {
    local dir="$fixture_root/openterminal"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    printf 'abc' > "$dir/target.txt"
    local ran="$dir/ran.log" opened="$dir/opened.log" real_bin="$flea_bin"
    : > "$ran"
    : > "$opened"
    # FLEA_BIN is the backend's binary as well as the opener's, so only --terminal is intercepted
    # and every other mode execs the real one: a stub that swallowed --backend would leave the
    # window with no listing to press a key in. The sleep is what makes the single-flight guard and
    # the two-paths-at-once check observable at all.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = --terminal ] || exec %q "$@"\n' "$real_bin"
      printf 'sleep 1\n'
      printf 'printf "TERMINAL %%s\\n" "$2" >> %q\n' "$ran"
      printf 'exit 0\n'
    } > "$dir/bin/flea"
    chmod +x "$dir/bin/flea"
    # Only the open subcommand is intercepted, so stubbing the opener leaves the gio mount calls
    # ui/NetworkMounts.qml makes on every launch answering from the real gio. That name is the mount
    # tool's own and is spelled by hand here; the stub's name is derived from src/open.rs instead.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$dir/bin/$open_handoff"
    chmod +x "$dir/bin/$open_handoff"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    flea_bin="$dir/bin/flea"
    launch "$dir"
    export PATH="$saved_path"
    flea_bin="$real_bin"
    # bin, opened.log, ran.log, target.txt.
    wait_listing 4

    # The pointer half: the topbar button, by its glyph, which is the same action the chord raises.
    click_chrome terminal
    wait_terminal "$ran" "$dir" "the topbar button"
    printf 'OPENTERMINAL button log=%q\n' "$(cat "$ran")"

    # The keyboard half, from the list.
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    wait_terminal "$ran" "$dir" "ctrl+t in the list"

    # And from the rail, which owns its own keys and would otherwise swallow the chord.
    : > "$ran"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "openterminal: tab did not reach the rail"
    hotkey --global ctrl t flea >/dev/null
    wait_terminal "$ran" "$dir" "ctrl+t on the rail"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "openterminal: escape did not return to the list"

    # A second request while the first terminal is still starting is dropped, and the third, once it
    # has exited, is not: without the second half this check passes for a key that does nothing.
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    settle
    hotkey --global ctrl t flea >/dev/null
    wait_terminal "$ran" "$dir" "the single-flight guard"
    sleep 1
    [[ "$(grep -c . "$ran")" == "1" ]] || fail "openterminal: two terminals were started, log is $(cat "$ran")"
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    wait_terminal "$ran" "$dir" "a request after the child exited"

    # Both at once: the terminal child is still sleeping when Enter opens a file, so a shared path or
    # a shared Process would show up as one of the two logs carrying the other's argument.
    : > "$ran"
    : > "$opened"
    hotkey --global ctrl t flea >/dev/null
    seek_row_named target.txt
    key -k Return >/dev/null
    local waited
    for waited in $(seq 1 100); do
        grep -q "^OPENED $dir/target.txt$" "$opened" && break
        sleep 0.05
    done
    grep -q "^OPENED $dir/target.txt$" "$opened" \
        || fail "openterminal: a file open during a terminal launch never reached $open_handoff, log is $(cat "$opened")"
    wait_terminal "$ran" "$dir" "the terminal launched beside a file open"
    printf 'OPENTERMINAL crossed terminal=%q opened=%q\n' "$(cat "$ran")" "$(cat "$opened")"
    [[ "$(grep -c . "$opened")" == "1" ]] || fail "openterminal: the opener ran twice, log is $(cat "$opened")"

    # A nonzero exit reaches the status line as one sentence. The stub is rewritten rather than
    # relaunched, because a fresh exec reads the file again.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = --terminal ] || exec %q "$@"\n' "$real_bin"
      printf 'exit 2\n'
    } > "$dir/bin/flea"
    chmod +x "$dir/bin/flea"
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    wait_message "That directory could not be opened in a terminal; nothing on this system took it."
    shot openterminal-failed
    [[ ! -s "$ran" ]] || fail "openterminal: the failing stub still logged $(cat "$ran")"

    printf 'OPENTERMINAL button=ok list=ok rail=ok single-flight=ok crossed=ok failure=ok\n'
    kill_flea
}

# The operator's defect of 2026-09-02, in their own words: "when clicking a single click opens the
# file, that should not happen and should behave like mac os, double clicking opens things". This
# drives keys.toml's [[pointer]] table through the real TapHandler, which is the half tests/js/tap.js
# cannot see: that suite checks what ui/js/Tap.js decides, never that a delegate hands it the tap
# count and the modifiers a real click carried.
case_click() {
    local dir="$fixture_root/click"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir" "$dir/bin"
    printf 'alpha\n' > "$dir/alpha.txt"
    printf 'beta\n' > "$dir/beta.txt"
    printf 'gamma\n' > "$dir/gamma.txt"
    local opened="$dir/opened.log"
    : > "$opened"
    # Only the open subcommand is intercepted, so stubbing the opener leaves the gio mount calls
    # ui/NetworkMounts.qml makes on every launch answering from the real gio. That name is the mount
    # tool's own and is spelled by hand here; the stub's name is derived from src/open.rs instead.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$dir/bin/$open_handoff"
    chmod +x "$dir/bin/$open_handoff"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export PATH="$saved_path"
    # Measured row order: bin, subdir, alpha.txt, beta.txt, gamma.txt, opened.log.
    wait_listing 6
    local alpha=2

    # The list view. One tap selects the row and opens nothing at all.
    click_row "$alpha" left
    settle
    printf 'CLICK list single cursor=%s path=%q opened=%q\n' "$(ipc cursor)" "$(ipc path)" "$(cat "$opened")"
    shot click-list-single
    [[ "$(ipc cursor)" == "$alpha" ]] || fail "click: one tap did not move the cursor, it is $(ipc cursor)"
    [[ ! -s "$opened" ]] || fail "click: one tap opened $(cat "$opened")"
    [[ "$(ipc path)" == "$dir" ]] || fail "click: one tap left the directory for $(ipc path)"

    # The second tap is what opens, which is also the negative control for the check above.
    click_row "$alpha" left --double
    for _attempt in $(seq 1 100); do
        grep -q "^OPENED $dir/alpha.txt$" "$opened" && break
        sleep 0.05
    done
    printf 'CLICK list double opened=%q\n' "$(cat "$opened")"
    shot click-list-double
    grep -q "^OPENED $dir/alpha.txt$" "$opened" || fail "click: a double click did not open alpha.txt"
    [[ "$(grep -c OPENED "$opened")" == "1" ]] || fail "click: a double click opened it $(grep -c OPENED "$opened") times"

    # Ctrl and shift are Finder's selection modifiers, and neither ever opens.
    : > "$opened"
    click_row 4 left --mods ctrl
    settle
    [[ "$(ipc selectedIndices)" == "4" ]] || fail "click: ctrl+click selected '$(ipc selectedIndices)', not row 4"
    click_row 2 left --mods shift
    settle
    printf 'CLICK modifiers indices=%s opened=%q\n' "$(ipc selectedIndices)" "$(cat "$opened")"
    shot click-modifiers
    [[ "$(ipc selectedIndices)" == "2,3,4" ]] || fail "click: shift+click selected '$(ipc selectedIndices)', not the run 2,3,4"
    [[ ! -s "$opened" ]] || fail "click: a modified click opened $(cat "$opened")"

    # A modifier never opens, whatever the tap count: this is the case a tapCount check alone gets
    # wrong, because the second tap of a ctrl-held double click still carries tapCount 2.
    click_row 2 left --mods ctrl --double
    settle
    [[ ! -s "$opened" ]] || fail "click: a ctrl-held double click opened $(cat "$opened")"

    # A plain click replaces the selection, so the next shift+click extends from the row the cursor
    # is visibly on and a write operation cannot reach rows the user thinks they dropped.
    click_row 5 left
    settle
    [[ "$(ipc selectionCount)" == "0" ]] \
        || fail "click: a plain click left $(ipc selectionCount) rows selected, so the selection is stale"

    # Right click keeps its own contract in every view: the cursor moves and the menu opens.
    click_row 3 right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "click: right click opened no menu"
    [[ "$(ipc cursor)" == "3" ]] || fail "click: right click did not set the cursor, it is $(ipc cursor)"
    key -k Escape >/dev/null
    settle

    # A directory row navigates on the second tap and not on the first.
    click_row 1 left
    settle
    [[ "$(ipc path)" == "$dir" ]] || fail "click: one tap on a directory navigated to $(ipc path)"
    click_row 1 left --double
    wait_path "$dir/subdir"
    key -k BackSpace >/dev/null
    wait_path "$dir"
    wait_listing 6

    # The grid, a different delegate in a different file carrying the same contract.
    click_chrome grid
    settle
    [[ "$(ipc viewMode)" == "grid" ]] || fail "click: the chrome did not switch to the grid"
    : > "$opened"
    click_row "$alpha" left
    settle
    printf 'CLICK grid single cursor=%s opened=%q\n' "$(ipc cursor)" "$(cat "$opened")"
    shot click-grid-single
    [[ "$(ipc cursor)" == "$alpha" ]] || fail "click: one tap in the grid did not move the cursor"
    [[ ! -s "$opened" ]] || fail "click: one tap in the grid opened $(cat "$opened")"
    click_row "$alpha" left --double
    for _attempt in $(seq 1 100); do
        grep -q "^OPENED $dir/alpha.txt$" "$opened" && break
        sleep 0.05
    done
    shot click-grid-double
    grep -q "^OPENED $dir/alpha.txt$" "$opened" || fail "click: a double click in the grid did not open alpha.txt"

    # The columns view's middle column, the one carrying the pane's own listing. Its two neighbours
    # are peeks with no cursor and no IPC coordinate, so ui/js/Tap.js tappedColumn is where the
    # verbs they answer are checked, in tests/js/tap.js.
    click_chrome columns
    settle
    [[ "$(ipc viewMode)" == "columns" ]] || fail "click: the chrome did not switch to the columns"
    : > "$opened"
    click_row 3 left
    settle
    printf 'CLICK columns single cursor=%s opened=%q\n' "$(ipc cursor)" "$(cat "$opened")"
    shot click-columns-single
    [[ "$(ipc cursor)" == "3" ]] || fail "click: one tap in the columns did not move the cursor"
    [[ ! -s "$opened" ]] || fail "click: one tap in the columns opened $(cat "$opened")"
    click_row 3 left --double
    for _attempt in $(seq 1 100); do
        grep -q "^OPENED $dir/beta.txt$" "$opened" && break
        sleep 0.05
    done
    printf 'CLICK columns double opened=%q\n' "$(cat "$opened")"
    shot click-columns-double
    grep -q "^OPENED $dir/beta.txt$" "$opened" || fail "click: a double click in the columns did not open beta.txt"
    click_chrome list
    settle
}

# Catches narrowing the delegate TapHandler back to Qt.LeftButton in ui/Pane.qml.
case_menu() {
    local dir="$fixture_root/menu"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/c.txt"
    : > "$dir/plain.txt"
    launch "$dir"
    wait_listing 5
    local row_height row_padding_x centre cx cy wx wy ww wh row_left beneath_y metrics
    metrics=$(ipc metrics) || fail "menu: metrics unavailable"
    read -r _body _caption row_padding_x row_height <<< "$metrics"
    click_row 1 right
    settle
    printf 'MENU on-file visible=%s cursor=%s\n' "$(ipc contextMenuVisible)" "$(ipc cursor)"
    shot menu-open
    printf 'MENU_OCR_BEGIN\n'
    omarchy-drive ocr flea || true
    printf 'MENU_OCR_END\n'
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "right click did not open the context menu"
    [[ "$(ipc cursor)" == "1" ]] || fail "right click did not set the cursor, it is $(ipc cursor)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "Escape did not close the context menu"
    key k >/dev/null
    settle
    [[ "$(ipc cursor)" == "0" ]] || fail "the list did not take the keyboard back after the menu closed"
    centre=$(ipc rowCentre 0)
    read -r cx cy <<< "$centre"
    read -r wx wy ww wh < <(window_box)

    # Clicking outside still closes through the backdrop after menu pointer ownership changes.
    click_row 0 right
    settle
    centre=$(ipc rowCentre 1)
    read -r _beneath_cx beneath_y <<< "$centre"
    row_left=$(ipc rowLeft 1)
    omarchy-drive click "$((wx + row_left + row_padding_x))" "$((wy + beneath_y))" left >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "an outside click did not close the context menu"

    # Rename lies over visible row 3 here; its click belongs only to the menu, never that row below.
    click_row 0 right
    settle
    centre=$(ipc rowCentre 3)
    read -r _beneath_cx beneath_y <<< "$centre"
    [[ -n "$beneath_y" ]] || fail "menu: row 3 is not visible beneath Rename"
    omarchy-drive click "$((wx + cx + row_height))" "$((wy + beneath_y))" left >/dev/null
    settle
    printf 'MENU rename-over-row cursor=%s renaming=%s live=%s text=%q beneath=%s\n' \
        "$(ipc cursor)" "$(ipc renamingIndex)" "$(ipc renameEditorLive)" \
        "$(ipc renameEditorText)" "$(ipc rowAt 3 | cut -d'|' -f1)"
    shot menu-rename-over-row
    [[ "$(ipc cursor)" == "0" ]] || fail "Rename passed its click to row $(ipc cursor)"
    [[ "$(ipc renamingIndex)" == "0" ]] || fail "Rename retargeted or committed row 0, renamingIndex is $(ipc renamingIndex)"
    [[ "$(ipc renameEditorLive)" == "true" ]] || fail "Rename left no live editor on row 0"
    [[ "$(ipc renameEditorText)" == "subdir" ]] || fail "Rename opened over '$(ipc renameEditorText)', not subdir"
    key -k Escape >/dev/null
    settle

    click_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "right click on the directory opened no menu"
    omarchy-drive click "$((cx + wx + row_height))" "$((cy + wy + row_height / 2))" left >/dev/null
    settle
    printf 'MENU chosen path=%q visible=%s message=%q\n' \
        "$(ipc path)" "$(ipc contextMenuVisible)" "$(ipc lastMessage)"
    shot menu-chosen
    [[ "$(ipc path)" == "$dir/subdir" ]] || fail "the Open action did not open the directory"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "the menu stayed open after its action ran"
}

# Task 18: dotfiles off by default, the "." key and the context menu entry both flip one state
# and re-list, which is also what clears the cursor back to row 0.
case_hidden() {
    local dir="$fixture_root/hidden"
    sandbox_scratch "$dir"
    : > "$dir/.dotfile"
    : > "$dir/visible.txt"
    launch "$dir"
    wait_listing 1
    [[ "$(ipc showHidden)" == "false" ]] || fail "hidden: showHidden did not default to false"
    [[ "$(ipc rowAt 0)" == "visible.txt|"* ]] || fail "hidden: the dotfile leaked into the default listing, row 0 is $(ipc rowAt 0)"

    click_row 0 right
    settle
    # This case cares about one row, so it looks for that row rather than matching the whole menu:
    # which other entries are present depends on the box's live tailnet and on how many of the
    # operations design's own rows have shipped, and neither is this case's concern.
    [[ "$(ipc contextMenuEntries)" == *"Show hidden files"* ]] \
        || fail "hidden: the menu did not offer Show hidden files, got $(ipc contextMenuEntries)"
    shot hidden-menu-off
    key -k Escape >/dev/null
    settle

    key . >/dev/null
    wait_listing 2
    [[ "$(ipc showHidden)" == "true" ]] || fail "hidden: . did not flip showHidden on"
    [[ "$(ipc cursor)" == "0" ]] || fail "hidden: the toggle did not reset the cursor, it is $(ipc cursor)"
    shot hidden-shown

    click_row 0 right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Hide hidden files"* ]] \
        || fail "hidden: the menu label did not flip to Hide hidden files, got $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    key . >/dev/null
    wait_listing 1
    [[ "$(ipc showHidden)" == "false" ]] || fail "hidden: a second . did not flip back off"
    [[ "$(ipc rowAt 0)" == "visible.txt|"* ]] || fail "hidden: toggling back off left the dotfile visible, row 0 is $(ipc rowAt 0)"

    printf 'HIDDEN default=ok toggle-on=ok menu-label=ok toggle-off=ok\n'
    kill_flea
}

# Toggle, extend, select-all, clear, and the invariant that matters most: an index into a
# directory that no longer exists means nothing, so a re-list must never carry a stale selection.
case_selection() {
    local dir="$fixture_root/selection"
    sandbox_scratch "$dir"
    mkdir -p "$dir/sub"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/c.txt"
    : > "$dir/d.txt"
    launch "$dir"
    # Directories sort first: sub, a.txt, b.txt, c.txt, d.txt.
    wait_listing 5
    [[ "$(ipc selectionCount)" == "0" ]] || fail "selection: a fresh listing already has a selection"

    # Negative control: plain cursor movement must never touch the selection.
    key j >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "0" ]] || fail "selection: plain j moved the count"

    goto_row 1
    key v >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "1" ]] || fail "selection: v did not toggle row 1 on"
    [[ "$(ipc selectedIndices)" == "1" ]] || fail "selection: selectedIndices is $(ipc selectedIndices), not 1"
    key v >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "0" ]] || fail "selection: a second x did not toggle row 1 back off"

    # x re-marks the anchor, then J J extends it two rows down: 1, 2, 3.
    key v >/dev/null
    key J >/dev/null
    key J >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "3" ]] || fail "selection: x then J J left $(ipc selectionCount), not 3"
    [[ "$(ipc selectedIndices)" == "1,2,3" ]] || fail "selection: extend covered $(ipc selectedIndices), not 1,2,3"
    shot selection-extend

    hotkey --global ctrl a flea >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "5" ]] || fail "selection: ctrl+a selected $(ipc selectionCount), not every row"

    key -k Escape >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "0" ]] || fail "selection: escape did not clear the selection"

    goto_row 1
    key v >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "1" ]] || fail "selection: setup toggle before the stale check did not take"
    key -k Backspace >/dev/null
    wait_path "$fixture_root"
    [[ "$(ipc selectionCount)" == "0" ]] || fail "selection: a new listing kept a stale selection"

    printf 'SELECTION toggle=ok extend=ok all=ok clear=ok stale=ok\n'
    kill_flea
}

# Mirrors the two env vars src/gui.rs sets from a resolved --select; tests/modes.sh covers the resolution itself.
case_select() {
    local dir="$fixture_root/select"
    sandbox_scratch "$dir"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/c.txt"

    kill_flea
    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    : > "$flea_log"
    FLEA_PATH="$dir" FLEA_SELECT="$dir/b.txt" FLEA_BIN="$flea_bin" \
        setsid nohup qs -p "$flea_ui" >"$flea_log" 2>&1 </dev/null &
    omarchy-drive wait window flea --timeout 15 >/dev/null
    omarchy-drive focus flea >/dev/null
    assert_window
    wait_listing 3
    [[ "$(ipc path)" == "$dir" ]] || fail "select: opened $(ipc path), not $dir"
    local want_index
    want_index=$(row_index_of "b.txt")
    [[ "$(ipc cursor)" == "$want_index" ]] || fail "select: cursor is $(ipc cursor), not row $want_index"
    [[ "$(ipc selectionCount)" == "1" ]] || fail "select: selectionCount is $(ipc selectionCount), not 1"
    [[ "$(ipc selectedIndices)" == "$want_index" ]] || fail "select: selectedIndices is $(ipc selectedIndices), not $want_index"

    # A missing target still opens its directory, with nothing selected.
    kill_flea
    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    : > "$flea_log"
    FLEA_PATH="$dir" FLEA_SELECT="$dir/does-not-exist.txt" FLEA_BIN="$flea_bin" \
        setsid nohup qs -p "$flea_ui" >"$flea_log" 2>&1 </dev/null &
    omarchy-drive wait window flea --timeout 15 >/dev/null
    omarchy-drive focus flea >/dev/null
    assert_window
    wait_listing 3
    [[ "$(ipc path)" == "$dir" ]] || fail "select: a missing target opened $(ipc path), not $dir"
    [[ "$(ipc selectionCount)" == "0" ]] || fail "select: a missing target still selected $(ipc selectionCount)"

    printf 'SELECT reveal=ok missing=ok\n'
    kill_flea
}

# Catches restoring the directory accent branch of Row.nameColor in ui/Row.qml.
case_colour() {
    local dir="$fixture_root/colour"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir"
    : > "$dir/plain.txt"
    launch "$dir"
    wait_listing 2
    local foreground accent dir_colour file_colour
    read -r _background _surface foreground _muted accent _rest <<< "$(ipc palette)"
    dir_colour=$(ipc rowNameColor 0)
    file_colour=$(ipc rowNameColor 1)
    printf 'COLOUR dir=%s file=%s foreground=%s accent=%s cursor=%s\n' \
        "$dir_colour" "$file_colour" "$foreground" "$accent" "$(ipc cursor)"
    shot colour-directory
    [[ "$dir_colour" != "$accent" ]] || fail "the directory name is still the cursor accent $accent"
    [[ "$dir_colour" == "$foreground" ]] || fail "the directory name is $dir_colour, not foreground $foreground"
    [[ "$file_colour" == "$foreground" ]] || fail "the file name is $file_colour, not foreground $foreground"
}

# Catches deleting the lifted branch from Row.nameColor or Row.cellColor in ui/Row.qml.
case_lifted() {
    local dir="$fixture_root/lifted"
    sandbox_scratch "$dir"
    : > "$dir/a-plain.txt"
    ln -s "$dir/a-plain.txt" "$dir/z-link"
    launch "$dir"
    wait_listing 2

    # Found by name, not by a predicted sort position, see goto_row and icon_of for the same rule.
    local foreground symlink_colour i link_row=-1 plain_row=-1
    read -r _background _surface foreground _muted _accent _error symlink_colour _executable <<< "$(ipc palette)"
    for ((i = 0; i < 2; i++)); do
        case "$(ipc rowAt "$i")" in
            z-link\|*) link_row=$i ;;
            a-plain.txt\|*) plain_row=$i ;;
        esac
    done
    [[ "$link_row" -ge 0 && "$plain_row" -ge 0 ]] || fail "the lifted fixture did not list both rows"

    goto_row "$plain_row"
    [[ "$(ipc cursor)" == "$plain_row" ]] || fail "the cursor did not land on the plain row"
    local off_name off_cell
    off_name=$(ipc rowNameColor "$link_row")
    # The symlink row is the one under test throughout; here the cursor sits elsewhere, so it reads unlifted.
    off_cell=$(ipc rowCellColor "$link_row")
    printf 'LIFTED off name=%s symlink=%s foreground=%s cell=%s\n' \
        "$off_name" "$symlink_colour" "$foreground" "$off_cell"
    [[ "$off_name" == "$symlink_colour" ]] \
        || fail "the symlink off the cursor is $off_name, not the symlink colour $symlink_colour"
    [[ "$off_name" != "$foreground" ]] || fail "the symlink off the cursor is already foreground"
    [[ "$off_cell" != "$foreground" ]] || fail "the symlink row's cell is foreground before it is ever lifted"

    goto_row "$link_row"
    [[ "$(ipc cursor)" == "$link_row" ]] || fail "the cursor did not land on the symlink row"
    local on_name on_cell
    on_name=$(ipc rowNameColor "$link_row")
    on_cell=$(ipc rowCellColor "$link_row")
    printf 'LIFTED on name=%s cell=%s foreground=%s\n' "$on_name" "$on_cell" "$foreground"
    shot lifted
    [[ "$on_name" == "$foreground" ]] || fail "the cursor's symlink name is $on_name, not foreground $foreground"
    [[ "$on_cell" == "$foreground" ]] || fail "the cursor's cell colour is $on_cell, not foreground $foreground"
}

# The one place the theme's own colours are read, so the test cannot copy the product's parser.
theme_key() {
    grep -E "^\s*$1\s*=" "$HOME/.local/state/omarchy/current/theme/colors.toml" \
        | head -1 | grep -oE '#[0-9A-Fa-f]{6}'
}

# The header and the rows read the same width tokens, so a drift shows up as a misaligned column.
case_columns() {
    local dir="$fixture_root/columns"
    sandbox_scratch "$dir"
    mkdir -p "$dir/inner"
    printf 'body\n' > "$dir/inner/deep.txt"
    # One row per preview state this case asserts, made by real tools so the facts are real facts.
    magick -size 640x480 xc:navy "$dir/shot.png"
    printf 'one\ntwo\nthree\n' > "$dir/notes.txt"
    ln -s inner "$dir/link"
    # The canvas's own Unsupported example: a name nothing identifies, which is not text however far
    # the backend's icon ladder falls.
    printf 'nothing readable\n' > "$dir/core.dump"
    # An archive too, because the archive tile is the one preview state with no live coverage at all.
    mkdir -p "$dir/inner/pack"
    printf 'packed\n' > "$dir/inner/pack/a.txt"
    printf 'packed\n' > "$dir/inner/pack/b.txt"
    bsdtar -a -c -f "$dir/backup.tar.zst" -C "$dir/inner/pack" . 2>/dev/null
    rm -rf "$dir/inner/pack"
    # The pdf logic is covered in tests/js/facts.js, but PdfDocument, the page render, the page count
    # and the error state had never met a real file anywhere in this suite. Each page is built in its
    # own parentheses: without them magick applies one -draw to the whole list and both pages come out
    # identical, measured on the built file before this was written.
    magick \( -size 400x560 xc:white -fill black -draw "rectangle 40,40 120,80" \) \
           \( -size 400x560 xc:white -fill black -draw "rectangle 40,40 360,520" \) \
           "$dir/manual.pdf"
    [[ -s "$dir/manual.pdf" ]] || fail "magick produced no manual.pdf"
    # A real header over a truncated body, so the document is unreadable rather than absent; poppler
    # calls the same 200 bytes "Couldn't read xref table".
    head -c 200 "$dir/manual.pdf" > "$dir/broken.pdf"

    launch "$dir"
    wait_listing 8
    click_chrome columns
    settle
    [[ "$(ipc viewMode)" == "columns" ]] || fail "columns: the chrome button did not switch the view"

    # Like the grid, a column view has no columns to head, so the strip collapses.
    (( $(ipc headerTop) == $(ipc chromeHeight) )) || fail "columns: the column header did not collapse"

    # Row 0 is the directory "inner", so the third pane is its contents and not a preview.
    [[ "$(ipc rowAt 0)" == "inner|dir|"* ]] || fail "columns: row 0 is $(ipc rowAt 0), not the directory"

    # The archive tile: an exact count, an unpacked total, and the entries the frame could name.
    seek_row_named "backup.tar.zst"
    settle
    [[ "$(ipc previewColumnState)" == "archive" ]] \
        || fail "columns: an archive previews as $(ipc previewColumnState)"
    archive_facts=$(ipc previewFacts)
    [[ "$archive_facts" == *"Entries=3"* ]] \
        || fail "columns: the archive states $archive_facts, not the three members it holds"
    [[ "$archive_facts" == *"Kind="*"Packed="*"Unpacked="* ]] \
        || fail "columns: the archive tile's labels are wrong, got $archive_facts"

    seek_row_named "shot.png"
    settle
    [[ "$(ipc previewColumnState)" == "image" ]] \
        || fail "columns: a png previews as $(ipc previewColumnState)"
    # The canvas's Image tile, label for label and in its own order.
    # The label sequence is the canvas's contract; the values are checked one at a time, because a
    # glob pattern carrying the multiplication sign does not match under this suite's own locale.
    local facts
    facts=$(ipc previewFacts)
    [[ "$(fact_labels "$facts")" == "Kind|Size|Pixels|Modified" ]] \
        || fail "columns: the image states $(fact_labels "$facts"), not the canvas's own four rows"
    [[ "$facts" == "Kind=PNG image|"* ]] || fail "columns: the image kind is wrong in $facts"
    [[ "$facts" == *"640"*"480"* ]] || fail "columns: the image pixels are wrong in $facts"
    printf 'COLUMNS image=%s\n' "$facts"
    shot columns-image

    seek_row_named "notes.txt"
    settle
    [[ "$(ipc previewColumnState)" == "text" ]] || fail "columns: notes.txt previews as $(ipc previewColumnState)"
    facts=$(ipc previewFacts)
    [[ "$(fact_labels "$facts")" == "Kind|Size|Lines|Modified" ]] \
        || fail "columns: the text states $(fact_labels "$facts")"
    [[ "$facts" == *"Lines=3"* ]] || fail "columns: the line count is wrong in $facts"
    printf 'COLUMNS text=%s\n' "$facts"

    # The near-white count is what proves a page rendered: the page is the only light surface in the
    # app, and the broken document gives the same tile with no page in it as the control.
    local whole="100%x100%+0+0" near_white="((r+g+b)/3) > 0.9"
    local blank_white page1_white page2_white centre cx cy wx wy i
    seek_row_named "broken.pdf"
    settle
    # A PdfDocument opens asynchronously, so a state read taken at once catches Loading and proves
    # nothing; both polls below are the wait and the assertion in one.
    for i in $(seq 1 25); do
        [[ "$(ipc previewColumnState)" == "error" ]] && break
        sleep 0.2
    done
    [[ "$(ipc previewColumnState)" == "error" ]] \
        || fail "columns: a truncated pdf previews as $(ipc previewColumnState), not error"
    shot columns-pdf-broken
    blank_white=$(count_pixels "$evidence_dir/columns-pdf-broken.png" "$whole" "$near_white")

    seek_row_named "manual.pdf"
    settle
    for i in $(seq 1 25); do
        (( $(ipc columnPdfPages) == 2 )) && break
        sleep 0.2
    done
    [[ "$(ipc previewColumnState)" == "pdf" ]] \
        || fail "columns: manual.pdf previews as $(ipc previewColumnState), not pdf"
    [[ "$(ipc columnPdfLoaded)" == "true" ]] \
        || fail "columns: no PdfDocument was built for manual.pdf"
    (( $(ipc columnPdfPages) == 2 )) \
        || fail "columns: manual.pdf reports $(ipc columnPdfPages) pages, not the two it has"
    (( $(ipc columnPdfPage) == 0 )) \
        || fail "columns: a freshly opened document sits on page $(ipc columnPdfPage), not the first"
    facts=$(ipc previewFacts)
    [[ "$facts" == *"Pages=2"* ]] || fail "columns: the pdf tile states $facts, not two pages"
    printf 'COLUMNS pdf=%s\n' "$facts"
    shot columns-pdf-page1
    page1_white=$(count_pixels "$evidence_dir/columns-pdf-page1.png" "$whole" "$near_white")
    # A factor rather than a tuned threshold: page one is 99 per cent white across the whole frame
    # and the control carries only chrome, so the gap is a magnitude and not a margin.
    (( page1_white > 2 * blank_white )) \
        || fail "columns: the pdf frame drew $page1_white near-white pixels against $blank_white with no document, so no page rendered"

    centre=$(ipc columnChevronCentre right)
    [[ -n "$centre" ]] || fail "columns: the pdf pager's right chevron has no on-screen centre"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box)
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null
    settle
    (( $(ipc columnPdfPage) == 1 )) \
        || fail "columns: the chevron left the frame on page $(ipc columnPdfPage), not the second"
    # PdfPageImage renders asynchronously, so the page number flips before the pixels do; the poll is
    # the wait and the assertion at once. Page two is 34 per cent dark against page one's 0.7, both
    # measured on the built file, so an unchanged count means the render never followed the turn.
    page2_white=$page1_white
    for i in $(seq 1 20); do
        shot columns-pdf-page2
        page2_white=$(count_pixels "$evidence_dir/columns-pdf-page2.png" "$whole" "$near_white")
        (( page2_white != page1_white )) && break
        sleep 0.2
    done
    (( page2_white != page1_white )) \
        || fail "columns: page two renders the same $page1_white near-white pixels as page one, so the turn changed nothing"
    printf 'COLUMNS pdf pixels blank=%s page1=%s page2=%s\n' "$blank_white" "$page1_white" "$page2_white"

    seek_row_named "link"
    settle
    [[ "$(ipc previewColumnState)" == "symlink" ]] || fail "columns: link previews as $(ipc previewColumnState)"
    facts=$(ipc previewFacts)
    [[ "$(fact_labels "$facts")" == "Kind|Target|Points at|Mode" ]] \
        || fail "columns: the symlink states $(fact_labels "$facts")"
    [[ "$facts" == "Kind=Symbolic link|Target=inner|Points at=Folder|"* ]] \
        || fail "columns: the symlink facts are $facts"
    printf 'COLUMNS symlink=%s\n' "$facts"

    seek_row_named "core.dump"
    settle
    facts=$(ipc previewFacts)
    [[ "$(ipc previewColumnState)" == "unsupported" ]] \
        || fail "columns: core.dump previews as $(ipc previewColumnState), not unsupported"
    [[ "$(fact_labels "$facts")" == "Kind|Size|Modified|Mode" ]] \
        || fail "columns: an unpreviewable row states $(fact_labels "$facts")"
    printf 'COLUMNS unsupported=%s state=%s\n' "$facts" "$(ipc previewColumnState)"
    shot columns-unsupported

    click_chrome list
    settle
    [[ "$(ipc viewMode)" == "list" ]] || fail "columns: the list button did not switch back"
    kill_flea
}

case_operations() {
    local dir="$fixture_root/operations"
    sandbox_scratch "$dir"
    printf 'body\n' > "$dir/notes.txt"
    magick -size 48x32 xc:navy "$dir/shot.png"
    bsdtar -a -c -f "$dir/bundle.tar.zst" -C "$dir" notes.txt

    launch "$dir"
    wait_listing 3

    # The submenu is exactly the table the backend probed, never a fixed list.
    [[ "$(ipc archiveFormats)" == *"tar.zst"* ]] || fail "operations: the probed formats are $(ipc archiveFormats)"
    [[ "$(ipc canConvert)" == "true" ]] || fail "operations: this box reports no converter"

    # Each row kind is offered exactly the operations that apply to it, which is the design's gating.
    seek_row_named "shot.png"
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Convert"* ]] || fail "operations: an image was offered no Convert"
    [[ "$(ipc contextMenuEntries)" != *"Extract"* ]] || fail "operations: an image was offered Extract"
    key -k Escape >/dev/null
    settle

    seek_row_named "bundle.tar.zst"
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Extract"* ]] || fail "operations: an archive was offered no Extract"
    [[ "$(ipc contextMenuEntries)" != *"Convert"* ]] || fail "operations: an archive was offered Convert"
    printf 'OPERATIONS archive-menu=%s\n' "$(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    echo "-- compress through the submenu --"
    seek_row_named "notes.txt"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Compress"
    key -k Return >/dev/null
    settle
    # The first submenu row is the first format the table offered.
    key -k Return >/dev/null
    for _ in $(seq 1 40); do [[ -s "$dir/notes.zip" ]] && break; sleep 0.25; done
    [[ -s "$dir/notes.zip" ]] || fail "operations: compress wrote no archive, bar says $(ipc lastMessage)"
    printf 'OPERATIONS compressed=%s\n' "$(ipc lastMessage)"
    shot operations-compressed

    echo "-- convert through the one popup --"
    seek_row_named "shot.png"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Convert"
    key -k Return >/dev/null
    settle
    [[ "$(ipc convertOpen)" == "true" ]] || fail "operations: the convert popup did not open"
    # The format that starts picked is never the one the file already is, and strip starts off.
    [[ "$(ipc convertFormat)" == "jpg" ]] || fail "operations: the popup opened on $(ipc convertFormat)"
    [[ "$(ipc convertStrip)" == "false" ]] \
        || fail "operations: remove-metadata started ticked, which is not least surprise"
    shot operations-convert
    key -k Return >/dev/null
    for _ in $(seq 1 40); do [[ -s "$dir/shot (converted).jpg" ]] && break; sleep 0.25; done
    [[ -s "$dir/shot (converted).jpg" ]] || fail "operations: convert wrote nothing, bar says $(ipc lastMessage)"
    [[ "$(magick identify -format '%m' "$dir/shot (converted).jpg")" == "JPEG" ]] \
        || fail "operations: the converted file is not a jpeg"
    # Never in place: the file it came from is untouched.
    [[ "$(magick identify -format '%m' "$dir/shot.png")" == "PNG" ]] \
        || fail "operations: the source was written over"
    printf 'OPERATIONS converted=%s\n' "$(ipc lastMessage)"
    kill_flea
}

case_grid() {
    local dir="$fixture_root/grid"
    sandbox_scratch "$dir"
    # More than two rows of tiles at any sane column count, so a step down never lands on the clamp
    # and the stride assertions below measure the grid rather than the end of the listing.
    local i
    for i in $(seq -w 1 60); do printf 'body\n' > "$dir/file-$i.txt"; done
    launch "$dir"
    wait_listing 60
    [[ "$(ipc viewMode)" == "list" ]] || fail "grid: the window did not open in list view"

    click_chrome grid
    settle
    [[ "$(ipc viewMode)" == "grid" ]] || fail "grid: the chrome button did not switch the view"
    shot grid-view

    # The grid has no columns to head, so its strip collapses and the tiles start under the chrome.
    local header_y chrome_h
    header_y=$(ipc headerTop)
    chrome_h=$(ipc chromeHeight)
    (( header_y == chrome_h )) \
        || fail "grid: the column header did not collapse, it is at $header_y with a chrome of $chrome_h"

    # A step down moves one row of tiles, which is the column count; a step right moves one tile.
    local start down right
    start=$(ipc cursor)
    key -k Down >/dev/null
    settle
    down=$(ipc cursor)
    (( down > start + 1 )) \
        || fail "grid: Down moved $((down - start)), which is a list step and not a row of tiles"
    key -k Right >/dev/null
    settle
    right=$(ipc cursor)
    (( right == down + 1 )) || fail "grid: Right moved $((right - down)) rather than one tile"
    key -k Up >/dev/null
    settle
    (( $(ipc cursor) == right - (down - start) )) || fail "grid: Up did not undo the row Down moved"
    printf 'GRID columns=%s cursorAfterDown=%s afterRight=%s\n' "$((down - start))" "$down" "$right"

    click_chrome list
    settle
    [[ "$(ipc viewMode)" == "list" ]] || fail "grid: the list button did not switch back"
    # The keyboard has to come back with the view, or the hidden grid would still be holding focus.
    start=$(ipc cursor)
    key -k Down >/dev/null
    settle
    (( $(ipc cursor) == start + 1 )) \
        || fail "grid: after switching back the list did not take one step, so focus stayed with the grid"
    kill_flea
}

case_header() {
    launch "$repo"
    local titles mark
    titles=$(ipc headerTitles)
    mark=$(ipc sortMark)
    printf 'HEADER titles=%s mark=%s\n' "$titles" "$mark"
    shot header
    [[ "$titles" == "Name|Mode|Size|Date Modified|Kind" ]] || fail "header: titles are $titles"
    [[ "$mark" == "name:asc" ]] || fail "header: the sort mark reads $mark"

    # Gaps are anchored constants, so this only guards the wiring; overflow is guarded per cell in case_overflow.
    local name_x name_w mode_x mode_w size_x size_w date_x date_w kind_x kind_w
    IFS='|' read -r name_x name_w <<< "$(ipc headerCellRect name)"
    IFS='|' read -r mode_x mode_w <<< "$(ipc headerCellRect mode)"
    IFS='|' read -r size_x size_w <<< "$(ipc headerCellRect size)"
    IFS='|' read -r date_x date_w <<< "$(ipc headerCellRect date)"
    IFS='|' read -r kind_x kind_w <<< "$(ipc headerCellRect kind)"
    printf 'HEADER geometry name=%s+%s mode=%s+%s size=%s+%s date=%s+%s kind=%s+%s\n' \
        "$name_x" "$name_w" "$mode_x" "$mode_w" "$size_x" "$size_w" "$date_x" "$date_w" "$kind_x" "$kind_w"
    (( mode_x >= name_x + name_w )) || fail "header: mode starts at $mode_x, before name ends at $((name_x + name_w))"
    (( size_x >= mode_x + mode_w )) || fail "header: size starts at $size_x, before mode ends at $((mode_x + mode_w))"
    (( date_x >= size_x + size_w )) || fail "header: date starts at $date_x, before size ends at $((size_x + size_w))"
    (( kind_x >= date_x + date_w )) || fail "header: kind starts at $kind_x, before date ends at $((date_x + date_w))"

    kill_flea
}

# contentWidth exceeds width only when elide is missing, since elide always caps it to width; this guards elide, not sizing.
case_overflow() {
    local dir="$fixture_root/overflow"
    sandbox_scratch "$dir"
    # 999,950-999,999 bytes is the SI ladder's own widest render, "1000.0 kB" (9 chars, measured in fix round 1).
    truncate -s 999950 "$dir/worst-case.bin"
    chmod 777 "$dir/worst-case.bin"
    touch -d "-1 day" "$dir/worst-case.bin"
    launch "$dir"
    wait_listing 1
    local overflow
    overflow=$(ipc rowCellOverflow 0)
    printf 'OVERFLOW row0=%s row=%s\n' "$overflow" "$(ipc rowAt 0)"
    shot overflow
    [[ "$overflow" == "0|0|0|0" ]] || fail "overflow: a cell painted past its own column, $overflow"
    kill_flea
}

# The OEM modules are reached by symlink, so a broken link is a silent palette regression.
case_oem() {
    launch "$repo"
    local fg
    fg=$(ipc themeForeground)
    [[ "$fg" == "$(theme_key foreground)" ]] \
        || fail "oem: Flea's foreground is $fg, the theme's is $(theme_key foreground)"
    local ladder
    ladder=$(ipc selectedFill)
    [[ -n "$ladder" && "$ladder" != "#00000000" ]] \
        || fail "oem: the OEM state ladder did not resolve, selectedFill is $ladder"
    printf 'OEM foreground=%s selectedFill=%s\n' "$fg" "$ladder"
    kill_flea
}

# Catches removing the icon slot from ui/Row.qml or its theme fallback.
case_icons() {
    local dir="$fixture_root/icons"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir"
    : > "$dir/plain.txt"
    printf 'x' > "$dir/cert.pem"
    # Empty on purpose: the MIME comes from the name, and a real jpeg would grow a thumbnail once Task 7 lands.
    : > "$dir/photo.jpg"
    ln -s "$dir/subdir" "$dir/linkdir"
    launch "$dir"
    wait_listing 5
    local row_height i y0 y1 pitch
    row_height=$(ipc metrics | cut -d' ' -f4)
    for i in 0 1 2 3 4; do
        printf 'ICONS row=%s name=%q glyph=%q\n' "$i" "$(ipc rowAt "$i")" "$(ipc rowGlyph "$i")"
    done
    shot icons
    for i in 0 1 2 3 4; do
        [[ -n "$(ipc rowGlyph "$i")" ]] || fail "row $i has no glyph name at all"
    done
    [[ "$(glyph_of subdir)" == "folder" ]] || fail "the directory row is not the folder glyph: $(glyph_of subdir)"
    [[ "$(glyph_of linkdir)" == "folder" ]] || fail "the symlink to a directory is not the folder glyph: $(glyph_of linkdir)"
    [[ "$(glyph_of photo.jpg)" == "image" ]] || fail "the jpeg row is not the image glyph: $(glyph_of photo.jpg)"
    [[ "$(glyph_of cert.pem)" != "terminal" ]] || fail "the pem row still draws as the terminal (executable) glyph"
    read -r _pitch_x0 y0 <<< "$(ipc rowCentre 0)"
    read -r _pitch_x1 y1 <<< "$(ipc rowCentre 1)"
    [[ -n "$y0" && -n "$y1" ]] || fail "row 0 or row 1 reported no on-screen centre, so no pitch was ever rendered to measure"
    pitch=$(( y1 - y0 ))
    printf 'ICONS pitch=%s rowHeight=%s\n' "$pitch" "$row_height"
    # The pitch is read off itemRect and not off a Theme token, so a slot that grows its row reddens here.
    [[ "$pitch" == "$row_height" ]] || fail "the rendered row pitch is $pitch, not the $row_height the theme asks for"
}

# Catches turning the settle timer in ui/Pane.qml into a request per scrolled frame.
case_thumbs() {
    [[ -d "$FIXTURE_ROOT/flea-media-btrfs" ]] || fail "the media fixture is missing"
    sandbox_make "$thumb_fixture"
    local i
    for i in $(seq 0 $((thumb_rows - 1))); do
        ln "$FIXTURE_ROOT/flea-media-btrfs/photo_0.jpg" "$thumb_fixture/p$i.jpg"
    done
    # kill_flea waits out the previous backend's drain, so the baseline is stable before this one generates.
    local before_large
    kill_flea
    before_large=$(ls -A "$cache_large" | wc -l)
    launch "$thumb_fixture"
    wait_listing "$thumb_rows"
    # Read before any thumbnail lands, so the slot is compared against its pre-thumbnail row.
    local row_height
    row_height=$(ipc metrics | cut -d' ' -f4)

    # One settle after the first window, and one request naming the visible rows.
    local waited
    for waited in $(seq 1 $((thumb_fill_s * 20))); do
        [[ -n "$(ipc thumbFile 0)" ]] && break
        sleep 0.05
    done
    printf 'THUMBS requests=%s file0=%q file1=%q\n' "$(ipc thumbRequests)" "$(ipc thumbFile 0)" "$(ipc thumbFile 1)"
    shot thumbs-first-screen
    [[ "$(ipc thumbRequests)" == "1" ]] || fail "the first screen took $(ipc thumbRequests) requests, not one"
    [[ "$(ipc thumbFile 0)" == "$cache_large/"*.png ]] || fail "row 0 has no cached thumbnail: $(ipc thumbFile 0)"
    local screen_rows icon0 icon_next glyph_next y0 y1 pitch
    screen_rows=$(ipc visibleRows)
    icon0=$(ipc rowIcon 0)
    # The first row past the requested screen is still built by the cache buffer, so it witnesses the scoping.
    icon_next=$(ipc rowIcon "$screen_rows")
    glyph_next=$(ipc rowGlyph "$screen_rows")
    read -r _thumb_x0 y0 <<< "$(ipc rowCentre 0)"
    read -r _thumb_x1 y1 <<< "$(ipc rowCentre 1)"
    [[ -n "$y0" && -n "$y1" ]] || fail "row 0 or row 1 reported no on-screen centre, so no pitch was ever rendered to measure"
    pitch=$(( y1 - y0 ))
    printf 'THUMBS icon0=%q screenRows=%s iconNext=%q glyphNext=%q pitch=%s rowHeight=%s\n' \
        "$icon0" "$screen_rows" "$icon_next" "$glyph_next" "$pitch" "$row_height"
    # The mtime query is part of the shape, so cutting it reddens here as well as in case_stale.
    [[ "$icon0" == "file://$cache_large/"*".png?m="* ]] || fail "row 0 still draws its icon: $icon0"
    # The glyph, not the thumbnail URL, proves the row was built: an unrequested row carries no thumbnail at all.
    [[ -n "$glyph_next" ]] || fail "row $screen_rows was never built, so no unrequested row was looked at"
    [[ -z "$icon_next" ]] || fail "row $screen_rows was never requested and still draws a thumbnail: $icon_next"
    # The pitch comes off itemRect and not off a Theme token, so a thumbnail that grew its row reddens here.
    [[ "$pitch" == "$row_height" ]] || fail "a thumbnail made the rendered pitch $pitch, not the $row_height the theme asks for"

    # Nothing is requested while the list is moving: the cursor tracks the viewport, so it is the witness.
    local wx wy ww wh before_requests moved_without_request cursor_a cursor_b scroller
    read -r wx wy ww wh < <(window_box)
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    before_requests=$(ipc thumbRequests)
    moved_without_request=0
    # Every detent lands before the scroll call returns, so the fling is sampled while it is still being sent.
    omarchy-drive scroll down "$fling_clicks" >/dev/null &
    scroller=$!
    cursor_a=$(ipc cursor)
    wait "$scroller"
    sleep 0.5
    cursor_b=$(ipc cursor)
    [[ "$cursor_b" != "$cursor_a" ]] && moved_without_request=1
    printf 'THUMBS fling before=%s during_samples=%s after=%s\n' \
        "$before_requests" "$moved_without_request" "$(ipc thumbRequests)"
    sleep 1

    # Every number is read while the window lives and asserted after it dies, so the cache count can go first.
    local screen_rows requests added
    screen_rows=$(ipc visibleRows)
    requests=$(ipc thumbRequests)
    kill_flea
    sandbox_make "$thumb_fixture"
    added=$(( $(ls -A "$cache_large" | wc -l) - before_large ))
    # A window that is not row aligned straddles one more row than it holds, so the bound is the viewport rule plus that row.
    printf 'THUMBS settled requests=%s cache before=%s now=%s added=%s bound=%s\n' \
        "$requests" "$before_large" "$(ls -A "$cache_large" | wc -l)" "$added" "$(( requests * (screen_rows + 1) ))"

    # A warm cache would satisfy the ceiling below with nothing generated, so the witness checks itself first.
    (( added >= screen_rows )) \
        || fail "only $added rows were generated, so the cache was already warm and the bound below proves nothing"
    # The rule that forfeits the project is a row count and thumbRequests counts lines; see AGENTS.md "Thumbnail requests in the GUI".
    (( added <= requests * (screen_rows + 1) )) \
        || fail "$added of $thumb_rows rows were generated by $requests requests of at most $screen_rows visible rows and one straddled row each"
    (( requests - before_requests <= 2 )) \
        || fail "one fling issued $(( requests - before_requests )) requests"
    # And a lower bound, because a settle timer no scroll ever restarts would also issue none at all.
    (( requests - before_requests >= 1 )) \
        || fail "the fling stopped on rows nothing had asked for and settled without asking"
    # Measured before and after the fling rather than sampled inside it. The sampled form asked to
    # catch the cursor moving with no request in flight, and could not do so reliably: it read a
    # settled value and reported zero samples on a fling that had plainly moved 9 rows, which made
    # the witness unprovable rather than false, and that is worse. Not because the fling is short.
    # It is 1500 ydotool spawns, and 1500 spawns of /usr/bin/true alone take about 0.8 s here, against
    # an ipc round trip of a few hundred ms (190 to 565 ms measured, see clip_seconds below), so the
    # fling outlasts a round trip several times over, as fling_clicks says. Why the sampling window
    # admitted so few reads is an open question. What the witness still does is the job it was for:
    # a fling that moves nothing fails here instead of passing quietly, and the "nothing is requested
    # while moving" property is carried by the request-count bounds above.
    (( moved_without_request >= 1 )) \
        || fail "the fling did not move the viewport at all, so the bounds above prove nothing"
    [[ "$(ls -A "$cache_large" | grep -c '^\.flea-')" == "0" ]] || fail "a temp file was left in the shared cache"
}

# Catches encodeURI in ui/Row.qml leaving # or ? literal, which Qt reads as URL syntax and cannot open.
case_hashcache() {
    [[ -d "$FIXTURE_ROOT/flea-media-btrfs" ]] || fail "the media fixture is missing"
    local pics="$hash_fixture/pics"
    # The two bytes encodeURI leaves alone, in the half of the path that can legally hold them.
    local cache="$hash_fixture/c#a?che"
    sandbox_make "$hash_fixture"
    mkdir -p "$pics" "$cache/thumbnails/large" "$cache/thumbnails/fail"
    ln "$FIXTURE_ROOT/flea-media-btrfs/photo_0.jpg" "$pics/one.jpg"
    # Exported inside this case's own subshell, so no other case reads or writes the redirected root.
    export XDG_CACHE_HOME="$cache"
    launch "$pics"
    wait_listing 1

    local waited file icon status
    for waited in $(seq 1 $((thumb_fill_s * 20))); do
        [[ -n "$(ipc thumbFile 0)" ]] && break
        sleep 0.05
    done
    file=$(ipc thumbFile 0)
    # The decode is asynchronous, so a status read taken at once would catch Loading and prove nothing.
    for waited in $(seq 1 $((thumb_fill_s * 20))); do
        status=$(ipc rowIconStatus 0)
        [[ "$status" != "$image_loading" ]] && break
        sleep 0.05
    done
    icon=$(ipc rowIcon 0)
    printf 'HASHCACHE file=%q icon=%q status=%s generated=%s\n' \
        "$file" "$icon" "$status" "$(ls -A "$cache/thumbnails/large" | wc -l)"
    shot hashcache
    [[ "$file" == "$cache/thumbnails/large/"*.png ]] \
        || fail "the backend did not use the redirected cache root, so no # ever reached the row: $file"
    [[ "$icon" == *%23* && "$icon" == *%3F* ]] || fail "the row URL left # or ? unescaped: $icon"
    [[ "$status" == "$image_ready" ]] || fail "the row URL never opened, Image.status is $status: $icon"
    kill_flea
    sandbox_make "$hash_fixture"
}

# Catches Qt's URL-keyed pixmap cache in ui/Row.qml redrawing the old frame after a thumbnail is regenerated.
case_stale() {
    command -v magick >/dev/null || fail "ImageMagick is missing, so no chroma of the icon slot can be read"
    local pics="$stale_fixture/tree/pics"
    local cache="$stale_fixture/cache"
    local src="$stale_fixture/src"
    sandbox_make "$stale_fixture"
    mkdir -p "$pics" "$src" "$cache/thumbnails/large" "$cache/thumbnails/fail"
    magick -size 512x512 xc:red "$src/before.jpg"
    magick -size 512x512 xc:blue "$src/after.jpg"
    cp "$src/before.jpg" "$pics/one.jpg"
    # Exported inside this case's own subshell, so no other case reads or writes the redirected root.
    export XDG_CACHE_HOME="$cache"
    launch "$pics"
    wait_listing 1

    local first_file crop red_before blue_before
    wait_thumb_ready
    first_file=$(ipc thumbFile 0)
    crop=$(icon_crop)
    shot stale-before
    red_before=$(count_pixels "$evidence_dir/stale-before.png" "$crop" "$icon_red")
    blue_before=$(count_pixels "$evidence_dir/stale-before.png" "$crop" "$icon_blue")
    printf 'STALE before file=%q crop=%s red=%s blue=%s\n' "$first_file" "$crop" "$red_before" "$blue_before"
    [[ "$first_file" == "$cache/thumbnails/large/"*.png ]] \
        || fail "the backend did not use the redirected cache root, so nothing here is this case's own: $first_file"
    (( red_before > 0 )) || fail "the first thumbnail drew no red pixel at all, so this case could not fail"
    (( blue_before == 0 )) || fail "the first thumbnail drew $blue_before blue pixels before anything was edited"

    cp "$src/after.jpg" "$pics/one.jpg"
    touch -d "@$(( $(date +%s) - stale_mtime_back_s ))" "$pics/one.jpg"
    # Only Pane.open clears the thumbnail map, so leaving the directory and coming back is what re-asks for the row.
    key h >/dev/null
    wait_path "$stale_fixture/tree"
    wait_listing 1
    # The second tap is what opens a row, see keys.toml's [[pointer]] table.
    click_row 0 left --double
    wait_path "$pics"
    wait_listing 1

    local second_file red_after blue_after cache_colour cache_blue cache_red
    wait_thumb_ready
    second_file=$(ipc thumbFile 0)
    crop=$(icon_crop)
    shot stale-after
    red_after=$(count_pixels "$evidence_dir/stale-after.png" "$crop" "$icon_red")
    blue_after=$(count_pixels "$evidence_dir/stale-after.png" "$crop" "$icon_blue")
    cache_colour=$(magick "$second_file" -resize 1x1! -format "%[fx:int(u.b*255)] %[fx:int(u.r*255)]" info:)
    printf 'STALE after file=%q same_path=%s icon=%q red=%s blue=%s cachepng_b_r=%s\n' \
        "$second_file" "$([[ "$second_file" == "$first_file" ]] && printf yes || printf no)" \
        "$(ipc rowIcon 0)" "$red_after" "$blue_after" "$cache_colour"
    [[ "$second_file" == "$first_file" ]] \
        || fail "the regenerated thumbnail landed at a new path, so the stale frame this case guards is unreachable"
    read -r cache_blue cache_red <<< "$cache_colour"
    (( cache_blue > 128 && cache_red < 128 )) \
        || fail "the backend never regenerated the thumbnail, so the screen below has nothing new to show"
    (( blue_after > 0 )) || fail "the regenerated thumbnail drew no blue pixel, so the row is showing the old frame"
    (( red_after == 0 )) || fail "the row still draws $red_after red pixels of the thumbnail it replaced"
    kill_flea
    sandbox_make "$stale_fixture"
}

# Catches any request for a row the client did not name, which is the rule that forfeits the project.
case_nosweep() {
    [[ -d "$bench_dir" ]] || fail "the 100,000-file fixture is missing at $bench_dir"
    # Same shape as case_thumbs: the drain the previous backend owes this cache is finished before the baseline.
    local before_large after_large wx wy ww wh burst
    kill_flea
    before_large=$(ls -A "$cache_large" | wc -l)
    launch "$bench_dir"
    wait_listing 100000
    read -r wx wy ww wh < <(window_box)
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    for burst in $(seq 1 "$scroll_bursts"); do
        omarchy-drive scroll down "$wheel_clicks" >/dev/null
    done
    key G >/dev/null
    sleep 1
    key g >/dev/null
    sleep 1
    after_large=$(ls -A "$cache_large" | wc -l)
    printf 'NOSWEEP requests=%s cursor=%s cache before=%s after=%s\n' \
        "$(ipc thumbRequests)" "$(ipc cursor)" "$before_large" "$after_large"
    shot nosweep
    [[ "$(ipc thumbRequests)" == "0" ]] || fail "a directory of text files produced $(ipc thumbRequests) thumb requests"
    [[ "$before_large" == "$after_large" ]] || fail "the cache grew from $before_large to $after_large"
    [[ "$(ls -A "$cache_large" | grep -c '^\.flea-')" == "0" ]] || fail "a temp file was left in the shared cache"

    # Task 16's twin: every row here is a dirsize candidate; the delta bound stands in for a literal zero, see AGENTS.md "Thumbnail requests".
    local dirsweep_dir before_requests after_requests delta
    dirsweep_dir="$fixture_root/nosweep-dirs"
    if [[ "$(find "$dirsweep_dir" -maxdepth 1 -mindepth 1 -type d 2>/dev/null | wc -l)" -lt 100000 ]]; then
        sandbox_scratch "$dirsweep_dir"
        seq 1 100000 | sed "s#^#$dirsweep_dir/dir_#" | xargs mkdir
    fi
    kill_flea
    launch "$dirsweep_dir"
    wait_listing 100000
    read -r wx wy ww wh < <(window_box)
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    before_requests=$(ipc dirSizeRequests)
    omarchy-drive scroll down "$fling_clicks" >/dev/null
    sleep 1
    after_requests=$(ipc dirSizeRequests)
    delta=$((after_requests - before_requests))
    printf 'DIRSWEEP fling before=%s after=%s delta=%s cursor=%s\n' \
        "$before_requests" "$after_requests" "$delta" "$(ipc cursor)"
    shot dirsweep
    (( delta >= 1 )) \
        || fail "settling after the fling issued zero dirsize requests, so nothing here proves the gate is even wired"
    (( delta <= 2 )) \
        || fail "one fling issued $delta dirsize requests, not the one settle's worth a debounced viewport should cost"
}

# Catches Tab not reaching Focus.next, cursorUp not being wired, or Enter never opening a favourite.
case_focus() {
    local dir="$fixture_root/focus"
    sandbox_scratch "$dir"
    : > "$dir/plain.txt"
    launch "$dir"
    wait_listing 1
    [[ "$(ipc focusView)" == "list" ]] || fail "focus: Flea does not start on the list"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "focus: tab did not reach the rail"
    # Home plus real favourites, so cursorDown and cursorUp both have somewhere to land; avoids
    # racing the rail's two FileViews, which load after Sidebar exists and not before.
    wait_rail 3
    shot focus-railed
    key jj >/dev/null
    settle
    [[ "$(ipc railCursor)" == "2" ]] || fail "focus: j did not move the rail cursor"
    key k >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "focus: k did not move the rail cursor back up"
    key k >/dev/null
    settle
    [[ "$(ipc railCursor)" == "0" ]] || fail "focus: k did not return the rail cursor to Home"
    # Row 0 is always Home, which is $HOME on this box, so the opened path is predictable.
    key -k Return >/dev/null
    wait_path "$HOME"
    [[ "$(ipc path)" == "$HOME" ]] || fail "focus: Enter on Home did not open $HOME, path is $(ipc path)"
    # RailKeys.act's open case only emits sidebar.opened; nothing there hands focus back to the list.
    [[ "$(ipc focusView)" == "rail" ]] || fail "focus: opening a favourite unexpectedly moved focus off the rail"
    shot focus-opened
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "focus: escape did not return to the list"
    printf 'FOCUS view=%s railCursor=%s path=%s\n' "$(ipc focusView)" "$(ipc railCursor)" "$(ipc path)"
    shot focus-listed
    kill_flea
}

# Catches t not opening a tab, 1-9 not switching, w not closing, or the bar showing with one tab.
case_tabs() {
    local dir="$fixture_root/tabs"
    sandbox_scratch "$dir"
    mkdir -p "$dir/alpha" "$dir/beta"
    : > "$dir/note.txt"
    launch "$dir"
    wait_listing 3
    [[ "$(ipc tabCount)" == "1" ]] || fail "tabs: started with $(ipc tabCount) tabs, not 1"
    [[ "$(ipc tabBarVisible)" == "false" ]] || fail "tabs: the bar showed with one tab"
    key t >/dev/null
    settle
    [[ "$(ipc tabCount)" == "2" ]] || fail "tabs: t did not open a second tab, count=$(ipc tabCount)"
    [[ "$(ipc tabBarVisible)" == "true" ]] || fail "tabs: the bar stayed hidden after t"
    [[ "$(ipc tabIndex)" == "1" ]] || fail "tabs: t did not land on the new tab, index=$(ipc tabIndex)"
    seek_row_named "alpha" || fail "tabs: could not find alpha"
    key -k Return >/dev/null
    wait_path "$dir/alpha"
    key 1 >/dev/null
    wait_path "$dir"
    [[ "$(ipc tabIndex)" == "0" ]] || fail "tabs: 1 did not return to the first tab, index=$(ipc tabIndex)"
    key 2 >/dev/null
    wait_path "$dir/alpha"
    [[ "$(ipc tabIndex)" == "1" ]] || fail "tabs: 2 did not return to the second tab, index=$(ipc tabIndex)"
    key w >/dev/null
    wait_path "$dir"
    [[ "$(ipc tabCount)" == "1" ]] || fail "tabs: w did not close the current tab, count=$(ipc tabCount)"
    [[ "$(ipc tabBarVisible)" == "false" ]] || fail "tabs: the bar stayed up after the last extra tab closed"
    key w >/dev/null
    wait_message "Can't close the last tab."
    key 3 >/dev/null
    wait_message "No tab 3."
    shot tabs-one
    key t >/dev/null
    settle
    local centre cx cy wx wy
    centre=$(ipc tabCentre 0)
    [[ -n "$centre" ]] || fail "tabs: tab 0 has no centre"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box)
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null
    settle
    [[ "$(ipc tabIndex)" == "0" ]] || fail "tabs: clicking tab 0 did not select it, index=$(ipc tabIndex)"
    shot tabs-two
    printf 'TABS count=%s index=%s labels=%s\n' "$(ipc tabCount)" "$(ipc tabIndex)" "$(ipc tabLabels)"
    kill_flea
}

# Catches Space not opening a preview, the kind dispatch misclassifying a row, or the size gate not firing.
case_preview() {
    command -v ffmpeg >/dev/null || fail "ffmpeg is missing, so the mp4 fixture cannot be built"
    local dir="$fixture_root/preview"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    # The double click below opens the row, so notes.md would hit the real gio open without this stub, the same hazard case_open already fixed.
    local opened="$dir/opened.log"
    : > "$opened"
    # Only the open subcommand is intercepted, so stubbing the opener leaves the gio mount calls
    # ui/NetworkMounts.qml makes on every launch answering from the real gio. That name is the mount
    # tool's own and is spelled by hand here; the stub's name is derived from src/open.rs instead.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$dir/bin/$open_handoff"
    chmod +x "$dir/bin/$open_handoff"
    printf 'hello from flea\n' > "$dir/sample.txt"
    printf '# Notes\n\nSome *text*.\n' > "$dir/notes.md"
    truncate -s 2M "$dir/big.txt"
    # A 440 Hz tone and not silence, so playback is provable by ear and not just by state. Fifteen
    # seconds, not one: an omarchy-drive ipc round trip costs 190 to 565 ms measured on this box
    # (see the KB's ipc-timing-loops entry), so a one-second clip leaves the poll below one or two
    # samples to land the Playing state in, and this case caught that exact miss before the fix.
    # Task 22's own Right/Left checks need headroom on both sides of a 5 s seek (Focus.js's
    # SEEK_MS) starting from whatever position the round trips above already spent: at three
    # seconds the clip had finished before the checks ran at all, and at eight, Right alone
    # landed within 5 s of the end, clamped to it, and stopped the player before Left ran.
    clip_seconds=15
    python3 - "$dir/tone.wav" "$clip_seconds" <<'PYEOF'
import sys, wave, struct, math
sample_rate = 44100
seconds = int(sys.argv[2])
with wave.open(sys.argv[1], "w") as f:
    f.setnchannels(1)
    f.setsampwidth(2)
    f.setframerate(sample_rate)
    for i in range(sample_rate * seconds):
        sample = int(16000 * math.sin(2 * math.pi * 440 * i / sample_rate))
        f.writeframesraw(struct.pack("<h", sample))
PYEOF
    ffmpeg -y -f lavfi -i "testsrc=duration=$clip_seconds:size=64x64:rate=10" "$dir/clip.mp4" >/dev/null 2>&1
    [[ -s "$dir/clip.mp4" ]] || fail "ffmpeg produced no clip.mp4"
    # Space on a pdf answered "This file cannot be previewed" until kindOf gained its branch, on a
    # type the preview column had rendered all along, so this row is what guards that branch.
    magick \( -size 400x560 xc:white -fill black -font Liberation-Sans -pointsize 40 -annotate +40+80 'PAGEONE' \) \
           \( -size 400x560 xc:white -fill black -font Liberation-Sans -pointsize 40 -annotate +40+80 'PAGETWO' \) \
           "$dir/manual.pdf"
    [[ -s "$dir/manual.pdf" ]] || fail "magick produced no manual.pdf"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export PATH="$saved_path"
    # bin/ and opened.log are the gio stub's own fixture entries, alongside the six under test.
    wait_listing 8

    # A single click must move the cursor and nothing else: no preview, and since 2026-09-02 no open
    # either. The double click below is the negative control that the stub can see one at all.
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: open at launch, before any interaction"
    click_row "$(row_index_of notes.md)" left
    settle
    [[ "$(ipc cursor)" == "$(row_index_of notes.md)" ]] || fail "preview: click did not move the cursor to notes.md"
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: a left click opened the preview"
    [[ ! -s "$opened" ]] || fail "preview: a single left click opened $(cat "$opened")"
    click_row "$(row_index_of notes.md)" left --double
    for _attempt in $(seq 1 100); do
        grep -q "^OPENED $dir/notes.md$" "$opened" && break
        sleep 0.05
    done
    grep -q "^OPENED $dir/notes.md$" "$opened" || fail "preview: a double click never reached the real-open path the stub is guarding"
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: the double click opened the preview rather than the file"
    # Hover is a plain pointer move, no button, over a different row than the click landed on.
    read -r hx hy <<< "$(ipc rowCentre "$(row_index_of big.txt)")"
    read -r wx wy ww wh < <(window_box)
    omarchy-drive move "$((hx + wx))" "$((hy + wy))" >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: hovering a row opened the preview"
    # Negative control: Space still opens it, so the two checks above proved a real absence, not a broken previewOpen read.
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: space stopped opening the preview after the click/hover checks"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the preview after the negative control"

    goto_row "$(row_index_of sample.txt)"
    key l >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: l on sample.txt did not open a preview"
    [[ "$(ipc previewKind)" == "text" ]] || fail "preview: sample.txt classified as $(ipc previewKind), not text"
    shot preview-text
    # Task 22 repurposes Space for play/pause only on a MEDIA preview; text keeps the old binding.
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: space did not close the text preview, the media-only rebinding leaked into text"

    open_row notes.md
    # Markdown renders verbatim like any other text, so the kind is text and there is no second path.
    [[ "$(ipc previewKind)" == "text" ]] || fail "preview: notes.md classified as $(ipc previewKind), not text"
    shot preview-markdown
    key -k Escape >/dev/null
    settle

    open_row_fast tone.wav
    [[ "$(ipc previewKind)" == "audio" ]] || fail "preview: tone.wav classified as $(ipc previewKind), not audio"
    wait_preview_state playing
    shot preview-audio

    # Left/Right seek 5 s (Focus.js's SEEK_MS), read before the pause/resume dance below spends
    # its own several IPC round trips (190 to 565 ms each, see the clip_seconds comment above): an
    # eight-second clip still has room left once this runs, so both directions are unclamped.
    local pos_before pos_after
    pos_before=$(ipc previewPosition)
    key -k Right >/dev/null
    settle
    pos_after=$(ipc previewPosition)
    (( pos_after > pos_before )) \
        || fail "preview: Right did not move tone.wav forward, before=$pos_before after=$pos_after"
    pos_before=$pos_after
    key -k Left >/dev/null
    settle
    pos_after=$(ipc previewPosition)
    (( pos_after < pos_before )) \
        || fail "preview: Left did not move tone.wav back, before=$pos_before after=$pos_after"

    # Task 22: Space toggles play/pause on a MEDIA preview instead of closing it.
    key -k space >/dev/null
    wait_preview_state paused
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: space paused tone.wav but also closed the preview"
    pos_before=$(ipc previewPosition)
    key l >/dev/null
    settle
    pos_after=$(ipc previewPosition)
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: l closed the paused audio preview"
    [[ "$(ipc previewState)" == "paused" ]] || fail "preview: l changed paused audio to $(ipc previewState)"
    [[ "$pos_after" == "$pos_before" ]] \
        || fail "preview: l moved paused audio, before=$pos_before after=$pos_after"
    key -k space >/dev/null
    wait_preview_state playing

    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the audio preview after the media-control checks"

    open_row_fast clip.mp4
    [[ "$(ipc previewKind)" == "video" ]] || fail "preview: clip.mp4 classified as $(ipc previewKind), not video"
    wait_preview_state playing
    shot preview-video
    [[ "$(ipc previewStripVisible)" == "true" ]] \
        || fail "preview: the strip is not visible right after opening the video preview"

    # Fix round 1: the reviewer's finding was that PanelSlider's own MouseArea swallows pointer
    # activity during a scrub or a wheel-seek, so it never reached the two outer hover MouseAreas
    # that call revealStrip(); the fix relays the slider's own moved signal to revealStrip() too
    # (PanelSlider emits it for both a drag in progress and a wheel step). Live isolation of that
    # one line proved impractical on this harness: a temporary signal-level trace (console.log on
    # revealStrip, A/B'd against a git-diff-isolated build with only the onMoved line removed)
    # showed the two outer hover MouseAreas already reveal the strip for both a wheel scroll and
    # an in-place drag on the slider, fix present or not, because Qt Quick delivers hover position
    # updates to every hoverEnabled MouseArea under the pointer independent of which item currently
    # holds the press grab, and the slider sits entirely inside mediaStrip's own hover area by
    # construction; see the fix round report for the full trace. What this still protects, real
    # end-to-end contract regardless of which code path currently satisfies it: interacting with
    # the slider keeps the strip visible past the point it would otherwise auto-hide, and it still
    # genuinely hides once both the interaction and the pointer itself have stopped. SECONDS is
    # bash's own elapsed-seconds counter, reset here and read nowhere else in this file.
    read -r slx sly <<< "$(ipc previewSliderCentre)"
    [[ -n "$slx" ]] || fail "preview: the seek slider reported no on-screen centre"
    read -r wx wy ww wh < <(window_box)
    omarchy-drive move "$((slx + wx))" "$((sly + wy))" >/dev/null

    SECONDS=0
    while (( SECONDS < 3 )); do sleep 0.2; done
    omarchy-drive scroll up 1 >/dev/null
    local interact_at=$SECONDS
    # A pointer left resting anywhere over the overlay keeps re-triggering the hover MouseAreas
    # on its own with no further input (measured live: positionChanged fires repeatedly for a
    # motionless pointer too, likely the compositor's own periodic re-affirm), which would make
    # the "still eventually hides" check below meaningless; moving off the window stops that.
    omarchy-drive move 5 5 >/dev/null
    while (( SECONDS < 6 )); do sleep 0.2; done
    [[ "$(ipc previewStripVisible)" == "true" ]] \
        || fail "preview: the strip hid before the slider interaction's own stripHideMs window expired"
    while (( SECONDS < interact_at + 5 )); do sleep 0.2; done
    [[ "$(ipc previewStripVisible)" == "false" ]] \
        || fail "preview: the strip never auto-hid once the slider interaction's own window expired"

    key -k Escape >/dev/null
    settle

    open_row manual.pdf
    [[ "$(ipc previewKind)" == "pdf" ]] \
        || fail "preview: manual.pdf classified as $(ipc previewKind), not pdf"
    [[ "$(ipc previewState)" == "pdf" ]] \
        || fail "preview: the pdf overlay reports $(ipc previewState), so it fell through to the refusal"
    [[ "$(ipc previewPdfPage)" == "0" ]] || fail "preview: manual.pdf opened on page $(ipc previewPdfPage), not page 0"
    key l >/dev/null
    omarchy-drive wait ipc -p "$flea_ui" flea previewPdfPage 1 --timeout 10 >/dev/null \
        || fail "preview: l left manual.pdf on page $(ipc previewPdfPage), not page 1"
    omarchy-drive wait ocr flea PAGETWO --timeout 10 >/dev/null \
        || fail "preview: l advanced manual.pdf state but left page 1 painted"
    shot preview-pdf
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the pdf overlay"

    open_row big.txt
    [[ "$(ipc previewKind)" == "text" ]] || fail "preview: big.txt classified as $(ipc previewKind), not text"
    [[ "$(ipc previewState)" == "This file is too large to preview." ]] \
        || fail "preview: big.txt did not report the too-large sentence, state=$(ipc previewState)"
    shot preview-toolarge
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the final preview"

    printf 'PREVIEW text=ok markdown=ok audio=ok video=ok toolarge=ok mediacontrols=ok striphide=ok\n'
    kill_flea
}

# Catches the Network group failing to self-hide, the add dialog's keyboard path breaking the
# list's own keyboard focus, "a" leaking out of the rail into the list, and the dialog's own submit path
# silently never updating the rail when ~/.config/gtk-3.0/ was absent at launch (a FileView never
# watches a directory that did not exist at its own construction; the fix is Sidebar's own
# reloadBookmarks(), driven off the dialog's saved() signal, not the watch alone). HOME is
# overridden only for the launched process: the calling shell's own HOME is restored right after
# launch() so omarchy-drive itself is unaffected. Two rows, not one, in the fixture directory:
# setCursor clamps cursorDown to the single valid index on a one-row listing, which reads exactly
# like dead keyboard input and cost real time to tell apart from it while this case was written.
# gio's own mount table is per-user, not per-HOME, so a real share left mounted from other work
# would leak into networkEntries and fail the empty check no matter what fixture HOME says; this
# gates the empty check on "gio mount -l" itself carrying no Mount() line, and fails loud with
# that listing rather than guessing, since this case cannot unmount another task's own work.
case_network() {
    local dir="$fixture_root/network"
    sandbox_scratch "$dir"
    # Three rows so a cursor that did not move is distinguishable from one clamped to a short listing.
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"
    : > "$dir/apple.txt"
    local fixture_home="$fixture_root/network-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME"

    local live_mounts
    live_mounts=$(gio mount -l 2>/dev/null | grep -c '^Mount(') || true
    [[ "$live_mounts" -eq 0 ]] \
        || fail "network: $live_mounts real gio mount(s) already present, cannot assert an empty rail against ambient state: $(gio mount -l 2>/dev/null)"

    # No .config/gtk-3.0/ at all yet: the exact "absent at launch" shape the dialog's write must survive.
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 3
    [[ -z "$(ipc networkEntries)" ]] || fail "network: the group is not empty with no bookmarks, gio mounts or Dropbox"
    shot network-empty

    # "a" is the rail's add-dialog binding and nothing in the list; ui/js/Focus.js "lookup" scopes it.
    # Since v0.1.3 there is no type-ahead, so the cursor must not move either.
    [[ "$(ipc focusView)" == "list" ]] || fail "network: did not start on the list"
    [[ "$(ipc cursor)" == "0" ]] || fail "network: did not start on row 0"
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: a opened the dialog from the list, where it is not bound"
    [[ "$(ipc cursor)" == "0" ]] || fail "network: a moved the cursor, so something still type-aheads: $(ipc cursor)"

    # The real submit path: Tab to the rail, "a" opens the dialog there, type a location, Enter submits.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "network: Tab did not reach the rail"
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: a from the rail did not open the add-location dialog"
    shot network-dialog-open
    # The form opens with the caret in Host, which is the one field it actually needs.
    # TEST-NET-2 (RFC 5737): guaranteed non-routable, so this never actually dials out.
    key "198.51.100.1" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "smb://198.51.100.1:445/" ]] \
        || fail "network: the Mounts-as line reads $(ipc networkUri)"
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: Enter did not submit and close the dialog"
    for _attempt in $(seq 1 100); do
        [[ -n "$(ipc networkEntries)" ]] && break
        sleep 0.05
    done
    # With no share typed the label falls back to the host, which is what Protocols.label does and
    # what the sidebar row then carries.
    [[ "$(ipc networkEntries)" == "198.51.100.1|network|share|false" ]] \
        || fail "network: the dialog's own write never reached the rail with gtk-3.0/ absent at launch, got $(ipc networkEntries)"
    shot network-appeared
    [[ -f "$fixture_home/.config/gtk-3.0/bookmarks" ]] || fail "network: the dialog did not create gtk-3.0/bookmarks under the fixture HOME"

    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: Escape did not close the dialog"
    # g resets to row 0 first, so this reads the same whatever the dialog left the cursor on.
    key g >/dev/null
    settle
    key j >/dev/null
    settle
    [[ "$(ipc cursor)" == "1" ]] || fail "network: keyboard nav is dead after the dialog closed, cursor is $(ipc cursor)"

    # The five protocols the canvas draws, each prefilling its own port and naming its own path row.
    key -k Tab >/dev/null
    settle
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: the dialog did not reopen for the protocol pass"
    local want protocol port pathlabel
    for want in "SMB 445 Share" "SFTP 22 Path" "FTPS 21 Path" "WebDAV 443 Path" "NFS 2049 Export"; do
        read -r protocol port pathlabel <<< "$want"
        click_chip "$protocol"
        settle
        [[ "$(ipc networkProtocol)" == "$protocol" ]] \
            || fail "network: the chip did not pick $protocol, it is $(ipc networkProtocol)"
        [[ "$(ipc networkPort)" == "$port" ]] \
            || fail "network: $protocol prefilled port $(ipc networkPort), not $port"
        [[ "$(ipc networkPathLabel)" == "$pathlabel" ]] \
            || fail "network: $protocol names its path row $(ipc networkPathLabel), not $pathlabel"
    done
    printf 'NETWORK protocols=ok ports=445,22,21,443,2049\n'
    shot network-protocols
    key -k Escape >/dev/null
    settle

    # The traversal, driven rather than read. No new IPC is needed: the Mounts-as line is an exact
    # projection of protocol, host, port, path, domain and user, so where a typed pair LANDS is
    # observable, and a Tab that went wrong puts its pair somewhere else in the same string.
    # Port is tabbed through and never typed into, because it is the one prefilled field and the
    # caret position on programmatic focus is not something this case should depend on.
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: the dialog did not reopen for the traversal walk"
    key "hh" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "ss" >/dev/null
    key -k Tab >/dev/null
    key "dd" >/dev/null
    key -k Tab >/dev/null
    key "uu" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "smb://dd;uu@hh:445/ss" ]] \
        || fail "network: Tab did not walk host, port, share, domain, username in order, URI is $(ipc networkUri)"

    # Reached by keyboard rather than by click, which is the only thing that exercises a chip's own
    # Enter. Under SMB the TLS row is hidden, so one Tab from Username wraps past it to the first
    # chip and a second lands on SFTP: the wrap and the hidden-row skip are proven by arriving.
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh:22/ss" ]] \
        || fail "network: Enter on a tabbed-to chip did not pick SFTP, URI is $(ipc networkUri)"

    # SFTP hides Domain and the TLS row both. Two backtabs wrap back past the hidden TLS row to
    # Username, and a third must skip the hidden Domain and land on Path.
    key -k Backtab >/dev/null
    key -k Backtab >/dev/null
    key -k Backtab >/dev/null
    key "XX" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh:22/ssXX" ]] \
        || fail "network: Shift-Tab did not skip the hidden Domain and reach Path, URI is $(ipc networkUri)"
    printf 'NETWORK traversal=ok wrap=ok skip=ok chip-enter=ok\n'
    shot network-traversal
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: escape did not close the dialog after the walk"

    # Finding 17: picking a chip that hides the field holding the caret must re-home the caret to
    # the next visible field, or the next keystroke lands in a field the form never draws while
    # Enter still submits from it. Domain is SMB-only, so SFTP hides it.
    # The URI drops a domain that carries no username, so the walk is proven by the share instead,
    # and a caret one field early or late puts "uu" where the last assertion would catch it.
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: the dialog did not reopen for the re-home walk"
    key "hh" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "ss" >/dev/null
    key -k Tab >/dev/null
    key "dd" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "smb://hh:445/ss" ]] \
        || fail "network: the re-home walk did not reach Domain, URI is $(ipc networkUri)"
    click_chip SFTP
    settle
    [[ "$(ipc networkProtocol)" == "SFTP" ]] \
        || fail "network: the chip click did not pick SFTP, it is $(ipc networkProtocol)"
    key "uu" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh:22/ss" ]] \
        || fail "network: a chip click left the caret in the hidden Domain, URI is $(ipc networkUri)"
    printf 'NETWORK rehome=ok\n'
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] \
        || fail "network: escape did not close the dialog after the re-home walk"

    printf 'NETWORK empty=ok a-scoped=ok dialog=ok submit-path=ok keyboard-after=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
}

# Item 1 of Task 15 fix round 2: activating a bare smb://host/ entry lists its shares as pane
# rows (ui/ShareBrowser.qml) instead of round 1's superseded sidebar bookmark expansion, and Enter
# on a row mounts and opens through the exact ui/NetworkMounts.qml pipeline a bookmarked share
# already uses. gio is stubbed (the case_open/case_preview idiom) so this is hermetic: no real NAS,
# no auth prompt. The stub also reproduces gio's own "already mounted" quirk on the second share,
# found live against the real NAS this round when a share picked from the overlay was already
# mounted from a prior activation; mountProcess used to treat that as a hard failure for anything
# but a bare root, misreporting a location that actually mounted fine. The fix reads the process's
# own stderr instead of guessing from the uri's shape, see ui/NetworkMounts.qml "isAlreadyMountedQuirk".
case_sharebrowser() {
    local dir="$fixture_root/sharebrowser"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"

    local share1_dir="$fixture_root/sharebrowser-share1"
    local share2_dir="$fixture_root/sharebrowser-share2"
    sandbox_remove "$share1_dir"; sandbox_remove "$share2_dir"
    mkdir -p "$share1_dir" "$share2_dir"
    : > "$share1_dir/one.txt"
    : > "$share2_dir/one.txt"
    : > "$share2_dir/two.txt"

    local base_uri="smb://stubhost/"
    local share1_uri="smb://stubhost/share1/"
    local share2_uri="smb://stubhost/share2/"

    # A plain dispatcher, not a canned fixture: it answers exactly the four gio subcommands
    # ui/NetworkMounts.qml issues, keyed on the exact uri each entry activates.
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
case "\$1" in
  mount)
    if [ "\$2" = "-l" ] || [ "\$2" = "-u" ]; then
      exit 0
    fi
    if [ "\$2" = "$share2_uri" ]; then
      echo "gio: \$2: Location is already mounted" >&2
      exit 2
    fi
    exit 0
    ;;
  info)
    case "\$2" in
      "$share1_uri") printf 'local path: %s\n' "$share1_dir" ;;
      "$share2_uri") printf 'local path: %s\n' "$share2_dir" ;;
    esac
    exit 0
    ;;
  list)
    printf 'share1\nshare2\n'
    exit 0
    ;;
esac
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/sharebrowser-home"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf '%s StubNAS\n' "$base_uri" > "$fixture_home/.config/gtk-3.0/bookmarks"
    local real_home="$HOME"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/ is the gio stub's own fixture entry, alongside the two files under test.
    wait_listing 3
    wait_rail 2
    [[ "$(ipc networkEntries)" == "StubNAS|network|share|false" ]] \
        || fail "sharebrowser: the stub NAS bookmark did not appear, got $(ipc networkEntries)"

    # Tab to the rail and l the bare-root entry: it lists shares, it does not open anything.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "sharebrowser: Tab did not reach the rail"
    key g >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "sharebrowser: expected the rail cursor on StubNAS, got $(ipc railCursor)"
    key l >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: l on the bare root never opened the overlay"
    [[ "$(ipc shareBrowserEntries)" == "$(printf 'share1\nshare2')" ]] \
        || fail "sharebrowser: the overlay's own shares over IPC are wrong: $(ipc shareBrowserEntries)"
    [[ "$(ipc path)" == "$dir" ]] || fail "sharebrowser: listing the shares navigated away from $dir"
    shot sharebrowser-open

    # Escape returns to the listing underneath without mounting or navigating anywhere.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc shareBrowserOpen)" == "false" ]] || fail "sharebrowser: Escape did not close the overlay"
    [[ "$(ipc path)" == "$dir" ]] || fail "sharebrowser: Escape navigated to $(ipc path)"
    [[ "$(ipc total)" == "3" ]] || fail "sharebrowser: Escape changed the listing underneath"
    # A second Escape hands focus back to the list, ui/js/RailKeys.js "act"'s own escape case;
    # case_network's own post-dialog check relies on the exact same mechanism.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "sharebrowser: focus never returned to the list"
    key j >/dev/null
    settle
    [[ "$(ipc cursor)" == "1" ]] || fail "sharebrowser: keyboard nav is dead after Escape, cursor is $(ipc cursor)"

    # Pointer-opened overlay leaves list focus underneath, so l must route through the active overlay.
    click_rail_row 1 left
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: pointer reactivation of StubNAS did not reopen the overlay"
    [[ "$(ipc focusView)" == "list" ]] || fail "sharebrowser: pointer activation moved focus to $(ipc focusView)"
    [[ "$(ipc shareBrowserCursor)" == "0" ]] || fail "sharebrowser: the overlay did not reset its cursor to 0, got $(ipc shareBrowserCursor)"
    key l >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc path)" == "$share1_dir" ]] && break
        sleep 0.05
    done
    [[ "$(ipc path)" == "$share1_dir" ]] || fail "sharebrowser: l on share1 never opened $share1_dir, path is $(ipc path)"
    [[ "$(ipc shareBrowserOpen)" == "false" ]] || fail "sharebrowser: opening share1 did not close the overlay"
    [[ "$(ipc total)" == "1" ]] || fail "sharebrowser: share1's own listing did not load, total is $(ipc total)"
    shot sharebrowser-opened-share1

    # Pointer reopens the overlay over list focus; retained Return drives the already-mounted share.
    [[ "$(ipc focusView)" == "list" ]] || fail "sharebrowser: opening share1 moved focus to $(ipc focusView)"
    click_rail_row 1 left
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: second pointer reactivation of StubNAS did not reopen the overlay"
    [[ "$(ipc focusView)" == "list" ]] || fail "sharebrowser: second pointer activation moved focus to $(ipc focusView)"
    key j >/dev/null
    settle
    [[ "$(ipc shareBrowserCursor)" == "1" ]] || fail "sharebrowser: cursor did not move to share2, got $(ipc shareBrowserCursor)"
    key -k Return >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc path)" == "$share2_dir" ]] && break
        sleep 0.05
    done
    [[ "$(ipc path)" == "$share2_dir" ]] \
        || fail "sharebrowser: the already-mounted quirk was misreported as a failure, path is $(ipc path), message=$(ipc lastMessage)"
    [[ "$(ipc total)" == "2" ]] || fail "sharebrowser: share2's own listing did not load, total is $(ipc total)"

    printf 'SHAREBROWSER list=ok escape=ok mount-open=ok already-mounted-quirk=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"; sandbox_remove "$share1_dir"; sandbox_remove "$share2_dir"
}

# The rail's own context menu, which is the whole affordance: a release nobody can see is a release
# nobody has. gio is stubbed so no real unmount ever runs, and the stub logs each call it receives.
case_unmount() {
    local dir="$fixture_root/unmount"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"

    local unmount_log="$dir/unmount.log"
    : > "$unmount_log"
    local share_uri="smb://stubhost/stubshare/"
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
case "\$1 \$2" in
  "mount -l")
    printf 'Mount(0): stubshare on stubhost -> $share_uri\n  Type: GDaemonMount\n'
    exit 0
    ;;
  "mount -u")
    printf 'UNMOUNT %s\n' "\$3" >> "$unmount_log"
    exit 0
    ;;
esac
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/unmount-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/ and unmount.log are the gio stub's own fixture entries, alongside the two files under test.
    wait_listing 4
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] \
        || fail "unmount: the stub mount never appeared live, got $(ipc networkEntries)"
    [[ "$(ipc themeLoaded)" == "true" ]] || fail "unmount: the fixture home did not load a real theme"

    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "unmount: Tab did not reach the rail"

    # Right click raises the menu over the row and nothing else: one row, named, marked, and no
    # unmount has run. The old two-right-click arm is gone, see ui/Sidebar.qml "openRailMenu".
    click_rail_row 1 right
    settle
    printf 'UNMOUNT menu visible=%s entries=%s glyphs=%s\n' \
        "$(ipc contextMenuVisible)" "$(ipc contextMenuEntries)" "$(ipc contextMenuGlyphs)"
    shot unmount-menu
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "unmount: right click opened no menu on the share"
    [[ "$(ipc contextMenuEntries)" == "Unmount" ]] \
        || fail "unmount: the share's menu is $(ipc contextMenuEntries), not one Unmount row"
    [[ "$(ipc contextMenuGlyphs)" == "eject" ]] \
        || fail "unmount: the Unmount row draws $(ipc contextMenuGlyphs), not the eject mark"
    [[ -z "$(cat "$unmount_log")" ]] || fail "unmount: opening the menu already unmounted: $(cat "$unmount_log")"

    # Escape closes it and still nothing has run, which is what makes the menu the confirmation.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "unmount: Escape did not close the rail menu"
    [[ -z "$(cat "$unmount_log")" ]] || fail "unmount: Escape unmounted anyway: $(cat "$unmount_log")"

    # Choosing the row is what unmounts, and the row's key is what says which share, not its index.
    click_rail_row 1 right
    settle
    key -k Return >/dev/null
    wait_message "Unmounted stubshare."
    [[ "$(cat "$unmount_log")" == "UNMOUNT $share_uri" ]] \
        || fail "unmount: the menu row did not unmount $share_uri, log is: $(cat "$unmount_log")"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "unmount: the menu stayed open after its action ran"

    # A rail row with nothing to release opens no menu at all, rather than an empty frame. Home is
    # the one favourite this fixture home has, and it is not a mount.
    click_rail_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "unmount: a favourite opened a menu with nothing in it"

    # The one instance is shared with the listing, so the keyboard must come back to it afterwards:
    # a second ContextMenu in this tree once killed every key in the window, see AGENTS.md.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "unmount: Tab did not reach the list after the rail menu closed"
    local before after
    before=$(ipc cursor)
    key j >/dev/null
    settle
    after=$(ipc cursor)
    [[ "$after" != "$before" ]] || fail "unmount: the list stopped taking keys after the rail menu, cursor stuck at $before"

    printf 'UNMOUNT menu=ok escape=ok fire=ok no-menu-on-favourite=ok keyboard=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
}

# The eject half of the same menu, and the one property that must never bend: "safe to unplug" is
# read off an lsblk listing taken after gio exits, never off gio's exit code. Both are stubbed, so
# no real device is touched and no privilege is needed; the gio stub always exits 0, which is the
# exact lie the real tool told, and only the listing decides what the status bar says.
case_eject() {
    local dir="$fixture_root/eject"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin" "$dir/mnt/FLEASTICK"
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"

    local gio_log="$dir/gio.log"
    : > "$gio_log"
    # Every state this case needs, at zero privilege: one internal disk and one removable volume,
    # whose mountpoint goes away only once the gio stub has been told to really eject it.
    cat > "$dir/bin/lsblk" <<EOS
#!/bin/sh
if [ -f "$dir/ejected" ]; then
  mp=null
else
  mp='"$dir/mnt/FLEASTICK"'
fi
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","label":null,"mountpoint":null,"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G"},
{"name":"sda","label":null,"mountpoint":null,"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",
"children":[{"name":"sda1","label":"FLEASTICK","mountpoint":\$mp,"rm":true,"size":"116.1G","type":"part","model":null}]}]}
JSON
EOS
    chmod +x "$dir/bin/lsblk"
    # Always exit 0, whatever it was asked to do: that is what the real gio did over a volume it
    # had not ejected, and it is why the verdict is never allowed to read an exit code.
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
printf '%s\n' "\$*" >> "$gio_log"
if [ "\$1 \$2" = "mount -e" ] && [ -f "$dir/really" ]; then
  : > "$dir/ejected"
fi
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/eject-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME" saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/, mnt/ and gio.log are the stubs' own fixture entries beside the two files under test.
    wait_listing 5
    for _attempt in $(seq 1 100); do
        [[ "$(ipc deviceEntries)" == *"FLEASTICK|device|volume|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"FLEASTICK|device|volume|true" ]] \
        || fail "eject: the stub volume never appeared live, got $(ipc deviceEntries)"
    # The hostname prefix makes the disk row's label the box's own, so the shape is asserted, not the text.
    [[ "$(ipc deviceEntries | grep -c '|device|disk|true')" == "1" ]] \
        || fail "eject: the stub listing did not produce one internal disk row"
    [[ "$(ipc themeLoaded)" == "true" ]] || fail "eject: the fixture home did not load a real theme"

    # Network is empty here, so the last two rail rows are the internal disk and the volume.
    local rail_count volume_row disk_row
    rail_count=$(ipc railCount)
    volume_row=$((rail_count - 1))
    disk_row=$((rail_count - 2))

    # The internal disk is in this group and reads as mounted, and it must be offered nothing:
    # an eject on it would ask gio to spin the box's own system disk down.
    click_rail_row "$disk_row" right
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "eject: the internal disk was offered a menu"

    click_rail_row "$volume_row" right
    settle
    printf 'EJECT menu visible=%s entries=%s glyphs=%s\n' \
        "$(ipc contextMenuVisible)" "$(ipc contextMenuEntries)" "$(ipc contextMenuGlyphs)"
    shot eject-menu
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "eject: right click opened no menu on the volume"
    [[ "$(ipc contextMenuEntries)" == "Eject" ]] \
        || fail "eject: the volume's menu is $(ipc contextMenuEntries), not one Eject row"
    [[ "$(ipc contextMenuGlyphs)" == "eject" ]] \
        || fail "eject: the Eject row draws $(ipc contextMenuGlyphs), not the eject mark"
    if grep -q '^mount -e' "$gio_log"; then
        fail "eject: opening the menu already ejected: $(cat "$gio_log")"
    fi

    # The negative control, and the whole point of the case: gio exits 0 and the volume is still
    # mounted, so the sentence must refuse. A verdict read off the exit code would say safe here.
    key -k Return >/dev/null
    settle
    grep -q "^mount -e $dir/mnt/FLEASTICK\$" "$gio_log" \
        || fail "eject: the menu row did not run gio mount -e on the mount point, log is: $(cat "$gio_log")"
    local refusal="FLEASTICK could not be ejected; it is still mounted, close anything using it and try again."
    local seen=""
    for _attempt in $(seq 1 250); do
        seen=$(ipc lastMessage)
        if [[ "$seen" == *"safe to unplug"* ]]; then
            fail "eject: gio exited 0 over a still-mounted volume and the status bar said: $seen"
        fi
        if [[ "$seen" == "$refusal" ]]; then
            break
        fi
        sleep 0.1
    done
    printf 'EJECT exit-zero-still-mounted message=%q\n' "$seen"
    [[ "$seen" == "$refusal" ]] || fail "eject: the refusal sentence is: $seen"

    # Now the same click with a stub that really unmounts it, and only then does the sentence change.
    : > "$dir/really"
    click_rail_row "$volume_row" right
    settle
    key -k Return >/dev/null
    wait_message "Ejected FLEASTICK, it is safe to unplug."
    printf 'EJECT really entries=%q\n' "$(ipc deviceEntries)"
    shot eject-safe

    # An unmounted volume has nothing to release, so its menu is gone with its mount.
    click_rail_row "$volume_row" right
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "eject: an unmounted volume was still offered a menu"

    # gio's own -f is offered nowhere, and the eject never went through --device, which glib
    # dispatches before it ever reads --eject: see ui/DeviceMounts.qml "eject".
    if grep -qE '(^| )(-f|--force)( |$)' "$gio_log"; then
        fail "eject: a forced unmount reached gio: $(cat "$gio_log")"
    fi
    if grep -q '^mount -d' "$gio_log"; then
        fail "eject: an eject went through --device, which glib dispatches before --eject"
    fi
    [[ "$(grep -c '^mount -e ' "$gio_log")" == "2" ]] \
        || fail "eject: expected exactly two ejects, log is: $(cat "$gio_log")"

    printf 'EJECT menu=ok internal-disk-offers-nothing=ok exit-code-is-not-the-verdict=ok listing-is=ok no-force=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
}

# Task 19: F2 renames a Network rail entry in place; "NAS" is bookmark-only, "isos" is mount-only.
case_rename() {
    local dir="$fixture_root/rename"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/0-one.txt"

    printf 'Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n' > "$dir/bin/gio-out"
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
case "\$1 \$2" in
  "mount -l") cat "$dir/bin/gio-out"; exit 0 ;;
esac
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/rename-home"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    printf 'smb://192.168.1.10/data NAS\n' > "$bookmarks"
    local real_home="$HOME" saved_path="$PATH"

    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/ is the gio stub's own fixture entry, alongside the one file under test.
    wait_listing 2
    wait_rail 3
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"NAS|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"NAS|network|share|false" ]] \
        || fail "rename: unexpected starting entries, got $(ipc networkEntries)"

    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "rename: Tab did not reach the rail"

    # Home(0), isos(1), NAS(2): two j's from Home reaches the bookmark-only entry.
    key j >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "2" ]] || fail "rename: cursor did not reach NAS, it is $(ipc railCursor)"

    # F2 starts the field pre-filled and pre-selected; typing replaces the whole label.
    key -k F2 >/dev/null
    settle
    [[ "$(ipc railRenamingIndex)" == "2" ]] || fail "rename: F2 did not start renaming, railRenamingIndex is $(ipc railRenamingIndex)"
    shot rename-editing

    # Escape cancels first, proving it before the real rename below: no write, state unwound.
    key "garbage" >/dev/null
    key -k Escape >/dev/null
    settle
    [[ "$(ipc railRenamingIndex)" == "-1" ]] || fail "rename: Escape did not close the field"
    [[ "$(cat "$bookmarks")" == "smb://192.168.1.10/data NAS" ]] \
        || fail "rename: Escape wrote to the bookmarks file, it now reads: $(cat "$bookmarks")"

    # The real rename: a line-scoped label rewrite, byte-identical apart from that one field.
    key -k F2 >/dev/null
    settle
    key "Homelab" >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc railRenamingIndex)" == "-1" ]] || fail "rename: Return did not close the field"
    [[ "$(cat "$bookmarks")" == "smb://192.168.1.10/data Homelab" ]] \
        || fail "rename: the bookmarks line was not rewritten in place, it now reads: $(cat "$bookmarks")"
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"Homelab|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"Homelab|network|share|false" ]] \
        || fail "rename: the rail did not pick up the new label, got $(ipc networkEntries)"
    shot rename-relabelled

    # An empty submitted name reverts: no write, the previous label stands. The field selects its
    # whole text on focus (see ui/SidebarRow.qml), so one Backspace clears it.
    key -k F2 >/dev/null
    settle
    key -k Backspace >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(cat "$bookmarks")" == "smb://192.168.1.10/data Homelab" ]] \
        || fail "rename: an empty submit changed the file, it now reads: $(cat "$bookmarks")"

    # isos has no bookmark line at all yet: renaming it must create one, not fail silently.
    key k >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "rename: cursor did not reach isos, it is $(ipc railCursor)"
    key -k F2 >/dev/null
    settle
    key "ISOs Archive" >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(cat "$bookmarks")" == "smb://192.168.1.10/data Homelab"$'\n'"smb://192.168.1.10/isos ISOs Archive" ]] \
        || fail "rename: a mount-only entry did not gain a bookmark line, file now reads: $(cat "$bookmarks")"
    # The rail still shows gio's own live label for a mounted entry, by design; the new
    # bookmark line only becomes visible once gio stops reporting it live.
    [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"Homelab|network|share|false" ]] \
        || fail "rename: a mounted entry's rail label changed, which should never happen, got $(ipc networkEntries)"

    # A poll that finds the same shares must hand the Repeater nothing, or every rail row rebinds on
    # a five second timer and an open editor loses what was typed into it.
    key -k F2 >/dev/null
    settle
    key "Surviving" >/dev/null
    settle
    [[ "$(ipc railRenameEditorText)" == "Surviving" ]] \
        || fail "rename: the rail editor holds $(ipc railRenameEditorText), not what was typed"
    sleep "$rail_poll_wait_s"
    [[ "$(ipc railRenameEditorText)" == "Surviving" ]] \
        || fail "rename: the mount poll emptied the rail editor, it now holds $(ipc railRenameEditorText)"
    [[ "$(ipc railRenameFieldShown)" == "true" ]] || fail "rename: the mount poll closed the rail editor"

    # A poll that finds a DIFFERENT share at that position is a different row under the editor, so
    # the rename is void: it used to stand there empty, one keystroke from relabelling the new share.
    printf 'Mount(0): photos on 192.168.1.10 -> smb://192.168.1.10/photos/\n' > "$dir/bin/gio-out"
    for _attempt in $(seq 1 $((rail_poll_wait_s * 20))); do
        [[ "$(ipc networkEntries)" == "photos|"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "photos|"* ]] \
        || fail "rename: the rail never picked up the swapped share, got $(ipc networkEntries)"
    [[ "$(ipc railRenamingIndex)" == "-1" ]] \
        || fail "rename: a swapped share left the rail editor open over it, index $(ipc railRenamingIndex)"
    [[ "$(ipc railRenameEditorLive)" == "false" ]] || fail "rename: a rail editor is live over a share nobody chose"
    # Put the stub back: the relaunch below asserts the share list this case started with.
    printf 'Mount(0): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n' > "$dir/bin/gio-out"
    printf 'RENAME poll-survives=ok swap-closes=ok\n'

    kill_flea

    # Persistence across a real relaunch: the whole point of writing to the bookmarks file at all.
    export HOME="$fixture_home"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 2
    wait_rail 3
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"Homelab|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "isos|network|share|true"$'\n'"Homelab|network|share|false" ]] \
        || fail "rename: the label did not survive a relaunch, got $(ipc networkEntries)"

    printf 'RENAME relabel=ok escape=ok empty=ok create-bookmark=ok persists=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"
}

# Task 20: the Taildrop submenu lists real tailnet peers and self-hides with nothing to send to.
case_taildrop() {
    local dir="$fixture_root/taildrop"
    sandbox_scratch "$dir"
    mkdir -p "$dir/adir" "$dir/bin"
    printf 'taildrop test payload\n' > "$dir/send-me.txt"
    # The third site of a hazard case_open and case_preview each fixed once. Without this the case
    # reached the real opener on send-me.txt and left an editor running: two were still resident
    # eighteen hours later. The stub goes in before the FIRST launch, not before the second, because
    # a stub the earlier half of the case cannot see is not a stub.
    # The log lives inside bin/, whose contents are not listed, so the row count and every click_row
    # index in this case stay exactly as they were.
    local opened="$dir/bin/opened.log"
    : > "$opened"
    # Only the open subcommand is intercepted, so stubbing the opener leaves the gio mount calls
    # ui/NetworkMounts.qml makes on every launch answering from the real gio. That name is the mount
    # tool's own and is spelled by hand here; the stub's name is derived from src/open.rs instead.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$dir/bin/$open_handoff"
    chmod +x "$dir/bin/$open_handoff"

    local first_path="$PATH"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export PATH="$first_path"
    # Directories sort first, alphabetically: adir, bin, send-me.txt.
    wait_listing 3
    click_row 0 right
    settle
    # The claim is the absence of one row, so that is what is asserted; the rest of the menu is
    # the operations design's business and grows as its own rows land.
    [[ "$(ipc contextMenuEntries)" != *"Send with Taildrop"* ]] \
        || fail "taildrop: a directory offered the entry anyway, got $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    click_row 2 right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Send with Taildrop"* ]] \
        || fail "taildrop: the real tailnet did not offer the entry on a file, got $(ipc contextMenuEntries)"
    shot taildrop-menu

    # GM's ruling: both third-party marks are ordinary cut glyphs, so the rows name them like any
    # other row rather than reaching for a component of their own.
    menu_glyph_of() {
        local want="$1" entries glyphs i=0
        entries=$(ipc contextMenuEntries)
        glyphs=$(ipc contextMenuGlyphs)
        local IFS='|'
        local -a e g
        read -r -a e <<< "$entries"
        read -r -a g <<< "$glyphs"
        for i in "${!e[@]}"; do
            [[ "${e[$i]}" == "$want" ]] && { printf '%s' "${g[$i]}"; return 0; }
        done
        printf 'no such row'
    }
    [[ "$(menu_glyph_of "Send with Taildrop")" == "tailscale" ]] \
        || fail "taildrop: the row draws $(menu_glyph_of "Send with Taildrop"), not the tailscale glyph"

    # The row is found by its label, because the menu grows as the operations design's rows land.
    # Enter opens the flyout, whose rows are whichever peers this tailnet has: the case reads the
    # one it lands on rather than naming a machine, so it does not depend on whose network it runs on.
    menu_seek "Send with Taildrop"
    key -k Return >/dev/null
    settle
    # A peer row names a machine, not the product, so it keeps the cut glyph for a machine.
    [[ "$(ipc contextMenuSubmenuGlyphs)" == *"server"* ]] \
        || fail "taildrop: the peer submenu lost the server glyph, got $(ipc contextMenuSubmenuGlyphs)"
    shot taildrop-flyout
    # Down moves to the second row, so the expected name is read off the flyout before it closes
    # rather than written here. A third peer joining or a rename then changes nothing.
    local peers second_peer
    peers=$(ipc contextMenuSubmenuEntries)
    second_peer=$(printf '%s' "$peers" | cut -d'|' -f2)
    [[ -n "$second_peer" ]] \
        || fail "taildrop: the flyout has no second peer to choose, entries are $peers"
    key -k Down >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "taildrop: choosing a peer left the menu open"
    [[ "$(ipc lastMessage)" == "Sending send-me.txt to $second_peer." ]] \
        || fail "taildrop: the dispatch message is wrong, got $(ipc lastMessage)"
    sleep 2
    shot taildrop-sent
    kill_flea

    # A stubbed tailscale, the logged-out shape (BackendState NeedsLogin, no peers at all).
    cat > "$dir/bin/tailscale" <<'EOS'
#!/bin/sh
if [ "$1 $2" = "status --json" ]; then
  printf '{"BackendState":"NeedsLogin","Peer":{}}\n'
  exit 0
fi
exit 1
EOS
    chmod +x "$dir/bin/tailscale"
    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export PATH="$saved_path"
    wait_listing 3
    click_row 2 right
    settle
    [[ "$(ipc contextMenuEntries)" != *"Send with Taildrop"* ]] \
        || fail "taildrop: a logged-out tailscale still offered the entry, got $(ipc contextMenuEntries)"
    shot taildrop-hidden

    # A stub PATH dir symlinking every real binary except tailscale, so hyprctl/gio/wtype/qs still work.
    local stub_bin="$fixture_root/taildrop-no-tailscale-bin" part f base
    sandbox_scratch "$stub_bin"
    IFS=':' read -ra _pp <<< "$PATH"
    for part in "${_pp[@]}"; do
        [[ -d "$part" ]] || continue
        for f in "$part"/*; do
            [[ -e "$f" ]] || continue
            base="${f##*/}"
            [[ "$base" == "tailscale" || -e "$stub_bin/$base" ]] && continue
            ln -s "$f" "$stub_bin/$base"
        done
    done
    kill_flea
    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    : > "$flea_log"
    PATH="$stub_bin" FLEA_PATH="$dir" FLEA_BIN="$flea_bin" \
        setsid nohup qs -p "$flea_ui" >"$flea_log" 2>&1 </dev/null &
    omarchy-drive wait window flea --timeout 15 >/dev/null
    omarchy-drive focus flea >/dev/null
    assert_window
    wait_listing 3
    click_row 2 right
    settle
    [[ "$(ipc contextMenuEntries)" != *"Send with Taildrop"* ]] \
        || fail "taildrop: a PATH with no tailscale at all still offered the entry, got $(ipc contextMenuEntries)"
    shot taildrop-absent
    grep -q 'Command: QList("tailscale", "status", "--json")' "$flea_log" \
        || fail "taildrop: no PATH miss was ever logged for tailscale, the absence was not real"
    # Expected and asserted above: scrubbed so it does not trip the suite's own generic log check.
    grep -v 'Command: QList("tailscale", "status", "--json")' "$flea_log" > "$flea_log.tmp" \
        && mv "$flea_log.tmp" "$flea_log"

    printf 'TAILDROP directory-hides=ok menu=ok real-send=ok logged-out-hides=ok absent-hides=ok\n'
    kill_flea
    sandbox_remove "$stub_bin"
}

# The rename editor's lifetime. renamingIndex used to outlive the editor it armed, and ui/js/Focus.js
# swallowed every key while it was set, so a view change, a scroll past the cache buffer or a
# navigation left the window keyboard-dead with no escape and no recovery but the mouse. Each leg
# below proves the flag is gone AND that a key moves the cursor again, because the flag reading -1
# is a claim about state and the cursor moving is the thing the operator actually lost.
case_renamelife() {
    local dir="$fixture_root/renamelife"
    sandbox_scratch "$dir"
    local i
    for i in $(seq 1 200); do printf 'x' > "$dir/$(printf 'f%03d.txt' "$i")"; done
    launch "$dir"
    wait_listing 200

    # A key press that reaches the pane moves the cursor; a swallowed one leaves it where it was.
    moved() {
        local before after
        before=$(ipc cursor)
        key j
        settle
        after=$(ipc cursor)
        [[ "$before" != "$after" ]]
    }

    open_editor() {
        key j
        settle
        key r
        settle
        [[ "$(ipc renameEditorLive)" == "true" ]] \
            || fail "renamelife: r opened no editor, renamingIndex is $(ipc renamingIndex)"
        key "HALFTYPED"
        settle
    }

    # The editor arms itself, so the field carries the row's own name rather than an empty box.
    key r
    settle
    [[ "$(ipc renameEditorText)" == "$(ipc rowAt 0 | cut -d'|' -f1)" ]] \
        || fail "renamelife: the editor opened holding '$(ipc renameEditorText)', not row 0's name"
    key -k Escape
    settle

    # Leg one: a chrome view button hides the list without moving focus.
    open_editor
    click_chrome grid
    settle
    [[ "$(ipc viewMode)" == "grid" ]] || fail "renamelife: the grid button did not change the view"
    [[ "$(ipc renamingIndex)" == "-1" ]] \
        || fail "renamelife: a view change left renamingIndex at $(ipc renamingIndex)"
    moved || fail "renamelife: the keyboard is dead in the grid after a view change during a rename"
    click_chrome list
    settle

    # Leg two: the renaming row is scrolled past the cache buffer and its delegate released.
    open_editor
    local wx wy ww wh
    read -r wx wy ww wh < <(window_box)
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    omarchy-drive scroll down "$fling_clicks" >/dev/null
    settle
    [[ "$(ipc renamingIndex)" == "-1" ]] \
        || fail "renamelife: a scroll left renamingIndex at $(ipc renamingIndex)"
    moved || fail "renamelife: the keyboard is dead after the renaming row scrolled away"

    # Leg three: the pointer navigates out of the directory the editor's row belongs to.
    key -k Home
    settle
    open_editor
    click_chrome arrow-up
    wait_path "$fixture_root"
    [[ "$(ipc renamingIndex)" == "-1" ]] \
        || fail "renamelife: a navigation left an editor open over $(ipc rowAt "$(ipc renamingIndex)")"
    [[ "$(ipc renameEditorLive)" == "false" ]] \
        || fail "renamelife: an editor is live in a directory nobody asked to rename anything in"
    moved || fail "renamelife: the keyboard is dead after navigating away from a rename"
    shot renamelife-navigated

    # A name that is nothing but spaces is refused, the way the rail has always refused one.
    launch "$dir"
    wait_listing 200
    key r
    settle
    key -k BackSpace
    for i in 1 2 3 4; do key -k Delete; done
    settle
    [[ "$(ipc renameEditorText)" == "   " ]] \
        || printf 'renamelife: the field holds %q, not three spaces\n' "$(ipc renameEditorText)"
    key "   "
    key -k Return
    settle
    [[ ! -e "$dir/   " ]] || fail "renamelife: a name of three spaces was accepted"
    [[ -e "$dir/f001.txt" ]] || fail "renamelife: f001.txt was renamed by a whitespace submit"

    printf 'RENAMELIFE arms=ok view=ok scroll=ok navigate=ok whitespace=ok\n'
    kill_flea
}

cache_snapshot
trap cleanup EXIT

declare -a wanted=("$@")
[[ ${#wanted[@]} -eq 0 ]] && wanted=(cursor terminal open openterminal click menu hidden selection select colour lifted icons thumbs hashcache stale nosweep oem header overflow focus preview network sharebrowser unmount eject rename renamelife taildrop grid columns operations tabs)

: > "$run_log"
: > "$flea_log"
failures=0
# A refusal and an assertion failure mean different things: a failure says a test is wrong, a refusal
# says the environment is unsafe and every case after it is running against that. Every case runs in
# its own subshell, so a refusal's exit cannot stop the loop; counting it separately is what makes it
# visible instead of arriving as one more FAIL among many.
refusals=0
for name in "${wanted[@]}"; do
    printf '\n== case %s ==\n' "$name"
    # The subshell stays an if-condition, because a command substitution makes its set -e live and
    # every bare fallible statement mid-case (kill, ln -s, a bsdtar whose stderr is suppressed
    # precisely because it may fail) would abort that case with no diagnostic. Output goes to a file
    # instead, which reads the same and changes no regime.
    : > "$case_log"
    if ( trap - EXIT; set -e; "case_$name" ) > "$case_log" 2>&1; then
        cat "$case_log"
        printf 'PASS %s\n' "$name"
    else
        cat "$case_log"
        printf 'FAIL %s\n' "$name"
        failures=$((failures + 1))
    fi
    # grep on the file, not through a pipe: pipefail turns grep -q's early exit into a SIGPIPE on
    # the writer and a 141 status, which reads as no match on a payload larger than the pipe buffer.
    if grep -q '^REFUSED:' "$case_log"; then
        printf 'REFUSAL %s\n' "$name"
        refusals=$((refusals + 1))
    fi
done

# launch() rolled every earlier case into the run log, so this adds the last case's share.
cat "$flea_log" >> "$run_log" 2>/dev/null || true

# The last case's backend is reaped here and not only by the trap, so a wedge lands in the tally like any other check.
if ! ( kill_flea ); then
    printf 'FAIL drain\n'
    failures=$((failures + 1))
fi

printf '\nLOG_CHECK_BEGIN %s\n' "$run_log"
if grep -E 'WARN|ERROR|TypeError|ReferenceError|Cannot open' "$run_log"; then
    printf 'FAIL log\n'
    failures=$((failures + 1))
fi
printf 'LOG_CHECK_END\n'
cache_restore
# The redirect is what makes this structural rather than a promise, so the run asserts it held.
if [[ "$(ls -A "$real_cache_large" 2>/dev/null | wc -l)" != "$real_cache_before" ]]; then
    printf 'FAIL cache\n'
    failures=$((failures + 1))
fi
printf '%s of %s checks failed\n' "$failures" "$(( ${#wanted[@]} + 3 ))"
# Loud and on its own line: a refused case means the guard stopped something, not that a test is wrong.
if [[ "$refusals" -gt 0 ]]; then
    printf '%s cases were REFUSED by the sandbox guard, so the environment is unsafe\n' "$refusals"
fi
exit "$((failures > 0))"
