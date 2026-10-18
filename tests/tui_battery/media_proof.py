"""Prove completed displays and their order against the raw PTY transcript."""
import base64
from bisect import bisect_right
import json
import re

from .evidence import Blocked

KITTY = re.compile(r"\x1b_G([^;\x1b]*)(?:;([^\x1b]*))?\x1b\\")
SIXEL = re.compile(r"\x1bP[0-9;?]*q[^\x1b]*\x1b\\")


def traffic(case):
    output, starts, times = bytearray(), [], []
    for line in (case / "pty.log").read_text().splitlines():
        row = json.loads(line)
        if row["direction"] == "output":
            data = base64.b64decode(row["base64"])
            if data:
                starts.append(len(output))
                times.append(row.get("ns"))
                output.extend(data)
    offset, covered = 0, set()
    for line in (case / "graphics.log").read_text().splitlines():
        row = json.loads(line)
        if not (KITTY.fullmatch(row["data"]) or SIXEL.fullmatch(row["data"])):
            continue
        data = row["data"].encode()
        index = output.find(data, offset)
        if index < 0:
            raise Blocked("graphics control lacks matching raw PTY output")
        offset = index + len(data)
        covered.add(index)
        start, end = (times[bisect_right(starts, p) - 1] for p in (index, offset - 1))
        if type(start) is not int or type(end) is not int:
            raise Blocked("graphics output lacks timestamps")
        yield row["data"], start, end
    for pattern in (KITTY, SIXEL):
        for match in re.finditer(pattern.pattern.encode(), output):
            fields = dict(field.split(b"=", 1) for field in match[1].split(b",") if b"=" in field) if pattern is KITTY else {}
            image = pattern is SIXEL or fields.get(b"a") in (b"T", b"t", b"p") or (b"a" not in fields and b"m" in fields)
            if image and match.start() not in covered:
                raise Blocked("raw image output is missing from the recorded graphics evidence")


def placements(case, received, mode):
    replies = received.get("kitty_replies" if mode == "kitty" else "sixel_replies", [])
    positive = [r["received_ns"] for r in replies if r.get("queried") and
                (r.get("reply") == "OK" if mode == "kitty" else r.get("supported"))
                and type(r.get("received_ns")) is int]
    displays, stored, pending = [], set(), None
    for token, start, end in traffic(case):
        match = KITTY.fullmatch(token) if mode == "kitty" else SIXEL.fullmatch(token)
        if not match:
            continue
        if mode == "kitty":
            fields = dict(field.split("=", 1) for field in match[1].split(",") if "=" in field)
            action = fields.get("a")
            if action not in ("T", "t", "p") and not (action is None and pending):
                continue
        earlier = [ns for ns in positive if ns < start]
        if not earlier:
            raise Blocked("image was sent before candidate-received positive negotiation")
        receipt = {"transport": mode, "start_ns": start, "end_ns": end, "positive_ns": min(earlier)}
        if mode == "sixel":
            displays.append(receipt)
            continue
        if action == "p":
            ident = fields.get("i")
            if pending or ident not in stored:
                raise Blocked("Kitty placement has no completed transmission for its image ID")
            displays.append(dict(receipt, image_id=ident, action="p"))
            continue
        if action in ("T", "t"):
            if pending:
                raise Blocked("Kitty image transmission was interrupted")
            pending = dict(receipt, action=action, image_id=fields.get("i", "0"), payload=False)
        pending["payload"] |= bool(match[2])
        if fields.get("m", "0") == "0":
            if not pending["payload"]:
                raise Blocked("Kitty image transmission has no payload")
            stored.add(pending["image_id"])
            if pending["action"] == "T":
                displays.append(dict(pending, end_ns=end))
            pending = None
    if pending or not displays:
        raise Blocked(f"native picture pixels lack completed required {mode} traffic (Kitty placement or sixel)")
    return displays
