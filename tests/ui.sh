#!/usr/bin/env bash
# Drives the real Quickshell window with omarchy-drive and asserts through the read-only IPC seam.
# Usage: ./tests/ui.sh [cursor|terminal|open|rows|click|menu|hidden|selection|select|colour|lifted|icons|thumbs|hashcache|stale|nosweep|oem|header|overflow|focus|preview|network|netmark|networktimeout|networklive|gvfs|sharebrowser|unmount|eject|rename|renamelife|taildrop|grid|columns|operations|tabs|openterminal|renderer|settings ...]; networklive is opt-in.
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
# Two pixels inside each edge of the strip: the rows a font-tall crumb box left dead, measured at y=2 and y=24 of 27.
chrome_band_inset=2
# Wide enough to hold the elided head's opaque fill and the hairline either side of it; that gap measured at x 80 to 86.
chrome_edge_sample_width=200
# The Hyprland corner arc shows wallpaper through the window's own top-left pixels, so start past it.
header_sample_x=16
header_sample_width=600
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

# Window class used by every preflight, focus check and injected keystroke.
flea_window_class=com.thisisgm.flea

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

# A packaged Flea uses another UI path, but still makes every title/class-based drive ambiguous.
windows_status=0
windows_json=$(omarchy-drive --json windows) || windows_status=$?
[[ "$windows_status" -eq 0 ]] \
    || fail "omarchy-drive windows failed with status $windows_status, so foreign-window state is unknown"
foreign_window_count=$(jq -er --arg class "$flea_window_class" \
    'select(.ok == true and (.windows | type == "array")) | [.windows[] | select(.class == $class)] | length' \
    <<< "$windows_json") \
    || fail "omarchy-drive windows returned an invalid payload, so foreign-window state is unknown"
if [[ "$foreign_window_count" -ne 0 ]]; then
    printf 'REFUSED %s Flea window%s already open outside this suite.\n' \
        "$foreign_window_count" "$( [[ "$foreign_window_count" -ne 1 ]] && printf s )"
    printf 'REFUSED close every Flea window before running pointer-driven tests.\n'
    exit 1
fi
unset foreign_window_count windows_json windows_status

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

# The state a case needs the window to start from, written through flea --ui-state so the schema
# sees it too, into a state home inside the fixture root: hard rule 9 covers writes, so no case here
# reaches the operator's own ~/.local/state/flea/ui.json. Exports it, because launch() below hands
# the window whatever environment the case is holding.
seed_ui_state() {
    local state="$1" patch="$2"
    sandbox_scratch "$state"
    env XDG_STATE_HOME="$state" "$flea_bin" --ui-state "$patch" >/dev/null \
        || fail "the seeding write through flea --ui-state failed for $patch"
    export XDG_STATE_HOME="$state"
}

# The shipped menu.hidden set less Open in terminal, so a case can drive that row without changing
# any other row of the menu; src/uischema.rs DEFAULTS is where the eight come from.
terminal_shown='["delete","openwith","moveto","copyto","properties","permissions","copypath"]'
# The shipped set whole, from the same DEFAULTS. A case asserting a menu's exact row list seeds this
# rather than reading whatever the operator has switched off in the Menus section.
menu_shipped='["delete","openwith","openTerminal","moveto","copyto","properties","permissions","copypath"]'

launch() {
    local start_path="$1"
    kill_flea
    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    : > "$flea_log"
    # The renderer is stated because src/gui.rs owns that choice and a direct qs launch never runs it.
    QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" FLEA_PATH="$start_path" FLEA_BIN="$flea_bin" \
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

# Focused network setup must not multiply the IPC client's own timeout by a retry count.
wait_listing_wall() {
    local want_total="$1" timeout_s="${2:-20}" total=unavailable row=loading state=unavailable
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        total=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea total 2>/dev/null || printf unavailable)
        if [[ "$want_total" == 0 ]]; then
            state=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea state 2>/dev/null || printf unavailable)
            [[ "$total" == 0 && "$state" == empty ]] && return 0
        else
            row=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea rowAt 0 2>/dev/null || printf loading)
            [[ "$total" == "$want_total" && "$row" != "loading" ]] && return 0
        fi
        sleep 0.05
    done
    fail "listing did not reach total $want_total before wall-clock deadline, got total=$total row=$row state=$state"
}

wait_path_wall() {
    local want="$1" timeout_s="${2:-20}" seen=unavailable
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea path 2>/dev/null || printf unavailable)
        [[ "$seen" == "$want" ]] && return 0
        sleep 0.05
    done
    fail "the pane did not open $want before wall-clock deadline, it is at $seen"
}

find_row_wall() {
    local want="$1" timeout_s="${2:-20}" total=0 seen="" path=unavailable row
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        total=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea total 2>/dev/null || printf 0)
        for ((row = 0; row < total && $(date +%s%3N) < deadline; row++)); do
            seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea rowAt "$row" 2>/dev/null || true)
            if [[ "$seen" == "$want|"* ]]; then
                printf '%s\n' "$row"
                return 0
            fi
        done
        sleep 0.05
    done
    [[ "$total" =~ ^[0-9]+$ ]] || total=-1
    path=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea path 2>/dev/null || printf unavailable)
    printf 'NETWORKLIVE traversal expected=%s total=%s path=%s\n' "$want" "$total" "$path" >&2
    return 1
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

# The index of a menu row by its label, for a case that has to click that row: the Menus settings
# section can change how many rows sit above it, so no case derives one from a hardcoded count.
menu_row_index() {
    local want="$1" entries i=0 label
    entries=$(ipc contextMenuEntries)
    local IFS='|'
    for label in $entries; do
        if [[ "$label" == "$want" ]]; then
            unset IFS
            printf '%s' "$i"
            return 0
        fi
        i=$((i + 1))
    done
    unset IFS
    return 1
}

# The listing row whose painted centre is nearest a y in window coordinates, or nothing when the
# point lies off every row. Used to prove a menu row really does lie over a list row before a case
# asserts that clicking it does not fall through to that row.
list_row_at_y() {
    local want_y="$1" total i centre _cx cy best="" best_gap=1000000 gap
    total=$(ipc total)
    for (( i = 0; i < total; i++ )); do
        centre=$(ipc rowCentre "$i")
        [[ -n "$centre" ]] || continue
        read -r _cx cy <<< "$centre"
        gap=$(( want_y > cy ? want_y - cy : cy - want_y ))
        if (( gap < best_gap )); then
            best_gap=$gap
            best=$i
        fi
    done
    (( best_gap <= $(ipc metrics | cut -d' ' -f4) / 2 )) && printf '%s' "$best"
}

# A right click on the listing's empty space, the background menu's own entrance. The point is
# proved to lie off every drawn row and inside the view before the click, so a case can never pass
# on a row's own menu and can never fail because it clicked the status bar instead.
click_background() {
    local cx cy wx wy ww wh landed
    read -r cx cy <<< "$(ipc listingBackgroundCentre)"
    [[ -n "$cy" ]] || fail "click_background: the listing area has no centre of its own"
    landed=$(list_row_at_y "$cy")
    [[ -z "$landed" ]] || fail "click_background: the listing area's centre lands on row $landed"
    read -r wx wy ww wh < <(window_box)
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" right >/dev/null
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

# Submits one malformed port through the real dialog and keeps enough state to test every refusal.
assert_invalid_network_port() {
    local port="$1" bookmarks="$2" snapshot="$3"
    local uri dialog error bytes seen_port visible_text

    cp "$bookmarks" "$snapshot" || return 1
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || return 1
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || return 1
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || return 1

    key "203.0.113.1" >/dev/null
    key -k Tab >/dev/null
    key -M ctrl -k a -m ctrl >/dev/null
    key "$port" >/dev/null
    settle
    seen_port=$(ipc networkPort)
    key -k Return >/dev/null
    settle
    uri=$(ipc networkUri)
    dialog=$(ipc dialogOpen)
    visible_text=$(omarchy-drive ocr flea 2>/dev/null || true)
    error=missing
    grep -Fq "Enter a valid host and port." <<< "$visible_text" && error=visible
    bytes=changed
    cmp -s "$bookmarks" "$snapshot" && bytes=unchanged
    printf 'NETWORK invalid-port=%s entered=%s uri=%s dialog=%s error=%s bookmarks=%s\n' \
        "$port" "$seen_port" "${uri:-empty}" "$dialog" "$error" "$bytes"

    [[ "$dialog" == "true" ]] && key -k Escape >/dev/null
    settle
    [[ "$seen_port" == "$port" && -z "$uri" && "$dialog" == "true" \
        && "$error" == "visible" && "$bytes" == "unchanged" ]]
}

assert_network_attempt_reset() {
    local cache="$1" process="$2" line previous=""
    while IFS= read -r line; do
        if [[ "$line" == *"$process.running = true"* ]]; then
            [[ "$previous" == *"root.$cache = \"\""* ]]
            return
        fi
        [[ -z "${line//[[:space:]]/}" ]] || previous="$line"
    done < "$repo/ui/NetworkMounts.qml"
    return 1
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

# GM's contract, measured and not recomputed: the NETWORK "+" ink, its hit target and the rail's own
# indicator dot share one x centre. The three boxes come from ui/shell.qml's boxOf, in window
# coordinates, so nothing here restates the anchoring the way an arithmetic slot did.
# The centres compare as strings and not with -eq, because -eq is integer-only and the defect this
# case exists for is half a pixel: 8 and 8.5 round to the same whole number and pass a numeric test.
assert_network_mark_alignment() {
    local label="$1" take_shot="${2:-false}" geometry glyph target dot
    local gx gw gc tx tw tc dx dw dc caption
    # GM's contract names a 24 px target; this is a floor, not a mirror of Theme.hitMin.
    local hit_target_min=24
    geometry=$(ipc networkMarkGeometry)
    [[ -n "$geometry" ]] || fail "network: the + ink, its target or the rail dot has no box at $label"
    IFS='|' read -r glyph target dot <<< "$geometry"
    read -r gx gw gc <<< "$glyph"
    read -r tx tw tc <<< "$target"
    read -r dx dw dc <<< "$dot"
    read -r _body caption _pad _rowheight <<< "$(ipc metrics)"
    [[ "$take_shot" == "true" ]] && shot network-add-alignment
    printf 'NETWORK mark %s caption=%s ink=[%s,%s)c%s target=[%s,%s)c%s dot=[%s,%s)c%s\n' \
        "$label" "$caption" "$gx" "$((gx + gw))" "$gc" \
        "$tx" "$((tx + tw))" "$tc" "$dx" "$((dx + dw))" "$dc"
    [[ "$gc" == "$dc" ]] \
        || fail "network: the + ink centre $gc differs from the rail indicator centre $dc at $label"
    [[ "$tc" == "$dc" ]] \
        || fail "network: the + hit target centre $tc differs from the rail indicator centre $dc at $label"
    [[ "$tw" -ge "$hit_target_min" ]] \
        || fail "network: the + hit target is $tw wide, under the $hit_target_min px contract at $label"
}

# A measured box is still only a claim until a click at its edge opens the dialog. Four probes: one
# just inside and one just outside each edge of the hit target, all at the target's own centre line.
probe_network_mark_target() {
    local geometry target tx tw centre cy wx wy probe x want opened
    geometry=$(ipc networkMarkGeometry)
    IFS='|' read -r _glyph target _dot <<< "$geometry"
    read -r tx tw _tc <<< "$target"
    centre=$(ipc networkMarkCentre)
    [[ -n "$centre" ]] || fail "network: the + hit target has no centre point"
    read -r _cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box)
    for probe in "$((tx + 1)) true" "$((tx + tw - 1)) true" "$((tx - 2)) false" "$((tx + tw + 1)) false"; do
        read -r x want <<< "$probe"
        omarchy-drive click "$((x + wx))" "$((cy + wy))" >/dev/null
        settle
        opened=$(ipc dialogOpen)
        printf 'NETWORK probe x=%s want-open=%s opened=%s\n' "$x" "$want" "$opened"
        if [[ "$opened" == "true" ]]; then
            key -k Escape >/dev/null
            settle
            [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: Escape did not close the probed dialog"
        fi
        [[ "$opened" == "$want" ]] \
            || fail "network: a click at x=$x reported dialogOpen=$opened against the target box [$tx,$((tx + tw)))"
    done
}

# A single IPC call may consume its own two-second timeout, so count wall time and bound each read;
# a retry count alone turned one nominal 25-second wait into more than eight minutes.

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
    local want="$1" seen="" deadline=$(( $(date +%s%3N) + 25000 ))
    while (( $(date +%s%3N) < deadline )); do
        seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea lastMessage 2>/dev/null || true)
        if [[ "$seen" == "$want" ]]; then
            return 0
        fi
        sleep 0.1
    done
    fail "the status bar never said: $want (the last thing it said was: $seen)"
}

# Durable mount state does not disappear with the status bar, so live network checks wait on it.
wait_network_result() {
    local want="$1" timeout_s="${2:-40}" seen=""
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea networkResult 2>/dev/null || true)
        [[ "$seen" == "$want" ]] && return 0
        sleep 0.1
    done
    fail "network result never became $want (last: ${seen:-unavailable})"
}

wait_network_entry_state() {
    local want="$1" timeout_s="${2:-20}" seen=""
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea networkEntries 2>/dev/null || true)
        [[ "$seen" == *"|network|share|$want" ]] && return 0
        sleep 0.1
    done
    fail "network row never became mounted=$want (last: ${seen:-unavailable})"
}

# A stub that hangs on purpose is the only thing that can say when it started hanging, so a case
# waits for its marker rather than sleeping and hoping the guard it is testing has closed.
wait_marker() {
    local marker="$1" why="$2"
    for _attempt in $(seq 1 300); do
        [[ -f "$marker" ]] && return 0
        sleep 0.05
    done
    fail "$why"
}

# Tab toggles between the list and the rail, so a case that does not know which one it is on asks.
rail_focus() {
    [[ "$(ipc focusView)" == "rail" ]] && return 0
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "Tab did not reach the rail, focus is $(ipc focusView)"
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
    settle
    shot cursor-before-scroll
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
    local header_height band changed header_x header_y
    header_height=$(ipc chromeHeight)
    # The sample is the header's real box; a row-height crop includes valid list pixels on compact chrome.
    header_x=$(ipc headerLeft)
    header_y=$(ipc headerTop)
    band="${header_sample_width}x${header_height}+$(( header_x + header_sample_x ))+${header_y}"
    changed=$(magick "$evidence_dir/cursor-before-scroll.png" \
        "$evidence_dir/cursor-after-scroll.png" -compose difference -composite -crop "$band" +repage \
        -fx '(r+g+b) > 0 ? 1.0 : 0.0' -format "%[fx:int(mean*w*h+0.5)]" info:)
    printf 'CURSOR header band=%s changed=%s\n' "$band" "$changed"
    (( changed == 0 )) || fail "scrolling changed $changed pixels in the column header"
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

# The row and menu presentation the 0.1.4 boards specify: FleaWindow.html's symlink row, its own
# trailing slash on a directory, and Menus.html's right-aligned key beside every bound row.
# Catches deleting decoratedName, sizeText's link branch or Icons.glyphForRow from ui/Row.qml, and
# the hint slot from ui/MenuRow.qml.
case_rows() {
    local dir="$fixture_root/rows"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir"
    printf 'abc' > "$dir/target.txt"
    ln -s "$dir/subdir" "$dir/linkdir"
    ln -s ../elsewhere "$dir/relative"
    # The hint slot is the Menus section's own row and ships off, and Open in terminal ships hidden,
    # so this case says outright which state it is asserting rather than reading the operator's.
    local real_state="${XDG_STATE_HOME-}"
    seed_ui_state "$fixture_root/rows-state" \
        "{\"keyHints\":true,\"menu\":{\"hidden\":$terminal_shown}}"
    launch "$dir"
    # subdir, linkdir, relative, target.txt.
    wait_listing 4
    settle
    shot rows-presentation

    # Found by name, never by a predicted sort position, the same rule seek_row_named carries.
    local i dir_row=-1 link_row=-1 rel_row=-1 file_row=-1
    for ((i = 0; i < 4; i++)); do
        case "$(ipc rowAt "$i")" in
            subdir\|*) dir_row=$i ;;
            linkdir\|*) link_row=$i ;;
            relative\|*) rel_row=$i ;;
            target.txt\|*) file_row=$i ;;
        esac
    done
    [[ "$dir_row" -ge 0 && "$link_row" -ge 0 && "$rel_row" -ge 0 && "$file_row" -ge 0 ]] \
        || fail "rows: the fixture did not list all four rows"

    printf 'ROWS dir=%q link=%q relative=%q file=%q\n' \
        "$(ipc rowNameText "$dir_row")" "$(ipc rowNameText "$link_row")" \
        "$(ipc rowNameText "$rel_row")" "$(ipc rowNameText "$file_row")"
    printf 'ROWS sizes dir=%q link=%q file=%q glyphs dir=%q link=%q\n' \
        "$(ipc rowSizeText "$dir_row")" "$(ipc rowSizeText "$link_row")" \
        "$(ipc rowSizeText "$file_row")" \
        "$(ipc rowGlyph "$dir_row")" "$(ipc rowGlyph "$link_row")"

    [[ "$(ipc rowNameText "$dir_row")" == "subdir/" ]] \
        || fail "rows: the directory reads $(ipc rowNameText "$dir_row"), not subdir/"
    [[ "$(ipc rowNameText "$file_row")" == "target.txt" ]] \
        || fail "rows: a plain file grew a suffix, it reads $(ipc rowNameText "$file_row")"
    [[ "$(ipc rowNameText "$link_row")" == "linkdir -> $dir/subdir" ]] \
        || fail "rows: the symlink reads $(ipc rowNameText "$link_row")"
    [[ "$(ipc rowNameText "$rel_row")" == "relative -> ../elsewhere" ]] \
        || fail "rows: a relative target was resolved, it reads $(ipc rowNameText "$rel_row")"
    # A link's own st_size is the length of its target path, so the column says what the row is.
    [[ "$(ipc rowSizeText "$link_row")" == "link" ]] \
        || fail "rows: the symlink's size reads $(ipc rowSizeText "$link_row"), not link"
    [[ "$(ipc rowSizeText "$file_row")" == "3 B" ]] \
        || fail "rows: a plain file's size reads $(ipc rowSizeText "$file_row")"
    # The backend resolves a link-to-directory's icon to folder, so only the mode can tell them apart.
    [[ "$(ipc rowGlyph "$link_row")" == "symlink" ]] \
        || fail "rows: the symlink draws $(ipc rowGlyph "$link_row"), not the link mark"
    [[ "$(ipc rowGlyph "$dir_row")" == "folder" ]] \
        || fail "rows: the real directory draws $(ipc rowGlyph "$dir_row"), not a folder"

    # Menus.html's hint slot: a key on every bound row, nothing on Duplicate, which nothing binds.
    seek_row_named target.txt
    key m >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "rows: m opened no menu"
    shot rows-menu-hints
    local labels hints
    labels=$(ipc contextMenuEntries)
    hints=$(ipc contextMenuHints)
    printf 'ROWS menu labels=%q\n hints=%q\n' "$labels" "$hints"
    hint_of() {
        local want="$1" i=0 label
        local IFS='|'
        for label in $labels; do
            if [[ "$label" == "$want" ]]; then
                unset IFS
                printf '%s' "$(printf '%s' "$hints" | cut -d'|' -f$((i + 1)))"
                return 0
            fi
            i=$((i + 1))
        done
        unset IFS
        fail "rows: no menu row labelled $want in $labels"
    }
    [[ "$(hint_of Open)" == "enter" ]] || fail "rows: Open prints $(hint_of Open), not enter"
    [[ "$(hint_of Cut)" == "x" ]] || fail "rows: Cut prints $(hint_of Cut), not x"
    [[ "$(hint_of Copy)" == "y" ]] || fail "rows: Copy prints $(hint_of Copy), not y"
    [[ "$(hint_of Paste)" == "p" ]] || fail "rows: Paste prints $(hint_of Paste), not p"
    [[ "$(hint_of Rename)" == "r" ]] || fail "rows: Rename prints $(hint_of Rename), not r"
    [[ "$(hint_of 'Move to Trash')" == "d" ]] \
        || fail "rows: Move to Trash prints $(hint_of 'Move to Trash'), not d"
    [[ "$(hint_of 'Show hidden files')" == "." ]] \
        || fail "rows: the hidden toggle prints $(hint_of 'Show hidden files'), not ."
    # The unbound row, and the whole point of the slot being derived rather than written by hand.
    [[ -z "$(hint_of Duplicate)" ]] || fail "rows: Duplicate printed $(hint_of Duplicate), and nothing binds it"
    [[ -z "$(hint_of 'Open in terminal')" ]] \
        || fail "rows: Open in terminal printed a bare key, and only a chord reaches it"
    key -k Escape >/dev/null
    settle

    # The other half of the same row: with hints off, which is what ships, the slot draws nothing at
    # all. The labels are unchanged, so this is the hint column leaving and not the menu changing.
    seed_ui_state "$fixture_root/rows-state" \
        "{\"keyHints\":false,\"menu\":{\"hidden\":$terminal_shown}}"
    launch "$dir"
    wait_listing 4
    seek_row_named target.txt
    key m >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "rows: m opened no menu with hints off"
    shot rows-menu-nohints
    local off_labels off_hints
    off_labels=$(ipc contextMenuEntries)
    off_hints=$(ipc contextMenuHints)
    printf 'ROWS hints-off labels=%q\n hints=%q\n' "$off_labels" "$off_hints"
    [[ "$off_labels" == "$labels" ]] \
        || fail "rows: switching the hints off changed the menu to $off_labels"
    [[ -z "${off_hints//|/}" ]] || fail "rows: the hints row is off and the slot still prints $off_hints"
    key -k Escape >/dev/null
    settle
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
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
# terminalFailed and --terminal appeared nowhere in this file, so the menu row, its dispatch through
# ui/js/Focus.js act() and the chord's interception there were all deletable with the display suite
# still green.
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

    # Open in terminal ships switched off in the Menus section, so the menu half below says which
    # state it is driving instead of reading whatever the operator's own ui.json holds.
    local real_state="${XDG_STATE_HOME-}"
    seed_ui_state "$fixture_root/openterminal-state" "{\"menu\":{\"hidden\":$terminal_shown}}"
    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    flea_bin="$dir/bin/flea"
    launch "$dir"
    export PATH="$saved_path"
    flea_bin="$real_bin"
    # bin, opened.log, ran.log, target.txt.
    wait_listing 4

    # The pointer half: the context-menu row, which is the route SettingsMenus.html specifies and
    # the only one left now that the chrome carries no terminal button.
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuEntries)" == *"Open in terminal"* ]] \
        || fail "openterminal: the menu offers no terminal row, got $(ipc contextMenuEntries)"
    menu_seek "Open in terminal"
    key -k Return >/dev/null
    wait_terminal "$ran" "$dir" "the context menu"
    printf 'OPENTERMINAL menu log=%q\n' "$(cat "$ran")"

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
    # The drop is announced, because a swallowed keypress with nothing on screen is the defect.
    : > "$ran"
    hotkey --global ctrl t flea >/dev/null
    settle
    hotkey --global ctrl t flea >/dev/null
    wait_message "Still opening the last terminal; try again in a moment."
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

    printf 'OPENTERMINAL menu=ok list=ok rail=ok single-flight=ok crossed=ok failure=ok\n'
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
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

    # The elided head is an opaque fill drawn over crumbs that have slid underneath it, so a press
    # there used to open whichever one was behind it: a directory the operator could not see.
    # Twelve of these overflow the strip on this box's own 2560 wide monitor; six did not, measured.
    local seg="a-directory-with-a-deliberately-long-name" deep="$dir" _level
    for _level in $(seq 1 12); do deep="$deep/$seg"; done
    mkdir -p "$deep"
    : > "$deep/leaf.txt"
    launch "$deep"
    wait_listing 1
    local marker ex ey wx wy
    marker=$(ipc elisionCentre)
    [[ -n "$marker" ]] || fail "click: the path fits the bar here, so the elision marker is not under test at all"
    read -r ex ey <<< "$marker"
    read -r wx wy _ww _wh < <(window_box)
    # The status is read, because a click that never reached the compositor leaves the path
    # unchanged too and would satisfy both assertions below without pressing anything.
    omarchy-drive click "$((ex + wx))" "$((ey + wy))" >/dev/null \
        || fail "click: omarchy-drive refused the press on the elision marker"
    # A crumb's single tap is deferred until the double-tap interval expires, see ui/ChromeBar.qml
    # "exclusiveSignals", so the reading is taken well after the click rather than on top of it.
    settle
    settle
    printf 'CLICK elision path=%q barOpen=%s\n' "$(ipc path)" "$(ipc pathBarOpen)"
    shot click-elision
    [[ "$(ipc path)" == "$deep" ]] || fail "click: a tap on the elision marker navigated to $(ipc path)"
    [[ "$(ipc pathBarOpen)" == "false" ]] || fail "click: a tap on the elision marker opened the path bar"

    # keys.toml's chrome/left x2/any row is the whole strip, and the chrome is the window's own top item, so its band is y 0 to chromeHeight - 1.
    local chrome_h band_crumb band_x band
    chrome_h=$(ipc chromeHeight)
    band_crumb=$(( $(ipc crumbCount) - 2 ))
    read -r band_x _band_y <<< "$(ipc crumbCentre "$band_crumb")"
    [[ -n "$band_x" ]] || fail "click: crumb $band_crumb has no on-screen centre, so no band of the strip can be pressed over one"
    for band in "$chrome_band_inset" "$(( chrome_h - 1 - chrome_band_inset ))"; do
        omarchy-drive click "$((band_x + wx))" "$((band + wy))" --double >/dev/null \
            || fail "click: omarchy-drive refused the double click at y $band of the strip"
        settle
        settle
        printf 'CLICK chrome-band y=%s of %s barOpen=%s path=%q\n' "$band" "$chrome_h" "$(ipc pathBarOpen)" "$(ipc path)"
        shot "click-chrome-band-$band"
        [[ "$(ipc pathBarOpen)" == "true" ]] \
            || fail "click: a double click at y $band of the ${chrome_h}px strip did not open the path bar"
        [[ "$(ipc path)" == "$deep" ]] || fail "click: the double click at y $band navigated to $(ipc path)"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc pathBarOpen)" == "false" ]] || fail "click: Escape did not close the path bar opened at y $band"
    done

    # The strip's own bottom edge is one flat rule, so that row holds one colour until something opaque draws over it.
    local edge_y edge_colours
    edge_y=$(( chrome_h - 1 ))
    shot click-chrome-edge
    edge_colours=$(magick "$evidence_dir/click-chrome-edge.png" \
        -crop "${chrome_edge_sample_width}x1+0+${edge_y}" +repage -unique-colors -format "%[fx:w]" info:)
    printf 'CLICK chrome-edge y=%s width=%s colours=%s\n' "$edge_y" "$chrome_edge_sample_width" "$edge_colours"
    [[ "$edge_colours" == "1" ]] \
        || fail "click: the strip's bottom edge holds $edge_colours colours across ${chrome_edge_sample_width}px, so something drew over it"

    # Issue 45's own control, and the positive half the elision check needs: with only the negative
    # above, a click that missed the window entirely passed it. Nothing drove a crumb at all, so a
    # ChromeBar that ignored every press passed the whole suite.
    local crumbs target centre cx cy up
    up=$(dirname "$deep")
    crumbs=$(ipc crumbCount)
    (( crumbs >= 3 )) || fail "click: the bar drew $crumbs crumbs, too few to press a parent"
    # keys.toml declares the press on a parent and not on the leaf, which is the directory already
    # listed; the segment before the leaf is the one this fixture is standing in.
    target=$((crumbs - 2))
    centre=$(ipc crumbCentre "$target")
    [[ -n "$centre" ]] || fail "click: crumb $target has no on-screen centre"
    read -r cx cy <<< "$centre"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null \
        || fail "click: omarchy-drive refused the press on crumb $target"
    settle
    settle
    printf 'CLICK crumb=%s path=%q barOpen=%s\n' "$target" "$(ipc path)" "$(ipc pathBarOpen)"
    shot click-crumb
    [[ "$(ipc path)" == "$up" ]] || fail "click: a tap on the parent crumb went to $(ipc path), not $up"
    [[ "$(ipc pathBarOpen)" == "false" ]] || fail "click: a single tap on a crumb opened the path bar"

    # Issue 20's own control, the other row this table declares and nothing pressed. omarchy-drive
    # click knows left, right and middle only, so the extra button goes through ydotool directly;
    # 0xC3 is its down|up for button 3, which Hyprland delivers as Qt.BackButton on this box.
    command -v ydotool >/dev/null || fail "click: ydotool is missing, so the mouse back button cannot be pressed"
    # The same default omarchy-drive exports, because calling ydotool directly skips that wrapper.
    export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-$XDG_RUNTIME_DIR/.ydotool_socket}"
    [[ -S "$YDOTOOL_SOCKET" ]] || fail "click: no ydotoold socket at $YDOTOOL_SOCKET"
    local rx ry
    read -r rx ry <<< "$(ipc rowCentre 0)"
    omarchy-drive move "$((rx + wx))" "$((ry + wy))" >/dev/null \
        || fail "click: the pointer could not be parked over the listing"
    ydotool click 0xC3 >/dev/null 2>&1 || fail "click: ydotool refused the mouse back button"
    settle
    settle
    printf 'CLICK back path=%q\n' "$(ipc path)"
    shot click-back
    [[ "$(ipc path)" == "$deep" ]] || fail "click: the back button went to $(ipc path), not back to $deep"
    # The same button's other half: with the history spent it climbs, which is the whole of
    # ui/js/Nav.js mouseBack and the half a stub calling mouseBack directly cannot prove is bound.
    ydotool click 0xC3 >/dev/null 2>&1 || fail "click: ydotool refused the second back button press"
    settle
    settle
    printf 'CLICK back-climb path=%q\n' "$(ipc path)"
    [[ "$(ipc path)" == "$up" ]] || fail "click: the back button with no history left went to $(ipc path), not $up"
    kill_flea
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
    # Five more rows than this case used to need: with the six basic actions drawn, Rename is the
    # eighth menu row and no list row lay under it on a five-row listing, which the check below says
    # out loud rather than passing on a click that hit nothing.
    : > "$dir/z1.txt"
    : > "$dir/z2.txt"
    : > "$dir/z3.txt"
    : > "$dir/z4.txt"
    : > "$dir/z5.txt"
    launch "$dir"
    wait_listing 10
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

    # Rename's click belongs only to the menu, never to the list row it lies over. Both the row's own
    # index and the list row beneath it are found live: the Menus settings section can move Rename.
    click_row 0 right
    settle
    local rename_index rename_centre rx ry beneath
    rename_index=$(menu_row_index "Rename") || fail "menu: the open menu has no Rename row"
    rename_centre=$(ipc contextMenuRowCentre "$rename_index")
    read -r rx ry <<< "$rename_centre"
    [[ -n "$ry" ]] || fail "menu: the Rename row has no on-screen centre"
    beneath=$(list_row_at_y "$ry")
    [[ -n "$beneath" ]] || fail "menu: no list row lies under Rename, so a pass-through cannot happen"
    omarchy-drive click "$((wx + rx))" "$((wy + ry))" left >/dev/null
    settle
    printf 'MENU rename-over-row index=%s cursor=%s renaming=%s live=%s text=%q beneath=%s\n' \
        "$rename_index" "$(ipc cursor)" "$(ipc renamingIndex)" "$(ipc renameEditorLive)" \
        "$(ipc renameEditorText)" "$(ipc rowAt "$beneath" | cut -d'|' -f1)"
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

# Menus.html's background column, on a right click that landed on no row. Every row it draws is
# exercised here except Settings, which is the board's third door and is driven where the other two
# are, in settings_doors. New File is the one row on the board this release has no backend command
# for at all: the backend has mkdir and nothing that creates an empty file, so the row is not built.
case_background() {
    local dir="$fixture_root/background"
    sandbox_scratch "$dir"
    mkdir -p "$dir/dest"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    : > "$dir/c.txt"
    # Every row list below is the shipped column, and ui/js/Menu.js applyHidden builds it from
    # menu.hidden, which is operator state: without this seed the assertions read the operator's own
    # Menus section and fail on a box that has switched any of the eight back on.
    local real_state="${XDG_STATE_HOME-}"
    seed_ui_state "$fixture_root/background-state" "{\"menu\":{\"hidden\":$menu_shipped}}"
    launch "$dir"
    wait_listing 4

    click_background
    settle
    printf 'BACKGROUND visible=%s entries=%s glyphs=%s hints=%q\n' \
        "$(ipc contextMenuVisible)" "$(ipc contextMenuEntries)" \
        "$(ipc contextMenuGlyphs)" "$(ipc contextMenuHints)"
    shot background-menu
    printf 'BACKGROUND_OCR_BEGIN\n'
    omarchy-drive ocr flea || true
    printf 'BACKGROUND_OCR_END\n'
    [[ "$(ipc contextMenuVisible)" == "true" ]] \
        || fail "background: a right click on empty space opened no menu"
    [[ "$(ipc contextMenuEntries)" == "New folder|-|Paste|Select all|-|Sort by|Show hidden files|-|Settings" ]] \
        || fail "background: the menu is not the board's column, it is $(ipc contextMenuEntries)"
    [[ "$(ipc contextMenuGlyphs)" == "folder-plus|-|clipboard|check|-|sort|eye|-|sliders" ]] \
        || fail "background: a row lost its mark, the set is $(ipc contextMenuGlyphs)"
    # A right click ON a row still gets that row's own menu: the two entrances share one instance,
    # so a hasRow left standing from the last open would be the defect this asserts against.
    key -k Escape >/dev/null
    settle
    click_row 1 right
    settle
    [[ "$(ipc contextMenuEntries)" == Open\|* ]] \
        || fail "background: a row's own menu came back as $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    # Sort by, the one submenu row. Its flyout is the three orders the backend can produce, and the
    # order the listing lands in is read off the header's own mark, not off a row's contents.
    click_background
    settle
    menu_seek "Sort by"
    key -k Return >/dev/null
    settle
    printf 'BACKGROUND sort from=%s flyout=%s glyphs=%s\n' \
        "$(ipc sortMark)" "$(ipc contextMenuSubmenuEntries)" "$(ipc contextMenuSubmenuGlyphs)"
    shot background-sort
    [[ "$(ipc contextMenuSubmenuEntries)" == "Name|Size|Date Modified" ]] \
        || fail "background: the Sort by flyout is $(ipc contextMenuSubmenuEntries)"
    [[ "$(ipc contextMenuSubmenuGlyphs)" == "sort|sort|sort" ]] \
        || fail "background: the sort flyout drew $(ipc contextMenuSubmenuGlyphs)"
    key -k Down >/dev/null
    key -k Return >/dev/null
    settle
    printf 'BACKGROUND sorted to=%s\n' "$(ipc sortMark)"
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "background: choosing an order left the menu open"
    [[ "$(ipc sortMark)" == "size:asc" ]] \
        || fail "background: Sort by Size left the listing in $(ipc sortMark)"
    # And back to the order this case found, which the steps below read row numbers against. It is a
    # literal and not a saved reading because ui/Backend.qml sets sortBy to name and sortDesc to
    # false on every list and nothing writes the order to ui.json, so name ascending is what every
    # window starts in; the flip above lives and dies with this window.
    menu_click "Sort by" name
    [[ "$(ipc sortMark)" == "name:asc" ]] \
        || fail "background: Sort by Name left the listing in $(ipc sortMark)"

    # Select all, clicked rather than keyed: a row only the keyboard can reach is not the row the
    # board drew. Escape then hands the listing back the empty selection the steps below want.
    menu_click "Select all"
    printf 'BACKGROUND selectall count=%s total=%s\n' "$(ipc selectionCount)" "$(ipc total)"
    [[ "$(ipc selectionCount)" == "$(ipc total)" ]] \
        || fail "background: Select all selected $(ipc selectionCount) of $(ipc total)"
    key -k Escape >/dev/null
    settle

    # Paste, with something really on the clipboard and a destination of its own, so the row is
    # exercised doing work rather than only answering the empty-clipboard sentence. Directories sort
    # first, so under name ascending row 0 is dest and row 1 is a.txt.
    click_row 1 left
    settle
    key y >/dev/null
    settle
    click_row 0 left
    settle
    key -k Return >/dev/null
    # wait_listing cannot answer for an empty directory: rowAt 0 has no delegate to describe and
    # says "loading" forever, so the empty state's own flag is what says the listing arrived.
    for _attempt in $(seq 1 300); do
        [[ "$(ipc emptyShown)" == "true" && "$(ipc path)" == "$dir/dest" ]] && break
        sleep 0.05
    done
    [[ "$(ipc path)" == "$dir/dest" ]] || fail "background: the case is in $(ipc path), not $dir/dest"
    [[ "$(ipc total)" == "0" ]] || fail "background: $dir/dest listed $(ipc total) rows, not 0"
    # The empty listing is also the strongest case for this menu, and it has no row to aim from.
    click_background
    settle
    [[ "$(ipc contextMenuEntries)" == "New folder|-|Paste|Select all|-|Sort by|Show hidden files|-|Settings" ]] \
        || fail "background: an empty directory drew $(ipc contextMenuEntries)"
    shot background-empty
    key -k Escape >/dev/null
    settle
    menu_click "Paste"
    settle
    printf 'BACKGROUND paste landed=%s message=%q\n' \
        "$([[ -f "$dir/dest/a.txt" ]] && echo yes || echo no)" "$(ipc lastMessage)"
    [[ -f "$dir/dest/a.txt" ]] || fail "background: Paste put nothing in $dir/dest"

    # New folder, the only background row that writes on its own, so the directory is the proof.
    [[ ! -e "$dir/dest/New Folder" ]] || fail "background: New Folder existed before the row ran"
    menu_click "New folder"
    settle
    printf 'BACKGROUND newfolder made=%s message=%q\n' \
        "$([[ -d "$dir/dest/New Folder" ]] && echo yes || echo no)" "$(ipc lastMessage)"
    [[ -d "$dir/dest/New Folder" ]] || fail "background: New folder created nothing in $dir/dest"

    # Show hidden files, the row the Menus board locks, read off the state it flips and flipped back
    # through its own changed label so the case leaves the listing as it found it.
    local was
    was=$(ipc showHidden)
    menu_click "Show hidden files"
    printf 'BACKGROUND hidden %s -> %s\n' "$was" "$(ipc showHidden)"
    [[ "$(ipc showHidden)" != "$was" ]] || fail "background: the hidden toggle stayed $was"
    menu_click "Hide hidden files"
    [[ "$(ipc showHidden)" == "$was" ]] || fail "background: the hidden toggle did not flip back"

    # The grid and the columns view carry the same entrance, because the board draws one menu and
    # not a list-view menu: a right click on empty space means the same thing in all three.
    local view
    for view in grid columns; do
        key -M ctrl -k "$([[ "$view" == grid ]] && echo 3 || echo 2)" -m ctrl >/dev/null
        settle
        [[ "$(ipc viewMode)" == "$view" ]] || fail "background: the $view view did not come up"
        click_background
        settle
        printf 'BACKGROUND %s entries=%s\n' "$view" "$(ipc contextMenuEntries)"
        shot "background-$view"
        [[ "$(ipc contextMenuEntries)" == "New folder|-|Paste|Select all|-|Sort by|Show hidden files|-|Settings" ]] \
            || fail "background: the $view view drew $(ipc contextMenuEntries)"
        key -k Escape >/dev/null
        settle
    done
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
}

# Opens the background menu and clicks one of its rows by label, optionally stepping into that row's
# flyout and choosing the entry named second. The row's own centre is read off the drawn frame, so
# no case derives a pixel from a row count the Menus section can change under it.
menu_click() {
    local want="$1" sub="${2-}" index cx cy wx wy ww wh
    click_background
    settle
    index=$(menu_row_index "$want") || fail "menu_click: the background menu has no $want row"
    read -r cx cy <<< "$(ipc contextMenuRowCentre "$index")"
    [[ -n "$cy" ]] || fail "menu_click: the $want row has no on-screen centre"
    read -r wx wy ww wh < <(window_box)
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    settle
    [[ -z "$sub" ]] && return 0
    local i=0 entry found=no entries
    entries=$(ipc contextMenuSubmenuEntries)
    local IFS='|'
    for entry in $entries; do
        [[ "${entry,,}" == "${sub,,}" ]] && { found=yes; break; }
        i=$((i + 1))
    done
    unset IFS
    [[ "$found" == yes ]] || fail "menu_click: the $want flyout has no $sub entry, it holds $entries"
    for _ in $(seq 1 "$i"); do key -k Down >/dev/null; done
    key -k Return >/dev/null
    settle
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
    # The renderer is stated because src/gui.rs owns that choice and a direct qs launch never runs it.
    QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" FLEA_PATH="$dir" FLEA_SELECT="$dir/b.txt" FLEA_BIN="$flea_bin" \
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
    # The renderer is stated because src/gui.rs owns that choice and a direct qs launch never runs it.
    QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" FLEA_PATH="$dir" FLEA_SELECT="$dir/does-not-exist.txt" FLEA_BIN="$flea_bin" \
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
    local fg expected_fg
    fg=$(ipc themeForeground)
    expected_fg=$(theme_key foreground)
    [[ "${fg,,}" == "${expected_fg,,}" ]] \
        || fail "oem: Flea's foreground is $fg, the theme's is $expected_fg"
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
    # FleaWindow.html and GridView.html both draw a symlink with the link mark, whatever it points at,
    # so the mark follows the mode here while the backend's i still follows the target; the backend
    # side of that split is tests/protocol.sh "and draws as a folder".
    [[ "$(glyph_of linkdir)" == "symlink" ]] || fail "the symlink to a directory is not the link glyph: $(glyph_of linkdir)"
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

# The one scene-graph failure found to be raisable here: Qt's GL backend with no EGL vendor file to load.
case_renderer() {
    kill_flea
    local dir="$fixture_root/renderer"
    sandbox_scratch "$dir"
    local log="$dir/shell.log"
    local relaunched="$dir/relaunch.log"
    : > "$relaunched"
    printf '#!/bin/sh\nprintf "RAN %%s\\n" "$*" >> %q\n' "$relaunched" > "$dir/flea-stub"
    chmod +x "$dir/flea-stub"
    # The marker is set, so the renderer's own name is the only thing standing between this and a retry.
    env QSG_RHI_BACKEND=opengl FLEA_RENDERER_AUTOMATIC=1 \
        __EGL_VENDOR_LIBRARY_FILENAMES="$dir/no-such-egl-vendor.json" \
        FLEA_PATH="$dir" FLEA_BIN="$dir/flea-stub" \
        setsid nohup qs -p "$flea_ui" > "$log" 2>&1 </dev/null &
    local waited
    for waited in $(seq 1 200); do
        grep -aq 'graphics backend opengl failed' "$log" && break
        sleep 0.1
    done
    kill_flea
    printf 'RENDERER ran=%q\n' "$(tr '\n' ' ' < "$relaunched")"
    grep -aq 'graphics backend opengl failed' "$log" \
        || fail "no scene-graph error reached ui/shell.qml, so its Connections never held the window"
    # The denominator: with no stub run at all, the count of retries below would be zero for free.
    local ran retried
    ran=$(grep -c -- '--backend' "$relaunched" || true)
    [[ "$ran" != "0" ]] || fail "the stub Flea was never run, so no retry could have been recorded either"
    retried=$(grep -c -- '--gui' "$relaunched" || true)
    [[ "$retried" == "0" ]] || fail "the retry fired for a renderer the operator named: $(cat "$relaunched")"
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
    # Task 22's own Right/Left checks need headroom on both sides of a 5 s seek (the SEEK_MS in
    # ui/js/PreviewKeys.js) starting from whatever position the round trips above already spent:
    # at three seconds the clip had finished before the checks ran at all, and at eight, Right alone
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

    # Left/Right seek 5 s (PreviewKeys.js's SEEK_MS), read before the pause/resume dance below spends
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
    assert_network_attempt_reset _infoOutput infoProcess \
        || fail "network: info output is not cleared immediately before infoProcess starts"
    assert_network_attempt_reset _listSharesOutput listSharesProcess \
        || fail "network: share output is not cleared immediately before listSharesProcess starts"
    local dir="$fixture_root/network"
    sandbox_scratch "$dir"
    local fake_root="$fixture_root/network-fake"
    sandbox_scratch "$fake_root"
    mkdir -p "$fake_root/bin"
    # Three rows so a cursor that did not move is distinguishable from one clamped to a short listing.
    : > "$dir/0-one.txt"
    : > "$dir/0-two.txt"
    : > "$dir/apple.txt"
    local fixture_home="$fixture_root/network-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME" product_root mount_log="$fake_root/mount.log"
    : > "$mount_log"

    local live_mounts
    live_mounts=$(gio mount -l 2>/dev/null | grep -c '^Mount(') || true
    [[ "$live_mounts" -eq 0 ]] \
        || fail "network: $live_mounts real gio mount(s) already present, cannot assert an empty rail against ambient state: $(gio mount -l 2>/dev/null)"

    # This case proves form/bookmark behavior, not a network route; a bounded local gio double keeps
    # the newly functional Save action from dialing TEST-NET-2 or reopening on its later timeout.
    cat > "$fake_root/bin/gio" <<EOS
#!/bin/sh
case "\$1 \${2:-}" in
"mount -l") exit 0 ;;
"mount nfs://stale-one.test/export") printf 'Location is already mounted\n' >&2; exit 2 ;;
"mount nfs://stale-two.test/export") exit 2 ;;
"mount "*) printf '%s\n' "\$*" > "$mount_log" ;;
"info nfs://stale-two.test/export") exit 1 ;;
"info smb://shares-one.test/"|"info smb://shares-two.test/") exit 1 ;;
"info "*) printf 'local path: %s\n' "$dir" ;;
"list smb://shares-one.test/") printf 'old-share\n' ;;
"list smb://shares-two.test/") exit 0 ;;
*) exit 0 ;;
esac
EOS
    chmod +x "$fake_root/bin/gio"
    local saved_path="$PATH"
    export PATH="$fake_root/bin:$PATH"

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
    [[ "$(ipc networkUri)" == "smb://198.51.100.1/" ]] \
        || fail "network: the Mounts-as line reads $(ipc networkUri)"
    key -k Return >/dev/null
    wait_network_result mounted 5
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: Enter did not submit and close the dialog"
    [[ "$(cat "$mount_log")" == 'mount --anonymous smb://198.51.100.1/' ]] \
        || fail "network: guest SMB did not use gio mount --anonymous"
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

    # The mark's own case walks the text sizes and probes the target; here it is read once, on the
    # rail this case just filled, so a regression shows up in the case that produced the row.
    settle
    assert_network_mark_alignment "the live text size" true

    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    local invalid_port invalid_port_failures=0 snapshot
    for invalid_port in "22/path" "0" "65536"; do
        snapshot="$fixture_root/network-bookmarks-${invalid_port//\//-}"
        if ! assert_invalid_network_port "$invalid_port" "$bookmarks" "$snapshot"; then
            invalid_port_failures=$((invalid_port_failures + 1))
        fi
    done
    [[ "$invalid_port_failures" -eq 0 ]] \
        || fail "network: $invalid_port_failures invalid-port submit checks failed"

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
    [[ "$(ipc networkUri)" == "smb://dd;uu@hh/ss" ]] \
        || fail "network: Tab did not walk host, port, share, domain, username in order, URI is $(ipc networkUri)"

    # Reached by keyboard rather than by click, which is the only thing that exercises a chip's own
    # Enter. Under SMB, Username steps through Password before wrapping to the first chip; the third
    # Tab lands on SFTP, proving the new secret row participates in the visible ring.
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh/ss" ]] \
        || fail "network: Enter on a tabbed-to chip did not pick SFTP, URI is $(ipc networkUri)"

    # SFTP hides Domain and TLS. Backtab crosses SMB, Password and Username; the fourth must skip
    # hidden Domain and land on Path.
    key -k Backtab >/dev/null
    key -k Backtab >/dev/null
    key -k Backtab >/dev/null
    key -k Backtab >/dev/null
    key "XX" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh/ssXX" ]] \
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
    [[ "$(ipc networkUri)" == "smb://hh/ss" ]] \
        || fail "network: the re-home walk did not reach Domain, URI is $(ipc networkUri)"
    click_chip SFTP
    settle
    [[ "$(ipc networkProtocol)" == "SFTP" ]] \
        || fail "network: the chip click did not pick SFTP, it is $(ipc networkProtocol)"
    key "uu" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh/ss" ]] \
        || fail "network: a chip click left the caret in the hidden Domain, URI is $(ipc networkUri)"
    printf 'NETWORK rehome=ok\n'
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] \
        || fail "network: escape did not close the dialog after the re-home walk"

    # Remove on the place this session's own dialog added, which is the sequence the rail's own
    # write used to refuse forever: ui/NetworkDialog.qml appends through its own FileView, and
    # ui/NetworkPlaces.qml's never reloads, so the body it wrote back was the pre-Add snapshot. The
    # write is derived from bookmarksText now, the text ui/shell.qml:158 has the rail reload on saved().
    click_rail_row 1 right
    settle
    [[ "$(ipc contextMenuEntries)" == "Rename|Remove" ]] \
        || fail "network: the added place offers $(ipc contextMenuEntries), not Rename then Remove"
    menu_seek Remove
    key -k Return >/dev/null
    wait_message "198.51.100.1 is forgotten."
    # cat and stat both print nothing for a path that is gone, so existence is asserted separately:
    # an empty read alone cannot tell a correct removal from a forget that unlinked the file. The
    # sentence above is what separates a correct removal from the stale write-back this case exists
    # for, because that one refused with "is not a saved place" instead.
    [[ -f "$fixture_home/.config/gtk-3.0/bookmarks" ]] \
        || fail "network: Remove unlinked the bookmarks file instead of rewriting it"
    [[ -z "$(cat "$fixture_home/.config/gtk-3.0/bookmarks")" ]] \
        || fail "network: Remove wrote a body older than the rail, the file reads: $(cat "$fixture_home/.config/gtk-3.0/bookmarks")"
    for _attempt in $(seq 1 100); do
        [[ -z "$(ipc networkEntries)" ]] && break
        sleep 0.05
    done
    [[ -z "$(ipc networkEntries)" ]] \
        || fail "network: the forgotten place stayed on the rail, got $(ipc networkEntries)"
    printf 'NETWORK add-then-remove=ok\n'

    # The other half of the same fix, and the half nothing drove: rename() derives its body from
    # bookmarksText too. The Remove above left ui/NetworkPlaces.qml's write view holding "", the
    # dialog appends through a view of its own, so a rename taken from the write view would write
    # one of the two lines below and drop the other. Focus is still on the rail after the menu.
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    local host
    for host in 198.51.100.2 198.51.100.3; do
        # The dialog hands focus back to the list when it closes, so the rail is reached explicitly
        # rather than assumed: "a" is bound on the rail alone.
        rail_focus
        key a >/dev/null
        settle
        [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: a from the rail did not reopen the dialog for $host"
        key "$host" >/dev/null
        key -k Return >/dev/null
        settle
    done
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "198.51.100.2|network|share|false"$'\n'"198.51.100.3|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "198.51.100.2|network|share|false"$'\n'"198.51.100.3|network|share|false" ]] \
        || fail "network: the two added places did not reach the rail, got $(ipc networkEntries)"
    rail_focus
    key g >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "network: cursor did not reach the first added place, it is $(ipc railCursor)"
    key -k F2 >/dev/null
    settle
    key "Second" >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(grep -c . "$bookmarks")" == "2" ]] \
        || fail "network: the rename changed the line count, the file reads: $(cat "$bookmarks")"
    # ui/RenameField.qml preselects the stem alone, and this label's last dot reads as an extension,
    # so typing over it keeps the ".2": the name written is the name the operator would have seen.
    grep -q ' Second\.2$' "$bookmarks" \
        || fail "network: the rename did not write the name just typed, the file reads: $(cat "$bookmarks")"
    grep -q '198.51.100.3' "$bookmarks" \
        || fail "network: the rename dropped the line the second Add wrote, the file reads: $(cat "$bookmarks")"
    printf 'NETWORK rename-after-add=ok\n'

    # The append re-reads this file before it writes, and a read that failed empties FileView.text():
    # the write that followed left a bookmarks file holding one line and destroyed the rest. Mode 200
    # is the exact shape, unreadable and still writable, because taking write away too would hide the
    # defect behind a second failure. The mode is restored before the tick arm below reads
    # anything. See AGENTS.md "A failed FileView read".
    local before
    before=$(cat "$bookmarks")
    [[ -n "$before" ]] || fail "network: the unreadable-file arm needs saved places to lose, and the file is empty"
    chmod 200 "$bookmarks"
    rail_focus
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: a from the rail did not reopen the dialog for the unreadable-file arm"
    key "198.51.100.4" >/dev/null
    key -k Return >/dev/null
    settle
    chmod 600 "$bookmarks"
    [[ "$(cat "$bookmarks")" == "$before" ]] \
        || fail "network: a read that failed still wrote, and the file now reads: $(cat "$bookmarks")"
    # And the refusal is visible rather than silent: the dialog stays open over its own sentence.
    [[ "$(ipc dialogOpen)" == "true" ]] \
        || fail "network: the dialog closed on an append it could not read a body for"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] \
        || fail "network: escape did not close the dialog after the unreadable-file arm"
    printf 'NETWORK unreadable-file-writes-nothing=ok\n'

    # Plain WebDAV is port 80, so the tick that picks the scheme has to pick the number with it, or
    # the dialog offers a port that scheme does not use while the rail dedups against the one it
    # does. The arm above closed its own dialog, so this one opens a fresh one.
    rail_focus
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: a from the rail did not reopen the dialog for the TLS pass"
    click_chip WebDAV
    settle
    [[ "$(ipc networkPort)" == "443" ]] || fail "network: WebDAV opened on port $(ipc networkPort), not 443"
    key "wd.example" >/dev/null
    # Domain is hidden under WebDAV and the walk skips it, Password is shown and the walk crosses it,
    # so the walk reads where it is rather than counting: the traversal arm above pins the order.
    for _attempt in $(seq 1 8); do
        [[ "$(ipc networkFocus)" == "TLS" ]] && break
        key -k Tab >/dev/null
        settle
    done
    [[ "$(ipc networkFocus)" == "TLS" ]] \
        || fail "network: the walk never reached the TLS row, it is on $(ipc networkFocus)"
    key -k Space >/dev/null
    settle
    [[ "$(ipc networkPort)" == "80" ]] \
        || fail "network: unticking TLS left the port at $(ipc networkPort), not plain dav's own 80"
    [[ "$(ipc networkUri)" == "dav://wd.example/" ]] \
        || fail "network: the Mounts-as line reads $(ipc networkUri) after the tick"
    printf 'NETWORK tls-port=ok\n'
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: escape did not close the dialog after the TLS pass"

    # The same class on the rail's own rename, which derived its body from the text the rail was
    # built from: a read that failed empties that too, so relabelling it appended the renamed share
    # to nothing and left a bookmarks file holding one line. Only a live mount reaches it, because a
    # saved place is drawn from the very text the failed read emptied; gio is stubbed for one (the
    # case_sharebrowser idiom), and the mode is set before the launch so the read fails at startup
    # rather than depending on what a chmod tells inotify. See AGENTS.md "A failed FileView read".
    local gio_stub="$fixture_root/network-gio"
    sandbox_scratch "$gio_stub"
    mkdir -p "$gio_stub/bin"
    # Nothing here is activated, so the listing is the one subcommand the stub is ever asked for.
    cat > "$gio_stub/bin/gio" <<'EOS'
#!/bin/sh
if [ "$1" = mount ] && [ "$2" = "-l" ]; then
  printf 'Mount(0): data on 198.51.100.9 -> smb://198.51.100.9/data/\n'
fi
exit 0
EOS
    chmod +x "$gio_stub/bin/gio"
    local before_rename
    before_rename=$(cat "$bookmarks")
    [[ -n "$before_rename" ]] || fail "network: the rename arm needs saved places to lose, and the file is empty"
    chmod 200 "$bookmarks"
    # Its own name: "local saved_path" again would reassign the one this case restores PATH from.
    local rename_arm_path="$PATH"
    export PATH="$gio_stub/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$rename_arm_path"
    wait_listing 3
    # Only the live mount: the two saved places are invisible because the read that would have drawn
    # them failed, which is the state the rename then has to refuse to derive a body from.
    for _attempt in $(seq 1 200); do
        [[ "$(ipc networkEntries)" == "data|network|share|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "data|network|share|true" ]] \
        || fail "network: the stubbed live mount is not the rail's only network row, got $(ipc networkEntries)"
    rail_focus
    key g >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "network: cursor did not reach the live mount, it is $(ipc railCursor)"
    key -k F2 >/dev/null
    settle
    key "Renamed" >/dev/null
    key -k Return >/dev/null
    settle
    chmod 600 "$bookmarks"
    [[ "$(cat "$bookmarks")" == "$before_rename" ]] \
        || fail "network: a rename derived from a read that failed still wrote, the file now reads: $(cat "$bookmarks")"
    # And the refusal reaches the operator: a rename that reported nothing at all is how the file was
    # lost in silence, so the sentence is asserted and not only the bytes.
    wait_message "Saved places could not be read, so the new name was not saved."

    # The other side of the same guard, and the whole reason it lets FileNotFound through: a box that
    # has never saved a place has no file to read at all, and renaming a live mount is how the first
    # one gets written. A guard that refused every failed read would refuse this too, in silence.
    rm -f "$bookmarks"
    rail_focus
    [[ "$(ipc railCursor)" == "1" ]] || fail "network: the refused rename moved the cursor to $(ipc railCursor)"
    key -k F2 >/dev/null
    settle
    key "First" >/dev/null
    key -k Return >/dev/null
    settle
    for _attempt in $(seq 1 200); do
        [[ -s "$bookmarks" ]] && break
        sleep 0.05
    done
    [[ "$(cat "$bookmarks" 2>&1)" == "smb://198.51.100.9/data First" ]] \
        || fail "network: a rename with no bookmarks file at all did not write the first place, it reads: $(cat "$bookmarks" 2>&1)"
    printf 'NETWORK unreadable-file-renames-nothing=ok absent-file-renames-write-the-first=ok\n'

    # The arm above runs against a listing-only gio, so the cache arms below need this case's own
    # stub back and a window started under it. Its saved places go with it: they are the arm above's
    # subject, not this one's, and the rail rows they draw are nothing below reads.
    kill_flea
    rm -f "$bookmarks"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 3
    rail_focus
    key a >/dev/null
    settle
    click_chip NFS
    key "nfs.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted 5
    [[ "$(cat "$mount_log")" == 'mount nfs://nfs.test/export' ]] \
        || fail "network: NFS did not retain plain gio mount"

    # Two direct opens in a row, the second refused. A mount exit code no longer decides on its own
    # (ui/NetworkMounts.qml "_mountFailed"), so the refused location refuses its info call too, and
    # what this arm proves is that the second open reports its own verdict and not the first's.
    [[ "$(ipc focusView)" == "rail" ]] || fail "network: direct-cache setup did not return to rail"
    key a >/dev/null
    click_chip NFS
    key "stale-one.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted 5
    key a >/dev/null
    click_chip NFS
    key "stale-two.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    wait_network_result failed 5
    [[ "$(ipc networkStatus)" == "Connect failed: network location was refused" ]] \
        || fail "network: the refused direct open reported the previous open's verdict"
    key -k Escape >/dev/null
    settle

    [[ "$(ipc focusView)" == "rail" ]] || fail "network: share-cache setup did not return to rail"
    key a >/dev/null
    key "shares-one.test" >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted 5
    [[ "$(ipc shareBrowserOpen)" == "true" && "$(ipc shareBrowserEntries)" == "old-share" ]] \
        || fail "network: share-cache setup did not list first root"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "network: first share browser did not return to rail"
    key a >/dev/null
    key "shares-two.test" >/dev/null
    key -k Return >/dev/null
    wait_network_result failed 5
    [[ "$(ipc shareBrowserOpen)" == "false" \
        && "$(ipc networkStatus)" == "Connect failed: location has no browsable folder" ]] \
        || fail "network: bare root reused stale share output"

    printf 'NETWORK empty=ok a-scoped=ok dialog=ok submit-path=ok keyboard-after=ok guest-smb=anonymous nfs=plain caches=isolated\n'
    export PATH="$saved_path"
    kill_flea
    sandbox_remove "$gio_stub"
    sandbox_remove "$fixture_home"
    sandbox_remove "$fake_root"
}

# The NETWORK "+" against every text size Omarchy offers. The gap between the ink and its 24 px hit
# target is (hitMin - caption) / 2, so the term that moves it is the caption token and the lever that
# moves that is [font] base-size, read from the fixture HOME rather than the operator's own config.
case_netmark() {
    local dir="$fixture_root/netmark"
    sandbox_scratch "$dir"
    : > "$dir/one.txt"
    local fake_root="$fixture_root/netmark-fake"
    sandbox_scratch "$fake_root"
    mkdir -p "$fake_root/bin"
    # This case measures geometry, so the rail must not depend on whatever the box has mounted.
    printf '#!/bin/sh\nexit 0\n' > "$fake_root/bin/gio"
    chmod +x "$fake_root/bin/gio"
    local saved_path="$PATH"
    export PATH="$fake_root/bin:$PATH"

    local fixture_home="$fixture_root/netmark-home" real_home="$HOME"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0" "$fixture_home/.config/omarchy"
    # One bookmark is one NETWORK row, and a NETWORK row is the only thing that draws an indicator dot.
    printf 'smb://198.51.100.1/ 198.51.100.1\n' > "$fixture_home/.config/gtk-3.0/bookmarks"

    local base first=true caption smallest="" largest=""
    for base in 9 10 11 12 14 16 20; do
        printf '[font]\nbase-size = %s\n' "$base" > "$fixture_home/.config/omarchy/shell.toml"
        export HOME="$fixture_home"
        launch "$dir"
        export HOME="$real_home"
        wait_rail 1
        # The probes cost four clicks, so they run once, at the stop with the widest overhang, and
        # before the invariant: a measured box has to be real before its centre is worth arguing about.
        if [[ "$first" == true ]]; then
            probe_network_mark_target
            first=false
        fi
        assert_network_mark_alignment "text size $base"
        read -r _body caption _pad _rowheight <<< "$(ipc metrics)"
        [[ -n "$smallest" ]] || smallest="$caption"
        largest="$caption"
    done
    # A sweep whose stops all render the same caption would pass without testing anything.
    [[ "$largest" -gt "$smallest" ]] \
        || fail "netmark: caption stayed at $smallest across every text size, so no stop took effect"

    # The text-size chords are the second live lever on this base, keys.toml textSizeUp/textSizeDown,
    # and their top stop is the only place the caption grows past Theme.hitMin, where the hit target
    # and the caption slot become the same box. Those stops are read back the way the base sizes are.
    local step zoom_small zoom_large
    for step in 1 2; do
        key -M ctrl -M shift -k minus -m shift -m ctrl >/dev/null
    done
    settle
    assert_network_mark_alignment "text size 20 at the smallest zoom"
    read -r _body zoom_small _pad _rowheight <<< "$(ipc metrics)"
    for step in $(seq 1 12); do
        key -M ctrl -M shift -k equal -m shift -m ctrl >/dev/null
    done
    settle
    assert_network_mark_alignment "text size 20 at the largest zoom"
    read -r _body zoom_large _pad _rowheight <<< "$(ipc metrics)"
    key -M ctrl -M shift -k 0 -m shift -m ctrl >/dev/null
    settle
    # The chords are keystrokes into a live window, so the zoom ends are proven the same way: a pair
    # that never moved the caption would assert the invariant twice against one layout.
    [[ "$zoom_large" -gt "$zoom_small" ]] \
        || fail "netmark: caption stayed at $zoom_small across both zoom ends, so the chords did nothing"

    printf 'NETMARK stops=7 caption=%s..%s probes=4 zoom=%s..%s\n' \
        "$smallest" "$largest" "$zoom_small" "$zoom_large"
    export PATH="$saved_path"
    kill_flea
    sandbox_remove "$fixture_home"
    sandbox_remove "$fake_root"
    sandbox_remove "$dir"
}

# Restores the authenticated route as a product-level control: the helper sees one URI argument and
# one stdin line, while every durable/test-visible surface stays secret-free.
case_networkauth() {
    local dir="$fixture_root/network-auth" fixture_home="$fixture_root/network-auth-home"
    local state="$dir/state" runtime_canary
    runtime_canary=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
    local helper_log="$state/helper.log"
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    local retry_dir="$state/retry-root"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin" "$state" "$retry_dir"
    : > "$dir/local.txt"
    : > "$retry_dir/alpha.txt"
    : > "$state/mounted"
    : > "$helper_log"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf '%s\n' 'sftp://tester@slot.test/home42/tester Own slot' > "$bookmarks"
    printf '%s\n' 'sftp://tester@slot.test/' > "$state/mounted"

    assert_runtime_canary_absent() {
        local surface method value
        for surface in "$fixture_root" "$thumb_fixture" "$hash_fixture" "$stale_fixture" \
            "$evidence_dir" "$flea_log" "$run_log" "$case_log" \
            "$repo/.superpowers/flea/release-014-20260904/reports/baseline-authenticated-ui.md"; do
            [[ -e "$surface" ]] || continue
            ! printf '%s\n' "$runtime_canary" | grep -R -a -F -q -f - "$surface" 2>/dev/null \
                || fail "networkauth: runtime canary reached byte-addressable surface"
        done
        for method in themeForeground selectedFill palette metrics tokens selectedIndices focusView \
            path lastMessage stickyMessage firstRowsAt inputToRows mode state stateMessage \
            contextMenuEntries contextMenuGlyphs contextMenuSubmenuGlyphs contextMenuSubmenuEntries \
            renameEditorText railRenameEditorText previewKind previewState previewPdfZoom previewExpanded \
            previewPdfPage headerTitles sortMark previewSliderCentre headerLeft viewMode archiveFormats \
            keymapSheetRows convertFormat previewFacts previewColumnState columnPlayCentre columnStripCentre \
            tabLabels pathBarText pathCentre \
            headerTop networkProtocol networkPort networkUri networkPathLabel networkTitle networkFields \
            networkFocus networkHostPortWidths networkPasswordState networkNote networkAction networkStatus \
            networkDialogMetrics networkDialogMetricTargets networkResult networkPasswordEyeCentre \
            shareBrowserEntries networkEntries deviceEntries; do
            value=$(ipc "$method" 2>/dev/null || true)
            [[ "$value" != *"$runtime_canary"* ]] \
                || fail "networkauth: runtime canary reached IPC $method"
        done
    }

    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
case "\$1 \${2:-}" in
"mount -l")
    if [ -s "$state/mounted" ]; then
        uri=\$(cat "$state/mounted")
        printf 'Mount(0): auth-test -> %s\n' "\$uri"
    fi
    ;;
"mount -u")
    printf '%s\n' "\$3" > "$state/unmount-uri"
    : > "$state/mounted"
    ;;
"info "*)
    mounted=\$(cat "$state/mounted")
    if [ "\$mounted" != "\$2" ]; then
        [ "\$mounted" = "sftp://tester@slot.test/" ] \
            && [ "\$2" = "sftp://tester@slot.test/home42/tester" ] || exit 1
    fi
    printf '%s\n' "\$2" > "$state/info-uri"
    if [ "\$2" = "ftps://tester@slot.test/retry" ]; then
        printf 'local path: %s\n' "$retry_dir"
    else
        printf 'local path: %s\n' "$dir"
    fi
    ;;
"mount "*) printf '%s\n' "\$2" > "$state/mounted" ;;
*) exit 1 ;;
esac
EOS
    cat > "$dir/bin/flea-gio-auth" <<'EOS'
#!/bin/sh
[ "$#" -eq 1 ] || exit 2
IFS= read -r secret || exit 3
[ -n "$secret" ] || exit 4
cmdline=$(tr '\0' '\n' < "/proc/$$/cmdline")
environment=$(env)
case "$cmdline$environment" in *"$secret"*) exit 5 ;; esac
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
state=${script_dir%/bin}/state
printf 'argc=1 uri=%s stdin-lines=1\n' "$1" >> "$state/helper.log"
[ ! -e "$state/fail" ] || exit 60
printf '%s\n' "$1" > "$state/mounted"
secret=
EOS
    chmod +x "$dir/bin/gio" "$dir/bin/flea-gio-auth"

    local real_home="$HOME" saved_path="$PATH"
    export HOME="$fixture_home"
    export PATH="$dir/bin:$PATH"
    export FLEA_GIO_AUTH="$dir/bin/flea-gio-auth"
    launch "$dir"
    export HOME="$real_home"
    wait_listing_wall 3

    # GIO exposes an authority-root mount for SFTP even when the saved product location is an
    # addressable descendant. The rail must keep the saved URI, mark it mounted, and emit one row.
    local want_entries='Own slot|network|share|true' seen_entries="" entries_deadline
    entries_deadline=$(( $(date +%s%3N) + 12000 ))
    while (( $(date +%s%3N) < entries_deadline )); do
        seen_entries=$(timeout 1 omarchy-drive ipc -p "$flea_ui" flea networkEntries 2>/dev/null || true)
        [[ "$seen_entries" == "$want_entries" ]] && break
        sleep 0.1
    done
    [[ "$seen_entries" == "$want_entries" ]] \
        || fail "networkauth: authority-root mount duplicated descendant bookmark ($seen_entries)"
    local network_index step
    network_index=$(ipc networkStartIndex)
    key -k Tab >/dev/null
    key g >/dev/null
    for ((step = 0; step < network_index; step++)); do key j >/dev/null; done
    key -k Return >/dev/null
    wait_network_result mounted 5
    [[ "$(cat "$state/info-uri")" == 'sftp://tester@slot.test/home42/tester' ]] \
        || fail "networkauth: mounted saved row did not resolve its descendant URI"
    [[ ! -s "$helper_log" ]] || fail "networkauth: already-mounted descendant launched helper"
    click_rail_row "$network_index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Unmount|Rename|Remove" ]] \
        || fail "networkauth: the projected mounted row offers $(ipc contextMenuEntries), not Unmount first"
    key -k Return >/dev/null
    wait_network_result unmounted 5
    [[ "$(cat "$state/unmount-uri")" == 'sftp://tester@slot.test/' ]] \
        || fail "networkauth: projected row unmounted its saved descendant URI"
    printf 'NETWORKAUTH descendant-dedup=ok saved-uri=ok mount-uri=ok\n'
    kill_flea
    : > "$state/mounted"
    printf '%s\n' \
        'dav://tester@plain-default.test/ Plain 80' \
        'dav://tester@plain-secure-port.test:443/ Plain 443' > "$bookmarks"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_network_entry_state false
    network_index=$(ipc networkStartIndex)
    key -k Tab >/dev/null
    key g >/dev/null
    for ((step = 0; step < network_index; step++)); do key j >/dev/null; done
    key -k Return >/dev/null
    settle
    [[ "$(ipc networkPort)" == 80 && "$(ipc networkUri)" == 'dav://tester@plain-default.test/' ]] \
        || fail "networkauth: dav default reparsed as port $(ipc networkPort), URI $(ipc networkUri)"
    key -k Escape >/dev/null
    key j >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc networkPort)" == 443 && "$(ipc networkUri)" == 'dav://tester@plain-secure-port.test:443/' ]] \
        || fail "networkauth: dav :443 reparsed as port $(ipc networkPort), URI $(ipc networkUri)"
    printf 'NETWORKAUTH dav-default=80 dav-explicit=443\n'
    kill_flea
    : > "$bookmarks"
    : > "$state/info-uri"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing_wall 3

    key -k Tab >/dev/null
    key a >/dev/null
    settle
    [[ "$(ipc networkTitle)" == "SMB share" ]] || fail "networkauth: wrong SMB title"
    [[ "$(ipc networkFields)" == "Label|Host|Port|Share|Domain|Username|Password" ]] \
        || fail "networkauth: SMB fields are $(ipc networkFields)"
    [[ "$(ipc networkHostPortWidths)" == *"|"* ]] || fail "networkauth: no Host/Port geometry"
    local host_width port_width
    IFS='|' read -r host_width port_width <<< "$(ipc networkHostPortWidths)"
    [[ "$host_width" == "$port_width" ]] || fail "networkauth: Host/Port widths differ ($host_width/$port_width)"
    [[ "$(ipc networkPasswordState)" == "masked|empty" ]] \
        || fail "networkauth: fresh password state is $(ipc networkPasswordState)"
    [[ "$(ipc networkDialogMetrics)" == "$(ipc networkDialogMetricTargets)" ]] \
        || fail "networkauth: card padding/gap $(ipc networkDialogMetrics) differs from scaled 16/12 target $(ipc networkDialogMetricTargets)"

    local title_case want_protocol want_title want_fields
    for title_case in \
        "SFTP|SFTP host|Label|Host|Port|Path|Username|Password" \
        "FTPS|FTPS|Label|Host|Port|Path|Username|Password|TLS" \
        "WebDAV|WebDAV endpoint|Label|Host|Port|Path|Username|Password|TLS"; do
        IFS='|' read -r want_protocol want_title want_fields <<< "$title_case"
        click_chip "$want_protocol"
        [[ "$(ipc networkTitle)" == "$want_title" ]] \
            || fail "networkauth: $want_protocol title is $(ipc networkTitle)"
        [[ "$(ipc networkFields)" == "$want_fields" ]] \
            || fail "networkauth: $want_protocol fields are $(ipc networkFields)"
    done

    click_chip NFS
    settle
    [[ "$(ipc networkTitle)" == "NFS export" ]] || fail "networkauth: wrong NFS title"
    [[ "$(ipc networkFields)" == "Label|Host|Port|Export" ]] || fail "networkauth: NFS fields are $(ipc networkFields)"
    [[ "$(ipc networkNote)" == "No credentials: NFS trusts the client host" ]] \
        || fail "networkauth: NFS note is $(ipc networkNote)"

    click_chip SMB
    key "slot.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "data" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "tester" >/dev/null
    key -k Tab >/dev/null
    [[ "$(ipc networkFocus)" == "Password" ]] || fail "networkauth: Password did not follow Username"
    printf '%s' "$runtime_canary" | omarchy-drive key --window flea - >/dev/null
    [[ "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: typed password is not masked"
    shot networkauth-password-masked
    assert_runtime_canary_absent
    local eye_centre eye_x eye_y wx wy held_password_state released_password_state
    eye_centre=$(ipc networkPasswordEyeCentre 2>/dev/null) \
        || fail "networkauth: password eye has no IPC centre"
    read -r eye_x eye_y <<< "$eye_centre"
    read -r wx wy _ww _wh < <(window_box)
    omarchy-drive move "$((wx + eye_x))" "$((wy + eye_y))" >/dev/null
    ydotool click 0x40 >/dev/null 2>&1
    held_password_state=$(ipc networkPasswordState)
    ydotool click 0x80 >/dev/null 2>&1
    released_password_state=$(ipc networkPasswordState)
    [[ "$held_password_state" == "visible|set" ]] \
        || fail "networkauth: password eye did not reveal only while held"
    [[ "$released_password_state" == "masked|set" ]] \
        || fail "networkauth: password eye did not remask on release"
    key -k Return >/dev/null
    wait_network_result mounted
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "networkauth: successful save left dialog open"
    [[ "$(cat "$helper_log")" == "argc=1 uri=smb://tester@slot.test/data stdin-lines=1" ]] \
        || fail "networkauth: SMB helper route is $(cat "$helper_log")"
    [[ "$(cat "$bookmarks")" == "smb://tester@slot.test/data data" ]] \
        || fail "networkauth: secret-free bookmark is $(cat "$bookmarks")"

    # An already-mounted authenticated row resolves directly and never needs the process password.
    local helper_calls
    helper_calls=$(wc -l < "$helper_log")
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_network_entry_state true
    key -k Tab >/dev/null
    key g >/dev/null
    key j >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted
    [[ "$(ipc dialogOpen)" == "false" ]] \
        || fail "networkauth: already-mounted row asked for a password"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: already-mounted row launched helper"

    # A restart erases the process map. The saved credentialed row must reopen populated and must
    # not launch the helper until the user supplies a new password.
    : > "$state/mounted"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_network_entry_state false
    key -k Tab >/dev/null
    key g >/dev/null
    key j >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" && "$(ipc networkUri)" == "smb://tester@slot.test/data" ]] \
        || fail "networkauth: missing session credential did not reopen populated"
    [[ "$(ipc networkAction)" == "Retry" && "$(ipc networkPasswordState)" == "masked|empty" ]] \
        || fail "networkauth: missing credential state is $(ipc networkAction)/$(ipc networkPasswordState)"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: missing credential launched helper"

    # Missing and non-executable helpers both fail closed, retain the secret in process memory for
    # Retry, and never turn the password into an argument, environment value or log line.
    local _field
    for _field in Port Share Domain Username Password; do key -k Tab >/dev/null; done
    [[ "$(ipc networkFocus)" == "Password" ]] \
        || fail "networkauth: missing-helper setup did not reach Password"
    printf '%s' "$runtime_canary" | omarchy-drive key --window flea - >/dev/null
    mv "$dir/bin/flea-gio-auth" "$dir/bin/flea-gio-auth.real"
    key -k Return >/dev/null
    wait_network_result failed 5
    [[ "$(ipc dialogOpen)" == "true" \
        && "$(ipc networkStatus)" == "Connect failed: authentication helper is unavailable" \
        && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: missing helper did not fail closed and retain fields"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: missing helper wrote helper output"

    cp "$dir/bin/flea-gio-auth.real" "$dir/bin/flea-gio-auth"
    chmod 0644 "$dir/bin/flea-gio-auth"
    key -k Escape >/dev/null
    key -k Return >/dev/null
    wait_network_result failed 5
    [[ "$(ipc dialogOpen)" == "true" \
        && "$(ipc networkStatus)" == "Connect failed: authentication helper is unavailable" \
        && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: permission-denied helper did not fail closed and retain fields"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: permission-denied helper wrote helper output"

    # A helper that never exits must be stopped by the product deadline, not by this driver's wait.
    cat > "$dir/bin/flea-gio-auth" <<'EOS'
#!/bin/sh
IFS= read -r _password || exit 3
sleep 120
EOS
    chmod +x "$dir/bin/flea-gio-auth"
    key -k Escape >/dev/null
    key -k Return >/dev/null
    wait_network_result mounting 5
    wait_network_result failed 40
    [[ "$(ipc dialogOpen)" == "true" \
        && "$(ipc networkStatus)" == "Connect failed: host did not respond" \
        && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: helper timeout did not retain one sanitized Retry artifact"

    # After another restart the same saved row has no map entry. Submitting its populated form with
    # an empty password keeps it open and does not launch any helper.
    cp "$dir/bin/flea-gio-auth.real" "$dir/bin/flea-gio-auth"
    chmod +x "$dir/bin/flea-gio-auth"
    key -k Escape >/dev/null
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_network_entry_state false
    key -k Tab >/dev/null
    key g >/dev/null
    key j >/dev/null
    key -k Return >/dev/null
    settle
    helper_calls=$(wc -l < "$helper_log")
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" && "$(ipc networkResult)" == "missing-credential" \
        && "$(ipc networkAction)" == "Retry" && "$(ipc networkPasswordState)" == "masked|empty" ]] \
        || fail "networkauth: empty password did not remain a missing credential"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: empty password launched helper"

    for _field in Port Share Domain Username Password; do key -k Tab >/dev/null; done
    [[ "$(ipc networkFocus)" == "Password" ]] \
        || fail "networkauth: corrected-retry setup did not reach Password"
    printf '%s' "$runtime_canary" | omarchy-drive key --window flea - >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted
    [[ "$(cat "$state/info-uri")" == "smb://tester@slot.test/data" ]] \
        || fail "networkauth: corrected-retry setup did not resolve first location"

    # The approved failed-connect artifact is FTPS: keep every field, mask the password, say one
    # sentence and replace Save with Retry.
    [[ "$(ipc focusView)" == "rail" ]] \
        || fail "networkauth: corrected-retry setup did not return to rail"
    key a >/dev/null
    click_chip FTPS
    key "slot.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "retry" >/dev/null
    key -k Tab >/dev/null
    key "tester" >/dev/null
    key -k Tab >/dev/null
    printf '%s' "$runtime_canary" | omarchy-drive key --window flea - >/dev/null
    : > "$state/fail"
    key -k Return >/dev/null
    wait_network_result failed
    [[ "$(ipc dialogOpen)" == "true" && "$(ipc networkTitle)" == "FTPS, failed connect" ]] \
        || fail "networkauth: failure did not reopen approved FTPS artifact"
    [[ "$(ipc networkStatus)" == "Connect failed: host refused the TLS handshake" ]] \
        || fail "networkauth: failure said $(ipc networkStatus)"
    [[ "$(ipc networkAction)" == "Retry" && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: failure lost Retry or masked credential state"
    [[ "$(ipc networkUri)" == "ftps://tester@slot.test/retry" ]] \
        || fail "networkauth: failure lost fields, URI is $(ipc networkUri)"
    assert_runtime_canary_absent

    helper_calls=$(wc -l < "$helper_log")
    rm -f "$state/fail"
    key -k Return >/dev/null
    wait_network_result mounted
    [[ "$(wc -l < "$helper_log")" -eq $((helper_calls + 1)) ]] \
        || fail "networkauth: corrected Retry did not launch helper once"
    [[ "$(tail -n 1 "$helper_log")" == "argc=1 uri=ftps://tester@slot.test/retry stdin-lines=1" ]] \
        || fail "networkauth: corrected Retry used stale helper URI"
    [[ "$(cat "$state/info-uri")" == "ftps://tester@slot.test/retry" ]] \
        || fail "networkauth: corrected Retry resolved stale info URI"
    [[ "$(cat "$state/mounted")" == "ftps://tester@slot.test/retry" ]] \
        || fail "networkauth: corrected Retry did not mount current URI"
    wait_path_wall "$retry_dir" 10
    wait_listing_wall 1 10
    [[ "$(ipc rowAt 0)" == "alpha.txt|"* ]] \
        || fail "networkauth: corrected Retry browsed stale child"
    [[ "$(ipc dialogOpen)" == "false" && "$(ipc shareBrowserOpen)" == "false" ]] \
        || fail "networkauth: corrected Retry left stale failure UI"
    assert_runtime_canary_absent
    sleep "$transient_clear_s"
    [[ -z "$(ipc lastMessage)" ]] \
        || fail "networkauth: corrected Retry retained stale error text"

    printf 'NETWORKAUTH artifact=ok stdin=one persistence=none retry=corrected eye=held missing-session=no-launch\n'
    unset FLEA_GIO_AUTH
    export PATH="$saved_path"
    kill_flea
    sandbox_remove "$fixture_home"
}

case_networktimeout() {
    local dir="$fixture_root/network-timeout"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/one.txt"
    : > "$dir/two.txt"
    local calls="$dir/calls"
    printf '0\n' > "$calls"

    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
if [ "\$1 \$2" != "mount -l" ]; then
  exec /usr/bin/gio "\$@"
fi
count=\$(cat "$calls")
count=\$((count + 1))
printf '%s\n' "\$count" > "$calls"
case "\$count" in
  1) printf 'Mount(0): First -> smb://stub/first\n' ;;
  2) exec sleep 20 ;;
  *) printf 'Mount(0): Recovered -> smb://stub/recovered\n' ;;
esac
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/network-timeout-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME"
    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"

    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "First|network|share|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "First|network|share|true" ]] \
        || fail "networktimeout: first listing never appeared, got $(ipc networkEntries)"
    sleep "$rail_poll_wait_s"
    [[ "$(ipc networkEntries)" == "First|network|share|true" ]] \
        || fail "networktimeout: a wedged listing erased the last good row"

    for _attempt in $(seq 1 220); do
        [[ "$(ipc networkEntries)" == "Recovered|network|share|true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc networkEntries)" == "Recovered|network|share|true" ]] \
        || fail "networktimeout: polling never recovered, got $(ipc networkEntries)"
    [[ "$(cat "$calls")" -ge 3 ]] || fail "networktimeout: expected three list attempts"

    printf 'NETWORKTIMEOUT retained=ok retry=ok\n'
    export PATH="$saved_path"
    kill_flea
    sandbox_remove "$fixture_home"
}

case_networklive() {
    local uri=${FLEA_NETWORK_LIVE_URI:-}
    local mount_uri=${FLEA_NETWORK_LIVE_MOUNT_URI:-$uri}
    local mount_root=${FLEA_NETWORK_LIVE_ROOT:-}
    local relative=${FLEA_NETWORK_LIVE_RELATIVE:-}
    local protocol=${FLEA_NETWORK_LIVE_PROTOCOL:-}
    local host=${FLEA_NETWORK_LIVE_HOST:-}
    local remote_path=${FLEA_NETWORK_LIVE_PATH:-}
    local remote_user=${FLEA_NETWORK_LIVE_USER:-}
    local auth=${FLEA_NETWORK_LIVE_AUTH:-none}
    local product_root
    [[ "$uri" == *://* && "$mount_uri" == *://* \
        && "$mount_root" == "/run/user/$(id -u)/gvfs/"* ]] \
        || fail "networklive: missing or unsafe live mount contract"
    [[ -n "$relative" && "$relative" != /* && "$relative" != *".."* ]] \
        || fail "networklive: unsafe relative test path"
    [[ -n "$host" && ( "$auth" == password || "$auth" == none ) ]] \
        || fail "networklive: incomplete form contract"

    local dir="$fixture_root/network-live"
    local fixture_home="$fixture_root/network-live-home"
    sandbox_scratch "$dir"
    : > "$dir/local.txt"
    fixture_home_make "$fixture_home"
    local real_home="$HOME"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"

    wait_listing_wall 1
    key -k Tab >/dev/null
    key a >/dev/null
    settle
    click_chip "$protocol"
    key "$host" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    [[ -z "$remote_path" ]] || key "$remote_path" >/dev/null
    if [[ "$auth" == password ]]; then
        key -k Tab >/dev/null
        key "$remote_user" >/dev/null
    fi
    [[ "$(ipc networkUri)" == "$uri" ]] \
        || fail "networklive: form did not build the exact product URI"
    if [[ "$auth" == password ]]; then
        key -k Tab >/dev/null
        [[ "$(ipc networkFocus)" == "Password" ]] \
            || fail "networklive: password field did not receive focus"
        [[ "$(ipc networkPasswordState)" == "masked|empty" ]] \
            || fail "networklive: password was populated before stdin delivery"
        omarchy-drive key --window flea - >/dev/null
        [[ "$(ipc networkPasswordState)" == "masked|set" ]] \
            || fail "networklive: password stdin was empty"
        key -k Return >/dev/null
    else
        key -k Return >/dev/null
    fi
    wait_network_result mounted 120
    product_root=$(timeout 5 gio info "$uri" 2>/dev/null | sed -n 's/^local path: //p')
    [[ "$product_root" == "/run/user/$(id -u)/gvfs/"* ]] \
        || fail "networklive: product mount has no safe FUSE root"
    mount_root=$product_root
    [[ -f "${mount_root%/}/$relative/alpha.txt" ]] \
        || fail "networklive: relative fixture is absent below product mount root $mount_root"
    wait_path_wall "$mount_root" 25

    wait_network_entry_state true
    local entries entry label network_count network_index step
    local -a components
    entries=$(ipc networkEntries)
    network_count=0
    while IFS= read -r entry; do
        [[ -n "$entry" ]] && network_count=$((network_count + 1))
    done <<< "$entries"
    [[ "$network_count" -eq 1 && "$entries" == *"|network|share|true" ]] \
        || fail "networklive: expected one mounted share, got $network_count"
    label=${entries%%|*}
    network_index=$(ipc networkStartIndex)

    if [[ "$(ipc focusView)" == "rail" ]]; then key -k Escape >/dev/null; fi
    [[ "$(ipc focusView)" == "list" ]] \
        || fail "networklive: could not normalize focus before rail activation"
    key -k Tab >/dev/null
    [[ "$(ipc focusView)" == "rail" ]] \
        || fail "networklive: Tab did not focus the rail"
    key g >/dev/null
    for ((step = 0; step < network_index; step++)); do key j >/dev/null; done
    key -k Return >/dev/null
    wait_path_wall "$mount_root" 25
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "networklive: Escape did not return focus to the list"

    local current=$mount_root component row
    IFS=/ read -ra components <<< "$relative"
    for component in "${components[@]}"; do
        [[ -n "$component" ]] || continue
        row=$(find_row_wall "$component" 25) \
            || fail "networklive: path component unavailable"
        goto_row "$row"
        key -k Return >/dev/null
        current="${current%/}/$component"
        wait_path_wall "$current" 25
    done

    row=$(find_row_wall alpha.txt 25) || fail "networklive: alpha.txt absent"
    goto_row "$row"
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "text" ]] \
        || fail "networklive: remote text preview failed"
    key -k Escape >/dev/null

    key -k Tab >/dev/null
    key g >/dev/null
    key -k Return >/dev/null
    wait_path_wall "$fixture_home" 25
    wait_listing_wall 0 25
    click_rail_row "$network_index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Unmount|Rename|Remove" ]] \
        || fail "networklive: mounted share menu is $(ipc contextMenuEntries), not Unmount first"
    key -k Return >/dev/null
    wait_message "Unmounted $label."
    wait_network_result unmounted 25
    wait_network_entry_state false 25
    ! gio mount -l 2>/dev/null | grep -Fq -- "-> $mount_uri" || fail "networklive: GIO mount survived"

    printf 'NETWORKLIVE protocol=%s cold-mount=ok rail=ok browse=ok preview=ok unmount=ok saved=false\n' "$protocol"
    kill_flea
    sandbox_remove "$fixture_home"
}

case_gvfs_cleanup() {
    ( kill_flea ) >/dev/null 2>&1 || true
    if [[ -n "${FLEA_GVFS_CASE_DIR:-}" ]]; then
        DIR="$FLEA_GVFS_CASE_DIR" ./tools/flea-gvfs-fixture clean >/dev/null 2>&1 || true
    fi
    if [[ -n "${FLEA_GVFS_CASE_HOME:-}" ]]; then
        sandbox_remove "$FLEA_GVFS_CASE_HOME"
    fi
}

case_gvfs() {
    local dir="$fixture_root/gvfs-local"
    sandbox_scratch "$dir"
    : > "$dir/one.txt"
    : > "$dir/two.txt"
    local share_dir="$fixture_root/gvfs-share"
    local fixture_home="$fixture_root/gvfs-home"
    fixture_home_make "$fixture_home"
    FLEA_GVFS_CASE_DIR="$share_dir"
    FLEA_GVFS_CASE_HOME="$fixture_home"
    trap case_gvfs_cleanup EXIT HUP INT TERM
    local real_home="$HOME"
    local uri local_path

    uri=$(DIR="$share_dir" ./tools/flea-gvfs-fixture make)
    DIR="$share_dir" ./tools/flea-gvfs-fixture mount
    local_path=$(gio info "$uri" | sed -n 's/^local path: //p')
    [[ "$local_path" == "/run/user/$(id -u)/gvfs/"* ]] \
        || fail "gvfs: fixture has no FUSE path, got $local_path"

    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 2
    wait_rail 2
    [[ "$(ipc networkEntries)" == "share.zip|network|share|true" ]] \
        || fail "gvfs: live mount never appeared, got $(ipc networkEntries)"

    key -k Tab >/dev/null
    key g >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "gvfs: share row is not rail index 1"
    key -k Return >/dev/null
    wait_path "$local_path"
    wait_listing 2
    [[ "$(ipc rowAt 0)" == "alpha.txt|"* || "$(ipc rowAt 1)" == "alpha.txt|"* ]] \
        || fail "gvfs: alpha.txt absent from Flea listing"

    key -k Escape >/dev/null
    open_row alpha.txt
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "text" ]] \
        || fail "gvfs: alpha.txt did not open as a text preview"
    key -k Escape >/dev/null
    key -k Tab >/dev/null
    key g >/dev/null
    key -k Return >/dev/null
    wait_path "$fixture_home"

    click_rail_row 1 right
    settle
    [[ "$(ipc contextMenuEntries)" == "Unmount|Rename|Remove" ]] \
        || fail "gvfs: mounted share menu is $(ipc contextMenuEntries)"
    key -k Return >/dev/null
    wait_message "Unmounted share.zip."
    for _attempt in $(seq 1 100); do
        [[ -z "$(ipc networkEntries)" ]] && break
        sleep 0.05
    done
    [[ -z "$(ipc networkEntries)" ]] || fail "gvfs: row survived unmount"
    ! gio mount -l | grep -Fq -- "-> $uri" || fail "gvfs: GIO mount survived Flea unmount"

    printf 'GVFS rail=ok browse=ok preview=ok unmount=ok\n'
    case_gvfs_cleanup
    trap - EXIT HUP INT TERM
}

# Item 1 of Task 15 fix round 2: activating a bare smb://host/ entry lists its shares as pane
# rows (ui/ShareBrowser.qml) instead of round 1's superseded sidebar bookmark expansion, and Enter
# on a row mounts and opens through the exact ui/NetworkMounts.qml pipeline a bookmarked share
# already uses. gio is stubbed (the case_open/case_preview idiom) so this is hermetic: no real NAS,
# no auth prompt. The stub also reproduces gio's own "already mounted" quirk on the second share,
# found live against the real NAS this round when a share picked from the overlay was already
# mounted from a prior activation; mountProcess used to treat that as a hard failure for anything
# but a bare root, misreporting a location that actually mounted fine. Nothing reads that sentence
# any more, so the stub speaks it in Spanish: issue #36 reported network shares that never open on
# a non-English box, and a stub that only ever spoke English could not fail for that reason. It
# answers gio's own "local path" line in Spanish too, unless the caller pinned the C locale the way
# ui/NetworkMounts.qml "gioEnvironment" does, which is what makes this case that fix's control:
# measured, not asserted, by live matrix step 0b, which neutralised the pin and reddened this case
# twice and greened it twice. A third share fails for real, so the harmless refusal and the genuine
# failure differ here by their verdict and not by their exit code.
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
    local share3_uri="smb://stubhost/share3/"

    # A plain dispatcher, not a canned fixture: it answers exactly the four gio subcommands
    # ui/NetworkMounts.qml issues, keyed on the exact uri each entry activates.
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
# The gio client translates its own output, so it answers English only where the caller pinned C.
local_path_label='ruta local'
[ "\$LC_ALL" = C ] && local_path_label='local path'
case "\$1" in
  mount)
    if [ "\$2" = "-l" ] || [ "\$2" = "-u" ]; then
      exit 0
    fi
    # The product passes --anonymous on smb, so the location is the last argument, not the second.
    shift \$(( \$# - 1 ))
    if [ "\$1" = "$share2_uri" ]; then
      # gvfsd composes this refusal, so no client locale makes it English: it stays Spanish.
      echo "gio: \$1: La ubicacion ya esta montada" >&2
      exit 2
    fi
    if [ "\$1" = "$share3_uri" ]; then
      # A genuine failure, translated by the same daemon, and the control the already-mounted half
      # never had: same nonzero exit, opposite verdict, told apart only by the gio info below.
      echo "gio: \$1: No se pudo conectar con el servidor" >&2
      exit 1
    fi
    exit 0
    ;;
  info)
    # ui/NetworkMounts.qml normalizes before it asks, so the trailing slash is off by here; real gio
    # answers either spelling and this literal double is made to as well.
    asked="\${2%/}/"
    case "\$asked" in
      "$share1_uri") printf '%s: %s\n' "\$local_path_label" "$share1_dir" ;;
      "$share2_uri") printf '%s: %s\n' "\$local_path_label" "$share2_dir" ;;
      # A location gio could not describe at all: no line, nonzero, which is the whole difference.
      "$share3_uri") exit 1 ;;
    esac
    exit 0
    ;;
  list)
    printf 'share1\nshare2\nshare3\n'
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
    [[ "$(ipc shareBrowserEntries)" == "$(printf 'share1\nshare2\nshare3')" ]] \
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

    # share3 fails the same way share2 refused, and the opposite thing has to happen: the bar names
    # the failure and the pane stays where it is. Without this arm a product that read any nonzero
    # mount exit as the harmless already-mounted case would pass every assertion above.
    click_rail_row 1 left
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: third pointer reactivation of StubNAS did not reopen the overlay"
    key j >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc shareBrowserCursor)" == "2" ]] || fail "sharebrowser: cursor did not move to share3, got $(ipc shareBrowserCursor)"
    key -k Return >/dev/null
    wait_message "Connect failed: network location was refused"
    [[ "$(ipc path)" == "$share2_dir" ]] \
        || fail "sharebrowser: a mount that genuinely failed navigated to $(ipc path)"

    printf 'SHAREBROWSER list=ok escape=ok mount-open=ok already-mounted-quirk=ok mount-failure=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"; sandbox_remove "$share1_dir"; sandbox_remove "$share2_dir"
}

# The deadline over every leg of an open and the refusal that says the guard closed.
# ui/NetworkMounts.qml "openShare" is single flight over three children and only the mount one was
# bounded at first: a "gio info" that never returns left infoProcess.running true and every later
# share opened in silence for the life of the window, and a "gio list" that never returns did the
# same to the share browser. The stub hangs info on one share, answers for the other, and hangs list
# on the bare root, so all four behaviours are driven: the busy refusal, the deadline's own sentence,
# the second share opening afterwards, and the listing leg ending in that same sentence.
case_hangshare() {
    local dir="$fixture_root/hangshare"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    : > "$dir/0-one.txt"

    local good_dir="$fixture_root/hangshare-good"
    sandbox_remove "$good_dir"
    mkdir -p "$good_dir"
    : > "$good_dir/one.txt"

    local hang_uri="smb://stubhost/hang/"
    local good_uri="smb://stubhost/good/"
    local root_uri="smb://stubhost/"
    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
# Every call is logged, because a case that hangs on purpose has no other way to say which leg it
# reached; the fail messages below quote it. Same idea as case_unmount's own stub log.
printf '%s\n' "\$*" >> "$dir/bin/calls"
if [ "\$1 \$2" = "mount -l" ]; then
  printf 'Mount(0): hang en stubhost -> $hang_uri\n  Type: GDaemonMount\n'
  printf 'Mount(1): good en stubhost -> $good_uri\n  Type: GDaemonMount\n'
  exit 0
fi
if [ "\$1" = info ]; then
  # A bare server root has no FUSE path of its own, so the product goes on to gio list.
  [ "\$2" = "$root_uri" ] && exit 0
  # The dead-server shape ui/NetworkMounts.qml documents: gio info on a location gvfs cannot reach
  # never returns. The marker is what tells the test the guard has actually closed, so the busy
  # refusal below is never asserted against an open that has not started. exec so the product's own
  # terminate reaches the sleep and leaves nothing behind.
  if [ "\$2" = "$hang_uri" ]; then : > "$dir/bin/info-started"; exec sleep 25; fi
  printf 'local path: %s\n' "$good_dir"
fi
# gio list on a server gvfs cannot reach hangs the same way gio info does, and this is the third
# leg of an open: without its own deadline nothing in the chain is left to end the share browser.
if [ "\$1" = list ]; then : > "$dir/bin/list-started"; exec sleep 25; fi
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/hangshare-home"
    fixture_home_make "$fixture_home"
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf '%s StubRoot\n' "$root_uri" > "$fixture_home/.config/gtk-3.0/bookmarks"
    local real_home="$HOME" saved_path="$PATH"

    local want_rail="hang|network|share|true"$'\n'"good|network|share|true"$'\n'"StubRoot|network|share|false"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    # bin/ is the gio stub's own fixture entry, alongside the one file under test.
    wait_listing 2
    wait_rail 4
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "$want_rail" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "$want_rail" ]] \
        || fail "hangshare: the stub mounts and bare root did not reach the rail, got $(ipc networkEntries)"

    # Home(0), hang(1), good(2). Opening hang starts the info that never answers.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "hangshare: Tab did not reach the rail"
    key g >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "1" ]] || fail "hangshare: expected the rail cursor on hang, got $(ipc railCursor)"
    key l >/dev/null
    wait_marker "$dir/bin/info-started" "hangshare: the hang share's gio info never started, the stub saw: $(grep -v '^mount -l$' "$dir/bin/calls" 2>/dev/null | sort -u | tr '\n' ';')"

    # The guard is closed now, proven by the marker above rather than by a sleep, and a second share
    # must say so rather than swallow the keypress.
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "2" ]] || fail "hangshare: expected the rail cursor on good, got $(ipc railCursor)"
    key l >/dev/null
    wait_message "Another network location is still opening; give it a moment."
    [[ "$(ipc path)" == "$dir" ]] || fail "hangshare: the refused open navigated to $(ipc path)"

    # And the deadline is what reopens it, with the same sentence a mount that never answers gets.
    # The sentence is emitted in the same handler that terminates the leg, and Process.running only
    # clears when onExited is delivered (measured on quickshell 0.3.1), so an l pressed the instant
    # it appears can still meet the guard. Press until it takes: with no deadline none ever does.
    wait_message "Connect failed: host did not respond"
    for _attempt in $(seq 1 200); do
        [[ "$(ipc path)" == "$good_dir" ]] && break
        key l >/dev/null
        sleep 0.05
    done
    [[ "$(ipc path)" == "$good_dir" ]] \
        || fail "hangshare: good never opened after the deadline, path is $(ipc path)"
    [[ "$(ipc total)" == "1" ]] || fail "hangshare: good's own listing did not load, total is $(ipc total)"

    # The third leg, driven from a fresh window: the deadline's sentence is the same one the info
    # leg already produced, and lastMessage still carries it, so a stale match would green this arm
    # against a product that never ended the listing at all. A bare server root ends in gio list,
    # which hangs here exactly as gio info did, and nothing else in the chain is left to end it.
    # The stub and the fixture home are exported around this launch exactly as they were around the
    # first: a relaunch that inherits the restored HOME starts Flea on the operator's own rail.
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 2
    wait_rail 4
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "$want_rail" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "$want_rail" ]] \
        || fail "hangshare: the fresh window's rail is $(ipc networkEntries)"
    [[ -z "$(ipc lastMessage)" ]] || fail "hangshare: the fresh window already says $(ipc lastMessage)"
    # Home(0), hang(1), good(2), StubRoot(3).
    rail_focus
    key g >/dev/null
    key j >/dev/null
    key j >/dev/null
    key j >/dev/null
    settle
    [[ "$(ipc railCursor)" == "3" ]] || fail "hangshare: expected the rail cursor on StubRoot, got $(ipc railCursor)"
    key l >/dev/null
    wait_marker "$dir/bin/list-started" "hangshare: the bare root's gio list never started, the stub saw: $(grep -v '^mount -l$' "$dir/bin/calls" 2>/dev/null | sort -u | tr '\n' ';')"
    wait_message "Connect failed: host did not respond"
    [[ "$(ipc shareBrowserOpen)" == "false" ]] \
        || fail "hangshare: the overlay opened on a listing that never answered"
    # The relaunch above started this window at $dir, so that is where a listing that ended in the
    # deadline has to leave it.
    [[ "$(ipc path)" == "$dir" ]] || fail "hangshare: the timed-out listing navigated to $(ipc path)"

    printf 'HANGSHARE busy-refusal=ok deadline=ok next-share-opens=ok list-deadline=ok\n'
    kill_flea
    sandbox_remove "$fixture_home"; sandbox_remove "$good_dir"
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
    # gvfsd composes this label and translates the word between share and host, so the stub speaks
    # Spanish here whatever the client locale is. What that proves is that the parser is robust to a
    # translated connector, and nothing about the C pin: live matrix step 0b ran this case in both
    # arms of the pin and it passed in both, because ui/js/Mounts.js's regex never reads the
    # connector word, ui/js/Protocols.js "shareName" strips the host plus one word in any language,
    # and this stub answers no gio info, so localPath() is empty either way. case_sharebrowser is
    # where the pin reddens, measured twice in the same step.
    printf 'Mount(0): stubshare en stubhost -> $share_uri\n  Type: GDaemonMount\n'
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

    # Right click raises the menu over the row and nothing else: the release row first, then the two
    # rows the saved place itself owns, and no unmount has run. The old two-right-click arm is gone,
    # see ui/Sidebar.qml "openRailMenu" and ui/js/Mounts.js "rowMenu".
    click_rail_row 1 right
    settle
    printf 'UNMOUNT menu visible=%s entries=%s glyphs=%s\n' \
        "$(ipc contextMenuVisible)" "$(ipc contextMenuEntries)" "$(ipc contextMenuGlyphs)"
    shot unmount-menu
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "unmount: right click opened no menu on the share"
    [[ "$(ipc contextMenuEntries)" == "Unmount|Rename|Remove" ]] \
        || fail "unmount: the share's menu is $(ipc contextMenuEntries), not Unmount then Rename then Remove"
    [[ "$(ipc contextMenuGlyphs)" == "eject|rename|minus" ]] \
        || fail "unmount: the share's rows draw $(ipc contextMenuGlyphs), not eject, rename and minus"
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

    # PR #21's Remove row, driven at last: three of the states ui/NetworkMounts.qml "forget" answers
    # for, each with its own sentence. This home has no bookmarks file, so the live share is unsaved.
    click_rail_row 1 right
    settle
    [[ "$(ipc contextMenuEntries)" == "Unmount|Rename|Remove" ]] \
        || fail "unmount: the share's menu is $(ipc contextMenuEntries) before Remove"
    menu_seek Remove
    key -k Return >/dev/null
    wait_message "stubshare is not a saved place, and stays on the rail until it is unmounted."
    [[ ! -e "$fixture_home/.config/gtk-3.0/bookmarks" ]] \
        || fail "unmount: Remove on a place nothing saved wrote a bookmarks file"
    [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] \
        || fail "unmount: Remove took a live mount off the rail, got $(ipc networkEntries)"

    printf 'UNMOUNT menu=ok escape=ok fire=ok no-menu-on-favourite=ok keyboard=ok unsaved=ok\n'
    kill_flea

    # The next two need a saved place, so the file goes in before the launch that reads it: one line
    # for the share the stub reports live, one for a place nothing mounts.
    mkdir -p "$fixture_home/.config/gtk-3.0"
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    printf 'smb://stubhost/stubshare Saved Share\nsmb://stubhost/ghost Ghost Place\n' > "$bookmarks"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 4
    wait_rail 3
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "Saved Share|network|share|true"$'\n'"Ghost Place|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "Saved Share|network|share|true"$'\n'"Ghost Place|network|share|false" ]] \
        || fail "unmount: the two saved places did not reach the rail, got $(ipc networkEntries)"

    # Mounted and saved: the one line goes, the row stays as the live mount it still is, and its name
    # falls back to gio's own because the bookmark that was winning it is gone. The stub reports the
    # share live whatever happens, so the rail cannot show a Remove that also unmounted; the log the
    # stub keeps is the only thing that can, and it already carries the deliberate unmount above.
    local unmount_log_before
    unmount_log_before=$(cat "$unmount_log")
    click_rail_row 1 right
    settle
    menu_seek Remove
    key -k Return >/dev/null
    wait_message "Saved Share is forgotten, and stays on the rail until it is unmounted."
    [[ "$(cat "$bookmarks")" == "smb://stubhost/ghost Ghost Place" ]] \
        || fail "unmount: Remove did not drop just its own line, the file reads: $(cat "$bookmarks")"
    [[ "$(cat "$unmount_log")" == "$unmount_log_before" ]] \
        || fail "unmount: Remove also unmounted the share, the log now reads: $(cat "$unmount_log")"
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "stubshare|network|share|true"$'\n'"Ghost Place|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "stubshare|network|share|true"$'\n'"Ghost Place|network|share|false" ]] \
        || fail "unmount: the live row kept a name nothing saves any more, got $(ipc networkEntries)"

    # Saved and nothing mounted: the line and the row both go, and no unmount clause is offered.
    click_rail_row 2 right
    settle
    [[ "$(ipc contextMenuEntries)" == "Rename|Remove" ]] \
        || fail "unmount: an unmounted place offers $(ipc contextMenuEntries), not Rename then Remove"
    menu_seek Remove
    key -k Return >/dev/null
    wait_message "Ghost Place is forgotten."
    [[ -f "$bookmarks" ]] || fail "unmount: Remove unlinked the bookmarks file instead of emptying it"
    [[ -z "$(cat "$bookmarks")" ]] \
        || fail "unmount: the last saved line survived Remove, the file reads: $(cat "$bookmarks")"
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "stubshare|network|share|true" ]] \
        || fail "unmount: the forgotten place stayed on the rail, got $(ipc networkEntries)"

    local before_press4
    before_press4=$(stat -c %y "$bookmarks")
    # The second press on the row that is still there, which is the state one sentence used to blame
    # on a file nobody had read: the file has been read, and this share is simply not in it.
    click_rail_row 1 right
    settle
    menu_seek Remove
    key -k Return >/dev/null
    wait_message "stubshare is not a saved place, and stays on the rail until it is unmounted."
    # Both stat and cat print nothing for a path that is gone, so neither assertion below means
    # anything until the file is known to be there.
    [[ -f "$bookmarks" ]] || fail "unmount: the refused press unlinked the bookmarks file"
    [[ -z "$(cat "$bookmarks")" ]] \
        || fail "unmount: the refused press wrote to the file, it reads: $(cat "$bookmarks")"
    [[ "$(stat -c %y "$bookmarks")" == "$before_press4" ]] \
        || fail "unmount: the refused press rewrote the file, mtime moved from $before_press4"

    printf 'UNMOUNT remove saved-mounted=ok saved-only=ok pressed-again=ok\n'
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
    # PR #21's rule, which this case used to assert the other way round: the operator's own name wins
    # on the live row too, or the rename just typed is written to the file and never drawn again.
    # The live mount still wins the row itself, and its "true" here says so; see ui/js/Mounts.js
    # "railLabel" and ui/NetworkMounts.qml "rebuild".
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == "ISOs Archive|network|share|true"$'\n'"Homelab|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "ISOs Archive|network|share|true"$'\n'"Homelab|network|share|false" ]] \
        || fail "rename: a mounted entry did not take the name just typed, got $(ipc networkEntries)"

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
        [[ "$(ipc networkEntries)" == "ISOs Archive|network|share|true"$'\n'"Homelab|network|share|false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == "ISOs Archive|network|share|true"$'\n'"Homelab|network|share|false" ]] \
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
    # The renderer is stated because src/gui.rs owns that choice and a direct qs launch never runs it.
    QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" PATH="$stub_bin" FLEA_PATH="$dir" FLEA_BIN="$flea_bin" \
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

# The settings panel: its doors, its three control groups, and the one thing a settings window
# has to do that a menu does not, which is outlive the process that wrote it. XDG_STATE_HOME and
# XDG_CONFIG_HOME both point inside the fixture root for the whole case, so nothing here can write
# the operator's own ~/.local/state/flea/ui.json; hard rule 9 covers writes and not only deletes.
case_settings() {
    local dir="$fixture_root/settings"
    local config="$fixture_root/settings-config"
    local state="$fixture_root/settings-state"
    sandbox_scratch "$dir"
    sandbox_scratch "$config"
    sandbox_scratch "$state"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    local real_config="${XDG_CONFIG_HOME-}"
    local real_state="${XDG_STATE_HOME-}"
    export XDG_CONFIG_HOME="$config"
    export XDG_STATE_HOME="$state"
    local stored="$state/flea/ui.json"

    # Seeded through the same CLI the window writes through: a column set the header menu owns, two
    # keys the backend owns and no control in this panel writes, and one key only a newer Flea knows.
    # What keeps them below is src/uistate.rs's merge, not a copy the window happened to be holding.
    settings_seed "$state" "$config" "$stored"

    launch "$dir"
    wait_listing 2

    settings_doors
    settings_display
    settings_menus
    settings_keys

    # One override left standing, so the restart below has a text size to bring back as well.
    key -M ctrl -M shift -k equal -m shift -m ctrl >/dev/null
    settle
    local pinned_base
    pinned_base=$(token_of baseSize)

    # Restart survival, which is what separates a setting from a session's mood. Every value is
    # asserted in the file the panel wrote, again through the backend that owns it, and again in the
    # behaviour of a process that only read it.
    [[ -f "$stored" ]] || fail "settings: the panel wrote no state file at $stored"
    grep -q '"paste"' "$stored" || fail "settings: the hidden action never reached the state file"
    grep -q '"keys": "windows"' "$stored" || fail "settings: the preset never reached the state file"
    grep -q "\"mode\": $pinned_base" "$stored" \
        || fail "settings: the state file holds no ${pinned_base}px stop"
    # The board's own words: an override stores a stop, never a free number or a multiplier, and one
    # stored vocabulary rather than two that would have to be kept in step.
    ! grep -q 'uiScale' "$stored" || fail "settings: the state file still carries an interface-scale multiplier"
    ! grep -q '"px"' "$stored" || fail "settings: the state file still carries 0.1.3's override shape"
    [[ ! -e "$config/flea/view.json" ]] \
        || fail "settings: a second settings file was written at $config/flea/view.json"

    settings_assert_backend "$state" "$config" "$pinned_base"

    kill_flea
    launch "$dir"
    wait_listing 2
    [[ "$(token_of baseSize)" == "$pinned_base" ]] \
        || fail "settings: a restart lost the ${pinned_base}px override, it draws at $(token_of baseSize)"
    key , >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"ruler|Effective|${pinned_base}px"* ]] \
        || fail "settings: a restart brought the panel back on a different stop"
    # settingsRows draws the section the panel is ON and a new process always opens on Display, so
    # the master row is not reachable until the rail has been walked. The master is derived from the
    # stored set, so a restart that read only menu.hidden must still draw the five of six the panel
    # left behind, and the six rows under it must agree with it.
    settings_section menus
    [[ "$(ipc settingsRows)" == *"master|All basic file actions|5 of 6"* ]] \
        || fail "settings: a restart did not derive the master back to five of six, got $(ipc settingsRows)"
    key -k Escape >/dev/null
    settle
    click_row 0 right
    settle
    local reopened="|$(ipc contextMenuEntries)|"
    [[ "$reopened" != *"|Paste|"* ]] || fail "settings: a restart brought the hidden Paste row back"
    [[ "$reopened" == *"|Cut|"* ]] || fail "settings: a restart lost the rows that were left enabled"
    key -k Escape >/dev/null
    settle
    key -M ctrl -k h -m ctrl >/dev/null
    settle
    [[ "$(ipc showHidden)" == "true" ]] || fail "settings: the stored Windows preset did not survive a restart"
    key -M ctrl -k h -m ctrl >/dev/null
    settle

    settings_write_refused "$state" "$pinned_base"

    # Back to following, so nothing after this case runs at a size it did not ask for.
    key -M ctrl -M shift -k 0 -m shift -m ctrl >/dev/null
    settle

    settings_read_refused "$stored" "$dir"

    printf 'SETTINGS doors=ok display=ok menus=ok keys=ok restart=ok backend=ok refused=ok unread=ok\n'
    if [[ -n "$real_config" ]]; then export XDG_CONFIG_HOME="$real_config"; else unset XDG_CONFIG_HOME; fi
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    kill_flea
}

# The state this case starts from, laid down through flea --ui-state so the schema sees it too. The
# unknown key goes in by hand afterwards, because the CLI refuses a key this build does not know.
settings_seed() {
    local state="$1" config="$2" stored="$3"
    env XDG_STATE_HOME="$state" XDG_CONFIG_HOME="$config" "$flea_bin" --ui-state \
        '{"columns":["name","size"],"places":{"sidebarWidth":240},"sort":{"key":"size"}}' >/dev/null \
        || fail "settings: the seeding write through flea --ui-state failed"
    jq '. + {fromANewerFlea: {aKeyThisBuildHasNeverHeardOf: true}}' "$stored" > "$stored.seed" \
        || fail "settings: the newer-Flea key could not be added to the seed"
    mv "$stored.seed" "$stored"
}

# flea --ui-state with no patch is the read half of the one shared path, so this reads the panel's
# own three settings back out of the backend, and every key beside them that nobody here writes.
settings_assert_backend() {
    local state="$1" config="$2" pinned_base="$3" doc
    doc=$(env XDG_STATE_HOME="$state" XDG_CONFIG_HOME="$config" "$flea_bin" --ui-state) \
        || fail "settings: flea --ui-state could not read the state file back"
    settings_backend_holds "$doc" ".display.textSize.mode == $pinned_base" "the ${pinned_base}px stop"
    settings_backend_holds "$doc" '.keys == "windows"' "the Windows preset"
    settings_backend_holds "$doc" '.menu.hidden | index("paste")' "the hidden Paste action"
    # menu.hidden is the sole state: the five of six the panel drew is derived from it, so a second
    # value beside it here would be a value that could disagree with the set the menus actually read.
    settings_backend_holds "$doc" '.menu | has("basic") | not' "menu.hidden alone, with no stored master"
    # The preservation half, and the whole point of one store: four settings writes are four merges,
    # so the retained view state, the backend's own keys and a newer Flea's key are all still here.
    settings_backend_holds "$doc" '.columns == ["name","size"]' "the stored column set"
    settings_backend_holds "$doc" '.places.sidebarWidth == 240' "places.sidebarWidth"
    settings_backend_holds "$doc" '.sort.key == "size"' "sort.key"
    settings_backend_holds "$doc" '.fromANewerFlea.aKeyThisBuildHasNeverHeardOf == true' \
        "the key only a newer Flea knows"
}

settings_backend_holds() {
    local doc="$1" filter="$2" what="$3"
    printf '%s' "$doc" | jq -e "$filter" >/dev/null \
        || fail "settings: the backend does not read $what back, it reads $(printf '%s' "$doc" | jq -c 'del(.places.favourites)')"
}

# The other half of the same honesty: a state file this window could not READ is a window about to
# draw the shipped defaults over the operator's own settings, which is the unchecked-read defect
# ui/NetworkDialog.qml carried once. It has to say so rather than look like a first launch.
settings_read_refused() {
    local stored="$1" dir="$2" before_sha before_ino
    # Before the reading, because the window this case has been driving still owns a writer, and a
    # patch that landed between the sha below and the chmod would read as this block's own damage.
    kill_flea
    before_sha=$(sha256sum "$stored" | cut -d' ' -f1)
    before_ino=$(stat -c '%i' "$stored")
    chmod 000 "$stored" || fail "settings: the state file could not be made unreadable"
    launch "$dir"
    wait_listing 2
    [[ "$(ipc lastMessage)" == "Your saved settings could not be read, so these are the defaults." ]] \
        || fail "settings: an unreadable state file was not reported, the status bar says $(ipc lastMessage)"
    # And the write half of that same file, one keystroke away: the window is holding the shipped
    # defaults, so a patch that went ahead would rename a full default document over every key in it.
    key -M ctrl -M shift -k minus -m shift -m ctrl >/dev/null
    settle
    [[ "$(ipc lastMessage)" == "That setting could not be saved." ]] \
        || fail "settings: a save onto an unreadable state file was not reported, the status bar says $(ipc lastMessage)"
    kill_flea
    chmod 600 "$stored" || fail "settings: the state file could not be made readable again"
    [[ "$(sha256sum "$stored" | cut -d' ' -f1)" == "$before_sha" ]] \
        || fail "settings: a save onto an unreadable state file spent the operator's bytes"
    [[ "$(stat -c '%i' "$stored")" == "$before_ino" ]] \
        || fail "settings: a save onto an unreadable state file renamed a new file over it"
}

# A failed write is reported, never swallowed. The state directory is made unwritable, so the temp
# file src/uistore.rs renames into place cannot be created at all, and the panel's next change is a
# change the file does not have. The user is told that in the one place Flea says things.
settings_write_refused() {
    local state="$1" pinned_base="$2" before refused_base retried_base
    before=$(cat "$state/flea/ui.json")
    chmod 500 "$state/flea" || fail "settings: the state directory could not be made read-only"
    key , >/dev/null
    settle
    # The stop row is Display's, and the panel reopens on whatever section the last block left it on.
    settings_section display
    key j >/dev/null
    settle
    key h >/dev/null
    settle
    refused_base=$(token_of baseSize)
    (( refused_base < pinned_base )) \
        || fail "settings: the refused step did not move the size on screen, still $refused_base"
    [[ "$(ipc lastMessage)" == "That setting could not be saved." ]] \
        || fail "settings: a refused write was not reported, the status bar says $(ipc lastMessage)"
    chmod 700 "$state/flea" || fail "settings: the state directory could not be made writable again"
    [[ "$(cat "$state/flea/ui.json")" == "$before" ]] \
        || fail "settings: a refused write changed the state file anyway"
    grep -q "\"mode\": $pinned_base" "$state/flea/ui.json" \
        || fail "settings: the state file did not keep the stop the refusal could not replace"
    # The book must not have believed the refusal: the next step still writes, and lands.
    key h >/dev/null
    settle
    retried_base=$(token_of baseSize)
    (( retried_base < refused_base )) \
        || fail "settings: the step after a refusal did not move the size, still $retried_base"
    grep -q "\"mode\": $retried_base" "$state/flea/ui.json" \
        || fail "settings: the step after a refusal never reached the state file"
    key -k Escape >/dev/null
    settle
}

# All three doors the Settings board draws: the comma key from either view, the toolbar's sliders
# button, and the Settings row on the background menu.
settings_doors() {
    key , >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "settings: the comma key did not open the panel"
    [[ "$(ipc settingsSection)" == "display" ]] \
        || fail "settings: the panel did not open on Display, it is on $(ipc settingsSection)"
    shot settings-display
    key -k Escape >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "false" ]] || fail "settings: Escape did not close the panel"

    local wx wy ww wh bx by
    read -r wx wy ww wh < <(window_box)
    read -r bx by <<< "$(ipc chromeButtonCentre sliders)"
    [[ -n "$by" ]] || fail "settings: the chrome strip has no sliders button"
    omarchy-drive click "$((wx + bx))" "$((wy + by))" left >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "settings: the sliders button did not open the panel"
    key -k Escape >/dev/null
    settle

    # The third door: the background menu's own Settings row. SettingsMenus.html's table gives it to
    # that column alone, so a row's menu offering one would be a fourth door the board denies.
    click_row 0 right
    settle
    [[ "|$(ipc contextMenuEntries)|" != *"|Settings|"* ]] \
        || fail "settings: a row's own menu offered a Settings row, which the board gives the background alone"
    key -k Escape >/dev/null
    settle
    menu_click "Settings"
    [[ "$(ipc settingsOpen)" == "true" ]] \
        || fail "settings: the background menu's Settings row did not open the panel"
    shot settings-from-background
    key -k Escape >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "false" ]] || fail "settings: Escape did not close the panel again"
}

# The Display section, whose consumer is ui/Theme.qml. The board rules that Omarchy owns the size
# until Flea is told otherwise, that an override takes one of seven stops and not a free number, and
# that the monitor scale is read-only. Every stop is walked and its whole token row is read back off
# the live seam against the board's own layout table, because the table is the contract.
settings_display() {
    key , >/dev/null
    settle
    local omarchy_base
    omarchy_base=$(token_of baseSize)
    [[ "$(ipc settingsRows)" == *"choice|Text size|Follow Omarchy"* ]] \
        || fail "settings: Display did not open on Follow Omarchy, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" == *"ruler|Effective|${omarchy_base}px"* ]] \
        || fail "settings: the ruler does not report Omarchy's own ${omarchy_base}px"
    # Read-only means read-only: the compositor's two rows are facts, and no control sits on them.
    [[ "$(ipc settingsRows)" == *"fact|Scale|"* ]] \
        || fail "settings: the Display section draws no monitor scale, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" != *"choice|Scale|"* ]] \
        || fail "settings: the monitor scale is a control, and the board says Flea never steps it"
    assert_monitor_scale_row
    shot settings-text-follow

    # Switching to Override changes the mode and nothing on screen, which is what makes the switch
    # safe to press: only a step moves the type.
    local before after
    before=$(ipc metrics)
    key -k Return >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"choice|Text size|Override"* ]] \
        || fail "settings: Enter on the mode row did not reach Override, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" == *"ruler|Effective|${omarchy_base}px"* ]] \
        || fail "settings: the override did not start on Omarchy's own stop"
    [[ "$(ipc metrics)" == "$before" ]] \
        || fail "settings: switching to Override moved the type before any step, $before then $(ipc metrics)"

    # Down onto the stop row, then the whole list, each stop checked against the board's table.
    key j >/dev/null
    settle
    settings_walk_to_stop 9
    local stop
    for stop in 9 10 11 12 14 16 20; do
        settings_walk_to_stop "$stop"
        assert_board_row "$stop"
    done
    after=$(ipc metrics | cut -d' ' -f1)
    (( after > $(cut -d' ' -f1 <<< "$before") )) \
        || fail "settings: the largest stop did not grow the type past Omarchy's own size"
    shot settings-text-override

    # The way back is one row, and it puts every token where Omarchy had it.
    key k >/dev/null
    settle
    key -k Return >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"choice|Text size|Follow Omarchy"* ]] \
        || fail "settings: the mode row did not go back to Follow Omarchy"
    [[ "$(ipc settingsRows)" == *"ruler|Effective|${omarchy_base}px"* ]] \
        || fail "settings: following Omarchy again left the ruler on the override's stop"
    [[ "$(ipc metrics)" == "$before" ]] \
        || fail "settings: following Omarchy again did not put the type back, $before then $(ipc metrics)"
    key -k Escape >/dev/null
    settle

    settings_chord_alias "$omarchy_base"
}

# The chord is an alias, not a second engine: keys.toml binds textSizeUp, textSizeDown and
# textSizeReset, and each one has to move the very state the panel's own rows show.
settings_chord_alias() {
    local omarchy_base="$1"
    key -M ctrl -M shift -k equal -m shift -m ctrl >/dev/null
    settle
    local grown
    grown=$(token_of baseSize)
    (( grown > omarchy_base )) \
        || fail "settings: Ctrl+Shift+Plus did not grow the text size, still $grown"
    [[ "$(ipc lastMessage)" == "Text size ${grown}px. Ctrl+Shift+0 follows Omarchy again." ]] \
        || fail "settings: the chord did not announce its stop, got $(ipc lastMessage)"
    key , >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"ruler|Effective|${grown}px"* ]] \
        || fail "settings: the panel does not show the stop the chord set, got $(ipc settingsRows)"
    key -k Escape >/dev/null
    settle
    key -M ctrl -M shift -k minus -m shift -m ctrl >/dev/null
    settle
    [[ "$(token_of baseSize)" == "$omarchy_base" ]] \
        || fail "settings: Ctrl+Shift+Minus did not step back one stop"
    key -M ctrl -M shift -k 0 -m shift -m ctrl >/dev/null
    settle
    [[ "$(ipc lastMessage)" == "Text size follows Omarchy, ${omarchy_base}px." ]] \
        || fail "settings: Ctrl+Shift+0 did not announce following Omarchy, got $(ipc lastMessage)"
    key , >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"choice|Text size|Follow Omarchy"* ]] \
        || fail "settings: the chord's reset did not reach the panel's own mode row"
    key -k Escape >/dev/null
    settle
}

# h and l walk the stop row. The floor and the ceiling clamp, so pressing past either is a no-op
# rather than a wrap, and this walks far enough to reach any stop from any other.
settings_walk_to_stop() {
    local want="$1" step=h attempt
    (( want > $(token_of baseSize) )) && step=l
    for attempt in 1 2 3 4 5 6 7; do
        [[ "$(token_of baseSize)" == "$want" ]] && return 0
        key "$step" >/dev/null
        settle
    done
    [[ "$(token_of baseSize)" == "$want" ]] \
        || fail "settings: seven steps did not reach the ${want}px stop, stopped at $(token_of baseSize)"
}

# One key of Theme.tokens(), which is the live seam tools/flea-metrics-gate diffs.
token_of() {
    ipc tokens | grep "^$1=" | cut -d= -f2-
}

# The SettingsScale board's layout table, base|bodySmall|caption|paddingY|rowHeight|iconSize|mark.
# mark is the board's own unrounded number rounded to whole pixels, which is what Theme draws.
assert_board_row() {
    local want_base="$1" row got
    for row in "9|8|7|5|24|14|12" "10|9|8|5|26|16|13" "11|10|9|6|30|18|15" \
               "12|11|10|6|32|20|16" "14|13|12|7|37|23|19" "16|15|13|8|43|27|22" \
               "20|18|17|10|52|32|26"; do
        IFS='|' read -r base body caption padding height icon mark <<< "$row"
        [[ "$base" == "$want_base" ]] || continue
        got="$(token_of baseSize)|$(token_of bodySmall)|$(token_of caption)|$(token_of rowPaddingY)|$(token_of rowHeight)|$(token_of iconSize)|$(token_of markSize)"
        printf 'SETTINGS stop=%s tokens=%s\n' "$base" "$got"
        [[ "$got" == "$base|$body|$caption|$padding|$height|$icon|$mark" ]] \
            || fail "settings: the ${base}px stop draws $got, and the board's table says $base|$body|$caption|$padding|$height|$icon|$mark"
        return 0
    done
    fail "settings: ${want_base}px is not a stop the board tabulates"
}

# The compositor's own number, read the same way ui/Theme.qml reads it, so the row cannot show a
# scale Hyprland is not on and cannot quietly read "not reported" on a box that answers.
assert_monitor_scale_row() {
    local live shown
    live=$(hyprctl monitors -j | jq -r 'map(select(.focused)) | .[0].scale // empty')
    [[ -n "$live" ]] || fail "settings: hyprctl reports no focused monitor, so the row has no contract"
    shown=$(awk -v s="$live" 'BEGIN { printf "%g", s + 0 }')
    [[ "$(ipc settingsRows)" == *"fact|Scale|${shown}x"* ]] \
        || fail "settings: the Scale row does not show the compositor's ${shown}x, got $(ipc settingsRows)"
}

# The Menus section, whose consumer is ui/js/Menu.js: every assertion here is made against the real
# context menu, never against the stored set alone.
settings_menus() {
    key , >/dev/null
    settle
    settings_section menus
    [[ "$(ipc settingsRows)" == *"master|All basic file actions|6 of 6"* ]] \
        || fail "settings: the master row does not start at six of six, got $(ipc settingsRows)"
    shot settings-menus

    # Down three from the master is Paste, and Space is the board's own toggle key.
    key j >/dev/null; key j >/dev/null; key j >/dev/null
    settle
    key -k Space >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"master|All basic file actions|5 of 6"* ]] \
        || fail "settings: switching one action off did not read as five of six"
    key -k Escape >/dev/null
    settle
    settings_menu_lacks "Paste"
    [[ "|$(ipc contextMenuEntries)|" == *"|Cut|Copy|Duplicate|"* ]] \
        || fail "settings: hiding Paste moved the rows around it, got $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    # The master itself: a partial one enables all six, and a checked one switches all six off.
    key , >/dev/null
    settle
    key -k Space >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"master|All basic file actions|6 of 6"* ]] \
        || fail "settings: activating the partial master did not switch all six on"
    key -k Space >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"master|All basic file actions|0 of 6"* ]] \
        || fail "settings: activating the checked master did not switch all six off"
    key -k Escape >/dev/null
    settle
    local label
    for label in Cut Copy Paste Duplicate Rename "Move to Trash"; do
        settings_menu_lacks "$label"
        key -k Escape >/dev/null
        settle
    done
    click_row 0 right
    settle
    [[ "|$(ipc contextMenuEntries)|" == *"|Open|"* ]] || fail "settings: the locked Open row went with them"
    [[ "|$(ipc contextMenuEntries)|" == *"|Show hidden files|"* ]] \
        || fail "settings: the locked hidden toggle went with them"
    key -k Escape >/dev/null
    settle

    # Back to all six, then off with Paste alone, which is the state the restart check reads back.
    key , >/dev/null
    settle
    key -k Space >/dev/null
    settle
    key j >/dev/null; key j >/dev/null; key j >/dev/null
    key -k Space >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"master|All basic file actions|5 of 6"* ]] \
        || fail "settings: the panel did not end the Menus block with Paste alone switched off"
    key -k Escape >/dev/null
    settle
}

# The rail walk to a named section, with the panel already open, from wherever it was last left. The
# section outlives a close, so a block that needs one says so rather than inheriting it: leaving the
# panel on Menus after the restart check sent the whole refusal block's h presses to a menu row.
# ui/SettingsPanel.qml clamps the rail rather than wrapping it, so two k presses reach the top row
# from any of the three and j walks down from there.
settings_section() {
    local want="$1" down step
    case "$want" in
        keys) down=0 ;;
        display) down=1 ;;
        menus) down=2 ;;
        *) fail "settings: $want is not a rail section" ;;
    esac
    key -k Tab >/dev/null
    settle
    [[ "$(ipc settingsSide)" == "rail" ]] || fail "settings: Tab did not give the cursor to the rail"
    key k >/dev/null
    key k >/dev/null
    settle
    [[ "$(ipc settingsSection)" == "keys" ]] \
        || fail "settings: two k presses did not reach the top of the rail, it is on $(ipc settingsSection)"
    for (( step = 0; step < down; step++ )); do
        key j >/dev/null
        settle
    done
    [[ "$(ipc settingsSection)" == "$want" ]] \
        || fail "settings: the rail did not reach $want, it is on $(ipc settingsSection)"
    key -k Tab >/dev/null
    settle
}

# Opens the row menu and refuses a label that should not be in it, delimiters included so Copy path
# cannot answer for Copy.
settings_menu_lacks() {
    local label="$1"
    click_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "settings: the row menu did not open"
    [[ "|$(ipc contextMenuEntries)|" != *"|$label|"* ]] \
        || fail "settings: $label is still in the menu, got $(ipc contextMenuEntries)"
}

# The Mac/Windows toggle, proved by the keys themselves: a chord one preset binds and the other
# does not, driven through the real window in both states.
settings_keys() {
    key , >/dev/null
    settle
    settings_section keys
    [[ "$(ipc settingsRows)" == *"choice|Keybinding preset|Mac"* ]] \
        || fail "settings: the preset row does not start on Mac, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" == *"fact|connect to server|ctrl-k"* ]] \
        || fail "settings: the Mac preset lists none of its own chords"
    shot settings-keys
    key l >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"choice|Keybinding preset|Windows"* ]] \
        || fail "settings: l did not step the preset to Windows"
    [[ "$(ipc settingsRows)" == *"fact|hidden files|ctrl-h"* ]] \
        || fail "settings: the Windows preset lists none of its own chords"
    key -k Escape >/dev/null
    settle

    # The preset rebinds in this process at once, which is the whole point of a toggle over one map.
    [[ "$(ipc showHidden)" == "false" ]] || fail "settings: the fixture did not start with hidden files off"
    key -M ctrl -k h -m ctrl >/dev/null
    settle
    [[ "$(ipc showHidden)" == "true" ]] || fail "settings: Ctrl+H is not bound under the Windows preset"
    key -M ctrl -k h -m ctrl >/dev/null
    settle
    [[ "$(ipc showHidden)" == "false" ]] || fail "settings: Ctrl+H did not toggle back"
    # And Finder's own chord goes quiet, which is what makes this a preset and not an addition.
    [[ "$(ipc viewMode)" == "list" ]] || fail "settings: the fixture did not start in the list view"
    key -M ctrl -k 2 -m ctrl >/dev/null
    settle
    [[ "$(ipc viewMode)" == "list" ]] \
        || fail "settings: Ctrl+2 still switched the view under the Windows preset"
    key -M ctrl -M shift -k 2 -m shift -m ctrl >/dev/null
    settle
    [[ "$(ipc viewMode)" == "columns" ]] \
        || fail "settings: Explorer's own Ctrl+Shift+2 did not reach the columns view"
    key -M ctrl -M shift -k 1 -m shift -m ctrl >/dev/null
    settle
    [[ "$(ipc viewMode)" == "list" ]] || fail "settings: Ctrl+Shift+1 did not go back to the list view"
}

cache_snapshot
trap cleanup EXIT

declare -a wanted=("$@")
[[ ${#wanted[@]} -eq 0 ]] && wanted=(cursor terminal open rows click menu background hidden selection select colour lifted icons thumbs hashcache stale nosweep oem header overflow focus preview network netmark networkauth networktimeout gvfs sharebrowser unmount eject rename renamelife taildrop grid columns operations tabs openterminal renderer settings hangshare)

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
# case_network makes its own bookmarks file unreadable on purpose, and Quickshell correctly reports
# that it cannot watch a file it cannot read. This drops that one line and nothing else: the path
# carries this run's own pid and names one fixture home, so no product warning can ever match it.
# The reader has no -q, so it drains the pipe and takes no SIGPIPE; pipefail then reports its own
# status, which is what says whether anything but that one line matched.
expected_warning="inotify_add_watch($fixture_root/network-home/.config/gtk-3.0/bookmarks) failed: (Permission denied)"
if grep -F -v "$expected_warning" "$run_log" | grep -E 'WARN|ERROR|TypeError|ReferenceError|Cannot open'; then
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
