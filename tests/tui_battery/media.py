"""Native pixels and protocol evidence are distinct from cell-only fallback proof."""
import hashlib
import math
from pathlib import Path

from .evidence import Blocked, digest
from .geometry import CELL_HEIGHT, CELL_WIDTH, ORIGIN
from .references import Board
from .media_proof import placements
from . import fallback, png

FALLBACK = {"tui-list-image": "tui-image-chafa", "tui-list-symlink": "tui-image-chafa",
            "tui-3d-column": "tui-3d-chafa"}
FIXTURE_ROOT = "/fx"
NONEMPTY_PICTURE_COLORS = 4


def transport_for(adapter, forced_fallback):
    transports = {"kitty": "kitty", "ghostty": "kitty", "foot": "sixel", "alacritty": "half-blocks", "tmux": "half-blocks"}
    if adapter not in transports:
        raise Blocked("unknown media adapter: " + adapter)
    return "half-blocks" if forced_fallback else transports[adapter]


def fixture_path(fx, cwd, name):
    if cwd != FIXTURE_ROOT and not cwd.startswith(FIXTURE_ROOT + "/"):
        raise Blocked("media scenario cwd is outside the /fx fixture root: " + cwd)
    root = fx.resolve()
    path = (root / cwd.removeprefix(FIXTURE_ROOT).lstrip("/") / name).resolve()
    if Path(name).is_absolute() or not path.is_relative_to(root):
        raise Blocked("media fixture escapes the /fx fixture root: " + name)
    return path


def picture_status(requirements, review):
    decision = review.get("representation") or {}
    if any(decision.get(key) != value for key, value in requirements.items()):
        return "BLOCKED"
    if decision.get("approved") is True:
        return "PASS"
    return "FAIL" if decision.get("approved") is False else "BLOCKED"


def inspect(state, board, snapshot, image, grid_rect, caps, case, fx, boards, review, *, adapter, expected_transport, commands=None):
    boxes = board.media_boxes()
    mode = transport_for(adapter, caps["forced_fallback"])
    if expected_transport != mode:
        raise Blocked("scenario transport conflicts with adapter and forced-fallback setting")
    native = mode in ("kitty", "sixel")
    received = caps["candidate_received"]
    if boxes or state["id"] in FALLBACK.values():
        if native and not received[mode + "_positive"]:
            raise Blocked(f"{adapter} requires actual {mode} negotiation for this primary media scenario")
        if not native and (received["kitty_positive"] or received["sixel_positive"]):
            raise Blocked("half-block scenario unexpectedly enabled native graphics")
    if not boxes:
        return {"status": "PASS", "mode": "half-blocks" if state["id"] in FALLBACK.values() else "cells",
                "adapter": adapter, "expected_transport": mode}, None, []
    if not native and state["id"] not in FALLBACK:
        result, alternate, expectations = fallback.inspect(state, board, snapshot, grid_rect, case, fx, review, commands)
        result.update(adapter=adapter, expected_transport=mode, forced=caps["forced_fallback"])
        return result, alternate, expectations
    columns, rows = snapshot["columns"], snapshot["rows"]
    cw, ch = grid_rect[2] / columns, grid_rect[3] / rows
    proof, captures = [], []
    for left, top, width, height in boxes:
        x, y = (left - ORIGIN[0]) / CELL_WIDTH, (top - ORIGIN[1]) / CELL_HEIGHT
        cell_box = [math.floor(x), math.floor(y), math.ceil(width / CELL_WIDTH), math.ceil(height / CELL_HEIGHT)]
        rect = [round(grid_rect[0] + x * cw), round(grid_rect[1] + y * ch),
                max(1, round(width / CELL_WIDTH * cw)), max(1, round(height / CELL_HEIGHT * ch))]
        cropped = png.crop(image, rect)
        colors = {bytes(cropped[2][i:i + 3]) for i in range(0, len(cropped[2]), 4)}
        proof.append({"cells": cell_box, "native_rect": rect, "distinct_colors": len(colors)})
        captures.append(hashlib.sha256(bytes(cropped[2])).hexdigest())
        if native and len(colors) < NONEMPTY_PICTURE_COLORS:
            raise Blocked("negotiated graphics did not produce nonempty native picture pixels")
    display_proof = placements(case, received, mode) if native else []
    filename = state.get("cursor")
    if not filename:
        raise Blocked("media scenario names no selected fixture")
    fixture = fixture_path(fx, state["cwd"], filename)
    if not fixture.is_file():
        raise Blocked("selected media fixture is missing: " + filename)
    result = {"status": "PASS", "mode": mode, "boxes": proof, "fixture_sha256": digest(fixture),
              "forced": caps["forced_fallback"], "native_protocol": native,
              "adapter": adapter, "expected_transport": expected_transport, "placements": display_proof}
    if native:
        # Nonempty crop colors prove only that pixels arrived; the picture needs a controller decision bound to these identities.
        requirements = {"mode": mode, "fixture_sha256": result["fixture_sha256"], "boxes": proof,
                        "placements": display_proof, "native_capture_sha256": captures}
        result.update(status=picture_status(requirements, review), representation_bound=True,
                      review_requirements=requirements)
    alternate = None
    if not native:
        if state["id"] in FALLBACK:
            alternate = Board(boards / "boards" / (FALLBACK[state["id"]] + ".html"))
            result["picture_reference"] = FALLBACK[state["id"]]
    return result, alternate, []
