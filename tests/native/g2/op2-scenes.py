#!/usr/bin/env python3
"""Pin the archive scene actions and the approved primary-button description."""
import json
from pathlib import Path

repo = Path(__file__).resolve().parents[3]
manifest = json.loads((repo / "tests/visual/g2/op2.json").read_text())
scenes = {scene["scene"]: scene for scene in manifest["scenes"]}
mismatch = scenes["g2.op2.extra.compress-mismatch"]
assert mismatch["setup"]["focus"] == "confirm"
assert mismatch["setup"].get("key") == "enter", "the mismatch scene must submit its confirm field"
assert mismatch["setup"]["typed"] == "mismatch"
focus = scenes["g2.op2.extra.compress-focus"]
for description in (focus["title"], *focus["assertions"]):
    assert "dimmed" not in description
    assert "lit as the primary, still in the walk" in description
print("ARCHIVE SCENES PASS submitted mismatch and lit primary focus")
