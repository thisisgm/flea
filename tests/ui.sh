#!/usr/bin/env bash
# Drives the real Quickshell window with omarchy-drive and asserts through the read-only IPC seam.
# Usage: ./tests/ui.sh [cursor|terminal|open|rows|click|ctrlclick|viewrestart|dd|sortrestart|dirsortstale|editplace|mute|placemenu|runscript|unmounted|sidebar|menu|hidden|selection|watch|optical|select|colour|lifted|icons|thumbs|hashcache|stale|nosweep|oem|header|overflow|focus|preview|network|netmark|networktimeout|networklive|gvfs|sharebrowser|unmount|phones|eject|rename|renamelife|taildrop|grid|columns|operations|tabs|openterminal|renderer|settings|makedefault|noblank ...]; networklive is opt-in.
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
# 1500 detents are 432,000 px at this box's 288 px notch: past the end of the two 200-row cases (thumbs, renamelife) many times over, and about 11,700 of nosweep's 100,000 rows.
fling_clicks=1500
# 15 shots 0.2 s apart span one 2800 ms replay of ui/FleaMark.qml's draw, whose mark is lit from about 0.4 s to 2.8 s of it.
mark_poll_shots=15
mark_poll_s=0.2
# The backend's own DRAIN_LIMIT is 25 s, so anything alive past this is wedged rather than draining.
drain_wait_s=30
# Hard rule 9 covers writes, not only deletes: an overridable path that is truncated or written into
# is the same hazard as one that is deleted, so both of these are pinned rather than taken from the
# environment. Neither override had a caller.
run_root=$(mktemp -d /tmp/flea-ui-run.XXXXXXXX) || fail "cannot create native evidence sandbox"
printf 'flea native evidence\n' > "$run_root/.flea-test-sandbox"
export FLEA_TEST_RUN_ROOT="$run_root"
evidence_dir="$run_root/evidence"
# Quickshell truncates nothing, so each case gets a fresh log and every log lands in the run log.
flea_log="$run_root/flea.log"
run_log="$run_root/run.log"
# One case's own output, re-read for the refusal check rather than piped. Pid-scoped like every
# fixture root here, because two runs sharing it would read each other's output, and truncated before
# each case because a failed redirect would otherwise leave the previous case's bytes for the
# refusal grep to find and report a refusal for a case that never ran.
case_log="$run_root/case.log"
expected_warnings="$run_root/expected-warnings"
: > "$expected_warnings"
printf 'NATIVE_EVIDENCE_ROOT=%s\n' "$run_root"

# Every window this suite launches writes its settings back, so without a state home of its own the
# run edits the operator's real ui.json: case_views left "view":"columns" in it and case_cursor,
# which seeds nothing, then opened the 100k listing in the columns view and could not scroll a list.
# A case that wants particular settings still seeds its own through seed_ui_state.
suite_state="$run_root/state"
mkdir -p "$suite_state" || fail "the suite state home could not be created at $suite_state"
export XDG_STATE_HOME="$suite_state"

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
# The rule is the house hairline, foreground at 12 percent, so a crumb glyph under it shows through: measured 2 of 255 on this box, against 23 for the surface an opaque fill would expose in its place.
chrome_edge_max_spread=8
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
    omarchy-drive ipc -p "$flea_ui/boot" flea "$@"
}

# Window class used by every preflight, focus check and injected keystroke.
flea_window_class=com.thisisgm.flea

settle() {
    sleep "$settle_s"
}

flea_pids() {
    local pid pids process result=0
    pids=$(pgrep -x qs) || result=$?
    (( result <= 1 )) || return "$result"
    for pid in $pids; do
        process=$(flea_process_dir "$pid") || return 3
        [[ -r "$process/cmdline" ]] || continue
        # Redirections apply left to right, so the silencer has to precede the read it is silencing.
        if tr '\0' ' ' 2>/dev/null < "$process/cmdline" | grep -Fq "$flea_ui"; then
            printf '%s\n' "$pid"
        fi
    done
}

flea_process_dir() { printf '/proc/%s\n' "$1"; }

# Return 0 for this run, 1 for foreign, 2 for vanished, and 3 when a live process cannot be inspected.
flea_process_owned() {
    local process environment
    [[ "$1" =~ ^[0-9]+$ ]] || return 1
    process=$(flea_process_dir "$1") || return 3
    [[ -d "$process" ]] || return 2
    [[ -O "$process" ]] || return 1
    # The environ of a process this run started is unreadable for the moment it spends in exec and
    # for as long as it stays unreaped, so a single failed read is not evidence of a foreign process.
    local attempt read=0
    for attempt in 1 2 3 4; do
        environment=$(tr '\0' '\n' 2>/dev/null < "$process/environ") && { read=1; break; }
        [[ -d "$process" ]] || return 2
        # Sample /proc/PID/stat: "347 (gio) Z 1 347 ...", so the state is the field after the ")".
        [[ "$(sed 's/.*) //' "$process/stat" 2>/dev/null | cut -d' ' -f1)" == Z ]] && return 2
        sleep 0.05
    done
    (( read )) || return 3
    grep -Fx "FLEA_TEST_RUN_ROOT=$run_root" <<< "$environment" >/dev/null
}

# Any Flea from this checkout that we did not start, captured once before anything is killed. The
# operator works at this box, and flea_pids cannot tell their window from ours: both match "$flea_ui".
foreign_pids=$(flea_pids | tr '\n' ' ') || fail "cannot enumerate native windows before launch"
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
    local pid pids process result=0
    pids=$(pgrep -x flea) || result=$?
    (( result <= 1 )) || return "$result"
    for pid in $pids; do
        process=$(flea_process_dir "$pid") || return 3
        [[ -r "$process/cmdline" ]] || continue
        if tr '\0' ' ' 2>/dev/null < "$process/cmdline" | grep -Fq -- "$flea_bin --backend"; then
            if flea_process_owned "$pid"; then printf '%s\n' "$pid"
            else result=$?; (( result != 3 )) || return 3; fi
        fi
    done
}

flea_pid() {
    local found
    local -a pids
    found=$(flea_pids) || fail "cannot enumerate native window processes"
    pids=()
    [[ -z "$found" ]] || mapfile -t pids <<< "$found"
    [[ ${#pids[@]} -eq 1 ]] || fail "expected one exact Flea qs pid, got ${#pids[@]}"
    printf '%s\n' "${pids[0]}"
}

owned_trash_monitors() {
    local pid pids process result=0
    pids=$(pgrep -x gio) || result=$?
    (( result <= 1 )) || return "$result"
    for pid in $pids; do
        process=$(flea_process_dir "$pid") || return 3
        if flea_process_owned "$pid"; then
            if tr '\0' '\n' < "$process/environ" | grep -Fx "FLEA_BIN=$flea_bin" >/dev/null \
                && tr '\0' '\n' < "$process/environ" | grep -F "FLEA_PATH=$fixture_root/" >/dev/null; then
                printf '%s\n' "$pid"
            fi
        else
            result=$?
            (( result != 3 )) || return 3
        fi
    done
}

kill_flea() {
    local pid pids found waited ownership deadline=$((SECONDS + drain_wait_s))
    [[ "$run_root" == /* && -f "$run_root/.flea-test-sandbox" ]] || fail "native process ownership root is missing"
    pids=$(flea_pids) || fail "cannot enumerate native windows for teardown"
    for pid in $pids; do
        [[ " $foreign_pids " == *" $pid "* ]] && continue
        if flea_process_owned "$pid"; then
            kill "$pid" || {
                [[ ! -d "$(flea_process_dir "$pid")" ]] || fail "could not stop owned native window $pid"
            }
        else
            ownership=$?
            (( ownership == 2 )) || fail "refusing to signal unowned or unreadable native window $pid"
        fi
    done
    while :; do
        found=0
        pids=$(flea_pids) || fail "cannot enumerate native windows while draining"
        for pid in $pids; do
            [[ " $foreign_pids " == *" $pid "* ]] && continue
            if flea_process_owned "$pid"; then found=1
            else
                ownership=$?
                (( ownership == 2 )) || fail "refusing to drain unowned or unreadable native window $pid"
            fi
        done
        [[ "$found" -eq 0 ]] && break
        (( SECONDS < deadline )) || fail "owned native window survived for $drain_wait_s s; fixtures kept"
        sleep 0.05
    done
    # Killing qs closes the backend's stdin, and it keeps publishing into the shared cache until its drain ends.
    for waited in $(seq 1 $((drain_wait_s * 20))); do
        found=0
        pids=$(backend_pids) || fail "cannot inspect backend ownership while draining"
        [[ -z "$pids" ]] || found=1
        pids=$(owned_trash_monitors) || fail "cannot inspect Trash monitor ownership while draining"
        [[ -z "$pids" ]] || found=1
        [[ "$found" -eq 0 ]] && return
        sleep 0.05
    done
    fail "an owned backend or Trash monitor survived for $drain_wait_s s after its window closed"
}

# The four roots carry the markers; every per-case directory inside them is a scratch, so a listing
# a case asserts on holds exactly what that case put there.
sandbox_make "$fixture_root"
sandbox_make "$thumb_fixture"
sandbox_make "$hash_fixture"
sandbox_make "$stale_fixture"

cleanup() {
    # fail is an exit that || true cannot catch, so the reap runs in a subshell and its status is re-raised below.
    if ! ( kill_flea ); then
        printf 'FAIL drain at exit; active fixture roots kept: %s\n' "$fixture_root" >&2
        exit 1
    fi
    local root
    for root in "$fixture_root" "$thumb_fixture" "$hash_fixture" "$stale_fixture"; do
        sandbox_remove "$root"
    done
    cache_restore
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
# any other row of the menu; src/uischema.rs DEFAULTS is where the seven come from.
terminal_shown='["delete","moveto","copyto","properties","permissions","copypath"]'
# The shipped set whole, from the same DEFAULTS. A case asserting a menu's exact row list seeds this
# rather than reading whatever the operator has switched off in the Menus section.
menu_shipped='["delete","openTerminal","moveto","copyto","properties","permissions","copypath"]'

launch() {
    local start_path="$1"
    kill_flea
    cat "$flea_log" >> "$run_log" 2>/dev/null || true
    : > "$flea_log"
    FLEA_UI="$flea_ui" FLEA_BIN="$flea_bin" \
        setsid nohup "$flea_bin" --gui "$start_path" >"$flea_log" 2>&1 </dev/null &
    omarchy-drive wait window flea --timeout 15 >/dev/null
    omarchy-drive focus flea >/dev/null
    assert_window
    printf 'LAUNCH path=%q pid=%s\n' "$start_path" "$(flea_pid)"
}

wait_listing() {
    local want_total="$1"
    local total row state
    for _attempt in $(seq 1 300); do
        total=$(ipc total 2>/dev/null || printf unavailable)
        if [[ "$want_total" == 0 ]]; then
            state=$(ipc state 2>/dev/null || printf unavailable)
            if [[ "$total" == 0 && "$state" == empty && "$(ipc listInFlight)" == false ]]; then return; fi
            sleep 0.05
            continue
        fi
        row=$(ipc rowAt 0 2>/dev/null || printf loading)
        # The listing swap keeps the old rows and count up while the next listing is out, so the count alone can match early.
        if [[ "$total" == "$want_total" && "$row" != "loading" && "$(ipc listInFlight 2>/dev/null)" == false ]]; then
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
        total=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea total 2>/dev/null || printf unavailable)
        if [[ "$want_total" == 0 ]]; then
            state=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea state 2>/dev/null || printf unavailable)
            [[ "$total" == 0 && "$state" == empty ]] && return 0
        else
            row=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea rowAt 0 2>/dev/null || printf loading)
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
        seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea path 2>/dev/null || printf unavailable)
        [[ "$seen" == "$want" ]] && return 0
        sleep 0.05
    done
    fail "the pane did not open $want before wall-clock deadline, it is at $seen"
}

find_row_wall() {
    local want="$1" timeout_s="${2:-20}" total=0 seen="" path=unavailable row
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        total=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea total 2>/dev/null || printf 0)
        for ((row = 0; row < total && $(date +%s%3N) < deadline; row++)); do
            seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea rowAt "$row" 2>/dev/null || true)
            if [[ "$seen" == "$want|"* ]]; then
                printf '%s\n' "$row"
                return 0
            fi
        done
        sleep 0.05
    done
    [[ "$total" =~ ^[0-9]+$ ]] || total=-1
    path=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea path 2>/dev/null || printf unavailable)
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

# Waits up to 3 s for scrollbarState .shown to read $1, since the scroller holds 1 s and fades for 0.3 s before it hides.
wait_scrollbar_shown() {
    local want="$1" why="$2" seen
    for _attempt in $(seq 1 60); do
        seen=$(ipc scrollbarState | jq -r '.shown')
        [[ "$seen" == "$want" ]] && return
        sleep 0.05
    done
    fail "scrollbar: $why (shown is $seen)"
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

# j down the rail until the cursor row carries this label. Row positions shift with the box's own
# devices and the operator's Places switches, so no case may count rail rows.
rail_seek() {
    local want="$1" count
    count=$(ipc railCount)
    for _attempt in $(seq 1 "$count"); do
        [[ "$(ipc railLabel "$(ipc railCursor)")" == "$want" ]] && return
        key j >/dev/null
        settle
    done
    fail "rail_seek never reached $want; model is $(ipc railEntries | jq -c 'map(.label)'), drawn is $(ipc railLabels)"
}

# The rail index of a named row. Row positions move with Trash, Favorites and this box's own
# devices, so a case names the row it means and never writes a number.
# Waits for a named rail row to exist. A saved network location is a Favorite now, so a case that
# restarts with nothing mounted waits for that row rather than for a network row that cannot exist.
wait_rail_label() {
    local want="$1" index
    for _attempt in $(seq 1 200); do
        index=$(ipc railEntries | jq -r --arg want "$want" 'map(.label) | index($want) // -1')
        [[ "$index" -ge 0 ]] && return 0
        sleep 0.1
    done
    fail "the rail never carried a row labelled $want; model is $(ipc railEntries | jq -c 'map(.label)')"
}

rail_row_of() {
    local want="$1" index
    index=$(ipc railEntries | jq -r --arg want "$want" 'map(.label) | index($want) // -1')
    [[ "$index" -ge 0 ]] \
        || fail "rail_row_of: no rail row labelled $want; model is $(ipc railEntries | jq -c 'map(.label)'), drawn is $(ipc railLabels)"
    printf '%s' "$index"
}

# The Flea window is tiled here, so a pane coordinate needs its origin added before a click.
window_box() {
    local clients geometry pid expected wx wy width height
    # The driver's windows summary omits PID; native client IPC ties coordinates to this run.
    clients=$(hyprctl clients -j) || fail "cannot inspect native window ownership"
    # hyprctl clients -j: [{"class":"com.thisisgm.flea","pid":123,"at":[12,42],"size":[880,620]}]
    geometry=$(jq -er --arg class "$flea_window_class" '
        [.[] | select(.class == $class)] | select(length == 1) | .[0]
        | [.pid, .at[0], .at[1], .size[0], .size[1]]
        | select(all(.[]; type == "number" and . == floor))
        | select(.[0] > 0 and .[3] > 0 and .[4] > 0) | @tsv' <<< "$clients") \
        || fail "expected exactly one native Flea window with valid geometry"
    read -r pid wx wy width height <<< "$geometry"
    expected=$(flea_pid) || fail "cannot identify the owned native window"
    [[ "$pid" == "$expected" ]] && flea_process_owned "$pid" \
        || fail "refusing coordinates from unowned native window $pid"
    printf '%s %s %s %s\n' "$wx" "$wy" "$width" "$height"
}

click_row() {
    local index="$1"; shift
    local centre cx cy wx wy ww wh
    centre=$(ipc rowCentre "$index")
    [[ -n "$centre" ]] || fail "row $index has no on-screen centre"
    read -r cx cy <<< "$centre"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    # Everything after the index goes straight to omarchy-drive: the button, --double, --mods.
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" "$@" >/dev/null
}

# Steps the menu cursor onto a row by its label rather than by a hardcoded number of Downs, so a
# case survives the operations design's own rows landing between the ones it cares about.
menu_seek() {
    local want="$1" entries target i cursor steps step
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
    steps=$(ipc contextMenuModel | jq -er 'length') || fail "menu_seek: could not read menu inventory"
    for ((step = 0; step <= steps; step++)); do
        cursor=$(ipc contextMenuCursor)
        [[ "$cursor" == "$target" ]] && return 0
        key -k Down >/dev/null
        settle
    done
    fail "menu_seek: could not reach $want, cursor stalled at $(ipc contextMenuCursor)"
}

# The same for an open flyout, whose rows are whatever the backend or the scripts directory offered,
# so no case counts Downs: Sort by is four orders and Run script is however many scripts are there.
menu_seek_submenu() {
    local want="$1" entries target i cursor step
    entries=$(ipc contextMenuSubmenuEntries)
    target=-1
    i=0
    local IFS='|'
    for label in $entries; do
        [[ "$label" == "$want" ]] && { target=$i; break; }
        i=$((i + 1))
    done
    unset IFS
    [[ "$target" -ge 0 ]] || fail "menu_seek_submenu: no row labelled $want in $entries"
    for ((step = 0; step <= i; step++)); do
        cursor=$(ipc menuState | jq -er '.submenuCursor') \
            || fail "menu_seek_submenu: could not read the flyout cursor"
        [[ "$cursor" == "$target" ]] && return 0
        key -k Down >/dev/null
        settle
    done
    fail "menu_seek_submenu: could not reach $want in $entries"
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
    (( best_gap <= $(ipc fileRowHeight) / 2 )) && printf '%s' "$best"
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
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" right >/dev/null
}

# "Kind=PNG image|Size=346 B" becomes "Kind|Size": the labels are the canvas's contract, and the
# values move with the fixture.
fact_labels() {
    printf '%s' "$1" | tr '|' '\n' | cut -d= -f1 | paste -sd'|' -
}

# Walks the cursor to a row by name, from the top, so no case depends on an index the sort could move.
seek_row_named() {
    local want="$1" i n
    n=$(ipc total)
    [[ "$n" =~ ^[0-9]+$ ]] && (( n > 40 )) || n=40
    key g >/dev/null
    for i in $(seq 1 "$n"); do
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
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
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
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
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
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" "$button" >/dev/null
}

# GM's contract, measured and not recomputed: the NETWORK "+" ink, its hit target and the rail's own
# indicator dot share one x centre. The three boxes come from ui/boot/shell.qml's boxOf, in window
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
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
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
        # 3 s, not 1: one ipc round trip costs hundreds of ms and grows under load, and a call that
        # times out returns nothing, which spends a sample of a sentence that stands for only 4 s.
        seen=$(timeout 3 omarchy-drive ipc -p "$flea_ui/boot" flea lastMessage 2>/dev/null || true)
        if [[ "$seen" == "$want" ]]; then
            return 0
        fi
    done
    fail "the status bar never said: $want (the last thing it said was: $seen)"
}

# Durable mount state does not disappear with the status bar, so live network checks wait on it.
wait_network_result() {
    local want="$1" timeout_s="${2:-40}" seen=""
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea networkResult 2>/dev/null || true)
        [[ "$seen" == "$want" ]] && return 0
        sleep 0.1
    done
    fail "network result never became $want (last: ${seen:-unavailable})"
}

# networkResult is a state machine value that moves on; the sentence the dialog shows is what the
# operator reads and what the assertions below check, so a terminal verdict is waited for by name.
wait_network_status() {
    local want="$1" timeout_s="${2:-40}" seen=""
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        seen=$(timeout 2 omarchy-drive ipc -p "$flea_ui/boot" flea networkStatus 2>/dev/null || true)
        [[ "$seen" == "$want" ]] && return 0
        sleep 0.1
    done
    fail "the network dialog never said: $want (last: ${seen:-unavailable})"
}

wait_network_entry_state() {
    local want="$1" timeout_s="${2:-20}" seen=""
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea networkEntries 2>/dev/null || true)
        [[ "$seen" == *"|network|share|$want" ]] && return 0
        sleep 0.1
    done
    fail "network row never became mounted=$want (last: ${seen:-unavailable})"
}

# A successful IPC must show the exact live row gone; an error is not an empty network group.
wait_network_entry_absent() {
    local row="$1" mount_uri="$2" timeout_s="${3:-20}" seen="" status=0 line present
    local deadline=$(( $(date +%s%3N) + timeout_s * 1000 ))
    while (( $(date +%s%3N) < deadline )); do
        if seen=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea networkEntries 2>/dev/null); then
            status=0
            present=false
            # networkEntries row: SFTP|network|share|true
            while IFS= read -r line; do
                if [[ "${line%|*}" == "${row%|*}" ]]; then
                    present=true
                    break
                fi
            done <<< "$seen"
            [[ "$present" == false ]] && return 0
        else
            status=$?
        fi
        sleep 0.1
    done
    [[ "$status" -eq 0 ]] || fail "network row disappearance IPC failed for $mount_uri (status $status)"
    fail "network row $row for $mount_uri survived unmount (last: ${seen:-empty})"
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
# The count is printed as a plain integer: magick writes a million pixels as 1.25604e+06, which bash arithmetic refuses.
count_pixels() {
    local png="$1" geometry="$2" expression="$3"
    magick "$png" -crop "$geometry" +repage -fx "$expression ? 1.0 : 0.0" \
        -format "%[fx:int(mean*w*h+0.5)]" info: | awk '{printf "%d\n", $1}'
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

# A fresh run owns its evidence directory; never replace an earlier screenshot and mistake it for new proof.
shot() {
    local name="$1" png="$evidence_dir/$1.png"
    mkdir -p "$evidence_dir"
    [[ ! -e "$png" && ! -L "$png" ]] || fail "shot: refusing existing evidence $png"
    omarchy-drive shot "$png" flea >/dev/null || fail "shot: omarchy-drive shot failed for $name"
    [[ -s "$png" ]] || fail "shot: $png is missing or empty after a capture that reported success"
    printf 'SHOT %s\n' "$png"
}

# Catches the wheel handler losing its wiring, its sign or its rate: one notch over the list moves
# ListView.contentY by exactly the platform's lines times Theme.scroll.notchPx times the multiplier,
# 3 x 24 x 4 on this box, and a notch at the top moves nothing and stays inside the bounds.
case_scroll() {
    [[ -d "$bench_dir" ]] || fail "the 100,000-file fixture is missing at $bench_dir"
    launch "$bench_dir"
    wait_listing 100000
    settle
    local wx wy ww wh cx cy before after
    [[ "$(ipc wheelLines)" == "3" ]] || fail "scroll: the platform reports $(ipc wheelLines) lines a notch, this case assumes 3"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    read -r cx cy <<< "$(ipc rowCentre 5)"
    # omarchy-drive scroll takes no point: warp there, then one uinput pixel so Qt sees a pointer frame.
    hyprctl dispatch "hl.dsp.cursor.move({x = $((wx + cx - 1)), y = $((wy + cy))})" >/dev/null
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
    settle
    before=$(ipc listContentY)
    [[ "$before" == "0" ]] || fail "scroll: the list did not start at the top, contentY $before"
    omarchy-drive scroll up 1 >/dev/null
    settle
    [[ "$(ipc listContentY)" == "0" ]] || fail "scroll: a notch up at the top moved contentY to $(ipc listContentY)"
    omarchy-drive scroll down 1 >/dev/null
    settle
    after=$(ipc listContentY)
    [[ "$after" == "288" ]] || fail "scroll: one notch down moved contentY to $after, not 288 (3 lines x 24 px x 4)"
    omarchy-drive scroll down 2 >/dev/null
    settle
    after=$(ipc listContentY)
    [[ "$after" == "864" ]] || fail "scroll: two more notches moved contentY to $after, not 864"
    printf 'SCROLL one notch 288, three notches 864, top held at 0\n'
    shot scroll-three-notches
}

# The scrollbar is a viewport control over the integer model, not a second model: a short listing
# draws none, a scale listing draws one, and a real track press moves the same ListView whose wheel
# path drives window refetch. Switching views then proves the shared control reached all three
# directory surfaces rather than being painted only over the default list.
case_scrollbar() {
    local dir="$fixture_root/scrollbar" state wx wy ww wh sx sy sw sh before after handle travel ratio list_bar
    [[ -d "$bench_dir" ]] || fail "scrollbar: the 100,000-file fixture is missing at $bench_dir"
    sandbox_scratch "$dir"
    : > "$dir/only.txt"
    launch "$dir"
    wait_listing 1
    state=$(ipc scrollbarState)
    jq -e '.visible == false' <<< "$state" >/dev/null \
        || fail "scrollbar: a one-row folder draws a scrollbar: $state"

    launch "$bench_dir"
    wait_listing 100000
    click_chrome list
    settle
    state=$(ipc scrollbarState)
    jq -e '.visible == true and .handle >= 24 and .content > .viewport and .offset == 0' <<< "$state" >/dev/null \
        || fail "scrollbar: the scale listing has no usable top handle: $state"
    # Sample input: scrollbarState .rect prints e.g. `1234 200 12 800`.
    read -r sx sy sw sh <<< "$(jq -r '.rect' <<< "$state")"
    [[ "$sx" =~ ^[0-9]+$ && "$sy" =~ ^[0-9]+$ && "$sw" =~ ^[0-9]+$ && "$sh" =~ ^[0-9]+$ ]] \
        || fail "scrollbar: no scrollbar rect, ipc answered [$sx $sy $sw $sh]"
    read -r wx wy ww wh < <(window_box) || fail "scrollbar: native window coordinates unavailable"
    # Finder's overlay scroller hides at rest: once the load settles, nothing is drawn with the pointer away.
    hyprctl dispatch "hl.dsp.cursor.move({x = $((wx + ww / 2)), y = $((wy + wh / 2))})" >/dev/null
    wait_scrollbar_shown false "the scroller stayed drawn at rest"
    omarchy-drive click "$((wx + sx + sw / 2))" "$((wy + sy + sh - 2))" left >/dev/null
    settle
    state=$(ipc scrollbarState)
    # A press near the bottom of the track jumps there (Finder's jump to the spot clicked), not one page down.
    jq -e '.shown == true and (.offset - ((.rect | split(" ")[3] | tonumber) - .handle) | fabs) <= 1' <<< "$state" >/dev/null \
        || fail "scrollbar: a track press near the bottom did not jump the knob there: $state"
    after=$(ipc listContentY)
    (( after > $(jq -r '.viewport * 2 | ceil' <<< "$state") )) || fail "scrollbar: a track press moved the list only to $after, a page at most"
    jq -e '.knob > 6' <<< "$state" >/dev/null || fail "scrollbar: the knob did not widen with the pointer in the lane: $state"
    # The warp alone sends Qt no motion (see hover_row), so the lane would never learn the pointer left.
    hyprctl dispatch "hl.dsp.cursor.move({x = $((wx + ww / 2)), y = $((wy + wh / 2))})" >/dev/null
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1 \
        || fail "scrollbar: pointer motion out of the lane failed"
    wait_scrollbar_shown false "the scroller stayed drawn after the pointer left and the view stopped"

    key -k Home >/dev/null
    settle
    [[ "$(ipc listContentY)" == 0 ]] || fail "scrollbar: the track press stole keyboard focus"
    state=$(ipc scrollbarState)
    handle=$(jq -r '.handle | floor' <<< "$state")
    travel=$(( (sh - handle) / 2 ))
    hyprctl dispatch "hl.dsp.cursor.move({x = $((wx + sx + sw / 2 - 1)), y = $((wy + sy + handle / 2))})" >/dev/null
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
    # libinput accelerates relative motion about 2x, so halve the rest until it lands; hyprctl cursorpos prints e.g. `1214, 735`.
    local target_y cursor_y cursor_now step
    cursor_now=$(hyprctl cursorpos | tr -d ',' | cut -d' ' -f2)
    [[ "$cursor_now" =~ ^[0-9]+$ ]] || fail "scrollbar: no pointer row from hyprctl cursorpos [$cursor_now]"
    target_y=$(( cursor_now + travel ))
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 \
        || fail "scrollbar: pointer press failed"
    for step in $(seq 1 16); do
        cursor_y=$(hyprctl cursorpos | tr -d ',' | cut -d' ' -f2)
        if [[ ! "$cursor_y" =~ ^[0-9]+$ ]]; then
            YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || true
            fail "scrollbar: no pointer row from hyprctl cursorpos [$cursor_y]"
        fi
        (( cursor_y >= target_y - 1 && cursor_y <= target_y + 1 )) && break
        if ! YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 0 -y "$(( (target_y - cursor_y) / 2 ))" >/dev/null 2>&1; then
            YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || true
            fail "scrollbar: pointer drag failed"
        fi
        sleep 0.05
    done
    # Read while still pressed, so a jump on release (MouseArea onClicked) cannot stand in for the drag.
    settle
    local pressed_ratio
    pressed_ratio=$(ipc scrollbarState | jq -r '.offset / ((.rect | split(" ")[3] | tonumber) - .handle)')
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 \
        || fail "scrollbar: pointer release failed"
    jq -e '. >= 0.45 and . <= 0.55' <<< "$pressed_ratio" >/dev/null \
        || fail "scrollbar: the knob had not followed the drag before the release, at track ratio $pressed_ratio"
    settle
    state=$(ipc scrollbarState)
    ratio=$(jq -r '.offset / ((.rect | split(" ")[3] | tonumber) - .handle)' <<< "$state")
    jq -e '. >= 0.45 and . <= 0.55' <<< "$ratio" >/dev/null \
        || fail "scrollbar: a midpoint drag landed at track ratio $ratio: $state"
    # Home put contentY back to 0 before the drag, and that was asserted, so a number above 0 is the drag's own.
    after=$(ipc listContentY)
    [[ "$after" =~ ^[0-9]+$ ]] && (( after > 0 )) \
        || fail "scrollbar: a midpoint drag moved the handle but the list never scrolled: contentY [$after], $state"
    list_bar=$(ipc scrollbarState)

    click_chrome grid
    settle
    [[ "$(ipc viewMode)" == grid ]] || fail "scrollbar: the chrome button did not switch to the grid, still in $(ipc viewMode)"
    state=$(ipc scrollbarState)
    jq -e '.visible == true' <<< "$state" >/dev/null \
        || fail "scrollbar: the scale grid has no scrollbar: $state"
    [[ "$(jq -r '.rect' <<< "$state")" != "$(jq -r '.rect' <<< "$list_bar")" || "$(jq -r '.content' <<< "$state")" != "$(jq -r '.content' <<< "$list_bar")" ]] \
        || fail "scrollbar: the grid reports the list bar's own rect and content: $state"
    click_chrome columns
    settle
    [[ "$(ipc viewMode)" == columns ]] || fail "scrollbar: the chrome button did not switch to the columns, still in $(ipc viewMode)"
    state=$(ipc scrollbarState)
    jq -e '.visible == true' <<< "$state" >/dev/null \
        || fail "scrollbar: the scale Miller column has no scrollbar: $state"
    [[ "$(jq -r '.rect' <<< "$state")" != "$(jq -r '.rect' <<< "$list_bar")" || "$(jq -r '.content' <<< "$state")" != "$(jq -r '.content' <<< "$list_bar")" ]] \
        || fail "scrollbar: the columns report the list bar's own rect and content: $state"
    printf 'SCROLLBAR short=hidden scale=visible track=jump drag=middle views=list,grid,columns\n'
}

# Catches removing the cursor clamp from ListView.onContentYChanged in ui/Pane.qml.
case_cursor() {
    [[ -d "$bench_dir" ]] || fail "the 100,000-file fixture is missing at $bench_dir"
    launch "$bench_dir"
    wait_listing 100000
    settle
    shot cursor-before-scroll
    local wx wy ww wh burst cursor centre first_row
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
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
    omarchy-drive wait ipc -p "$flea_ui/boot" flea state error --timeout 15 >/dev/null \
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

# The row and menu presentation the boards specify: FleaWindow.html's symlink row, a directory drawn
# without the 0.1.4 trailing slash (0.3.3), and Menus.html's right-aligned key beside every bound row.
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

    [[ "$(ipc rowNameText "$dir_row")" == "subdir" ]] \
        || fail "rows: the directory reads $(ipc rowNameText "$dir_row"), not subdir"
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
    # d, the key the operator presses: Menus.html and the OpenWith overseer board both draw it.
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
    # A real archive, because the archive arm below reads the backend's own index of it and not a name.
    bsdtar -a -c -f "$dir/sample.zip" -C "$dir" target.txt
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
    # Measured row order: bin, subdir, broken, linkdir, linkfile, opened.log, sample.zip, target.txt.
    wait_listing 8

    # Bare l enters a real directory and stays silent when its empty listing has no row.
    seek_row_named subdir
    key l >/dev/null
    wait_path "$dir/subdir"
    omarchy-drive wait ipc -p "$flea_ui/boot" flea state empty --timeout 10 >/dev/null \
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
    [[ "$(ipc total)" == "8" ]] || fail "the listing did not survive Enter on a broken symlink"

    # The operator's 0.1.4 ruling: Enter on an archive opens Flea's own view and hands nothing on,
    # because every archive type this box can name defaults to org.gnome.Nautilus.desktop.
    seek_row_named sample.zip
    key -k Return >/dev/null
    omarchy-drive wait ipc -p "$flea_ui/boot" flea previewState archive --timeout 10 >/dev/null \
        || fail "Enter on an archive left the preview at $(ipc previewState), kind $(ipc previewKind), and the log holds $(cat "$opened")"
    printf 'OPEN archive kind=%q state=%q log=%q\n' "$(ipc previewKind)" "$(ipc previewState)" "$(cat "$opened")"
    shot open-archive
    [[ "$(grep -c OPENED "$opened")" == "1" ]] || fail "Enter on an archive handed $(cat "$opened") to $open_handoff open"
    [[ "$(ipc path)" == "$dir" ]] || fail "Enter on an archive left the directory for $(ipc path)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "Escape left the archive preview open"
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
    # The real route, stubbed too: if anything ever reaches src/terminal.rs this records it instead
    # of opening a terminal on the operator's display, which is what used to leak a window per run.
    {
      printf '#!/bin/sh\n'
      printf '# Sample input: xdg-terminal-exec --dir=/home/flea-sandbox/fixtures/openterminal\n'
      printf 'printf "REAL-TERMINAL %%s\\n" "${1#--dir=}" >> %q\n' "$ran"
      printf 'exit 0\n'
    } > "$dir/bin/xdg-terminal-exec"
    chmod +x "$dir/bin/xdg-terminal-exec"
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
    wait_message "No terminal on this system opened that directory."
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
    # Outside the directory under test on purpose: the stub appends to it on every open, and a write
    # inside the listed folder is an outside change that re-reads it under the clicks below.
    local opened="$fixture_root/click-opened.log"
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
    # Measured row order: bin, subdir, alpha.txt, beta.txt, gamma.txt.
    wait_listing 5
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
    # The double click left the cursor on alpha with nothing marked, which is alpha selected, so the
    # ctrl+click adds gamma to it rather than replacing it; case_ctrlclick drives that rule whole.
    click_row 4 left --mods ctrl
    settle
    [[ "$(ipc selectedIndices)" == "2,4" ]] || fail "click: ctrl+click selected '$(ipc selectedIndices)', not rows 2,4"
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
    click_row 4 left
    settle
    [[ "$(ipc selectedIndices)" == "4" ]] \
        || fail "click: a plain click selected '$(ipc selectedIndices)', not row 4 alone"

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
    wait_listing 5
    # PR 48 (shawnyeager): the climb reselects the directory it left, so the cursor is back on the
    # row that was opened rather than on the first row, and Enter is a round trip.
    [[ "$(ipc rowAt "$(ipc cursor)")" == "subdir|"* ]] \
        || fail "click: the climb left the cursor on $(ipc rowAt "$(ipc cursor)" | cut -d'|' -f1), not the directory it came out of"

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
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    # The status is read, because a click that never reached the compositor leaves the path
    # unchanged too and would satisfy both assertions below without pressing anything.
    omarchy-drive click "$((ex + wx))" "$((ey + wy))" >/dev/null \
        || fail "click: omarchy-drive refused the press on the elision marker"
    # Two settles, so a press that navigated after all would have landed before the reading.
    settle
    settle
    printf 'CLICK elision path=%q barOpen=%s\n' "$(ipc path)" "$(ipc pathBarOpen)"
    shot click-elision
    [[ "$(ipc path)" == "$deep" ]] || fail "click: a tap on the elision marker navigated to $(ipc path)"
    [[ "$(ipc pathBarOpen)" == "false" ]] || fail "click: a tap on the elision marker opened the path bar"

    # The marker is one of keys.toml's inert segments, so the double click a tap on it never answers types the path.
    omarchy-drive click "$((ex + wx))" "$((ey + wy))" --double >/dev/null \
        || fail "click: omarchy-drive refused the double click on the elision marker"
    settle
    settle
    printf 'CLICK elision-double path=%q barOpen=%s\n' "$(ipc path)" "$(ipc pathBarOpen)"
    shot click-elision-double
    [[ "$(ipc pathBarOpen)" == "true" ]] || fail "click: a double click on the elision marker did not open the path bar"
    [[ "$(ipc path)" == "$deep" ]] || fail "click: the double click on the elision marker navigated to $(ipc path)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc pathBarOpen)" == "false" ]] || fail "click: Escape did not close the path bar the elision marker opened"

    # keys.toml's chrome/left x2/inert row covers the current folder's own segment, and the chrome is the window's own top item, so its band is y 0 to chromeHeight - 1.
    local chrome_h band_crumb band_x band
    chrome_h=$(ipc chromeHeight)
    band_crumb=$(( $(ipc crumbCount) - 1 ))
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

    # The strip's own bottom edge is one rule across that row, and what would break it is an opaque fill standing where the rule should be.
    local edge_y edge_spread
    edge_y=$(( chrome_h - 1 ))
    shot click-chrome-edge
    edge_spread=$(magick "$evidence_dir/click-chrome-edge.png" \
        -crop "${chrome_edge_sample_width}x1+0+${edge_y}" +repage \
        -format "%[fx:round(255*max(max(maxima.r-minima.r,maxima.g-minima.g),maxima.b-minima.b))]" info:)
    printf 'CLICK chrome-edge y=%s width=%s spread=%s\n' "$edge_y" "$chrome_edge_sample_width" "$edge_spread"
    [[ "$edge_spread" -le "$chrome_edge_max_spread" ]] \
        || fail "click: the strip's bottom edge spans $edge_spread of 255 across ${chrome_edge_sample_width}px, so something opaque drew over it"

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

    # GM's ruling of 2026-09-22: a parent answers on the first tap, so a double click on one is two taps and never types the path.
    crumbs=$(ipc crumbCount)
    target=$((crumbs - 2))
    centre=$(ipc crumbCentre "$target")
    [[ -n "$centre" ]] || fail "click: crumb $target has no on-screen centre for the double click"
    read -r cx cy <<< "$centre"
    local qs_pid pid held_pid="" click_status held_path held_bar
    qs_pid=$(flea_pid)
    # Sample input: /proc/<pid>/cmdline "/usr/bin/flea\0--backend\0"; ViewState's writer is a flea child too, run as --ui-state.
    for pid in $(pgrep -P "$qs_pid" -x flea); do
        tr '\0' ' ' 2>/dev/null < "/proc/$pid/cmdline" | grep -Fq -- ' --backend ' && held_pid="$held_pid $pid"
    done
    held_pid=${held_pid# }
    [[ "$held_pid" =~ ^[0-9]+$ ]] || fail "click: expected one flea --backend child of qs $qs_pid to hold, found '$held_pid'"
    # The path is written only when a listing answers, so a held backend keeps one crumb delegate under both taps.
    kill -STOP "$held_pid" || fail "click: could not stop backend $held_pid, so the double click would run unheld"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" --double >/dev/null
    click_status=$?
    settle
    held_path=$(ipc path)
    held_bar=$(ipc pathBarOpen)
    kill -CONT "$held_pid" || fail "click: could not resume backend $held_pid after the double click"
    (( click_status == 0 )) || fail "click: omarchy-drive refused the double click on crumb $target"
    printf 'CLICK crumb-double=%s held path=%q barOpen=%s\n' "$target" "$held_path" "$held_bar"
    shot click-crumb-double
    [[ "$held_path" == "$up" ]] || fail "click: the path moved to $held_path with the backend held, so the two taps did not meet one crumb"
    [[ "$held_bar" == "false" ]] || fail "click: a double click on a parent crumb typed the path instead of opening it"
    settle
    settle
    printf 'CLICK crumb-double released path=%q\n' "$(ipc path)"
    [[ "$(ipc path)" == "$(dirname "$up")" ]] || fail "click: a double click on a parent crumb went to $(ipc path), not the one directory it names"
    kill_flea
}

# Ctrl+click after a plain click, in all three views. The plain click leaves the set empty with the
# cursor on its row, which every write operation and shift+click read as "that row is the selection";
# the ctrl+click used to replace it and now adds to it, keys.toml [[pointer]] "add the row to the
# selection". ui/js/Filter.js toggleRow decides it and tests/js/filter.js checks the arithmetic; this
# is the half that proves each view's delegate hands the real click there.
case_ctrlclick() {
    local dir="$fixture_root/ctrlclick"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir"
    printf 'alpha\n' > "$dir/alpha.txt"
    printf 'beta\n' > "$dir/beta.txt"
    printf 'gamma\n' > "$dir/gamma.txt"
    launch "$dir"
    # Measured row order: subdir, alpha.txt, beta.txt, gamma.txt.
    wait_listing 4
    local view
    for view in list grid columns; do
        if [[ "$view" != list ]]; then
            click_chrome "$view"
            settle
            local drawn
            drawn=$(ipc viewMode)
            [[ "$drawn" == "$view" ]] || fail "ctrlclick: the chrome drew '$drawn', not the $view"
        fi
        # ui/js/Tap.js's rule: a plain tap replaces the selection with its own row, Finder's.
        click_row 1 left
        settle
        [[ "$(ipc selectedIndices)" == "1" ]] \
            || fail "ctrlclick: a plain click in the $view selected '$(ipc selectedIndices)', not its own row"
        click_row 3 left --mods ctrl
        settle
        printf 'CTRLCLICK %s indices=%s cursor=%s\n' "$view" "$(ipc selectedIndices)" "$(ipc cursor)"
        shot "ctrlclick-$view"
        [[ "$(ipc selectedIndices)" == "1,3" ]] \
            || fail "ctrlclick: in the $view ctrl+click selected '$(ipc selectedIndices)', not 1,3"
        [[ "$(ipc cursor)" == "3" ]] || fail "ctrlclick: in the $view the cursor is $(ipc cursor), not 3"
        # The anchor is the ctrl+clicked row, so a shift+click from it runs 2,3 and never back to 1.
        click_row 2 left --mods shift
        settle
        [[ "$(ipc selectedIndices)" == "2,3" ]] \
            || fail "ctrlclick: in the $view shift+click selected '$(ipc selectedIndices)', not 2,3"
        # PR 106's own case: a cursor row with nothing marked is that row selected to every write
        # operation, so the ctrl+click adds to it rather than replacing it.
        key -k Escape >/dev/null
        settle
        [[ "$(ipc selectionCount)" == "0" ]] \
            || fail "ctrlclick: escape left $(ipc selectionCount) rows marked in the $view"
        # No key moves it: escape leaves the cursor where the shift+click put it, and Up is a row
        # of tiles in the grid against a row of text in the list.
        [[ "$(ipc cursor)" == "2" ]] || fail "ctrlclick: the cursor is $(ipc cursor) in the $view, not 2"
        click_row 3 left --mods ctrl
        settle
        [[ "$(ipc selectedIndices)" == "2,3" ]] \
            || fail "ctrlclick: in the $view ctrl+click on an unmarked cursor row selected '$(ipc selectedIndices)', not 2,3"
        # And on a row that is the whole selection it takes that row off, which is what a toggle is.
        click_row 2 left
        settle
        click_row 2 left --mods ctrl
        settle
        [[ "$(ipc selectedIndices)" == "" ]] \
            || fail "ctrlclick: in the $view ctrl+click on the marked row left '$(ipc selectedIndices)' marked"
        # Nothing marked, so the next view's own first click is a transition and can fail.
        key -k Escape >/dev/null
        settle
        local left
        left=$(ipc selectionCount)
        [[ "$left" == "0" ]] || fail "ctrlclick: the $view left $left rows marked for the next view"
    done
}

# PR 97 (DouglasdeMoura), issue 96's neighbour: the view the window is left on is the view the next
# launch opens on. The write is ui/Pane.qml onViewModeChanged and the read is its Component.onCompleted,
# so nothing but a relaunch proves the pair; a tab carries its own view and must not be the one stored.
case_viewrestart() {
    local dir="$fixture_root/viewrestart" drew stored_view
    sandbox_scratch "$dir"
    mkdir -p "$dir/sub"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    # Columns, not the list: the list is what the reader falls back to, so a seeded list would pass
    # this whether the stored word was read or ignored.
    seed_ui_state "$fixture_root/viewrestart-state" '{"view":"columns"}'
    launch "$dir"
    wait_listing 3
    drew=$(ipc viewMode)
    [[ "$drew" == "columns" ]] || fail "viewrestart: the seeded state opened on '$drew', not the columns"
    switch_view grid
    launch "$dir"
    wait_listing 3
    drew=$(ipc viewMode)
    [[ "$drew" == "grid" ]] || fail "viewrestart: the next launch opened on '$drew', not the grid it was left on"
    # A tab carries its own view, so the window is left on the first tab's grid and not the second's list.
    key t >/dev/null
    settle
    switch_view list
    key -M ctrl -k Page_Down -m ctrl >/dev/null
    settle
    drew=$(ipc viewMode)
    [[ "$drew" == "grid" ]] || fail "viewrestart: the first tab came back as '$drew', not the grid it held"
    launch "$dir"
    wait_listing 3
    drew=$(ipc viewMode)
    [[ "$drew" == "grid" ]] || fail "viewrestart: after the second tab's own switch the launch opened on '$drew', not the grid the window was left on"
    # PR 97's own edge: a word this build cannot draw is read as the list and put back drawable.
    kill_flea
    local stored="$fixture_root/viewrestart-state/flea/ui.json"
    python3 - "$stored" <<'EDIT'
import json, sys
path = sys.argv[1]
with open(path) as f:
    state = json.load(f)
state["view"] = "banana"
with open(path, "w") as f:
    json.dump(state, f)
EDIT
    launch "$dir"
    wait_listing 3
    drew=$(ipc viewMode)
    [[ "$drew" == "list" ]] || fail "viewrestart: a stored word this build cannot draw opened on '$drew', not the list"
    # Checked before it is parsed, so a file nobody can read is not reported as a settle that wrote nothing.
    [[ -r "$stored" ]] || fail "viewrestart: $stored cannot be read, so what the settle wrote cannot be judged"
    # Sample input, the one key this reads out of the state file: {"keys":"default","view":"list"}
    stored_view=$(grep -o '"view": *"[^"]*"' "$stored" | cut -d'"' -f4) || stored_view=""
    [[ -n "$stored_view" ]] || fail "viewrestart: $stored names no view key at all"
    [[ "$stored_view" == "list" ]] || fail "viewrestart: the settle left '$stored_view' in the state file, not the list the pane drew"
    kill_flea
}

# Issue 70, TyRichards: the sort choice outlives the window, and the next launch lists in it rather
# than in name ascending. The order is checked on the rows, not only on the header's own mark.
case_sortrestart() {
    local dir="$fixture_root/sortrestart" mark
    sandbox_scratch "$dir"
    # Size order and name order disagree on purpose: a listing in name order cannot pass this.
    head -c 300 /dev/zero > "$dir/a.txt"
    head -c 10 /dev/zero > "$dir/b.txt"
    head -c 100 /dev/zero > "$dir/c.txt"
    seed_ui_state "$fixture_root/sortrestart-state" '{"sort":{"key":"size","reverse":true}}'

    launch "$dir"
    wait_listing 3
    mark=$(ipc sortMark)
    [[ "$mark" == "size:desc" ]] || fail "sortrestart: the seeded order opened as '$mark', not size:desc"
    [[ "$(ipc rowAt 0)" == "a.txt|"* && "$(ipc rowAt 1)" == "c.txt|"* && "$(ipc rowAt 2)" == "b.txt|"* ]] \
        || fail "sortrestart: the seeded listing reads $(ipc rowAt 0) $(ipc rowAt 1) $(ipc rowAt 2)"

    echo "-- S reverses it, and the next launch opens in what was left --"
    key S >/dev/null
    settle
    mark=$(ipc sortMark)
    [[ "$mark" == "size:asc" ]] || fail "sortrestart: S left the mark on '$mark', not size:asc"
    launch "$dir"
    wait_listing 3
    mark=$(ipc sortMark)
    [[ "$mark" == "size:asc" ]] || fail "sortrestart: the next launch opened on '$mark', not the size:asc it was left on"
    [[ "$(ipc rowAt 0)" == "b.txt|"* && "$(ipc rowAt 1)" == "c.txt|"* && "$(ipc rowAt 2)" == "a.txt|"* ]] \
        || fail "sortrestart: the restored listing reads $(ipc rowAt 0) $(ipc rowAt 1) $(ipc rowAt 2)"
    printf 'SORTRESTART mark=%s rows=%s %s %s\n' "$mark" "$(ipc rowAt 0)" "$(ipc rowAt 1)" "$(ipc rowAt 2)"
    kill_flea
}

# MediaMute board: one mark at the strip's right end says the state by its glyph, m flips it while a
# media preview is open, the mark's own click does the same, and neither pauses the player.
case_mute() {
    command -v ffmpeg >/dev/null || fail "ffmpeg is missing, so the audio fixture cannot be built"
    local dir="$fixture_root/mute"
    sandbox_scratch "$dir"
    # Audio, not video: the strip is permanent on an audio preview, so nothing has to be revealed.
    ffmpeg -y -f lavfi -i "sine=frequency=440:duration=20" "$dir/tone.wav" >/dev/null 2>&1
    [[ -s "$dir/tone.wav" ]] || fail "mute: ffmpeg produced no tone.wav"

    launch "$dir"
    wait_listing 1
    key -k space >/dev/null
    for _attempt in $(seq 1 60); do
        [[ "$(ipc previewOpen)" == "true" ]] && break
        sleep 0.25
    done
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "audio" ]] \
        || fail "mute: the preview is $(ipc previewKind), open=$(ipc previewOpen)"
    local strip
    strip=$(ipc previewStrip) || fail "mute: the strip has no state"
    [[ "$(jq -r .visible <<< "$strip")" == "true" ]] || fail "mute: an audio preview drew no strip"
    [[ "$(jq -r .muted <<< "$strip")" == "false" ]] || fail "mute: the session opened muted"

    echo "-- m mutes, and the player keeps going --"
    local before after
    before=$(ipc previewPosition)
    key m >/dev/null
    for _attempt in $(seq 1 40); do
        [[ "$(ipc previewStrip | jq -r .muted)" == "true" ]] && break
        sleep 0.25
    done
    [[ "$(ipc previewStrip | jq -r .muted)" == "true" ]] || fail "mute: m did not mute"
    [[ "$(ipc previewState)" == "playing" ]] || fail "mute: m stopped the player, state is $(ipc previewState)"
    sleep 2
    after=$(ipc previewPosition)
    (( after > before )) || fail "mute: the clock stopped at $after, so mute paused the player"
    printf 'MUTE position %s then %s while muted\n' "$before" "$after"

    echo "-- and the mark's own click flips it back --"
    local wx wy cx cy
    read -r wx wy _ _ < <(window_box) || fail "mute: native window coordinates unavailable"
    read -r cx cy <<< "$(ipc previewStrip | jq -r .mute)"
    [[ -n "$cy" ]] || fail "mute: the mark has no centre"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null || fail "mute: could not click the mark"
    for _attempt in $(seq 1 40); do
        [[ "$(ipc previewStrip | jq -r .muted)" == "false" ]] && break
        sleep 0.25
    done
    [[ "$(ipc previewStrip | jq -r .muted)" == "false" ]] || fail "mute: the mark's click did not unmute"
    [[ "$(ipc previewState)" == "playing" ]] || fail "mute: the click stopped the player"

    echo "-- the flag is the session's, so it survives the preview that set it --"
    key m >/dev/null
    for _attempt in $(seq 1 40); do
        [[ "$(ipc previewStrip | jq -r .muted)" == "true" ]] && break
        sleep 0.25
    done
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "mute: escape left the preview open"
    key -k space >/dev/null
    for _attempt in $(seq 1 60); do
        [[ "$(ipc previewOpen)" == "true" ]] && break
        sleep 0.25
    done
    [[ "$(ipc previewStrip | jq -r .muted)" == "true" ]] \
        || fail "mute: the next preview forgot the session's own flag"
    key -k Escape >/dev/null
    settle
    kill_flea
}

# MenuAdditions rule 3: a Places or Favorites row opens the folder menu for its own path, and with
# the Extras switch off the rail keeps the one Remove a favourite has offered since 0.2.1.
case_placemenu() {
    local dir="$fixture_root/placemenu"
    sandbox_scratch "$dir"
    mkdir -p "$dir/Work"
    : > "$dir/Work/one.txt"
    : > "$dir/plain.txt"
    local state="$fixture_root/placemenu-state"
    # The switch on, and one favourite to open the menu over. Everything else is the shipped set.
    # The switch on, and Open in terminal and Copy path on too, because a row switched off in Settings
    # is off on this menu as well: with the shipped set those two are absent and the menu is shorter.
    seed_ui_state "$state" "$(printf '{"menu":{"hidden":["delete","moveto","copyto","properties","permissions"]},"places":{"favourites":[{"label":"Work","path":"%s/Work"}]}}' "$dir")"

    launch "$dir"
    wait_listing 2
    local favourite_index
    favourite_index=$(ipc railEntries | jq -r 'map(.label) | index("Work")')
    [[ -n "$favourite_index" && "$favourite_index" != "null" ]] \
        || fail "placemenu: the seeded favourite is not on the rail, which carries $(ipc railEntries)"

    echo "-- a Favorites row ends on Remove, and a Places row on Add --"
    click_rail_row "$favourite_index" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "placemenu: the favourite's right click opened no menu"
    [[ "$(ipc contextMenuEntries)" == "Open|Open in new tab|-|Open in terminal|Copy path|Remove from Favorites" ]] \
        || fail "placemenu: the favourite offers $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    for _attempt in $(seq 1 20); do
        [[ "$(ipc contextMenuVisible)" == "false" ]] && break
        sleep 0.25
    done
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "placemenu: escape left the favourite's menu open"
    local home_index
    home_index=$(ipc railEntries | jq -r 'map(.label) | index("Home")')
    [[ -n "$home_index" && "$home_index" != "null" ]] \
        || fail "placemenu: the rail has no Home row, it carries $(ipc railEntries | jq -r 'map(.label) | join(",")')"
    click_rail_row "$home_index" right
    settle
    # Read visible before entries: the menu keeps its last rows, so a row that opens nothing would
    # otherwise answer with the menu before it, which is exactly how this case first read green.
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "placemenu: the Home row's right click opened no menu"
    [[ "$(ipc contextMenuEntries)" == "Open|Open in new tab|-|Open in terminal|Copy path|Add to Favorites" ]] \
        || fail "placemenu: the Home row offers $(ipc contextMenuEntries)"

    echo "-- and a row acts on its own path, not on the listing's cursor --"
    local tabs_before
    tabs_before=$(ipc tabCount)
    menu_seek "Open in new tab"
    key -k Return >/dev/null
    for _attempt in $(seq 1 40); do
        [[ "$(ipc tabCount)" == "$((tabs_before + 1))" ]] && break
        sleep 0.25
    done
    [[ "$(ipc tabCount)" == "$((tabs_before + 1))" ]] \
        || fail "placemenu: Open in new tab left $(ipc tabCount) tabs"
    wait_path "$HOME"
    printf 'PLACEMENU tabs=%s path=%s labels=%s\n' "$(ipc tabCount)" "$(ipc path)" "$(ipc tabLabels)"

    echo "-- with the switch off it is the menu it was --"
    kill_flea
    seed_ui_state "$fixture_root/placemenu-off" "$(printf '{"places":{"favourites":[{"label":"Work","path":"%s/Work"}]}}' "$dir")"
    launch "$dir"
    wait_listing 2
    favourite_index=$(ipc railEntries | jq -r 'map(.label) | index("Work")')
    click_rail_row "$favourite_index" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "placemenu: with the switch off the favourite opened no menu"
    [[ "$(ipc contextMenuEntries)" == "Remove" ]] \
        || fail "placemenu: with the switch off the favourite offers $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle
    local home_off
    home_off=$(ipc railEntries | jq -r 'map(.label) | index("Home")')
    click_rail_row "$home_off" right
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] \
        || fail "placemenu: with the switch off the Home row opened $(ipc contextMenuEntries)"
    printf 'PLACEMENU off=%s\n' "$(ipc contextMenuVisible)"
    kill_flea
}

# MenuAdditions rule 2: one row per executable in ~/.config/flea/scripts, read when the menu opens,
# run with the selected paths in the first one's folder, and absent when that directory holds none.
case_runscript() {
    local dir="$fixture_root/runscript"
    sandbox_scratch "$dir"
    printf 'one\n' > "$dir/a.txt"
    printf 'two\n' > "$dir/b.txt"
    local config="$fixture_root/runscript-config"
    sandbox_scratch "$config"
    mkdir -p "$config/flea/scripts"
    export XDG_CONFIG_HOME="$config"
    # Three, one of them not executable and one failing, which is the whole of the rule's own edges.
    printf '#!/bin/sh\nprintf "%%s\\n" "$@" > %s/ran.log\nprintf "%%s\\n" "$PWD" >> %s/ran.log\n' "$dir" "$dir" > "$config/flea/scripts/stamp.sh"
    printf '#!/bin/sh\nprintf "no such page\\n" >&2\nexit 2\n' > "$config/flea/scripts/ocr.sh"
    printf '#!/bin/sh\nexit 0\n' > "$config/flea/scripts/not-executable.sh"
    chmod +x "$config/flea/scripts/stamp.sh" "$config/flea/scripts/ocr.sh"
    # The switch on: everything else in the shipped set stays as it is.
    seed_ui_state "$fixture_root/runscript-state" '{"menu":{"hidden":["delete","openTerminal","placeMenu","moveto","copyto","properties","permissions","copypath"]}}'

    launch "$dir"
    wait_listing 2
    seek_row_named "a.txt"
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "runscript: the row's right click opened no menu"
    [[ "$(ipc contextMenuEntries)" == *"Run script"* ]] \
        || fail "runscript: the menu offers $(ipc contextMenuEntries)"
    menu_seek "Run script"
    key -k Return >/dev/null
    settle
    # Sorted by name, the label without the extension, and the file that is not executable absent.
    [[ "$(ipc contextMenuSubmenuEntries)" == "ocr|stamp" ]] \
        || fail "runscript: the submenu offers $(ipc contextMenuSubmenuEntries)"

    echo "-- a row runs its script with the selected paths, in the first one's folder --"
    menu_seek_submenu "stamp"
    key -k Return >/dev/null
    for _attempt in $(seq 1 40); do [[ -s "$dir/ran.log" ]] && break; sleep 0.25; done
    [[ -s "$dir/ran.log" ]] || fail "runscript: the script never ran, the bar says $(ipc lastMessage)"
    [[ "$(head -1 "$dir/ran.log")" == "$dir/a.txt" ]] \
        || fail "runscript: the script was handed $(head -1 "$dir/ran.log")"
    [[ "$(tail -1 "$dir/ran.log")" == "$dir" ]] \
        || fail "runscript: the script ran in $(tail -1 "$dir/ran.log"), not the file's own folder"
    printf 'RUNSCRIPT args=%s cwd=%s\n' "$(head -1 "$dir/ran.log")" "$(tail -1 "$dir/ran.log")"

    echo "-- and a non-zero exit is its own last stderr line --"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Run script"
    key -k Return >/dev/null
    settle
    menu_seek_submenu "ocr"
    key -k Return >/dev/null
    wait_message "ocr.sh · no such page"
    printf 'RUNSCRIPT said=%s\n' "$(ipc lastMessage)"

    echo "-- an empty directory offers no row at all --"
    rm -f "$config/flea/scripts/stamp.sh" "$config/flea/scripts/ocr.sh"
    click_row "$(ipc cursor)" right
    settle
    key -k Escape >/dev/null
    settle
    click_row "$(ipc cursor)" right
    settle
    [[ "$(ipc contextMenuEntries)" != *"Run script"* ]] \
        || fail "runscript: an empty directory still offers $(ipc contextMenuEntries)"
    printf 'RUNSCRIPT empty=%s\n' "$(ipc contextMenuEntries)"
    kill_flea
}

# Catches narrowing the delegate TapHandler back to Qt.LeftButton in ui/Pane.qml.
case_menu() {
    local dir="$fixture_root/menu" state="$fixture_root/menu-state"
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
    seed_ui_state "$state" '{"view":"list","keys":"default"}'
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
    # The pointer rule, both halves: a menu opened under a resting pointer keeps its first row (0d626ed,
    # or Enter fires the pointer's row), and a pointer that then moves lights the row it moved onto.
    # An action row by name: a separator sits at index 2 and disables hover on purpose, so an index alone proves nothing.
    local rest_x rest_y rest_row
    key m >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "menu: the m key did not open the menu at the cursor"
    rest_row=$(menu_row_index "Rename")
    [[ -n "$rest_row" && "$rest_row" -ge 0 ]] || fail "menu: no Rename row in $(ipc contextMenuEntries)"
    read -r rest_x rest_y <<< "$(ipc contextMenuRowCentre "$rest_row")"
    [[ -n "$rest_y" ]] || fail "menu: the menu has no row $rest_row to rest the pointer on"
    key -k Escape >/dev/null
    settle
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    # A warp alone reaches Qt as no motion at all, so the one uinput pixel is what makes the pointer rest there.
    omarchy-drive move "$((wx + rest_x))" "$((wy + rest_y))" >/dev/null
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
    settle
    # m through uinput (evdev 50): the key helper refocuses the window first, and Hyprland warps the cursor on focus without telling Qt.
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool key 50:1 50:0 >/dev/null 2>&1
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "menu: the m key did not reopen the menu under the resting pointer"
    # The target is proven before the move is judged: the reopened menu's Rename row is where it was, and the pointer is over it.
    [[ "$(ipc contextMenuRowCentre "$rest_row")" == "$rest_x $rest_y" ]] || fail "menu: the reopened menu put Rename at $(ipc contextMenuRowCentre "$rest_row"), not $rest_x $rest_y"
    # Where the pointer is, is what the centre above proves. Qt reports no hover on a row built
    # under a pointer that has not moved since, which is the very rule the two checks below read.
    [[ "$(ipc contextMenuCursor)" == "0" ]] || fail "menu: a menu opened under a resting pointer moved its cursor to row $(ipc contextMenuCursor)"
    printf 'MENU probe before the move: %s\n' "$(ipc contextMenuRowProbe "$rest_row")"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 3 -y 0 >/dev/null 2>&1
    settle
    printf 'MENU probe after the move: %s\n' "$(ipc contextMenuRowProbe "$rest_row")"
    [[ "$(ipc contextMenuCursor)" == "$rest_row" ]] || fail "menu: the pointer moved inside row $rest_row (Rename) and the cursor stayed on row $(ipc contextMenuCursor)"
    printf 'MENU pointer rest=0 moved=%s\n' "$rest_row"
    key -k Escape >/dev/null
    settle
    centre=$(ipc rowCentre 0)
    read -r cx cy <<< "$centre"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"

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
    # Menus section and fail on a box that has switched any of the seven back on.
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
    # Menus.html's background column: New Folder and New File lead it, and GM ruled Add to Favorites stays.
    [[ "$(ipc contextMenuEntries)" == "New Folder|New File|-|Paste|Select all|-|Add to Favorites|-|Sort by|Show hidden files|-|Settings" ]] \
        || fail "background: the menu is not the board's column, it is $(ipc contextMenuEntries)"
    [[ "$(ipc contextMenuGlyphs)" == "folder-plus|file-plus|-|clipboard|check|-|star|-|sort|eye|-|sliders" ]] \
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
    # Four orders, because src/backend/ordering.rs answers kind as well as the three in sort.rs.
    [[ "$(ipc contextMenuSubmenuEntries)" == "Name|Size|Modified|Kind" ]] \
        || fail "background: the Sort by flyout is $(ipc contextMenuSubmenuEntries)"
    [[ "$(ipc contextMenuSubmenuGlyphs)" == "sort|sort|sort|sort" ]] \
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
    # The mark has to paint, not only be flagged (a z below the view's paint once hid it), and its draw starts blank, so the shot is polled across one replay.
    local mark lit=0 shots=0 captured=1 png="$evidence_dir/background-empty.png"
    mark=$(ipc emptyMarkRect)
    set -- $mark
    # A stale image at this path from an earlier run would score as a live one, so each shot is a fresh file or a failure.
    rm -f "$png"
    for _attempt in $(seq 1 "$mark_poll_shots"); do
        shots=$_attempt
        omarchy-drive shot "$png" flea >/dev/null || { captured=0; break; }
        lit=$(count_pixels "$png" "${3}x${4}+${1}+${2}" "((r+g+b)/3) > 0.25")
        (( lit > 0 )) && break
        sleep "$mark_poll_s"
    done
    printf 'SHOT %s\nBACKGROUND mark rect=%s lit=%s shots=%s captured=%s\n' "$png" "$mark" "$lit" "$shots" "$captured"
    (( captured )) || fail "background: omarchy-drive shot failed on shot $shots of $png"
    (( ! captured || lit > 0 )) || fail "background: the empty mark painted no pixel inside ${3}x${4}+${1}+${2} across $shots shots"
    # The empty listing is also the strongest case for this menu, and it has no row to aim from.
    click_background
    settle
    [[ "$(ipc contextMenuEntries)" == "New Folder|New File|-|Paste|Select all|-|Add to Favorites|-|Sort by|Show hidden files|-|Settings" ]] \
        || fail "background: an empty directory drew $(ipc contextMenuEntries)"
    # Its own name: the empty-mark poll above already owns background-empty.png.
    shot background-empty-menu
    key -k Escape >/dev/null
    settle
    menu_click "Paste"
    settle
    printf 'BACKGROUND paste landed=%s message=%q\n' \
        "$([[ -f "$dir/dest/a.txt" ]] && echo yes || echo no)" "$(ipc lastMessage)"
    [[ -f "$dir/dest/a.txt" ]] || fail "background: Paste put nothing in $dir/dest"

    # New folder, the only background row that writes on its own, so the directory is the proof.
    [[ ! -e "$dir/dest/New Folder" ]] || fail "background: New Folder existed before the row ran"
    menu_click "New Folder"
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
        [[ "$(ipc contextMenuEntries)" == "New Folder|New File|-|Paste|Select all|-|Add to Favorites|-|Sort by|Show hidden files|-|Settings" ]] \
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
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
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
    local parent_cursor parent_row parent_state parent_loading
    for _attempt in $(seq 1 300); do
        parent_cursor=$(ipc cursor 2>/dev/null || printf 0)
        parent_row=$(ipc rowAt "$parent_cursor" 2>/dev/null || true)
        parent_state=$(ipc state 2>/dev/null || printf loading)
        parent_loading=$(ipc listInFlight 2>/dev/null || printf true)
        [[ "$parent_state" == ready && "$parent_loading" == false && "$parent_row" == selection\|dir\|* ]] && break
        sleep 0.05
    done
    [[ "$parent_state" == ready && "$parent_loading" == false && "$parent_row" == selection\|dir\|* ]] \
        || fail "selection: parent listing did not reselect selection/, row=$parent_row state=$parent_state inFlight=$parent_loading"
    [[ "$(ipc selectionCount)" == "1" ]] \
        || fail "selection: parent listing selection count is $(ipc selectionCount), not the pending folder"
    [[ "$(ipc selectedIndices)" == "$parent_cursor" ]] \
        || fail "selection: selectedIndices=$(ipc selectedIndices), cursor=$parent_cursor"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "0" ]] || fail "selection: Escape did not clear the pending folder selection"

    printf 'SELECTION toggle=ok extend=ok all=ok clear=ok stale=ok\n'
    kill_flea
}

# Issue 68, driven exactly as it was reported: the reporter's own four changes made from outside
# the window, with nothing clicked and no folder left. Catches deleting the watch from
# src/backend/watch.rs, the changed branch from ui/Backend.qml, or the re-read from ui/PaneWire.qml.
case_watch() {
    local dir="$fixture_root/watch"
    sandbox_scratch "$dir"
    printf 'a\n' > "$dir/alpha.txt"
    printf 'b\n' > "$dir/beta.txt"
    printf 'p\n' > "$dir/preview-me.txt"
    launch "$dir"
    wait_listing 3
    [[ "$(ipc rowAt 0)" == alpha.txt\|* ]] || fail "watch: row 0 is $(ipc rowAt 0), not alpha.txt"

    # Issue 159's move-into-new-directory case: create and move from outside Flea while its one window stays open.
    local move_bytes='watch-move-payload-159' watch_pid
    watch_pid=$(flea_pid)
    printf '%s' "$move_bytes" > "$dir/move-source.txt"
    mkdir "$dir/move-target"
    mv "$dir/move-source.txt" "$dir/move-target/move-source.txt"
    omarchy-drive wait ipc -p "$flea_ui/boot" flea total 4 --timeout 15 >/dev/null \
        || fail "watch: creating a directory and moving a file into it left the listing at $(ipc total) rows"
    [[ "$(ipc rowAt 0)" == move-target\|dir\|* ]] \
        || fail "watch: the moved-into directory is not row 0, got $(ipc rowAt 0)"
    for _move_row in 0 1 2 3; do
        [[ "$(ipc rowAt "$_move_row")" != move-source.txt\|* ]] \
            || fail "watch: the moved file stayed in the parent listing at row $_move_row"
    done
    seek_row_named move-target
    key -k Return >/dev/null
    wait_path "$dir/move-target"
    wait_listing 1
    [[ "$(ipc rowAt 0)" == move-source.txt\|file\|* ]] \
        || fail "watch: entering the new directory lists $(ipc rowAt 0), not the moved file"
    open_row move-source.txt
    local preview_wait
    for preview_wait in $(seq 1 100); do
        [[ "$(ipc previewState)" == ready ]] && break
        sleep 0.05
    done
    [[ "$(ipc previewState)" == ready && "$(ipc previewText)" == "$move_bytes" ]] \
        || fail "watch: the moved file preview is state=$(ipc previewState), bytes=$(ipc previewText | od -An -tx1)"
    key -k Escape >/dev/null
    settle
    key -k Backspace >/dev/null
    wait_path "$dir"
    wait_listing 4
    [[ "$(flea_pid)" == "$watch_pid" ]] || fail "watch: the move case relaunched Flea"
    printf 'WATCH move=ok parent-row=gone child-bytes=exact no-relaunch=ok\n'
    [[ "$(ipc selectionCount)" == 1 ]] || fail "watch: returning to the parent did not select the folder"
    key -k Escape >/dev/null
    [[ "$(ipc selectionCount)" == 0 ]] || fail "watch: Escape did not release the folder selection"
    rm "$dir/move-target/move-source.txt"
    rmdir "$dir/move-target"
    omarchy-drive wait ipc -p "$flea_ui/boot" flea total 3 --timeout 15 >/dev/null \
        || fail "watch: move-case cleanup left the parent listing at $(ipc total) rows"

    # The reporter's four changes, from another process, while the window sits on the folder.
    printf 'new\n' > "$dir/NEWFILE-appeared.txt"
    mv "$dir/alpha.txt" "$dir/alpha-RENAMED.txt"
    rm "$dir/beta.txt"
    mkdir "$dir/brand-new-folder"
    # The 400 ms settle plus the re-read; the reporter waited several seconds and saw nothing move.
    omarchy-drive wait ipc -p "$flea_ui/boot" flea total 4 --timeout 15 >/dev/null \
        || fail "watch: the listing stayed at $(ipc total) rows after four outside changes"
    settle
    printf 'WATCH total=%s row0=%q row1=%q row2=%q\n' \
        "$(ipc total)" "$(ipc rowAt 0)" "$(ipc rowAt 1)" "$(ipc rowAt 2)"
    shot watch-after-outside-changes
    # Directories first, then name ascending and case-insensitive: brand-new-folder,
    # alpha-RENAMED.txt, NEWFILE-appeared.txt, preview-me.txt.
    [[ "$(ipc rowAt 0)" == brand-new-folder\|dir\|* ]] \
        || fail "watch: the new directory is not row 0, got $(ipc rowAt 0)"
    [[ "$(ipc rowAt 1)" == alpha-RENAMED.txt\|* ]] \
        || fail "watch: the renamed file is not row 1, got $(ipc rowAt 1)"
    [[ "$(ipc rowAt 2)" == NEWFILE-appeared.txt\|* ]] \
        || fail "watch: the created file is not row 2, got $(ipc rowAt 2)"
    [[ "$(ipc rowAt 3)" == preview-me.txt\|* ]] \
        || fail "watch: the untouched file is not row 3, got $(ipc rowAt 3)"

    # The cursor is put back on the file it was on, not on the row that index now names: without the
    # anchor the create above it leaves the cursor on brand-new-folder.
    goto_row 3
    [[ "$(ipc rowAt "$(ipc cursor)")" == preview-me.txt\|* ]] \
        || fail "watch: the cursor did not start on preview-me.txt"
    printf 'z\n' > "$dir/AAA-above-the-cursor.txt"
    omarchy-drive wait ipc -p "$flea_ui/boot" flea total 5 --timeout 15 >/dev/null \
        || fail "watch: the second outside create left the listing at $(ipc total) rows"
    settle
    printf 'WATCH cursor=%s row=%q\n' "$(ipc cursor)" "$(ipc rowAt "$(ipc cursor)")"
    [[ "$(ipc rowAt "$(ipc cursor)")" == preview-me.txt\|* ]] \
        || fail "watch: a create above the cursor moved it to $(ipc rowAt "$(ipc cursor)")"

    # A selection names rows by index, so the re-read waits for it rather than re-pointing it.
    key v >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "1" ]] || fail "watch: v did not select the cursor row"
    printf 'held\n' > "$dir/BBB-while-selected.txt"
    sleep 2
    [[ "$(ipc total)" == "5" ]] \
        || fail "watch: the listing re-read to $(ipc total) rows while a selection stood"
    [[ "$(ipc selectionCount)" == "1" ]] || fail "watch: the held selection was cleared anyway"
    # Clearing the selection is what pays the debt the notification left standing.
    key -k Escape >/dev/null
    omarchy-drive wait ipc -p "$flea_ui/boot" flea total 6 --timeout 15 >/dev/null \
        || fail "watch: clearing the selection did not run the owed re-read, total is $(ipc total)"
    printf 'WATCH deferred=ok paid=ok total=%s\n' "$(ipc total)"

    # A directory under continuous writing still has to settle. The timer absorbs notifications rather
    # than being restarted by them, so the sample that matters is taken WHILE the writing is still
    # going: a restart() is pushed forward by every notification and re-reads nothing until the writer
    # stops, which a sample taken afterwards cannot tell apart from a timer that fired all along.
    local before during writer n
    before=$(ipc total)
    ( for n in $(seq 1 40); do
          printf 'x\n' > "$dir/stream-$n.txt"
          sleep 0.1
      done ) &
    writer=$!
    sleep 1.2
    during=$(ipc total)
    wait "$writer"
    printf 'WATCH stream before=%s during=%s after=%s\n' "$before" "$during" "$(ipc total)"
    (( during > before )) \
        || fail "watch: nothing re-read while the directory was still being written, total stayed $before"

    # A debt owed by this directory must not be paid by re-listing the next one. The selection is what
    # holds the debt, and leaving clears that selection, so without the guard the owed re-read fires
    # against whatever the pane has just opened.
    key v >/dev/null
    settle
    printf 'owed\n' > "$dir/CCC-owed-on-leaving.txt"
    sleep 0.5
    # Counted from before the navigation, not from after it: the owed re-read lands about 400 ms after
    # the selection clears, which is inside wait_path's own polling, so a sample taken on arrival has
    # already counted it and could never tell the two apart.
    local before_nav after_nav
    before_nav=$(ipc listRequests)
    key -k Backspace >/dev/null
    wait_path "$fixture_root"
    sleep 1.5
    after_nav=$(ipc listRequests)
    printf 'WATCH carried lists %s to %s, one navigation and nothing else\n' "$before_nav" "$after_nav"
    (( after_nav == before_nav + 1 )) \
        || fail "watch: leaving cost $(( after_nav - before_nav )) listings, so a debt owed for the directory just left was paid by the one the pane moved to"
    key -k Escape >/dev/null
    settle
    assert_window
    kill_flea
}

# Issue 143, stubbed at lsblk and gio: empty, inserted and mounted optical media are all exercised without a real drive.
case_optical() {
    local dir="$fixture_root/optical" state="$fixture_root/optical-state"
    local label='MATSHITA DVD+/-RW UJ8FB' payload='optical-payload-143' gio_log="$dir/gio.log"
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    mkdir -p "$dir/bin" "$dir/files" "$dir/mnt/DVD" "$state/flea"
    printf 'keep\n' > "$dir/files/keep.txt"
    printf '%s' "$payload" > "$dir/mnt/DVD/disc-bytes.txt"
    : > "$gio_log"

    cat > "$dir/bin/lsblk" <<EOS
#!/bin/sh
if [ -f "$dir/malformed" ]; then
    printf 'not json at all\\n'
    exit 0
fi
if [ -f "$dir/mounted" ]; then
    optical_points='["$dir/mnt/DVD"]'
    optical_fs='"iso9660"'
elif [ -f "$dir/inserted" ]; then
    optical_points='[null]'
    optical_fs='"iso9660"'
else
    optical_points='[null]'
    optical_fs=null
fi
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G",
"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},
{"name":"sr0","path":"/dev/sr0","label":null,"mountpoints":\$optical_points,"rm":true,"size":0,"type":"rom","fstype":\$optical_fs,"model":"$label"},
{"name":"sdb","path":"/dev/sdb","label":"USB","mountpoints":[null],"rm":true,"size":34359738368,"type":"disk","model":"USB Flash Disk"}
]}
JSON
EOS
    chmod +x "$dir/bin/lsblk"

    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
printf '%s\\n' "\$*" >> "$gio_log"
if [ "\$1 \$2 \$3" = "mount -d /dev/sr0" ]; then
    [ -f "$dir/refuse" ] && exit 1
    : > "$dir/mounted"
fi
exit 0
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/optical-home" real_home="$HOME" saved_path="$PATH" old_state="${XDG_STATE_HOME:-}"
    fixture_home_make "$fixture_home"
    export PATH="$dir/bin:$PATH"
    export HOME="$fixture_home"
    export XDG_STATE_HOME="$state"
    launch "$dir/files"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 1

    local entries
    for _attempt in $(seq 1 200); do
        entries=$(ipc deviceEntries)
        [[ "$entries" == *"USB|device|volume|false"* ]] && break
        sleep 0.05
    done
    [[ "$entries" == *"USB|device|volume|false"* ]] \
        || fail "optical: the unmounted USB negative control is missing, got $entries"
    [[ "$entries" != *"$label|device|volume|"* ]] \
        || fail "optical: an empty optical drive was offered, got $entries"
    if grep -q '^mount -d /dev/sr0$' "$gio_log"; then
        fail "optical: an empty drive reached gio mount, log is $(cat "$gio_log")"
    fi
    shot optical-empty

    : > "$dir/inserted"
    wait_rail_label "$label"
    for _attempt in $(seq 1 200); do
        entries=$(ipc deviceEntries)
        [[ "$entries" == *"$label|device|volume|false"* ]] && break
        sleep 0.05
    done
    [[ "$entries" == *"$label|device|volume|false"* ]] \
        || fail "optical: inserted media did not remain unmounted, got $entries"
    : > "$dir/refuse"
    click_rail_row "$(rail_row_of "$label")" left
    wait_message "$label could not be mounted."
    [[ "$(ipc lastMessage)" != *unplug* ]] || fail "optical: refusal still advised unplugging: $(ipc lastMessage)"
    grep -q '^mount -d /dev/sr0$' "$gio_log" \
        || fail "optical: inserted media never reached gio mount, log is $(cat "$gio_log")"
    shot optical-refused

    rm -f "$dir/refuse"
    : > "$dir/mounted"
    for _attempt in $(seq 1 200); do
        entries=$(ipc deviceEntries)
        [[ "$entries" == *"$label|device|volume|true"* ]] && break
        sleep 0.05
    done
    [[ "$entries" == *"$label|device|volume|true"* ]] \
        || fail "optical: mounted media did not retain its row, got $entries"
    click_rail_row "$(rail_row_of "$label")" left
    wait_path "$dir/mnt/DVD"
    wait_listing 1
    [[ "$(ipc rowAt 0)" == disc-bytes.txt\|file\|* ]] \
        || fail "optical: mounted disc lists $(ipc rowAt 0), not disc-bytes.txt"
    shot optical-mounted
    open_row disc-bytes.txt
    for _attempt in $(seq 1 100); do
        [[ "$(ipc previewState)" == ready ]] && break
        sleep 0.05
    done
    [[ "$(ipc previewState)" == ready && "$(ipc previewText)" == "$payload" ]] \
        || fail "optical: mounted disc bytes are state=$(ipc previewState), bytes=$(ipc previewText | od -An -tx1)"
    key -k Escape >/dev/null
    settle

    : > "$dir/malformed"
    for _attempt in $(seq 1 200); do
        entries=$(ipc deviceEntries)
        [[ "$entries" != *"$label|device|volume|"* ]] && break
        sleep 0.05
    done
    [[ "$entries" != *"$label|device|volume|"* ]] \
        || fail "optical: malformed lsblk input retained a stale row, got $entries"
    rm -f "$dir/malformed"
    printf 'OPTICAL empty-hidden=ok usb-retained=ok inserted=ok refusal=observable mounted=ok bytes=exact malformed=clears\n'
    if [[ -n "$old_state" ]]; then export XDG_STATE_HOME="$old_state"; else unset XDG_STATE_HOME; fi
    kill_flea
    sandbox_remove "$fixture_home"
}

# F5, the re-read a selection does not defer. The watch holds a change back while a selection
# stands, which is the stale listing the operator reported on 2026-09-24; F5 is the operator deciding
# not to wait. Catches deleting the F5 row from keys.toml, the refresh case from ui/js/Focus.js, or the
# anchor or the paid debt from ui/PaneWire.qml refreshNow. The first check is the control: if the watch
# did not hold the change, F5 would be proving nothing.
case_refresh() {
    local dir="$fixture_root/refresh"
    sandbox_scratch "$dir"
    printf 'a\n' > "$dir/alpha.txt"
    printf 'b\n' > "$dir/beta.txt"
    printf 'c\n' > "$dir/gamma.txt"
    launch "$dir"
    wait_listing 3
    goto_row 2
    [[ "$(ipc rowAt "$(ipc cursor)")" == gamma.txt\|* ]] || fail "refresh: the cursor did not start on gamma.txt"
    key v >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "1" ]] || fail "refresh: v did not select the cursor row"
    printf 'n\n' > "$dir/AAA-arrived.txt"
    sleep 2
    [[ "$(ipc total)" == "3" ]] \
        || fail "refresh: the watch re-read to $(ipc total) rows under a selection, so F5 is not what is tested"

    local before after
    before=$(ipc listRequests)
    key -k F5 >/dev/null
    omarchy-drive wait ipc -p "$flea_ui" flea total 4 --timeout 15 >/dev/null \
        || fail "refresh: F5 left the listing at $(ipc total) rows"
    settle
    # A second listing would be the watch paying a debt F5 already paid, about 400 ms after the first.
    sleep 1.5
    after=$(ipc listRequests)
    printf 'REFRESH total=%s lists=%s->%s cursor=%q selected=%s\n' \
        "$(ipc total)" "$before" "$after" "$(ipc rowAt "$(ipc cursor)")" "$(ipc selectionCount)"
    shot refresh-after-f5
    [[ "$(ipc rowAt "$(ipc cursor)")" == gamma.txt\|* ]] \
        || fail "refresh: a create above the cursor moved it to $(ipc rowAt "$(ipc cursor)")"
    [[ "$(ipc selectionCount)" == "0" ]] || fail "refresh: F5 kept a selection over renumbered rows"
    (( after == before + 1 )) \
        || fail "refresh: F5 cost $(( after - before )) listings, so the watch paid its debt a second time"
    assert_window
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
        setsid nohup qs -p "$flea_ui/boot" >"$flea_log" 2>&1 </dev/null &
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
        setsid nohup qs -p "$flea_ui/boot" >"$flea_log" 2>&1 </dev/null &
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
    read -r _background _surface foreground _muted _accent _error symlink_colour _executable _rest <<< "$(ipc palette)"
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
    sandbox_require "$dir/inner/pack"
    sandbox_under "$SANDBOX_PATH" "$dir" || fail "columns: archive fixture escaped its own sandbox"
    rm -rf -- "$SANDBOX_PATH"
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
    # Preview board rule 1: Size above says what it weighs packed, so Entries carries both counts.
    [[ "$archive_facts" == *"Entries=3, "*" out"* ]] \
        || fail "columns: the archive states $archive_facts, not the three members it holds"
    [[ "$archive_facts" == *"Kind="*"Size="*"Modified="*"Entries="* ]] \
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
    [[ "$(fact_labels "$facts")" == "Kind|Size|Modified|Pixels" ]] \
        || fail "columns: the image states $(fact_labels "$facts"), not the canvas's own four rows"
    [[ "$facts" == "Kind=PNG image|"* ]] || fail "columns: the image kind is wrong in $facts"
    [[ "$facts" == *"640"*"480"* ]] || fail "columns: the image pixels are wrong in $facts"
    printf 'COLUMNS image=%s\n' "$facts"
    shot columns-image

    seek_row_named "notes.txt"
    settle
    [[ "$(ipc previewColumnState)" == "text" ]] || fail "columns: notes.txt previews as $(ipc previewColumnState)"
    facts=$(ipc previewFacts)
    [[ "$(fact_labels "$facts")" == "Kind|Size|Modified|Lines" ]] \
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
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
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

# Directive 48: there is no centre lane. The transient ends one padding before the text the disk facts
# draw, and those facts never move: everything below is read off the rendered items, and the padding
# comes from the theme's own token through ipc metrics rather than from the layout under test.
transient_beside_disk() {
    local label="$1" state lane disk padding gap disk_now
    state=$(ipc statusFooterState)
    lane=$(jq -c '.lane' <<< "$state")
    disk=$(jq -c '.disk' <<< "$state")
    padding=$(ipc metrics | cut -d' ' -f3)
    [[ "$padding" =~ ^[0-9]+$ ]] || fail "status: the $label strip reported no padding token, got [$padding]"
    jq -e '(.lane.width | numbers) and (.disk.width | numbers) and .disk.width > 0 and .lane.width > 0' <<< "$state" >/dev/null \
        || fail "status: the $label strip has no transient to measure: lane=$lane disk=$disk"
    # The facts are right aligned inside a fixed zone, so their text begins at its right edge less its own width.
    gap=$(jq -r --argjson p "$padding" '(.disk.x + .disk.width - ([.disk.implicitWidth, .disk.width] | min))
        - (.lane.x + .lane.width) - $p' <<< "$state")
    printf 'TRANSIENT %s lane=%s disk=%s padding=%s gap=%s\n' "$label" "$lane" "$disk" "$padding" "$gap"
    awk -v g="$gap" 'BEGIN { exit (g < 1 && g > -1) ? 0 : 1 }' \
        || fail "status: the $label transient ends $gap px off one padding before the disk facts: lane=$lane disk=$disk"
    # Rule 2: the facts keep their zone. status_disk_x is taken before any transient exists.
    disk_now=$(jq -r '.disk.x' <<< "$state")
    [[ "${status_disk_x:-}" =~ ^[0-9.]+$ ]] || fail "status: the $label check has no disk baseline to hold to"
    [[ "$disk_now" == "$status_disk_x" ]] \
        || fail "status: the $label transient moved the disk facts from $status_disk_x to $disk_now"
}
case_status() {
    local dir="$fixture_root/status" failure status_disk_x
    sandbox_scratch "$dir"
    printf 'body\n' > "$dir/notes.txt"
    launch "$dir"
    wait_listing 1
    status_disk_x=$(ipc statusFooterState | jq -r '.disk.x')
    [[ "$status_disk_x" =~ ^[0-9.]+$ ]] || fail "status: the quiet strip reported no disk position, got [$status_disk_x]"
    sandbox_require "$dir"
    chmod u-w "$dir"
    click_row 0 right
    settle
    menu_seek "Duplicate"
    key -k Return >/dev/null
    settle
    failure=$(ipc lastMessage)
    [[ "$(ipc statusError)" == true && -n "$failure" ]] || fail "status: unwritable duplicate produced no error: $failure"
    sandbox_require "$dir"
    chmod u+w "$dir"
    key y >/dev/null
    settle
    [[ "$(ipc lastMessage)" == "$failure" ]] || fail "status: clipboard notice acknowledged error"
    key f >/dev/null
    key notes >/dev/null
    key -k Return >/dev/null
    settle
    [[ "$(ipc statusPrimary)" == "$failure" ]] || fail "status: search hid error"
    transient_beside_disk "single pane, error"
    # V7, StatusBar rule 4: a refusal is drawn alone, so the walk's own count yields while one stands
    # and comes back when it is acknowledged. Before this the two shared the zone in one sentence.
    [[ "$(ipc statusSecondary)" == " · esc dismisses" ]] \
        || fail "status: the refusal kept company other than its own key: $(ipc statusSecondary)"
    [[ "$(ipc statusColor)" == "$(ipc palette | cut -d' ' -f6)" ]] || fail "status: error lost its color"
    shot status-error-search
    key -k Escape >/dev/null
    settle
    [[ "$(ipc statusError)" == true ]] || fail "status: closing search acknowledged error"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc statusError)" == false ]] || fail "status: Escape did not acknowledge error"
    # V7: a result's hint is drawn on the secondary the way the undo hint is, so the sentence itself
    # is one fact and does not end in advice. The clipboard note put up before the search says both.
    [[ "$(ipc statusPrimary)" == "Copied 1 item" && "$(ipc statusSecondary)" == " · p pastes" ]] \
        || fail "status: the clipboard note reads $(ipc statusPrimary)$(ipc statusSecondary)"
    shot status-dismissed
    transient_beside_disk "single pane"
    shot status-transient
    # A selection widens the counts zone, which is what moves the leftover between the two zones and
    # what the old centring followed. The strip's own midpoint does not move, so this state is the one
    # that tells the two rules apart.
    key -M ctrl -k a -m ctrl >/dev/null
    settle
    [[ "$(ipc statusFooterState | jq -r '.left.text')" == *selected* ]] \
        || fail "status: select all did not widen the counts zone, it reads $(ipc statusFooterState | jq -r '.left.text')"
    transient_beside_disk "single pane, selection"
    shot status-transient-selection
    key -k Escape >/dev/null
    settle
    # The trash keeps the same rule, so a transient is put up on the listing first and carried in:
    # the trash view raises none of its own, and a lane with nothing in it measures nothing.
    key y >/dev/null
    settle
    click_rail_row "$(rail_row_of 'Trash')" left
    settle
    settle
    # Or the measurement below is the listing's again, taken under a name that says otherwise.
    [[ "$(ipc statusFooterState | jq -r '.path')" == "Trash" ]] \
        || fail "status: the Trash row did not open the trash, the strip still says $(ipc statusFooterState | jq -r '.path')"
    transient_beside_disk "trash"
    shot status-transient-trash
    key -k Escape >/dev/null
    settle
    [[ "$(ipc statusFooterState | jq -r '.path')" == "$dir" ]] \
        || fail "status: Escape did not return the strip to $dir, it says $(ipc statusFooterState | jq -r '.path')"
    kill_flea
}

case_operations() {
    local dir="$fixture_root/operations"
    sandbox_scratch "$dir"
    printf 'body\n' > "$dir/notes.txt"
    magick -size 48x32 xc:navy "$dir/shot.png"
    bsdtar -a -c -f "$dir/bundle.tar.zst" -C "$dir" notes.txt
    # #165: the card case needs an extraction still running when the harness looks.
    python3 - "$dir/slowpayload" <<'PY' || fail "operations: the slow extraction fixture could not be populated"
from pathlib import Path
import sys

root = Path(sys.argv[1])
root.mkdir()
for index in range(100000):
    (root / f"member_{index}").touch()
PY
    bsdtar -a -c -f "$dir/slow.zip" -C "$dir/slowpayload" . \
        || fail "operations: the slow extraction archive could not be built"

    launch "$dir"
    wait_listing 5

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
    # Same fall-through as the settings panel: a click on the card's title must not close the popup.
    local cwx cwy ctx cty
    read -r cwx cwy _ _ < <(window_box) || fail "native window coordinates unavailable"
    read -r ctx cty <<< "$(ipc convertTitleCentre)"
    [[ -n "$cty" ]] || fail "operations: the convert popup has no title to click"
    omarchy-drive click "$((cwx + ctx))" "$((cwy + cty))" left >/dev/null
    settle
    [[ "$(ipc convertOpen)" == "true" ]] || fail "operations: a click on the popup's own title closed it"
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

    # #165: Extract drives the copy card, and its Escape cancel must leave no destination.
    echo "-- extract in the copy card, cancelled with the transfer's own key --"
    local state deadline
    seek_row_named "slow.zip"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Extract"
    key -k Return >/dev/null
    deadline=$((SECONDS + 20))
    state=""
    while (( SECONDS < deadline )); do
        state=$(ipc statusActivityState) || fail "operations: extract-card observation failed"
        jq -e '.transferCard.visible and (.transferCard.byteLine == "") and (.transferCard.cancel.visible and .transferCard.cancel.enabled) and (.activities[0].text | startswith("Extracting · slow.zip"))' <<< "$state" >/dev/null && break
        sleep 0.05
    done
    jq -e '.transferCard.visible and (.transferCard.byteLine == "") and (.transferCard.cancel.visible and .transferCard.cancel.enabled) and (.activities[0].text | startswith("Extracting · slow.zip"))' <<< "$state" >/dev/null \
        || fail "operations: no live no-byte Extracting card before the deadline: $state"
    printf 'OPERATIONS extract-card=%s\n' "$state"
    shot operations-extracting
    key -k Escape >/dev/null
    deadline=$((SECONDS + 20))
    while (( SECONDS < deadline )); do
        state=$(ipc statusActivityState) || fail "operations: extract-cancel observation failed"
        jq -e '(.transferCard.visible | not) and (.activities | length) == 0' <<< "$state" >/dev/null && break
        sleep 0.05
    done
    jq -e '(.transferCard.visible | not) and (.activities | length) == 0' <<< "$state" >/dev/null \
        || fail "operations: a cancelled extraction never reached a terminal state: $state"
    [[ ! -e "$dir/slow" ]] || fail "operations: a cancelled extract published $dir/slow"
    printf 'OPERATIONS extract-cancelled=%s\n' "$state"

    echo "-- a completed extract writes the tree beside the archive --"
    seek_row_named "bundle.tar.zst"
    click_row "$(ipc cursor)" right
    settle
    menu_seek "Extract"
    key -k Return >/dev/null
    for _ in $(seq 1 80); do [[ -f "$dir/bundle/notes.txt" ]] && break; sleep 0.25; done
    [[ -f "$dir/bundle/notes.txt" ]] || fail "operations: extract wrote nothing, bar says $(ipc lastMessage)"
    printf 'OPERATIONS extracted=%s\n' "$(ipc lastMessage)"
    kill_flea
}

# Issue: after dd the cursor lands on the row that took the removed one's place, which for a block
# is the row after the block and not the row below where the cursor happened to sit. PR 53, W4HO-ham.
case_dd() {
    local dir="$fixture_root/dd"
    sandbox_scratch "$dir"
    local i
    for i in 1 2 3 4 5 6; do printf 'body\n' > "$dir/f$i.txt"; done
    # The trash this case fills is its own, inside the sandbox this case owns, never the operator's.
    export XDG_DATA_HOME="$fixture_root/dd-data"
    mkdir -p "$XDG_DATA_HOME"

    launch "$dir"
    wait_listing 6

    echo "-- a block leaves together, and the cursor takes the block's place --"
    goto_row 1
    key v >/dev/null
    key J >/dev/null
    settle
    [[ "$(ipc selectedIndices)" == "1,2" ]] || fail "dd: the block is $(ipc selectedIndices), not rows 1,2"
    key d >/dev/null
    wait_message "Press d again to trash, or Delete on its own."
    key d >/dev/null
    for _attempt in $(seq 1 40); do [[ -e "$dir/f3.txt" ]] || break; sleep 0.25; done
    [[ -e "$dir/f3.txt" ]] && fail "dd: the block was not trashed, the bar reads $(ipc lastMessage)"
    # Hard rule 9 in the case itself: the rows went to this case's own trash and not the operator's.
    [[ -s "$XDG_DATA_HOME/Trash/files/f2.txt" && -s "$XDG_DATA_HOME/Trash/files/f3.txt" ]] \
        || fail "dd: the block did not land in $XDG_DATA_HOME/Trash/files, which holds $(ls -A "$XDG_DATA_HOME/Trash/files" 2>&1)"
    wait_listing 4
    settle
    [[ "$(ipc cursor)" == "1" ]] \
        || fail "dd: after a block the cursor is $(ipc cursor), not 1, the row the block left"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "f4.txt|"* ]] \
        || fail "dd: after a block the cursor sits on $(ipc rowAt "$(ipc cursor)"), not the row that slid up"
    printf 'DD block row=%s cursor=%s\n' "$(ipc rowAt "$(ipc cursor)")" "$(ipc cursor)"

    echo "-- one row, and the cursor keeps its own index --"
    key -k Escape >/dev/null
    settle
    seek_row_named "f5.txt"
    local at
    at=$(ipc cursor)
    key d >/dev/null
    wait_message "Press d again to trash, or Delete on its own."
    key d >/dev/null
    for _attempt in $(seq 1 40); do [[ -e "$dir/f5.txt" ]] || break; sleep 0.25; done
    [[ -e "$dir/f5.txt" ]] && fail "dd: the row was not trashed, the bar reads $(ipc lastMessage)"
    wait_listing 3
    settle
    [[ "$(ipc cursor)" == "$at" ]] || fail "dd: the cursor left row $at for $(ipc cursor)"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "f6.txt|"* ]] \
        || fail "dd: the cursor sits on $(ipc rowAt "$(ipc cursor)"), not the row that slid up"

    # The delete's anchor selects the row it landed on, so the next dd would take that row and not the
    # cursor's: Escape is what hands the keyboard back to the cursor rule.
    [[ "$(ipc selectionCount)" == "1" ]] \
        || fail "dd: the row the cursor landed on is not selected, count is $(ipc selectionCount)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "0" ]] || fail "dd: Escape left $(ipc selectionCount) rows selected"

    echo "-- the last row clamps rather than running past the end --"
    seek_row_named "f6.txt"
    at=$(ipc cursor)
    key d >/dev/null
    wait_message "Press d again to trash, or Delete on its own."
    key d >/dev/null
    for _attempt in $(seq 1 40); do [[ -e "$dir/f6.txt" ]] || break; sleep 0.25; done
    [[ -e "$dir/f6.txt" ]] && fail "dd: the last row was not trashed, the bar reads $(ipc lastMessage)"
    wait_listing 2
    settle
    [[ "$(ipc cursor)" == "$((at - 1))" ]] \
        || fail "dd: after the last row the cursor is $(ipc cursor), not $((at - 1))"
    [[ "$(ipc rowAt "$(ipc cursor)")" == "f4.txt|"* ]] \
        || fail "dd: the clamped cursor sits on $(ipc rowAt "$(ipc cursor)")"
    kill_flea
}

# Paste onto a name that exists: asked once, and Cancel, Keep both, Skip and Replace with its Undo each do
# what the card says. Replace fills this case's own trash, inside the sandbox, and Undo empties it again.
# Every collide-<state> shot is one the canvas board draws, for the controller to compare by eye.
case_collide() {
    local dir="$fixture_root/collide" menus_checks=0 name wx wy rx ry
    sandbox_scratch "$dir"
    mkdir -p "$dir/to"
    for name in a b c d; do printf 'yours %s\n' "$name" > "$dir/$name.txt"; done
    printf 'notes\n' > "$dir/notes.txt"
    # Text bodies under a text name: a .png holding text makes the preview column log a decode failure, which the log check refuses.
    printf 'yours\n' > "$dir/photo.txt"
    printf 'there\n' > "$dir/to/photo.txt"
    export XDG_DATA_HOME="$fixture_root/collide-data"
    sandbox_scratch "$XDG_DATA_HOME"
    # Undo finds a replaced item through gio trash --list, which only a gvfsd started with this XDG_DATA_HOME answers, so the window gets a private session bus.
    local bus
    mapfile -t bus < <(dbus-daemon --session --fork --print-address=1 --print-pid=1)
    [[ ${#bus[@]} -eq 2 && "${bus[1]}" =~ ^[0-9]+$ ]] || fail "collide: no private session bus, dbus-daemon printed: ${bus[*]}"
    collide_bus_pid=${bus[1]}
    trap '( kill_flea ) >/dev/null 2>&1 || true; kill "$collide_bus_pid" 2>/dev/null || true' EXIT
    export DBUS_SESSION_BUS_ADDRESS="${bus[0]}"

    # A finished transfer's own line, polled because it stands only until the bar clears it; the undo hint tells it from the clipboard's.
    collide_said() {
        local want="$1" seen=""
        for _attempt in $(seq 1 300); do
            seen=$(ipc lastMessage)
            [[ "$seen" == "$want"* ]] && return 0
            sleep 0.05
        done
        fail "collide: the bar never said $want, it said $seen"
    }
    collide_until() {
        local what="$1"; shift
        for _attempt in $(seq 1 300); do
            "$@" && return 0
            sleep 0.05
        done
        fail "collide: $what"
    }
    collide_holds() { [[ "$(cat "$1" 2>/dev/null)" == "$2" ]]; }
    collide_absent() { [[ ! -e "$1" && ! -L "$1" ]]; }
    collide_untouched() { collide_absent "$dir/to/photo copy.txt" && collide_absent "$dir/to/notes.txt" && collide_absent "$dir/to/c.txt" && collide_holds "$dir/to/photo.txt" there; }
    # The six files onto the clipboard and the pane into to, which is the whole of every paste below.
    collide_copy_six() {
        launch "$dir"
        wait_listing 7
        seek_row_named "a.txt"
        key v >/dev/null
        for _ in 1 2 3 4 5; do key J >/dev/null; done
        settle
        [[ "$(ipc selectedIndices)" == "1,2,3,4,5,6" ]] || fail "collide: the six files are not selected, $(ipc selectedIndices) is"
        key y >/dev/null
        menus_expect keyDeliveryState '(.clipboard.paths | length) == 6 and (.clipboard.cut | not)' "the six files are on the clipboard"
        seek_row_named "to"
        key -k Return >/dev/null
        wait_path "$dir/to"
    }
    # Nothing on the card can wrap: the title and the names elide, and the one explanation line stays one line.
    collide_one_line() {
        menus_expect collideState '.buttonsFit and (.titleTruncated | not) and .explainLines == 1 and ([.buttons[].visible] | all)' "$1"
    }

    collide_copy_six
    wait_listing 1

    echo "-- asked once, on Keep both, and Cancel sends nothing --"
    key p >/dev/null
    menus_expect collideState '.opened and .title == "photo.txt already exists in to" and .names == ["photo.txt"] and .more == ""
        and .explain == "Replaced items go to Trash, and Undo restores them." and .focus == "keep"' "one collision is asked about once, on Keep both"
    collide_one_line "the one-name card keeps every line to one line"
    shot collide-one-keep
    key -M ctrl -k Return -m ctrl >/dev/null
    settle
    menus_expect collideState '.opened' "a modified Enter is ignored"

    echo "-- the mouse back button leaves the pane where it is behind the card --"
    command -v ydotool >/dev/null || fail "collide: ydotool is missing, so the mouse back button cannot be pressed"
    export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-$XDG_RUNTIME_DIR/.ydotool_socket}"
    [[ -S "$YDOTOOL_SOCKET" ]] || fail "collide: no ydotoold socket at $YDOTOOL_SOCKET"
    read -r wx wy _ _ < <(window_box) || fail "native window coordinates unavailable"
    read -r rx ry <<< "$(ipc rowCentre 0)"
    [[ -n "$rx" && -n "$ry" ]] || fail "collide: row 0 has no on-screen centre to park the pointer on"
    omarchy-drive move "$((rx + wx))" "$((ry + wy))" >/dev/null || fail "collide: the pointer could not be parked over the window"
    ydotool click 0xC3 >/dev/null 2>&1 || fail "collide: ydotool refused the mouse back button"
    settle
    settle
    [[ "$(ipc path)" == "$dir/to" ]] || fail "collide: the mouse back button took the pane to $(ipc path) behind the card"
    menus_expect collideState '.opened and .focus == "keep"' "the mouse back button leaves the card as it was"

    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels"
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 6 and (.paneFocus or .listFocus)' "Cancel keeps the clipboard and hands the list back"
    settle
    collide_untouched || fail "collide: Cancel changed $dir/to: $(ls -A "$dir/to")"
    # The control: the same press at the same spot with the card shut does go back, so the one above reached the pane and was refused.
    ydotool click 0xC3 >/dev/null 2>&1 || fail "collide: ydotool refused the control press"
    wait_path "$dir"
    seek_row_named "to"
    key -k Return >/dev/null
    wait_path "$dir/to"
    wait_listing 1

    echo "-- Enter takes Keep both, and Undo takes the copies away --"
    key p >/dev/null
    menus_expect collideState '.opened and .focus == "keep"' "the card opens on Keep both again"
    key -k Return >/dev/null
    collide_said "Copied 6 items · z undoes"
    collide_holds "$dir/to/photo copy.txt" yours || fail "collide: Keep both wrote no photo copy.txt"
    collide_holds "$dir/to/photo.txt" there || fail "collide: Keep both touched the photo.txt already there"
    collide_holds "$dir/to/notes.txt" notes || fail "collide: Keep both did not copy the rest"
    key z >/dev/null
    collide_until "Undo left the kept copies in place" collide_untouched

    echo "-- Skip leaves the collision and copies the rest --"
    key p >/dev/null
    menus_expect collideState '.opened' "the card opens for Skip"
    key h >/dev/null
    menus_expect collideState '.focus == "skip"' "h moves the focus to Skip"
    key -k Return >/dev/null
    collide_said "Copied 5 of 6 · 1 skipped · z undoes"
    # The files are small, so the transfer card is gone by now and the line it leaves is what shows.
    shot collide-skip-done
    collide_holds "$dir/to/photo.txt" there || fail "collide: Skip touched the photo.txt already there"
    collide_absent "$dir/to/photo copy.txt" || fail "collide: Skip kept a copy"
    collide_holds "$dir/to/notes.txt" notes || fail "collide: Skip did not copy the rest"
    key z >/dev/null
    collide_until "Undo left the skipped transfer's copies in place" collide_untouched

    echo "-- Replace sends the old item to Trash, and one Undo swaps them back --"
    key p >/dev/null
    menus_expect collideState '.opened' "the card opens for Replace"
    key l >/dev/null
    menus_expect collideState '.focus == "replace"' "l moves the focus to Replace"
    shot collide-one-replace
    key -k Return >/dev/null
    collide_said "Copied 6 items · z undoes"
    collide_holds "$dir/to/photo.txt" yours || fail "collide: Replace did not put the incoming photo.txt in place"
    collide_holds "$XDG_DATA_HOME/Trash/files/photo.txt" there \
        || fail "collide: the replaced photo.txt is not in this case's trash: $(ls -A "$XDG_DATA_HOME/Trash/files" 2>&1)"
    shot collide-replaced
    key z >/dev/null
    collide_until "Undo did not restore the replaced photo.txt" collide_untouched
    collide_absent "$XDG_DATA_HOME/Trash/files/photo.txt" || fail "collide: Undo left the old photo.txt in Trash as well"

    echo "-- several names, all of them listed --"
    for name in a b; do printf 'there %s\n' "$name" > "$dir/to/$name.txt"; done
    key p >/dev/null
    menus_expect collideState '.opened and .title == "3 items already exist in to" and .names == ["a.txt","b.txt","photo.txt"] and .more == ""' \
        "three collisions are all named, with no more line"
    collide_one_line "the several-name card keeps every line to one line"
    shot collide-several
    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels the several-name card"

    echo "-- more names than the card lists: three of them, and the rest counted --"
    for name in c d; do printf 'there %s\n' "$name" > "$dir/to/$name.txt"; done
    key p >/dev/null
    menus_expect collideState '.opened and .title == "5 items already exist in to" and .names == ["a.txt","b.txt","c.txt"] and .more == "and 2 more"' \
        "five collisions name three and count the rest"
    collide_one_line "the and-more card keeps every line to one line"
    shot collide-more
    key -k Tab >/dev/null
    menus_expect collideState '.focus == "replace"' "Tab moves on from Keep both"
    key -k Tab >/dev/null
    menus_expect collideState '.focus == "cancel"' "and comes round to Cancel"
    key -k Return >/dev/null
    menus_expect collideState '.opened | not' "Enter on Cancel cancels"
    settle
    for name in a b c d; do collide_holds "$dir/to/$name.txt" "there $name" || fail "collide: Cancel touched $name.txt"; done
    collide_holds "$dir/to/photo.txt" there || fail "collide: Cancel touched photo.txt"

    echo "-- at the largest text-size stop nothing on the card wraps --"
    seed_ui_state "$fixture_root/collide-state" '{"display":{"textSize":{"mode":20}}}'
    collide_copy_six
    wait_listing 5
    [[ "$(ipc bodyPx)" == 20 ]] || fail "collide: the seeded largest stop draws at $(ipc bodyPx)px, not 20"
    key p >/dev/null
    menus_expect collideState '.opened and .names == ["a.txt","b.txt","c.txt"] and .more == "and 2 more"' "the card opens at the largest stop"
    collide_one_line "at the largest stop the buttons fit on one row and no line wraps"
    shot collide-largest-text
    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels at the largest stop"
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

    # Issue 162: j moves the row of tiles Down moved, and k undoes it; a [[text]] chord goes bare.
    local jfrom jdown
    jfrom=$(ipc cursor)
    key j >/dev/null
    settle
    jdown=$(ipc cursor)
    (( jdown == jfrom + (down - start) )) \
        || fail "grid: j moved $((jdown - jfrom)) from $jfrom, not the row of tiles Down moved"
    key k >/dev/null
    settle
    (( $(ipc cursor) == jfrom )) || fail "grid: k did not undo the row j moved"

    click_chrome list
    settle
    [[ "$(ipc viewMode)" == "list" ]] || fail "grid: the list button did not switch back"
    # The keyboard has to come back with the view, or the hidden grid would still be holding focus.
    start=$(ipc cursor)
    key -k Down >/dev/null
    settle
    (( $(ipc cursor) == start + 1 )) \
        || fail "grid: after switching back the list did not take one step, so focus stayed with the grid"
    # Issue 162 is the grid's only: the same letter is one row in the list.
    local lstart lnext
    lstart=$(ipc cursor)
    key j >/dev/null
    settle
    lnext=$(ipc cursor)
    (( lnext == lstart + 1 )) || fail "grid: in the list j moved $((lnext - lstart)), not one item"
    kill_flea
}

grid_chrome_inventory() {
    local view="$1" available="${2:-true}" glyph state visible
    for glyph in search filter sort list columns grid dual sliders; do
        visible=true
        case "$glyph:$view" in search:grid|filter:list|filter:columns|sort:list|sort:columns) visible=false ;; esac
        state=$(ipc chromeButtonState "$glyph") || fail "grid chrome: $glyph observer failed"
        jq -e --argjson visible "$visible" '.visible == $visible' <<< "$state" >/dev/null \
            || fail "grid chrome: $view has the wrong $glyph visibility: $state"
        if [[ "$view" == grid && ( "$glyph" == filter || "$glyph" == sort ) ]]; then
            jq -e --argjson enabled "$available" '.enabled == $enabled' <<< "$state" >/dev/null \
                || fail "grid chrome: $glyph eligibility differs from the search context: $state"
        fi
        printf 'GRID_CHROME_CONTROL view=%s state=%s\n' "$view" "$state"
    done
}

grid_chrome_controls() {
    local preset="$1" dir="$2" menus_checks=0 view order first glyph before query index
    local -a search_names=(file-01.txt file-02.txt file-03.txt file-04.txt file-05.txt file-06.txt file-07.txt file-08.txt file-09.txt file-10.txt file-20.txt file-30.txt file-40.txt file-50.txt file-60.txt)
    for view in list columns grid; do
        click_chrome "$view"
        cardsize_expect viewMode "$view"
        grid_chrome_inventory "$view"
    done
    click_chrome filter
    menus_expect keyDeliveryState '.filterTyping and .filterQuery == "" and (.paneFocus or .listFocus)' \
        "Grid Filter pointer activation gives the query keyboard focus"
    key file-0 >/dev/null
    menus_expect keyDeliveryState '.filterTyping and .filterQuery == "file-0"' "Grid Filter receives actual typed text"
    cardsize_expect drawnCount 9
    key -k Return -k Home >/dev/null
    menus_expect keyDeliveryState '(.filterTyping | not) and .filterQuery == "file-0" and (.paneFocus or .listFocus)' \
        "Grid Filter commits with keyboard focus in the results"
    cardsize_expect visibleRowName file-01.txt 0
    shot "grid-chrome-$preset-filter"
    key -k Escape >/dev/null
    cardsize_expect drawnCount 61
    cardsize_expect sortMark name:asc
    click_row 3 left
    cardsize_expect selectedIndices 3
    for order in size mtime kind name; do
        case "$order" in size|kind) first=file-59.json ;; mtime) first=file-60.txt ;; name) first=file-01.txt ;; esac
        click_chrome sort
        cardsize_expect sortMark "$order:asc"
        cardsize_expect visibleRowName "$first" 0
        cardsize_expect cursor 0
        cardsize_expect selectionCount 0
        printf 'GRID_CHROME_SORT preset=%s order=%s first=%s\n' "$preset" "$order" "$(ipc visibleRowName 0)"
    done
    key -M ctrl -k f -m ctrl >/dev/null
    menus_expect keyDeliveryState '.searchMode == "typing" and .searchQuery == "" and (.filterTyping | not)' "Grid Search opens through its retained key"
    for view in typing results; do
        grid_chrome_inventory grid false
        before=$(ipc sortMark)
        query=""
        [[ "$view" != results ]] || query=file-0
        for glyph in filter sort; do
            click_chrome "$glyph"
            click_chrome "$glyph"
            menus_expect keyDeliveryState ".searchMode == \"$view\" and .searchQuery == \"$query\" and (.filterTyping | not) and .filterQuery == \"\"" \
                "disabled Grid $glyph refuses repeated pointer activation during search $view"
            cardsize_expect sortMark "$before"
        done
        shot "grid-chrome-$preset-search-$view-disabled"
        if [[ "$view" == typing ]]; then
            key file-0 -k Return >/dev/null
            menus_expect keyDeliveryState '.searchMode == "results" and .searchQuery == "file-0" and (.searchRunning | not)' \
                "Grid Search finishes its actual fixture walk"
            cardsize_expect path "$dir"
            wait_listing "${#search_names[@]}"
            cardsize_expect drawnCount "${#search_names[@]}"
            for index in "${!search_names[@]}"; do
                cardsize_expect visibleRowName "${search_names[$index]}" "$index"
            done
        fi
    done
    key -k Escape >/dev/null
    menus_expect keyDeliveryState '.searchMode == "" and (.filterTyping | not)' "closing Grid Search restores browsing"
    wait_listing 61
    cardsize_expect drawnCount 61
    grid_chrome_inventory grid true
    click_chrome filter
    menus_expect keyDeliveryState '.filterTyping and .filterQuery == ""' "Grid Filter re-enables after search closes"
    key -k Escape >/dev/null
    printf 'GRID_CHROME preset=%s filter_focus=ok sort_cycle=ok search_refusal=ok view_inventory=ok\n' "$preset"
}

# Move through measured rows and columns to the exact fixture tile.
grid_move_to() {
    local target="$1" columns="$2" i
    key -k Home >/dev/null
    for ((i = 0; i < target / columns; i++)); do key -k Down >/dev/null; done
    for ((i = 0; i < target % columns; i++)); do key -k Right >/dev/null; done
    cardsize_expect cursor "$target"
}

case_gridnavigation() {
    local dir="$fixture_root/grid-navigation" preset i columns next_columns edge selected_name
    local start_x start_y target_x target_y caption
    local opener="$fixture_root/grid-opener"
    sandbox_scratch "$dir"
    sandbox_scratch "$opener"
    printf '#!/bin/sh\n[ "$1" = open ] || exec /usr/bin/gio "$@"\nexit 1\n' > "$opener/$open_handoff"
    chmod +x "$opener/$open_handoff"
    # Distinct MIME and mtime make each Sort press visibly reorder real fixture files.
    for i in $(seq -w 1 60); do
        if [[ "$i" == 59 ]]; then printf '{"grid":59}\n' > "$dir/file-$i.json"
        else printf 'grid navigation %s\n' "$i" > "$dir/file-$i.txt"; fi
    done
    printf 'grid caption\n' > "$dir/long-grid-caption-with-enough-words-to-wrap-and-truncate-after-two-complete-lines.txt"
    sandbox_require "$dir/file-60.txt"
    touch -d '2000-01-01 UTC' "$dir/file-60.txt" || fail "grid: distinct mtime fixture could not be set"
    for preset in default vim mac windows; do
        seed_ui_state "$fixture_root/grid-state" "{\"keys\":\"$preset\",\"view\":\"list\",\"wrapAtEnds\":true,\"preview\":{\"thumbnails\":\"off\"}}"
        PATH="$opener:$PATH" launch "$dir"
        wait_listing 61
        click_chrome grid
        cardsize_expect viewMode grid
        permissions_viewport 1100 800
        grid_chrome_controls "$preset" "$dir"
        columns=$(ipc gridColumns)
        [[ "$columns" =~ ^[0-9]+$ ]] && (( columns > 1 && columns < 30 )) || fail "grid: invalid measured column count $columns"
        click_row "$((columns - 1))" left
        key j >/dev/null
        cardsize_expect cursor "$((2 * columns - 1))"
        key k >/dev/null
        cardsize_expect cursor "$((columns - 1))"
        key -k Right >/dev/null
        cardsize_expect cursor "$((columns - 1))"
        key -k Home -k Down -k Left >/dev/null
        cardsize_expect cursor "$columns"
        read -r start_x start_y <<< "$(ipc rowCentre "$columns")"
        key -k Down >/dev/null
        cardsize_expect cursor "$((columns * 2))"
        read -r target_x target_y <<< "$(ipc rowCentre "$((columns * 2))")"
        (( start_x == target_x && target_y > start_y )) || fail "grid: Down did not reach the next visual cell"
        key -k Up >/dev/null
        cardsize_expect cursor "$columns"
        key -k Home -k Up -k Left >/dev/null
        cardsize_expect cursor 0
        key -k End -k Down -k Right >/dev/null
        cardsize_expect cursor 60
        caption=$(ipc gridCaptionState 60)
        jq -e '.lines == 2 and .truncated and .textHeight <= .slotHeight and .bottom <= .tileHeight' <<< "$caption" >/dev/null \
            || fail "grid: two-line caption leaves its reserved tile slot: $caption"
        shot "grid-navigation-$preset-two-line-caption"
        # Prime fixture count guarantees an incomplete row at every admitted column count.
        (( 61 % columns != 0 )) || fail "grid: fixture did not produce an incomplete row"
        edge=$((61 - columns))
        grid_move_to "$edge" "$columns"
        key -k Down >/dev/null
        cardsize_expect cursor "$edge"
        key -k Escape >/dev/null
        cardsize_expect selectionCount 0
        grid_move_to 6 "$columns"
        key v >/dev/null
        cardsize_expect selectedIndices 6
        selected_name=$(ipc visibleRowName 6)
        [[ "$selected_name" == file-07.txt ]] || fail "grid: selected visible tile identity unavailable"
        permissions_viewport 800 600
        next_columns=$(ipc gridColumns)
        (( next_columns > 1 && next_columns < columns )) || fail "grid: narrower native viewport did not reflow $columns columns"
        cardsize_expect cursor 6
        cardsize_expect selectedIndices 6
        [[ "$(ipc visibleRowName 6)" == "$selected_name" ]] || fail "grid: reflow changed the selected item's identity"
        key -k Down >/dev/null
        cardsize_expect cursor "$((6 + next_columns))"
        cardsize_expect selectedIndices 6
        key -k Up >/dev/null
        cardsize_expect cursor 6
        shot "grid-navigation-$preset-reflow"
        key -k Escape -k Home -M shift -k Down -m shift >/dev/null
        cardsize_expect cursor "$next_columns"
        cardsize_expect selectionCount "$((next_columns + 1))"
        key -k Escape /file-0 -k Return -k Home >/dev/null
        printf 'GRID_FILTER state=%s\n' "$(ipc keyDeliveryState)"
        jq -e '.filterQuery == "file-0" and (.filterTyping | not)' <<< "$(ipc keyDeliveryState)" >/dev/null \
            || fail "grid: native filter text or committed state differs"
        cardsize_expect drawnCount 9
        cardsize_expect cursor 0
        cardsize_expect visibleRowName file-01.txt 0
        key -k Down >/dev/null
        cardsize_expect cursor "$next_columns"
        key v >/dev/null
        cardsize_expect selectedIndices "$next_columns"
        shot "grid-navigation-$preset-filtered"
        key -k Escape /no-such-tile -k Return >/dev/null
        cardsize_expect drawnCount 0
        [[ -z "$(ipc visibleRowName "$next_columns")" ]] || fail "grid: hidden filtered tile still has a visible delegate"
        shot "grid-navigation-$preset-zero-matches"
        key -k Escape >/dev/null
        cardsize_expect drawnCount 61
        printf 'GRID_NAVIGATION preset=%s columns=%s reflow=%s item_keys=ok cell_edges=ok identity=ok range=ok\n' "$preset" "$columns" "$next_columns"
        kill_flea
    done
}

case_header() {
    launch "$repo"
    local titles mark
    titles=$(ipc headerTitles)
    mark=$(ipc sortMark)
    printf 'HEADER titles=%s mark=%s\n' "$titles" "$mark"
    shot header
    [[ "$titles" == "Name|Mode|Size|Modified|Kind" ]] || fail "header: titles are $titles"
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
    # No ui.json at all, a first run's state, so these rows take the window's own default density.
    sandbox_scratch "$fixture_root/icons-state"
    export XDG_STATE_HOME="$fixture_root/icons-state"
    launch "$dir"
    wait_listing 5
    local row_height i y0 y1 pitch
    row_height=$(ipc fileRowHeight)
    (( row_height < $(ipc metrics | cut -d' ' -f4) )) || fail "a first run drew $row_height px rows, not the compact default below the board's row"
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
    seed_ui_state "$fixture_root/thumbs-state" '{"view":"list"}'
    launch "$thumb_fixture"
    wait_listing "$thumb_rows"
    [[ "$(ipc viewMode)" == list ]] || fail "thumbs: fixture did not open its list view"
    # Read before any thumbnail lands, so the slot is compared against its pre-thumbnail row.
    local row_height
    row_height=$(ipc fileRowHeight)

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

    # The fling has to move the viewport, or the request bounds below would pass on a list that never scrolled.
    local wx wy ww wh before_requests moved cursor_a cursor_b
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    before_requests=$(ipc thumbRequests)
    # Read before the fling starts: the list saturates within its first notches, 21 of 1500 here, so a sample taken during it already reads the end.
    cursor_a=$(ipc cursor)
    omarchy-drive scroll down "$fling_clicks" >/dev/null
    sleep 0.5
    cursor_b=$(ipc cursor)
    moved=0
    [[ "$cursor_b" != "$cursor_a" ]] && moved=1
    printf 'THUMBS fling before=%s cursor=%s..%s moved=%s after=%s\n' \
        "$before_requests" "$cursor_a" "$cursor_b" "$moved" "$(ipc thumbRequests)"
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
    # A fling that moves nothing fails here instead of passing quietly; "nothing is requested while moving" is carried by the request-count bounds above.
    (( moved >= 1 )) \
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
    local state="$stale_fixture/state"
    sandbox_make "$stale_fixture"
    mkdir -p "$pics" "$src" "$cache/thumbnails/large" "$cache/thumbnails/fail"
    magick -size 512x512 xc:red "$src/before.jpg"
    magick -size 512x512 xc:blue "$src/after.jpg"
    cp "$src/before.jpg" "$pics/one.jpg"
    # Exported inside this case's own subshell, so no other case reads or writes the redirected root.
    export XDG_CACHE_HOME="$cache"
    seed_ui_state "$state" '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"images"}}'
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

    # The columns view draws the slot with its own Image, so the same regeneration runs once more under it, back to red.
    local col_crop red_cols blue_cols
    switch_view columns
    for _attempt in $(seq 1 50); do
        [[ "$(ipc rowThumbReady 0)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc rowThumbReady 0)" == "true" ]] || fail "columns: the row never decoded its thumbnail"
    read -r cx cy cw ch <<< "$(ipc rowThumbRect 0)"
    col_crop="${cw}x${ch}+${cx}+${cy}"
    shot stale-columns-before
    blue_cols=$(count_pixels "$evidence_dir/stale-columns-before.png" "$col_crop" "$icon_blue")
    (( blue_cols > 0 )) || fail "columns: the row draws no blue pixel of the current thumbnail in $col_crop"
    cp "$src/before.jpg" "$pics/one.jpg"
    touch -d "@$(( $(date +%s) - 2 * stale_mtime_back_s ))" "$pics/one.jpg"
    key h >/dev/null
    wait_path "$stale_fixture/tree"
    wait_listing 1
    click_row 0 left --double
    wait_path "$pics"
    wait_listing 1
    for _attempt in $(seq 1 50); do
        [[ "$(ipc rowThumbReady 0)" == "true" && "$(ipc thumbFile 0)" == "$first_file" ]] && break
        sleep 0.1
    done
    read -r cx cy cw ch <<< "$(ipc rowThumbRect 0)"
    col_crop="${cw}x${ch}+${cx}+${cy}"
    shot stale-columns-after
    red_cols=$(count_pixels "$evidence_dir/stale-columns-after.png" "$col_crop" "$icon_red")
    blue_cols=$(count_pixels "$evidence_dir/stale-columns-after.png" "$col_crop" "$icon_blue")
    printf 'STALE columns file=%q red=%s blue=%s\n' "$(ipc thumbFile 0)" "$red_cols" "$blue_cols"
    [[ "$(ipc thumbFile 0)" == "$first_file" ]] || fail "columns: the regenerated thumbnail landed at a new path, $(ipc thumbFile 0)"
    (( red_cols > 0 )) || fail "columns: the regenerated thumbnail drew no red pixel, so the row is showing the old frame"
    (( blue_cols == 0 )) || fail "columns: the row still draws $blue_cols blue pixels of the thumbnail it replaced"
    switch_view list
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
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
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
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
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
    # #181: opening a rail row hands focus to the folder it opened, so the next key moves in Home.
    [[ "$(ipc focusView)" == "list" ]] || fail "focus: Enter on a favourite left focus on $(ipc focusView), not the list"
    shot focus-opened
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "focus: tab did not return to the rail after an open"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "focus: escape did not return to the list"
    printf 'FOCUS view=%s railCursor=%s path=%s\n' "$(ipc focusView)" "$(ipc railCursor)" "$(ipc path)"
    shot focus-listed
    kill_flea
}

# Catches t not opening a tab, 1-9 not switching, w not closing, or the bar showing with one tab.
# Click a tab in the bar. The GUI leaves bare digits unbound, so this is the only pointer or key
# route a case has to a particular tab.
click_tab() {
    local index="$1" centre cx cy wx wy
    centre=$(ipc tabCentre "$index")
    [[ -n "$centre" ]] || fail "click_tab: tab $index has no centre"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box) || fail "click_tab: native window coordinates unavailable"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null
    settle
}

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
    # The bar is how a tab is chosen here: ui/js/Keymap.js answers 1 to 9 with a tab in the TUI
    # alone, and lookupFor leaves a bare digit unbound in this frontend on purpose.
    click_tab 0
    wait_path "$dir"
    [[ "$(ipc tabIndex)" == "0" ]] || fail "tabs: clicking tab 0 did not return to it, index=$(ipc tabIndex)"
    click_tab 1
    wait_path "$dir/alpha"
    [[ "$(ipc tabIndex)" == "1" ]] || fail "tabs: clicking tab 1 did not return to it, index=$(ipc tabIndex)"
    key w >/dev/null
    wait_path "$dir"
    [[ "$(ipc tabCount)" == "1" ]] || fail "tabs: w did not close the current tab, count=$(ipc tabCount)"
    [[ "$(ipc tabBarVisible)" == "false" ]] || fail "tabs: the bar stayed up after the last extra tab closed"
    key w >/dev/null
    wait_message "Can't close the last tab."
    shot tabs-one
    key t >/dev/null
    settle
    local centre cx cy wx wy
    centre=$(ipc tabCentre 0)
    [[ -n "$centre" ]] || fail "tabs: tab 0 has no centre"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
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
        setsid nohup qs -p "$flea_ui/boot" > "$log" 2>&1 </dev/null &
    local waited
    for waited in $(seq 1 200); do
        grep -aq 'graphics backend opengl failed' "$log" && break
        sleep 0.1
    done
    kill_flea
    printf 'RENDERER ran=%q\n' "$(tr '\n' ' ' < "$relaunched")"
    grep -aq 'graphics backend opengl failed' "$log" \
        || fail "no scene-graph error reached ui/boot/shell.qml, so its Connections never held the window"
    # A scene-graph failure now starts no backend at all, so counting FLEA_BIN is a zero denominator.
    local retried
    retried=$(grep -c -- '--gui' "$relaunched" || true)
    [[ "$retried" == "0" ]] || fail "the retry fired for a renderer the operator named: $(cat "$relaunched")"
    # The argv side of that arm, from a root carrying ui/boot's own two symlinks: the helper implicitly imports ui/, whose qmldir singletons import qs.Commons, and qs: resolves against the root.
    local proberoot="$dir/retry-cfg" probe="$dir/retry-probe.log"
    mkdir -p "$proberoot"
    cp -a "$flea_ui/boot/Commons" "$flea_ui/boot/Ui" "$proberoot/" \
        || fail "the boot root has no Commons or Ui to copy, so this probe would test its own fake root"
    cp "$repo/tests/renderer-retry.qml" "$proberoot/probe.qml"
    : > "$probe"
    env RETRY_HELPER="$flea_ui/RendererRetry.qml" RETRY_BACKEND=vulkan \
        FLEA_RENDERER_AUTOMATIC=1 FLEA_BIN="$dir/flea-stub" QT_QPA_PLATFORM=offscreen \
        timeout 20 qs -p "$proberoot/probe.qml" > "$probe" 2>&1
    local want='RETRY argv ["/usr/bin/env","QSG_RHI_BACKEND=opengl","'"$dir/flea-stub"'","--gui"]'
    grep -aqF "$want" "$probe" \
        || fail "the retry helper did not answer with the argv Renderer.js names: $(grep -a RETRY "$probe" | head -2)"
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
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
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

    # GM, 2026-09-11: space closes every kind, media included, and playback moved to its own p, which
    # ui/js/PreviewKeys.js reaches only in the media context. This block is that ruling, not Task 22's.
    key p >/dev/null
    wait_preview_state paused
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: p paused tone.wav but also closed the preview"
    pos_before=$(ipc previewPosition)
    key l >/dev/null
    settle
    pos_after=$(ipc previewPosition)
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: l closed the paused audio preview"
    [[ "$(ipc previewState)" == "paused" ]] || fail "preview: l changed paused audio to $(ipc previewState)"
    [[ "$pos_after" == "$pos_before" ]] \
        || fail "preview: l moved paused audio, before=$pos_before after=$pos_after"
    key p >/dev/null
    wait_preview_state playing

    # And the second space closes a playing media preview, the way it closes every other kind.
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: space did not close the playing audio preview"

    open_row_fast clip.mp4
    [[ "$(ipc previewKind)" == "video" ]] || fail "preview: clip.mp4 classified as $(ipc previewKind), not video"
    wait_preview_state playing
    shot preview-video
    [[ "$(ipc previewStrip | jq -r .visible)" == "true" ]] \
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
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
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
    [[ "$(ipc previewStrip | jq -r .visible)" == "true" ]] \
        || fail "preview: the strip hid before the slider interaction's own stripHideMs window expired"
    while (( SECONDS < interact_at + 5 )); do sleep 0.2; done
    [[ "$(ipc previewStrip | jq -r .visible)" == "false" ]] \
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
    omarchy-drive wait ipc -p "$flea_ui/boot" flea previewPdfPage 1 --timeout 10 >/dev/null \
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

    # The operator's ruling of 2026-09-11: escape was the only way out of a preview. A click on the
    # overlay's ground closes it, and a click on the surface still belongs to whatever pane is drawn
    # there. Both halves are asserted, because a shield that closes on every click is the same defect
    # in the other direction.
    local pwx pwy pww pwh
    open_row big.txt
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: the click-away fixture did not open"
    read -r pwx pwy pww pwh < <(window_box) || fail "preview: native window coordinates unavailable"
    # The surface is a centred fraction of the window, so a point near the top-left corner is ground.
    omarchy-drive click "$((pwx + 40))" "$((pwy + 140))" >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: a click on the ground did not close it"
    open_row big.txt
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: it did not reopen for the surface check"
    omarchy-drive click "$((pwx + pww / 2))" "$((pwy + pwh / 2))" >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: a click on the surface closed it"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the click-away preview"

    printf 'PREVIEW text=ok markdown=ok audio=ok video=ok toolarge=ok mediacontrols=ok striphide=ok clickaway=ok\n'
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
# Issue 21, TomFaulkner: a saved network place can be edited from the rail, and the address that
# finally mounts is written back over that place's own line rather than saved beside it.
case_editplace() {
    local dir="$fixture_root/editplace"
    sandbox_scratch "$dir"
    : > "$dir/one.txt"
    local fake_root="$fixture_root/editplace-fake"
    sandbox_scratch "$fake_root"
    mkdir -p "$fake_root/bin"
    local fixture_home="$fixture_root/editplace-home"
    fixture_home_make "$fixture_home"
    local real_home="$HOME" mount_log="$fake_root/mount.log"
    local bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    export XDG_CONFIG_HOME="$fixture_home/.config"
    seed_ui_state "$fixture_root/editplace-state" '{"view":"list","keys":"default","places":{"favourites":[]}}'
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf 'smb://legacy.test/data Legacy share\nfile:///missing/legacy Local legacy\n' > "$bookmarks"
    : > "$mount_log"

    # The same shape case_network's own fake has: this box's real mounts are the operator's, so the
    # rail under test is exactly the two seeded lines and nothing the box happens to have open.
    cat > "$fake_root/bin/gio" <<EOS
#!/bin/sh
case "\$1 \${2:-}" in
"mount -li") exit 0 ;;
"mount "*) printf '%s\n' "\$*" > "$mount_log" ;;
"info "*) printf 'local path: %s\n' "$dir" ;;
*) exit 0 ;;
esac
EOS
    chmod +x "$fake_root/bin/gio"
    local saved_path="$PATH"
    export PATH="$fake_root/bin:$PATH"

    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 1
    local _attempt
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == 'Legacy share|network|share|false' ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == 'Legacy share|network|share|false' ]] \
        || fail "editplace: the rail reads $(ipc networkEntries), not the one saved place"

    local legacy_index
    legacy_index=$(ipc railEntries | jq -r 'map(.label) | index("Legacy share")')
    [[ -n "$legacy_index" && "$legacy_index" != "null" ]] || fail "editplace: the saved place is not on the rail"

    click_rail_row "$legacy_index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Edit|Rename|Remove" ]] \
        || fail "editplace: the saved place offers $(ipc contextMenuEntries), not Edit, Rename and Remove"
    menu_seek "Edit"
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "editplace: Edit opened no dialog"
    [[ "$(ipc networkUri)" == "smb://legacy.test/data" ]] \
        || fail "editplace: the dialog opened on $(ipc networkUri), not the place Edit named"
    # The host holds the caret on open, so the old one is taken out and the corrected one typed in.
    local back
    for back in $(seq 1 24); do key -k BackSpace >/dev/null; done
    key "legacy2.test" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "smb://legacy2.test/data" ]] \
        || fail "editplace: the Mounts-as line reads $(ipc networkUri) after the address was corrected"
    key -k Return >/dev/null
    for _attempt in $(seq 1 200); do
        [[ "$(ipc dialogOpen)" == "false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "editplace: the dialog stayed open, it says $(ipc networkStatus)"
    # SMB mounts go through --anonymous, the same argv ui/NetworkMounts.qml sends for every share.
    [[ "$(cat "$mount_log")" == 'mount --anonymous smb://legacy2.test/data' ]] \
        || fail "editplace: the edit mounted $(cat "$mount_log")"
    local edited_marks
    edited_marks=$(cat "$bookmarks")
    [[ "$edited_marks" == 'smb://legacy2.test/data Legacy share
file:///missing/legacy Local legacy' ]] \
        || fail "editplace: the bookmarks file reads $edited_marks after the edit"
    # The rail reads the file it was rewritten in, so the place is one row carrying the new address.
    local rail_uri
    for _attempt in $(seq 1 100); do
        rail_uri=$(ipc railEntries | jq -r '[.[] | select(.group == "network")] | map(.uri) | join(",")')
        [[ "$rail_uri" == "smb://legacy2.test/data" ]] && break
        sleep 0.05
    done
    [[ "$rail_uri" == "smb://legacy2.test/data" ]] \
        || fail "editplace: the rail's network rows are $rail_uri, not the one corrected place"
    # The edit rewrites the place it came from, so Flea's own favourites gain nothing.
    [[ "$(ipc railEntries | jq -r '[.[] | select(.group == "favorite")] | length')" == "0" ]] \
        || fail "editplace: the edit added a favourite as well as rewriting the place"
    printf 'EDITPLACE edit=%s rail=%s\n' "$(head -1 "$bookmarks")" "$rail_uri"

    echo "-- an Edit nobody finished leaves the place alone when the next place is saved --"
    legacy_index=$(ipc railEntries | jq -r 'map(.label) | index("Legacy share")')
    [[ -n "$legacy_index" && "$legacy_index" != "null" ]] \
        || fail "editplace: the edited place left the rail, which carries $(ipc railEntries)"
    click_rail_row "$legacy_index" right
    settle
    menu_seek "Edit"
    key -k Return >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "editplace: the second Edit opened no dialog"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "editplace: Escape left the dialog open"
    # Without the dialog's own disarm this next mount rewrites the line the abandoned Edit armed.
    rail_focus
    key a >/dev/null
    settle
    key "other.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/share" >/dev/null
    key -k Return >/dev/null
    for _attempt in $(seq 1 200); do
        [[ "$(ipc dialogOpen)" == "false" ]] && break
        sleep 0.05
    done
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "editplace: the second place never saved, it says $(ipc networkStatus)"
    [[ "$(cat "$mount_log")" == 'mount --anonymous smb://other.test/share' ]] \
        || fail "editplace: the second place mounted $(cat "$mount_log")"
    settle
    [[ "$(cat "$bookmarks")" == "$edited_marks" ]] \
        || fail "editplace: the abandoned Edit rewrote the saved place, the file now reads $(cat "$bookmarks")"
    printf 'EDITPLACE abandoned=unchanged marks=%s\n' "$(head -1 "$bookmarks")"

    export PATH="$saved_path"
    kill_flea
}

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
    local real_home="$HOME" mount_log="$fake_root/mount.log" mount_calls="$fake_root/mount-calls"
    local state="$fixture_root/network-state" stored bookmarks="$fixture_home/.config/gtk-3.0/bookmarks"
    local real_state="${XDG_STATE_HOME-}" real_config="${XDG_CONFIG_HOME-}"
    export XDG_CONFIG_HOME="$fixture_home/.config"
    seed_ui_state "$state" '{"view":"list","keys":"default","places":{"favourites":[]}}'
    stored="$state/flea/ui.json"
    : > "$mount_log"
    : > "$mount_calls"
    mkfifo "$fake_root/mount-release"
    local races="$fixture_root/network-races" race_state="$fixture_root/network-race-state" name
    sandbox_scratch "$races"
    mkdir -p "$races/left" "$races/right/child" "$races/mounted" "$races/share-mounted"
    for name in a b c; do
        : > "$races/left/$name.txt"
        : > "$races/right/$name.txt"
    done
    for name in a b; do
        : > "$races/right/child/$name.txt"
        : > "$races/mounted/$name.txt"
        : > "$races/share-mounted/$name.txt"
    done
    for name in late origin shares child; do mkfifo "$fake_root/$name-release"; done

    network_cleanup() {
        local name fifo fd result=0
        local -a release_fds=()
        sandbox_require "$fake_root"
        sandbox_owned "$fake_root" || fail "network: refusing cleanup of an unowned fixture"
        for name in mount late origin shares child; do
            sandbox_require "$fake_root/$name-release"
            fifo="$SANDBOX_PATH"
            [[ -p "$fifo" && ! -L "$fifo" ]] || fail "network: cleanup barrier is not an owned FIFO: $fifo"
            # Keep both ends open through the reap so even a helper arriving late receives its release.
            exec {fd}<>"$fifo" || fail "network: could not open cleanup barrier: $fifo"
            release_fds+=("$fd")
            printf 'release\n' >&"$fd" || fail "network: could not release cleanup barrier: $fifo"
        done
        ( kill_flea ) || result=1
        for fd in "${release_fds[@]}"; do exec {fd}>&-; done
        return "$result"
    }
    trap 'network_cleanup || exit 1' EXIT

    network_wait_favourites() {
        settings_wait_value "$1"
        local expected
        expected=$(ipc uiSettings | jq -c '.places.favourites')
        ipc railEntries | jq -e --argjson expected "$expected" '[.[] | select(.kind == "favourite") | .original] == $expected' >/dev/null \
            || fail "network: saved favourites and their visible rail rows differ"
    }
    network_wait_closed() {
        local attempt
        for attempt in $(seq 1 100); do
            [[ "$(ipc dialogOpen)" == false ]] && return
            sleep 0.05
        done
        fail "network: successful persistence did not close the form: $(ipc networkStatus)"
    }
    network_wait_panes() {
        local filter="$1" attempt seen
        for attempt in $(seq 1 100); do
            seen=$(ipc dualState)
            jq -e "$filter" <<< "$seen" >/dev/null && return
            sleep 0.05
        done
        printf 'NETWORK_PANE_FAILURE cursor=%q key=%s network=%s geometry=%s\n' \
            "$(hyprctl cursorpos)" "$(ipc keyDeliveryState)" "$(ipc networkFocusState)" "$(ipc shareBrowserState)"
        shot network-pane-focus-failure
        fail "network: pane transition did not satisfy $filter: $seen"
    }
    network_click_pane() {
        local side="$1" x y width height wx wy
        read -r x y width height <<< "$(ipc shareBrowserState | jq -r --argjson side "$side" '.paneRects[$side]')"
        [[ "$width" -gt 0 && "$height" -gt 0 ]] || fail "network: pane $side has no clickable listing"
        read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
        printf 'NETWORK_PANE_CLICK side=%s target=%s,%s rectangle=%s,%s,%s,%s before=%s\n' \
            "$side" "$((wx + x + width / 2))" "$((wy + y + height / 2))" "$x" "$y" "$width" "$height" "$(ipc dualState)"
        omarchy-drive click "$((wx + x + width / 2))" "$((wy + y + height / 2))" >/dev/null
        network_wait_panes ".focused == $side"
    }
    network_wait_tabs() {
        local count="$1" visible="$2" attempt
        for attempt in $(seq 1 100); do
            [[ "$(ipc tabCount)" == "$count" && "$(ipc tabBarVisible)" == "$visible" ]] && return
            sleep 0.05
        done
        fail "network: expected $count tabs with strip visible=$visible, got $(ipc tabCount)/$(ipc tabBarVisible)"
    }
    network_share_geometry() {
        local label="$1" expected="${2:-}" attempt seen
        for attempt in $(seq 1 100); do
            seen=$(ipc shareBrowserState) || fail 'network: share geometry observer failed'
            if jq -e --arg expected "$expected" '.active and .owner == 1 and .baseUri == "smb://shares-second.test/"
                and .rect == .paneRects[1] and ($expected == "" or .rect == $expected)' <<< "$seen" >/dev/null; then
                network_geometry_rect=$(jq -r .rect <<< "$seen")
                network_geometry_checks=$((network_geometry_checks + 1))
                printf 'NETWORK_SHARE_GEOMETRY %s %s expected=%q observed=%s\n' "$network_geometry_checks" "$label" "$expected" "$seen"
                return
            fi
            sleep 0.05
        done
        fail "network: $label geometry expected=$expected observed=$seen panes=$(ipc dualState)"
    }
    network_share_reflow() {
        local hidden_rect tabs_rect x y width height chrome font_before font_after direction stops
        network_wait_tabs 1 false
        network_share_geometry 'second-pane shares start without tabs'
        hidden_rect="$network_geometry_rect"
        network_click_pane 0
        network_wait_tabs 1 false
        network_share_geometry 'pointer focus in the other pane retains owner geometry' "$hidden_rect"
        key t >/dev/null || fail 'network: native new-tab key failed'
        network_wait_tabs 2 true
        read -r x y width height <<< "$hidden_rect"
        chrome=$(ipc chromeHeight)
        tabs_rect="$x $((y + chrome)) $width $((height - chrome))"
        network_share_geometry 'new tab adds exactly the live tab-strip height' "$tabs_rect"
        [[ "$(ipc focusView)" == list ]] || fail 'network: non-owner pane lacks listing focus before Tab'
        key -k Tab >/dev/null || fail 'network: native pane focus key failed'
        network_wait_panes '.focused == 1'
        network_wait_tabs 1 false
        network_share_geometry 'keyboard focus to one-tab owner removes the tab strip' "$hidden_rect"
        network_click_pane 0
        network_wait_tabs 2 true
        network_share_geometry 'pointer focus to two-tab pane restores the tab strip' "$tabs_rect"
        shot network-shares-tabs-reflow
        key w >/dev/null || fail 'network: native close-tab key failed'
        network_wait_tabs 1 false
        network_share_geometry 'closing the other pane tab restores the original rectangle' "$hidden_rect"

        permissions_viewport 880 620
        network_share_geometry 'shares match the owner at 880x620'
        hidden_rect="$network_geometry_rect"
        permissions_viewport 1100 800
        network_share_geometry 'shares match the owner after native resize to 1100x800'
        [[ "$network_geometry_rect" != "$hidden_rect" ]] || fail 'network: window resize did not change the owner rectangle'
        shot network-shares-resized

        settings_wait_value '.display.textSize.mode == "system"'
        font_before=$(token_of baseSize)
        # Sample source: var STOPS = [9, 10, 11, 12, 14, 16, 20]
        stops=$(grep '^var STOPS = ' "$flea_ui/js/TextSize.js" | cut -d= -f2-) || fail 'network: text-size stops are unavailable'
        font_after=$(jq -r --argjson size "$font_before" 'map(select(. > $size)) | first' <<< "$stops") \
            || fail 'network: text-size stops could not be read'
        direction=equal
        if [[ -z "$font_after" || "$font_after" == null ]]; then
            font_after=$(jq -er --argjson size "$font_before" 'map(select(. < $size)) | last' <<< "$stops") \
                || fail 'network: no alternate text-size stop is available'
            direction=minus
        fi
        hidden_rect="$network_geometry_rect"
        key -M ctrl -M shift -k "$direction" -m shift -m ctrl >/dev/null || fail 'network: text-size chord failed'
        settings_wait_value ".display.textSize.mode == $font_after"
        [[ "$(token_of baseSize)" == "$font_after" ]] || fail 'network: persisted text size did not reach the live theme'
        network_share_geometry 'shares follow the native text-size change'
        [[ "$network_geometry_rect" != "$hidden_rect" ]] || fail 'network: text-size change did not change owner geometry'
        shot network-shares-font-reflow
        key -M ctrl -M shift -k 0 -m shift -m ctrl >/dev/null || fail 'network: text-size reset chord failed'
        settings_wait_value '.display.textSize.mode == "system"'
        [[ "$(token_of baseSize)" == "$font_before" ]] || fail 'network: Follow Omarchy did not restore the original font'
        network_share_geometry 'font reset restores the original owner rectangle' "$hidden_rect"
        printf 'NETWORK shares-tab-focus=exact shares-resize=exact shares-font-reflow=exact checks=%s\n' "$network_geometry_checks"
    }
    network_click_favourite() {
        local label="$1" index
        index=$(ipc railEntries | jq -r --arg label "$label" 'map(.label) | index($label)')
        [[ "$index" != null ]] || fail "network: missing fixture favourite $label"
        click_rail_row "$index" left
    }
    network_release() {
        timeout 2 sh -c 'printf "release\n" > "$1"' sh "$fake_root/$1-release" \
            || fail "network: $1 fixture was no longer waiting at its release barrier"
    }

    local live_mounts
    live_mounts=$(gio mount -l 2>/dev/null | grep -c '^Mount(') || true
    [[ "$live_mounts" -eq 0 ]] \
        || fail "network: $live_mounts real gio mount(s) already present, cannot assert an empty rail against ambient state: $(gio mount -l 2>/dev/null)"

    # This case proves form/bookmark behavior, not a network route; a bounded local gio double keeps
    # the newly functional Save action from dialing TEST-NET-2 or reopening on its later timeout.
    cat > "$fake_root/bin/gio" <<EOS
#!/bin/sh
case "\$*" in
"info --attributes=trash::item-count trash:///"|"monitor --dir=trash:///") exec /usr/bin/gio "\$@" ;;
esac
case "\$1 \${2:-}" in
"mount -li") exit 0 ;;
"mount nfs://cancel.test/export")
    : > "$fake_root/cancel-started"
    read release < "$fake_root/mount-release"
    ;;
"mount nfs://late-retry.test/export")
    : > "$fake_root/late-started"
    read release < "$fake_root/late-release"
    exit 2
    ;;
"mount nfs://origin.test/export")
    : > "$fake_root/origin-started"
    read release < "$fake_root/origin-release"
    ;;
"mount nfs://stale-one.test/export") printf 'Location is already mounted\n' >&2; exit 2 ;;
"mount nfs://stale-two.test/export") exit 2 ;;
"mount "*) printf '%s\n' "\$*" > "$mount_log"; printf '%s\n' "\$*" >> "$mount_calls" ;;
"info nfs://stale-two.test/export") exit 1 ;;
"info nfs://late-retry.test/export") exit 1 ;;
"info nfs://origin.test/export") printf 'local path: %s\n' "$races/mounted" ;;
"info smb://shares-origin.test/alpha")
    : > "$fake_root/child-started"
    read release < "$fake_root/child-release"
    printf 'local path: %s\n' "$races/share-mounted"
    ;;
"info smb://shares-one.test/"|"info smb://shares-two.test/") exit 1 ;;
"info smb://shares-origin.test/"|"info smb://shares-second.test/") exit 1 ;;
"info "*) printf 'local path: %s\n' "$dir" ;;
"list smb://shares-one.test/") printf 'old-share\n' ;;
"list smb://shares-two.test/") exit 0 ;;
"list smb://shares-origin.test/")
    : > "$fake_root/shares-started"
    read release < "$fake_root/shares-release"
    printf 'alpha\nbeta\n'
    ;;
"list smb://shares-second.test/") printf 'second-alpha\nsecond-beta\n' ;;
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

    # The current keymap binds "a" to add network from both listing and rail contexts.
    [[ "$(ipc focusView)" == "list" ]] || fail "network: did not start on the list"
    [[ "$(ipc cursor)" == "0" ]] || fail "network: did not start on row 0"
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "network: a from the list did not open the add-location dialog"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: list add-location dialog did not close"
    [[ "$(ipc cursor)" == "0" ]] || fail "network: list a changed the cursor to $(ipc cursor)"

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
    network_wait_closed
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "network: Enter did not submit and close the dialog"
    [[ "$(cat "$mount_log")" == 'mount --anonymous smb://198.51.100.1/' ]] \
        || fail "network: guest SMB did not use gio mount --anonymous"
    network_wait_favourites '.places.favourites == [{"label":"198.51.100.1","path":"smb://198.51.100.1/"}]'
    network_wait_closed
    [[ -z "$(ipc networkEntries)" ]] || fail "network: an unlisted mount was invented as a Network row"
    [[ ! -e "$bookmarks" ]] || fail "network: the new favourite created shared GTK bookmarks"
    shot network-appeared

    local invalid_port invalid_port_failures=0 snapshot
    for invalid_port in "22/path" "0" "65536"; do
        snapshot="$fixture_root/network-state-${invalid_port//\//-}"
        if ! assert_invalid_network_port "$invalid_port" "$stored" "$snapshot"; then
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
    # Repeat the reverse walk with a physical Shift+Tab chord, which can arrive as Tab plus Shift.
    key -k Tab -k Tab -k Tab -k Tab >/dev/null
    local expected_focus
    for expected_focus in "" Password Username Path; do
        key -M shift -k Tab -m shift >/dev/null
        settle
        [[ "$(ipc networkFocus)" == "$expected_focus" ]] \
            || fail "network: Shift+Tab expected '$expected_focus', got '$(ipc networkFocus)'"
    done
    key "YY" >/dev/null
    settle
    [[ "$(ipc networkUri)" == "sftp://uu@hh/ssXXYY" ]] \
        || fail "network: Shift+Tab did not return typing to Path, URI is $(ipc networkUri)"
    click_chip FTPS
    settle
    [[ "$(ipc networkFocus)" == Path ]] || fail "network: protocol change moved the visible Path focus"
    key -k Tab -k Tab -k Tab >/dev/null
    settle
    [[ "$(ipc networkFocus)" == TLS ]] || fail "network: forward traversal did not reach TLS"
    key -M shift -k Tab -m shift >/dev/null
    settle
    [[ "$(ipc networkFocus)" == Password ]] || fail "network: Shift+Tab from TLS did not reach Password"
    key -k Tab -k Backtab >/dev/null
    settle
    [[ "$(ipc networkFocus)" == Password ]] || fail "network: Backtab from TLS did not reach Password"
    printf 'NETWORK traversal=ok wrap=ok skip=ok chip-enter=ok shift-tab=ok tls-reverse=ok\n'
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

    # Favourites owns the new row; Remove edits that store without touching GTK or unmounting.
    local added_favourite_index
    added_favourite_index=$(ipc railEntries | jq -er 'map(.kind) | index("favourite")') \
        || fail "network: the added favorite is not present in the rail"
    click_rail_row "$added_favourite_index" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Remove" ]] \
        || fail "network: the added favourite offers $(ipc contextMenuEntries), not Remove"
    key -k Return >/dev/null
    network_wait_favourites '.places.favourites == []'
    [[ ! -e "$bookmarks" ]] || fail "network: Remove created shared GTK bookmarks"
    [[ "$(wc -l < "$mount_calls")" -eq 1 ]] || fail "network: Remove called the mount helper"
    printf 'NETWORK add-then-remove=ok gtk-absent=ok\n'

    local host
    for host in 198.51.100.2 198.51.100.3; do
        rail_focus
        key a >/dev/null
        settle
        [[ "$(ipc dialogOpen)" == true ]] || fail "network: the form did not reopen for $host"
        key "$host" >/dev/null
        key -k Return >/dev/null
        network_wait_closed
    done
    network_wait_favourites '.places.favourites == [{"label":"198.51.100.2","path":"smb://198.51.100.2/"},{"label":"198.51.100.3","path":"smb://198.51.100.3/"}]'

    # A real unreadable-but-writable store must refuse its asynchronous append without losing inputs.
    local before mount_count
    before=$(cat "$stored")
    mount_count=$(wc -l < "$mount_calls")
    chmod 200 "$stored"
    rail_focus
    key a >/dev/null
    settle
    key "198.51.100.4" >/dev/null
    key -k Return -k Return >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkStatus)" == *"could not be read"* ]] && break
        sleep 0.05
    done
    chmod 600 "$stored"
    [[ "$(ipc dialogOpen)" == true && "$(ipc networkStatus)" == *"could not be read"* ]] \
        || fail "network: persistence refusal did not keep its error and form visible: $(ipc networkStatus)"
    [[ "$(ipc networkUri)" == 'smb://198.51.100.4/' && "$(ipc networkAction)" == Retry ]] \
        || fail "network: save refusal lost the entered endpoint or retry action"
    [[ "$(cat "$stored")" == "$before" ]] || fail "network: unreadable state was overwritten"
    [[ "$(wc -l < "$mount_calls")" -eq $((mount_count + 1)) ]] \
        || fail "network: repeated submit mounted more than once"
    shot network-save-refused
    key -k Return >/dev/null
    network_wait_closed
    network_wait_favourites '.places.favourites | length == 3 and .[2] == {"label":"198.51.100.4","path":"smb://198.51.100.4/"}'
    [[ "$(wc -l < "$mount_calls")" -eq $((mount_count + 1)) ]] \
        || fail "network: persistence retry remounted an already connected endpoint"
    [[ ! -e "$bookmarks" ]] || fail "network: persistence retry created GTK bookmarks"
    printf 'NETWORK save-refusal=retained repeat-submit=single retry=save-only\n'

    # A mount blocked at a fixture-owned FIFO provides an observable, cancellable in-flight request.
    before=$(cat "$stored")
    rail_focus
    key a >/dev/null
    settle
    click_chip NFS
    key "cancel.test" >/dev/null
    key -k Tab -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    for _attempt in $(seq 1 100); do
        [[ -e "$fake_root/cancel-started" ]] && break
        sleep 0.05
    done
    [[ -e "$fake_root/cancel-started" && "$(ipc dialogOpen)" == true ]] \
        || fail "network: cancellable mount did not remain in the open form"
    [[ "$(cat "$stored")" == "$before" ]] || fail "network: favourite was saved before mount completion"
    local pending_state pending_key
    pending_state=$(ipc dualState)
    ipc networkFocusState | jq -e '.inside and .busy' >/dev/null \
        || fail "network: pending mount did not own keyboard focus"
    for pending_key in Tab Backtab ShiftTab; do
        if [[ "$pending_key" == ShiftTab ]]; then
            key -M shift -k Tab -m shift >/dev/null
        elif [[ "$pending_key" == Backtab ]]; then
            key -k ISO_Left_Tab >/dev/null
        else
            key -k "$pending_key" >/dev/null
        fi
        settle
        ipc networkFocusState | jq -e '.inside and .busy' >/dev/null \
            || fail "network: pending $pending_key escaped the dialog: $(ipc networkFocusState)"
        [[ "$(ipc dualState)" == "$pending_state" ]] \
            || fail "network: pending $pending_key changed the listing, selection or pane focus"
    done
    shot network-mount-pending
    key -k Escape >/dev/null
    wait_network_result cancelled 5
    [[ "$(ipc dialogOpen)" == false && "$(cat "$stored")" == "$before" ]] \
        || fail "network: Cancel reopened the form or saved an unfinished mount"
    printf 'NETWORK mount-first=ok pending-Tab=contained pending-Backtab=contained pending-ShiftTab=contained cancel=inflight no-place=ok\n'

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

    # Legacy GTK rows stay readable and byte-for-byte intact alongside persisted Flea favourites.
    mkdir -p "$fixture_home/.config/gtk-3.0"
    printf 'smb://legacy.test/data Legacy share\nfile:///missing/legacy Local legacy\n' > "$bookmarks"
    local legacy_before
    legacy_before=$(cat "$bookmarks")
    kill_flea
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_listing 3
    network_wait_favourites '.places.favourites | length == 3'
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" == 'Legacy share|network|share|false' ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == 'Legacy share|network|share|false' ]] \
        || fail "network: restarting did not preserve the readable legacy Network row"
    assert_network_mark_alignment "the live text size" true
    local legacy_index
    legacy_index=$(ipc railEntries | jq -r 'map(.label) | index("Legacy share")')
    click_rail_row "$legacy_index" right
    settle
    # The rail menu is offered, the same as 0.1.6: opening it writes nothing, only activating a row does.
    [[ "$(ipc contextMenuVisible)" == true ]] || fail "network: a legacy GTK row offered no rail menu"
    printf 'NETWORK legacy-rail-menu=%s\n' "$(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle
    [[ "$(cat "$bookmarks")" == "$legacy_before" ]] || fail "network: legacy GTK bytes changed"
    printf 'NETWORK restart=favourites-and-legacy gtk=unchanged\n'

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
    network_wait_closed
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
    network_wait_closed
    key a >/dev/null
    click_chip NFS
    key "stale-two.test" >/dev/null
    key -k Tab >/dev/null
    key -k Tab >/dev/null
    key "/export" >/dev/null
    key -k Return >/dev/null
    wait_network_result failed 5
    network_wait_favourites '.places.favourites | all(.[]; .path != "nfs://stale-two.test/export")'
    [[ "$(ipc dialogOpen)" == true ]] || fail "network: failed mount discarded its draft"
    [[ "$(ipc networkStatus)" == "Connect failed: network location was refused" ]] \
        || fail "network: the refused direct open reported the previous open's verdict"
    key -k Escape >/dev/null
    settle

    [[ "$(ipc focusView)" == "rail" ]] || fail "network: share-cache setup did not return to rail"
    key a >/dev/null
    key "shares-one.test" >/dev/null
    key -k Return >/dev/null
    wait_network_result mounted 5
    network_wait_closed
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

    network_wait_favourites '.places.favourites | all(.[]; .path != "smb://shares-two.test/")'
    [[ "$(cat "$bookmarks")" == "$legacy_before" ]] || fail "network: later Network actions changed GTK bookmarks"
    key -k Escape >/dev/null
    settle
    printf 'NETWORK empty=ok a-scoped=ok dialog=ok submit-path=ok keyboard-after=ok guest-smb=anonymous nfs=plain caches=isolated\n'

    # Separate persisted fixtures keep the mount-origin races independent of the save inventory above.
    kill_flea
    seed_ui_state "$race_state" "$(jq -n --arg left "$races/left" --arg right "$races/right" \
        '{view:"list",keys:"default",dual:{paths:[$left,$right],focus:0},places:{favourites:[
            {label:"Late retry",path:"nfs://late-retry.test/export"},
            {label:"Origin mount",path:"nfs://origin.test/export"},
            {label:"Origin shares",path:"smb://shares-origin.test/"},
            {label:"Second shares",path:"smb://shares-second.test/"}]}}')"
    HOME="$fixture_home" launch "$races/left"
    wait_listing 3
    network_wait_favourites '.places.favourites | length == 4'

    network_click_favourite "Late retry"
    wait_marker "$fake_root/late-started" "network: late retry did not reach its mount barrier"
    [[ "$(ipc keymapPreset)" == default && "$(ipc networkResult)" == mounting ]] \
        || fail "network: direct mount context differs: preset=$(ipc keymapPreset) focus=$(ipc focusView) result=$(ipc networkResult)"
    key -M ctrl -k k -m ctrl >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == false ]] || fail "network: Mac-only Ctrl+K opened a draft in the Default preset"
    if [[ "$(ipc focusView)" == list ]]; then key -k Tab >/dev/null; fi
    [[ "$(ipc focusView)" == rail ]] || fail "network: native Tab did not focus the rail during mounting"
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == true ]] \
        || fail "network: rail a did not open a newer draft: preset=$(ipc keymapPreset) focus=$(ipc focusView) dialog=$(ipc dialogOpen) mount=$(ipc networkResult) dialogFocus=$(ipc networkFocusState)"
    printf 'NETWORK direct-mount preset=%s focus=%s dialog=%s mount=%s default-CtrlK=refused rail-a=opened\n' \
        "$(ipc keymapPreset)" "$(ipc networkFocusState)" "$(ipc dialogOpen)" "$(ipc networkResult)"
    click_chip NFS
    key "newer-draft.test" >/dev/null
    key -k Tab -k Tab >/dev/null
    key "/kept" >/dev/null
    local draft_uri draft_title draft_action draft_focus
    draft_uri=$(ipc networkUri)
    draft_title=$(ipc networkTitle)
    draft_action=$(ipc networkAction)
    draft_focus=$(ipc networkFocusState)
    [[ "$draft_uri" == 'nfs://newer-draft.test/kept' ]] || fail "network: newer draft did not receive native typing"
    network_release late
    wait_network_result failed 5
    [[ "$(ipc dialogOpen)" == true && "$(ipc networkUri)" == "$draft_uri" \
        && "$(ipc networkTitle)" == "$draft_title" && "$(ipc networkAction)" == "$draft_action" \
        && "$(ipc networkFocusState)" == "$draft_focus" && -z "$(ipc networkStatus)" ]] \
        || fail "network: late failure replaced or refocused the newer draft"
    [[ "$(ipc statusError)" == true && "$(ipc lastMessage)" == 'Connect failed: network location was refused' ]] \
        || fail "network: preserving the newer draft hid the older mount failure: statusError=$(ipc statusError) message=$(ipc lastMessage) footer=$(ipc statusFooterState) result=$(ipc networkResult)"
    network_wait_favourites '.places.favourites | length == 4'
    shot network-late-retry-newer-draft
    key -k Escape >/dev/null
    settle
    printf 'NETWORK late-retry=newer-draft-retained focus=retained failure=visible\n'

    click_chrome dual
    network_wait_panes '.active and .focused == 0 and .panes[1].total == 4 and (.panes[1].loading | not)'
    network_click_favourite "Origin mount"
    wait_marker "$fake_root/origin-started" "network: origin mount did not reach its barrier"
    network_click_pane 1
    key j >/dev/null
    settle
    local other_before
    other_before=$(ipc dualState | jq -c '.panes[1]')
    network_release origin
    wait_network_result mounted 5
    network_wait_panes ".focused == 1 and .panes[0].path == \"$races/mounted\" and .panes[0].total == 2 and (.panes[0].loading | not)"
    [[ "$(ipc dualState | jq -c '.panes[1]')" == "$other_before" ]] \
        || fail "network: completing the first pane's mount changed the second pane"
    shot network-dual-origin-mount

    network_click_pane 0
    network_click_favourite "Origin shares"
    wait_marker "$fake_root/shares-started" "network: share listing did not reach its barrier"
    network_click_pane 1
    other_before=$(ipc dualState | jq -c '.panes[1]')
    network_release shares
    wait_network_result mounted 5
    ipc shareBrowserState | jq -e '.active and .owner == 0 and .rect == .paneRects[0] and .baseUri == "smb://shares-origin.test/"' >/dev/null \
        || fail "network: completed shares did not cover only their originating pane: $(ipc shareBrowserState)"
    [[ "$(ipc dualState | jq -c '.panes[1]')" == "$other_before" ]] \
        || fail "network: completed shares changed the other pane"
    local other_cursor
    other_cursor=$(ipc cursor)
    key j >/dev/null
    settle
    [[ "$(ipc cursor)" -eq $((other_cursor + 1)) && "$(ipc shareBrowserCursor)" == 0 ]] \
        || fail "network: keys in the other pane moved the share cursor"
    goto_row 0
    key l >/dev/null
    network_wait_panes ".focused == 1 and .panes[1].path == \"$races/right/child\" and .panes[1].total == 2 and (.panes[1].loading | not)"
    [[ "$(ipc shareBrowserOpen)" == true ]] || fail "network: other-pane navigation dismissed the originating pane's shares"
    shot network-dual-origin-shares

    local sx sy sw sh wx wy row_height
    read -r sx sy sw sh <<< "$(ipc shareBrowserRect)"
    read -r _body _caption _padding row_height <<< "$(ipc metrics)"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((wx + sx + sw / 2))" "$((wy + sy + row_height / 2))" >/dev/null
    wait_marker "$fake_root/child-started" "network: share row pointer activation did not mount its child"
    network_wait_panes '.focused == 0'
    network_click_pane 1
    other_before=$(ipc dualState | jq -c '.panes[1]')
    network_release child
    wait_network_result mounted 5
    network_wait_panes ".focused == 1 and .panes[0].path == \"$races/share-mounted\" and .panes[0].total == 2 and (.panes[0].loading | not)"
    [[ "$(ipc shareBrowserOpen)" == false && "$(ipc dualState | jq -c '.panes[1]')" == "$other_before" ]] \
        || fail "network: child mount lost its origin, stole focus or left its shares visible"
    shot network-dual-origin-child
    printf 'NETWORK dual-mount=origin-preserved shares=origin-preserved other-pane-keys=independent other-pane-navigation=independent child-mount=origin-preserved\n'

    printf 'NETWORK_SECOND_SHARES before=%s result=%s panes=%s\n' "$(ipc shareBrowserState)" "$(ipc networkResult)" "$(ipc dualState)"
    network_click_favourite "Second shares"
    wait_network_result mounted 5
    printf 'NETWORK_SECOND_SHARES after-result=%s result=%s panes=%s\n' "$(ipc shareBrowserState)" "$(ipc networkResult)" "$(ipc dualState)"
    local second_shares attempt
    for attempt in $(seq 1 100); do
        second_shares=$(ipc shareBrowserState)
        jq -e '.active and .baseUri == "smb://shares-second.test/"' <<< "$second_shares" >/dev/null && break
        sleep 0.05
    done
    jq -e '.active and .baseUri == "smb://shares-second.test/" and .owner == 1 and .rect == .paneRects[1]' <<< "$second_shares" >/dev/null \
        || fail "network: second-pane shares did not retain their owner: $second_shares; panes=$(ipc dualState); result=$(ipc networkResult)"
    shot network-second-pane-shares
    local network_geometry_rect network_geometry_checks=0
    network_share_reflow
    local exit_log_start retained_secondary retained_shares retained_entries retained_rect rx ry rw rh menus_checks=0
    retained_secondary=$(ipc dualState | jq -c '.panes[1] | del(.focused)')
    retained_shares=$(ipc shareBrowserState | jq -c '{active,owner,baseUri,cursor}')
    retained_entries=$(ipc shareBrowserEntries)
    retained_rect="$network_geometry_rect"
    exit_log_start=$(wc -l < "$flea_log")
    click_chrome list
    network_wait_panes "(.active | not) and .focused == 0 and (.panes[1].focused | not) and ((.panes[1] | del(.focused)) == $retained_secondary)"
    menus_expect shareBrowserState "{active,owner,baseUri,cursor} == $retained_shares" 'leaving dual retains the secondary share listing session'
    key -k Home >/dev/null || fail 'network: primary Home delivery failed after dual exit'
    network_wait_panes '.focused == 0 and .panes[0].cursor == 0'
    key j >/dev/null || fail 'network: primary j delivery failed after dual exit'
    network_wait_panes ".focused == 0 and .panes[0].cursor == 1 and ((.panes[1] | del(.focused)) == $retained_secondary)"
    menus_expect shareBrowserState "{active,owner,baseUri,cursor} == $retained_shares" 'hidden secondary shares cannot consume primary-list keys'
    read -r rx ry rw rh <<< "$(ipc rowRect 0)"
    [[ "$rw" -gt 0 && "$rh" -gt 0 ]] || fail 'network: primary row has no pointer target after dual exit'
    menus_point "$((rx + rw * 3 / 4)) $((ry + rh / 2))"
    network_wait_panes ".focused == 0 and .panes[0].cursor == 0 and .panes[0].selected == [0] and ((.panes[1] | del(.focused)) == $retained_secondary)"
    menus_expect shareBrowserState "{active,owner,baseUri,cursor} == $retained_shares" 'pointer selects the expanded primary listing without activating hidden shares'
    key -k Escape >/dev/null || fail 'network: primary Escape delivery failed after dual exit'
    network_wait_panes '.focused == 0 and .panes[0].selected == []'
    menus_expect shareBrowserState "{active,owner,baseUri,cursor} == $retained_shares" 'primary Escape clears selection without dismissing the retained secondary session'
    shot network-shares-owner-hidden

    click_chrome dual
    network_wait_panes ".active and .focused == 0 and ((.panes[1] | del(.focused)) == $retained_secondary)"
    network_share_geometry 'reopening dual restores retained shares geometry' "$retained_rect"
    menus_equal 'reopening dual preserves the original share names' "$retained_entries" "$(ipc shareBrowserEntries)"
    key -k Tab >/dev/null || fail 'network: reopened secondary focus delivery failed'
    network_wait_panes '.focused == 1'
    key j >/dev/null || fail 'network: reopened share cursor delivery failed'
    menus_expect shareBrowserState '.active and .owner == 1 and .cursor == 1' 'reopened secondary shares receive their own cursor key'
    network_wait_panes ".focused == 1 and .panes[0].cursor == 0 and ((.panes[1] | del(.focused)) == $retained_secondary)"
    shot network-shares-owner-reopened
    key -k Escape >/dev/null || fail 'network: reopened share Escape delivery failed'
    menus_expect shareBrowserState '(.active | not) and .owner == 1' 'Escape in the owning pane dismisses shares without destroying its pane'
    key j >/dev/null || fail 'network: secondary listing j failed after dismissing shares'
    network_wait_panes '.focused == 1 and .panes[1].focused and .panes[1].cursor == 1 and .panes[0].cursor == 0'
    shot network-shares-owner-dismissed
    click_chrome list
    network_wait_panes '(.active | not) and .focused == 0 and (.panes[1].focused | not)'
    tail -n "+$((exit_log_start + 1))" "$flea_log" > "$fake_root/second-pane-exit.log"
    if grep -E 'WARN|ERROR|TypeError|ReferenceError|Cannot' "$fake_root/second-pane-exit.log"; then
        fail "network: second-pane exit or reopen produced native QML errors"
    fi
    [[ "$(cat "$bookmarks")" == "$legacy_before" ]] || fail "network: origin races changed GTK bookmarks"
    printf 'NETWORK second-pane-session=retained hidden-keys-pointer=primary reopen=owner-and-geometry Escape=focused-owner focus=restored qml-log=clean checks=%s\n' "$menus_checks"
    export PATH="$saved_path"
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    if [[ -n "$real_config" ]]; then export XDG_CONFIG_HOME="$real_config"; else unset XDG_CONFIG_HOME; fi
    network_cleanup || fail "network: owned process did not drain after releasing its barriers"
    trap - EXIT
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
            # -D skip and -r, not -R: case_network leaves FIFOs in its fixture, and a recursive read
            # of one blocks in the kernel forever waiting for a writer that never comes.
            ! printf '%s\n' "$runtime_canary" | grep -r -D skip -a -F -q -f - "$surface" 2>/dev/null \
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
"mount -li")
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
        seen_entries=$(timeout 1 omarchy-drive ipc -p "$flea_ui/boot" flea networkEntries 2>/dev/null || true)
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
    [[ "$(ipc contextMenuEntries)" == "Unmount|Edit|Rename|Remove" ]] \
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
    # Dialogs rule 6: the title says the task, so it is the same sentence under every chip below.
    [[ "$(ipc networkTitle)" == "Add a network share" ]] \
        || fail "networkauth: the add dialog does not name its own task, got $(ipc networkTitle)"
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

    local title_case want_protocol want_fields
    for title_case in \
        "SFTP|Label|Host|Port|Path|Username|Password" \
        "FTPS|Label|Host|Port|Path|Username|Password|TLS" \
        "WebDAV|Label|Host|Port|Path|Username|Password|TLS"; do
        IFS='|' read -r want_protocol want_fields <<< "$title_case"
        click_chip "$want_protocol"
        [[ "$(ipc networkTitle)" == "Add a network share" ]] \
            || fail "networkauth: $want_protocol moved the title to $(ipc networkTitle)"
        [[ "$(ipc networkFields)" == "$want_fields" ]] \
            || fail "networkauth: $want_protocol fields are $(ipc networkFields)"
    done

    click_chip NFS
    settle
    [[ "$(ipc networkTitle)" == "Add a network share" ]] || fail "networkauth: NFS moved the title"
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
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
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
    # A save lands in Flea Favorites, never the shared GTK file, and it carries no secret.
    local saved_favourite
    saved_favourite=$(ipc uiSettings | jq -c '.places.favourites[-1]')
    [[ "$saved_favourite" == '{"label":"data","path":"smb://tester@slot.test/data"}' ]] \
        || fail "networkauth: the saved favourite is $saved_favourite"
    [[ -z "$(cat "$bookmarks")" ]] \
        || fail "networkauth: a save wrote the shared GTK file, it now reads: $(cat "$bookmarks")"
    ipc uiSettings | grep -F -q -- "$runtime_canary" \
        && fail "networkauth: the saved favourite carries the password"

    # An already-mounted authenticated row resolves directly and never needs the process password.
    local helper_calls
    helper_calls=$(wc -l < "$helper_log")
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    wait_network_entry_state true
    key -k Tab >/dev/null
    key g >/dev/null
    settle
    rail_seek auth-test
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
    wait_rail_label data
    key -k Tab >/dev/null
    key g >/dev/null
    settle
    rail_seek data
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
    wait_network_status "Connect failed: authentication helper is unavailable" 10
    [[ "$(ipc dialogOpen)" == "true" \
        && "$(ipc networkPasswordState)" == "masked|set" ]] \
        || fail "networkauth: missing helper did not fail closed and retain fields"
    [[ "$(wc -l < "$helper_log")" -eq "$helper_calls" ]] \
        || fail "networkauth: missing helper wrote helper output"

    cp "$dir/bin/flea-gio-auth.real" "$dir/bin/flea-gio-auth"
    chmod 0644 "$dir/bin/flea-gio-auth"
    key -k Escape >/dev/null
    key -k Return >/dev/null
    wait_network_status "Connect failed: authentication helper is unavailable" 10
    [[ "$(ipc dialogOpen)" == "true" \
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
    wait_rail_label data
    key -k Tab >/dev/null
    key g >/dev/null
    settle
    rail_seek data
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

    # #181: the share opened once its credential landed, and focus followed that open into it.
    [[ "$(ipc focusView)" == "list" ]] \
        || fail "networkauth: the corrected retry's open did not take focus into the share, focus is $(ipc focusView)"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] \
        || fail "networkauth: corrected-retry setup did not return to rail"

    # The approved failed-connect artifact is FTPS: keep every field, mask the password, say one
    # sentence and replace Save with Retry.
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
    # Rule 6 again: a reopen after a failed connect is a connect, not an add, and it says which failed.
    [[ "$(ipc dialogOpen)" == "true" && "$(ipc networkTitle)" == "Connect to a network share, failed connect" ]] \
        || fail "networkauth: failure did not reopen approved FTPS artifact, title $(ipc networkTitle)"
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
        || fail "networkauth: corrected Retry left stale failure UI (dialog $(ipc dialogOpen), shares $(ipc shareBrowserOpen), status $(ipc networkStatus), result $(ipc networkResult))"
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
if [ "\$1 \$2" != "mount -li" ]; then
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
    local product_root=${FLEA_NETWORK_LIVE_ROOT:-}
    local relative=${FLEA_NETWORK_LIVE_RELATIVE:-}
    local mount_relative=${FLEA_NETWORK_LIVE_MOUNT_RELATIVE:-}
    local protocol=${FLEA_NETWORK_LIVE_PROTOCOL:-}
    local host=${FLEA_NETWORK_LIVE_HOST:-}
    local remote_path=${FLEA_NETWORK_LIVE_PATH:-}
    local remote_user=${FLEA_NETWORK_LIVE_USER:-}
    local auth=${FLEA_NETWORK_LIVE_AUTH:-none}
    local mount_root
    [[ "$uri" == *://* && "$mount_uri" == *://* \
        && "$product_root" == "/run/user/$(id -u)/gvfs/"* ]] \
        || fail "networklive: missing or unsafe live mount contract"
    [[ -n "$relative" && "$relative" != /* && "$relative" != *".."* ]] \
        || fail "networklive: unsafe relative test path"
    [[ -n "$mount_relative" && "$mount_relative" != /* && "$mount_relative" != *".."* ]] \
        || fail "networklive: unsafe mount-relative test path"
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
    [[ "$product_root" == "/run/user/$(id -u)/gvfs/"* && -d "$product_root" ]] \
        || fail "networklive: product mount has no safe FUSE root"
    [[ -f "${product_root%/}/$relative/alpha.txt" ]] \
        || fail "networklive: relative fixture is absent below product mount root $product_root"
    wait_path_wall "$product_root" 25

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
    mount_root=$(timeout 5 gio info "$mount_uri" 2>/dev/null | sed -n 's/^local path: //p')
    [[ "$mount_root" == "/run/user/$(id -u)/gvfs/"* && -d "$mount_root" ]] \
        || fail "networklive: mount URI has no safe FUSE root"
    printf 'NETWORKLIVE roots product=%q mount=%q product-relative=%q mount-relative=%q\n' \
        "$product_root" "$mount_root" "$relative" "$mount_relative"
    wait_path_wall "$mount_root" 25
    key -k Escape >/dev/null
    settle
    [[ "$(ipc focusView)" == "list" ]] || fail "networklive: Escape did not return focus to the list"

    local current=$mount_root component row
    IFS=/ read -ra components <<< "$mount_relative"
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
    [[ "$(ipc contextMenuEntries)" == "Unmount|Edit|Rename|Remove" ]] \
        || fail "networklive: mounted share menu is $(ipc contextMenuEntries), not Unmount first"
    key -k Return >/dev/null
    wait_message "Unmounted $label."
    wait_network_result unmounted 25
    wait_network_entry_absent "$entries" "$mount_uri" 25
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
    # The rail count is satisfied by Home and Trash, and ui/NetworkMounts.qml polls mounts every five
    # seconds, so the fixture's own row is waited for. The operator's own shares sit on this rail too
    # and are none of this case's business, which is why the row is looked for rather than counted.
    for _attempt in $(seq 1 300); do
        [[ "$(ipc networkEntries)" == *"share.zip|network|share|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" == *"share.zip|network|share|true"* ]] \
        || fail "gvfs: live mount never appeared, the rail carries $(ipc networkEntries)"

    key -k Tab >/dev/null
    key g >/dev/null
    settle
    rail_seek share.zip
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

    click_rail_row "$(rail_row_of share.zip)" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Unmount|Edit|Rename|Remove" ]] \
        || fail "gvfs: mounted share menu is $(ipc contextMenuEntries)"
    key -k Return >/dev/null
    wait_message "Unmounted share.zip."
    for _attempt in $(seq 1 100); do
        [[ "$(ipc networkEntries)" != *"share.zip"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc networkEntries)" != *"share.zip"* ]] \
        || fail "gvfs: the row survived unmount, the rail carries $(ipc networkEntries)"
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

    # The overlay's box in the lazy views first: placed on a lazy view's item it sat at the Loader's origin, and that is where the old coordinates put it.
    local mode
    for mode in grid columns; do
        switch_view "$mode"
        key -k Tab >/dev/null
        settle
        key g >/dev/null
        settle
        rail_seek StubNAS
        key l >/dev/null
        for _attempt in $(seq 1 100); do
            [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
            sleep 0.05
        done
        [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: $mode: l on the bare root never opened the overlay"
        rect_is "$(ipc shareBrowserRect)" $(ipc listAreaRect) 1 \
            || fail "sharebrowser: $mode: the overlay's box is $(ipc shareBrowserRect), not the listing slot $(ipc listAreaRect)"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc shareBrowserOpen)" == "false" ]] || fail "sharebrowser: $mode: Escape did not close the overlay"
        key -k Escape >/dev/null
        settle
    done
    switch_view list

    # Tab to the rail and l the bare-root entry: it lists shares, it does not open anything.
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "sharebrowser: Tab did not reach the rail"
    key g >/dev/null
    settle
    rail_seek StubNAS
    key l >/dev/null
    for _attempt in $(seq 1 100); do
        [[ "$(ipc shareBrowserOpen)" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc shareBrowserOpen)" == "true" ]] || fail "sharebrowser: l on the bare root never opened the overlay"
    rect_is "$(ipc shareBrowserRect)" $(ipc listAreaRect) 1 \
        || fail "sharebrowser: list: the overlay's box is $(ipc shareBrowserRect), not the listing slot $(ipc listAreaRect)"
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
    click_rail_row "$(rail_row_of StubNAS)" left
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
    click_rail_row "$(rail_row_of StubNAS)" left
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
    click_rail_row "$(rail_row_of StubNAS)" left
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
if [ "\$1 \$2" = "mount -li" ]; then
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
    settle
    rail_seek hang
    settle
    key l >/dev/null
    wait_marker "$dir/bin/info-started" "hangshare: the hang share's gio info never started, the stub saw: $(grep -v '^mount -l$' "$dir/bin/calls" 2>/dev/null | sort -u | tr '\n' ';')"

    # An open that has not landed keeps focus on the rail, so the next key still moves the rail's cursor (#181).
    [[ "$(ipc focusView)" == "rail" ]] || fail "hangshare: focus left the rail for $(ipc focusView) while the share had not opened"

    # The guard is closed now, proven by the marker above rather than by a sleep, and a second share
    # must say so rather than swallow the keypress.
    key j >/dev/null
    settle
    [[ "$(ipc railLabel "$(ipc railCursor)")" == "good" ]] \
        || fail "hangshare: expected the rail cursor on good, got $(ipc railLabel "$(ipc railCursor)")"
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
    settle
    rail_seek StubRoot
    settle
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
  "mount -li")
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
    # GM's ruling of 2026-09-08: the machine's rail row is the bare hostname, no device suffix, so it fits at the body size.
    [[ "|$(ipc railLabels)|" == *"|$(cat /etc/hostname | tr -d '[:space:]')|"* && "$(ipc railLabels)" != *" · "* ]] \
        || fail "unmount: the rail's machine row is not the bare hostname: $(ipc railLabels)"
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

    # Right click raises the menu over the row and nothing else: the release row first, then the three
    # rows the saved place itself owns, and no unmount has run. The old two-right-click arm is gone,
    # see ui/Sidebar.qml "openRailMenu" and ui/js/Mounts.js "rowMenu".
    click_rail_row "$(rail_row_of stubshare)" right
    settle
    printf 'UNMOUNT menu visible=%s entries=%s glyphs=%s\n' \
        "$(ipc contextMenuVisible)" "$(ipc contextMenuEntries)" "$(ipc contextMenuGlyphs)"
    shot unmount-menu
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "unmount: right click opened no menu on the share"
    [[ "$(ipc contextMenuEntries)" == "Unmount|Edit|Rename|Remove" ]] \
        || fail "unmount: the share's menu is $(ipc contextMenuEntries), not Unmount, Edit, Rename then Remove"
    [[ "$(ipc contextMenuGlyphs)" == "eject|sliders|rename|minus" ]] \
        || fail "unmount: the share's rows draw $(ipc contextMenuGlyphs), not eject, sliders, rename and minus"
    [[ -z "$(cat "$unmount_log")" ]] || fail "unmount: opening the menu already unmounted: $(cat "$unmount_log")"

    # Escape closes it and still nothing has run, which is what makes the menu the confirmation.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "unmount: Escape did not close the rail menu"
    [[ -z "$(cat "$unmount_log")" ]] || fail "unmount: Escape unmounted anyway: $(cat "$unmount_log")"

    # Choosing the row is what unmounts, and the row's key is what says which share, not its index.
    click_rail_row "$(rail_row_of stubshare)" right
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
    [[ "$(ipc contextMenuVisible)" == "false" ]] \
        || fail "unmount: a favourite opened $(ipc contextMenuEntries)"

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
    click_rail_row "$(rail_row_of stubshare)" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Unmount|Edit|Rename|Remove" ]] \
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
    click_rail_row "$(rail_row_of 'Saved Share')" right
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
    click_rail_row "$(rail_row_of 'Ghost Place')" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Edit|Rename|Remove" ]] \
        || fail "unmount: an unmounted place offers $(ipc contextMenuEntries), not Edit, Rename then Remove"
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
    click_rail_row "$(rail_row_of stubshare)" right
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

# PR 122's phone rows, stubbed, because no MTP device is plugged into this box and gvfs is what the
# parser reads. The stub answers what "gio mount -li" answers here, can_mount=0 on a volume gio has
# already mounted included, which is the line the row has to survive to keep its Unmount.
case_phones() {
    local dir="$fixture_root/phones" state="$fixture_root/phones-state"
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    mkdir -p "$dir/bin" "$dir/files" "$state/flea"
    # The folder gio hands back after the mount, named the way gvfs names it, so the pane's own path
    # is what ui/js/Mounts.js "trashable" reads for issue 133.
    local fuse="$dir/gvfs/mtp:host=SAMSUNG_Android"
    mkdir -p "$fuse/DCIM"
    : > "$dir/files/local.txt"
    : > "$fuse/DCIM/IMG_0001.jpg"
    local mtp_uri="mtp://SAMSUNG_SAMSUNG_Android_RQGL705T0NR/"
    # The MTP mount stub holds this long, so the keyboard leg can read focus while the mount is still running.
    local mtp_mount_hold_s=2
    # Directive 44's capture: an iPhone answers on GPhoto2 and on AFC at once, and the rail folds the
    # pair into one row on the serial they share. The uuid here is GM's own phone's, as measured.
    local afc_uuid="00008130-001641411883401C"
    local afc_uri="afc://$afc_uuid/"
    local afc_fuse="$dir/gvfs-afc/afc:host=$afc_uuid"
    mkdir -p "$afc_fuse/DCIM/113APPLE"
    : > "$afc_fuse/DCIM/113APPLE/IMG_3263.HEIC"
    local unmount_log="$dir/unmount.log"
    : > "$unmount_log"

    cat > "$dir/bin/gio" <<EOS
#!/bin/sh
mounted="$dir/mounted"
afcmounted="$dir/afcmounted"
case "\$1 \${2:-}" in
"mount -li")
    printf 'Volume(0): SAMSUNG Android\n'
    printf '  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n'
    printf '  activation_root=$mtp_uri\n'
    if [ -f "\$mounted" ]; then
        printf '  can_mount=0\n'
        printf '  Mount(0): SAMSUNG Android -> $mtp_uri\n'
        printf '    Type: GProxyShadowMount (GProxyVolumeMonitorMTP)\n'
        printf 'Mount(0): mtp -> $mtp_uri\n'
        printf '  Type: GDaemonMount\n'
        printf '  is_shadowed=1\n'
    else
        printf '  can_mount=1\n'
    fi
    printf 'Volume(1): NIKON DSC D3500\n'
    printf '  Type: GProxyVolume (GProxyVolumeMonitorGPhoto2)\n'
    printf '  activation_root=gphoto2://%%5Busb%%3A001%%2C004%%5D/\n'
    printf '  can_mount=1\n'
    printf 'Volume(2): iPhone\n'
    printf '  Type: GProxyVolume (GProxyVolumeMonitorGPhoto2)\n'
    printf '  activation_root=gphoto2://Apple_Inc._iPhone_00008130001641411883401C/\n'
    printf '  can_mount=1\n'
    printf 'Volume(3): Documents on GM’s iPhone\n'
    printf '  Type: GProxyVolume (GProxyVolumeMonitorAfc)\n'
    printf '  uuid=$afc_uuid\n'
    printf '  activation_root=afc://$afc_uuid:3/\n'
    if [ -f "\$afcmounted" ]; then
        printf '  can_mount=0\n'
        printf 'Mount(2): GM’s iPhone -> $afc_uri\n'
        printf '  Type: GDaemonMount\n'
    else
        printf '  can_mount=1\n'
    fi
    exit 0 ;;
"mount -u")
    printf 'UNMOUNT %s\n' "\$3" >> "$unmount_log"
    rm -f "\$mounted" "\$afcmounted"
    exit 0 ;;
"mount $mtp_uri")
    # A real MTP mount takes a moment; the started marker lets the keyboard leg below read focus inside it.
    : > "$dir/mount-started"
    sleep $mtp_mount_hold_s
    : > "\$mounted"
    exit 0 ;;
"info $mtp_uri")
    printf 'local path: %s\n' "$fuse"
    exit 0 ;;
"mount $afc_uri")
    : > "\$afcmounted"
    exit 0 ;;
"info $afc_uri")
    printf 'local path: %s\n' "$afc_fuse"
    exit 0 ;;
esac
# Everything else is the real tool's, so the Trash count and its monitor keep working under the stub.
exec /usr/bin/gio "\$@"
EOS
    chmod +x "$dir/bin/gio"

    local fixture_home="$fixture_root/phones-home"
    fixture_home_make "$fixture_home"
    # Drive size on is what makes a phone row's detail reachable: a device row draws one when its
    # size is not null, and a row carrying no size at all wrote a type error into this run's log.
    printf '{"view":"list","places":{"driveSize":true,"trashCount":true}}\n' > "$state/flea/ui.json"
    local real_home="$HOME" saved_path="$PATH" old_state="${XDG_STATE_HOME:-}"
    export PATH="$dir/bin:$PATH"
    export XDG_STATE_HOME="$state"
    export HOME="$fixture_home"
    launch "$dir/files"
    export HOME="$real_home"
    wait_listing 1
    wait_rail 1

    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* \
        && "$(ipc deviceEntries)" == *"NIKON DSC D3500|device|phone|false"* \
        && "$(ipc deviceEntries)" == *"GM’s iPhone|device|phone|false"* ]] \
        || fail "phones: the three volumes are not three unmounted DEVICES rows, got $(ipc deviceEntries)"
    # Directive 44: the iPhone answers on GPhoto2 as well, and that leg is folded into the row above
    # rather than drawn beside it, so the phone is one row and its camera store is not a second.
    [[ "$(ipc deviceEntries)" != *"iPhone|device|phone|false|camera"* ]] \
        || fail "phones: the iPhone's camera leg drew its own row, got $(ipc deviceEntries)"
    # Behind the block devices, per the DEVICES rule: a block device leads and the phones are the tail.
    ipc railEntries | jq -e '[.[] | select(.group == "device") | .kind] | index("phone") as $i
        | ($i != null) and ($i > 0) and (.[$i:] | all(. == "phone"))' >/dev/null \
        || fail "phones: the phone rows do not sit behind the block devices, got $(ipc railEntries | jq -c '[.[]|select(.group=="device")|.kind]')"
    # PhoneMark rule 1: the monitor picks the mark, MTP the phone and GPhoto2 the camera.
    # AFC is a phone the same way MTP is: the mark names the transport, and only GPhoto2 is a camera.
    ipc railEntries | jq -e '[.[] | select(.kind == "phone") | .glyph] == ["smartphone", "camera", "smartphone"]' >/dev/null \
        || fail "phones: the marks are $(ipc railEntries | jq -c '[.[]|select(.kind=="phone")|.glyph]'), not phone, camera, phone"
    # RailDetails: a phone has no capacity to draw, and it keeps the fixed indicator slot a volume has.
    ipc railDetails | jq -e '[.rows[] | select(.kind == "phone")] | length == 3
        and all(.[]; .detail == "" and .indicatorVisible)' >/dev/null \
        || fail "phones: a phone row drew a size or lost its indicator, got $(ipc railDetails | jq -c '[.rows[]|select(.kind=="phone")|{detail,indicatorVisible}]')"

    # A row with no number gives its label the whole width up to the indicator slot: the label needs
    # 126 px at this text size and the numbers column used to leave it 110.
    ipc railDetails | jq -e '[.rows[] | select(.kind == "phone")] | length == 3
        and all(.[]; .labelNeeds > 0 and .labelWidth >= .labelNeeds)' >/dev/null \
        || fail "phones: a phone label is still cut, got $(ipc railDetails | jq -c '[.rows[]|select(.kind=="phone")|{label,labelWidth,labelNeeds}]')"

    # An unmounted volume offers the mount its own row does, per RailAdditions rule 2, and choosing it
    # is the row's own activation: this is the only thing that drives Sidebar.openPhone.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" && "$(ipc contextMenuEntries)" == "Mount" ]] \
        || fail "phones: an unmounted phone's menu is $(ipc contextMenuEntries), not Mount alone"
    key -k Escape >/dev/null
    settle

    # Activating the unmounted row mounts it, resolves the folder and opens it, the way a share does.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" left
    wait_path "$fuse"
    wait_listing 1
    [[ "$(ipc rowAt 0)" == "DCIM|"* ]] || fail "phones: the phone's own listing is $(ipc rowAt 0)"
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|true"* ]] \
        || fail "phones: the row did not survive its own mount, got $(ipc deviceEntries)"
    [[ -z "$(ipc networkEntries)" ]] \
        || fail "phones: gio's shadow mount became a NETWORK row as well, got $(ipc networkEntries)"
    shot phones-mounted

    # Issue 133: gio cannot trash into this mount, so neither the row nor the key is offered here.
    click_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" && "$(ipc contextMenuEntries)" == *"Move to Dropbox"* ]] \
        || fail "phones: the listing menu did not open on the phone's own row, got $(ipc contextMenuEntries)"
    [[ "$(ipc contextMenuEntries)" != *"Move to Trash"* ]] \
        || fail "phones: an MTP path still offers Move to Trash: $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle
    # The listing holds the keyboard here, so the silence below is the guard refusing and not a key
    # that went to the rail: asserted before the key rather than recovered from afterwards.
    focus_now=$(ipc focusView)
    [[ "$focus_now" == "list" ]] || fail "phones: the listing does not hold the keyboard, focus is $focus_now"
    key d >/dev/null
    # 50 ms apart cannot step over an arm that stands for four seconds, and a second of them outlasts the key's own round trip.
    for _attempt in $(seq 1 20); do
        [[ "$(ipc statusPrimary)" != *"Press d again"* ]] \
            || fail "phones: d armed a trash that can only fail, the bar reads $(ipc statusPrimary)"
        sleep 0.05
    done
    # One directory up is the folder the mount sits in rather than the mount, so the same key does arm
    # there: the silence above is the guard and not a keyboard that stopped answering.
    key h >/dev/null
    wait_path "$dir/gvfs"
    wait_listing 1
    key j >/dev/null
    settle
    key d >/dev/null
    wait_message "Press d again to trash, or Delete on its own."
    # The arm itself, not the hint: ui/Pane.qml escapePressed clears a selection before it clears the
    # bar, and coming up from the mount leaves the row we left selected (ui/js/Nav.js
    # applyPendingSelect), so the sentence stands for its four seconds while the arm is already gone.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc keyDeliveryState | jq -er '.trashArmedAt')" == "0" ]] \
        || fail "phones: Escape left the trash armed one directory up, the arm reads $(ipc keyDeliveryState | jq -c '.trashArmedAt')"
    key -k Return >/dev/null
    wait_path "$fuse"
    wait_listing 1

    # Unmount is the release a phone offers, never Eject, and the key that carries it is its uri.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Open|Unmount" ]] \
        || fail "phones: the mounted phone's menu is $(ipc contextMenuEntries), not Open then Unmount"
    menu_seek Unmount
    key -k Return >/dev/null
    for _attempt in $(seq 1 200); do
        [[ -s "$unmount_log" ]] && break
        sleep 0.05
    done
    [[ "$(cat "$unmount_log")" == "UNMOUNT $mtp_uri" ]] \
        || fail "phones: the menu row unmounted $(cat "$unmount_log"), not $mtp_uri"
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] \
        || fail "phones: the row never came back unmounted, got $(ipc deviceEntries)"

    # And the menu's own Mount drives the same activation, on the row the unmount above just released:
    # this is the only thing that reaches Sidebar.openPhone through the menu.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" right
    settle
    [[ "$(ipc contextMenuEntries)" == "Mount" ]] \
        || fail "phones: the released phone's menu is $(ipc contextMenuEntries), not Mount alone"
    key -k Return >/dev/null
    wait_path "$fuse"
    wait_listing 1
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|true"* ]] \
        || fail "phones: the menu's Mount did not mount the row, got $(ipc deviceEntries)"

    # #181 from the keyboard: Enter on the released row only starts its mount, and focus follows the folder once it opens.
    click_rail_row "$(rail_row_of 'SAMSUNG Android')" right
    settle
    menu_seek Unmount
    : > "$unmount_log"
    key -k Return >/dev/null
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"SAMSUNG Android|device|phone|false"* ]] \
        || fail "phones: the row did not come back unmounted for the keyboard leg, got $(ipc deviceEntries)"
    # Off the mount first, so the wait below can only be satisfied by this leg's own open.
    [[ "$(ipc path)" != "$fuse" ]] || { key h >/dev/null; wait_path "$dir/gvfs"; }
    key -k Tab >/dev/null
    settle
    [[ "$(ipc focusView)" == "rail" ]] || fail "phones: Tab did not reach the rail, focus is $(ipc focusView)"
    key g >/dev/null
    for ((_step = 0; _step < $(rail_row_of 'SAMSUNG Android'); _step++)); do key j >/dev/null; done
    settle
    [[ "$(ipc railCursor)" == "$(rail_row_of 'SAMSUNG Android')" ]] || fail "phones: the rail cursor is on row $(ipc railCursor), not the phone"
    rm -f "$dir/mount-started"
    key -k Return >/dev/null
    wait_marker "$dir/mount-started" "phones: Enter on the unmounted phone never started its mount"
    [[ "$(ipc focusView)" == "rail" ]] \
        || fail "phones: Enter on the unmounted phone moved focus to $(ipc focusView) before its mount landed"
    wait_path "$fuse"
    wait_listing 1
    [[ "$(ipc focusView)" == "list" ]] \
        || fail "phones: Enter on the unmounted phone opened it but left focus on $(ipc focusView), not the list"

    # The same three legs on the iPhone's own row, which reaches its files over AFC: its menu offers
    # the mount, activating it resolves the root rather than the documents volume gvfs advertises,
    # and DCIM is what lists. Directive 44's live proof on the plugged phone is the overseer's.
    click_rail_row "$(rail_row_of 'GM’s iPhone')" right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" && "$(ipc contextMenuEntries)" == "Mount" ]] \
        || fail "phones: the unmounted iPhone's menu is $(ipc contextMenuEntries), not Mount alone"
    key -k Escape >/dev/null
    settle
    click_rail_row "$(rail_row_of 'GM’s iPhone')" left
    wait_path "$afc_fuse"
    wait_listing 1
    [[ "$(ipc rowAt 0)" == "DCIM|"* ]] || fail "phones: the iPhone's own listing is $(ipc rowAt 0)"
    for _attempt in $(seq 1 200); do
        [[ "$(ipc deviceEntries)" == *"GM’s iPhone|device|phone|true"* ]] && break
        sleep 0.05
    done
    [[ "$(ipc deviceEntries)" == *"GM’s iPhone|device|phone|true"* ]] \
        || fail "phones: the iPhone row did not survive its own mount, got $(ipc deviceEntries)"
    [[ -z "$(ipc networkEntries)" ]] \
        || fail "phones: the AFC root mount became a NETWORK row as well, got $(ipc networkEntries)"
    shot phones-iphone-mounted
    printf 'PHONES rows=ok marks=ok mount=ok trash-guard=ok unmount=ok iphone=ok\n'
    export PATH="$saved_path"
    if [[ -n "$old_state" ]]; then export XDG_STATE_HOME="$old_state"; else unset XDG_STATE_HOME; fi
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
    # Every state this case needs, at zero privilege: one internal disk carrying /, and one removable
    # volume whose mountpoint goes away only once the gio stub has been told to really eject it.
    # The columns are the ones ui/DeviceMounts.qml asks for, MOUNTPOINTS and PATH included.
    cat > "$dir/bin/lsblk" <<EOS
#!/bin/sh
if [ -f "$dir/ejected" ]; then
  mp=null
else
  mp='"$dir/mnt/FLEASTICK"'
fi
cat <<JSON
{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":"238.5G","type":"disk","model":"KBG40ZNS256G",
"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":"238.5G","type":"part","model":null}]},
{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":"116.1G","type":"disk","model":"USB Flash Disk",
"children":[{"name":"sda1","path":"/dev/sda1","label":"FLEASTICK","mountpoints":[\$mp],"rm":true,"size":"116.1G","type":"part","model":null}]}]}
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
    local real_home="$HOME" saved_path="$PATH" real_state="${XDG_STATE_HOME-}"
    # Show unmounted drives ships on from 0.3.3 and puts Open and Unmount above Eject, so this seeds it off.
    seed_ui_state "$fixture_root/eject-state" '{"places":{"showUnmounted":false}}'
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
    local refusal="FLEASTICK is still mounted; close what is using it."
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
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
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
  "mount -li") cat "$dir/bin/gio-out"; exit 0 ;;
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

    # The rail also carries Trash and this box's own devices, so the bookmark-only entry is sought
    # by its label rather than counted: j from the top until the cursor row says NAS.
    rail_seek NAS

    # F2 starts the field pre-filled and pre-selected; typing replaces the whole label.
    key -k F2 >/dev/null
    settle
    [[ "$(ipc railRenamingIndex)" == "$(ipc railCursor)" ]] \
        || fail "rename: F2 did not start renaming on the cursor row, railRenamingIndex is $(ipc railRenamingIndex)"
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
    [[ "$(ipc railLabel "$(ipc railCursor)")" == "isos" ]] \
        || fail "rename: k did not reach isos, the cursor row is $(ipc railLabel "$(ipc railCursor)")"
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

# Taildrop uses guarded provider doubles; directory eligibility and second-peer dispatch remain native checks.
case_taildrop() (
    local menu_box="$fixture_root/taildrop" menu_dir="$fixture_root/taildrop/list" menus_checks=0
    local taildrop_fd dropbox_fd before provider_ready
    provider_ready=$(jq -cn '{BackendState:"Running",Self:{UserID:1001,Capabilities:["https://tailscale.com/cap/file-sharing"]},Peer:{
        "first-peer":{HostName:"Alpha",DNSName:"unused.invalid.",Online:true,TaildropTarget:1,UserID:1001},
        "second-peer":{HostName:"Bravo",DNSName:"fixture.invalid.",Online:true,TaildropTarget:1,UserID:1001}}}') \
        || fail 'taildrop: cannot construct private peer fixture'
    providers_fixture
    trap 'providers_cleanup || exit 1' EXIT
    menus_guard "$menu_dir/adir"
    mkdir "$menu_dir/adir" || fail 'taildrop: cannot create directory eligibility fixture'
    menus_guard "$menu_box/open-bin"
    mkdir "$menu_box/open-bin" || fail 'taildrop: cannot create private opener directory'
    [[ "$open_handoff" == gio ]] || fail 'taildrop: opener contract changed; isolate the new handoff before running'
    menus_guard "$menu_box/open-bin/$open_handoff"
    cat > "$menu_box/open-bin/$open_handoff" <<'EOS'
#!/usr/bin/env bash
set -eu
[[ "${1:-}" == open ]] || exec /usr/bin/gio "$@"
box=${FLEA_PROVIDERS_BOX:?}
[[ "$box" == /* && -f "$box/.flea-test-sandbox" ]] || exit 90
log=$(realpath -m -- "$box/calls.jsonl")
[[ "$log" == "$box/"* && "$log" != "$box" ]] || exit 91
jq -cn --arg helper gio --args '{helper:$helper,args:$ARGS.positional}' -- "$@" >> "$log"
EOS
    chmod 700 "$menu_box/open-bin/$open_handoff" || fail 'taildrop: cannot make the private opener executable'
    providers_install tailscale yes
    providers_install omarchy-tailscale-send yes
    export HOME="$menu_box/home" XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config"
    export XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    export PATH="$menu_box/open-bin:$menu_box/bin" FLEA_PROVIDERS_BOX="$menu_box"
    "$flea_bin" --ui-state '{"view":"list","keys":"default","menu":{"hidden":[]}}' >/dev/null \
        || fail 'taildrop: private settings seed failed'
    launch "$menu_dir"
    wait_listing 3
    providers_open adir pointer
    providers_disabled taildrop 'Taildrop sends files only'
    menus_expect menuState 'any(.entries[]; .action == "taildrop" and .disabled and .submenu == [])' 'directory keeps its installed provider but offers no file targets'
    providers_close

    providers_open b-cursor.txt pointer
    menus_expect menuState 'any(.entries[]; .action == "taildrop" and (.disabled | not) and .mark == "tailscale")' 'file menu uses the Tailscale brand mark'
    menus_shot taildrop-menu
    providers_seek taildrop
    key -k Return >/dev/null || fail 'taildrop: submenu Enter failed'
    menus_expect menuState '.submenu and .submenuCursor == 0 and (.submenuEntries | map(.label)) == ["Alpha","Bravo"]' 'Enter opens both fixture peers in name order'
    menus_equal 'each peer retains the machine glyph' 'server|server' "$(ipc contextMenuSubmenuGlyphs)"
    menus_shot taildrop-flyout
    key -k Down >/dev/null || fail 'taildrop: second-peer movement failed'
    menus_expect menuState '.submenu and .submenuCursor == 1 and .submenuEntries[.submenuCursor].id == "second-peer"' 'Down selects the second peer identity'
    before=$(providers_calls omarchy-tailscale-send)
    key -k Return >/dev/null || fail 'taildrop: second-peer Enter failed'
    providers_call omarchy-tailscale-send "$(jq -cn --arg path "$menu_dir/b-cursor.txt" '["fixture.invalid",$path]')" "$before"
    menus_expect menuState '(.opened | not) and (.submenu | not)' 'dispatch closes both native menus'
    providers_expect '.listFocus' 'dispatch restores listing focus'
    menus_message 'Sending b-cursor.txt to Bravo.' 'dispatch names the chosen peer and exact cursor file'
    menus_equal 'dispatch preserves source bytes' 'list/b-cursor.txt original' "$(cat "$menu_dir/b-cursor.txt")"
    menus_shot taildrop-sent

    providers_mode tailscale ready '{"BackendState":"NeedsLogin","Peer":{}}'
    providers_open b-cursor.txt pointer
    providers_disabled taildrop 'signed out'
    menus_expect menuState 'any(.entries[]; .action == "taildrop" and .disabled and .submenu == [])' 'signed-out installation offers no cached peer'
    menus_shot taildrop-signed-out
    providers_close
    providers_install tailscale no
    before=$(providers_calls tailscale)
    providers_open b-cursor.txt pointer
    menus_expect menuState 'all(.entries[]; .action != "taildrop")' 'missing installation removes the provider row'
    providers_expect '.facts.taildrop.installed == false and (.taildrop.checking | not)' 'private PATH proves the provider is absent'
    menus_equal 'absence does not attempt a missing status helper' "$before" "$(providers_calls tailscale)"
    menus_shot taildrop-absent
    providers_close
    menus_equal 'native menu input never opens a file' 0 "$(providers_calls gio)"
    menus_equal 'Taildrop never touches the clipboard recorder' 0 "$(providers_calls wl-copy)"
    printf 'TAILDROP checks=%s directory-disabled=ok brand=ok submenu=ok second-peer-recorded=ok signed-out-disabled=ok absent=ok isolated=ok\n' "$menus_checks"
)

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
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
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

# The settings panel: its doors, its seven sections, and the one thing a settings window
# has to do that a menu does not, which is outlive the process that wrote it. XDG_STATE_HOME and
# XDG_CONFIG_HOME both point inside the fixture root for the whole case, so nothing here can write
# the operator's own ~/.local/state/flea/ui.json; hard rule 9 covers writes and not only deletes.
case_places() {
    local dir="$fixture_root/places"
    local config="$fixture_root/places-config"
    local state="$fixture_root/places-state"
    sandbox_scratch "$dir"
    sandbox_scratch "$config"
    sandbox_scratch "$state"
    : > "$dir/a.txt"
    mkdir -p "$dir/sub" || fail "places: the second favourite's folder could not be made"
    export XDG_CONFIG_HOME="$config" XDG_STATE_HOME="$state"
    settings_seed "$state" "$config" "$state/flea/ui.json"
    launch "$dir"
    wait_listing 2
    settings_places "$dir" places-favourites
    kill_flea
}

case_dual() {
    local dir="$fixture_root/dual" state="$fixture_root/dual-state" before status_disk_x
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    mkdir -p "$dir/left/nested" "$dir/right" "$state/flea"
    printf 'left\n' > "$dir/left/a.txt"
    printf 'left\n' > "$dir/left/b.txt"
    printf 'nested\n' > "$dir/left/nested/one.txt"
    printf 'right\n' > "$dir/right/c.txt"
    printf 'right\n' > "$dir/right/d.txt"
    export XDG_STATE_HOME="$state"
    jq -n --arg left "$dir/left" --arg right "$dir/right" \
        '{view:"list",keys:"default",dual:{paths:[$left,$right],focus:0}}' > "$state/flea/ui.json"
    launch "$dir/left"
    wait_listing 3
    # The baseline the disk facts hold to for the rest of this case, taken before any transient exists.
    status_disk_x=$(ipc statusFooterState | jq -r '.disk.x')
    [[ "$status_disk_x" =~ ^[0-9.]+$ ]] || fail "dual: the quiet strip reported no disk position, got [$status_disk_x]"
    # The single view keeps the pane's own path at height 0, so it must hold no crumbs to rebuild on every move.
    [[ "$(ipc paneCrumbCount 0)" == 0 ]] || fail "dual: the hidden pane path holds $(ipc paneCrumbCount 0) crumbs in the single view"
    click_chrome dual
    settle
    ipc dualState | jq -e '.active and .focused == 0' >/dev/null || fail "dual: chrome did not enter dual mode"
    key -k Tab >/dev/null
    wait_listing 2
    [[ "$(ipc path)" == "$dir/right" ]] || fail "dual: Tab did not focus independent right listing"
    key j >/dev/null
    settle
    [[ "$(ipc cursor)" == 1 ]] || fail "dual: right cursor did not move"
    key -M ctrl -k Tab -m ctrl >/dev/null
    settle
    ipc dualState | jq -e '.focused == 1' >/dev/null || fail "dual: single-pane preview chord moved dual focus"
    key -k Tab >/dev/null
    settle
    [[ "$(ipc cursor)" == 0 && "$(ipc path)" == "$dir/left" ]] || fail "dual: left state was overwritten"
    key l >/dev/null
    wait_listing 1
    [[ "$(ipc path)" == "$dir/left/nested" ]] || fail "dual: left navigation did not enter nested folder"
    # A message first, or the lane is empty and its centre is a point rather than a measured box.
    key y >/dev/null
    settle
    transient_beside_disk "dual left"
    shot dual-transient-left
    key -k Tab >/dev/null
    settle
    key y >/dev/null
    settle
    transient_beside_disk "dual right"
    shot dual-transient-right
    key -k Tab >/dev/null
    settle
    key -k Tab >/dev/null
    settle
    [[ "$(ipc path)" == "$dir/right" && "$(ipc cursor)" == 1 ]] || fail "dual: left navigation changed right state"
    printf 'external\n' > "$dir/left/nested/two.txt"
    local attempt
    for attempt in $(seq 1 40); do
        if ipc dualState | jq -e '.panes[0].total == 2 and (.panes[0].loading | not)' >/dev/null; then break; fi
        sleep 0.1
    done
    ipc dualState | jq -e '.panes[0].total == 2 and .panes[1].cursor == 1' >/dev/null \
        || fail "dual: independent watch refresh did not preserve right cursor"
    shot dual-right-focused
    before=$(ipc dualState)
    kill_flea
    # A named folder goes to the focused side since 0.3.3 (case_duallaunch); the restored pair is what dualState proves below.
    launch "$dir/right"
    wait_listing 2
    [[ "$(ipc path)" == "$dir/right" ]] || fail "dual: the focused pane is not on $dir/right after the relaunch"
    ipc dualState | jq -e --arg path "$dir/left/nested" '.active and .focused == 1 and .panes[0].path == $path' >/dev/null \
        || fail "dual: independent paths did not survive restart"
    shot dual-reopened
    local crumbs target centre cx cy wx wy _ww _wh
    # Quick Look covers both panes, so a press on its ground over the left pane's parent crumb must not navigate that pane.
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == true ]] || fail "dual: space did not open Quick Look on the right pane's file"
    centre=$(ipc paneCrumbCentre 0 "$(( $(ipc paneCrumbCount 0) - 2 ))")
    read -r cx cy <<< "$centre"
    # A null or empty centre would press the window's corner and pass without reaching the crumb.
    [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "dual: the left pane's parent crumb has no centre under Quick Look [$centre]"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null || fail "dual: omarchy-drive refused the press over Quick Look"
    settle
    settle
    shot dual-quicklook-crumb
    ipc dualState | jq -e --arg nested "$dir/left/nested" '.panes[0].path == $nested' >/dev/null \
        || fail "dual: a press on Quick Look reached the left pane's crumb beneath, left is at $(ipc dualState | jq -r '.panes[0].path')"
    [[ "$(ipc previewOpen)" == false ]] || { key -k Escape >/dev/null; settle; }
    # The right pane keeps focus through it, so the tap below is what moves focus left.
    [[ "$(ipc dualState | jq -r '.focused')" == 1 ]] || fail "dual: a press on Quick Look moved focus to pane $(ipc dualState | jq -r '.focused')"
    # Issue 45 in dual view: the unfocused pane's own path answers one tap on a parent, and the tap focuses that pane.
    crumbs=$(ipc paneCrumbCount 0)
    (( crumbs >= 3 )) || fail "dual: the left pane drew $crumbs crumbs, too few to press a parent"
    target=$((crumbs - 2))
    centre=$(ipc paneCrumbCentre 0 "$target")
    [[ -n "$centre" ]] || fail "dual: left crumb $target has no on-screen centre"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null \
        || fail "dual: omarchy-drive refused the press on left crumb $target"
    # Two settles, so the reading lands after the tap's navigation has had time to reach both panes.
    settle
    settle
    shot dual-crumb
    ipc dualState | jq -e --arg left "$dir/left" --arg right "$dir/right" \
        '.focused == 0 and .panes[0].path == $left and .panes[1].path == $right' >/dev/null \
        || fail "dual: a tap on the left pane's parent crumb left $(ipc dualState | jq -c '[.focused, .panes[0].path, .panes[1].path]')"
    [[ "$(ipc pathBarOpen)" == "false" ]] || fail "dual: a single tap on a pane crumb opened the path bar"
    # A double click types the path wherever a tap opens nothing: the padding before the first crumb, the space after the last, the current folder's own crumb.
    local strip sx sy sw sh press_x press_y crumb_inset
    read -r sx sy sw sh <<< "$(ipc panePathRect 0)"
    [[ -n "$sh" ]] || fail "dual: the left pane path has no on-screen box"
    # The crumbs start rowPaddingX in and fit inside it at the far end, so a press nearer either edge is on the strip's own area.
    crumb_inset=$(ipc metrics | cut -d' ' -f3)
    (( chrome_band_inset < crumb_inset )) || fail "dual: a press $chrome_band_inset px in would land on a crumb, which starts $crumb_inset px in"
    # The last crumb is the pane's own folder, the one crumb a tap cannot open.
    target=$(( $(ipc paneCrumbCount 0) - 1 ))
    centre=$(ipc paneCrumbCentre 0 "$target")
    [[ -n "$centre" ]] || fail "dual: left crumb $target has no on-screen centre after the tap"
    read -r cx cy <<< "$centre"
    for strip in padding tail crumb; do
        case $strip in
            padding) press_x=$((sx + chrome_band_inset)) press_y=$((sy + sh / 2)) ;;
            tail) press_x=$((sx + sw - chrome_band_inset)) press_y=$((sy + sh / 2)) ;;
            crumb) press_x=$cx press_y=$cy ;;
        esac
        omarchy-drive click "$((press_x + wx))" "$((press_y + wy))" --double >/dev/null \
            || fail "dual: omarchy-drive refused the double click on the left pane path's $strip"
        settle
        settle
        shot "dual-path-double-$strip"
        [[ "$(ipc pathBarOpen)" == "true" ]] || fail "dual: a double click on the left pane path's $strip did not open the path bar"
        ipc dualState | jq -e --arg left "$dir/left" '.panes[0].path == $left' >/dev/null \
            || fail "dual: the double click on the left pane path's $strip navigated to $(ipc dualState | jq -r '.panes[0].path')"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc pathBarOpen)" == "false" ]] || fail "dual: Escape did not close the path bar opened from the pane path's $strip"
    done
    # Leaving dual has to put focus back on the primary pane, which only shows if the second pane holds it first.
    key -k Tab >/dev/null
    settle
    ipc dualState | jq -e '.focused == 1' >/dev/null || fail "dual: Tab did not focus the right pane before leaving dual"
    click_chrome list
    settle
    ipc dualState | jq -e '(.active | not) and .focused == 0' >/dev/null || fail "dual: leaving dual did not restore primary focus"
    kill_flea
    printf 'DUAL navigation=ok focus=ok watch=ok restart=ok crumb=ok before=%s\n' "$before"
}

dual_sort_wait() {
    local mark="$1" first="$2" deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        if [[ "$(ipc sortMark)" == "$mark" && "$(ipc rowAt 0)" == "$first|"* && "$(ipc listInFlight)" == false ]]; then
            menus_checks=$((menus_checks + 1))
            printf 'DUAL_SORT_CHECK %s mark=%s first=%s\n' "$menus_checks" "$mark" "$first"
            return
        fi
        sleep 0.05
    done
    fail "dualsort: expected $mark/$first, observed $(ipc sortMark)/$(ipc rowAt 0)"
}

dual_sort_header() {
    local column="$1" x width
    IFS='|' read -r x width <<< "$(ipc headerCellRect "$column")"
    [[ "$x" =~ ^[0-9]+$ && "$width" =~ ^[0-9]+$ && "$width" -gt 0 ]] \
        || fail "dualsort: header $column has no native pointer target"
    menus_point "$(( $(ipc headerLeft) + x + width / 2 )) $(( $(ipc headerTop) + $(ipc chromeHeight) / 2 ))"
}

case_dualsort() {
    local dir="$fixture_root/dual-sort" state="$fixture_root/dual-sort-state" menus_checks=0
    local side index name left right left_scroll right_scroll mode list_requests date_epoch=1700000000
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    for side in left right; do
        mkdir "$dir/$side" || fail 'dualsort: private listing directory failed'
        for ((index = 0; index < 80; index++)); do
            printf -v name 'file-%02d.txt' "$index"
            truncate -s "$((index + 1))" "$dir/$side/$name" || fail 'dualsort: private file creation failed'
            touch -d "@$((date_epoch + index))" "$dir/$side/$name" || fail 'dualsort: private file date failed'
        done
    done
    seed_ui_state "$state" "$(jq -cn --arg left "$dir/left" --arg right "$dir/right" \
        '{view:"dual",keys:"default",sort:{key:"name",reverse:false},dual:{paths:[$left,$right],focus:0}}')"
    launch "$dir/left"
    menus_expect dualState '.active and .focused == 0 and all(.panes[]; .total == 80 and (.loading | not))' 'both dual listings start independently'
    dual_sort_wait name:asc file-00.txt
    key -k End >/dev/null; key v >/dev/null
    menus_expect dualState '.panes[0].cursor == 79 and .panes[0].selected == [79]' 'primary retains a nonzero marked cursor'
    menus_expect listContentY '. > 0' 'primary scrolls to its marked cursor'
    left=$(ipc dualState | jq -c '.panes[0] | del(.focused)')
    left_scroll=$(ipc listContentY)
    key -k Tab >/dev/null; key S >/dev/null
    dual_sort_wait name:desc file-79.txt
    menus_expect dualState ".focused == 1 and ((.panes[0] | del(.focused)) == $left)" 'secondary reverse leaves primary listing, marks and requests unchanged'
    settings_wait_value '.sort.key == "name" and .sort.reverse == false'
    key -k Tab >/dev/null
    menus_equal 'primary scroll survives peer sorting' "$left_scroll" "$(ipc listContentY)"
    menus_equal 'primary sort survives peer sorting' name:asc "$(ipc sortMark)"
    dual_sort_header size
    dual_sort_wait size:asc file-00.txt
    key -k Tab >/dev/null
    menus_equal 'secondary sort survives primary header click' name:desc "$(ipc sortMark)"
    key -k End >/dev/null; key v >/dev/null
    menus_expect dualState '.panes[1].cursor == 79 and .panes[1].selected == [79]' 'secondary retains a nonzero marked cursor'
    menus_expect listContentY '. > 0' 'secondary scrolls to its marked cursor'
    right=$(ipc dualState | jq -c '.panes[1] | del(.focused)')
    right_scroll=$(ipc listContentY)
    key -k Tab >/dev/null; key S >/dev/null
    dual_sort_wait size:desc file-79.txt
    menus_expect dualState ".focused == 0 and ((.panes[1] | del(.focused)) == $right)" 'primary reverse leaves secondary listing, marks and requests unchanged'
    shot dual-sort-independent

    click_chrome list
    menus_expect dualState '(.active | not) and .focused == 0' 'leaving dual exposes the primary listing'
    dual_sort_header size
    dual_sort_wait size:asc file-00.txt
    settings_wait_value '.sort.key == "size" and .sort.reverse == false'
    menus_expect dualState "(.active | not) and ((.panes[1] | del(.focused)) == $right)" 'single-pane sort persistence leaves the hidden secondary session unchanged'
    click_chrome dual
    menus_expect dualState ".active and ((.panes[1] | del(.focused)) == $right)" 'reopening dual retains secondary listing and selection'
    key -k Tab >/dev/null
    menus_equal 'reopened secondary keeps its session sort' name:desc "$(ipc sortMark)"
    # No session stores a pixel offset: a reopened pane re-derives its view from the cursor it kept,
    # so what is restored is the marked row, on screen, with the listing still scrolled off its top.
    menus_expect dualState '.panes[1].cursor == 79 and .panes[1].selected == [79]' 'reopened secondary keeps its marked cursor'
    menus_expect listContentY '. > 0' 'reopened secondary is still scrolled to that cursor'
    [[ -n "$(ipc rowCentre 79)" ]] || fail "dualsort: the reopened secondary's marked row is off screen"
    shot dual-sort-reopened

    settings_open_key
    settle
    settings_section view
    settings_focus_row sort.key
    key l >/dev/null
    settings_wait_value '.sort.key == "date"'
    key l >/dev/null
    settings_wait_value '.sort.key == "kind" and .sort.reverse == false'
    key -k Escape >/dev/null
    dual_sort_wait kind:asc file-00.txt
    key -k Tab >/dev/null
    dual_sort_wait kind:asc file-00.txt
    key -k Tab >/dev/null; key S >/dev/null
    dual_sort_wait kind:desc file-79.txt
    settings_wait_value '.sort.key == "kind" and .sort.reverse == false'
    shot dual-sort-settings

    for mode in typing results; do
        dual_sort_header size
        dual_sort_wait size:asc file-00.txt
        key S >/dev/null
        dual_sort_wait size:desc file-79.txt
        key f >/dev/null
        menus_expect keyDeliveryState '.searchMode == "typing"' "dual Search $mode starts through native key"
        if [[ "$mode" == results ]]; then
            key file- -k Return >/dev/null
            menus_expect keyDeliveryState '.searchMode == "results" and (.searchRunning | not)' 'dual Search finishes its private fixture walk'
        fi
        click_chrome sliders
        menus_expect settingsOpen '. == true' "Settings opens over dual Search $mode"
        settings_section view
        settings_focus_row sort.key
        key h >/dev/null
        settings_wait_value '.sort.key == "date" and .sort.reverse == false'
        key -k Escape >/dev/null
        menus_expect keyDeliveryState ".searchMode == \"$mode\"" "Settings preserves dual Search $mode"
        menus_equal "dual Search $mode defers Settings sort until browsing" size:desc "$(ipc sortMark)"
        list_requests=$(ipc dualState | jq -er '.panes[1].listRequests')
        key -k Escape >/dev/null
        menus_expect keyDeliveryState '.searchMode == ""' "Escape closes dual Search $mode"
        dual_sort_wait mtime:asc file-00.txt
        menus_expect dualState ".panes[1].listRequests == $((list_requests + 1))" "dual Search $mode consumes deferred sort in one listing"
        key -k Tab >/dev/null
        dual_sort_wait mtime:asc file-00.txt
        key -k Tab >/dev/null
        shot "dual-sort-search-$mode"
        settings_open_key
        menus_expect settingsOpen '. == true' 'Settings reopens after deferred sort applies'
        settings_section view
        settings_focus_row sort.key
        key l >/dev/null
        settings_wait_value '.sort.key == "kind" and .sort.reverse == false'
        key -k Escape >/dev/null
        dual_sort_wait kind:asc file-00.txt
    done
    kill_flea
    launch "$dir/left"
    menus_expect dualState '.active and .focused == 1 and all(.panes[]; .total == 80 and (.loading | not))' 'restart restores paths and focus with the saved default sort'
    dual_sort_wait kind:asc file-00.txt
    key -k Tab >/dev/null
    dual_sort_wait kind:asc file-00.txt
    kill_flea
    printf 'DUAL_SORT native-key=independent header-pointer=independent hidden-session=retained settings=live search-settings=deferred single-persistence=ok restart=defaults checks=%s\n' "$menus_checks"
}

case_settings() {
    local dir="$fixture_root/settings"
    local config="$fixture_root/settings-config"
    local state="$fixture_root/settings-state"
    sandbox_scratch "$dir"
    sandbox_scratch "$config"
    sandbox_scratch "$state"
    : > "$dir/a.txt"
    : > "$dir/b.txt"
    mkdir -p "$dir/sub" || fail "settings: the second favourite's folder could not be made"
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
    wait_listing 3

    settings_doors
    settings_view
    settings_preview
    settings_places "$dir"
    settings_about
    settings_display
    settings_menus
    settings_keys

    # One override left standing, so the restart below has a text size to bring back as well.
    key -M ctrl -M shift -k equal -m shift -m ctrl >/dev/null
    settle
    local pinned_base
    pinned_base=$(token_of baseSize)
    # Running text draws at the stop itself (GM, 2026-09-07); the row name is the rendered proof, bodySmall stays the geometry token.
    # rowNamePx reads the list's delegate, so the list has to be the view showing and row 0 a named row.
    [[ "$(ipc viewMode)" == "list" && "$(ipc rowAt 0)" == *"|"* ]] || fail "settings: the typography proof needs the list view on a named row, got $(ipc viewMode) and '$(ipc rowAt 0)'"
    [[ "$(ipc bodyPx)" == "$pinned_base" && "$(ipc rowNamePx 0)" == "$pinned_base" ]] \
        || fail "settings: at the ${pinned_base}px stop the body draws $(ipc bodyPx) and row 0's name $(ipc rowNamePx 0)"

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
    wait_listing 3
    [[ "$(token_of baseSize)" == "$pinned_base" ]] \
        || fail "settings: a restart lost the ${pinned_base}px override, it draws at $(token_of baseSize)"
    settings_open_key
    settle
    settings_section display
    [[ "$(ipc settingsRows)" == *"fact|Effective|${pinned_base} px"* ]] \
        || fail "settings: a restart brought the panel back on a different stop"
    # A new process opens on View; the explicit Display selection above reads the pinned size, while
    # the master row is not reachable until the rail has been walked. The master is derived from the
    # stored set, so a restart that read only menu.hidden must still draw the five of six the panel
    # left behind, and the six rows under it must agree with it.
    settings_section menus
    [[ "$(ipc settingsRows)" == *"group|Basic file actions|5 of 6"* ]] \
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
    # Following, running text is Omarchy's own regular body, which is its base size, and the row name draws it.
    [[ "$(ipc bodyPx)" == "$(token_of baseSize)" && "$(ipc rowNamePx 0)" == "$(ipc bodyPx)" ]] \
        || fail "settings: following Omarchy the body draws $(ipc bodyPx) against base $(token_of baseSize), row 0's name $(ipc rowNamePx 0)"

    settings_read_refused "$stored" "$dir"

    printf 'SETTINGS doors=ok view=ok places=ok preview=ok about=ok display=ok menus=ok keys=ok restart=ok backend=ok refused=ok unread=ok\n'
    if [[ -n "$real_config" ]]; then export XDG_CONFIG_HOME="$real_config"; else unset XDG_CONFIG_HOME; fi
    if [[ -n "$real_state" ]]; then export XDG_STATE_HOME="$real_state"; else unset XDG_STATE_HOME; fi
    kill_flea
}

# Preset-specific native delivery, with no mutation through the IPC seam.
settings_open_key() {
    case "$(ipc keymapPreset)" in
        mac|windows) key -M ctrl -k comma -m ctrl >/dev/null ;;
        *) key , >/dev/null ;;
    esac
}

settings_focus_row() {
    local id="$1" target cursor count attempt
    target=$(ipc settingsModel | jq -r --arg id "$id" 'map(.id) | index($id) // empty')
    count=$(ipc settingsModel | jq 'length')
    [[ "$target" =~ ^[0-9]+$ ]] || fail "settings: no control with id $id"
    [[ "$(ipc settingsSide)" == "pane" ]] || { key -k Tab >/dev/null; settle; }
    for (( attempt = 0; attempt <= count; attempt++ )); do
        cursor=$(ipc settingsCursor)
        [[ "$cursor" == "$target" ]] && return
        if (( cursor < target )); then key j >/dev/null; else key k >/dev/null; fi
        settle
    done
    fail "settings: keyboard could not focus $id at row $target"
}

settings_wait_value() {
    local filter="$1" attempt
    for attempt in $(seq 1 30); do
        if ipc uiSettings | jq -e "$filter" >/dev/null \
            && jq -e "$filter" "$XDG_STATE_HOME/flea/ui.json" >/dev/null; then return; fi
        sleep 0.1
    done
    fail "settings: session and persisted state never agreed on $filter"
}

settings_click_control() {
    local id="$1" wx wy ww wh cx cy
    settings_focus_row "$id"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    read -r cx cy <<< "$(ipc settingsRowCentre "$id")"
    [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "settings: no real centre for $id"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    settle
}

settings_view() {
    settings_open_key
    settle
    settings_section view
    local inventory
    inventory=$(ipc settingsSections | jq -r 'map(.id) | join(",")')
    [[ "$inventory" == "view,places,shelf,preview,keys,display,menus,about" ]] || fail "settings: wrong rail order $inventory"
    settings_focus_row view
    key l >/dev/null; settle
    [[ "$(ipc viewMode)" == "columns" ]] || fail "settings: View choice did not change the listing"
    key h >/dev/null; settle
    [[ "$(ipc viewMode)" == "list" ]] || fail "settings: View choice did not restore List"
    settings_focus_row addressBar
    key h >/dev/null; settle
    settings_wait_value '.addressBar == "path"'
    key l >/dev/null; settle
    settings_wait_value '.addressBar == "breadcrumb"'
    settings_focus_row density
    local chrome rail before_card
    chrome=$(token_of chromeHeight)
    rail=$(token_of railRowHeight)
    before_card=$(ipc settingsCardRect)
    # Compact is the default from 0.3.2, so the first step right lands on Normal.
    key l >/dev/null; settle
    settings_wait_value '.density == "normal"'
    [[ "$(token_of chromeHeight)" == "$chrome" && "$(token_of railRowHeight)" == "$rail" && "$(ipc settingsCardRect)" == "$before_card" ]] \
        || fail "settings: row density changed shared chrome, rail, or Settings card geometry"
    key h >/dev/null; settle
    settings_wait_value '.density == "compact"'
    settings_click_control hidden
    settings_wait_value '.hidden == true'
    [[ "$(ipc showHidden)" == "true" ]] || fail "settings: Show hidden control has no listing consumer"
    key -k Space >/dev/null; settle
    settings_wait_value '.hidden == false'
    settings_focus_row wrapAtEnds
    key -k Space >/dev/null; settle
    settings_wait_value '.wrapAtEnds == true'
    key -k Escape >/dev/null; settle
    key -k End >/dev/null; key j >/dev/null; settle
    [[ "$(ipc cursor)" == "0" ]] || fail "settings: Wrap at list ends did not wrap the native cursor"
    settings_open_key; settle
    settings_section view
    settings_focus_row wrapAtEnds
    key -k Space >/dev/null; settle
    settings_wait_value '.wrapAtEnds == false'
    shot settings-view
    key -k Escape >/dev/null; settle
}

settings_preview() {
    settings_open_key; settle
    settings_section preview
    settings_focus_row preview.loadOn
    key l >/dev/null; settle
    settings_wait_value '.preview.loadOn == "manual"'
    key h >/dev/null; settle
    settings_wait_value '.preview.loadOn == "automatic"'
    settings_click_control preview.column
    settings_wait_value '.preview.column == false'
    key -k Space >/dev/null; settle
    settings_wait_value '.preview.column == true'
    settings_focus_row preview.thumbnails
    key l >/dev/null; settle
    settings_wait_value '.preview.thumbnails == "off"'
    key h >/dev/null; settle
    settings_wait_value '.preview.thumbnails == "media"'
    settings_focus_row preview.thumbSize
    key l >/dev/null; settle
    settings_wait_value '.preview.thumbSize == "large"'
    key h >/dev/null; settle
    settings_wait_value '.preview.thumbSize == "medium"'
    settings_click_control preview.ctrlZoom
    settings_wait_value '.preview.ctrlZoom == false'
    key -k Space >/dev/null; settle
    settings_wait_value '.preview.ctrlZoom == true'
    shot settings-preview
    key -k Escape >/dev/null; settle
}

settings_places() {
    local dir="$1" frame="${2:-settings-places}" flag group
    settings_open_key; settle
    settings_section places
    settings_wait_value '.places.favourites == []'
    settings_focus_row addFavourite
    key -k Return >/dev/null; settle
    settings_wait_value '.places.favourites | length == 1'
    # Issue 138, src/favourites.rs: a place already held is kept once however it is spelled, so the
    # same folder added twice is still one row. The new favourite is also a row of its own now, so
    # the button is asked for by id rather than assumed to be still under the cursor.
    settings_focus_row addFavourite
    key -k Return >/dev/null; settle
    ipc uiSettings | jq -e '.places.favourites | length == 1' >/dev/null \
        || fail "settings: the same folder was saved twice, Favorites holds $(ipc uiSettings | jq -c '.places.favourites')"
    # The second favourite is a second directory, which is what the move and remove rows below need.
    key -k Escape >/dev/null; settle
    seek_row_named sub
    key -k Return >/dev/null
    wait_path "$dir/sub"
    settings_open_key; settle
    settings_section places
    settings_focus_row addFavourite
    key -k Return >/dev/null; settle
    settings_wait_value '.places.favourites | length == 2'
    ipc uiSettings | jq -e --arg dir "$dir" '[.places.favourites[].path] == [$dir, $dir + "/sub"]' >/dev/null \
        || fail "settings: Favorites holds $(ipc uiSettings | jq -c '[.places.favourites[].path]'), not the two folders in the order they were added"
    settings_focus_row favourite:0
    key -M shift -k j -m shift >/dev/null; settle
    [[ "$(ipc settingsCursor)" == "2" ]] || fail "settings: Shift+J did not keep focus on the moved favourite"
    # SettingsRest rule 4: x on the row is the same action its own mark performs.
    key x >/dev/null; settle
    settings_wait_value '.places.favourites | length == 1'
    settings_focus_row favourite:0
    key -k Delete >/dev/null; settle
    settings_wait_value '.places.favourites == []'
    for flag in showHome showNetwork showDevices showTrash; do
        settings_click_control "places.$flag"
        settings_wait_value ".places.$flag == false"
        case "$flag" in
            showHome) group=home ;;
            showNetwork) group=network ;;
            showDevices) group=device ;;
            showTrash) group=trash ;;
            *) group= ;;
        esac
        if [[ -n "$group" ]]; then
            ipc railEntries | jq -e --arg group "$group" 'all(.[]; .group != $group)' >/dev/null \
                || fail "settings: $flag did not remove its actual rail rows"
        fi
        key -k Space >/dev/null; settle
        settings_wait_value ".places.$flag == true"
    done
    for flag in driveSize trashCount; do
        settings_wait_value ".places.$flag == false"
        settings_click_control "places.$flag"
        settings_wait_value ".places.$flag == true"
        key -k Space >/dev/null; settle
        settings_wait_value ".places.$flag == false"
    done
    settings_focus_row places.sidebarWidth
    key l >/dev/null; settle
    settings_wait_value '.places.sidebarWidth == 256'
    key h >/dev/null; settle
    settings_wait_value '.places.sidebarWidth == 224'
    shot "$frame"
    key -k Escape >/dev/null; settle
    # Back where this block started: the second favourite was added from inside sub, and every caller
    # after this one drives the fixture's own listing.
    key h >/dev/null
    wait_path "$dir"
}

settings_about() {
    settings_open_key; settle
    settings_section about
    local rows
    rows=$(ipc settingsModel)
    printf '%s' "$rows" | jq -e 'any(.[]; .kind == "fact" and .label == "Language" and .value == "English")' >/dev/null \
        || fail "settings: About language is not passive metadata"
    printf '%s' "$rows" | jq -e 'any(.[]; .id == "support" and .kind == "action") and any(.[]; .id == "reportIssue" and .kind == "action")' >/dev/null \
        || fail "settings: About omitted support routes"
    shot settings-about
    settings_focus_row keyboardSheet
    key -k Return >/dev/null; settle
    [[ "$(ipc keymapSheetOpen)" == "true" && "$(ipc settingsOpen)" == "false" ]] || fail "settings: About Keyboard sheet did not open the real sheet"
    key -k Escape >/dev/null; settle
}

# The state this case starts from, laid down through flea --ui-state so the schema sees it too. The
# unknown key goes in by hand afterwards, because the CLI refuses a key this build does not know.
settings_seed() {
    local state="$1" config="$2" stored="$3"
    env XDG_STATE_HOME="$state" XDG_CONFIG_HOME="$config" "$flea_bin" --ui-state \
        '{"columns":["name","size"],"places":{"sidebarWidth":224},"sort":{"key":"size"}}' >/dev/null \
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
    settings_backend_holds "$doc" '.places.sidebarWidth == 224' "places.sidebarWidth"
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
    wait_listing 3
    [[ "$(ipc lastMessage)" == "Your saved settings could not be read, so these are the defaults." ]] \
        || fail "settings: an unreadable state file was not reported, the status bar says $(ipc lastMessage)"
    # And the write half of that same file, one keystroke away: the window is holding the shipped
    # defaults, so a patch that went ahead would rename a full default document over every key in it.
    key -M ctrl -M shift -k minus -m shift -m ctrl >/dev/null
    settle
    [[ "$(ipc lastMessage)" == "Your saved settings could not be read, so these are the defaults." ]] \
        || fail "settings: a second failure acknowledged the unreadable-settings error"
    # Three refusals can be waiting here: the read, the Favorites reader's own (ui/ViewState.qml
    # favouritesReadError, raised by the same unreadable file) and the save. Escape dismisses the head
    # of that queue one at a time, the way the operator would, until the save's sentence is showing.
    local dismissed
    for dismissed in 1 2 3; do
        key -k Escape >/dev/null
        settle
        [[ "$(ipc lastMessage)" == "That setting could not be saved." ]] && break
    done
    [[ "$(ipc lastMessage)" == "That setting could not be saved." ]] \
        || fail "settings: a save onto an unreadable state file was not reported, the status bar says $(ipc lastMessage)"
    kill_flea
    chmod 600 "$stored" || fail "settings: the state file could not be made readable again"
    [[ "$(sha256sum "$stored" | cut -d' ' -f1)" == "$before_sha" ]] \
        || fail "settings: a save onto an unreadable state file spent the operator's bytes"
    [[ "$(stat -c '%i' "$stored")" == "$before_ino" ]] \
        || fail "settings: a save onto an unreadable state file renamed a new file over it"
}

case_settingsrefused() {
    local dir="$fixture_root/settingsrefused" state="$fixture_root/settingsrefused-state"
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    local old_state="${XDG_STATE_HOME:-}" stored="$state/flea/ui.json"
    mkdir -p "$state/flea"
    printf '{"view":"list"}\n' > "$stored"
    printf 'a\n' > "$dir/a.txt"
    printf 'b\n' > "$dir/b.txt"
    printf 'c\n' > "$dir/c.txt"
    export XDG_STATE_HOME="$state"
    settings_read_refused "$stored" "$dir"
    if [[ -n "$old_state" ]]; then export XDG_STATE_HOME="$old_state"; else unset XDG_STATE_HOME; fi
}

# A failed write is reported, never swallowed. The state directory is made unwritable, so the temp
# file src/uistore.rs renames into place cannot be created at all, and the panel's next change is a
# change the file does not have. The user is told that in the one place Flea says things.
settings_write_refused() {
    local state="$1" pinned_base="$2" before refused_base retried_base
    before=$(cat "$state/flea/ui.json")
    chmod 500 "$state/flea" || fail "settings: the state directory could not be made read-only"
    settings_open_key
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
    settings_open_key
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "settings: the comma key did not open the panel"
    [[ "$(ipc settingsSection)" == "view" ]] \
        || fail "settings: the panel did not open on View, it is on $(ipc settingsSection)"
    shot settings-view-initial
    key -k Escape >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "false" ]] || fail "settings: Escape did not close the panel"

    local wx wy ww wh bx by
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    read -r bx by <<< "$(ipc chromeButtonCentre sliders)"
    [[ -n "$by" ]] || fail "settings: the chrome strip has no sliders button"
    omarchy-drive click "$((wx + bx))" "$((wy + by))" left >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "settings: the sliders button did not open the panel"
    # The Twitter report: a click on the card fell through to the dimmed ground and closed the panel,
    # and the next click landed on the listing. The title has no control on it, so it is the plainest spot.
    local tx ty
    read -r tx ty <<< "$(ipc settingsTitleCentre)"
    [[ -n "$ty" ]] || fail "settings: the panel has no title to click"
    omarchy-drive click "$((wx + tx))" "$((wy + ty))" left >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "settings: a click on the card's own title closed the panel"
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
# The title's centre with the Display section up: GM's ruling is that the card keeps one place and
# one height whichever section is up, so the two other sections are read against this.
settings_title_on_display=""

settings_display() {
    settings_open_key
    settle
    settings_section display
    settings_title_on_display=$(ipc settingsTitleCentre)
    [[ -n "$settings_title_on_display" ]] || fail "settings: the panel has no title to measure"
    local omarchy_base
    omarchy_base=$(token_of baseSize)
    [[ "$(ipc settingsRows)" == *"choice|Text size|Follow Omarchy"* ]] \
        || fail "settings: Display did not open on Follow Omarchy, got $(ipc settingsRows)"
    [[ "$(ipc settingsRows)" == *"fact|Effective|${omarchy_base} px"* ]] \
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
    [[ "$(ipc settingsRows)" == *"fact|Effective|${omarchy_base} px"* ]] \
        || fail "settings: the override did not start on Omarchy's own stop"
    [[ "$(ipc metrics)" == "$before" ]] \
        || fail "settings: switching to Override moved the type before any step, $before then $(ipc metrics)"

    # Down onto the stop row, then the whole list, each stop checked against the board's table.
    key j >/dev/null
    settle
    settings_walk_to_stop 9
    # Against the smallest stop, not against Omarchy's own size: the stops top out at 20, so a box
    # whose base-size is already 20 can never grow past itself and the check was unsatisfiable there.
    local smallest
    smallest=$(ipc metrics | cut -d' ' -f1)
    local stop
    for stop in 9 10 11 12 14 16 20; do
        settings_walk_to_stop "$stop"
        assert_board_row "$stop"
    done
    after=$(ipc metrics | cut -d' ' -f1)
    (( after > smallest )) \
        || fail "settings: the largest stop did not grow the type past the smallest, $smallest then $after"
    shot settings-text-override

    # The way back is one row, and it puts every token where Omarchy had it.
    key k >/dev/null
    settle
    key -k Return >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"choice|Text size|Follow Omarchy"* ]] \
        || fail "settings: the mode row did not go back to Follow Omarchy"
    [[ "$(ipc settingsRows)" == *"fact|Effective|${omarchy_base} px"* ]] \
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
    local omarchy_base="$1" start="$1"
    # The stops top out at 20, so a box whose own base-size is already there has no room to grow and
    # the chord has nothing it can prove. Step down first, and the pair is measured from that stop.
    if (( omarchy_base >= 20 )); then
        key -M ctrl -M shift -k minus -m shift -m ctrl >/dev/null
        settle
        start=$(token_of baseSize)
        (( start < omarchy_base )) \
            || fail "settings: Ctrl+Shift+Minus did not step down from $omarchy_base"
    fi
    key -M ctrl -M shift -k equal -m shift -m ctrl >/dev/null
    settle
    local grown
    grown=$(token_of baseSize)
    (( grown > start )) \
        || fail "settings: Ctrl+Shift+Plus did not grow the text size, still $grown"
    [[ "$(ipc lastMessage)" == "Text size ${grown}px. Ctrl+Shift+0 follows Omarchy again." ]] \
        || fail "settings: the chord did not announce its stop, got $(ipc lastMessage)"
    settings_open_key
    settle
    [[ "$(ipc settingsRows)" == *"fact|Effective|${grown} px"* ]] \
        || fail "settings: the panel does not show the stop the chord set, got $(ipc settingsRows)"
    key -k Escape >/dev/null
    settle
    key -M ctrl -M shift -k minus -m shift -m ctrl >/dev/null
    settle
    [[ "$(token_of baseSize)" == "$start" ]] \
        || fail "settings: Ctrl+Shift+Minus did not step back one stop to $start"
    key -M ctrl -M shift -k 0 -m shift -m ctrl >/dev/null
    settle
    [[ "$(ipc lastMessage)" == "Text size follows Omarchy, ${omarchy_base}px." ]] \
        || fail "settings: Ctrl+Shift+0 did not announce following Omarchy, got $(ipc lastMessage)"
    settings_open_key
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
# Mark geometry retains the board's fractional bodySmall * 1.45 size.
assert_board_row() {
    local want_base="$1" row got
    for row in "9|8|7|5|24|14|11.60" "10|9|8|5|26|16|13.05" "11|10|9|6|30|18|14.50" \
               "12|11|10|6|32|20|15.95" "14|13|12|7|37|23|18.85" "16|15|13|8|43|27|21.75" \
               "20|18|17|10|52|32|26.10"; do
        IFS='|' read -r base body caption padding height icon mark <<< "$row"
        [[ "$base" == "$want_base" ]] || continue
        got="$(token_of baseSize)|$(token_of bodySmall)|$(token_of caption)|$(token_of rowPaddingY)|$(token_of rowHeight)|$(token_of iconSize)|$(LC_NUMERIC=C printf '%.2f' "$(token_of markSize)")"
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
    settings_open_key
    settle
    settings_section menus
    [[ "$(ipc settingsTitleCentre)" == "$settings_title_on_display" ]] \
        || fail "settings: the card moved when Menus came up, title at $(ipc settingsTitleCentre) against $settings_title_on_display"
    [[ "$(ipc settingsRows)" == *"group|Basic file actions|6 of 6"* ]] \
        || fail "settings: the master row does not start at six of six, got $(ipc settingsRows)"
    shot settings-menus

    # Down three from the master is Paste, and Space is the board's own toggle key.
    key j >/dev/null; key j >/dev/null; key j >/dev/null
    settle
    key -k Space >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"group|Basic file actions|5 of 6"* ]] \
        || fail "settings: switching one action off did not read as five of six"
    key -k Escape >/dev/null
    settle
    settings_menu_lacks "Paste"
    [[ "|$(ipc contextMenuEntries)|" == *"|Cut|Copy|Duplicate|"* ]] \
        || fail "settings: hiding Paste moved the rows around it, got $(ipc contextMenuEntries)"
    key -k Escape >/dev/null
    settle

    # The master itself: a partial one enables all six, and a checked one switches all six off.
    settings_open_key
    settle
    key -k Space >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"group|Basic file actions|6 of 6"* ]] \
        || fail "settings: activating the partial master did not switch all six on"
    key -k Space >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"group|Basic file actions|0 of 6"* ]] \
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
    settings_open_key
    settle
    key -k Space >/dev/null
    settle
    key j >/dev/null; key j >/dev/null; key j >/dev/null
    key -k Space >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"group|Basic file actions|5 of 6"* ]] \
        || fail "settings: the panel did not end the Menus block with Paste alone switched off"
    key -k Escape >/dev/null
    settle
}

# Section selection survives a close; walk the rail through real keys. The rail's own length is read
# here rather than written down: case_settings above is where the inventory itself is asserted, and a
# count in two places is a count that goes stale in one of them, which is how it did.
settings_section() {
    local want="$1" sections down count step
    sections=$(ipc settingsSections)
    down=$(printf '%s' "$sections" | jq -r --arg id "$want" 'map(.id) | index($id) // empty')
    count=$(printf '%s' "$sections" | jq 'length')
    [[ "$down" =~ ^[0-9]+$ ]] || fail "settings: the rail has no $want section, it carries $sections"
    if [[ "$(ipc settingsSide)" != "rail" ]]; then key -k Tab >/dev/null; settle; fi
    [[ "$(ipc settingsSide)" == "rail" ]] || fail "settings: Tab did not give the cursor to the rail"
    for (( step = 1; step < count; step++ )); do key k >/dev/null; done
    settle
    [[ "$(ipc settingsSection)" == "view" ]] || fail "settings: rail did not reach View"
    for (( step = 0; step < down; step++ )); do key j >/dev/null; settle; done
    [[ "$(ipc settingsSection)" == "$want" ]] || fail "settings: rail did not reach $want"
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

# The Default/Windows toggle, proved by the keys themselves: a chord one preset binds and the other
# does not, driven through the real window in both states.
settings_keys() {
    settings_open_key
    settle
    settings_section keys
    [[ "$(ipc settingsTitleCentre)" == "$settings_title_on_display" ]] \
        || fail "settings: the card moved when Keys came up, title at $(ipc settingsTitleCentre) against $settings_title_on_display"
    # The shipped preset is "default", which ui/js/Settings.js labels Default and PRESET_KEYS gives
    # ctrl-1 to ctrl-3; a window that starts anywhere else is not the one this checks the toggle on.
    [[ "$(ipc settingsRows)" == *"choice|Keybinding preset|Default"* ]] \
        || fail "settings: the preset row does not start on Default, got $(ipc settingsRows)"
    ipc settingsModel | jq -e 'map(select(.id == "keyPreview"))[0].items | length == 6' >/dev/null \
        || fail "settings: Default must show six binding examples"
    shot settings-keys
    # PRESETS is default, vim, mac, windows, so Windows is three steps along and not one.
    key l >/dev/null
    key l >/dev/null
    key l >/dev/null
    settle
    [[ "$(ipc settingsRows)" == *"choice|Keybinding preset|Windows"* ]] \
        || fail "settings: three steps of l did not reach Windows"
    ipc settingsModel | jq -e 'map(select(.id == "keyPreview"))[0].items | length == 6 and any(.[]; .keys == "ctrl-c" and .label == "copy")' >/dev/null \
        || fail "settings: Windows binding examples must include its live copy chord"
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

# rows_run_under "<x y w h>" <label>: rows other than the parked row 0 lie under the whole card, so a press that runs on moves the cursor.
rows_run_under() {
    local card="$1" label="$2" cx cy cw ch visible last first second
    [[ "$card" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+$ ]] || fail "clickthrough: no card rect for $label, ipc answered [$card]"
    read -r cx cy cw ch <<< "$card"
    visible=$(ipc visibleRows)
    [[ "$visible" =~ ^[1-9][0-9]*$ ]] || fail "clickthrough: no visible row count for $label, ipc answered [$visible]"
    last=$(ipc rowCentre $(( visible - 1 )))
    first=$(ipc rowCentre 0)
    second=$(ipc rowCentre 1)
    [[ "$last" =~ ^[0-9]+\ [0-9]+$ && "$first" =~ ^[0-9]+\ [0-9]+$ && "$second" =~ ^[0-9]+\ [0-9]+$ ]] \
        || fail "clickthrough: no row centres for $label (row $(( visible - 1 )) [$last], row 0 [$first], row 1 [$second])"
    [[ "${last#* }" -gt $(( cy + ch )) ]] || fail "clickthrough: the list does not run under $label (last visible row centre y ${last#* }, card bottom $(( cy + ch )))"
    # The seam between rows 0 and 1 is the bottom of the parked row, and it must clear the card's top edge.
    [[ $(( (${first#* } + ${second#* }) / 2 )) -lt "$cy" ]] \
        || fail "clickthrough: the parked row 0 reaches under $label (rows 0 and 1 centres y ${first#* } ${second#* }, card top $cy)"
}

# A folder named at launch goes to the side of a saved dual view that had focus, and the other side
# keeps its saved folder; before 0.3.3 the saved pair won and the named folder was dropped.
case_duallaunch() {
    local dir="$fixture_root/duallaunch" state="$fixture_root/duallaunch-state" side dual want_left want_right
    sandbox_scratch "$dir"
    mkdir -p "$dir/left" "$dir/right" "$dir/asked"
    : > "$dir/left/l.txt"; : > "$dir/right/r.txt"; : > "$dir/asked/a.txt"
    for side in 0 1; do
        seed_ui_state "$state-$side" "$(jq -cn --arg l "$dir/left" --arg r "$dir/right" --argjson f "$side" \
            '{view:"dual",dual:{paths:[$l,$r],focus:$f}}')"
        launch "$dir/asked"
        want_left="$dir/asked"; want_right="$dir/right"
        [[ "$side" == 1 ]] && { want_left="$dir/left"; want_right="$dir/asked"; }
        for _attempt in $(seq 1 100); do
            dual=$(ipc dualState)
            [[ "$(jq -r '[.panes[].loading] | any' <<< "$dual")" == false && "$(jq -r '.panes[1].path' <<< "$dual")" != null ]] && break
            sleep 0.05
        done
        [[ "$(jq -r '.active' <<< "$dual")" == true ]] || fail "duallaunch: the saved dual view did not open: $dual"
        [[ "$(jq -r '.panes[0].path' <<< "$dual")" == "$want_left" && "$(jq -r '.panes[1].path' <<< "$dual")" == "$want_right" ]] \
            || fail "duallaunch: focus $side opened $(jq -c '[.panes[].path]' <<< "$dual"), wanted $want_left and $want_right"
        [[ "$(jq -r '.focused' <<< "$dual")" == "$side" ]] || fail "duallaunch: focus moved from side $side: $dual"
        printf 'DUALLAUNCH focus=%s left=%s right=%s\n' "$side" "$(jq -r '.panes[0].path' <<< "$dual")" "$(jq -r '.panes[1].path' <<< "$dual")"
        kill_flea
    done
}

# The cursor parks on row 0 above the card, so a press that runs on from an overlay control to any row beneath moves it.
case_clickthrough() {
    local dir="$fixture_root/clickthrough"
    sandbox_scratch "$dir"
    local i
    # Eighty rows overflow the window at every density; forty stopped short of a compact screen's 43.
    for i in $(seq -w 1 80); do : > "$dir/f$i.txt"; done
    launch "$dir"
    wait_listing 80
    local parked; parked=$(ipc cursor)
    [[ "$parked" == "0" ]] || fail "clickthrough: the cursor did not start on row 0, it is on $parked"

    key -k Tab >/dev/null
    settle
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "clickthrough: the rail's a key did not open the network dialog"
    local p
    for p in SFTP FTPS WebDAV NFS SMB; do
        rows_run_under "$(ipc networkCardRect)" "the network card"
        click_chip "$p"
        settle
        [[ "$(ipc networkProtocol)" == "$p" ]] || fail "clickthrough: the $p chip did not take its click, protocol is $(ipc networkProtocol)"
        [[ "$(ipc cursor)" == "$parked" && "$(ipc path)" == "$dir" ]] \
            || fail "clickthrough: the $p chip click reached the pane beneath, cursor $(ipc cursor), path $(ipc path)"
    done
    key -k Escape >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "false" ]] || fail "clickthrough: Escape did not close the network dialog"
    key -k Escape >/dev/null
    settle

    key , >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "clickthrough: the comma key did not open settings"
    local section centre cx cy wx wy
    for section in keys menus display; do
        rows_run_under "$(ipc settingsCardRect)" "the settings card"
        centre=$(ipc settingsRailRowCentre "$section")
        [[ -n "$centre" ]] || fail "clickthrough: the settings rail has no $section row"
        read -r cx cy <<< "$centre"
        read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
        omarchy-drive click "$((cx + wx))" "$((cy + wy))" >/dev/null
        settle
        [[ "$(ipc settingsSection)" == "$section" ]] || fail "clickthrough: the $section rail row did not take its click, section is $(ipc settingsSection)"
        [[ "$(ipc cursor)" == "$parked" && "$(ipc path)" == "$dir" ]] \
            || fail "clickthrough: the $section rail row click reached the pane beneath, cursor $(ipc cursor), path $(ipc path)"
    done
    key -k Escape >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "false" ]] || fail "clickthrough: Escape did not close settings"
    printf 'CLICKTHROUGH chips=ok rail=ok cursor=%s\n' "$parked"
    kill_flea
}

# The wheel over an open overlay stays with the overlay: the listing beneath a menu, a card or a
# sheet never scrolls. Each ground swallows the wheel the way it swallows a click (ui/ContextMenu.qml
# and the four card files), so the list's contentY is the witness.
case_wheelunder() {
    local dir="$fixture_root/wheelunder"
    sandbox_scratch "$dir"
    local i
    for i in $(seq -w 1 80); do : > "$dir/f$i.txt"; done
    launch "$dir"
    wait_listing 80
    local wx wy ww wh cx cy
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    read -r cx cy <<< "$(ipc rowCentre 5)"
    omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null
    # The control: with nothing open the same wheel moves the list, so a still list below is not a lost wheel.
    omarchy-drive scroll down 3 >/dev/null
    settle
    [[ "$(ipc listContentY)" != "0" ]] || fail "wheelunder: three notches with nothing open left contentY at 0, so the wheel is not reaching the list"
    key -k Home >/dev/null
    settle
    [[ "$(ipc listContentY)" == "0" ]] || fail "wheelunder: Home did not bring contentY back to 0, it is $(ipc listContentY)"

    wheel_holds() {
        local what="$1" reader="$2"
        omarchy-drive scroll down 3 >/dev/null
        settle
        [[ "$(ipc listContentY)" == "0" ]] || fail "wheelunder: three notches under $what scrolled the listing to contentY $(ipc listContentY)"
        [[ "$(ipc "$reader")" == "true" ]] || fail "wheelunder: the wheel closed $what"
    }
    # 80 rows leave no ground, so the menu is opened on row 5 itself: the same overlay, and the wheel under it is the question.
    read -r cx cy <<< "$(ipc rowCentre 5)"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" right >/dev/null
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "wheelunder: the right click on row 5 opened no menu"
    wheel_holds "the context menu" contextMenuVisible
    key -k Escape >/dev/null
    settle
    key , >/dev/null
    settle
    [[ "$(ipc settingsOpen)" == "true" ]] || fail "wheelunder: the comma key did not open settings"
    wheel_holds "the settings card" settingsOpen
    key -k Escape >/dev/null
    settle
    key '?' >/dev/null
    settle
    [[ "$(ipc keymapSheetOpen)" == "true" ]] || fail "wheelunder: the ? key did not open the keymap sheet"
    wheel_holds "the keymap sheet" keymapSheetOpen
    key -k Escape >/dev/null
    settle
    key -k Tab >/dev/null
    settle
    key a >/dev/null
    settle
    [[ "$(ipc dialogOpen)" == "true" ]] || fail "wheelunder: the rail's a key did not open the network dialog"
    wheel_holds "the network dialog" dialogOpen
    key -k Escape >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    printf 'WHEELUNDER menu=ok settings=ok keymap=ok network=ok\n'
    kill_flea
}

# A pointer warp sends no motion to Qt (hyprland cursor.move carries no wl_pointer frame), so hover needs the one uinput pixel the scroll case uses.
hover_row() {
    local cx cy wx wy
    read -r cx cy <<< "$(ipc rowCentre "$1")"
    [[ -n "$cy" ]] || fail "hover_row: row $1 has no centre"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1
}

# ctrl-1 list, ctrl-2 columns, ctrl-3 grid, per keys.toml; the reader proves the switch landed.
switch_view() {
    local want="$1" chord drew
    case "$want" in list) chord=1 ;; columns) chord=2 ;; grid) chord=3 ;; esac
    key -M ctrl -k "$chord" -m ctrl >/dev/null
    settle
    drew=$(ipc viewMode)
    [[ "$drew" == "$want" ]] || fail "switch_view: ctrl-$chord left the view on '$drew', not $want"
}

# rect_is "x y w h" ex ey ew eh tol: every edge of the box within tol pixels of the expected one.
rect_is() {
    local x y w h d
    read -r x y w h <<< "$1"
    [[ -n "$h" ]] || return 1
    for d in $((x - $2)) $((y - $3)) $((w - $4)) $((h - $5)); do
        (( ${d#-} <= $6 )) || return 1
    done
}

# Presses row $1 beside the open menu: in columns the menu covers the row's centre, and a press there picks a menu row.
click_row_beside_menu() {
    local rx ry rw rh mx my mw mh inset px py wx wy
    read -r rx ry rw rh <<< "$(ipc rowRect "$1")"
    [[ -n "$rh" ]] || fail "click_row_beside_menu: row $1 has no box"
    read -r mx my mw mh <<< "$(ipc contextMenuRect)"
    [[ -n "$mh" ]] || fail "click_row_beside_menu: no open menu to press beside"
    # Sample input: ipc metrics "13 12 14 37", the body and caption sizes, rowPaddingX, then the board's rowHeight.
    inset=$(ipc metrics | cut -d' ' -f3)
    [[ "$inset" =~ ^[0-9]+$ ]] || fail "click_row_beside_menu: ipc metrics gave no rowPaddingX, got [$inset]"
    px=$((rx + inset)) py=$((ry + rh / 2))
    (( px < rx + rw )) || fail "click_row_beside_menu: a press $inset px into row $1 falls past its $rw px width"
    (( px < mx || px > mx + mw || py < my || py > my + mh )) \
        || fail "click_row_beside_menu: the press at $px,$py on row $1 is under the menu at $mx $my $mw $mh"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((wx + px))" "$((wy + py))" "$2" >/dev/null
}

# A click at row i's height just right of the settings card: on the ground, and inside the row in every view (the middle column runs 200 px past the card).
click_row_edge() {
    local rx ry rw rh cx cy cw ch wx wy
    read -r rx ry rw rh <<< "$(ipc rowRect "$1")"
    [[ -n "$rh" ]] || fail "click_row_edge: row $1 has no box"
    read -r cx cy cw ch <<< "$(ipc settingsCardRect)"
    [[ -n "$ch" ]] || fail "click_row_edge: no settings card to click beside"
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((wx + cx + cw + 20))" "$((wy + ry + rh / 2))" "$2" >/dev/null
}

# Lit pixels inside a window-relative "x y w h", the count every painted-mark check reads.
lit_in_rect() {
    local png="$1"
    shift
    [[ -n "${4:-}" ]] || { echo 0; return; }
    count_pixels "$png" "${3}x${4}+${1}+${2}" "((r+g+b)/3) > 0.25"
}

# The media fixture's rows and a shared fresh layout for the two per-view cases below.
views_fixture() {
    local dir="$1" media="$FIXTURE_ROOT/flea-media-btrfs" i
    [[ -d "$media" ]] || fail "the media fixture is missing at $media"
    sandbox_scratch "$dir"
    mkdir -p "$dir/empty" "$dir/sub"
    : > "$dir/sub/s1.txt"; : > "$dir/sub/s2.txt"; : > "$dir/sub/s3.txt"
    cp "$media/clip_1006.mp4" "$dir/a-clip.mp4"
    cp "$media/clip_1007.mp4" "$dir/b-clip.mp4"
    cp "$(ls "$media"/*.png | head -1)" "$dir/c-pic.png"
    cp "$(ls "$media"/*.jpg | head -1)" "$dir/d-pic.jpg"
    # 400, because the grid holds about 200 tiles a screen and the wheel control needs a second one.
    for i in $(seq -w 1 400); do : > "$dir/t$i.txt"; done
}

# GM's manual test of 0.1.6 found every one of these in a view the suite never drove: rows lifting
# and scrolling under an open menu or card, a click outside the menu selecting the row beneath, and
# a right click on a card running on to the row under it. Every check runs in all three views.
case_overlays() {
    local dir="$fixture_root/overlays" mode n cx cy wx wy
    views_fixture "$dir"
    launch "$dir"
    wait_listing 406
    for mode in grid list columns; do
        switch_view "$mode"
        key -k Home >/dev/null
        settle
        n=$(( $(ipc visibleRows) - 3 ))
        [[ -n "$(ipc rowRect "$n")" ]] || fail "$mode: row $n has no box, the reader answers nothing here"
        hover_row "$n"
        settle
        [[ "$(ipc rowHovered "$n")" == "true" ]] || fail "$mode: hover over row $n with nothing open did not lift it, so the checks below prove nothing"
        omarchy-drive scroll down 3 >/dev/null
        settle
        [[ "$(ipc viewContentY)" != "0" ]] || fail "$mode: three notches with nothing open left the view at 0, so the wheel checks below prove nothing"
        key -k Home >/dev/null
        settle
        [[ "$(ipc viewContentY)" == "0" ]] || fail "$mode: Home did not bring the view back to 0, it is $(ipc viewContentY)"
        click_row 2 right
        settle
        [[ "$(ipc contextMenuVisible)" == "true" && "$(ipc cursor)" == "2" ]] || fail "$mode: a right click on row 2 opened no menu (visible $(ipc contextMenuVisible), cursor $(ipc cursor))"
        hover_row "$n"
        settle
        [[ "$(ipc rowHovered "$n")" == "false" ]] || fail "$mode: row $n lifted under the open menu"
        omarchy-drive scroll down 3 >/dev/null
        settle
        [[ "$(ipc viewContentY)" == "0" && "$(ipc contextMenuVisible)" == "true" ]] || fail "$mode: the wheel under the menu moved the view to $(ipc viewContentY) (menu $(ipc contextMenuVisible))"
        click_row_beside_menu "$n" left
        settle
        [[ "$(ipc contextMenuVisible)" == "false" && "$(ipc cursor)" == "2" ]] || fail "$mode: the click outside the menu left it $(ipc contextMenuVisible) and moved the cursor to $(ipc cursor)"
        key -k Home >/dev/null
        settle
        printf 'OVERLAYS_DIAG %s before view=%s focus=%s settings=%s menu=%s path=%s\n' \
            "$mode" "$(ipc viewMode)" "$(ipc focusView)" "$(ipc settingsOpen)" "$(ipc contextMenuVisible)" "$(ipc path)"
        key , >/dev/null
        settle
        printf 'OVERLAYS_DIAG %s after settings=%s focus=%s\n' "$mode" "$(ipc settingsOpen)" "$(ipc focusView)"
        [[ "$(ipc settingsOpen)" == "true" ]] || fail "$mode: the comma key did not open settings"
        hover_row "$n"
        settle
        [[ "$(ipc rowHovered "$n")" == "false" ]] || fail "$mode: row $n lifted under the settings card"
        omarchy-drive scroll down 3 >/dev/null
        settle
        [[ "$(ipc viewContentY)" == "0" && "$(ipc settingsOpen)" == "true" ]] || fail "$mode: the wheel under settings moved the view to $(ipc viewContentY)"
        read -r cx cy _cw _ch <<< "$(ipc settingsCardRect)"
        read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
        omarchy-drive click "$((wx + cx + 40))" "$((wy + cy + 40))" right >/dev/null
        settle
        [[ "$(ipc settingsOpen)" == "true" && "$(ipc contextMenuVisible)" == "false" && "$(ipc cursor)" == "0" ]] \
            || fail "$mode: a right click on the card body: settings $(ipc settingsOpen), menu $(ipc contextMenuVisible), cursor $(ipc cursor)"
        click_row_edge "$n" right
        settle
        [[ "$(ipc settingsOpen)" == "false" && "$(ipc contextMenuVisible)" == "false" && "$(ipc cursor)" == "0" ]] \
            || fail "$mode: a right click on the ground: settings $(ipc settingsOpen), menu $(ipc contextMenuVisible), cursor $(ipc cursor)"
        key , >/dev/null
        settle
        click_row_edge "$n" left
        settle
        [[ "$(ipc settingsOpen)" == "false" && "$(ipc cursor)" == "0" ]] || fail "$mode: a left click on the ground left settings $(ipc settingsOpen) and the cursor on $(ipc cursor)"
        printf 'OVERLAYS %s menu=ok settings=ok\n' "$mode"
    done
    kill_flea
}

# The same manual test: thumbnails, the empty hero, the peeked column's menu and video playback, per
# view. Each view enters its own uncached copy of the fixture through the UI, so it acquires its own
# thumbnails rather than reading ones another view warmed.
case_views() {
    # Four fixtures directly inside the sandbox root, which is what its guard allows; the fourth is the
    # grid again after the columns view, so a column kept alive under the grid is proven to plan nothing.
    local root="$fixture_root" pass mode dir r lit fx fy fw fh sx sy sw sh cx cy wx wy p1 p2 p3 p4 changed before i
    for pass in grid list columns again; do views_fixture "$root/views-$pass"; done
    # 44 fillers sort ahead of the fixtures, as the battery's own case directories do, so the seek has to walk past its old 40-row cap.
    for i in $(seq -w 1 44); do mkdir -p "$root/a-filler-$i"; done
    launch "$root"
    wait_listing "$(ls "$root" | wc -l)"
    for pass in grid list columns again; do
        dir="$root/views-$pass"
        mode=$pass
        [[ "$pass" == "again" ]] && mode=grid
        # Sought in the list, where j walks one row; the grid's j walks a tile row. The target view is on before Return.
        switch_view list
        seek_row_named "views-$pass"
        switch_view "$mode"
        key -k Return >/dev/null
        wait_listing 406
        [[ "$(ipc path)" == "$dir" ]] || fail "$mode: Return on the $mode row opened $(ipc path)"
        key -k Home >/dev/null
        settle
        sleep 2
        # $pass, not $mode: the "again" pass runs the grid a second time and shot keeps every capture.
        shot "views-$pass-thumbs"
        # Directories sort first, so the fixture's four media files are rows 2 to 5 in this order.
        local names=(a-clip.mp4 b-clip.mp4 c-pic.png d-pic.jpg)
        for r in 2 3 4 5; do
            [[ "$(ipc rowAt "$r")" == "${names[r - 2]}|"* ]] || fail "$mode: row $r is $(ipc rowAt "$r" | cut -d'|' -f1), not ${names[r - 2]}"
            [[ "$(ipc rowThumbReady "$r")" == "true" ]] || fail "$mode: row $r ($(ipc rowAt "$r" | cut -d'|' -f1)) has no decoded thumbnail"
            lit=$(lit_in_rect "$evidence_dir/views-$pass-thumbs.png" $(ipc rowThumbRect "$r"))
            (( lit > 30 )) || fail "$mode: row $r's thumbnail box painted $lit lit pixels"
        done
        if [[ "$mode" == "columns" ]]; then
            # The child column's hero: polled across one draw, the way case_background polls the pane's own.
            lit=0
            # Each attempt keeps its own capture: shot refuses to overwrite evidence, so one name
            # across the poll failed the case the moment the first draw came up blank.
            for _attempt in $(seq 1 "$mark_poll_shots"); do
                shot "views-child-hero-$_attempt"
                lit=$(lit_in_rect "$evidence_dir/views-child-hero-$_attempt.png" $(ipc columnChildMarkRect))
                (( lit > 0 )) && break
                sleep "$mark_poll_s"
            done
            (( lit > 0 )) || fail "columns: the child column's hero painted nothing (state $(ipc columnChildEmpty))"
            key j >/dev/null
            settle
            read -r fx fy <<< "$(ipc columnChildRowCentre 1)"
            [[ -n "$fy" ]] || fail "columns: the child column shows no row 1 for sub"
            read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
            omarchy-drive click "$((wx + fx))" "$((wy + fy))" right >/dev/null
            sleep 1
            [[ "$(ipc path)" == "$dir/sub" && "$(ipc rowAt "$(ipc cursor)")" == s2.txt\|* && "$(ipc contextMenuVisible)" == "true" ]] \
                || fail "columns: a right click on a peeked row: path $(ipc path), cursor row $(ipc rowAt "$(ipc cursor)" | cut -d'|' -f1), menu $(ipc contextMenuVisible)"
            key -k Escape >/dev/null
            settle
            key -k Backspace >/dev/null
            sleep 1
            [[ "$(ipc path)" == "$dir" ]] || fail "columns: Backspace did not return to the fixture, path $(ipc path)"
            seek_row_named "c-pic.png"
            # Ready, not shown: thumbShown is true while the picture still loads. Polled the way the thumbnail rows are.
            for _attempt in $(seq 1 30); do
                [[ "$(ipc columnFrameReady)" == "true" ]] && break
                sleep 0.1
            done
            [[ "$(ipc previewColumnState)" == "image" && "$(ipc columnFrameReady)" == "true" ]] || fail "columns: the cursor on c-pic.png shows $(ipc previewColumnState), frame ready $(ipc columnFrameReady), thumb shown $(ipc columnThumbShown)"
            shot views-columns-frame
            # Inset past the border and the hairline, so the frame's own outline cannot light the count.
            read -r fx fy fw fh <<< "$(ipc columnFrameRect)"
            lit=$(lit_in_rect "$evidence_dir/views-columns-frame.png" "$((fx + 6))" "$((fy + 6))" "$((fw - 12))" "$((fh - 12))")
            (( lit > 200 )) || fail "columns: the preview frame's interior painted $lit lit pixels for c-pic.png"
            seek_row_named "a-clip.mp4"
            sleep 1
            [[ "$(ipc previewColumnState)" == "video" ]] || fail "columns: the cursor on a-clip.mp4 shows $(ipc previewColumnState)"
            read -r fx fy fw fh <<< "$(ipc columnFrameRect)"
            (( fw > 0 && fh > 0 )) || fail "columns: the frame has no box"
            read -r cx cy <<< "$(ipc columnPlayCentre)"
            omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
            sleep 1.2
            p1=$(ipc columnMediaPosition)
            shot views-video-1
            sleep 1.5
            p2=$(ipc columnMediaPosition)
            shot views-video-2
            [[ "$(ipc columnMediaPlaying)" == "true" && "$(ipc columnPlayerLoaded)" == "true" ]] || fail "columns: play left playing $(ipc columnMediaPlaying), player $(ipc columnPlayerLoaded)"
            (( p2 > p1 )) || fail "columns: the position did not advance, $p1 then $p2"
            # The fixture clip is colour bars with a moving line about 750 px long, so a moved line differs by thousands of pixels and a frozen frame by none.
            changed=$(magick \( "$evidence_dir/views-video-1.png" -crop "${fw}x${fh}+${fx}+${fy}" +repage \) \
                \( "$evidence_dir/views-video-2.png" -crop "${fw}x${fh}+${fx}+${fy}" +repage \) \
                -compose difference -composite -threshold 10% -format "%[fx:int(mean*w*h+0.5)]" info:)
            (( changed > 1000 )) || fail "columns: the video frame changed $changed pixels in 1.5 s of playback"
            omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
            sleep 0.8
            p3=$(ipc columnMediaPosition)
            sleep 1
            p4=$(ipc columnMediaPosition)
            [[ "$(ipc columnMediaPlaying)" == "false" && "$p3" == "$p4" ]] || fail "columns: pause left playing $(ipc columnMediaPlaying), position $p3 then $p4"
            # Select all keeps the cursor and the path, so the only thing that can end the player is the strip going away.
            before="$(ipc cursor)|$(ipc path)"
            key -M ctrl -k a -m ctrl >/dev/null
            settle
            [[ "$(ipc cursor)|$(ipc path)" == "$before" ]] || fail "columns: select all moved the cursor or the path, $before to $(ipc cursor)|$(ipc path)"
            [[ "$(ipc previewColumnState)" == "multi" && "$(ipc columnPlayerLoaded)" == "false" ]] || fail "columns: a multi-selection left the player $(ipc columnPlayerLoaded) in state $(ipc previewColumnState)"
            # ui/js/Focus.js unwinds Escape in order: a standing status message goes before the
            # marks do, so the selection ends on the press after the one that clears the bar.
            local _press
            for _press in 1 2 3; do
                [[ "$(ipc selectionCount)" == "0" ]] && break
                key -k Escape >/dev/null
                settle
            done
            [[ "$(ipc selectionCount)" == "0" ]] || fail "columns: Escape never ended the selection, $(ipc selectionCount) rows still marked"
            [[ "$(ipc previewColumnState)" == "video" && "$(ipc columnPlayerLoaded)" == "false" ]] || fail "columns: back on the video with $(ipc columnPlayerLoaded) player, state $(ipc previewColumnState)"
            omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
            sleep 1
            key j >/dev/null
            sleep 1
            [[ "$(ipc columnPlayerLoaded)" == "false" ]] || fail "columns: moving the cursor away left a player behind"
            printf 'VIEWS columns hero=%s play=%s..%s changed=%s\n' "$lit" "$p1" "$p2" "$changed"
        fi
        key -k Home >/dev/null
        settle
        key -k Return >/dev/null
        settle
        [[ "$(ipc path)" == "$dir/empty" && "$(ipc emptyShown)" == "true" ]] || fail "$mode: Return on row 0 did not enter the empty directory ($(ipc path), empty $(ipc emptyShown))"
        lit=0
        for _attempt in $(seq 1 "$mark_poll_shots"); do
            shot "views-$pass-empty-$_attempt"
            lit=$(lit_in_rect "$evidence_dir/views-$pass-empty-$_attempt.png" $(ipc emptyMarkRect))
            (( lit > 0 )) && break
            sleep "$mark_poll_s"
        done
        (( lit > 0 )) || fail "$mode: the empty directory's hero painted nothing"
        # Its box is the listing slot exactly, and in the columns view every slot right of the
        # ancestor column: an empty active column has no peek beside it, so the hero centres in the
        # space there actually is. A lazy view's item reports a local origin, and the hero placed on
        # it once drew over the sidebar, still inside a wide slot.
        read -r sx sy sw sh <<< "$(ipc listAreaRect)"
        # The columns view draws three fixed slots and the pane's own listing is the middle one, so
        # the hero sits over that column alone: every view then puts the animation in the same place.
        local ex=$sx ew=$sw
        [[ "$mode" == "columns" ]] && { ex=$((sx + sw / 3)); ew=$((sw / 3)); }
        rect_is "$(ipc emptyStateRect)" "$ex" "$sy" "$ew" "$sh" 1 \
            || fail "$mode: the hero's box is $(ipc emptyStateRect), not the slot $ex $sy $ew $sh"
        read -r mx my mw mh <<< "$(ipc emptyMarkRect)"
        local want_centre=$((ex + ew / 2)) mark_centre=$((mx + mw / 2))
        (( mark_centre >= want_centre - 2 && mark_centre <= want_centre + 2 )) \
            || fail "$mode: the hero's mark centres at $mark_centre, not $want_centre"
        key -k Backspace >/dev/null
        sleep 1
        key -k Backspace >/dev/null
        sleep 1
        [[ "$(ipc path)" == "$root" ]] || fail "$mode: two Backspaces did not return to the root, path $(ipc path)"
        printf 'VIEWS %s thumbs=ok hero=%s\n' "$pass" "$lit"
    done
    kill_flea
}

# One row per format family the preview classifies, all in the columns view's own frame, judged on
# what the frame draws: decoded pixels, lines, member names, pages, an advancing position, the sentence.
formats_fixture() {
    local dir="$1" media="$FIXTURE_ROOT/flea-media-btrfs"
    [[ -d "$media" ]] || fail "the media fixture is missing at $media"
    sandbox_scratch "$dir"
    cp "$(ls "$media"/*.jpg | head -1)" "$dir/p.jpg"
    cp "$(ls "$media"/*.png | head -1)" "$dir/p.png"
    cp "$(ls "$media"/*.webp | head -1)" "$dir/p.webp"
    cp "$(ls "$media"/*.heic | head -1)" "$dir/p.heic"
    cp "$media/clip_1006.mp4" "$dir/v.mp4"
    cp "$(ls "$media"/*.mkv | head -1)" "$dir/v.mkv"
    cp "$(ls "$media"/*.webm | head -1)" "$dir/v.webm"
    python3 - "$dir/tone.wav" 15 <<'PYEOF'
import sys, wave, struct, math
sample_rate = 44100
seconds = int(sys.argv[2])
with wave.open(sys.argv[1], "w") as f:
    f.setnchannels(1)
    f.setsampwidth(2)
    f.setframerate(sample_rate)
    for i in range(sample_rate * seconds):
        f.writeframesraw(struct.pack("<h", int(16000 * math.sin(2 * math.pi * 440 * i / sample_rate))))
PYEOF
    magick \( -size 400x560 xc:white -fill black -draw "rectangle 40,40 120,80" \) \
           \( -size 400x560 xc:white -fill black -draw "rectangle 40,40 360,520" \) "$dir/manual.pdf"
    head -c 200 "$dir/manual.pdf" > "$dir/broken.pdf"
    printf 'hello from flea\nsecond line\n' > "$dir/sample.txt"
    printf '# Notes\n\nSome *text*.\n' > "$dir/notes.md"
    : > "$dir/empty.txt"
    head -c 1100000 /dev/zero | tr '\0' 'x' > "$dir/big.txt"
    printf 'fn main() {\n    println!("hi");\n}\n' > "$dir/main.rs"
    printf '{"a": 1}\n' > "$dir/data.json"
    ( cd "$dir" && bsdtar -a -cf a.zip sample.txt notes.md && bsdtar --zstd -cf b.tar.zst sample.txt main.rs )
    # A valid archive with no members: the end-of-central-directory record alone, 22 bytes.
    { printf 'PK\005\006'; head -c 18 /dev/zero; } > "$dir/empty.zip"
    head -c 4096 /dev/urandom > "$dir/corrupt.zip"
    ln -s "$dir/sample.txt" "$dir/link-file"
    mkdir -p "$dir/subdir"
    ln -s "$dir/subdir" "$dir/link-dir"
    ln -s "$dir/nowhere-at-all" "$dir/link-broken"
    cp "$dir/p.jpg" "$dir/shut.jpg"
    chmod 000 "$dir/shut.jpg"
    head -c 4096 /dev/urandom > "$dir/core.dump"
}

# Moves the cursor onto a row by name and waits for the column's state to settle on the family expected.
column_expect() {
    local name="$1" want="$2" _attempt
    seek_row_named "$name"
    for _attempt in $(seq 1 40); do
        [[ "$(ipc previewColumnState)" == "$want" ]] && return 0
        sleep 0.1
    done
    fail "formats: $name shows $(ipc previewColumnState), not $want (failure '$(ipc columnFailure)')"
}

# Prints one number, so the shot's own narration goes to stderr rather than into the caller's count.
column_frame_lit() {
    local fx fy fw fh
    read -r fx fy fw fh <<< "$(ipc columnFrameRect)"
    shot "formats-$1" >&2
    lit_in_rect "$evidence_dir/formats-$1.png" "$((fx + 6))" "$((fy + 6))" "$((fw - 12))" "$((fh - 12))"
}

case_formats() {
    local dir="$fixture_root/formats" name lit p1 p2 wx wy cx cy _attempt
    formats_fixture "$dir"
    launch "$dir"
    wait_listing 26
    switch_view columns
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    for name in p.jpg p.png p.webp p.heic; do
        column_expect "$name" image
        for _attempt in $(seq 1 40); do [[ "$(ipc columnFrameReady)" == "true" ]] && break; sleep 0.1; done
        [[ "$(ipc columnFrameReady)" == "true" ]] || fail "formats: $name never decoded in the frame"
        lit=$(column_frame_lit "$name")
        (( lit > 200 )) || fail "formats: $name's frame interior painted $lit lit pixels"
    done
    for name in v.mp4 v.mkv v.webm; do
        column_expect "$name" video
        for _attempt in $(seq 1 40); do [[ "$(ipc columnFrameReady)" == "true" ]] && break; sleep 0.1; done
        [[ "$(ipc columnFrameReady)" == "true" ]] || fail "formats: $name's first frame never decoded"
        read -r cx cy <<< "$(ipc columnPlayCentre)"
        omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
        sleep 1.2
        p1=$(ipc columnMediaPosition)
        sleep 1
        p2=$(ipc columnMediaPosition)
        [[ "$(ipc columnMediaPlaying)" == "true" ]] && (( p2 > p1 )) || fail "formats: $name did not play, playing $(ipc columnMediaPlaying), position $p1 then $p2"
    done
    column_expect tone.wav audio
    read -r cx cy <<< "$(ipc columnPlayCentre)"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    sleep 1.2
    p1=$(ipc columnMediaPosition)
    sleep 1
    p2=$(ipc columnMediaPosition)
    (( p2 > p1 )) || fail "formats: tone.wav did not play, position $p1 then $p2"
    column_expect manual.pdf pdf
    for _attempt in $(seq 1 40); do [[ "$(ipc columnPdfLoaded)" == "true" && "$(ipc columnPdfPages)" == "2" ]] && break; sleep 0.1; done
    [[ "$(ipc columnPdfPages)" == "2" && "$(ipc columnPdfPage)" == "0" ]] || fail "formats: manual.pdf shows $(ipc columnPdfPages) pages, page $(ipc columnPdfPage)"
    lit=$(column_frame_lit manual-p1)
    (( lit > 200 )) || fail "formats: manual.pdf's first page painted $lit lit pixels"
    read -r cx cy <<< "$(ipc columnChevronCentre right)"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    settle
    [[ "$(ipc columnPdfPage)" == "1" ]] || fail "formats: the right chevron left manual.pdf on page $(ipc columnPdfPage)"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    settle
    [[ "$(ipc columnPdfPage)" == "1" ]] || fail "formats: the right chevron went past the last page to $(ipc columnPdfPage)"
    column_expect broken.pdf error
    [[ "$(ipc columnFailure)" == "This file could not be read." ]] || fail "formats: broken.pdf's sentence is '$(ipc columnFailure)'"
    column_expect sample.txt text
    for _attempt in $(seq 1 40); do [[ "$(ipc columnTextLines)" == "hello from flea|"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnTextLines)" == "hello from flea|second line"* ]] || fail "formats: sample.txt's lines read '$(ipc columnTextLines)'"
    shot formats-sample-lines
    lit=$(lit_in_rect "$evidence_dir/formats-sample-lines.png" $(ipc columnLinesRect))
    (( lit > 50 )) || fail "formats: sample.txt's lines box painted $lit lit pixels"
    column_expect notes.md text
    for _attempt in $(seq 1 40); do [[ "$(ipc columnTextLines)" == "# Notes"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnTextLines)" == "# Notes"* ]] || fail "formats: notes.md's lines read '$(ipc columnTextLines)'"
    column_expect empty.txt text
    column_expect big.txt text
    for _attempt in $(seq 1 40); do [[ "$(ipc columnTextLines)" == "too large" ]] && break; sleep 0.1; done
    [[ "$(ipc columnTextLines)" == "too large" ]] || fail "formats: big.txt read '$(ipc columnTextLines | cut -c1-40)', not the too-large answer"
    column_expect main.rs code
    for _attempt in $(seq 1 40); do [[ "$(ipc columnTextLines)" == "fn main() {"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnTextLines)" == "fn main() {"* ]] || fail "formats: main.rs's lines read '$(ipc columnTextLines)'"
    column_expect data.json code
    column_expect a.zip archive
    for _attempt in $(seq 1 40); do [[ "$(ipc columnArchiveNames)" == *"sample.txt"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnArchiveNames)" == *"sample.txt"* && "$(ipc columnArchiveNames)" == *"notes.md"* ]] || fail "formats: a.zip's members read '$(ipc columnArchiveNames)'"
    shot formats-zip-members
    lit=$(lit_in_rect "$evidence_dir/formats-zip-members.png" $(ipc columnArchiveRect))
    (( lit > 50 )) || fail "formats: a.zip's member box painted $lit lit pixels"
    column_expect b.tar.zst archive
    for _attempt in $(seq 1 40); do [[ "$(ipc columnArchiveNames)" == *"main.rs"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnArchiveNames)" == *"main.rs"* ]] || fail "formats: b.tar.zst's members read '$(ipc columnArchiveNames)'"
    column_expect empty.zip archive
    sleep 0.5
    [[ "$(ipc columnArchiveNames)" == "" && "$(ipc columnFailure)" == "" ]] || fail "formats: the empty archive reads members '$(ipc columnArchiveNames)', failure '$(ipc columnFailure)'"
    column_expect corrupt.zip error
    [[ "$(ipc columnFailure)" == "This archive could not be read." ]] || fail "formats: corrupt.zip's sentence is '$(ipc columnFailure)'"
    column_expect link-file symlink
    column_expect link-dir symlink
    column_expect link-broken symlink
    column_expect core.dump unsupported
    column_expect shut.jpg image
    sleep 1
    [[ "$(ipc columnFrameReady)" == "false" && "$(ipc columnThumbShown)" == "false" ]] || fail "formats: an unreadable image reports frame ready $(ipc columnFrameReady), thumb shown $(ipc columnThumbShown)"
    key -M ctrl -k a -m ctrl >/dev/null
    settle
    [[ "$(ipc previewColumnState)" == "multi" ]] || fail "formats: select all shows $(ipc previewColumnState), not multi"
    key -k Escape >/dev/null
    settle
    printf 'FORMATS images=4 videos=3 audio=1 pdf=2 text=4 code=2 archives=4 links=3 error=1 unsupported=1 multi=1\n'
    kill_flea
}

# Lit pixels inside the preview's content box, inset past any border; the name and floor are the caller's.
preview_surface_lit() {
    local tag="$1" floor="$2" what="$3" sx sy sw sh lit
    read -r sx sy sw sh <<< "$(ipc previewSurfaceRect)"
    [[ -n "$sh" ]] && (( sw > 12 && sh > 12 )) || fail "previewviews: $what has no box on screen ('$(ipc previewSurfaceRect)')"
    shot "previewviews-$tag" >&2
    lit=$(lit_in_rect "$evidence_dir/previewviews-$tag.png" "$((sx + 6))" "$((sy + 6))" "$((sw - 12))" "$((sh - 12))")
    (( lit > floor )) || fail "previewviews: $what painted $lit lit pixels inside its box"
}

# The shared Space preview entered from each view on the same rows, judged on content, with the
# lifetimes GM's review named: a column player dies on a view switch and yields to Space.
case_previewviews() {
    local dir="$fixture_root/previewviews" mode name wx wy cx cy fx fy fw fh p1 p2 changed _attempt
    formats_fixture "$dir"
    launch "$dir"
    wait_listing 26
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    for mode in list grid columns; do
        switch_view list
        goto_row "$(row_index_of p.jpg)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "image" ]] && break; sleep 0.1; done
        [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "image" && "$(ipc previewState)" == "image" ]] \
            || fail "$mode: Space on p.jpg: open $(ipc previewOpen), kind $(ipc previewKind), state $(ipc previewState)"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc previewOpen)" == "false" ]] || fail "$mode: Escape did not close the preview"
        switch_view list
        goto_row "$(row_index_of manual.pdf)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "pdf" ]] && break; sleep 0.1; done
        [[ "$(ipc previewKind)" == "pdf" && "$(ipc previewState)" == "pdf" && "$(ipc previewPdfPage)" == "0" ]] \
            || fail "$mode: Space on manual.pdf: kind $(ipc previewKind), state $(ipc previewState), page $(ipc previewPdfPage)"
        key -k Right >/dev/null
        settle
        key -k Right >/dev/null
        settle
        [[ "$(ipc previewPdfPage)" == "1" ]] || fail "$mode: two Rights left manual.pdf on page $(ipc previewPdfPage), not the last page 1"
        key -k Escape >/dev/null
        settle
        switch_view list
        goto_row "$(row_index_of broken.pdf)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "This file could not be read." ]] && break; sleep 0.1; done
        [[ "$(ipc previewState)" == "This file could not be read." ]] || fail "$mode: Space on broken.pdf reads '$(ipc previewState)'"
        key -k Escape >/dev/null
        settle
        switch_view list
        goto_row "$(row_index_of a.zip)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "archive" ]] && break; sleep 0.1; done
        [[ "$(ipc previewKind)" == "archive" && "$(ipc previewState)" == "archive" && "$(ipc previewArchiveNames)" == *"sample.txt"* ]] \
            || fail "$mode: Space on a.zip: kind $(ipc previewKind), state $(ipc previewState), members '$(ipc previewArchiveNames)'"
        preview_surface_lit "$mode-zip" 50 "a.zip's members"
        key -k Escape >/dev/null
        settle
        # The three other image formats through the original file, not a cached thumbnail: PreviewImage reads the file itself.
        for name in p.png p.webp p.heic; do
            switch_view list
            goto_row "$(row_index_of "$name")"
            switch_view "$mode"
            key -k space >/dev/null
            for _attempt in $(seq 1 60); do [[ "$(ipc previewState)" == "image" ]] && break; sleep 0.1; done
            [[ "$(ipc previewKind)" == "image" && "$(ipc previewState)" == "image" ]] || fail "$mode: Space on $name: kind $(ipc previewKind), state $(ipc previewState)"
            preview_surface_lit "$mode-$name" 200 "$name's picture"
            key -k Escape >/dev/null
            settle
        done
        for name in sample.txt main.rs; do
            switch_view list
            goto_row "$(row_index_of "$name")"
            switch_view "$mode"
            key -k space >/dev/null
            for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "text" ]] && break; sleep 0.1; done
            [[ "$(ipc previewKind)" == "text" && "$(ipc previewText)" == *"$([[ $name == sample.txt ]] && echo 'hello from flea' || echo 'fn main')"* ]] \
                || fail "$mode: Space on $name: kind $(ipc previewKind), state $(ipc previewState), text '$(ipc previewText | cut -c1-40)'"
            preview_surface_lit "$mode-$name" 50 "$name's text"
            key -k Escape >/dev/null
            settle
        done
        # Audio and the three video containers: a player exists, its position advances, the picture moves, and Escape empties the loader.
        for name in tone.wav v.mp4 v.mkv v.webm; do
            switch_view list
            goto_row "$(row_index_of "$name")"
            switch_view "$mode"
            key -k space >/dev/null
            for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "playing" ]] && break; sleep 0.1; done
            [[ "$(ipc previewMediaLoaded)" == "true" && "$(ipc previewState)" == "playing" ]] || fail "$mode: Space on $name: loaded $(ipc previewMediaLoaded), state $(ipc previewState)"
            sleep 0.8
            p1=$(ipc previewPosition)
            shot "previewviews-$mode-$name-1"
            sleep 1.2
            p2=$(ipc previewPosition)
            shot "previewviews-$mode-$name-2"
            (( p2 > p1 )) || fail "$mode: $name's position did not advance, $p1 then $p2"
            if [[ "$name" != tone.wav ]]; then
                read -r fx fy fw fh <<< "$(ipc previewSurfaceRect)"
                (( fw > 0 && fh > 0 )) || fail "$mode: $name's picture has no box"
                changed=$(magick \( "$evidence_dir/previewviews-$mode-$name-1.png" -crop "${fw}x${fh}+${fx}+${fy}" +repage \) \
                    \( "$evidence_dir/previewviews-$mode-$name-2.png" -crop "${fw}x${fh}+${fx}+${fy}" +repage \) \
                    -compose difference -composite -threshold 10% -format "%[fx:int(mean*w*h+0.5)]" info:)
                (( changed > 1000 )) || fail "$mode: $name's picture changed $changed pixels in 1.2 s of playback"
            fi
            key -k Escape >/dev/null
            settle
            [[ "$(ipc previewOpen)" == "false" && "$(ipc previewMediaLoaded)" == "false" ]] || fail "$mode: after Escape on $name, open $(ipc previewOpen), media loaded $(ipc previewMediaLoaded)"
        done
        # Bad then good in the image reader: an unreadable picture, then two rows up to a readable one.
        switch_view list
        goto_row "$(row_index_of shut.jpg)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "This image could not be read." ]] && break; sleep 0.1; done
        [[ "$(ipc previewState)" == "This image could not be read." ]] || fail "$mode: Space on shut.jpg reads '$(ipc previewState)'"
        key k >/dev/null
        key k >/dev/null
        for _attempt in $(seq 1 60); do [[ "$(ipc previewState)" == "image" ]] && break; sleep 0.1; done
        [[ "$(ipc previewState)" == "image" ]] || fail "$mode: the preview did not recover from shut.jpg to p.webp, state $(ipc previewState)"
        preview_surface_lit "$mode-recovered" 200 "the recovered picture"
        key -k Escape >/dev/null
        settle
        printf 'PREVIEWVIEWS %s image=4 pdf=ok error=ok archive=ok text=2 media=4 recovery=ok\n' "$mode"
    done
    # The column player and the two things that must end it: another view, and Space on the same file.
    switch_view list
    goto_row "$(row_index_of v.mp4)"
    switch_view columns
    sleep 1
    [[ "$(ipc previewColumnState)" == "video" ]] || fail "previewviews: the cursor on v.mp4 shows $(ipc previewColumnState)"
    read -r cx cy <<< "$(ipc columnPlayCentre)"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    sleep 1.2
    [[ "$(ipc columnMediaPlaying)" == "true" ]] || fail "previewviews: play did not start in the column"
    switch_view list
    settle
    [[ "$(ipc columnPlayerLoaded)" == "false" ]] || fail "previewviews: the column player survived a switch to the list"
    switch_view columns
    sleep 1
    [[ "$(ipc columnPlayerLoaded)" == "false" ]] || fail "previewviews: a player came back with the view without a press"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    sleep 1.2
    [[ "$(ipc columnMediaPlaying)" == "true" ]] || fail "previewviews: play did not restart in the column"
    key -k space >/dev/null
    for _attempt in $(seq 1 40); do [[ "$(ipc previewKind)" == "video" ]] && break; sleep 0.1; done
    sleep 1
    p1=$(ipc previewPosition)
    sleep 1
    p2=$(ipc previewPosition)
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "video" ]] && (( p2 > p1 )) || fail "previewviews: Space over the playing column: open $(ipc previewOpen), kind $(ipc previewKind), position $p1 then $p2"
    [[ "$(ipc columnPlayerLoaded)" == "false" ]] || fail "previewviews: the column kept its player under the Space preview"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" && "$(ipc columnPlayerLoaded)" == "false" ]] || fail "previewviews: after Escape, preview $(ipc previewOpen), column player $(ipc columnPlayerLoaded)"
    printf 'PREVIEWVIEWS lifetimes=ok\n'
    kill_flea
}

. "$repo/tests/ui-pdf.sh"
. "$repo/tests/ui-trash.sh"
. "$repo/tests/ui-menus.sh"
. "$repo/tests/ui-rename-design.sh"
. "$repo/tests/ui-openwith-design.sh"
. "$repo/tests/ui-providers.sh"
. "$repo/tests/ui-rail.sh"
. "$repo/tests/ui-dropbox-roots.sh"
. "$repo/tests/ui-settings-layout.sh"
. "$repo/tests/ui-settings-places.sh"
. "$repo/tests/ui-card-layout.sh"
. "$repo/tests/ui-preview-visibility.sh"
. "$repo/tests/ui-permissions.sh"
. "$repo/tests/ui-marquee.sh"
. "$repo/tests/ui-oversight.sh"
. "$repo/tests/ui-preview-policy.sh"
. "$repo/tests/ui-operations-design.sh"
. "$repo/tests/ui-convert-design.sh"
. "$repo/tests/ui-dirsortstale.sh"
. "$repo/tests/ui-makedefault.sh"
. "$repo/tests/ui-noblank.sh"
. "$repo/tests/ui-transfer-live.sh"

declare -a wanted=("$@")
[[ ${#wanted[@]} -eq 0 ]] && wanted=(cursor scroll scrollbar terminal open rows click ctrlclick viewrestart dd collide sortrestart duallaunch dirsortstale editplace mute placemenu runscript unmounted sidebar menu background hidden selection watch refresh optical select colour lifted icons thumbs hashcache stale nosweep oem header overflow focus preview pdffocus network netmark networkauth networktimeout gvfs sharebrowser unmount phones eject rename renamelife taildrop providers grid columns operations tabs openterminal renderer settings makedefault clickthrough wheelunder overlays views formats previewviews hangshare openwithdesign noblank transferlive)

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
# case_network makes its own Flea store unreadable on purpose, and Quickshell correctly reports
# that it cannot watch a file it cannot read. This drops that one line and nothing else: the path
# carries this run's own pid and names one fixture home, so no product warning can ever match it.
# The reader has no -q, so it drains the pipe and takes no SIGPIPE; pipefail then reports its own
# status, which is what says whether anything but that one line matched.
expected_warning="inotify_add_watch($fixture_root/network-state/flea/ui.json) failed: (Permission denied)"
# Qt Multimedia's ffmpeg backend saying VAAPI zero-copy needs an OpenGL RHI; Flea runs Vulkan, the backend falls back, and case_views proves the frames still change.
vaapi_warning="VAAPITextureConverter: No rhi or non openGL based RHI"
# case_formats and case_previewviews open a file with no permission bits on purpose; Qt names it, and this run's fixture path is the whole match.
unreadable_warning="$fixture_root/formats/shut.jpg"
unreadable_warning2="$fixture_root/previewviews/shut.jpg"
# case_settings and case_networkauth chmod 000 a fixture ui.json on purpose, so Quickshell reports
# that it cannot watch it. How many times it says so is the watch's business, not this suite's.
unreadable_state_warning="/flea/ui.json) failed: (Permission denied)"
# A case registers a warning once for each time it causes it. Sample input, one uniq -c line: "      2 Process failed to start, ..."
while read -r want warning; do
    count=$(grep -F -c -- "$warning" "$run_log" || true)
    if [[ "$count" != "$want" ]]; then
        printf 'FAIL expected native warning count=%s, registered %s: %s\n' "$count" "$want" "$warning"
        failures=$((failures + 1))
    fi
done < <(sort "$expected_warnings" | uniq -c)
if grep -F -v -e "$expected_warning" -e "$vaapi_warning" -e "$unreadable_warning" -e "$unreadable_warning2" \
    -e "$unreadable_state_warning" "$run_log" \
    | grep -F -v -f "$expected_warnings" | grep -E 'WARN|ERROR|TypeError|ReferenceError|Cannot open'; then
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
