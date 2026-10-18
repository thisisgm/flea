"""The terminal's padding and the strip past its last whole cell, against the TUI's ground."""
from .evidence import Blocked

# Two fills of one colour may differ by this much per channel in a capture.
TOLERANCE = 2


def ground_edges(image, client, grid, cell, rounding):
    """Compares every padding pixel on each side of the grid, away from the rounded corners, with the first cell's ground."""
    width, height, pixels = image
    cx, cy, cw, ch = client
    gx, gy, gw, gh = grid
    strips = {"left": (range(cx, gx), range(cy + rounding, cy + ch - rounding)),
              "top": (range(cx + rounding, cx + cw - rounding), range(cy, gy)),
              "right": (range(gx + gw, cx + cw), range(cy + rounding, cy + ch - rounding)),
              "bottom": (range(cx + rounding, cx + cw - rounding), range(gy + gh, cy + ch))}
    if not (0 <= cx <= gx and 0 <= cy <= gy and gx + gw <= cx + cw <= width and gy + gh <= cy + ch <= height):
        raise Blocked("terminal client or grid lies outside the owned output capture")
    if not any(xs and ys for xs, ys in strips.values()):
        return {"status": "PASS", "sides": {}}

    def at(x, y):
        offset = (y * width + x) * 4
        return pixels[offset:offset + 3]

    # The first cell of the header row is ground in every state, and a blank cell draws no glyph over it.
    ground = at(gx + cell[0] // 2, gy + cell[1] // 2)
    sides, reason = {}, None
    for side, (xs, ys) in strips.items():
        delta, sampled = 0, 0
        for y in ys:
            for x in xs:
                delta = max(delta, *(abs(a - b) for a, b in zip(at(x, y), ground)))
                sampled += 1
        sides[side] = {"pixels": sampled, "delta": delta}
        if delta > TOLERANCE and reason is None:
            reason = f"TUI ground differs from the terminal background at the {side} edge"
    result = {"status": "FAIL" if reason else "PASS", "ground": list(ground), "sides": sides}
    if reason:
        result["reason"] = reason
    return result
