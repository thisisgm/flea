"""Applies the declared 0.4.0 changes to a 0.3.12 golden row, so every other byte still has to match 0.3.12."""
import base64
import json
from pathlib import Path

DELTAS = Path(__file__).with_name("v0400-deltas.json")


def refuses(text, change, found):
    try:
        apply("guard", base64.b64encode(text.encode()).decode(), [change])
    except AssertionError as error:
        return f"matches {found} times" in str(error)
    return False


def load(prefix):
    # The exactly-once guard is what keeps a delta from widening, so every load proves it refuses zero and two matches.
    assert refuses("a", {"from": "b", "to": "c"}, 0) and refuses("bb", {"from": "b", "to": "c"}, 2), "deltas.apply lost its exactly-once guard"
    # Sample input: {"cli:ui-state": [{"from": "\"stateVersion\": 3", "to": "\"stateVersion\": 4", "reason": "..."}]}
    rows = json.loads(DELTAS.read_text())
    for key, changes in rows.items():
        assert changes and all(c.get("from") and "to" in c and c.get("reason", "").strip() for c in changes), key
    return {key.split(":", 1)[1]: changes for key, changes in rows.items() if key.startswith(prefix + ":")}


def apply(name, encoded, changes):
    # Each declared change must match exactly once in the 0.3.12 bytes, so a stale or widened delta fails.
    text = base64.b64decode(encoded).decode()
    for change in changes:
        found = text.count(change["from"])
        assert found == 1, f"{name}: declared 0.4.0 change matches {found} times: {change['from']!r}"
        text = text.replace(change["from"], change["to"])
    return base64.b64encode(text.encode()).decode()
