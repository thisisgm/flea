"""Resolve logical actions from the candidate's key table with preset precedence."""
import tomllib

from .evidence import Blocked

NAMED = {"Home": b"\x1b[H", "End": b"\x1b[F", "Up": b"\x1b[A", "Down": b"\x1b[B",
         "Left": b"\x1b[D", "Right": b"\x1b[C", "Return": b"\r", "Enter": b"\r",
         "Escape": b"\x1b", "Tab": b"\t", "Space": b" ", "Backspace": b"\x7f",
         "Delete": b"\x1b[3~", "PageUp": b"\x1b[5~", "PageDown": b"\x1b[6~", "F2": b"\x1bOQ"}


def bytes_for(key, mods=""):
    if key == "Delete" and mods == "shift":
        return b"\x1b[3;2~"
    if not mods or mods in ("text", "none", "code"):
        if key in NAMED:
            return NAMED[key]
        if len(key) == 1:
            return key.encode()
    if mods == "ctrl" and len(key) == 1 and 64 <= ord(key.upper()) <= 95:
        return bytes([ord(key.upper()) - 64])
    raise Blocked(f"key needs negotiated enhanced keyboard support or a plain alternative: {mods}+{key}")


class Keymap:
    def __init__(self, path, preset="default"):
        self.table = tomllib.loads(path.read_text())
        self.preset = preset

    def rows(self):
        for preset in (self.preset, "all"):
            for row in self.table.get("preset", []):
                contexts = row.get("context", "listing").split(",")
                if row.get("name") == preset and row.get("frontend") != "gui" and ("listing" in contexts or "all" in contexts):
                    yield row.get("mods", ""), row
        for kind, rows in self.table.items():
            if kind in ("preset", "sheet", "pointer") or not isinstance(rows, list):
                continue
            for row in rows:
                yield kind, row

    def resolve(self, action):
        effective, candidates = set(), []
        for mods, row in self.rows():
            key = row.get("char", row.get("key"))
            normalized = "" if mods in ("text", "code", "none") else mods
            token = (key, normalized)
            if not key or token in effective:
                continue
            effective.add(token)
            if row.get("action") == action:
                candidates.append({"key": key, "mods": normalized, "action": action})
        for candidate in candidates:
            try:
                bytes_for(candidate["key"], candidate["mods"])
                return candidate
            except Blocked:
                continue
        if candidates:
            return candidates[0]
        raise Blocked(f"candidate keymap has no listing binding for action {action} in {self.preset}")


def shortcut(key):
    name, modifiers = key["key"], key.get("mods", "")
    mods = " ".join(label for part, label in (("ctrl", "CTRL"), ("shift", "SHIFT"), ("alt", "ALT"), ("super", "SUPER")) if part in modifiers)
    if len(name) == 1 and name.isupper() and not modifiers:
        mods, name = "SHIFT", name.lower()
    name = {"?": "question", ":": "colon", " ": "space", "/": "slash", "[": "bracketleft", "]": "bracketright"}.get(name, name)
    if any(c in name for c in ",\r\n"):
        raise Blocked("key cannot be sent through compositor shortcut syntax")
    return mods, name
