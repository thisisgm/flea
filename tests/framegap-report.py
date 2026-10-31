#!/usr/bin/env python3
"""Check every raw frame measurement before applying the positional fold allowance."""
from collections import defaultdict
from pathlib import Path
import re
import sys

SIDES = {"top", "bottom", "left", "right"}


def validate(text, expected_sides):
    frames = defaultdict(list)
    rows = clipped = 0
    for line in text.splitlines():
        # Sample input: "qml: FRAMEGAP ROW GapDialog\tDialogButton\tCancel\tRectangle:buttonFrame\t1,2,3,4\trest\t14\ttop\tnone\t3\t3".
        match = re.search(r"FRAMEGAP (ROW|CLIPPED) (.*)", line)
        if not match:
            continue
        # Sample fields: "SettingsPanel view\tCheckBox\t\tRectangle:\t1,2,3,4\trest\t14\ttop\tnone\t3\t3".
        fields = match[2].split("\t")
        if len(fields) != 11:
            raise ValueError("a raw frame row does not have eleven fields")
        if fields[5] not in ("rest", "focus") or fields[6] not in ("12", "14", "16"):
            raise ValueError("a raw frame row has an unknown state or text stop")
        if match[1] == "CLIPPED":
            if not fields[0].startswith("SettingsPanel ") or fields[7] != "fold":
                raise ValueError("a foreign or non-fold frame was called clipped")
            clipped += 1
        else:
            if fields[7] not in SIDES:
                raise ValueError("a raw frame row has an unknown side")
            rows += 1
        frames[tuple(fields[:7])].append(fields[7])
    for sides in frames.values():
        if sides != ["fold"] and (len(sides) != 4 or set(sides) != SIDES):
            raise ValueError("a frame is missing a side or repeats a side")
    if not frames or 4 * len(frames) != expected_sides:
        raise ValueError("the pre-clip frame-side inventory changed")
    # Sample input: "qml: FRAMEGAP DONE checks=4672 failed=0 controls=96 controlMiss=0 focusMiss=0".
    totals = re.findall(r"FRAMEGAP DONE checks=(\d+) failed=\d+ controls=\d+ controlMiss=\d+ focusMiss=\d+", text)
    if len(totals) != 1 or int(totals[0]) != rows:
        raise ValueError("the raw frame rows disagree with the sole completion tally")
    return rows, clipped, len(frames)


if __name__ == "__main__":
    try:
        rows, clipped, frames = validate(Path(sys.argv[1]).read_text(), int(sys.argv[2]))
    except (OSError, ValueError, IndexError) as error:
        raise SystemExit("FAIL framegap report: " + str(error))
    print(f"FRAMEGAP COVERAGE sides={4 * frames} frames={frames} rows={rows} clipped={clipped}")
