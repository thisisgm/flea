"""Calendar supplement producer: the original 17 date states, driven natively against the integrated candidate."""
import argparse
import ast
import base64
from contextlib import contextmanager
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import struct
import sys
import tempfile
import time
import uuid
import zlib
from zoneinfo import ZoneInfo

from .adapters import TERMINALS, Adapter, owned_identity
from .evidence import Blocked, Commands, MARKER, Owned, absolute, digest, output_root, save, source_identity
from .geometry import COLUMNS, ROWS
from .keys import Keymap
from .native import Deadline, Display, DisplayLost, Terminal, wait_for
from .protocol import capabilities
from .receipts import CALENDAR_CHECKS, REQUIRED
from .reconcile import shutdown_failure
from .references import Board, separator
from .scenarios import SETTLE_STABLE_SECONDS, send, settled
from . import fixtures as fixture_tree, png, preflight

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
ORIGINAL_DRIVER_SHA256 = "e3f18db34c5423ff8450b59d649d0395388bf8c85a5a3f45c5854ecf680b24b5"
ORIGINAL_MANIFEST_SHA256 = "10f86963175d3fe0d5a16d4af0e282e532a2e42c3d193e9b5ad8e9cc1a0add9b"
IDENTITY_FIELDS = ("commit", "dirty_patch_sha256", "elf_sha256", "references", "untracked")
# The listing the original harness built (v0.3.12 tests/ui-tui.sh lines 284-288), under the candidate's private /fx.
CWD = "/fx/v/listing"
RECIPE_FOLDERS = ("amber", "bronze")
RECIPE_FILES = ("charlie.txt", "delta-needleproof.txt", "echo.txt", ".hidden-proof")
RECIPE_NESTED = ("amber/nested-proof.txt", "native navigation proof\n")
DATE = re.compile(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}")
# Any part of a date or a time of day, in either the one date form or a month-name form.
PARTIAL_DATE = re.compile(r"\d{4}-\d{2}|\b\d{1,2}:\d{2}\b|\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\.? \d{1,2}\b")
LISTING_ROW = re.compile(r"^[▎ ](\S+) (.+?)(?: {2,}(\S(?:.*\S)?))? *$")
QUICK_LOOK_BAR = re.compile(r"^(\S+) (.+?)(?: {2,}(.*?))? {2,}(×|)$")
# The candidate names the new detail column in its status row for four seconds (src/tui/model.rs MESSAGE_TIME).
DETAIL_COLUMNS = ("Modified", "Mode", "Owner", "Size")
NOTICE = "Detail column · {}"
NOTICE_SECONDS, NOTICE_LAST_SHOWN, NOTICE_LATEST = 4.0, 3.5, 5.5
NOTICE_POLL = .05
STRINGS = ("properties.title", "properties.loading", "properties.path", "properties.kind", "properties.size",
           "properties.modified", "properties.permissions", "properties.owner", "properties.group",
           "properties.deleted", "properties.target", "card.esc", "preview.loadHint")
# The retained original driver, zlib and base64, read only as data: labels, cases and dates come from its syntax tree.
ORIGINAL_DRIVER = "".join("""
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
rKnLtHDVM9md/wJGa7Xb
""".split())


def original_source(path=None):
    data = zlib.decompress(base64.b64decode(ORIGINAL_DRIVER))
    if hashlib.sha256(data).hexdigest() != ORIGINAL_DRIVER_SHA256:
        raise Blocked("embedded original calendar driver SHA-256 mismatch")
    if path is not None:
        path = absolute(path)
        if not path.is_file():
            raise Blocked("retained original calendar driver is absent: " + str(path))
        if digest(path) != ORIGINAL_DRIVER_SHA256:
            raise Blocked("retained original calendar driver SHA-256 mismatch")
    return data


def contract():
    retained = json.loads((HERE / "calendar.json").read_text())
    states = retained.get("states")
    if (retained.get("schema") != 1 or retained.get("driver_sha256") != ORIGINAL_DRIVER_SHA256
            or retained.get("identity_sha256") != ORIGINAL_MANIFEST_SHA256
            or not isinstance(states, list) or len(states) != 17
            or any(not isinstance(label, str) or not label for label in states) or len(set(states)) != 17):
        raise Blocked("original calendar contract changed")
    return retained


def original_plan():
    """Labels, cases, fixture dates, Quick Look names and the hidden case, read from the original driver's syntax."""
    tree = ast.parse(original_source())
    assigned = {target.id: node.value for node in ast.walk(tree) if isinstance(node, ast.Assign)
                for target in node.targets if isinstance(target, ast.Name)}
    previews = ast.literal_eval(assigned["PREVIEW_CASES"])
    labels = assigned["EXPECTED_LABELS"]
    suffixes = ast.literal_eval(labels.left.generators[1].iter)
    expected = {case + suffix for case in previews for suffix in suffixes} | ast.literal_eval(labels.right)

    def item(node):
        return node.id if isinstance(node, ast.Name) else ast.literal_eval(node)
    definitions = [(item(row.elts[0]), None if isinstance(row.elts[1], ast.Constant) and row.elts[1].value is None
                    else tuple(item(part) for part in row.elts[1].elts)) for row in assigned["definitions"].elts]
    smoke = next(node for node in ast.walk(tree) if isinstance(node, ast.FunctionDef) and node.name == "smoke")
    quicklook = next(ast.literal_eval(node.comparators[0]) for node in ast.walk(smoke)
                     if isinstance(node, ast.Compare) and isinstance(node.ops[0], ast.In))
    loaded = next(node.args[0].value for node in ast.walk(smoke) if isinstance(node, ast.Call)
                  and isinstance(node.func, ast.Attribute) and node.func.attr == "endswith")
    calls = [node for node in ast.walk(smoke) if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)]
    hidden = next(node.args[0].value for node in calls if node.func.attr == "select"
                  and any(k.arg == "hidden" and k.value.value is True for k in node.keywords))
    hidden_captures = sorted(((node.lineno, node.args[0].value, node.args[2].value if len(node.args) > 2 else "preview")
                              for node in calls if node.func.attr == "capture" and len(node.args) > 1
                              and isinstance(node.args[0], ast.Constant) and getattr(node.args[1], "value", None) == hidden))
    plan = {"labels": sorted(expected), "definitions": definitions, "cases": ast.literal_eval(assigned["cases"]),
            "quicklook": list(quicklook), "loaded_suffix": loaded, "hidden": hidden,
            "hidden_captures": [(label, kind) for _, label, kind in hidden_captures]}
    captured = sorted(step["label"] for step in schedule(plan) if step["op"] == "capture")
    if captured != plan["labels"] or captured != sorted(contract()["states"]):
        raise Blocked("adapted schedule does not reproduce the original 17 labels")
    return plan


def schedule(plan):
    """The original smoke order, as logical candidate actions."""
    steps = []
    for name, label in plan["cases"]:
        steps += [{"op": "select", "name": name, "hidden": False},
                  {"op": "capture", "label": label + "-preview", "name": name, "kind": "preview",
                   "unloaded": name.endswith(plan["loaded_suffix"])}]
        if name.endswith(plan["loaded_suffix"]):
            steps += [{"op": "action", "action": "loadPreview"},
                      {"op": "capture", "label": label + "-loaded", "name": name, "kind": "loaded"}]
        steps += [{"op": "action", "action": "properties"},
                  {"op": "capture", "label": label + "-properties", "name": name, "kind": "properties"},
                  {"op": "key", "key": "Escape"}]
        if name in plan["quicklook"]:
            steps += [{"op": "action", "action": "preview"},
                      {"op": "capture", "label": label + "-quicklook", "name": name, "kind": "quicklook"},
                      {"op": "key", "key": "Escape"}]
    steps += [{"op": "action", "action": "toggleHidden"}, {"op": "select", "name": plan["hidden"], "hidden": True}]
    for label, kind in plan["hidden_captures"]:
        if kind == "properties":
            steps.append({"op": "action", "action": "properties"})
        steps.append({"op": "capture", "label": label, "name": plan["hidden"], "kind": kind})
        if kind == "properties":
            steps.append({"op": "key", "key": "Escape"})
    return steps


def plan(terminals=TERMINALS):
    """Inventory only: the 17 original labels for each selected terminal, never a native result."""
    terminals = list(terminals)
    if (not terminals or len(set(terminals)) != len(terminals)
            or any(terminal not in TERMINALS for terminal in terminals)):
        raise Blocked("calendar terminals contain unknown or duplicate values")
    retained = contract()
    adapted = digest(Path(__file__))
    return [{"terminal": terminal, "kind": "calendar", "id": label,
             "original_driver_sha256": retained["driver_sha256"],
             "original_manifest_sha256": retained["identity_sha256"],
             "adapted_driver_sha256": adapted, "checks": list(CALENDAR_CHECKS)}
            for terminal in terminals for label in retained["states"]]


def driver_provenance(original=None):
    retained = contract()
    original_source(original)
    return {"original_driver_sha256": retained["driver_sha256"],
            "original_manifest_sha256": retained["identity_sha256"],
            "adapted_driver_sha256": digest(Path(__file__)), "original_driver": str(original or "embedded")}


def original_png():
    """The original's 2,100,000-byte image, from its own png_bytes definition."""
    tree = ast.parse(original_source())
    function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "png_bytes")
    namespace = {"struct": struct, "zlib": zlib}
    exec(compile(ast.Module([function], []), "original-date_case.py", "exec"), namespace)
    return namespace["png_bytes"]()


def build_fixtures(listing, zone, plan=None, now=None):
    """The listing with the original definitions' exact wall times in the candidate's zone."""
    plan = plan or original_plan()
    listing = absolute(listing)
    if listing.exists():
        raise Blocked("fixture listing must be new: " + str(listing))
    listing.mkdir()
    for name in RECIPE_FOLDERS:
        (listing / name).mkdir()
    for name in RECIPE_FILES:
        (listing / name).write_text(name + "\n")
    (listing / RECIPE_NESTED[0]).write_text(RECIPE_NESTED[1])
    tz = ZoneInfo(zone)
    year = (now or datetime.datetime.now(tz)).year
    image = None
    for name, calendar in plan["definitions"]:
        if name.endswith(plan["loaded_suffix"]):
            image = listing / name
            image.write_bytes(original_png())
    rows = {}
    totals = {name: tree_bytes(listing / name) for name, _ in plan["definitions"] if (listing / name).is_dir()}
    for name, calendar in plan["definitions"]:
        path = listing / name
        if not path.exists() or path.is_symlink():
            raise Blocked("fixture recipe lacks the original definition: " + name)
        wall = tuple(year if part == "year" else part for part in calendar) if calendar else None
        mtime = int(datetime.datetime(*wall, tzinfo=tz).timestamp()) if wall else 0
        os.utime(path, (mtime, mtime))
        if path.stat().st_mtime_ns != mtime * 1000000000:
            raise Blocked(f"filesystem did not retain the fixture mtime: {name}")
        rows[name] = {"directory": path.is_dir(), "mtime": mtime,
                      "modified": datetime.datetime(*wall).strftime("%Y-%m-%d %H:%M") if wall else None,
                      "detail": size_text(totals[name]) if path.is_dir() else ("2.1 MB" if path == image else f"{path.stat().st_size} B"),
                      "children": sorted(p.name for p in path.iterdir()) if path.is_dir() else None}
    return rows


def tree_bytes(top):
    """A folder's total as the candidate's folder sizes count it: its own and every entry's lstat size, links unfollowed."""
    total = os.lstat(top).st_size
    for directory, folders, files in os.walk(top):
        total += sum(os.lstat(os.path.join(directory, name)).st_size for name in folders + files)
    return total


def size_text(count):
    """Decimal units with one decimal place above 999 bytes, the board's size words (462 B, 22.1 kB, 2.6 MB)."""
    if count < 1000:
        return f"{count} B"
    value, units = float(count), ["kB", "MB", "GB", "TB"]
    for unit in units:
        value /= 1000
        if value < 1000 or unit == units[-1]:
            return f"{value:.1f} {unit}"


def candidate_settings(preset):
    """The original harness's candidate settings (v0.3.12 tests/ui-tui.sh lines 289-290): previews load on the key."""
    return {"keys": preset, "hidden": False, "sort": {"key": "name", "reverse": False},
            "preview": {"loadOn": "manual", "column": True}}


def strings(repo=REPO):
    catalog = json.loads((repo / "i18n/en/flea.json").read_text())
    missing = [key for key in STRINGS if not isinstance(catalog.get(key), str)]
    if missing:
        raise Blocked("English catalog lacks calendar strings: " + ", ".join(missing))
    return {key: catalog[key] for key in STRINGS}


def mapped(cells):
    """A row's text with each character's cell column; a wide glyph's partner cell carries no character."""
    pairs = [(x, cell["ch"]) for x, cell in enumerate(cells) if cell["ch"]]
    return "".join(ch for _, ch in pairs), [x for x, ch in pairs for _ in ch]


def span(columns, start, length, y):
    return [(columns[i], y) for i in range(start, start + length)]


def listing_rows(snapshot):
    """Rows with exactly the two pane rules: the listing pane's mark, name and detail, and the preview pane's text."""
    rows = []
    for y, cells in enumerate(snapshot["grid"][2:-2], 2):
        text, columns = mapped(cells)
        rules = [i for i, ch in enumerate(text) if ch == "│"]
        if len(rules) != 2:
            continue
        pane = text[rules[0] + 1:rules[1]]
        first = columns[rules[0] + 1] if rules[0] + 1 < len(columns) else None
        row = {"y": y, "preview": text[rules[1] + 1:], "preview_start": rules[1] + 1, "columns": columns, "name": None,
               "detail": None, "cursor": first is not None and (cells[first]["ch"] == "▎" or any(
                   cells[x].get("reverse") for x in columns[rules[0] + 1:rules[1]]))}
        match = LISTING_ROW.match(pane)
        if match:
            row.update(name=match[2], detail=match[3], name_start=rules[0] + 1 + match.start(2),
                       detail_start=rules[0] + 1 + match.start(3) if match[3] else None)
        rows.append(row)
    return rows


def visible_fixtures(fixtures, selected, hidden=False):
    return [name for name in fixtures if hidden or not name.startswith(".") or name == selected]


def glyph(cell, choices):
    return bool(cell["ch"]) and cell["ch"] in choices


def card(snapshot):
    """The first framed card: its corners and its interior rows with their cell columns."""
    grid = snapshot["grid"]
    for y, cells in enumerate(grid):
        for x0, cell in enumerate(cells):
            if not glyph(cell, "╭┌"):
                continue
            x1 = x0 + 1
            while x1 < len(cells) and cells[x1]["ch"] == "─":
                x1 += 1
            if x1 >= len(cells) or not glyph(cells[x1], "╮┐") or x1 - x0 < 3:
                continue
            for y1 in range(y + 2, len(grid)):
                if glyph(grid[y1][x0], "╰└") and glyph(grid[y1][x1], "╯┘"):
                    interior = [(row, *mapped(grid[row][x0 + 1:x1])) for row in range(y + 1, y1)]
                    return {"x0": x0, "x1": x1, "y0": y, "y1": y1,
                            "rows": [(row, text, [x0 + 1 + c for c in columns]) for row, text, columns in interior]}
    return None


def expected_size(fixtures, name):
    detail = fixtures[name].get("detail")
    if not detail:
        raise AssertionError(f"no expected size for {name}")
    return detail


def preview_view(snapshot, fixtures, selected, kind, strings, wanted, hidden, unloaded=False):
    rows = listing_rows(snapshot)
    for name in visible_fixtures(fixtures, selected, hidden):
        matches = [row for row in rows if row["name"] == name]
        if len(matches) != 1:
            raise AssertionError(f"listing row for {name}: {len(matches)} rows")
        detail = expected_size(fixtures, name)
        if matches[0]["detail"] != detail:
            raise AssertionError(f"listing size for {name}: {matches[0]['detail']!r}, expected {detail!r}")
    if any(row["detail"] and DATE.search(row["detail"]) for row in rows):
        raise AssertionError("the Size column shows a date")
    cursor = [row for row in rows if row["cursor"]]
    if len(cursor) != 1 or cursor[0]["name"] != selected:
        raise AssertionError(f"cursor is on {[row['name'] for row in cursor]!r}, not {selected!r}")
    if len(rows) < 2:
        raise AssertionError("preview pane is absent")
    header, facts = rows[0], rows[1]
    match = re.match(r"^ ?(\S+) (.+?) *$", header["preview"])
    if not match or match[2] != selected:
        raise AssertionError(f"preview header names {header['preview'].strip()!r}, not {selected!r}")
    fields = facts["preview"].strip().split(" · ")
    fixture = fixtures[selected]
    if len(fields) < 2 or fields[1] != expected_size(fixtures, selected) or fixture["directory"] and fields[0] != "Folder":
        raise AssertionError(f"preview facts for {selected}: {facts['preview'].strip()!r}")
    observed = DATE.findall("\n".join(row["preview"] for row in rows))
    if observed != wanted or wanted and not facts["preview"].rstrip().endswith(" · " + wanted[0]):
        raise AssertionError(f"{kind} Modified for {selected}: {observed!r}, expected {wanted!r}")
    hint = re.compile(re.escape(strings["preview.loadHint"]).replace("%1", ".+"))
    waiting = any(hint.search(row["preview"]) for row in rows)
    if kind == "loaded" and waiting:
        raise AssertionError("the picture still waits for its load key")
    if unloaded and not waiting:
        raise AssertionError("the manual preview never waited for its load key")
    dates = []
    if wanted:
        start = facts["preview_start"] + len(facts["preview"].rstrip()) - len(wanted[0])
        dates = span(facts["columns"], start, len(wanted[0]), facts["y"])
    names = span(cursor[0]["columns"], cursor[0]["name_start"], len(selected), cursor[0]["y"])
    return observed, {"date": dates, "name": names}


def properties_view(snapshot, selected, cwd, strings, wanted):
    box = card(snapshot)
    if not box or not box["rows"] or not box["rows"][0][1].strip().startswith(strings["properties.title"]) \
            or not box["rows"][0][1].strip().endswith(strings["card.esc"]):
        raise AssertionError("Properties card is absent")
    text = "\n".join(row[1] for row in box["rows"])
    if strings["properties.loading"] in text:
        raise AssertionError("Properties has not answered for the selected fixture")
    labels = [strings[key] for key in STRINGS if key.startswith("properties.") and key not in ("properties.title", "properties.loading")]
    facts, places = {}, {}
    for y, line, columns in box["rows"][1:]:
        for label in labels:
            match = re.match(r"^( *)(" + re.escape(label) + r") {2,}(\S(?:.*\S)?) *$", line)
            if match:
                facts[label], places[label] = match[3], span(columns, match.start(3), len(match[3]), y)
    path = cwd + "/" + selected
    if facts.get(strings["properties.path"]) != path or strings["properties.permissions"] not in facts:
        raise AssertionError(f"Properties has not answered for {selected}: {facts!r}")
    modified = strings["properties.modified"]
    observed = [facts[modified]] if modified in facts else []
    if DATE.findall(text) != observed or observed != wanted:
        raise AssertionError(f"properties Modified for {selected}: {DATE.findall(text)!r}, expected {wanted!r}")
    return observed, {"date": places.get(modified, []), "name": places[strings["properties.path"]]}, path


def quicklook_view(snapshot, fixtures, selected, wanted, strings):
    box = card(snapshot)
    bar = QUICK_LOOK_BAR.match(box["rows"][0][1].strip()) if box and box["rows"] else None
    if not bar:
        raise AssertionError("Quick Look card is absent")
    if bar[2] != selected:
        raise AssertionError(f"Quick Look names {bar[2]!r}, not {selected!r}")
    text = "\n".join(row[1] for row in box["rows"])
    observed = DATE.findall(text)
    if observed != wanted:
        raise AssertionError(f"Quick Look dates for {selected}: {observed!r}, expected {wanted!r}")
    facts = text
    for name in sorted([selected, *(fixtures[selected].get("children") or [])], key=len, reverse=True):
        facts = facts.replace(name, " ")
    partial = PARTIAL_DATE.findall(facts)
    if strings["properties.modified"] in facts or partial:
        raise AssertionError(f"Quick Look shows a Modified fact or part of a date for {selected}: {partial!r}")
    missing = [child for child in fixtures[selected].get("children") or [] if child not in text]
    if missing:
        raise AssertionError(f"Quick Look folder body lacks {missing!r}")
    y, line, columns = box["rows"][0]
    start = line.index(bar[2], len(line) - len(line.lstrip()))
    return observed, {"date": [], "name": span(columns, start, len(selected), y)}


def view(snapshot, fixtures, selected, kind, cwd, strings, expected=None, hidden=False, unloaded=False):
    """One capture's exact date, selected identity and view predicate on the candidate's cells."""
    if kind not in ("preview", "loaded", "properties", "quicklook"):
        raise AssertionError("unknown calendar view: " + str(kind))
    if selected not in fixtures:
        raise AssertionError("selected fixture is unknown: " + str(selected))
    modified = fixtures[selected]["modified"]
    wanted = list(expected) if expected is not None else ([] if kind == "quicklook" or modified is None else [modified])
    receipt = {"name": selected, "state": kind, "expected": wanted}
    if kind in ("preview", "loaded"):
        observed, cells = preview_view(snapshot, fixtures, selected, kind, strings, wanted, hidden, unloaded)
    elif kind == "properties":
        observed, cells, receipt["path"] = properties_view(snapshot, selected, cwd, strings, wanted)
    else:
        observed, cells = quicklook_view(snapshot, fixtures, selected, wanted, strings)
    receipt.update(observed=observed, cells=cells)
    return receipt


def detail_rows(snapshot, fixtures, column, hidden=True):
    """The detail column after I: its status-row notice and every fixture row's value in that column."""
    notice = NOTICE.format(column)
    status, _ = mapped(snapshot["grid"][-1])
    if notice not in status:
        raise AssertionError(f"status row lacks {notice!r}")
    rows = {row["name"]: row for row in listing_rows(snapshot) if row["name"]}
    values, dates = {}, []
    for name in visible_fixtures(fixtures, None, hidden):
        row = rows.get(name)
        if not row:
            raise AssertionError(f"detail column lacks the row for {name}")
        value, fixture = row["detail"], fixtures[name]
        good = {"Modified": value == fixture["modified"], "Mode": bool(value and re.fullmatch(r"0[0-7]{3}", value)),
                "Owner": not (value and DATE.search(value)),
                "Size": value == expected_size(fixtures, name)}[column]
        if not good:
            raise AssertionError(f"{column} column for {name}: {value!r}")
        values[name] = value
        if column == "Modified" and value:
            dates += span(row["columns"], row["detail_start"], len(value), row["y"])
    return {"notice": notice, "rows": values, "cells": dates}


def notice_window(send_ns, observations, column):
    """The notice shows from the key until four seconds have passed, and clears within the poll slack after."""
    label = NOTICE.format(column)
    shown = [ns for ns, visible in observations if visible]
    if not shown:
        raise Blocked(label + " was never shown")
    gone = [ns for ns, visible in observations if not visible and ns > shown[0]]
    if not gone:
        raise Blocked(label + " never cleared")
    last = max(ns for ns in shown if ns < gone[0])
    seconds = lambda ns: (ns - send_ns) / 1e9
    if seconds(gone[0]) < NOTICE_SECONDS:
        raise Blocked(f"{label} cleared {seconds(gone[0]):.2f} s after the key, before four seconds")
    if seconds(last) < NOTICE_LAST_SHOWN:
        raise Blocked(f"{label} was last shown {seconds(last):.2f} s after the key")
    if seconds(gone[0]) > NOTICE_LATEST:
        raise Blocked(f"{label} still showed {seconds(gone[0]):.2f} s after the key")
    return {"notice": label, "last_shown_seconds": seconds(last), "gone_after_seconds": seconds(gone[0])}


def role_inks(boards, strings):
    """The approved inks of a preview date, a Properties date and a Quick Look name, read from the HIG boards."""
    def board(name):
        cells = Board(Path(boards) / "boards" / (name + ".html")).cells()
        return {"grid": [[dict(cells[(x, y)], reverse=False) for x in range(COLUMNS)] for y in range(ROWS)]}

    def one(snapshot, places, role):
        inks = {snapshot["grid"][y][x]["fg"] for x, y in places}
        if len(inks) != 1:
            raise Blocked(f"board ink for {role} is ambiguous: {sorted(inks)}")
        return inks.pop()
    text = board("tui-list-text")
    facts = listing_rows(text)[1]
    found = DATE.search(facts["preview"])
    preview = span(facts["columns"], facts["preview_start"] + found.start(), len(found[0]), facts["y"])
    sheet = board("tui-properties")
    box = card(sheet)
    places = next(span(columns, DATE.search(line).start(), 16, y) for y, line, columns in box["rows"] if DATE.search(line))
    look = board("tui-quicklook-image")
    y, line, columns = card(look)["rows"][0]
    bar = QUICK_LOOK_BAR.match(line.strip())
    name = span(columns, line.index(bar[2]), len(bar[2]), y)
    return {"preview": one(text, preview, "preview"), "properties": one(sheet, places, "properties"),
            "quicklook": one(look, name, "quicklook")}


def color_status(snapshot, places, ink):
    return "PASS" if all(snapshot["grid"][y][x].get("fg") == ink for x, y in places) else "FAIL"


def grid_points(cells, origin, pty):
    """Each observed cell's centre in capture pixels, from the PTY's own cell metrics and the calibrated grid origin."""
    rows, columns, width, height = pty
    cw, ch = width // columns, height // rows
    points = []
    for x, y in cells:
        if not (0 <= x < columns and 0 <= y < rows):
            raise Blocked(f"calendar cell {(x, y)!r} outside the PTY grid {columns}x{rows}")
        points.append((origin[0] + x * cw + cw // 2, origin[1] + y * ch + ch // 2))
    return points


def observed_canvas(image, client, snapshot, points):
    """Resolve the actual grid before accepting any observed pixel coordinate."""
    pty = snapshot.get("pty")
    grid = snapshot.get("grid")
    if (not isinstance(pty, (tuple, list)) or len(pty) != 4
            or any(type(value) is not int or value <= 0 for value in pty)
            or snapshot.get("rows") != pty[0] or snapshot.get("columns") != pty[1]
            or pty[2] % pty[1] or pty[3] % pty[0]
            or not isinstance(grid, list) or len(grid) != pty[0]
            or any(not isinstance(row, list) or len(row) != pty[1] for row in grid)
            or snapshot.get("unsupported") or snapshot.get("pending")):
        raise Blocked("calendar cell grid is incomplete or has invalid PTY metrics")
    rect = separator(image, client, pty, grid)
    left, top, width, height = rect
    for point in points:
        if (not isinstance(point, (tuple, list)) or len(point) != 2
                or any(type(value) is not int for value in point)
                or not left <= point[0] < left + width or not top <= point[1] < top + height):
            raise Blocked(f"calendar observation {point!r} outside measured grid {rect}")
    return rect


def candidate_receipt(expected, observed, terminal_exit, shutdown):
    """Check candidate provenance and the actual native/relay process shutdown receipts."""
    for field in IDENTITY_FIELDS:
        if field not in expected or field not in observed or observed[field] != expected[field]:
            raise Blocked("calendar candidate identity mismatch: " + field)
    failure = shutdown_failure(terminal_exit, shutdown)
    if failure:
        raise Blocked("calendar " + failure)
    return {"selected_candidate_identity": {key: observed[key] for key in IDENTITY_FIELDS},
            "clean-quit": True}


def refused(check, errors=(Blocked, AssertionError)):
    try:
        check()
    except Cancelled:
        raise
    except errors:
        return True
    return False


def session_negatives(identity, terminal_exit, shutdown):
    """After a clean quit: a changed binary, a signalled candidate and early or late notices must each be refused."""
    second = 1000000000
    return {"wrong-identity": refused(lambda: candidate_receipt(identity, dict(identity, elf_sha256="other"), terminal_exit, shutdown)),
            "dirty-quit": refused(lambda: candidate_receipt(identity, identity, terminal_exit, dict(shutdown, candidate_exit=-15))),
            "early-notice": refused(lambda: notice_window(0, [(second // 5, True), (3 * second, False)], "Modified")),
            "late-notice": refused(lambda: notice_window(0, [(second // 5, True), (6 * second, True), (7 * second, False)], "Modified"))}


def capture_negatives(snapshot, fixtures, selected, kind, cwd, strings, image, client, points, hidden, unloaded=False):
    """After a positive capture: a shifted date, another selected file and a shifted grid must each be refused."""
    fixture = fixtures[selected]
    shifted = "2000-01-01 00:00" if fixture["modified"] is None else (datetime.datetime.strptime(
        fixture["modified"], "%Y-%m-%d %H:%M") + datetime.timedelta(minutes=1)).strftime("%Y-%m-%d %H:%M")
    other = next(name for name in sorted(visible_fixtures(fixtures, selected, hidden)) if name != selected)
    wrong_date = ([shifted] if kind == "quicklook" else None)
    return {"wrong-date": refused(lambda: view(snapshot, dict(fixtures, **{selected: dict(fixture, modified=shifted)}),
                                               selected, kind, cwd, strings, wrong_date, hidden, unloaded)),
            "wrong-selected": refused(lambda: view(snapshot, fixtures, other, kind, cwd, strings, None, hidden, unloaded)),
            "shifted-grid": refused(lambda: observed_canvas(image, client, dict(snapshot, grid=[snapshot["grid"][0]] + snapshot["grid"][:-1]), points))}


class Cancelled(Blocked):
    """A SIGTERM, SIGHUP or SIGINT that stops the run through each session's own cleanup."""


CANCEL_SIGNALS = (signal.SIGTERM, signal.SIGHUP, signal.SIGINT)


@contextmanager
def shielded(signals=CANCEL_SIGNALS):
    """Holds cancellation until the block ends, then raises it there, so bounded termination in the block always finishes."""
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, signals)
    try:
        yield
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)


@contextmanager
def cancellable(signals=CANCEL_SIGNALS):
    previous = {number: signal.getsignal(number) for number in signals}

    fired = []

    # Later signals are recorded, never raised, and no disposition becomes SIG_IGN, so no child inherits an ignored signal.
    def handler(number, frame):
        fired.append(signal.Signals(number).name)
        if len(fired) == 1:
            raise Cancelled("cancelled by signal " + fired[0])
    for number in signals:
        signal.signal(number, handler)
    try:
        yield
    finally:
        for number, old in previous.items():
            signal.signal(number, old)


def run_sessions(terminals, start):
    """One session per terminal; a cancellation or a lost display blocks every later terminal without starting it."""
    sessions = []
    for name in terminals:
        if any(entry.get("cancelled") for entry in sessions):
            sessions.append({"terminal": name, "status": "BLOCKED", "reason": "cancelled before this terminal", "directories": []})
        elif any(entry.get("display_lost") for entry in sessions):
            sessions.append({"terminal": name, "status": "BLOCKED", "reason": "display lost before this terminal", "directories": []})
        else:
            sessions.append(start(name))
    return sessions


class Stages:
    """Names the setup step a refusal came from, so a fontconfig, terminal-config or grid refusal never hides another."""
    def __init__(self):
        self.failed = None

    @contextmanager
    def __call__(self, name):
        try:
            yield
        except (Blocked, AssertionError, OSError, ValueError, KeyError) as error:
            self.failed = self.failed or name
            kind = type(error) if isinstance(error, Blocked) else Blocked
            raise kind(f"{name}: {error}") from error


def output_bundle(path):
    return output_root(path)


def tint(pixels, width, rect, colour, weight):
    x, y, w, h = rect
    for row in range(y, y + h):
        for column in range(x, x + w):
            i = (row * width + column) * 4
            pixels[i:i + 3] = bytes(int(pixels[i + c] * (1 - weight) + colour[c] * weight) for c in range(3))


def write_artifacts(directory, capture):
    """The standard capture artifacts beside native.png, which the native loop writes first."""
    image = capture.get("image") or png.read(directory / "native.png")
    width, height, pixels = png.crop(image, capture["rect"])
    png.write(directory / "grid-crop.png", width, height, pixels)
    rows, columns, pw, ph = capture["snapshot"]["pty"]
    cw, ch = pw // columns, ph // rows
    overlay, mask = bytearray(pixels), bytearray(b"\0\0\0\xff" * width * height)
    for role, colour in (("date", (255, 0, 255)), ("name", (0, 255, 255))):
        for x, y in capture["receipt"]["cells"].get(role, []):
            cell = (x * cw, y * ch, cw, ch)
            if x * cw + cw > width or y * ch + ch > height:
                raise Blocked(f"observed cell {(x, y)!r} lies outside the grid crop")
            tint(overlay, width, cell, colour, .5)
            tint(mask, width, cell, colour, 1)
    png.write(directory / "overlay.png", width, height, overlay)
    png.write(directory / "diff.png", width, height, mask)
    with (directory / "pty.log").open("xb") as output:
        output.write(capture["pty_log"])
    for name, key in (("cells.json", "snapshot"), ("capabilities.json", "capabilities"), ("state.json", "state"),
                      ("commands.json", "commands"), ("profile.json", "profile")):
        save(directory / name, capture[key])
    save(directory / "observation.json", {key: capture.get(key) for key in
                                          ("terminal", "label", "selected", "kind", "receipt", "color", "negatives",
                                           "assertions", "cell_geometry", "rect", "client", "picture")})


def review_status(directory, terminal, label, reviews):
    record = reviews.get(f"{terminal}/{label}")
    if not record:
        return "PENDING"
    return "PASS" if record.get("capture_sha256") == leaf_digest(directory / "native.png") and record.get("approved") is True else "FAIL"


def write_verdict(directory, capture, session, reviews):
    negatives = capture.get("negatives") or {}
    assertions = {key: capture["assertions"].get(key) is True for key in ("exact-date", "selected-identity", "view-predicate")}
    assertions.update({"clean-quit": session.get("clean-quit") is True, "detail-notice": session.get("detail-notice") is True,
                       "session-negatives": session.get("session-negatives") is True,
                       "after-proof-negatives": bool(negatives) and all(value is True for value in negatives.values())})
    verdict = {"terminal": capture["terminal"], "kind": "calendar", "id": capture["label"], "selected": capture["selected"],
               "view": capture["kind"], "behavior": "PASS" if all(assertions.values()) else "FAIL",
               "cell_geometry": "PASS" if capture.get("cell_geometry") == "PASS" else "FAIL",
               "color": "PASS" if capture.get("color") == "PASS" else "FAIL",
               "native_visual": review_status(directory, capture["terminal"], capture["label"], reviews),
               "assertions": assertions, "receipt": capture["receipt"], "negatives": negatives,
               "original_driver_sha256": ORIGINAL_DRIVER_SHA256, "original_manifest_sha256": ORIGINAL_MANIFEST_SHA256,
               "adapted_driver_sha256": digest(Path(__file__)), "native_capture_sha256": digest(directory / "native.png"),
               "synthetic": capture.get("synthetic") is not False, "selected_candidate_identity": {key: session["identity"][key] for key in IDENTITY_FIELDS}}
    verdict["status"] = status(verdict)
    write_leaf(directory / "verdict.json", (json.dumps(verdict, ensure_ascii=False, indent=2) + "\n").encode())
    return verdict


def status(verdict):
    gates = all(verdict.get(key) == "PASS" for key in ("behavior", "cell_geometry", "color", "native_visual"))
    return "PASS" if gates and verdict.get("synthetic") is False else "BLOCKED"


def roster(terminals):
    """Every terminal/label pair a complete calendar run must hold, in contract order."""
    if not terminals or len(set(terminals)) != len(terminals) or any(name not in TERMINALS for name in terminals):
        raise Blocked("calendar terminals contain unknown or duplicate values")
    return [f"{terminal}/{label}" for terminal in terminals for label in contract()["states"]]


def acceptance(manifest, run):
    """Accepted only when the records are exactly the roster and every sealed verdict passes."""
    wanted = roster(manifest["terminals"])
    held = [f"{row['terminal']}/{row['id']}" for row in manifest["records"]]
    manifest["missing"] = [key for key in wanted if key not in held]
    manifest["unexpected"] = [key for key in held if key not in wanted or held.count(key) > 1]
    passing = all(json.loads(read_leaf(run / row["directory"] / "verdict.json"))["status"] == "PASS"
                  for row in manifest["records"])
    manifest["native_acceptance"] = bool(held) and not manifest["missing"] and not manifest["unexpected"] and passing
    return manifest


def seal(root, identity, directories, terminals):
    """The identity-matched --supplement manifest over every record that reached a verdict."""
    root = absolute(root)
    records = []
    for directory in directories:
        relative = directory.relative_to(root)
        if not (directory / "verdict.json").is_file():
            continue
        records.append({"terminal": relative.parts[0], "kind": "calendar", "id": relative.parts[1], "directory": str(relative),
                        "sha256": {name: digest(directory / name) for name in REQUIRED if (directory / name).is_file()},
                        "verdict_sha256": digest(directory / "verdict.json")})
    manifest = {"schema": 1, "kind": "calendar-supplement", "identity": {key: identity[key] for key in IDENTITY_FIELDS},
                "original_driver_sha256": ORIGINAL_DRIVER_SHA256, "original_manifest_sha256": ORIGINAL_MANIFEST_SHA256,
                "adapted_driver_sha256": digest(Path(__file__)), "terminals": list(terminals), "records": records}
    acceptance(manifest, root)
    write_leaf(root / "manifest.json", (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode())
    return manifest


def regular(path):
    """A leaf file that is neither a link nor a special file, checked without following it."""
    try:
        info = os.lstat(path)
    except FileNotFoundError:
        raise Blocked(f"missing calendar file: {path}") from None
    if not stat.S_ISREG(info.st_mode):
        raise Blocked(f"linked or non-regular calendar file: {path}")
    return path


def read_leaf(path):
    descriptor = os.open(regular(path), os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise Blocked(f"linked or non-regular calendar file: {path}")
        return source.read()


def write_leaf(path, data):
    if os.path.lexists(path):
        regular(path)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o644)
    with os.fdopen(descriptor, "wb") as output:
        output.write(data)


def leaf_digest(path):
    return hashlib.sha256(read_leaf(path)).hexdigest()


def record_directory(run, row):
    """A record's directory: relative, exactly terminal/label, inside the marked run, with no link on the way."""
    relative = Path(row.get("directory") or "")
    if not row.get("directory") or relative.is_absolute() or ".." in relative.parts:
        raise Blocked("record directory escaped the run: " + repr(row.get("directory")))
    if relative.parts != (row.get("terminal"), row.get("id")):
        raise Blocked(f"record directory names another record: {row.get('directory')!r} for {row.get('terminal')}/{row.get('id')}")
    directory = absolute(run / relative)
    if not directory.is_relative_to(run) or directory == run:
        raise Blocked("record directory escaped the run: " + repr(row.get("directory")))
    return directory


def apply_reviews(run, reviews):
    """Applies the controller's native visual decisions to sealed records whose artifacts and verdicts are unchanged."""
    run = Owned(run).path
    manifest = json.loads(read_leaf(run / "manifest.json"))
    checked = []
    for row in manifest["records"]:
        directory = record_directory(run, row)
        for name in REQUIRED:
            if leaf_digest(directory / name) != row["sha256"].get(name):
                raise Blocked(f"retained calendar artifact changed: {row['directory']}/{name}")
        if leaf_digest(directory / "verdict.json") != row.get("verdict_sha256"):
            raise Blocked(f"sealed verdict changed: {row['directory']}/verdict.json")
        checked.append((row, directory))
    for row, directory in checked:
        verdict = json.loads(read_leaf(directory / "verdict.json"))
        verdict["native_visual"] = review_status(directory, row["terminal"], row["id"], reviews)
        verdict["status"] = status(verdict)
        write_leaf(directory / "verdict.json", (json.dumps(verdict, ensure_ascii=False, indent=2) + "\n").encode())
        row["verdict_sha256"] = leaf_digest(directory / "verdict.json")
    acceptance(manifest, run)
    write_leaf(run / "manifest.json", (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode())
    return manifest


def settle_view(terminal, check, timeout, description):
    """The first frame the check accepts that stays unchanged for the settle interval; the last refusal names the cause."""
    last, since, refusal = None, time.monotonic(), ["no frame"]

    def attempt():
        nonlocal last, since
        snapshot = terminal.snapshot()
        if snapshot.get("pending") or snapshot.get("unsupported"):
            refusal[0] = "incomplete cell grid"
            return None
        try:
            receipt = check(snapshot)
        except AssertionError as error:
            refusal[0], last, since = str(error), None, time.monotonic()
            return None
        identity = json.dumps(snapshot["grid"], sort_keys=True)
        if identity != last:
            last, since = identity, time.monotonic()
        if time.monotonic() - since >= SETTLE_STABLE_SECONDS:
            return snapshot, receipt
    try:
        return wait_for(attempt, timeout, description)
    except Deadline:
        raise Blocked(f"{description} never held: {refusal[0]}") from None


def argv_environment(argv, key):
    for i, value in enumerate(argv[:-2]):
        if value == "--setenv" and argv[i + 1] == key:
            return argv[i + 2]
    raise Blocked(f"candidate namespace sets no {key}")


def build_fx(case):
    """A marked tmpfs root holding the candidate's private /fx, as the battery's fixture builder lays it out."""
    Owned(case)
    root = Path(tempfile.mkdtemp(prefix="flea-calendar-fx.", dir=fixture_tree.TMPFS))
    (root / MARKER).write_text(f"calendar fixture {os.getpid()}\n")
    fx = Owned(root).directory("fx")
    (fx / "v").mkdir()
    for name in ("home", "config", "data", "state", "cache", "runtime", "tmp", "tools"):
        (fx / name).mkdir(mode=0o700)
    return fx


def cursor_on(snapshot, name):
    return [row["name"] for row in listing_rows(snapshot) if row["cursor"]] == [name]


def select(terminal, name, hidden, fixtures, keymap, journal, timeout):
    order = sorted(visible_fixtures(fixtures, None, hidden), key=lambda key: (not fixtures[key]["directory"], key.lower(), key))
    send(terminal, keymap.resolve("cursorFirst"), journal)
    for _ in range(order.index(name)):
        send(terminal, keymap.resolve("cursorDown"), journal)
    wait_for(lambda: cursor_on(terminal.snapshot(), name), timeout, "cursor on " + name)


def picture(log, offset, snapshot):
    """Which picture path the candidate took after the load key: graphics bytes, half-blocks or facts alone."""
    data = bytearray()
    for line in log.read_bytes()[offset:].splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if row.get("direction") == "output":
            data += base64.b64decode(row["base64"])
    if b"\x1b_G" in data:
        return "kitty"
    if re.search(rb"\x1bP[0-9;]*q", data):
        return "sixel"
    return "half-block" if any(cell["ch"] == "▀" for row in snapshot["grid"] for cell in row) else "facts"


def detail_cycle(terminal, fixtures, keymap, journal, timeout):
    receipts = {}
    for column in DETAIL_COLUMNS:
        send_ns = time.monotonic_ns()
        send(terminal, keymap.resolve("detailNext"), journal)
        _, receipt = settle_view(terminal, lambda snapshot: detail_rows(snapshot, fixtures, column), timeout, "detail column " + column)
        if column == "Modified":
            observations, deadline = [], send_ns + 8 * 1000000000
            while time.monotonic_ns() < deadline:
                before = time.monotonic_ns()
                visible = NOTICE.format(column) in mapped(terminal.snapshot()["grid"][-1])[0]
                observations.append((before if visible else time.monotonic_ns(), visible))
                if not visible:
                    break
                time.sleep(NOTICE_POLL)
            receipt["window"] = notice_window(send_ns, observations, column)
        receipts[column] = receipt
    return receipts


def capture_state(context, step):
    terminal, display, case = context["terminal"], context["display"], context["case"]
    directory = context["root"].directory(f"{context['name']}/{step['label']}")
    selected, kind, fixtures = step["name"], step["kind"], context["fixtures"]
    hidden = context["hidden"]
    unloaded = bool(step.get("unloaded"))
    check = lambda snapshot: view(snapshot, fixtures, selected, kind, CWD, context["strings"], None, hidden, unloaded)
    try:
        snapshot, receipt = settle_view(terminal, check, context["timeout"], step["label"])
        image = display.capture(directory / "native.png")
        if terminal.snapshot()["grid"] != snapshot["grid"]:
            raise Blocked("screen changed between the predicate and the capture")
    except Blocked:
        try:
            display.capture(directory / "failure.png")
            save(directory / "last-cells.json", terminal.snapshot())
        except (Blocked, OSError, ValueError) as error:
            (directory / "failure-capture.txt").write_text(str(error) + "\n")
        raise
    client = display.client(terminal.adapter.identity)
    ox, oy = display.monitor["x"], display.monitor["y"]
    rect = [client["at"][0] - ox, client["at"][1] - oy, *client["size"]]
    padding = context["calibration"]["origin_padding"]
    points = grid_points(receipt["cells"]["date"] + receipt["cells"]["name"], (rect[0] + padding[0], rect[1] + padding[1]), snapshot["pty"])
    grid = observed_canvas(image, rect, snapshot, points)
    role = {"preview": "preview", "loaded": "preview", "properties": "properties", "quicklook": "quicklook"}[kind]
    places = receipt["cells"]["name"] if kind == "quicklook" else receipt["cells"]["date"]
    capture = {"terminal": context["name"], "label": step["label"], "selected": selected, "kind": kind, "snapshot": snapshot, "image": image,
               "rect": grid, "client": rect, "pty_log": (case / "pty.log").read_bytes(),
               "capabilities": capabilities(case / "pty.log"), "commands": context["commands"].records,
               "state": {"label": step["label"], "selected": selected, "view": kind, "cwd": CWD, "hidden": hidden,
                         "fixture": fixtures[selected], "input": list(context["journal"]), "preset": context["preset"]},
               "profile": context["profile"], "receipt": receipt, "color": color_status(snapshot, places, context["inks"][role]),
               "negatives": capture_negatives(snapshot, fixtures, selected, kind, CWD, context["strings"], image, rect, points, hidden, unloaded),
               "assertions": {"exact-date": True, "selected-identity": True, "view-predicate": True}, "cell_geometry": "PASS",
               "synthetic": False}
    if kind == "loaded":
        capture["picture"] = picture(case / "pty.log", context["offset"], snapshot)
    write_artifacts(directory, capture)
    return directory, capture


TERM_GRACE, KILL_GRACE = 2.0, 3.0
START_TOLERANCE_NS = 1_000_000_000
PIDFD_SIGNAL_PROCESS_GROUP = 4  # linux/pidfd.h, Linux 6.9+: deliver to the process group that the pidfd's process leads


def group_members(group, proc=Path("/proc")):
    """Live (non-zombie) processes in a process group, from /proc."""
    members = []
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
        except (OSError, IndexError):
            continue
        if int(fields[2]) == group and fields[0] not in "ZX":
            members.append(int(entry.name))
    return members


def process_start_ns(pid, proc=Path("/proc")):
    """Wall-clock start of a live process, from its start tick since boot and the current boot clock."""
    fields = (proc / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()
    # starttime is field 22, index 19 once the comm field is cut off.
    boot = time.time_ns() - time.clock_gettime_ns(time.CLOCK_BOOTTIME)
    return boot + int(fields[19]) * 1_000_000_000 // os.sysconf("SC_CLK_TCK")


def hold_group(case):
    """Pins the launched terminal leader with a pidfd, or returns None when the launch record does not name it."""
    start = case / "launch-start.json"
    if not start.is_file():
        return None
    record = json.loads(read_leaf(start))
    pid, started = record.get("pid"), record.get("start_ns")
    if type(pid) is not int or pid <= 1 or pid == os.getpgrp():
        raise Blocked("invalid terminal process group in " + str(start))
    try:
        anchor = os.pidfd_open(pid)
    except OSError:
        return None
    # The pidfd is open before the check, so the pid number cannot be reissued between the two.
    try:
        same = type(started) is int and abs(process_start_ns(pid) - started) <= START_TOLERANCE_NS
    except OSError:
        same = False
    if same:
        return anchor
    os.close(anchor)
    return None


def group_signal(anchor, number):
    """Signals the process group the anchor names; False when that group has no task left."""
    try:
        signal.pidfd_send_signal(anchor, number, None, PIDFD_SIGNAL_PROCESS_GROUP)
    except ProcessLookupError:
        return False
    return True


def group_live(anchor, pid):
    """True while the anchored group has a non-zombie member: the pidfd names the group, /proc reads its state."""
    # The group still exists, so its number is still held by it and the /proc scan cannot match another group.
    return group_signal(anchor, 0) and bool(group_members(pid))


def escalate(record, pid, anchor):
    """SIGTERM then SIGKILL through the anchor, each with its grace; without an anchor nothing is sent."""
    if anchor is None:
        record["live"] = bool(group_members(pid))
        if record["live"]:
            record["refused"] = "group not proven to be this case's, nothing sent"
        return
    try:
        for number, grace in ((signal.SIGTERM, TERM_GRACE), (signal.SIGKILL, KILL_GRACE)):
            if not group_live(anchor, pid) or not group_signal(anchor, number):
                break
            record["signals"].append(number.name)
            deadline = time.monotonic() + grace
            while group_live(anchor, pid) and time.monotonic() < deadline:
                time.sleep(.05)
        record["live"] = group_live(anchor, pid)
    except OSError as error:
        record["refused"] = f"BLOCKED: pidfd group signal failed ({error.strerror}), no numeric signal sent"
        record["live"] = True


def reap_group(case, anchor):
    """Ends the terminal's own group through its pidfd; a numeric process group id is never signalled."""
    record = {"signals": [], "live": False, "pid": None}
    try:
        with shielded():
            start = case / "launch-start.json"
            if start.is_file():
                pid = json.loads(read_leaf(start)).get("pid")
                if type(pid) is not int or pid <= 1 or pid == os.getpgrp():
                    raise Blocked("invalid terminal process group in " + str(start))
                record["pid"] = pid
                escalate(record, pid, anchor)
    except Cancelled as error:
        record["cancel"] = str(error)
    return record


def cleanup(result, key, step):
    """One cleanup step: a cancellation is kept and the rest of cleanup still runs; any other refusal is recorded."""
    try:
        return step()
    except Cancelled as error:
        result.update(cancelled=True, cancel_reason=str(error))
    except (Blocked, OSError, ValueError, KeyError) as error:
        result[key] = str(error)
    return None


def session(args, root, display, name, colors, identity, plan, strings_, inks, keymap, reviews, profiles):
    case = root.directory(f"{name}/session")
    (case / MARKER).write_text(uuid.uuid4().hex + "\n")
    stages, commands, journal, captures = Stages(), Commands(), [], []
    result = {"terminal": name, "status": "BLOCKED", "directories": []}
    terminal = fx = None
    context = {"root": root, "name": name, "display": display, "case": case, "commands": commands, "journal": journal,
               "strings": strings_, "inks": inks, "timeout": args.timeout, "preset": args.preset, "hidden": False, "offset": 0}
    try:
        with stages("terminal-config"):
            adapter = Adapter(name, case, owned_identity(), colors, profiles.get(name))
            packages = adapter.preflight(commands)
            profile = adapter.configure()
            profile["config_check"] = adapter.validate(commands)
        with stages("fontconfig"):
            from .driver import font_identity, private_theme
            fonts = font_identity(commands, profile["font"], adapter.environment())
        with stages("fixtures"):
            fx = build_fx(case)
            candidate = fixture_tree.namespace(args.binary, fx, CWD, {})
            zone = argv_environment(candidate, "TZ")
            fixtures = context["fixtures"] = build_fixtures(fx / CWD.removeprefix("/fx/"), zone, plan)
            save(case / "calendar-fixtures.json", {"zone": zone, "fixtures": fixtures})
            theme = private_theme(fx, colors)
            settings = fx / "state/flea/ui.json"
            settings.parent.mkdir(parents=True, exist_ok=True)
            settings.write_text(json.dumps(candidate_settings(args.preset)) + "\n")
        with stages("launch"):
            terminal = context["terminal"] = Terminal(display, adapter, case, commands, args.timeout, keymap.resolve("quit"))
            terminal.launch(candidate, dict(PATH="/usr/bin:/bin", LANG="C.UTF-8", PYTHONPATH=str(REPO / "tests")))
            initial = settled(terminal, {"contains": [plan["cases"][0][0]]}, args.timeout)
            rows, columns, width, height = initial["pty"]
            if (rows, columns) != (ROWS, COLUMNS) or not width or not height:
                raise Blocked(f"PTY reports {columns}x{rows} cells and {width}x{height} px, not {COLUMNS}x{ROWS} with pixel metrics")
        with stages("grid"):
            image = display.capture(case / "calibration.png")
            client = terminal.client
            ox, oy = display.monitor["x"], display.monitor["y"]
            client_rect = [client["at"][0] - ox, client["at"][1] - oy, *client["size"]]
            rect = separator(image, client_rect, initial["pty"], initial["grid"])
            context["calibration"] = {"cell": [width // columns, height // rows], "client": client_rect, "grid": rect,
                                      "origin_padding": [rect[0] - client_rect[0], rect[1] - client_rect[1]], "pty": initial["pty"]}
            context["profile"] = dict(profile, terminal=name, calibration=context["calibration"], fonts=fonts,
                                      packages=packages, theme_sha256=theme, zone=zone)
        for step in schedule(plan):
            if step["op"] == "select":
                with stages("select " + step["name"]):
                    context["hidden"] = context["hidden"] or step["hidden"]
                    select(terminal, step["name"], step["hidden"], fixtures, keymap, journal, args.timeout)
            elif step["op"] == "action":
                with stages("input " + step["action"]):
                    context["offset"] = (case / "pty.log").stat().st_size
                    send(terminal, keymap.resolve(step["action"]), journal)
            elif step["op"] == "key":
                with stages("input " + step["key"]):
                    send(terminal, {"key": step["key"]}, journal)
            else:
                with stages(step["label"]):
                    captures.append(capture_state(context, step))
        with stages("detail-cycle"):
            result["detail"] = detail_cycle(terminal, fixtures, keymap, journal, args.timeout)
    except Blocked as error:
        result["reason"] = str(error)
        if isinstance(error, DisplayLost):
            result["display_lost"] = True
        if isinstance(error, Cancelled):
            result.update(cancelled=True, cancel_reason=str(error))
    finally:
        quit = {"clean-quit": False, "detail-notice": "detail" in result, "session-negatives": False, "identity": identity}
        if terminal and terminal.launched:
            anchor = None
            try:
                # Held from the anchor through the reap: a cancellation waits, so the group's termination always finishes.
                with shielded():
                    # The anchor must open while the leader is alive; it is held until the group is reaped.
                    anchor = cleanup(result, "anchor_reason", lambda: hold_group(case))
                    def close():
                        terminal_exit = terminal.close()
                        shutdown = json.loads((case / "relay-exit.json").read_text())
                        observed = source_identity(REPO, args.binary, args.boards, Commands())
                        result["quit"] = candidate_receipt(identity, observed, terminal_exit, shutdown)
                        result["session_negatives"] = session_negatives(identity, terminal_exit, shutdown)
                        return all(result["session_negatives"].values())
                    negatives = cleanup(result, "quit_reason", close)
                    reap = result["reap"] = cleanup(result, "reap_reason", lambda: reap_group(case, anchor)) or {"signals": [], "live": True}
                    if reap["signals"] or reap["live"]:
                        refused = f", {reap['refused']}" if "refused" in reap else ""
                        result["quit_reason"] = "; ".join(filter(None, [result.get("quit_reason"),
                            f"terminal process group outlived its close: sent {reap['signals']}, live afterwards {reap['live']}{refused}"]))
                    elif negatives is not None and "quit_reason" not in result:
                        quit.update({"clean-quit": True, "session-negatives": negatives})
            except Cancelled as error:
                result.update(cancelled=True, cancel_reason=str(error))
                quit["clean-quit"] = False
            finally:
                if anchor is not None:
                    os.close(anchor)
        if fx:
            cleanup(result, "fixture_retain_reason", lambda: fixture_tree.retain(fx, case))
        save(case / "commands.json", commands.records)
        save(case / "input.json", journal)
        for directory, capture in captures:
            write_verdict(directory, capture, quit, reviews)
            result["directories"].append(str(directory))
        result["stage_failed"] = stages.failed
        if len(captures) == 17 and quit["clean-quit"] and quit["detail-notice"] and "reason" not in result and not result.get("cancelled"):
            result["status"] = "CAPTURED"
    return result


def run(args):
    original_source(args.original_driver)
    plan_ = original_plan()
    terminals = args.terminals.split(",")
    if len(set(terminals)) != len(terminals) or any(name not in TERMINALS for name in terminals):
        raise Blocked("calendar terminals contain unknown or duplicate values")
    if not args.binary or not args.binary.is_absolute() or not os.access(args.binary, os.X_OK):
        raise Blocked("--binary must name an absolute executable candidate")
    with args.binary.open("rb") as candidate:
        if candidate.read(4) != b"\x7fELF":
            raise Blocked("--binary must name the candidate ELF, not a wrapper")
    args.boards = absolute(args.boards)
    root = output_root(args.output)
    commands = Commands()
    identity = source_identity(REPO, args.binary, args.boards, commands)
    save(root.child("identity.json"), identity)
    from .driver import load, palette
    colors, strings_ = palette(args.boards), strings()
    inks = role_inks(args.boards, strings_)
    profiles = load(args.profiles) if args.profiles else {}
    reviews = load(args.reviews) if args.reviews else {}
    keymap = Keymap(REPO / "keys.toml", args.preset)
    save(root.child("preflight.json"), preflight.display(commands))
    with cancellable(), Display(commands, uuid.uuid4().hex) as display:
        sessions = run_sessions(terminals, lambda name: session(args, root, display, name, colors, identity, plan_, strings_,
                                                                 inks, keymap, reviews, profiles))
    directories = [Path(path) for entry in sessions for path in entry["directories"]]
    manifest = seal(root.path, identity, directories, terminals)
    save(root.child("sessions.json"), sessions)
    save(root.child("commands.json"), commands.records)
    complete = manifest["native_acceptance"]
    print(json.dumps({"output": str(root.path), "records": len(manifest["records"]), "native_acceptance": complete,
                      "sessions": [{k: s.get(k) for k in ("terminal", "status", "reason")} for s in sessions]}, ensure_ascii=False))
    return 0 if complete else 2


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true", help="print the 85-record inventory; never native acceptance")
    parser.add_argument("--review", type=Path, help="apply --reviews to a retained calendar run without recapturing")
    parser.add_argument("--original-driver", type=Path, help="retained original date_case.py; must match the embedded bytes")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--boards", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--terminals", default=",".join(TERMINALS))
    parser.add_argument("--preset", choices=("default", "vim", "mac", "windows", "yazi"), default="default")
    parser.add_argument("--profiles", type=Path)
    parser.add_argument("--reviews", type=Path, help="native visual approvals keyed terminal/label with capture_sha256")
    parser.add_argument("--timeout", type=float, default=15)
    args = parser.parse_args(argv)
    try:
        if args.plan:
            provenance = driver_provenance(args.original_driver)
            print(json.dumps({"mode": "inventory", "native_acceptance": False,
                              "provenance": provenance, "records": plan(args.terminals.split(","))}))
            return 0
        if args.review:
            if not args.reviews:
                raise Blocked("--review needs --reviews")
            manifest = apply_reviews(args.review, json.loads(args.reviews.read_text()))
            accepted = manifest["native_acceptance"] and manifest["terminals"] == args.terminals.split(",")
            print(json.dumps({"records": len(manifest["records"]), "terminals": manifest["terminals"],
                              "missing": manifest["missing"], "native_acceptance": accepted}))
            return 0 if accepted else 2
        if not args.boards or not args.output or not 0 < args.timeout <= 120:
            raise Blocked("a native run needs --binary, --boards, --output and a timeout in (0, 120] seconds")
        return run(args)
    except (Blocked, OSError, ValueError, KeyError) as error:
        print("BLOCKED calendar: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
