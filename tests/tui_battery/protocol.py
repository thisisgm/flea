"""Capability receipts use actual queries and replies, never terminal names."""
import base64
from bisect import bisect_right
import json
import re

KITTY_QUERY = re.compile(rb"\x1b_G([^\x1b]*?a=q[^\x1b]*?);[^\x1b]*\x1b\\")
KITTY_REPLY = re.compile(rb"\x1b_Gi=(\d+)(?:,[^;]*)?;([^\x1b]*)\x1b\\")
DA1 = re.compile(rb"\x1b\[\?([0-9;]+)c")
KEYBOARD = re.compile(rb"\x1b\[\?([0-9]+)u")


def query_ids(data):
    ids = set()
    for match in KITTY_QUERY.finditer(data):
        found = re.search(rb"(?:^|,)i=(\d+)(?:,|$)", match[1])
        if found:
            ids.add(int(found[1]))
    return ids


class GraphicsNegative:
    def __init__(self):
        self.pending = b""
        self.ids = set()
        self.dropped = []

    def feed(self, chunk):
        self.pending += chunk
        forwarded = bytearray()
        while self.pending:
            data = self.pending
            if data.startswith(b"\x1b_G"):
                end = data.find(b"\x1b\\", 3)
                if end < 0:
                    break
                token, self.pending = data[:end + 2], data[end + 2:]
                match = KITTY_REPLY.fullmatch(token)
                positive = bool(match and int(match[1]) in self.ids and match[2] == b"OK")
            elif data.startswith(b"\x1b[?"):
                match = re.match(rb"\x1b\[\?[0-9;]*[@-~]", data)
                if not match:
                    break
                token, self.pending = match[0], data[match.end():]
                da = DA1.fullmatch(token)
                positive = bool(da and b"4" in da[1].split(b";"))
            elif any(prefix.startswith(data) for prefix in (b"\x1b_G", b"\x1b[?")):
                break
            else:
                forwarded.extend(data[:1])
                self.pending = data[1:]
                continue
            if positive:
                self.dropped.append(base64.b64encode(token).decode())
            else:
                forwarded.extend(token)
        if len(self.pending) > 65536:
            raise ValueError("unterminated capability reply")
        return bytes(forwarded)

    def flush(self):
        data, self.pending = self.pending, b""
        return data


def capabilities(log, input_direction="terminal-input"):
    outgoing, incoming = bytearray(), bytearray()
    synthetic = []
    queries, replies, keyboards, sixel_replies = {}, [], [], []
    starts, arrivals = [], []

    def correlation(query, start):
        arrival = arrivals[bisect_right(starts, start) - 1]
        valid = query is not None and query[0] < arrival[0]
        if valid and query[1] is not None and arrival[1] is not None:
            valid = query[1] <= arrival[1]
        return valid, arrival

    for event, line in enumerate(log.read_text().splitlines()):
        row = json.loads(line)
        data = base64.b64decode(row["base64"])
        if row["direction"] == "output":
            previous = len(outgoing)
            outgoing.extend(data)
            for match in KITTY_QUERY.finditer(outgoing):
                if match.end() > previous:
                    for ident in query_ids(match[0]):
                        queries[ident] = (event, row.get("ns"))
            for token, key in ((b"\x1b[c", "sixel"), (b"\x1b[?u", "keyboard")):
                end = outgoing.rfind(token) + len(token)
                if end > previous and end >= len(token):
                    queries[key] = (event, row.get("ns"))
        elif row["direction"] == input_direction:
            previous = len(incoming)
            if data:
                starts.append(previous)
                arrivals.append((event, row.get("ns")))
            incoming.extend(data)
            for match in KITTY_REPLY.finditer(incoming):
                if match.end() > previous:
                    query = queries.get(int(match[1]))
                    queried, arrival = correlation(query, match.start())
                    replies.append({"id": int(match[1]), "reply": match[2].decode(errors="replace"),
                                    "queried": queried, "query_event": query[0] if query else None,
                                    "reply_start_event": arrival[0], "reply_start_ns": arrival[1], "received_ns": row.get("ns")})
            for match in DA1.finditer(incoming):
                if match.end() > previous:
                    queried, arrival = correlation(queries.get("sixel"), match.start())
                    sixel_replies.append({"queried": queried, "supported": b"4" in match[1].split(b";"),
                                          "reply_start_ns": arrival[1], "received_ns": row.get("ns")})
            keyboards.extend(int(m[1]) for m in KEYBOARD.finditer(incoming) if m.end() > previous
                             and correlation(queries.get("keyboard"), m.start())[0])
        elif row["direction"] == "setup-input":
            synthetic.append(row)
    return {"query_ids": sorted(key for key in queries if isinstance(key, int)), "kitty_replies": replies,
            "kitty_positive": any(r["queried"] and r["reply"] == "OK" for r in replies),
            "sixel_positive": any(r["queried"] and r["supported"] for r in sixel_replies),
            "sixel_replies": sixel_replies, "keyboard_flags": keyboards,
            "setup_replay_count": len(synthetic), "inferred_from_TERM": False}
