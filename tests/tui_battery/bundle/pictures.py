"""Observe presented Quick Look pixels using the existing board, transport and compositor paths."""
import base64
import hashlib
import json
from pathlib import Path
import time

from .. import png
from ..evidence import Blocked, Owned, absolute, save
from ..fallback import bounds
from ..geometry import CELL_WIDTH, CELL_HEIGHT, ORIGIN
from ..media_proof import KITTY, SIXEL, placements, traffic
from ..protocol import capabilities
from .outputs import digest, load

BLOCKS = frozenset("▀▄█▌▐")


def configure(terminal, board, column, grid_rect, cell):
    boxes = board.media_boxes()
    if len(boxes) != 1 or len(column.media_boxes()) != 1:
        raise Blocked("orbit specimen requires its exact Quick Look and column picture bounds")
    left, top, width, height = boxes[0]
    rect = [round(grid_rect[0] + (left - ORIGIN[0]) / CELL_WIDTH * cell[0]),
            round(grid_rect[1] + (top - ORIGIN[1]) / CELL_HEIGHT * cell[1]),
            round(width / CELL_WIDTH * cell[0]), round(height / CELL_HEIGHT * cell[1])]
    terminal.picture_geometry = {"native_rect": rect, "pixels": rect[2:], "cells": bounds(boxes[0]),
                                 "column_cells": bounds(column.media_boxes()[0])}
    save(terminal.case / "picture-config.json", terminal.picture_geometry)


def native_payload(case, mode, end_ns):
    stored, pending, payload = {}, None, None
    for token, _, end in traffic(case):
        if end > end_ns:
            break
        if mode == "sixel" and SIXEL.fullmatch(token):
            payload = token.encode()
        elif mode == "kitty" and (match := KITTY.fullmatch(token)):
            fields = dict(field.split("=", 1) for field in match[1].split(",") if "=" in field)
            action = fields.get("a")
            if action in ("T", "t"):
                pending = (fields.get("i", "0"), bytearray(), action)
            if pending and (action in ("T", "t") or action is None):
                pending[1].extend((match[2] or "").encode())
                if fields.get("m", "0") == "0":
                    ident, data, operation = pending
                    stored[ident] = base64.b64decode(data, validate=True)
                    if operation == "T":
                        payload = stored[ident]
                    pending = None
            elif action == "p":
                payload = stored.get(fields.get("i"))
    return payload


def decoded_native(terminal, payload, mode):
    from ..fixtures import namespace
    fx = terminal.fixture
    Owned(fx.parent).child(fx.name)
    directory = absolute(fx / "runtime/picture-native")
    directory.mkdir(parents=True, exist_ok=True)
    key = hashlib.sha256(payload).hexdigest()
    source = absolute(directory / (key + (".png" if mode == "kitty" else ".sixel")))
    target = absolute(directory / (key + ".decoded.png"))
    cached = getattr(terminal, "decoded_pictures", {})
    if key in cached:
        raster = cached[key].get("raster_path")
        if (digest(source) != key or digest(target) != cached[key]["sha256"]
                or (raster and digest(absolute(fx.parent / raster)) != cached[key]["raster_sha256"])):
            raise Blocked("retained native image decoding changed")
        return png.read(target), cached[key]
    with source.open("xb") as stream:
        stream.write(payload)
    inside = "/fx/" + str(source.relative_to(fx))
    output = "/fx/" + str(target.relative_to(fx))
    tool = absolute(Path("/usr/bin/magick"))
    decoders = []
    raster = None
    if mode == "sixel":
        # libsixel decodes the SIXEL; ImageMagick 7.1.2's reader drops every palette colour (upstream #9061).
        sixel = absolute(Path("/usr/bin/sixel2png"))
        raster = absolute(directory / (key + ".sixel.png"))
        decoders.append((sixel, ["-i", inside, "-o", "/fx/" + str(raster.relative_to(fx))]))
        inside = "/fx/" + str(raster.relative_to(fx))
    decoders.append((tool, ["PNG:" + inside, "-alpha", "on", "-depth", "8", "-interlace", "none", "PNG32:" + output]))
    for binary, arguments in decoders:
        # The receipt's command is the last run, the magick run that writes the target PNG.
        command = len(terminal.commands.records)
        result = terminal.commands.run(namespace(binary, fx, "/fx", {}, arguments=arguments), timeout=20)
        if result.stderr.strip():
            raise Blocked("native picture decoder reported warnings or errors")
    receipt = {"path": "fx/" + str(target.relative_to(fx)), "sha256": digest(target),
               "transport_path": "fx/" + str(source.relative_to(fx)), "transport_sha256": key,
               "decoder_sha256": [digest(binary) for binary, _ in decoders], "command": command}
    if raster is not None:
        receipt.update(raster_path="fx/" + str(raster.relative_to(fx)), raster_sha256=digest(raster))
    terminal.decoded_pictures = dict(cached, **{key: receipt})
    return png.read(target), receipt


def half_pixels(snapshot, geometry, cropped):
    x0, y0, columns, rows = geometry["cells"]
    cw, ch = cropped[0] / columns, cropped[1] / rows
    for y, line in enumerate(snapshot["grid"]):
        for x, cell in enumerate(line):
            if not (x0 <= x < x0 + columns and y0 <= y < y0 + rows) or cell["ch"] not in BLOCKS:
                continue
            fg, bg = cell["fg"], cell["bg"]
            if cell.get("reverse"):
                fg, bg = bg, fg
            samples = [(0.5,0.25,fg),(0.5,0.75,bg)] if cell["ch"] == "▀" else (
                      [(0.5,0.25,bg),(0.5,0.75,fg)] if cell["ch"] == "▄" else (
                      [(0.25,0.5,fg),(0.75,0.5,bg)] if cell["ch"] == "▌" else (
                      [(0.25,0.5,bg),(0.75,0.5,fg)] if cell["ch"] == "▐" else [(0.5,0.5,fg)])))
            for dx, dy, color in samples:
                px, py = int((x-x0+dx)*cw), int((y-y0+dy)*ch)
                at = (py * cropped[0] + px) * 4
                if bytes(cropped[2][at:at+3]) != bytes.fromhex(color.removeprefix("#")):
                    raise Blocked("compositor has not presented the observed Quick Look half-block cells")


def complete_card(snapshot):
    """The accepted card writer emits its entire bottom edge last, then only a rendition reset."""
    columns, rows = snapshot.get("columns", 0), snapshot.get("rows", 0)
    if columns < 24 or rows <= 4 or snapshot.get("pending"):
        return False
    margin = min(9, (columns - 24) // 2)
    bottom = rows - 3
    if snapshot.get("cursor") != [columns - margin, bottom]:
        return False
    edge = snapshot["grid"][bottom][margin:columns-margin]
    return (len(edge) >= 2 and edge[0]["ch"] in "└╰" and edge[-1]["ch"] in "┘╯"
            and all(cell["ch"] == "─" for cell in edge[1:-1]))


def frame(terminal):
    geometry = getattr(terminal, "picture_geometry", None)
    if not geometry:
        raise Blocked("orbit capture lacks calibrated Quick Look geometry")
    snapshot = terminal.snapshot()
    caps = capabilities(terminal.case / "pty.log", "candidate-input")
    mode = "kitty" if caps["kitty_positive"] else "sixel" if caps["sixel_positive"] else "half-blocks"
    if mode != "half-blocks":
        end = placements(terminal.case, caps, mode)[-1]["end_ns"]
        payload = native_payload(terminal.case, mode, end)
        decoded, source = decoded_native(terminal, payload, mode)
        if list(decoded[:2]) != geometry["pixels"]:
            raise Blocked("initial Quick Look picture has not completed at its full calibrated size")
    else:
        if not complete_card(snapshot):
            raise Blocked("initial Quick Look redraw has not completed its final card edge")
        x, y, width, height = geometry["cells"]
        cx, cy, cw, ch = geometry["column_cells"]
        cells = [(column, row) for row, line in enumerate(snapshot["grid"]) for column, cell in enumerate(line)
                 if x <= column < x + width and y <= row < y + height and cell["ch"] in BLOCKS]
        if not any(not (cx <= column < cx + cw and cy <= row < cy + ch) for column, row in cells):
            raise Blocked("initial Quick Look half-block picture has not replaced the historical column")
        outputs = [json.loads(line) for line in (terminal.case / "pty.log").read_text().splitlines()]
        end = max((row["ns"] for row in outputs if row["direction"] == "output"), default=0)
        if not end:
            raise Blocked("Quick Look half-block picture lacks output timing")
    directory = Owned(terminal.case).child("pictures")
    directory.mkdir(exist_ok=True)
    serial = getattr(terminal, "picture_serial", 0)
    terminal.picture_serial = serial + 1
    path = absolute(directory / (str(serial) + ".png"))
    image = terminal.display.capture(path)
    cropped = png.crop(image, geometry["native_rect"])
    if len({bytes(cropped[2][i:i+3]) for i in range(0, len(cropped[2]), 4)}) < 4:
        raise Blocked("Quick Look picture pixels are still empty")
    if mode != "half-blocks":
        if bytes(cropped[2]) != bytes(decoded[2]):
            raise Blocked("compositor has not presented the completed Quick Look native picture")
    else:
        half_pixels(snapshot, geometry, cropped)
    return {"native_source": source if mode != "half-blocks" else None, "mode": mode, "sha256": hashlib.sha256(bytes(cropped[2])).hexdigest(), "end_ns": end,
            "observed_ns": time.time_ns(), "capture": str(path.relative_to(terminal.case)),
            "capture_sha256": digest(path), "rect": geometry["native_rect"], "pixels": list(cropped[:2])}


def verify(case, frame):
    geometry = load(case / "picture-config.json")
    if frame["rect"] != geometry["native_rect"] or frame["pixels"] != geometry["pixels"]:
        raise Blocked("orbit pixels differ from the calibrated Quick Look region")
    path = absolute(case / frame["capture"])
    if not path.is_relative_to(case / "pictures") or digest(path) != frame["capture_sha256"]:
        raise Blocked("orbit's retained native capture is missing or changed")
    cropped = png.crop(png.read(path), frame["rect"])
    if list(cropped[:2]) != frame["pixels"] or hashlib.sha256(bytes(cropped[2])).hexdigest() != frame["sha256"]:
        raise Blocked("orbit receipt differs from its actual presented pixels")
