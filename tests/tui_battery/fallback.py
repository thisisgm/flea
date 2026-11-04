"""Independent source-to-cell expectations for renderer-specific picture fallbacks."""
import json
import math
import os
from pathlib import Path
import re
import shutil

from .cells import Screen
from .geometry import ORIGIN, CELL_WIDTH, CELL_HEIGHT, ALIGNMENT_EPSILON
from .evidence import Blocked, Commands, absolute, digest
from .fixtures import relative
from . import png

STATES = {"tui-list-pdf", "tui-list-video", "tui-quicklook-image", "tui-3d-quicklook",
          "tui-office", "tui-print-sent", "tui-combine", "tui-lang-ar"}


def bounds(box, origin=ORIGIN):
    left, top, width, height = box
    x, y = math.floor((left - origin[0]) / CELL_WIDTH + ALIGNMENT_EPSILON), math.floor((top - origin[1]) / CELL_HEIGHT + ALIGNMENT_EPSILON)
    return [x, y, math.ceil((left + width - origin[0]) / CELL_WIDTH - ALIGNMENT_EPSILON) - x,
            math.ceil((top + height - origin[1]) / CELL_HEIGHT - ALIGNMENT_EPSILON) - y]


def inputs(state):
    names = [state.get("cursor")]
    if state["id"] == "tui-combine":
        names += [step["select"] for step in state["steps"] if "select" in step]
    if not all(isinstance(name, str) and name for name in names):
        raise Blocked("source content requires the selected fixture names")
    return [str(relative(state["cwd"].removeprefix("/fx/")) / relative(name)) for name in names]


def sources(state, case, fx, boxes):
    if not (case / "fixtures.json").is_file():
        raise Blocked("source content requires the independently copied fixture manifest")
    manifest = json.loads((case / "fixtures.json").read_text())
    declarations = manifest.get("representations", {}).get(state["id"], {}).get("sources", [])
    expected_inputs = inputs(state)
    if len(declarations) != len(boxes) or len(expected_inputs) != len(boxes):
        raise Blocked("source content requires one independent fixture/raster declaration per picture")
    hashes = {"v/work/" + row["path"]: row["sha256"] for row in manifest.get("copied", [])}
    hashes.update({row["path"]: row["sha256"] for row in manifest.get("auxiliary_copied", [])})
    result, files = [], {}
    for declaration, name, box in zip(declarations, expected_inputs, boxes):
        if declaration.get("input") != name:
            raise Blocked("source content does not identify the selected fixtures in listing order")
        raster = str(relative(declaration["raster"]))
        for value in (name, raster):
            path = absolute(fx / relative(value))
            if not path.is_file() or digest(path) != hashes.get(value):
                raise Blocked("source content differs from the independently copied fixture: " + value)
            files[value] = hashes[value]
        derivation = declaration.get("derivation", "")
        if raster != name and (not isinstance(derivation, str) or not derivation.strip()):
            raise Blocked("prepared source raster requires its independent derivation for controller review")
        width, height, _ = png.read(fx / raster)
        bw, bh = box[2:]
        # Quantization may add at most one terminal cell along either picture edge.
        tolerance = CELL_WIDTH / bh + bw * CELL_HEIGHT / (bh * bh)
        if abs(width / height - bw / bh) > tolerance:
            raise Blocked("source aspect differs from the approved picture bounds")
        result.append({"input": name, "input_sha256": hashes[name], "raster": raster,
                       "raster_sha256": hashes[raster], "size": [width, height], "derivation": derivation})
    return result, files


def chafa_cells(source, box, board_cells, fx, commands, cell, case):
    left, top, width, height = box
    executable = shutil.which("chafa", path="/usr/bin:/bin")
    # A private tool has the same identity-bound treatment as the candidate's fixture resolver.
    private = fx / "tools/chafa"
    if private.is_file() and os.access(private, os.X_OK):
        executable = str(absolute(private))
        manifest = json.loads((case / "fixtures.json").read_text())
        copied = {row["path"]: row["sha256"] for row in manifest.get("auxiliary_copied", [])}
        if digest(private) != copied.get("tools/chafa"):
            raise Blocked("private chafa differs from its independently copied fixture")
    if not executable:
        raise Blocked("source-cell proof requires the actual chafa executable")
    commands.run([executable, "--version"])
    style = board_cells[left, top]
    argv = [executable, "--format=symbols", "--symbols=half", "--fill=space", "--colors=full",
            "--dither=none", "--color-extractor=average", "--color-space=rgb", "--preprocess=off",
            "--optimize=0", "--polite=on", "--relative=off", "--probe=off", "--animate=off", "--threads=1",
            "--exact-size=off", "--scale=max", f"--size={width}x{height}",
            f"--font-ratio={cell[0]}/{cell[1]}", "--fg=" + style["fg"], "--bg=" + style["bg"], str(fx / source["raster"])]
    env = {"PATH": str(fx / "tools") + ":/usr/bin:/bin", "HOME": str(fx / "home"),
           "XDG_CONFIG_HOME": str(fx / "config"), "XDG_CACHE_HOME": str(fx / "cache"),
           "TMPDIR": str(fx / "tmp"), "LC_ALL": "C.UTF-8", "TERM": "xterm-256color"}
    output = commands.run(argv, env=env, cwd=fx, timeout=30).stdout
    if len(output) > 2 * 1024 * 1024:
        raise Blocked("chafa source proof exceeds its output budget")
    # Polite mode suppresses cursor visibility; optimize zero leaves only rendition and newlines.
    text = output.decode("utf-8", errors="strict")
    visible = re.sub(r"\x1b\[[0-9;]*m", "", text).replace("\r\n", "\n")
    lines = visible.rstrip("\n").split("\n")
    # Each guard names its own branch; the charset guard runs first, as an escape byte would also read as an oversized row.
    if any(ch not in " ▀▄█▌▐\n" for ch in visible):
        raise Blocked("chafa source output holds characters outside the half-block set")
    if not any(ch in visible for ch in "▀▄█▌▐"):
        raise Blocked("chafa source output is empty, with no half-block cells")
    if len(lines) > height or any(len(line) > width for line in lines):
        raise Blocked("chafa source output exceeds the approved picture size")
    sw, sh = source["size"]
    ink_width, ink_height = max(map(len, lines)), len(lines)
    ratio = ink_width * cell[0] / (ink_height * cell[1])
    tolerance = cell[0] / (ink_height * cell[1]) + ratio / ink_height
    if abs(ratio - sw / sh) > tolerance:
        raise Blocked("chafa cells distort the source aspect")
    screen = Screen(width + 1, height + 1)
    screen.style.update({key: style[key] for key in ("fg", "bg", "bold", "dim", "underline")})
    screen.grid = [[screen.blank() for _ in range(width + 1)] for _ in range(height + 1)]
    screen.feed(output.replace(b"\n", b"\r\n"))
    if screen.unsupported or screen.pending or screen.graphics:
        raise Blocked("chafa source output contains unparsed or native graphics controls")
    result = {}
    for y in range(height):
        for x in range(width):
            value = dict(screen.grid[y][x])
            if value.pop("reverse"):
                value["fg"], value["bg"] = value["bg"], value["fg"]
            result[left + x, top + y] = value
    return result, {"chafa_sha256": digest(executable), "argv": argv, "cell_size": list(cell)}


def numbered_cells(box, number, focused, board, cells):
    left, top, width, height = box
    if min(width, height) < 3:
        raise Blocked("Combine numbered box must have a border and an interior")
    style = dict(cells[left, top], ch=" ", width=1, bold=False, dim=False, underline=False)
    frame = board.ink("var(--accent-frame)" if focused else "var(--muted-frame)")
    result = {(x, y): dict(style) for y in range(top, top + height) for x in range(left, left + width)}
    for y in range(height):
        for x in range(width):
            glyph = ("┌" if x == 0 else "┐" if x == width - 1 else "─") if y == 0 else (
                     ("└" if x == 0 else "┘" if x == width - 1 else "─") if y == height - 1 else (
                     "│" if x in (0, width - 1) else " "))
            if glyph != " ":
                result[left + x, top + y].update(ch=glyph, fg=frame)
    result[left + 1, top + 1].update(ch=str(number), fg=board.ink("var(--muted)"))
    return result


def representation_status(media, review):
    if not media.get("representation_bound"):
        return media["status"]
    decision = review.get("representation", {})
    required = media["review_requirements"]
    if all(decision.get(key) == value for key, value in required.items()) and decision.get("approved") is True:
        return "PASS" if media["status"] in ("PASS", "BLOCKED") else media["status"]
    return "BLOCKED" if media["status"] != "FAIL" else "FAIL"


def verify_sources(media, fx):
    for name, sha in media.get("source_files", {}).items():
        if digest(absolute(fx / relative(name))) != sha:
            raise Blocked("retained representation source changed: " + name)


def inspect(state, board, snapshot, grid_rect, case, fx, review, commands=None):
    if state["id"] not in STATES:
        raise Blocked("renderer-specific source mapping is not defined for this state")
    if not snapshot or not grid_rect:
        raise Blocked("source-cell proof requires captured cells and calibrated media bounds")
    boxes = board.media_boxes()
    cell_boxes = [bounds(box) for box in boxes]
    for left, top, width, height in cell_boxes:
        if (min(left, top) < 0 or min(width, height) <= 0 or left + width > snapshot["columns"]
                or top + height > snapshot["rows"]):
            raise Blocked("source media bounds exceed the captured terminal grid")
    source, files = sources(state, case, fx, boxes)
    expected = board.cells(snapshot["columns"], snapshot["rows"])
    mode = "numbered-boxes" if state["id"] == "tui-combine" else "half-blocks"
    overrides, renderer = [], []
    for index, (item, box) in enumerate(zip(source, cell_boxes)):
        if mode == "numbered-boxes":
            cells = numbered_cells(box, index + 1, index == 0, board, expected)
        else:
            cells, receipt = chafa_cells(item, box, expected, fx, commands or Commands(),
                                        (grid_rect[2] / snapshot["columns"], grid_rect[3] / snapshot["rows"]), case)
            renderer.append(receipt)
            executable = Path(receipt["argv"][0])
            if executable.is_relative_to(fx):
                files[str(executable.relative_to(fx))] = receipt["chafa_sha256"]
        overrides.append({"kind": "source-media-cells", "field": box,
                          "cells": [{"cell": list(key), "expected": value} for key, value in cells.items()]})
    requirements = {"mode": mode, "bounds": cell_boxes, "picture_bounds": boxes, "sources": source, "renderer": renderer}
    if mode == "numbered-boxes":
        requirements["focused_image"] = 1
    result = {"status": "BLOCKED", "mode": mode, "representation_bound": True,
              "review_requirements": requirements, "source_files": files,
              "boxes": [{"cells": box} for box in cell_boxes], "native_protocol": False}
    result["status"] = representation_status(result, review)
    if result["status"] != "PASS":
        result["reason"] = "controller mode-specific source/representation review required"
    return result, None, overrides
