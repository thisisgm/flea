"""Bind retained verdicts to the immutable capture inputs and configuration."""
import json
from pathlib import Path

from .evidence import Blocked, absolute, digest
from .adapters import font_profile

ARTIFACTS = ("native.png", "grid-crop.png", "cells.json", "pty.log", "graphics.log", "capabilities.json",
             "state.json", "commands.json", "diff.png", "overlay.png", "media.json", "dynamic-facts.json",
             "profile.json", "fixtures.json", "launch-spec.json", "relay-spec.json", "capture-config.json",
             "candidate-settings.json", "fx/state/flea/ui.json")


def configuration(terminal, preset, variant, state, profile=None, steps=None, seam=None):
    return {"terminal": terminal, "preset": preset, "variant": variant, "state": state,
            "profile": font_profile(profile),
            "steps": state["steps"] if steps is None else steps, "seam": seam or {}}


def artifacts(case):
    paths = list(ARTIFACTS)
    profile = case / "profile.json"
    if profile.is_file():
        data = json.loads(profile.read_text())
        path = absolute(Path(data["path"]))
        if not path.is_relative_to(case) or path == case:
            raise Blocked("terminal configuration escaped capture directory")
        paths.append(str(path.relative_to(case)))
        if "tmux_config_sha256" in data:
            paths.append("tmux.conf")
    return paths


def seal(case):
    paths = artifacts(case)
    missing = [name for name in paths if not absolute(case / name).is_file()]
    return {name: digest(absolute(case / name)) for name in paths if name not in missing}, missing


def verify(case, hashes, requested):
    if not requested:
        raise Blocked("requested capture configuration is required for reconciliation")
    for name in artifacts(case):
        if digest(absolute(case / name)) != hashes.get(name):
            raise Blocked("retained artifact changed or lacks its original hash: " + name)
    captured = json.loads((case / "capture-config.json").read_text())
    if captured != requested:
        raise Blocked("requested capture configuration differs from retained terminal, preset, size, font or setup")
    profile = json.loads((case / "profile.json").read_text())
    if profile.get("terminal") != requested["terminal"] or any(profile.get(k) != v for k, v in requested["profile"].items()):
        raise Blocked("terminal profile disagrees with capture configuration")
    for name in ("candidate-settings.json", "fx/state/flea/ui.json"):
        if json.loads((case / name).read_text()).get("keys") != requested["preset"]:
            raise Blocked("candidate preset disagrees with capture configuration")
    state = json.loads((case / "state.json").read_text())
    if (state.get("preset") != requested["preset"] or state.get("variant") != requested["variant"]
            or state.get("requested") != requested["state"]):
        raise Blocked("state receipt disagrees with capture configuration")
    cells = json.loads((case / "cells.json").read_text())
    if any(cells.get(key) != requested["variant"][key] for key in ("columns", "rows")):
        raise Blocked("captured cell size disagrees with requested variant")
