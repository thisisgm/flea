"""Refuses a native run in seconds when the Hyprland display is not the battery's to drive."""
import json
import time

from .adapters import OWNED_CLASS_PREFIX
from .evidence import Blocked
from . import native
from .native import compositor

# A person at the display moves the pointer, the focus or a window within this many seconds.
QUIET_SECONDS = 3


def sample(commands):
    # Sample input, hyprctl -j cursorpos: {"x": 1280, "y": 720}.
    cursor = json.loads(compositor(commands, "hypr_query_cursor").stdout)
    monitors = json.loads(compositor(commands, "hypr_query_monitors").stdout)
    clients = json.loads(compositor(commands, "hypr_query_clients").stdout)
    return {"cursor": [cursor.get("x"), cursor.get("y")],
            "focus": next((c.get("address") for c in clients if c.get("focusHistoryID") == 0), None),
            "workspaces": {m["name"]: (m.get("activeWorkspace") or {}).get("name") for m in monitors},
            "windows": sorted(c.get("address") for c in clients), "monitors": monitors, "clients": clients}


def display(commands, quiet=QUIET_SECONDS):
    compositor(commands, "hypr_query_version")
    if native.locked():
        raise Blocked("display is locked by hyprlock, so every capture would show the lock screen")
    first = sample(commands)
    stale = [m["name"] for m in first["monitors"] if m["name"].startswith("FLEA-")]
    if stale:
        raise Blocked("owned output " + ", ".join(stale) + " from another run is still present; that run is live or crashed")
    owned = [c.get("class") for c in first["clients"] if str(c.get("class", "")).startswith(OWNED_CLASS_PREFIX)]
    if owned:
        raise Blocked("owned terminal " + ", ".join(owned) + " from another run is still mapped")
    saver = [c.get("class") for c in first["clients"] if "screensaver" in str(c.get("class", "")).lower()]
    if saver:
        raise Blocked("a screensaver window is mapped (" + ", ".join(saver) + "), so the display is idle-managed, not free")
    time.sleep(quiet)
    second = sample(commands)
    for key, what in (("cursor", "pointer moved"), ("focus", "focused window changed"),
                      ("workspaces", "a monitor changed workspace"), ("windows", "a window opened or closed")):
        if first[key] != second[key]:
            raise Blocked(f"display in use: {what} within {quiet} s ({first[key]} then {second[key]})")
    return {"status": "PASS", "quiet_seconds": quiet, "cursor": second["cursor"],
            "workspaces": second["workspaces"], "windows": len(second["windows"])}
