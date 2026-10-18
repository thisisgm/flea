"""Write a copy of the native vectors whose first sheet loses or reorders one row."""
import json
import sys

source, target, mutation = sys.argv[1:4]
rows = [json.loads(line) for line in open(source, encoding="utf-8")]
first = next(row for row in rows if row["kind"] == "sheet")
group = (first["preset"], first["frontend"])
places = [i for i, row in enumerate(rows) if row["kind"] == "sheet" and (row["preset"], row["frontend"]) == group]
if len(places) < 2:
    sys.exit("sheet group too small to mutate")
if mutation == "drop":
    del rows[places[0]]
    places = places[:-1]
elif mutation == "swap":
    rows[places[0]], rows[places[1]] = rows[places[1]], rows[places[0]]
else:
    sys.exit("unknown mutation " + mutation)
for index, place in enumerate(places):
    if "index" in rows[place]:
        rows[place]["index"] = index
for row in rows:
    if row["kind"] == "sheetend" and (row["preset"], row["frontend"]) == group:
        row["count"] = len(places)
with open(target, "w", encoding="utf-8") as output:
    output.writelines(json.dumps(row, ensure_ascii=False) + "\n" for row in rows)
