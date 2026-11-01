#!/usr/bin/env python3
# Scan active clients and interactive layers for a verified free desktop point.
# Sample argv: xwtab_free_point.py 0 0 2560 1440 DP-2 clients.json layers.json monitors.json
import json
import sys
import math

GRID_INSET = 8 # Keep the pointer away from monitor edges.
GRID_STRIDE = 24 # Bound the bottom-up search without changing its coverage grid.
CATCHER_NAMESPACE = "flea-tab-tearoff"

def number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def rectangle(node, fields, label):
    if not isinstance(node, dict) or any(not number(node.get(key)) for key in fields):
        raise ValueError(label + " rectangle field missing or not a number")


# Sample snapshots: clients=[{"mapped":true,"at":[0,0],"size":[900,500]}], layers={"DP-2":{"levels":{"1":[]}}}, monitors=[{"x":0,"y":0,"width":2560,"height":1440}].
def validate_snapshots(clients, layers, monitors):
    if not isinstance(clients, list): raise ValueError("clients must be a list")
    for client in clients:
        if not isinstance(client, dict): raise ValueError("client must be an object")
        if not isinstance(client.get("mapped"), bool):
            raise ValueError("client mapped field missing or not a boolean")
        for field in ("at", "size"):
            values = client.get(field)
            if not isinstance(values, list) or len(values) != 2 or not all(number(v) for v in values):
                raise ValueError("client " + field + " rectangle field missing or not a number")
    if not isinstance(layers, dict): raise ValueError("layers must be an object")
    for entry in layers.values():
        if not isinstance(entry, dict) or not isinstance(entry.get("levels"), dict):
            raise ValueError("layers must contain an object of level lists")
        for level in entry["levels"].values():
            if not isinstance(level, list): raise ValueError("layer level must be a list")
            for node in level: rectangle(node, ("x", "y", "w", "h"), "layer")
    if not isinstance(monitors, list) or not monitors: raise ValueError("monitors must be a non-empty list")
    for monitor in monitors: rectangle(monitor, ("x", "y", "width", "height"), "monitor")


def monitor_entry(monitors, mon_name):
    # The named monitor only: another monitor's workspace never answers for it.
    for m in monitors:
        if m.get("name") == mon_name:
            return m
    raise ValueError("monitor " + mon_name + " is not in the monitors snapshot")
def workspace_ids(entry):
    # Active id plus the special id only when one is open (nonzero).
    active = None
    special = None
    aw = entry.get("activeWorkspace", None)
    if isinstance(aw, dict):
        active = aw.get("id", None)
    elif isinstance(aw, int):
        active = aw
    sw = entry.get("specialWorkspace", None)
    if isinstance(sw, dict):
        special = sw.get("id", None)
    elif isinstance(sw, int):
        special = sw
    valid = set()
    try:
        if active is not None and int(active) != 0:
            valid.add(int(active))
    except (TypeError, ValueError):
        pass
    try:
        if special is not None and int(special) != 0:
            valid.add(int(special))
    except (TypeError, ValueError):
        pass
    return valid
def client_rects(clients, valid):
    # Only mapped, non-hidden clients on valid workspaces cover the desktop; an unknown active workspace keeps every mapped row.
    rects = []
    for c in clients:
        if not c.get("mapped"):
            continue
        if c.get("hidden"):
            continue
        if valid:
            ws = c.get("workspace", None)
            wid = ws.get("id", None) if isinstance(ws, dict) else ws
            try:
                wid = int(wid)
            except (TypeError, ValueError):
                continue
            if wid not in valid:
                continue
        x, y = c["at"]
        w, h = c["size"]
        rects.append([x, y, w, h])
    return rects
BAR_NAMESPACE = "omarchy-bar"


def reserved_rects(monitor, mx, my, mw, mh):
    # Hyprland's reserved edges, [left, top, right, bottom]: the strip a screen-wide bar surface takes input on.
    left, top, right, bottom = (monitor.get("reserved") or [0, 0, 0, 0])[:4]
    return [r for r in ([mx, my, left, mh], [mx, my, mw, top], [mx + mw - right, my, right, mh], [mx, my + mh - bottom, mw, bottom]) if r[2] > 0 and r[3] > 0]


def layer_rects(layers, mon_name, mx, my, mw, mh, monitor=None):
    # Only levels 1, 2 and 3 of the focused monitor, read off the levels key.
    rects = []
    entry = layers.get(mon_name, None)
    if entry is None:
        raise ValueError("layers snapshot has no entry for monitor " + mon_name)
    levels = entry["levels"]
    for key in ("1", "2", "3"):
        for node in levels.get(key, []):
            ns = str(node.get("namespace", ""))
            x = node["x"]
            y = node["y"]
            w = node["w"]
            h = node["h"]
            full = x == mx and y == my and w == mw and h == mh
            if key == "1" and ns == CATCHER_NAMESPACE and full:
                continue
            # Omarchy 4.5's bar surface spans its screen and masks input to the strip (shell plugins/bar/Bar.qml).
            if ns == BAR_NAMESPACE and full and monitor is not None:
                rects.extend(reserved_rects(monitor, mx, my, mw, mh))
                continue
            rects.append([x, y, w, h])
    return rects
def find_free_point(mx, my, mw, mh, rects):
    # Bottom-up 24 px grid, first uncovered point wins, empty when none is free.
    def covered(px, py):
        for rx, ry, rw, rh in rects:
            if rx <= px < rx + rw and ry <= py < ry + rh:
                return True
        return False
    for py in range(my + mh - GRID_INSET, my - 1, -GRID_STRIDE):
        for px in range(mx + GRID_INSET, mx + mw - GRID_INSET, GRID_STRIDE):
            if not covered(px, py):
                return (px, py)
    return None
def main(argv):
    # Args: MX MY MW MH MON_NAME CLIENTS_JSON LAYERS_JSON MONITORS_JSON.
    if len(argv) != 9:
        sys.stderr.write("usage: xwtab_free_point.py MX MY MW MH MON_NAME CLIENTS LAYERS MONITORS\n")
        return 2
    try:
        mx = int(argv[1])
        my = int(argv[2])
        mw = int(argv[3])
        mh = int(argv[4])
    except ValueError:
        sys.stderr.write("xwtab_free_point: monitor geometry is not numeric\n")
        return 2
    mon_name = argv[5]
    try:
        with open(argv[6], "r", encoding="utf-8") as f:
            clients = json.load(f)
        with open(argv[7], "r", encoding="utf-8") as f:
            layers = json.load(f)
        with open(argv[8], "r", encoding="utf-8") as f:
            monitors = json.load(f)
    except (OSError, ValueError) as e:
        sys.stderr.write("xwtab_free_point: could not read input: %s\n" % e)
        return 2
    try:
        validate_snapshots(clients, layers, monitors)
        monitor = monitor_entry(monitors, mon_name)
        valid = workspace_ids(monitor)
        rects = client_rects(clients, valid)
        rects.extend(layer_rects(layers, mon_name, mx, my, mw, mh, monitor))
    except ValueError as error:
        sys.stderr.write("xwtab_free_point: invalid snapshot: %s\n" % error)
        return 2
    found = find_free_point(mx, my, mw, mh, rects)
    if found is None:
        sys.stdout.write("\n")
    else:
        sys.stdout.write("%d %d\n" % found)
    return 0
if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
