"""Native pixels and protocol evidence are distinct from cell-only fallback proof."""
import math

from .evidence import Blocked, digest
from .references import Board
from .media_proof import placements
from . import fallback, png

FALLBACK = {"tui-list-image": "tui-image-chafa", "tui-list-symlink": "tui-image-chafa",
            "tui-3d-column": "tui-3d-chafa"}


def transport_for(adapter, forced_fallback):
    transports = {"kitty": "kitty", "ghostty": "kitty", "foot": "sixel", "alacritty": "half-blocks", "tmux": "half-blocks"}
    if adapter not in transports:
        raise Blocked("unknown media adapter: " + adapter)
    return "half-blocks" if forced_fallback else transports[adapter]


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
    proof = []
    for left, top, width, height in boxes:
        x, y = (left - 4) / 8.4, (top - 9) / 22
        cell_box = [math.floor(x), math.floor(y), math.ceil(width / 8.4), math.ceil(height / 22)]
        rect = [round(grid_rect[0] + x * cw), round(grid_rect[1] + y * ch),
                max(1, round(width / 8.4 * cw)), max(1, round(height / 22 * ch))]
        cropped = png.crop(image, rect)
        colors = {bytes(cropped[2][i:i + 3]) for i in range(0, len(cropped[2]), 4)}
        proof.append({"cells": cell_box, "native_rect": rect, "distinct_colors": len(colors)})
        if native:
            if len(colors) < 4:
                raise Blocked("negotiated graphics did not produce nonempty native picture pixels")
    display_proof = placements(case, received, mode) if native else []
    filename = state.get("cursor")
    fixture = fx / state["cwd"].removeprefix("/fx/") / filename if filename else None
    fixture_hash = digest(fixture) if fixture and fixture.is_file() else None
    result = {"status": "PASS", "mode": mode, "boxes": proof, "fixture_sha256": fixture_hash,
              "forced": caps["forced_fallback"], "native_protocol": native,
              "adapter": adapter, "expected_transport": expected_transport, "placements": display_proof}
    alternate = None
    if not native:
        if state["id"] in FALLBACK:
            alternate = Board(boards / "boards" / (FALLBACK[state["id"]] + ".html"))
            result["picture_reference"] = FALLBACK[state["id"]]
    return result, alternate, []
