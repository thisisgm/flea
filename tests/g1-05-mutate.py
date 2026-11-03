"""Write a copy of the native vectors with one decision changed, so the oracle runner must reject it."""
import json
import sys

source, target, mutation = sys.argv[1:4]
# Sample rows: {"kind": "fn", "m": "Scroll", "f": "distance", "args": [...], "out": 288} and {"kind": "slow", "steps": [...], "answers": [[true, 2]]}.
rows = [json.loads(line) for line in open(source, encoding="utf-8")]


def first(kind, test):
    return next(row for row in rows if row["kind"] == kind and test(row))


if mutation == "nudge":
    # One scroll pixel off: a notch that lands a pixel short of the oracle.
    row = first("fn", lambda row: row["m"] == "Scroll" and row["f"] == "distance" and row["out"] != 0)
    row["out"] += 1
elif mutation == "slow":
    # A slow click the oracle arms that the native side refuses, a rename that never opens.
    row = first("slow", lambda row: any(answer[0] is True for answer in row["answers"]))
    answer = next(answer for answer in row["answers"] if answer[0] is True)
    answer[0] = False
else:
    sys.exit("unknown mutation " + mutation)
with open(target, "w", encoding="utf-8") as output:
    output.writelines(json.dumps(row, ensure_ascii=False) + "\n" for row in rows)
