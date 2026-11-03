"""Read only token-matched receipts for decoded pixels in presented GPUI frames."""

import json
import math
import statistics

# Quick Look's marked B7 raster edge, the same value as bench_thumbs::PREVIEW_EDGE.
PREVIEW_EDGE = 800
# Linux CLOCK_MONOTONIC, the only clock a receipt may be stamped with.
CLOCK_MONOTONIC = 1


class Receipts:
    def __init__(self, path, token, fixture):
        self.path, self.token = path, token
        self.expected = {str(path) for path in fixture.iterdir() if path.is_file()}
        if len(self.expected) != 16:
            raise ValueError("B7 needs exactly 16 fixture files")
        self.rows = []

    def poll(self):
        # Sample input: FLEA-B7 {"token":"t","kind":"preview","path":"/f/a.jpg","start_ns":1,"end_ns":2,"worker_ms":null,"width":800,"height":533,"bounds":[0.0, 0.0, 800.0, 533.0],"frame":9,"clock":1}
        rows = []
        for line in self.path.read_text(errors="replace").splitlines():
            if not line.startswith("FLEA-B7 "):
                continue
            try:
                row = json.loads(line.removeprefix("FLEA-B7 "))
            except json.JSONDecodeError:
                continue
            if not isinstance(row, dict):
                continue
            if row.get("token") != self.token:
                continue
            if row.get("path") not in self.expected or row.get("kind") not in ("thumbnail", "preview"):
                raise ValueError("B7 receipt does not name a fixture output")
            if row.get("clock") != CLOCK_MONOTONIC or any(type(row.get(k)) is not int or row[k] <= 0 for k in ("start_ns", "end_ns", "width", "height", "frame")):
                raise ValueError("B7 receipt lacks a positive native monotonic presentation")
            if row["end_ns"] <= row["start_ns"]:
                raise ValueError("B7 output precedes its request")
            bounds = row.get("bounds")
            if not isinstance(bounds, list) or len(bounds) != 4 or any(type(v) not in (int, float) or not math.isfinite(v) for v in bounds) or min(bounds[:2]) < 0 or min(bounds[2:]) <= 0:
                raise ValueError("B7 output has no visible paint bounds")
            if row["kind"] == "preview" and max(row["width"], row["height"]) != PREVIEW_EDGE:
                raise ValueError("B7 preview is not an 800 px raster")
            if any(r["kind"] == row["kind"] and r["path"] == row["path"] for r in rows):
                raise ValueError("duplicate B7 output receipt")
            rows.append(row)
        self.rows = rows

    def outputs(self, kind):
        return [row for row in self.rows if row["kind"] == kind]

    def complete(self, kind):
        return {row["path"] for row in self.outputs(kind)} == self.expected

    def metrics(self, verified):
        result = dict(all_thumbnails_ms=None, per_thumbnail_ms=None, preview_800_ms=None)
        if verified != 16 or not self.complete("thumbnail"):
            return result
        rows = self.outputs("thumbnail")
        result["all_thumbnails_ms"] = (max(r["end_ns"] for r in rows) - min(r["start_ns"] for r in rows)) / 1e6
        workers = [r.get("worker_ms") for r in rows]
        if all(type(v) in (int, float) and math.isfinite(v) and v > 0 for v in workers):
            result["per_thumbnail_ms"] = statistics.median(workers)
        if self.complete("preview"):
            result["preview_800_ms"] = statistics.median((r["end_ns"] - r["start_ns"]) / 1e6 for r in self.outputs("preview"))
        return result


def capture_receipt(root, name, reader, before_ns, after_ns):
    kind = "thumbnail" if name == "thumbnails" else "preview"
    (root / f"{name}-receipt.json").write_text(json.dumps(dict(
        capture_start_ns=before_ns, capture_end_ns=after_ns, outputs=reader.outputs(kind)), sort_keys=True) + "\n")
