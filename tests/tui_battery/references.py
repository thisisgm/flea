"""Read cell positions and ink recipes directly from immutable board HTML."""
from html.parser import HTMLParser
import math
import re

from .cells import width
from .dynamic import disk_field
from .evidence import Blocked, digest
from . import png

from .geometry import GEOMETRY, COLUMNS, ROWS, CELL_WIDTH, CELL_HEIGHT, ORIGIN


def styles(raw):
    return dict(part.split(":", 1) for part in raw.split(";") if ":" in part)


class Board(HTMLParser):
    def __init__(self, path):
        super().__init__(convert_charrefs=True)
        self.source = path.read_text()
        self.sha256 = digest(path)
        self.runs, self.shapes, self.pictures = [], [], []
        self.active = None
        self.stack = []
        self.palette = dict(re.findall(r"--([\w-]+):([^;}]+)", self.source))
        self.feed(self.source)
        if not self.runs:
            raise Blocked("board lacks positioned cell text")

    def handle_starttag(self, tag, attrs):
        attr = dict(attrs)
        style = styles(attr.get("style", ""))
        origin = self.stack[-1][1] if self.stack else (0, 0)
        offset = origin
        if "left" in style or "top" in style:
            offset = (origin[0] + float(style.get("left", "0").removesuffix("px")),
                      origin[1] + float(style.get("top", "0").removesuffix("px")))
            style = dict(style, left=f"{offset[0]}px", top=f"{offset[1]}px")
        if tag not in ("meta", "link", "img", "br", "hr", "input"):
            self.stack.append((tag, offset, style))
        if tag == "b" and "left" in style and "top" in style:
            self.active = {"style": style, "text": ""}
        elif tag == "i" and "left" in style and "top" in style:
            if "border-left" in style or "border-right" in style:
                if len(self.stack) >= 2:
                    style["clip"] = self.stack[-2][2]
            self.shapes.append(style)
        elif tag in ("img", "svg"):
            self.pictures.append(dict(attr, origin=offset, resolved_style=style))

    def handle_data(self, data):
        if self.active is not None:
            self.active["text"] += data

    def handle_endtag(self, tag):
        if tag == "b" and self.active is not None:
            self.runs.append(self.active)
            self.active = None
        while self.stack:
            current = self.stack.pop()
            if current[0] == tag:
                break

    def ink(self, expression):
        expression = expression.strip()
        for _ in range(20):
            if "var(" not in expression:
                break
            expression = re.sub(r"var\(--([\w-]+)\)", lambda m: self.palette[m[1]], expression)
        if "var(" in expression:
            raise Blocked("cyclic or unsupported board variable")
        if re.fullmatch(r"#[0-9a-fA-F]{6}", expression):
            return expression.lower()
        if expression.startswith("color-mix(") and expression.endswith(")"):
            parts, start, depth = [], 10, 0
            for i in range(10, len(expression) - 1):
                ch = expression[i]
                depth += (ch == "(") - (ch == ")")
                if ch == "," and depth == 0:
                    parts.append(expression[start:i].strip())
                    start = i + 1
            parts.append(expression[start:-1].strip())
            if len(parts) != 3 or parts[0] != "in srgb":
                raise Blocked("unsupported board color mix")
            mix = re.fullmatch(r"(.+)\s+(\d+(?:\.\d+)?)%", parts[1])
            if not mix:
                raise Blocked("unsupported board color weight")
            a, b = self.ink(mix[1]), self.ink(parts[2])
            ratio = float(mix[2]) / 100
            return "#" + "".join(f"{math.floor(int(a[i:i+2],16)*ratio+int(b[i:i+2],16)*(1-ratio)+.5):02x}" for i in (1, 3, 5))
        raise Blocked(f"unsupported board ink recipe: {expression}")

    def cells(self, columns=COLUMNS, rows=ROWS, origin=ORIGIN):
        result = {(x, y): {"ch": " ", "fg": self.ink("var(--fg)"), "bg": self.ink("var(--bg)"),
                           "bold": False, "dim": False, "underline": False, "width": 1} for y in range(rows) for x in range(columns)}
        strokes = {}
        for style in self.shapes:
            left, top, w, h = [float(style.get(k, "0").removesuffix("px")) for k in ("left", "top", "width", "height")]
            x, y = math.floor((left - origin[0] + .01) / CELL_WIDTH), math.floor((top - origin[1] + .01) / CELL_HEIGHT)
            if "clip" in style:
                clip = style["clip"]
                x = round((float(clip["left"].removesuffix("px")) - origin[0]) / CELL_WIDTH)
                y = round((float(clip["top"].removesuffix("px")) - origin[1]) / CELL_HEIGHT)
                if (x, y) in result:
                    glyph = ("╭" if "border-top" in style else "╰") if "border-left" in style else ("╮" if "border-top" in style else "╯")
                    ink = next(v.split(" solid ", 1)[1] for k, v in style.items() if k in ("border-left", "border-right"))
                    result[(x, y)].update(ch=glyph, fg=self.ink(ink))
                continue
            if "background" not in style or (x, y) not in result:
                continue
            ink = self.ink(style["background"])
            if abs((left - origin[0]) / CELL_WIDTH - x) < .01 and abs((top - origin[1]) / CELL_HEIGHT - y) < .01 and h == CELL_HEIGHT and w >= 8.39:
                for offset in range(round(w / CELL_WIDTH)):
                    if (x + offset, y) in result:
                        result[(x + offset, y)]["bg"] = ink
            elif h in (1, 2) and w <= CELL_WIDTH + .01:
                l = left <= origin[0] + x * CELL_WIDTH + .1
                r = left + w >= origin[0] + (x + 1) * CELL_WIDTH - .1
                entry = strokes.setdefault((x, y), [False, False, False, False, ink, h])
                entry[0], entry[1] = entry[0] or l, entry[1] or r
            elif w in (1, 2) and h <= CELL_HEIGHT:
                u = top <= origin[1] + y * CELL_HEIGHT + .1
                d = top + h >= origin[1] + (y + 1) * CELL_HEIGHT - .1
                entry = strokes.setdefault((x, y), [False, False, False, False, ink, w])
                entry[2], entry[3] = entry[2] or u, entry[3] or d
            elif abs(w - 2.1) < .02 and h == CELL_HEIGHT:
                result[(x, y)].update(ch="▎", fg=ink)
            elif abs(w - CELL_WIDTH) < .02 and h == 11:
                result[(x, y)].update(ch="▀" if abs((top - origin[1]) % CELL_HEIGHT) < .01 else "▄", fg=ink)
        line_glyphs = {(True, True, False, False): "─", (False, False, True, True): "│",
                       (False, True, False, True): "┌", (True, False, False, True): "┐",
                       (False, True, True, False): "└", (True, False, True, False): "┘",
                       (True, True, False, True): "┬", (True, True, True, False): "┴",
                       (False, True, True, True): "├", (True, False, True, True): "┤",
                       (True, True, True, True): "┼"}
        for cell, values in strokes.items():
            if tuple(values[:4]) in line_glyphs:
                glyph = line_glyphs[tuple(values[:4])]
                if values[5] == 2:
                    glyph = {"─": "━", "│": "┃"}.get(glyph, glyph)
                result[cell].update(ch=glyph, fg=values[4])
        for run in self.runs:
            style = run["style"]
            x = round((float(style["left"].removesuffix("px")) - origin[0]) / CELL_WIDTH)
            y = round((float(style["top"].removesuffix("px")) - origin[1]) / CELL_HEIGHT)
            for ch in run["text"]:
                w = width(ch)
                if 0 <= x < columns and 0 <= y < rows:
                    result[(x, y)].update(ch=ch, fg=self.ink(style.get("color", "var(--fg)")),
                                          bold=style.get("font-weight") == "700",
                                          underline="underline" in style.get("text-decoration", "").split(), width=w)
                    if w == 2 and x + 1 < columns:
                        result[(x + 1, y)].update(result[(x, y)], ch="", width=0)
                x += w
        # Positioned HTML omits some blank runs; the status disk is one foreground run.
        field = disk_field([result[x, rows - 1] for x in range(columns)])
        if field:
            ink = result[field["left"], rows - 1]["fg"]
            for x in range(field["left"], field["right"]):
                if result[x, rows - 1]["ch"] == " ":
                    result[x, rows - 1]["fg"] = ink
        return result

    def media_boxes(self):
        boxes = []
        for picture in self.pictures:
            style = picture["resolved_style"]
            w = style.get("width", picture.get("width", "0")).removesuffix("px")
            h = style.get("height", picture.get("height", "0")).removesuffix("px")
            if not re.fullmatch(r"[0-9.]+", w) or not re.fullmatch(r"[0-9.]+", h):
                raise Blocked("media board geometry is not expressed in pixels")
            if float(w) and float(h):
                boxes.append([*picture["origin"], float(w), float(h)])
        return boxes

    def compare(self, snapshot, masks=(), origin=ORIGIN, resting=False, picture_reference=None):
        errors, colors = [], []
        expected_cells = self.cells(snapshot["columns"], snapshot["rows"], origin)
        if picture_reference:
            alternate = picture_reference.cells(snapshot["columns"], snapshot["rows"], origin)
            for left, top, w, h in self.media_boxes():
                x0, y0 = math.floor((left - origin[0]) / CELL_WIDTH), math.floor((top - origin[1]) / CELL_HEIGHT)
                for y in range(y0, min(snapshot["rows"], math.ceil((top + h - origin[1]) / CELL_HEIGHT))):
                    for x in range(x0, min(snapshot["columns"], math.ceil((left + w - origin[0]) / CELL_WIDTH))):
                        if (x, y) in expected_cells:
                            expected_cells[(x, y)] = alternate[(x, y)]
        strict_foreground = set()
        for expectation in masks:
            if not isinstance(expectation, dict) or expectation.get("kind") not in ("verified-disk-text", "source-media-cells"):
                raise Blocked("rectangle exemptions cannot bypass cell geometry or rendition")
            left, top, count, height = expectation["field"]
            if (min(left, top) < 0 or min(count, height) <= 0 or left + count > snapshot["columns"]
                    or top + height > snapshot["rows"]):
                raise Blocked("derived expectation is outside the terminal grid")
            if expectation["kind"] == "verified-disk-text" and (top != snapshot["rows"] - 1 or height != 1):
                raise Blocked("verified disk field is outside the status row")
            required = {(x, y) for y in range(top, top + height) for x in range(left, left + count)}
            supplied = {tuple(entry["cell"]) for entry in expectation["cells"]}
            if supplied != required or len(supplied) != len(expectation["cells"]):
                raise Blocked("derived expectation must specify every bounded cell exactly once")
            strict_foreground.update(required)
            for entry in expectation["cells"]:
                expected_cells[tuple(entry["cell"])] = entry["expected"]
        for (x, y), expected in expected_cells.items():
            if resting and expected["ch"] == "┃":
                expected = dict(expected, ch="│")
            actual = snapshot["grid"][y][x]
            fg, bg = (actual["bg"], actual["fg"]) if actual["reverse"] else (actual["fg"], actual["bg"])
            if actual["ch"] != expected["ch"] or actual["width"] != expected["width"]:
                errors.append({"cell": [x, y], "expected": expected, "actual": actual})
            if (bg != expected["bg"] or ((expected["ch"].strip() or (x, y) in strict_foreground) and fg != expected["fg"])
                  or any(actual[key] != expected[key] for key in ("bold", "dim", "underline"))):
                colors.append({"cell": [x, y], "expected": expected, "actual": actual})
        return {"geometry": {"status": "FAIL" if errors else "PASS", "errors": errors},
                "color": {"status": "FAIL" if colors else "PASS", "errors": colors},
                "probe": dict(GEOMETRY, html_sha256=self.sha256, source="positioned board HTML")}


def separator(image, client, pty):
    """Locate the first rule inside the owned client, never desktop-wide changes."""
    w, h, pixels = image
    cx, cy, cw, ch = client
    rows, columns, grid_w, grid_h = pty
    if not grid_w or not grid_h or grid_w > cw or grid_h > ch or columns <= 0 or rows < 3:
        raise Blocked("PTY pixel geometry is absent or outside its compositor client")
    if grid_w % columns or grid_h % rows:
        raise Blocked("native PTY reports fractional cell pixels")
    cell_h = grid_h // rows
    anchors = []
    for y in range(cy + cell_h, min(h - 1, cy + ch - grid_h + 2 * cell_h)):
        runs, start = [], None
        for x in range(cx, min(w, cx + cw)):
            a = pixels[(y * w + x) * 4:(y * w + x) * 4 + 3]
            b = pixels[((y - 1) * w + x) * 4:((y - 1) * w + x) * 4 + 3]
            if a != b and start is None:
                start = x
            if a == b and start is not None:
                runs.append((start, x))
                start = None
        if start is not None:
            runs.append((start, min(w, cx + cw)))
        for left, right in runs:
            if right - left == grid_w:
                section = lambda row: pixels[(row * w + left) * 4:(row * w + right) * 4]
                background = section(y - 1)
                for bottom in range(y + 1, min(h, y + cell_h)):
                    if section(bottom) == background:
                        top = round((y + bottom - 1) / 2 - 1.5 * cell_h)
                        if cy <= top and top + grid_h <= cy + ch:
                            anchors.append((left, top, grid_w, grid_h))
                        break
    unique = sorted(set(anchors))
    if len(unique) != 1:
        raise Blocked(f"row-2 separator missing or ambiguous: {len(unique)} anchors")
    return unique[0]


def compare_pixels(native, board, rect, directory, reference_rect=(*ORIGIN, round(COLUMNS * CELL_WIDTH), ROWS * CELL_HEIGHT)):
    candidate = png.crop(png.read(native), rect)
    reference = png.crop(png.read(board), reference_rect)
    png.write(directory / "grid-crop.png", *candidate)
    try:
        return dict(png.artifacts(candidate, reference, directory), status="REVIEW")
    except ValueError as error:
        w, h = max(candidate[0], reference[0]), max(candidate[1], reference[1])
        def pad(image):
            result = bytearray([0, 0, 0, 255]) * (w * h)
            iw, ih, pixels = image
            for y in range(ih):
                result[y * w * 4:(y * w + iw) * 4] = pixels[y * iw * 4:(y + 1) * iw * 4]
            return w, h, result
        receipt = png.artifacts(pad(candidate), pad(reference), directory)
        return dict(receipt, status="BLOCKED", reason=str(error), padding_only=True,
                    native_size=list(candidate[:2]), reference_size=list(reference[:2]))
