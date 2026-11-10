"""Validate every captured Columns composition in a native parent-row navigation interval."""
import json
from pathlib import Path
import sys


def check(ok, message):
    if not ok:
        raise AssertionError(message)


def interval(frames, before, after):
    start = next((i for i, frame in enumerate(frames)
                  if frame["path"] == before and len(frame["columns"]) == 3
                  and all(column["rows"] for column in frame["columns"])), None)
    check(start is not None, "colhold: no settled original frame")
    end = next((i for i in range(start + 1, len(frames))
                if frames[i]["path"] == after and len(frames[i]["columns"]) == 3
                and frames[i]["columns"][-1]["firstName"] == "b.txt"), None)
    check(end is not None, "colhold: no settled completed frame")
    seen = frames[start:end + 1]
    for i, frame in enumerate(seen):
        check(len(frame["columns"]) == 3 and all(column["rows"] > 0 for column in frame["columns"]),
              f"colhold: frame {i + 1} drew an empty non-empty column: {frame}")
        check(frame["hasThird"], f"colhold: frame {i + 1} drew no third column: {frame}")
    return seen


if __name__ == "__main__":
    log, output, before, after = sys.argv[1:]
    # Sample input: COLHOLD FRAME {"path":"/fixture/b","columns":[{"rows":2,"firstName":"a"},{"rows":1,"firstName":"child"},{"rows":1,"firstName":"b.txt"}],"hasThird":true}
    frames = [json.loads(line.split("COLHOLD FRAME ", 1)[1])
              for line in Path(log).read_text(errors="replace").splitlines()
              if "COLHOLD FRAME " in line]
    seen = interval(frames, before, after)
    destination = Path(output)
    with destination.open("x") as handle:
        for frame in seen:
            handle.write(json.dumps(frame) + "\n")
    print(f"COLHOLD PASS parent sibling click: {len(seen)} frames, no empty column")
