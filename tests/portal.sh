#!/usr/bin/env bash
# Drives tools/flea-portal over a real D-Bus round trip and asserts what the handler decoded.
#
# The defect this exists for looked correct in isolation. path_option() gated on
# isinstance(value, bytes), and PyGObject 3.56.3 on Python 3.14 unpacks a D-Bus `ay` as a list of
# ints, so every current_folder and current_file was dropped on the floor and every SaveFiles raised
# an AttributeError inside on_call before invocation.return_value, leaving the caller to wait out its
# own 600 s timeout. A synthetic GLib.Variant unpack proves the shape and nothing else; only a call
# arriving over a bus proves the handler, which is why this suite exists rather than a unit test.
#
# No window opens: FLEA_BIN names a stub that records the request and writes the reply, which is the
# seam tools/flea-portal already reads. The bus is a private one this suite starts and takes with it,
# so the operator's own chooser routing is never touched and no D-Bus service file is written.
set -u
set -o pipefail

# The re-exec comes before everything, including the guard: every case needs a session bus and this
# suite must never borrow the operator's, where owning the backend's name would shadow the real one.
if [ -z "${FLEA_PORTAL_PRIVATE_BUS:-}" ]; then
    command -v dbus-run-session >/dev/null 2>&1 || {
        printf 'portal.sh: dbus-run-session is missing, and this suite will not run on the session bus\n' >&2
        exit 1
    }
    export FLEA_PORTAL_PRIVATE_BUS=1
    exec dbus-run-session -- "$0" "$@"
fi

# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

backend=tools/flea-portal
[ -f "$backend" ] || { printf 'portal.sh: %s is missing, refusing to report on nothing\n' "$backend" >&2; exit 1; }
python3 -c 'import gi; gi.require_version("Gio", "2.0")' 2>/dev/null || {
    printf 'portal.sh: python-gobject is missing, which is the backend the suite drives\n' >&2
    exit 1
}

dir="$FIXTURE_ROOT/flea-portal-$$"
# The one child this suite starts, so the one it may signal: the pid came from its own spawn.
portal=0
cleanup() {
    if [ "$portal" != 0 ] && kill -0 "$portal" 2>/dev/null; then
        kill "$portal" 2>/dev/null
        wait "$portal" 2>/dev/null
    fi
    sandbox_remove "$dir"
}
trap cleanup EXIT HUP INT TERM
sandbox_make "$dir"
mkdir -p "$dir/run" "$dir/folder"

capture="$dir/request.json"
# The picker the backend spawns, standing in for the window: it records the request the backend
# built and writes the reply the backend reads back. 1 is the user's own refusal, which carries no
# URI, so every case here stays about the request and not about the answer.
cat > "$dir/flea" <<'STUB'
#!/bin/sh
printf '%s' "$FLEA_PICKER" > "$FLEA_PORTAL_CAPTURE"
cat "$FLEA_PORTAL_ANSWER" > "$2"
STUB
chmod +x "$dir/flea"

# XDG_RUNTIME_DIR is where the backend's own mkdtemp goes, so it is pointed inside the sandbox.
env FLEA_BIN="$dir/flea" FLEA_PORTAL_CAPTURE="$capture" FLEA_PORTAL_ANSWER="$dir/answer.json" XDG_RUNTIME_DIR="$dir/run" \
    python3 "$backend" > "$dir/portal.log" 2>&1 &
portal=$!

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"
    echo "  expected: $expected"
    echo "  actual:   $actual"
    fail=1
  else
    echo "ok   $label"
  fi
}

out=$(env FLEA_PORTAL_CAPTURE="$capture" FLEA_PORTAL_FOLDER="$dir/folder" python3 - <<'ASK'
import json
import os
import sys
import time

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

BACKEND = "org.freedesktop.impl.portal.desktop.flea"
OBJECT_PATH = "/org/freedesktop/portal/desktop"
CHOOSER = "org.freedesktop.impl.portal.FileChooser"
# A caller waits 600 s for a response; ten seconds is long enough to tell answered from never.
CALL_TIMEOUT_MS = 10000
READY_TIMEOUT_SEC = 15

capture = os.environ["FLEA_PORTAL_CAPTURE"]
folder = os.environ["FLEA_PORTAL_FOLDER"]
fixture = os.path.dirname(folder)
last_results = {}
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)


def guard(path):
    assert path and os.path.isabs(path)
    assert os.path.isfile(os.path.join(fixture, ".flea-test-sandbox"))
    assert os.path.commonpath([os.path.realpath(path), fixture]) == fixture and os.path.realpath(path) != fixture
    return path


def owns_the_name():
    reply = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                          "ListNames", None, None, Gio.DBusCallFlags.NONE, 5000, None)
    return BACKEND in reply.unpack()[0]


# The suite spawned the backend a moment ago and there is no service file to activate it, so nothing
# below may run before it owns its name.
deadline = time.monotonic() + READY_TIMEOUT_SEC
while not owns_the_name():
    if time.monotonic() > deadline:
        print("ready never")
        sys.exit(1)
    time.sleep(0.2)


# One request: the capture is emptied first, so what is read back is this call's own and never the
# one before it. Returns the response code, or "unanswered" for the reply that never arrived.
def ask(method, options, token, answer=None):
    global last_results
    guard(os.path.join(fixture, "run"))
    with open(guard(os.path.join(fixture, "answer.json")), "w") as output:
        json.dump({"response": 1} if answer is None else answer, output)
    with open(guard(capture), "w"):
        pass
    handle = "%s/request/fleaportaltest/%s" % (OBJECT_PATH, token)
    try:
        reply = bus.call_sync(BACKEND, OBJECT_PATH, CHOOSER, method,
                              GLib.Variant("(osssa{sv})", (handle, "portal.sh", "", "portal.sh", options)),
                              None, Gio.DBusCallFlags.NONE, CALL_TIMEOUT_MS, None)
    except GLib.Error:
        return "unanswered", {}
    last_results = reply.unpack()[1]
    try:
        with open(capture, "r", encoding="utf-8") as handle_file:
            return str(reply.unpack()[0]), json.load(handle_file)
    except (OSError, ValueError):
        return str(reply.unpack()[0]), {}


def said(value):
    return value if value else "(none)"


def bytestring(text):
    return GLib.Variant("ay", text.encode("utf-8") + b"\0")


code, req = ask("OpenFile", {"current_folder": bytestring(folder)}, "open")
print("open %s %s" % (code, said(req.get("folder", ""))))

code, req = ask("SaveFile", {"current_folder": bytestring(folder),
                             "current_file": bytestring(folder + "/notes.md")}, "save")
print("save %s %s %s" % (code, said(req.get("folder", "")), said(req.get("file", ""))))

code, req = ask("SaveFiles", {"current_folder": bytestring(folder),
                              "files": GLib.Variant("aay", [b"one.txt\0", b"two.txt\0"])}, "savefiles")
print("savefiles %s %s" % (code, said(",".join(req.get("files", [])))))

# A shape that is neither bytes nor a list of them is refused rather than guessed at, so the picker
# opens where it would have anyway instead of somewhere the caller never named.
code, req = ask("OpenFile", {"current_folder": GLib.Variant("s", "/etc")}, "refused")
print("refused %s %s" % (code, said(req.get("folder", ""))))

filters = [("Same label", [(0, "*.png")]), ("Same label", [(1, "text/plain")])]
options = {"filters": GLib.Variant("a(sa(us))", filters), "current_filter": GLib.Variant("(sa(us))", filters[1])}
uri = "file://" + folder + "/name%20%23.txt"
code, req = ask("OpenFile", options, "filters", {"response": 0, "uris": [uri], "filter": 1})
print("filters %s %s %s %s" % (code, req.get("currentIndex"), last_results.get("uris") == [uri], last_results.get("current_filter") == filters[1]))
code, req = ask("OpenFile", options, "allfiles", {"response": 0, "uris": [uri], "filter": -1})
print("allfiles %s %s" % (code, last_results.get("current_filter") == ("All files", [(0, "*")])))
code, req = ask("OpenFile", options, "cancel", {"response": 1, "uris": [uri]})
print("cancel %s %s" % (code, last_results == {}))
for token, answer in [
    ("empty", {"response": 0, "uris": []}),
    ("badcode", {"response": True, "uris": [uri]}),
    ("missing", {}),
    ("badfilter", {"response": 0, "uris": [uri], "filter": 8}),
    ("remote", {"response": 0, "uris": ["file://remote/a"]}),
    ("nul", {"response": 0, "uris": ["file:///a%00b"]}),
    ("escape", {"response": 0, "uris": ["file:///a%xy"]}),
    ("rawspace", {"response": 0, "uris": ["file:///a b"]}),
]:
    code, req = ask("OpenFile", {}, token, answer)
    print("%s %s %s" % (token, code, last_results == {}))

os.rename(guard(os.path.join(fixture, "flea")), guard(os.path.join(fixture, "flea-away")))
try:
    code, req = ask("OpenFile", {}, "launcher")
    print("launcher %s %s" % (code, last_results == {}))
finally:
    os.rename(guard(os.path.join(fixture, "flea-away")), guard(os.path.join(fixture, "flea")))
os.rename(guard(os.path.join(fixture, "run")), guard(os.path.join(fixture, "run-away")))
try:
    code, req = ask("OpenFile", {}, "runtime")
    print("runtime %s %s" % (code, last_results == {}))
finally:
    os.rename(guard(os.path.join(fixture, "run-away")), guard(os.path.join(fixture, "run")))
ASK
)
ask_status=$?

line() { printf '%s\n' "$out" | grep "^$1 " | head -1; }

check "the backend owned its name" "0" "$(printf '%s\n' "$out" | grep -c '^ready never$')"
check "the D-Bus driver completed" "0" "$ask_status"
check "OpenFile decodes current_folder" "open 1 $dir/folder" "$(line open)"
check "SaveFile decodes current_folder and current_file" "save 1 $dir/folder $dir/folder/notes.md" "$(line save)"
check "SaveFiles answers its caller and decodes files" "savefiles 1 one.txt,two.txt" "$(line savefiles)"
check "a current_folder that is not a bytestring is refused" "refused 1 (none)" "$(line refused)"
check "filter identity and reviewed URI round-trip" "filters 0 1 True True" "$(line filters)"
check "All files is explicit in callback" "allfiles 0 True" "$(line allfiles)"
check "cancelled callback contains no results" "cancel 1 True" "$(line cancel)"
for refused in empty badcode missing badfilter remote nul escape rawspace launcher runtime; do
    check "$refused fails closed" "$refused 2 True" "$(line "$refused")"
done
# The backend elides what it cannot do to a sentence, so a traceback in its log is a defect of its own.
check "the backend raised nothing" "0" "$(grep -c 'Traceback' "$dir/portal.log")"

if [ "$fail" = 0 ]; then
  echo "portal.sh: all checks passed"
else
  echo "portal.sh: FAILED"
  echo "--- backend log ---"
  cat "$dir/portal.log"
fi
exit "$fail"
