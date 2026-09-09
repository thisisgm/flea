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
answer="$dir/answer.json"
# The picker the backend spawns, standing in for the window: it records the request the backend
# built and writes the reply the backend reads back. The reply is the answer file when a case put
# one there, else 1, the user's own refusal, which carries no URI and keeps a case about the request.
cat > "$dir/flea" <<'STUB'
#!/bin/sh
printf '%s' "$FLEA_PICKER" > "$FLEA_PORTAL_CAPTURE"
if [ -f "$FLEA_PORTAL_ANSWER" ]; then
    cat "$FLEA_PORTAL_ANSWER" > "$2"
else
    printf '{"response":1}' > "$2"
fi
STUB
chmod +x "$dir/flea"

# XDG_RUNTIME_DIR is where the backend's own mkdtemp goes, so it is pointed inside the sandbox.
env FLEA_BIN="$dir/flea" FLEA_PORTAL_CAPTURE="$capture" FLEA_PORTAL_ANSWER="$answer" \
    XDG_RUNTIME_DIR="$dir/run" python3 "$backend" > "$dir/portal.log" 2>&1 &
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

out=$(env FLEA_PORTAL_CAPTURE="$capture" FLEA_PORTAL_FOLDER="$dir/folder" FLEA_PORTAL_ANSWER="$answer" python3 - <<'ASK'
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
answer = os.environ["FLEA_PORTAL_ANSWER"]
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)


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
# one before it. Returns the response code, or "unanswered" for the reply that never arrived, the
# request the stub saw, and the results the caller was handed.
def ask(method, options, token):
    with open(capture, "w"):
        pass
    handle = "%s/request/fleaportaltest/%s" % (OBJECT_PATH, token)
    try:
        reply = bus.call_sync(BACKEND, OBJECT_PATH, CHOOSER, method,
                              GLib.Variant("(osssa{sv})", (handle, "portal.sh", "", "portal.sh", options)),
                              None, Gio.DBusCallFlags.NONE, CALL_TIMEOUT_MS, None)
    except GLib.Error:
        return "unanswered", {}, {}
    code, results = reply.unpack()
    try:
        with open(capture, "r", encoding="utf-8") as handle_file:
            return str(code), json.load(handle_file), results
    except (OSError, ValueError):
        return str(code), {}, results


# The stub answers with this reply for the calls made until forget_answer().
def set_answer(reply):
    with open(answer, "w", encoding="utf-8") as handle_file:
        json.dump(reply, handle_file)


def forget_answer():
    if os.path.exists(answer):
        os.remove(answer)


# A current_filter result as one line: the label, then each rule as tag:value.
def said_filter(results):
    picked = results.get("current_filter")
    if picked is None:
        return "(none)"
    label, rules = picked
    return "%s %s" % (label, ",".join("%d:%s" % (tag, rule) for tag, rule in rules))


def said(value):
    return value if value else "(none)"


def bytestring(text):
    return GLib.Variant("ay", text.encode("utf-8") + b"\0")


code, req, results = ask("OpenFile", {"current_folder": bytestring(folder)}, "open")
print("open %s %s" % (code, said(req.get("folder", ""))))
print("cancel %s" % said(",".join(sorted(results))))

code, req, _ = ask("SaveFile", {"current_folder": bytestring(folder),
                             "current_file": bytestring(folder + "/notes.md")}, "save")
print("save %s %s %s" % (code, said(req.get("folder", "")), said(req.get("file", ""))))

code, req, _ = ask("SaveFiles", {"current_folder": bytestring(folder),
                              "files": GLib.Variant("aay", [b"one.txt\0", b"two.txt\0"])}, "savefiles")
print("savefiles %s %s" % (code, said(",".join(req.get("files", [])))))

# A shape that is neither bytes nor a list of them is refused rather than guessed at, so the picker
# opens where it would have anyway instead of somewhere the caller never named.
code, req, _ = ask("OpenFile", {"current_folder": GLib.Variant("s", "/etc")}, "refused")
print("refused %s %s" % (code, said(req.get("folder", ""))))

# A pick under one of the caller's filters: the caller is told which one held, as the one
# (sa(us)) current_filter result, with globs tagged 0 and mime types tagged 1.
filters = GLib.Variant("a(sa(us))", [("Images", [(0, "*.png"), (1, "image/jpeg")]),
                                    ("Text", [(0, "*.txt")])])
set_answer({"response": 0, "uris": ["file://" + folder + "/a.png"],
            "current_filter": {"label": "Images", "globs": ["*.png"], "mimes": ["image/jpeg"]}})
code, req, results = ask("OpenFile", {"current_folder": bytestring(folder), "filters": filters}, "picked")
print("picked %s %s | %s" % (code, said(",".join(results.get("uris", []))), said_filter(results)))

# A filter with no rule narrows nothing, so the backend drops it rather than echoing it.
set_answer({"response": 0, "uris": ["file://" + folder + "/a.png"],
            "current_filter": {"label": "Nothing", "globs": [], "mimes": []}})
code, req, results = ask("OpenFile", {"current_folder": bytestring(folder)}, "ruleless")
print("ruleless %s %s" % (code, said_filter(results)))
forget_answer()
ASK
)

line() { printf '%s\n' "$out" | grep "^$1 " | head -1; }

check "the backend owned its name" "0" "$(printf '%s\n' "$out" | grep -c '^ready never$')"
check "OpenFile decodes current_folder" "open 1 $dir/folder" "$(line open)"
check "SaveFile decodes current_folder and current_file" "save 1 $dir/folder $dir/folder/notes.md" "$(line save)"
check "SaveFiles answers its caller and decodes files" "savefiles 1 one.txt,two.txt" "$(line savefiles)"
check "a current_folder that is not a bytestring is refused" "refused 1 (none)" "$(line refused)"
check "a cancel returns neither uris nor current_filter" "cancel (none)" "$(line cancel)"
check "a pick under a filter echoes it as current_filter" \
    "picked 0 file://$dir/folder/a.png | Images 0:*.png,1:image/jpeg" "$(line picked)"
check "a filter with no rule is not echoed" "ruleless 0 (none)" "$(line ruleless)"
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
