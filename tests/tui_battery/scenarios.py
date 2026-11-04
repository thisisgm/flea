"""Bounded state driving and independent evidence verdicts."""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import time

from .cells import predicate, text
from .evidence import Blocked, digest, save
from .keys import bytes_for
from .native import wait_for
from . import relay

SETTLE_STABLE_SECONDS = .12
SETTLE_REFUSAL_SECONDS = .3
LISTING_PREFIX_CELLS = 3  # focus marker, icon and the space after the icon


def settled(terminal, rule, timeout, transient=False):
    last, since = None, time.monotonic()
    def check():
        nonlocal last, since
        snapshot = terminal.snapshot()
        if not predicate(snapshot, rule):
            last, since = None, time.monotonic()
            return None
        identity = json.dumps(snapshot["grid"], sort_keys=True)
        if identity != last:
            last, since = identity, time.monotonic()
        if transient or time.monotonic() - since >= SETTLE_STABLE_SECONDS:
            return snapshot
    return wait_for(check, timeout, "state predicate " + json.dumps(rule, ensure_ascii=False))


def send(terminal, key, journal, replay=False):
    receipt = dict(key, ns=time.time_ns(), delivery="pty-setup" if replay else "compositor")
    if replay:
        data = bytes_for(key["key"], key.get("mods", ""))
        receipt["base64"] = base64.b64encode(data).decode()
        relay.rpc(terminal.case, {"op": "input", "base64": receipt["base64"]})
    else:
        terminal.key(key)
    journal.append(receipt)


# Sample input: pane cells "▎x todo.md   99 B  ", where the name ends at the first run of two spaces.
def listing_name(pane):
    field = "".join(c["ch"] for c in pane)[LISTING_PREFIX_CELLS:]
    return re.split(r"\s{2,}", field, maxsplit=1)[0]


def highlighted(snapshot, name):
    for row in snapshot["grid"][2:-2]:
        string = "".join(c["ch"] for c in row)
        if name not in string:
            continue
        # The middle pane is bounded by real rule cells, not a fixed desktop coordinate.
        separators = [i for i, c in enumerate(row) if c["ch"] == "│"]
        if len(separators) < 2:
            continue
        pane = row[separators[0] + 1:separators[1]]
        if listing_name(pane) == name and any(c["reverse"] or c["ch"] == "▎" for c in pane):
            return True
    return False


def select_file(terminal, name, cwd, fx, keymap, journal, timeout):
    directory = fx / cwd.removeprefix("/fx/")
    rows = sorted(directory.iterdir(), key=lambda p: (not p.is_dir(), p.name.lower(), p.name))
    names = [p.name for p in rows if not p.name.startswith(".")]
    if name not in names:
        raise Blocked(f"selected fixture is absent: {cwd}/{name}")
    send(terminal, keymap.resolve("cursorFirst"), journal)
    for _ in range(names.index(name)):
        send(terminal, keymap.resolve("cursorDown"), journal)
    wait_for(lambda: highlighted(terminal.snapshot(), name), timeout, "cursor on " + name)


def steps(terminal, state, fx, keymap, journal, timeout):
    if state.get("cursor"):
        select_file(terminal, state["cursor"], state["cwd"], fx, keymap, journal, timeout)
    for step in state["steps"]:
        if "action" in step:
            resolved = keymap.resolve(step["action"])
            if step["action"] == "deletePermanently" and (resolved["key"], resolved["mods"]) != ("Delete", "shift"):
                raise Blocked("permanent delete must use legacy Shift+Delete; X stays free")
            send(terminal, resolved, journal)
        elif "key" in step:
            if state["id"] in ("tui-delete", "tui-delete-focus") and step["key"] == "X":
                raise Blocked("X must never drive a permanent-delete state")
            if state.get("picture_steps") and step["key"] == state["picture_steps"]["key"]:
                from .candidate import picture_frame
                def ready():
                    try:
                        return picture_frame(terminal)
                    except Blocked:
                        return None
                before = wait_for(ready, timeout, "3D picture before orbit")
                send(terminal, {"key": step["key"], "mods": step.get("mods", "")}, journal)
                def changed():
                    frame = ready()
                    return frame if frame and frame["sha256"] != before["sha256"] else None
                after = wait_for(changed, timeout, "changed presented 3D picture after orbit")
                journal[-1].update(before_picture=before, after_picture=after)
            else:
                send(terminal, {"key": step["key"], "mods": step.get("mods", "")}, journal)
        elif "text" in step:
            if state["id"] in ("tui-delete", "tui-delete-focus") and "X" in step["text"]:
                raise Blocked("X must never drive a permanent-delete state")
            data = step["text"].encode()
            relay.rpc(terminal.case, {"op": "input", "base64": base64.b64encode(data).decode()})
            journal.append({"text": step["text"], "delivery": "pty-setup", "ns": time.time_ns()})
        elif "select" in step:
            select_file(terminal, step["select"], state["cwd"], fx, keymap, journal, timeout)
        elif "menu" in step:
            def selected():
                snapshot = terminal.snapshot()
                for row in snapshot["grid"]:
                    for start in range(len(row)):
                        if "".join(c["ch"] for c in row[start:]).startswith(step["menu"]):
                            label = row[start:start + len(step["menu"])]
                            return all(c["reverse"] or c["bg"] != terminal.adapter.palette["bg"] for c in label if c["ch"].strip())
                return False
            for _ in range(64):
                if selected():
                    break
                send(terminal, {"key": "Down"}, journal)
                settled(terminal, {"contains": [step["menu"]]}, timeout)
            else:
                raise Blocked("menu action never received focus")
        elif "wait" in step:
            settled(terminal, step["wait"], timeout)
        else:
            raise Blocked("state requires an unavailable controlled I/O hook: " + json.dumps(step))


def file_assertions(state, fx):
    work = fx / "v/work"
    checks = {}
    if state["id"] == "tui-trashed":
        files = fx / "data/Trash/files"
        checks["source_removed"] = not (work / "todo.md").exists()
        checks["private_trash_received_file"] = files.is_dir() and any(p.name.startswith("todo") for p in files.iterdir())
    elif state["id"] == "tui-undone":
        checks["source_restored"] = (work / "todo.md").is_file()
    elif state["id"] == "tui-trash-arm":
        checks["first_key_preserved_source"] = (work / "todo.md").is_file()
    return {"status": "PASS" if all(checks.values()) else "FAIL", "assertions": checks}


def legacy_shift_delete(log):
    observed = bytearray()
    for line in log.read_text().splitlines():
        # Sample input: {"ns": 1, "direction": "terminal-input", "base64": "G1szOzJ+"} per pty.log line.
        row = json.loads(line)
        if row["direction"] == "terminal-input":
            observed.extend(base64.b64decode(row["base64"]))
    if b"\x1b[3;2~" not in observed:
        raise Blocked("native Shift+Delete did not arrive as legacy CSI 3;2~")
    return {"status": "PASS", "expected_hex": "1b5b333b327e", "source": "terminal-input", "synthetic": False}


def visual_review(path, reviews, board_hash):
    key = str(path.parent.relative_to(path.parents[3]))
    record = reviews.get(key)
    sha = digest(path)
    if not record:
        return {"status": "BLOCKED", "reason": "controller native visual review required", "capture_sha256": sha}
    if record.get("capture_sha256") != sha or record.get("board_sha256") != board_hash:
        return {"status": "FAIL", "reason": "visual approval identity is stale", "capture_sha256": sha}
    return {"status": "PASS" if record.get("approved") is True else "FAIL", "review": record}


class FrameSequence:
    """Serves one earlier frame, then the current frame, like a terminal that has moved on."""
    def __init__(self, earlier, current):
        self.pending, self.current = [earlier], current

    def snapshot(self):
        return self.pending.pop() if self.pending else self.current


def negatives():
    from .cells import Screen
    from .protocol import GraphicsNegative, query_ids
    from .references import separator
    results = []
    screen = Screen(20, 4)
    screen.feed(b"\x1b[HREADY")
    if not predicate(screen.snapshot(), {"contains": ["READY"]}) or predicate(screen.snapshot(), {"contains": ["MISSING"]}):
        raise AssertionError("state predicate bypass control failed")
    results.append("missing state predicate refused")
    earlier = screen.snapshot()
    screen.feed(b"\x1b[HSTALE")
    try:
        settled(FrameSequence(earlier, screen.snapshot()), {"contains": ["READY"]}, SETTLE_REFUSAL_SECONDS)
    except Blocked:
        results.append("stale earlier frame refused by settle")
    else:
        raise AssertionError("stale frame bypass control failed")
    blank = (160, 80, bytearray([28, 18, 13, 255]) * 160 * 80)
    try:
        separator(blank, (0, 0, 160, 80), (4, 20, 160, 80))
    except Blocked:
        results.append("missing separator refused")
    else:
        raise AssertionError("blank image bypassed separator assertion")
    negative = GraphicsNegative()
    negative.ids.update(query_ids(b"\x1b_Gi=31,a=q;AAAA\x1b\\"))
    data = b"x\x1b_Gi=30;OK\x1b\\y\x1b_Gi=31;OK\x1b\\\x1b[?1;4c\x1b[?3uZ"
    forwarded = b"".join(negative.feed(data[i:i + 1]) for i in range(len(data))) + negative.flush()
    if forwarded != b"x\x1b_Gi=30;OK\x1b\\y\x1b[?3uZ" or len(negative.dropped) != 2:
        raise AssertionError("graphics-negative bypass control failed")
    results.append("fragmented graphics-negative filter preserves keys, stale IDs and keyboard replies")
    return results
