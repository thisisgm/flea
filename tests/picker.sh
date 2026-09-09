#!/usr/bin/env bash
# Drives the file chooser the way an application does: omarchy-file-select asks the XDG portal,
# xdg-desktop-portal routes org.freedesktop.impl.portal.FileChooser to whichever backend the
# configuration names, and this asserts what comes back AT THE CALLER. It proves nothing about Flea
# unless Flea is the backend, so it checks that first.
# Usage: ./tests/picker.sh [pick|click|crumbs|rail|save|savename|typed|typed_dir|typed_file|typed_url|typed_share|cancel|withdrawn|died|fault|pills|taildrop|dragout]; typed_share, taildrop and dragout are opt-in.
# FLEA_PICKER_CONFIG names the running picker's qs config path, which is the packaged one by default.
# FLEA_PICKER_EVIDENCE names a directory the caller owns for the taildrop case's screenshot.
# FLEA_PICKER_SHARE names a file on a reachable share, as smb://host/share/dir/name, for typed_share.
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
# Without the platform theme no themed icon name resolves, the same read tests/ui.sh makes.
if [[ -z "${QT_QPA_PLATFORMTHEME:-}" ]]; then
    export QT_QPA_PLATFORMTHEME="$(systemctl --user show-environment | grep '^QT_QPA_PLATFORMTHEME=' | cut -d= -f2-)"
fi
# A portal client opens its own connection to the session bus, which an ssh session does not export.
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"

picker_config="${FLEA_PICKER_CONFIG:-/usr/share/flea/ui/picker.qml}"
fixture="$FIXTURE_ROOT/flea-picker-$$"
# The taildrop case takes the title the real caller gives its window, so this is not readonly.
title="Flea picker test $$"
client=0
# The background portal client a case starts, tracked the same way the omarchy-file-select one is.
# A case that fails partway must not leave its window standing: the next case's ipc reaches the
# oldest instance on the same config path, so a leaked window answers for the one under test.
asker=0
# The loopback http server the typed_url case starts in the fixture, ended with the rest.
server=0

ipc() {
    timeout 5 omarchy-drive ipc -p "$picker_config" fleapicker "$@" 2>/dev/null
}

press() {
    omarchy-drive key --window "$title" "$@" >/dev/null || fail "could not press $*"
}

# Only ever this run's own window, named by a title no other run uses, and only when one is up.
cleanup() {
    if [[ "$client" != 0 ]] && kill -0 "$client" 2>/dev/null; then
        omarchy-drive key --window "$title" -k Escape >/dev/null 2>&1
        sleep 1
        kill "$client" 2>/dev/null
    fi
    if [[ "$asker" != 0 ]] && kill -0 "$asker" 2>/dev/null; then
        omarchy-drive key --window "$title" -k Escape >/dev/null 2>&1
        sleep 1
        kill "$asker" 2>/dev/null
    fi
    if [[ "$server" != 0 ]] && kill -0 "$server" 2>/dev/null; then
        kill "$server" 2>/dev/null
    fi
    sandbox_remove "$fixture"
}

# The ipc seam answers empty for a window it cannot reach, and empty is also what a refused save
# name looks like, so every case that reads emptiness as a result proves the seam first.
ipc_is_live() {
    [[ "$(ipc ready)" == "true" ]] || fail "the ipc seam at $picker_config answered nothing, so no reading below means anything"
}
trap cleanup EXIT

backend_is_flea() {
    local owner
    owner=$(busctl --user --list --no-pager 2>/dev/null | grep -c 'org.freedesktop.impl.portal.desktop.flea')
    [[ "$owner" -gt 0 ]] || fail "org.freedesktop.impl.portal.desktop.flea is not activatable, so run flea --picker and restart xdg-desktop-portal"
}

make_fixture() {
    sandbox_make "$fixture"
    printf 'flea picker fixture\n' > "$fixture/alpha.txt"
    printf 'flea picker fixture two\n' > "$fixture/beta.txt"
    head -c 4096 /dev/urandom > "$fixture/gamma.bin"
}

# Walks from the home directory to the fixture with the keyboard alone, which is also the whole of
# the board's navigation contract: Parent climbs, the cursor steps, Enter opens a directory.
walk_to_fixture() {
    local want step
    press -k BackSpace
    for step in 1 2 3 4 5 6 7 8 9 10; do
        [[ "$(ipc path)" == "/home" ]] && break
        sleep 0.3
    done
    [[ "$(ipc path)" == "/home" ]] || fail "Backspace did not climb to /home, the picker is at $(ipc path)"
    for want in flea-sandbox "${fixture##*/}"; do
        step=0
        while [[ "$(ipc cursorName)" != "$want" ]]; do
            step=$((step + 1))
            [[ "$step" -le 200 ]] || fail "no row named $want under $(ipc path)"
            press -k Down
        done
        press -k Return
        sleep 0.5
    done
    [[ "$(ipc path)" == "$fixture" ]] || fail "the picker is at $(ipc path), not $fixture"
}

start_client() {
    omarchy-file-select --title "$title" "$@" > "$fixture/picked.txt" 2> "$fixture/client.err" &
    client=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "no picker window named $title ever appeared"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
}

# The exit status lands in a variable, never on stdout: a command substitution runs this in a
# subshell, which does not own the background client and answers 127 for every run.
client_status=0
wait_for_client() {
    client_status=0
    wait "$client" || client_status=$?
    client=0
}

case_pick() {
    make_fixture
    start_client --multiple
    walk_to_fixture
    [[ "$(ipc cursorName)" == "alpha.txt" ]] || fail "the cursor landed on $(ipc cursorName), not the first row"
    press -k space
    [[ "$(ipc marks)" == "$fixture/alpha.txt" ]] || fail "space marked $(ipc marks)"
    press -k Down
    press -k space
    [[ "$(ipc marks)" == "$fixture/alpha.txt,$fixture/beta.txt" ]] || fail "the second mark left $(ipc marks)"
    press -k Return
    wait_for_client
    [[ "$client_status" == 0 ]] || fail "the caller exited $client_status with $(cat "$fixture/client.err")"
    local want
    want=$(printf '%s\n%s' "$fixture/alpha.txt" "$fixture/beta.txt")
    [[ "$(cat "$fixture/picked.txt")" == "$want" ]] || fail "the caller received $(cat "$fixture/picked.txt")"
    printf 'pick: the caller exited 0 with both paths\n'
}

# A drawn item's centre in screen pixels: the seam answers window pixels, and the floating window's
# own origin is added, the same sum tests/ui.sh's click_row makes for the tiled one. The centre is
# read by the caller, because each seam reader names its own kind of item and its own emptiness.
click_centre() {
    local centre="$1"; shift
    local cx cy wx wy
    read -r cx cy <<< "$centre"
    read -r wx wy < <(omarchy-drive windows --json | jq -r --arg t "$title" '.windows[] | select(.title == $t) | "\(.at[0]) \(.at[1])"')
    [[ -n "${wx:-}" ]] || fail "no window named $title to click in"
    # Everything after the centre goes straight to omarchy-drive: the button, --double, --mods.
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" "$@" >/dev/null
}

click_row() {
    local index="$1"; shift
    local centre
    centre=$(ipc rowCentre "$index")
    [[ -n "$centre" ]] || fail "row $index has no on-screen centre"
    click_centre "$centre" "$@" || fail "could not click row $index"
}

# A segment of the nav strip, by its index in the crumbs the strip draws; see ui/CrumbRow.qml.
click_crumb() {
    local centre
    centre=$(ipc crumbCentre "$1")
    [[ -n "$centre" ]] || fail "crumb $1 has no on-screen centre"
    click_centre "$centre" || fail "could not click crumb $1"
}

# A rail row, by the label it draws: Recent is a location and not a path, so the rail is its only door.
click_place() {
    local centre
    centre=$(ipc placeCentre "$1")
    [[ -n "$centre" ]] || fail "the rail draws no row labelled $1"
    click_centre "$centre" || fail "could not click the rail row $1"
}

# Opens the directory row with this name from the keyboard alone: the cursor steps to it, Enter walks in.
open_named() {
    local step=0
    while [[ "$(ipc cursorName)" != "$1" ]]; do
        step=$((step + 1))
        [[ "$step" -le 200 ]] || fail "no row named $1 under $(ipc path)"
        press -k Down
    done
    press -k Return
    sleep 0.5
}

# The nav strip's segments for a path, joined the way the crumbs reader joins them: what
# ui/js/Nav.js crumbs draws, "~/" or "/" first, then every component with its separator, the leaf bare.
crumb_texts() {
    local rel="$1" parts out="" i
    if [[ "$rel" == "$HOME" || "$rel" == "$HOME/"* ]]; then
        rel="~${rel#"$HOME"}"
    fi
    IFS=/ read -r -a parts <<< "$rel"
    for ((i = 0; i < ${#parts[@]}; i++)); do
        if (( i < ${#parts[@]} - 1 )); then
            out+="${parts[i]}/,"
        else
            out+="${parts[i]}"
        fi
    done
    printf '%s' "$out"
}

# The listing index of the row with this name, so a click can be aimed without assuming the sort.
row_named() {
    local i total
    total=$(ipc total)
    for ((i = 0; i < total; i++)); do
        if [[ "$(ipc rowAt "$i")" == "$1" ]]; then
            printf '%s' "$i"
            return
        fi
    done
    fail "no row named $1 under $(ipc path)"
}

# Finder's two marking modifiers on the chooser's rows: Ctrl toggles the row and Shift marks the
# run from the last toggled row, and neither opens anything, whatever the tap count.
case_click() {
    make_fixture
    sandbox_make "$fixture/sub"
    start_client --multiple
    walk_to_fixture
    local alpha beta gamma sub
    alpha=$(row_named alpha.txt); beta=$(row_named beta.txt); gamma=$(row_named gamma.bin); sub=$(row_named sub)
    click_row "$alpha" left --mods ctrl
    [[ "$(ipc marks)" == "$fixture/alpha.txt" ]] || fail "ctrl+click marked $(ipc marks)"
    click_row "$beta" left --mods ctrl
    [[ "$(ipc marks)" == "$fixture/alpha.txt,$fixture/beta.txt" ]] || fail "the second ctrl+click left $(ipc marks)"
    [[ "$(ipc cursor)" == "$beta" ]] || fail "ctrl+click left the cursor on $(ipc cursor)"
    click_row "$beta" left --mods ctrl
    [[ "$(ipc marks)" == "$fixture/alpha.txt" ]] || fail "a ctrl+click on a marked row left $(ipc marks)"
    click_row "$gamma" left --mods shift
    [[ "$(ipc marks)" == "$fixture/alpha.txt,$fixture/beta.txt,$fixture/gamma.bin" ]] \
        || fail "shift+click from beta to gamma left $(ipc marks)"
    # A modifier never opens: the held double click on a directory row stays in this folder.
    click_row "$sub" left --mods ctrl --double
    [[ "$(ipc path)" == "$fixture" ]] || fail "a ctrl-held double click opened $(ipc path)"
    press -k Escape
    wait_for_client
    [[ "$client_status" != 0 ]] || fail "the caller exited 0 after a cancel"
    printf 'click: ctrl+click toggles a row, shift+click marks the run, and neither opens\n'
}

# Issue 45's segments in the chooser, and the board's Back behind them: a tap on a segment above
# the leaf walks there, which is a move Back undoes; the leaf is a label and answers nothing; Recent
# is a location and not a path, so the strip draws its name and no segments there.
case_crumbs() {
    make_fixture
    sandbox_make "$fixture/sub/deeper"
    start_client --multiple
    walk_to_fixture
    open_named sub
    open_named deeper
    local deep="$fixture/sub/deeper" count
    [[ "$(ipc path)" == "$deep" ]] || fail "crumbs: the picker is at $(ipc path), not $deep"
    [[ "$(ipc crumbs)" == "$(crumb_texts "$deep")" ]] \
        || fail "crumbs: the strip drew $(ipc crumbs), not $(crumb_texts "$deep")"
    [[ -z "$(ipc locationLabel)" ]] || fail "crumbs: a directory drew the label $(ipc locationLabel) beside its segments"
    count=$(ipc crumbCount)
    (( count >= 3 )) || fail "crumbs: the strip drew $count segments, too few to press a parent"
    # A segment's single tap waits out the double-tap interval, see ui/CrumbRow.qml
    # "exclusiveSignals", so every reading after a tap is taken well after it rather than on top of it.
    click_crumb $((count - 1))
    sleep 1
    [[ "$(ipc path)" == "$deep" ]] || fail "crumbs: a tap on the leaf went to $(ipc path)"
    click_crumb $((count - 2))
    sleep 1
    [[ "$(ipc path)" == "$fixture/sub" ]] || fail "crumbs: a tap on the parent segment went to $(ipc path), not $fixture/sub"
    [[ "$(ipc crumbs)" == "$(crumb_texts "$fixture/sub")" ]] \
        || fail "crumbs: after the tap the strip drew $(ipc crumbs), not $(crumb_texts "$fixture/sub")"
    # The tap was pushed onto the history the way Parent is, so Back returns to the leaf it left.
    press -M alt -k Left -m alt
    sleep 0.5
    [[ "$(ipc path)" == "$deep" ]] || fail "crumbs: Back after the segment tap went to $(ipc path), not $deep"
    click_place Recent
    sleep 1
    [[ "$(ipc recent)" == "true" ]] || fail "crumbs: the Recent row opened $(ipc path)"
    [[ "$(ipc locationLabel)" == "Recent" ]] || fail "crumbs: Recent drew the label $(ipc locationLabel)"
    [[ -z "$(ipc crumbs)" ]] || fail "crumbs: Recent drew the segments $(ipc crumbs)"
    [[ "$(ipc crumbCount)" == "0" ]] || fail "crumbs: Recent drew $(ipc crumbCount) segments"
    press -k Escape
    wait_for_client
    [[ "$client_status" != 0 ]] || fail "crumbs: the caller exited 0 after a cancel"
    printf 'crumbs: a parent segment walks and Back returns, the leaf is a label, Recent draws its name alone\n'
}

# The rail from the keyboard: Tab moves the focus reader to the rail, the arrows move its own cursor,
# Enter opens the cursor row and hands the keyboard back, Escape hands it back without cancelling,
# and Tab reaches the rail again once the list has walked somewhere else. The rail's first two rows
# are Recent and Home in every open request, see ui/PickerPlaces.qml, so the cursor's stops are known.
case_rail() {
    make_fixture
    start_client --multiple
    walk_to_fixture
    ipc_is_live
    [[ "$(ipc focusView)" == "list" ]] || fail "rail: the keyboard started in $(ipc focusView), not the list"
    press -k Tab
    sleep 0.3
    [[ "$(ipc focusView)" == "rail" ]] || fail "rail: Tab left the keyboard in $(ipc focusView)"
    press -k Home
    sleep 0.2
    [[ "$(ipc railCursor)" == "0" ]] || fail "rail: Home left the rail cursor at $(ipc railCursor)"
    press -k Down
    sleep 0.2
    [[ "$(ipc railCursor)" == "1" ]] || fail "rail: Down moved the rail cursor to $(ipc railCursor), not 1"
    press -k Up
    sleep 0.2
    [[ "$(ipc railCursor)" == "0" ]] || fail "rail: Up moved the rail cursor to $(ipc railCursor), not 0"
    press -k Up
    sleep 0.2
    [[ "$(ipc railCursor)" == "0" ]] || fail "rail: Up past the first row left the cursor at $(ipc railCursor)"
    # The list's cursor did not move: the arrows reached the rail alone.
    [[ "$(ipc cursorName)" == "alpha.txt" ]] || fail "rail: the arrows moved the list cursor to $(ipc cursorName)"
    press -k Escape
    sleep 0.3
    [[ "$(ipc focusView)" == "list" ]] || fail "rail: Escape left the keyboard in $(ipc focusView)"
    kill -0 "$client" 2>/dev/null || fail "rail: Escape in the rail cancelled the dialog"
    [[ "$(ipc path)" == "$fixture" ]] || fail "rail: Escape moved the picker to $(ipc path)"
    press -k Tab
    sleep 0.3
    press -k Down
    sleep 0.2
    press -k Return
    sleep 1
    [[ "$(ipc path)" == "$HOME" ]] || fail "rail: Enter on Home opened $(ipc path), not $HOME"
    [[ "$(ipc focusView)" == "list" ]] || fail "rail: Enter on a place left the keyboard in $(ipc focusView)"
    [[ "$(ipc railCursor)" == "1" ]] || fail "rail: after opening Home the rail cursor sits at $(ipc railCursor), not on Home"
    # The keyboard is in the list again: Down moves the list cursor and not the rail's.
    press -k Down
    sleep 0.2
    [[ "$(ipc cursor)" == "1" ]] || fail "rail: after Enter the list cursor is at $(ipc cursor), so Down reached the rail"
    [[ "$(ipc railCursor)" == "1" ]] || fail "rail: after Enter Down still moved the rail cursor to $(ipc railCursor)"
    press -k Tab
    sleep 0.3
    [[ "$(ipc focusView)" == "rail" ]] || fail "rail: Tab after a navigation left the keyboard in $(ipc focusView)"
    press -k Escape
    sleep 0.3
    press -k Escape
    wait_for_client
    [[ "$client_status" != 0 ]] || fail "rail: the caller exited 0 after a cancel"
    printf 'rail: Tab reaches the rail, the arrows walk it, Enter opens and returns, Escape returns without cancelling\n'
}

case_cancel() {
    make_fixture
    start_client --multiple
    press -k Escape
    wait_for_client
    # 1 is the caller's "nothing picked", which is a decision; anything above it is a fault.
    [[ "$client_status" == 1 ]] || fail "a cancelled chooser exited $client_status, not 1"
    [[ ! -s "$fixture/picked.txt" ]] || fail "a cancelled chooser printed $(cat "$fixture/picked.txt")"
    printf 'cancel: the caller exited 1 with nothing picked, and nothing was sent\n'
}

# The response the portal actually answered with, which the caller collapses: 1 is the user's refusal
# and 2 is everything else, and omarchy-file-select turns both into exit 1. This is where the two can
# be told apart. Prints "<code> <uri...>", or "none" when nothing answered at all.
# Usage: portal_ask <OpenFile|SaveFile> [current_name]
portal_ask() {
    python3 - "$title" "$1" "${2:-}" <<'ASK'
import sys
import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

title, method, name = sys.argv[1], sys.argv[2], sys.argv[3]
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
loop = GLib.MainLoop()
seen = []

def on_response(connection, sender, path, interface, signal, params):
    code, results = params.unpack()
    seen.append(" ".join([str(code)] + list(results.get("uris", []))))
    loop.quit()

token = "fleatest"
sender = bus.get_unique_name()[1:].replace(".", "_")
path = "/org/freedesktop/portal/desktop/request/%s/%s" % (sender, token)
bus.signal_subscribe("org.freedesktop.portal.Desktop", "org.freedesktop.portal.Request",
                     "Response", path, None, Gio.DBusSignalFlags.NONE, on_response)
options = {"handle_token": GLib.Variant("s", token)}
if name:
    options["current_name"] = GLib.Variant("s", name)
bus.call_sync("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
              "org.freedesktop.portal.FileChooser", method,
              GLib.Variant("(ssa{sv})", ("", title, options)),
              None, Gio.DBusCallFlags.NONE, -1, None)

# A request that never answers is the defect this bounds: the caller would wait 600 s for it.
GLib.timeout_add_seconds(40, loop.quit)
loop.run()
print(seen[0] if seen else "none")
ASK
}

# The backend's own answer to a withdrawn request. org.freedesktop.portal.Request says a request the
# caller closed emits no Response, so the frontend signal portal_ask reads is never sent for this one
# and the backend's return value is the only place the contract is observable. Prints "<code>".
backend_withdraw() {
    python3 - "$title" <<'ASK'
import sys
import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

BACKEND = "org.freedesktop.impl.portal.desktop.flea"
CHOOSER = "org.freedesktop.impl.portal.FileChooser"
REQUEST = "org.freedesktop.impl.portal.Request"
handle = "/org/freedesktop/portal/desktop/request/fleatest/withdrawn"
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
loop = GLib.MainLoop()
seen = []

def on_reply(source, result, _data=None):
    try:
        seen.append(str(source.call_finish(result).unpack()[0]))
    except GLib.Error as error:
        seen.append("error " + error.message)
    loop.quit()

bus.call(BACKEND, "/org/freedesktop/portal/desktop", CHOOSER, "OpenFile",
         GLib.Variant("(osssa{sv})", (handle, "", "", sys.argv[1], {})),
         None, Gio.DBusCallFlags.NONE, 60000, None, on_reply)

def close():
    bus.call_sync(BACKEND, handle, REQUEST, "Close", None, None, Gio.DBusCallFlags.NONE, 10000, None)
    return False

GLib.timeout_add_seconds(6, close)
# A request that never answers is the defect this bounds: the caller would wait 600 s for it.
GLib.timeout_add_seconds(40, loop.quit)
loop.run()
print(seen[0] if seen else "none")
ASK
}

case_withdrawn() {
    make_fixture
    backend_withdraw > "$fixture/codes.txt"
    [[ "$(cat "$fixture/codes.txt")" == "2" ]] || fail "a withdrawn request answered $(cat "$fixture/codes.txt"), not 2"
    printf 'withdrawn: a request the caller closed answers 2, which is not the 1 a refusal answers\n'
}

# The window dying under a live request, which is the one way a chooser can leave a caller waiting.
# It must answer 2 rather than the 1 a refusal answers, and it must answer at all.
case_died() {
    make_fixture
    portal_ask OpenFile > "$fixture/died.txt" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "no picker window named $title appeared"
    omarchy-drive window kill "$title" >/dev/null || fail "could not kill the picker window"
    wait "$asker"; asker=0
    [[ "$(cat "$fixture/died.txt")" == "2" ]] || fail "a picker that died answered $(cat "$fixture/died.txt"), not 2"
    printf 'died: a picker killed mid request answers 2, and the caller is answered rather than left waiting\n'
}

# SaveFile, the board's other mode: the name the caller suggested comes back under the directory the
# window is standing in, and the caller owns the write after that.
case_save() {
    make_fixture
    portal_ask SaveFile notes.md > "$fixture/saved.txt" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "SaveFile raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    [[ "$(ipc saveName)" == "notes.md" ]] || fail "the save field holds $(ipc saveName), not the caller's name"
    walk_to_fixture
    press -k Return
    wait "$asker"; asker=0
    [[ "$(cat "$fixture/saved.txt")" == "0 file://$fixture/notes.md" ]] \
        || fail "SaveFile answered $(cat "$fixture/saved.txt")"
    [[ ! -e "$fixture/notes.md" ]] || fail "the chooser wrote the file itself, which is the caller's to do"
    printf 'save: SaveFile answers 0 with the reviewed URI, and writes nothing\n'
}

# The save name is a client string and the answer built from it must stay inside the folder the
# window showed. Both ways one arrives: the current_name tools/flea-portal passes through verbatim,
# and whatever somebody types into the field afterwards. Neither may be rewritten into a safe name;
# a rewrite answers the caller with a path nobody approved.
# What carries this case is three readings and no filesystem guard: ipc saveName is empty, so the
# name was never adopted; Enter on it says "Name the file"; and the request answers 1 with no URI.
# A guard hashing $HOME/.config/autostart stood here and could not redden: nothing in the chooser
# writes a file at all, it answers a URI the caller owns the write for, and two levels up from either
# folder this window can stand in is /home/.config/autostart or /.config/autostart, never that one.
case_savename() {
    make_fixture

    # The exact string the review demonstrated, arriving the way it did: as the caller's own name.
    portal_ask SaveFile '../../.config/autostart/pwn.desktop' > "$fixture/escaped.txt" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "SaveFile raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    ipc_is_live
    [[ -z "$(ipc saveName)" ]] || fail "the caller's traversal was adopted into the field as $(ipc saveName)"
    # Enter reaches accept() only from a file row: on a directory it walks in, which is the board's
    # own rule and is why case_save walks here too before pressing it.
    walk_to_fixture
    press -k Return
    [[ "$(ipc message)" == *"Name the file"* ]] || fail "Enter on the refused name said $(ipc message)"
    kill -0 "$asker" 2>/dev/null || fail "the request was answered while the field held no name"
    press -k Escape
    wait "$asker"; asker=0
    [[ "$(cat "$fixture/escaped.txt")" == "1" ]] \
        || fail "the traversal request answered $(cat "$fixture/escaped.txt"), and 1 is the only answer with no URI"

    # A name typed into the field goes through the same predicate at accept(), and the exhaustive
    # case list for it is tests/js/picker.js. It is not driven here: the field publishes no
    # accessibility tree, so a click on it has to be aimed by reading the screen, and the name it
    # would aim at is also drawn in the URI line directly below the box. Aiming a committed test at
    # whichever of the two OCR happens to return first buys a flake, not coverage.

    # An interior NUL cannot cross D-Bus, whose strings end at the first one, so this arm goes in
    # through FLEA_PICKER: JSON's \u0000 is six characters in the environment and one after parsing,
    # which is where the NUL that truncates a path at the syscall comes from.
    local flea reply
    flea="$(cd "$(dirname "$0")/.." && pwd)/target/release/flea"
    [[ -x "$flea" ]] || fail "no built flea at $flea"
    reply="$fixture/nul-reply.json"
    # FLEA_UI names the same picker the ipc above talks to. Without it paths::ui_dir() prefers the
    # packaged /usr/share/flea/ui, and this arm would drive a window the rest of the suite is not
    # reading, or none at all when that install predates the picker.
    FLEA_UI="$(dirname "$picker_config")" \
        FLEA_PICKER="{\"mode\":\"save\",\"title\":\"$title\",\"folder\":\"$fixture\",\"name\":\"pwn\\u0000.desktop\"}" \
        "$flea" --pick "$reply" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "the NUL request raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    ipc_is_live
    [[ -z "$(ipc saveName)" ]] || fail "a name carrying a NUL was adopted into the field as $(ipc saveName)"
    press -k Return
    [[ "$(ipc message)" == *"Name the file"* ]] || fail "Enter on the NUL name said $(ipc message)"
    press -k Escape
    wait "$asker"; asker=0
    [[ "$(cat "$reply")" == '{"response":1}' ]] || fail "the NUL request replied $(cat "$reply")"

    # A path the user types is the save dialog's own grammar, not a traversal: ":" moves the keyboard
    # into the Filename box, a folder with its trailing slash opens and clears the box, a file path
    # opens its parent and leaves the leaf as the name, and a second Return answers that leaf under
    # the folder now shown. A URL is refused in the footer with the dialog left open.
    reply="$fixture/path-reply.json"
    FLEA_UI="$(dirname "$picker_config")" \
        FLEA_PICKER="{\"mode\":\"save\",\"title\":\"$title\",\"folder\":\"$fixture\",\"name\":\"x.txt\"}" \
        "$flea" --pick "$reply" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "the path request raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    ipc_is_live
    [[ "$(ipc saveName)" == "x.txt" ]] || fail "the save field holds $(ipc saveName), not the caller's name"
    press ':'
    [[ "$(ipc saveFocused)" == "1" ]] || fail "the colon left the keyboard outside the save field"
    press -k BackSpace; press -k BackSpace; press -k BackSpace; press -k BackSpace; press -k BackSpace
    press 'https://example.org/a.txt'
    press -k Return
    sleep 1
    [[ "$(ipc message)" == "Save needs a local path" ]] || fail "a URL in the save field said $(ipc message)"
    kill -0 "$asker" 2>/dev/null || fail "a URL in the save field ended the dialog"
    local typed="https://example.org/a.txt" n
    for ((n = 0; n < ${#typed}; n++)); do press -k BackSpace; done
    press "$(dirname "$fixture")/"
    press -k Return
    sleep 1
    [[ "$(ipc path)" == "$(dirname "$fixture")" ]] || fail "the typed folder left the picker at $(ipc path)"
    [[ -z "$(ipc saveName)" ]] || fail "the save field still holds $(ipc saveName) after the folder opened"
    [[ "$(ipc saveFocused)" == "1" ]] || fail "the keyboard left the save field after the folder opened"
    press "$fixture/new.txt"
    press -k Return
    sleep 1
    [[ "$(ipc path)" == "$fixture" ]] || fail "the typed file path left the picker at $(ipc path)"
    [[ "$(ipc saveName)" == "new.txt" ]] || fail "the save field holds $(ipc saveName), not the typed leaf"
    press -k Return
    wait "$asker"; asker=0
    [[ "$(cat "$reply")" == "{\"response\":0,\"uris\":[\"file://$fixture/new.txt\"]}" ]] \
        || fail "the path request replied $(cat "$reply")"
    [[ ! -e "$fixture/new.txt" ]] || fail "the chooser wrote the file itself, which is the caller's to do"

    printf 'savename: a name that would leave the folder is refused from the caller and with a NUL in it, and a typed path walks or lands\n'
}

# The location field. Driven the way the NUL arm above is, a direct launch, because no portal call
# is needed to type into it. Three readings carry it: ":" moved the keyboard from the list into the
# field, the text typed after it is what ipc entry reports, and Escape in the field hands the
# keyboard back without cancelling, which the still-running process and the kept text both say.
case_typed() {
    make_fixture
    local flea reply
    flea="$(cd "$(dirname "$0")/.." && pwd)/target/release/flea"
    [[ -x "$flea" ]] || fail "no built flea at $flea"
    reply="$fixture/typed-reply.json"
    FLEA_UI="$(dirname "$picker_config")" \
        FLEA_PICKER="{\"mode\":\"open\",\"title\":\"$title\",\"folder\":\"$fixture\"}" \
        "$flea" --pick "$reply" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "the typed request raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    ipc_is_live
    [[ "$(ipc entryFocused)" == "0" ]] || fail "the field had the keyboard before anyone asked for it"
    press ':'
    [[ "$(ipc entryFocused)" == "1" ]] || fail "the colon left the keyboard in the list"
    press '/tmp'
    [[ "$(ipc entry)" == "/tmp" ]] || fail "the field holds $(ipc entry), not the typed path"
    press -k Escape
    [[ "$(ipc entryFocused)" == "0" ]] || fail "Escape in the field kept the keyboard there"
    kill -0 "$asker" 2>/dev/null || fail "Escape in the field cancelled the dialog"
    [[ "$(ipc entry)" == "/tmp" ]] || fail "Escape in the field emptied it to $(ipc entry)"
    press -k Escape
    wait "$asker"; asker=0
    [[ "$(cat "$reply")" == '{"response":1}' ]] || fail "the typed request replied $(cat "$reply")"

    printf 'typed: ":" focuses the location field, the text lands in it, and Escape returns to the list without cancelling\n'
}

# The user's own pills, from filters.toml under a config home the case owns: a caller that sent no
# filters draws All files first and active, then one pill per table labelled by extension; a caller
# that sent filters sees its own row and never a config pill. Both dialogs are refused, so no answer.
case_pills() {
    make_fixture
    local flea reply
    flea="$(cd "$(dirname "$0")/.." && pwd)/target/release/flea"
    [[ -x "$flea" ]] || fail "no built flea at $flea"
    mkdir -p "$fixture/config/flea"
    printf '[[filter]]\nglobs = ["*.jpg", "*.jpeg"]\n\n[[filter]]\nname = "Text"\nglobs = ["*.txt"]\n' > "$fixture/config/flea/filters.toml"
    reply="$fixture/pills-reply.json"
    XDG_CONFIG_HOME="$fixture/config" FLEA_UI="$(dirname "$picker_config")" \
        FLEA_PICKER="{\"mode\":\"open\",\"title\":\"$title\",\"folder\":\"$fixture\"}" \
        "$flea" --pick "$reply" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "the pills request raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    ipc_is_live
    [[ "$(ipc chips)" == "All files,.jpg (.jpg, .jpeg),Text" ]] || fail "the config drew the chips $(ipc chips)"
    [[ "$(ipc chip)" == "-1" ]] || fail "All files was not the active chip, chip $(ipc chip) was"
    [[ "$(ipc shownTotal)" == "$(ipc total)" ]] || fail "All files hid rows: $(ipc shownTotal) of $(ipc total) shown"
    press -k Escape
    wait "$asker"; asker=0
    [[ "$(cat "$reply")" == '{"response":1}' ]] || fail "the pills request replied $(cat "$reply")"

    XDG_CONFIG_HOME="$fixture/config" FLEA_UI="$(dirname "$picker_config")" \
        FLEA_PICKER="{\"mode\":\"open\",\"title\":\"$title\",\"folder\":\"$fixture\",\"filters\":[{\"label\":\"Images\",\"globs\":[\"*.png\"],\"mimes\":[]}]}" \
        "$flea" --pick "$reply" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "the filtered request raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    ipc_is_live
    [[ "$(ipc chips)" == "Images,All files" ]] || fail "a caller's filters drew the chips $(ipc chips)"
    [[ "$(ipc chip)" == "0" ]] || fail "the caller's first filter was not active, chip $(ipc chip) was"
    press -k Escape
    wait "$asker"; asker=0
    printf 'pills: a caller with no filters gets All files and the config pills, a caller with filters gets only its own\n'
}

# Starts the picker on the fixture directly, the way case_typed does, and leaves it to the caller
# to type and to end. asker holds the process; reply names the file it answers into.
start_typed() {
    local flea
    flea="$(cd "$(dirname "$0")/.." && pwd)/target/release/flea"
    [[ -x "$flea" ]] || fail "no built flea at $flea"
    reply="$fixture/$1-reply.json"
    FLEA_UI="$(dirname "$picker_config")" \
        FLEA_PICKER="{\"mode\":\"open\",\"title\":\"$title\",\"folder\":\"$fixture\"}" \
        "$flea" --pick "$reply" &
    asker=$!
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "the $1 request raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    ipc_is_live
}

# A typed folder is walked into, the box clears and the keyboard goes back to the list: the Windows
# filename box's rule for a directory. The parent of the fixture is typed with its trailing slash,
# which is how the line says "this is a directory"; the dialog is then refused so nothing is answered.
case_typed_dir() {
    make_fixture
    local reply
    start_typed typed_dir
    press ':'
    press "$(dirname "$fixture")/"
    press -k Return
    sleep 1
    [[ "$(ipc path)" == "$(dirname "$fixture")" ]] || fail "the typed folder left the picker at $(ipc path)"
    [[ -z "$(ipc entry)" ]] || fail "the field still holds $(ipc entry) after the folder opened"
    [[ "$(ipc entryFocused)" == "0" ]] || fail "the keyboard stayed in the field after the folder opened"
    press -k Escape
    wait "$asker"; asker=0
    [[ "$(cat "$reply")" == '{"response":1}' ]] || fail "the typed_dir request replied $(cat "$reply")"
    printf 'typed_dir: a typed folder opens, the field clears and the list has the keyboard\n'
}

# A typed file is selected in its parent: the cursor lands on it, it is the one mark, the text stays
# in the field, and Return on the same text again is the answer. A missing path first, which says so
# in the footer and leaves the dialog open, so the two are told apart by the reply arriving only once.
case_typed_file() {
    make_fixture
    local reply
    start_typed typed_file
    press ':'
    press "$fixture/nothing.txt"
    press -k Return
    sleep 1
    [[ "$(ipc message)" == "Not found: $fixture/nothing.txt" ]] || fail "a missing path said $(ipc message)"
    kill -0 "$asker" 2>/dev/null || fail "a missing path ended the dialog"
    press -k Escape
    press ':'
    # The field kept the refused line, so it is cleared from the end before the real path goes in.
    local typed="$fixture/nothing.txt" n
    for ((n = 0; n < ${#typed}; n++)); do press -k BackSpace; done
    press "$fixture/beta.txt"
    press -k Return
    sleep 1
    [[ "$(ipc path)" == "$fixture" ]] || fail "the typed file moved the picker to $(ipc path)"
    [[ "$(ipc cursorName)" == "beta.txt" ]] || fail "the cursor is on $(ipc cursorName), not the typed file"
    [[ "$(ipc marks)" == "$fixture/beta.txt" ]] || fail "the marks are $(ipc marks), not the typed file"
    [[ "$(ipc entry)" == "$fixture/beta.txt" ]] || fail "the field lost the file name: $(ipc entry)"
    press -k Return
    wait "$asker"; asker=0
    [[ "$(cat "$reply")" == "{\"response\":0,\"uris\":[\"file://$fixture/beta.txt\"]}" ]] \
        || fail "the typed_file request replied $(cat "$reply")"
    printf 'typed_file: a missing path is refused in the footer, a typed file is marked in its parent, and a second Return answers it\n'
}

# A typed URL is downloaded and the application gets the file, never the URL: the Windows file
# dialog's rule. python's own http.server serves the fixture on a loopback port it picks, the URL of
# alpha.txt is typed, and the reply has to name a file under the picker cache with the same bytes.
# The download stays in the cache, as it does for a real caller; the sweep at a later --pick takes it.
case_typed_url() {
    make_fixture
    local reply port step uri cache
    (cd "$fixture" && exec python3 -u -m http.server --bind 127.0.0.1 0) > "$fixture/server.out" 2>&1 &
    server=$!
    port=""
    for step in $(seq 1 50); do
        port=$(sed -n 's/^Serving HTTP on 127\.0\.0\.1 port \([0-9]*\) .*/\1/p' "$fixture/server.out" | head -1)
        [[ -n "$port" ]] && break
        sleep 0.2
    done
    [[ -n "$port" ]] || fail "http.server never said which port it bound: $(cat "$fixture/server.out")"
    start_typed typed_url
    press ':'
    press "http://127.0.0.1:$port/alpha.txt"
    press -k Return
    for step in $(seq 1 60); do
        [[ -s "$reply" ]] && break
        sleep 0.5
    done
    [[ -s "$reply" ]] || fail "the typed URL never answered; the footer says: $(ipc message) / $(ipc fetchLine)"
    wait "$asker"; asker=0
    uri=$(python3 -c 'import json, sys, urllib.parse
d = json.load(open(sys.argv[1]))
sys.exit(1) if d.get("response") != 0 or len(d.get("uris", [])) != 1 else print(urllib.parse.unquote(d["uris"][0]))' "$reply") \
        || fail "the typed_url request replied $(cat "$reply")"
    cache="${XDG_CACHE_HOME:-$HOME/.cache}/flea/picker/"
    [[ "$uri" == "file://$cache"* ]] || fail "the answer $uri is not a file under $cache"
    cmp -s "${uri#file://}" "$fixture/alpha.txt" || fail "the fetched file ${uri#file://} differs from alpha.txt"
    printf 'typed_url: a typed URL is fetched into the picker cache and the caller gets the file as %s\n' "$uri"
}

# Opt-in: a typed share URL is mounted through gvfs and walked on its FUSE path, so the file is
# marked in its FUSE directory and a second Return answers that path. It needs a share nobody on
# the box has to be told the password for, so it runs only when one is named.
case_typed_share() {
    local url="${FLEA_PICKER_SHARE:-}"
    [[ -n "$url" ]] || fail "typed_share needs FLEA_PICKER_SHARE, a file on a share this box can mount anonymously"
    make_fixture
    local reply leaf
    leaf="${url##*/}"
    start_typed typed_share
    press ':'
    press "$url"
    press -k Return
    local n
    for ((n = 0; n < 20; n++)); do
        [[ "$(ipc cursorName)" == "$leaf" ]] && break
        sleep 1
    done
    [[ "$(ipc cursorName)" == "$leaf" ]] || fail "the cursor is on $(ipc cursorName), not the typed file; the footer says $(ipc message)"
    [[ "$(ipc path)" == /run/user/*/gvfs/* ]] || fail "the typed share URL moved the picker to $(ipc path), not a gvfs path"
    [[ "$(ipc marks)" == "$(ipc path)/$leaf" ]] || fail "the marks are $(ipc marks), not the typed file"
    [[ "$(ipc entry)" == "$url" ]] || fail "the field lost the URL: $(ipc entry)"
    local fuse
    fuse="$(ipc path)/$leaf"
    press -k Return
    wait "$asker"; asker=0
    [[ "$(cat "$reply")" == "{\"response\":0,\"uris\":[\"file://$fuse\"]}" ]] \
        || fail "the typed_share request replied $(cat "$reply")"
    printf 'typed_share: a typed share URL is mounted, the file is marked on its FUSE path, and a second Return answers it\n'
}

# A chooser that cannot open at all refuses before any window and writes no reply file, which is what
# tools/flea-portal turns into 2. Both refusals are argv-level, so neither needs the display.
case_fault() {
    make_fixture
    local flea status
    flea="$(cd "$(dirname "$0")/.." && pwd)/target/release/flea"
    [[ -x "$flea" ]] || fail "no built flea at $flea"
    status=0
    env -u FLEA_PICKER "$flea" --pick "$fixture/never.json" 2>"$fixture/pick.err" || status=$?
    [[ "$status" == 2 ]] || fail "a pick with no request exited $status, not 2"
    grep -q "needs FLEA_PICKER" "$fixture/pick.err" || fail "the refusal said $(cat "$fixture/pick.err")"
    status=0
    FLEA_PICKER='{"mode":"open"}' "$flea" --pick "" 2>>"$fixture/pick.err" || status=$?
    [[ "$status" == 2 ]] || fail "a pick with no reply file exited $status, not 2"
    [[ ! -e "$fixture/never.json" ]] || fail "a refused picker still wrote a reply file"
    printf 'fault: a picker that cannot open exits 2 before any window and writes no reply\n'
}

# Opt-in, like tests/ui.sh's networklive: this one drives the stock Tailscale panel and really sends
# a file, so it never runs unless a peer is named and it is asked for by name.
case_taildrop() {
    local peer="${FLEA_TAILDROP_PEER:-}"
    [[ -n "$peer" ]] || fail "taildrop needs FLEA_TAILDROP_PEER, so no run can send a file to a peer nobody named"
    make_fixture
    local sent="flea-picker-acceptance-$(date +%Y%m%d-%H%M%S).txt"
    printf 'Flea file picker acceptance fixture, generated %s. Safe to delete.\n' "$(date -Is)" > "$fixture/$sent"
    title="Send to $peer"
    # The panel is a toggle, so a run that ended with it open would close it here instead: this
    # dismisses whatever is up before asking for it, or the click below reads an empty screen.
    omarchy-drive key -k Escape >/dev/null 2>&1
    sleep 1
    omarchy-drive ipc omarchy.tailscale open >/dev/null || fail "the stock Tailscale panel would not open"
    sleep 3
    # The peer row has no click action of its own; the hover under the click is what moves the
    # panel's own cursor, and s is the panel's own send key. Neither is Flea's. The label is a
    # separate variable because the panel draws the peer's display name, which is not always the
    # name tailscale addresses, and because click-text reads the screen with OCR.
    omarchy-drive click-text "${FLEA_TAILDROP_LABEL:-$peer}" >/dev/null \
        || fail "no row reading ${FLEA_TAILDROP_LABEL:-$peer} in the Tailscale panel"
    # The evidence for which row the pointer actually landed on, before anything is sent. It goes
    # in the fixture unless the caller named a directory of its own to keep it in.
    omarchy-drive shot "${FLEA_PICKER_EVIDENCE:-$fixture}/tailscale-panel.png" >/dev/null
    omarchy-drive key s >/dev/null || fail "the panel refused the send key"
    omarchy-drive wait window "$title" --timeout 25 >/dev/null || fail "the Tailscale send raised no picker window"
    omarchy-drive focus "$title" >/dev/null || fail "the picker window would not take focus"
    walk_to_fixture
    local step=0
    while [[ "$(ipc cursorName)" != "$sent" ]]; do
        step=$((step + 1))
        [[ "$step" -le 50 ]] || fail "no row named $sent under $(ipc path)"
        press -k Down
    done
    press -k space
    [[ "$(ipc marks)" == "$fixture/$sent" ]] || fail "space marked $(ipc marks)"
    press -k Return
    # The caller's own report, which is the only thing downstream of the picker: it sent, or it said
    # it could not. Read off the screen, because Omarchy's shell owns its notifications.
    omarchy-drive wait ocr "" "Sent to $peer" --timeout 60 >/dev/null \
        || fail "the caller never reported a send, and the screen says: $(omarchy-drive ocr 2>&1 | tail -4)"
    printf 'taildrop: the stock plugin raised Flea, and the caller sent %s to %s\n' "$sent" "$peer"
}

# Opt-in, like tests/drag.sh: a real pointer through uinput, which is the only motion Qt sees as a
# drag, and a second window on the screen. A row is lifted out of the chooser and dropped on the
# floor of a browser window standing in the fixture's dest folder. The browser reads the drag as
# foreign, because the marker names another process, so the drop is a copy by path: the file lands
# in dest byte for byte and the chooser's own row survives. The seam is read mid-gesture, inside
# the nested loop the platform drag runs, for the rows it carries and the footer's line.
case_dragout() {
    make_fixture
    sandbox_make "$fixture/dest"
    command -v ydotool >/dev/null || fail "dragout needs ydotool, the uinput pointer tests/drag.sh drives"
    local flea browser alpha centre cx cy wx wy bx by bw bh px py mid step
    flea="$(cd "$(dirname "$0")/.." && pwd)/target/release/flea"
    [[ -x "$flea" ]] || fail "no built flea at $flea"
    export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-$XDG_RUNTIME_DIR/.ydotool_socket}"
    # The drop target first, so the chooser tiles beside it and the pointer has two windows to cross.
    # flea execs qs, so the window's pid is this one, and the window is found by it and never by class alone.
    FLEA_UI="$(dirname "$picker_config")" setsid "$flea" "$fixture/dest" > "$fixture/browser.log" 2>&1 &
    browser=$!
    for step in $(seq 1 50); do
        read -r bx by bw bh < <(hyprctl clients -j | jq -r --argjson p "$browser" '.[] | select(.pid == $p) | "\(.at[0]) \(.at[1]) \(.size[0]) \(.size[1])"' | head -1)
        [[ -n "${bx:-}" ]] && break
        sleep 0.2
    done
    [[ -n "${bx:-}" ]] || { kill "$browser" 2>/dev/null; fail "no browser window came up on $fixture/dest"; }
    start_client --multiple
    walk_to_fixture
    alpha=$(row_named alpha.txt)
    centre=$(ipc rowCentre "$alpha")
    read -r cx cy <<< "$centre"
    read -r wx wy < <(omarchy-drive windows --json | jq -r --arg t "$title" '.windows[] | select(.title == $t) | "\(.at[0]) \(.at[1])"')
    [[ -n "${wx:-}" ]] || { kill "$browser" 2>/dev/null; fail "no window named $title to lift from"; }
    # The warp only places the pointer; the lift and the glide are uinput, which carries frames.
    hyprctl dispatch "hl.dsp.cursor.move({x = $((cx + wx)), y = $((cy + wy))})" >/dev/null; sleep 0.4
    ydotool click 0x40 >/dev/null 2>&1; sleep 0.3
    ydotool mousemove -x 12 -y 12 >/dev/null 2>&1; sleep 0.6
    # The seam answers inside the drag's nested loop, as tests/drag.sh R4 reads the window's own line.
    mid="$(ipc dragRows)|$(ipc message)"
    # Half the remaining distance a step, re-read each time: libinput accelerates relative motion.
    for step in $(seq 1 24); do
        read -r px py < <(hyprctl cursorpos | tr -d ',')
        local tx=$((bx + bw / 2)) ty=$((by + bh * 4 / 5)) dx dy
        dx=$((tx - px)); dy=$((ty - py))
        [[ ${dx#-} -le 4 && ${dy#-} -le 4 ]] && break
        ydotool mousemove -x $((dx / 2)) -y $((dy / 2)) >/dev/null 2>&1; sleep 0.05
    done
    sleep 0.5
    ydotool click 0x80 >/dev/null 2>&1; sleep 0.8
    for step in $(seq 1 40); do [[ -e "$fixture/dest/alpha.txt" ]] && break; sleep 0.25; done
    kill "$browser" 2>/dev/null
    [[ "$mid" == "$alpha|Copy 1 item to a folder" ]] || fail "mid-drag the seam read [$mid], not [$alpha|Copy 1 item to a folder]"
    [[ -e "$fixture/dest/alpha.txt" ]] || fail "the row never landed in $fixture/dest"
    cmp -s "$fixture/alpha.txt" "$fixture/dest/alpha.txt" || fail "the landed file differs from alpha.txt"
    [[ -e "$fixture/alpha.txt" ]] || fail "a drag out moved alpha.txt instead of copying it"
    [[ -z "$(ipc dragRows)" ]] || fail "the gesture left dragRows at $(ipc dragRows)"
    [[ -z "$(ipc message)" ]] || fail "the gesture left the footer saying $(ipc message)"
    press -k Escape
    wait_for_client
    printf 'dragout: a row lifted out of the chooser lands in another window as a copy, and the chooser survives\n'
}

backend_is_flea
[[ "$#" -gt 0 ]] || set -- pick click crumbs rail save savename typed typed_dir typed_file typed_url cancel withdrawn died fault pills
for name in "$@"; do
    case "$name" in
        pick) case_pick ;;
        click) case_click ;;
        crumbs) case_crumbs ;;
        rail) case_rail ;;
        save) case_save ;;
        savename) case_savename ;;
        typed) case_typed ;;
        typed_dir) case_typed_dir ;;
        typed_file) case_typed_file ;;
        typed_url) case_typed_url ;;
        typed_share) case_typed_share ;;
        cancel) case_cancel ;;
        withdrawn) case_withdrawn ;;
        died) case_died ;;
        fault) case_fault ;;
        pills) case_pills ;;
        taildrop) case_taildrop ;;
        dragout) case_dragout ;;
        *) fail "no case named $name" ;;
    esac
done
