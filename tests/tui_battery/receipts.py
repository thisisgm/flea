"""Reconcile controller-run supplemental native evidence by content identity."""
import json
from pathlib import Path

from .evidence import Blocked, absolute, digest
from . import png
from .scenarios import legacy_shift_delete

REQUIRED = ("native.png", "grid-crop.png", "pty.log", "cells.json", "capabilities.json",
            "state.json", "commands.json", "diff.png", "overlay.png", "profile.json")
CALENDAR_CHECKS = ["exact-date", "selected-identity", "view-predicate", "clean-quit"]


def supplement(path, identity, terminals, controls, calendar, surfaces=()):
    expectations = {row["id"]: row.get("checks") for row in controls}
    expectations.update({row["id"]: row.get("checks") for row in surfaces})
    requested = [(terminal, "control", row["id"]) for terminal in terminals for row in controls]
    requested += [(terminal, "calendar", label) for terminal in terminals for label in calendar["states"]]
    requested += [(terminal, "surface", row["id"]) for terminal in terminals for row in surfaces]
    index = {}
    if path:
        path = absolute(path)
        manifest = json.loads((path / "manifest.json").read_text())
        for key in ("commit", "dirty_patch_sha256", "elf_sha256", "references", "untracked", "candidate_inputs"):
            if key not in identity and key == "candidate_inputs":
                continue
            if key not in identity:
                raise Blocked("supplemental expected identity lacks: " + key)
            if manifest.get("identity", {}).get(key) != identity[key]:
                raise Blocked("supplemental evidence identity mismatch: " + key)
        from .candidate import validate_calendar_inputs
        validate_calendar_inputs(path, identity, manifest)
        for row in manifest.get("records", []):
            key = (row["terminal"], row["kind"], row["id"])
            if key in index:
                raise Blocked("duplicate supplemental evidence record")
            if key not in requested:
                raise Blocked("unexpected supplemental evidence record: " + repr(key))
            index[key] = row
    records = []
    for terminal, kind, label in requested:
        result = {"terminal": terminal, "kind": kind, "id": label, "status": "BLOCKED"}
        try:
            row = index.get((terminal, kind, label))
            if not row:
                raise Blocked("missing controller native receipt")
            if kind == "control":
                from .control_native import validate_record
                validate_record(path, row, identity)
            relative = Path(row["directory"])
            if relative.is_absolute() or ".." in relative.parts or not relative.parts:
                raise Blocked("supplemental record directory escaped evidence bundle")
            directory = absolute(path / relative)
            if relative.parts != (terminal, label):
                raise Blocked("supplemental directory identifies another record")
            if kind == "control":
                from .candidate import validate_control_inputs, validate_operations
                validate_control_inputs(directory, identity)
                validate_operations(directory, row)
            hashes = row.get("sha256", {})
            for name in REQUIRED:
                file = absolute(directory / name)
                if not file.is_file() or hashes.get(name) != digest(file):
                    raise Blocked("missing or stale supplemental artifact: " + name)
            png.read(directory / "native.png")
            verdict = json.loads((directory / "verdict.json").read_text())
            if row.get("verdict_sha256") != digest(directory / "verdict.json"):
                raise Blocked("stale supplemental verdict")
            if kind == "calendar" and (verdict.get("terminal"), verdict.get("id")) != (terminal, label):
                raise Blocked("calendar verdict identifies another terminal or state")
            if any(verdict.get(key) != "PASS" for key in ("behavior", "cell_geometry", "color", "native_visual")):
                raise Blocked("supplemental evidence lacks a separate passing verdict for each gate")
            assertions = verdict.get("assertions")
            if not isinstance(assertions, dict) or not assertions or any(v is not True for v in assertions.values()):
                raise Blocked("supplemental control has absent or failed behavior assertions")
            required_checks = CALENDAR_CHECKS if kind == "calendar" else expectations.get(label)
            if not isinstance(required_checks, list) or not required_checks:
                raise Blocked(f"{kind} definition lacks explicit checks: {label}")
            if any(assertions.get(check) is not True for check in required_checks):
                raise Blocked("supplemental evidence omits required control variants")
            if kind == "calendar" and verdict.get("original_driver_sha256") != calendar["driver_sha256"]:
                raise Blocked("calendar evidence did not retain original acceptance provenance")
            if kind == "calendar":
                from .calendar_native import IDENTITY_FIELDS, ORIGINAL_MANIFEST_SHA256, status
                if (status(verdict) != "PASS" or verdict.get("original_manifest_sha256") != ORIGINAL_MANIFEST_SHA256
                        or verdict.get("selected_candidate_identity") != {k: identity[k] for k in IDENTITY_FIELDS}
                        or any(assertions.get(k) is not True for k in
                               ("detail-notice", "session-negatives", "after-proof-negatives"))):
                    raise Blocked("calendar record is synthetic or lacks accepted producer identity or negative controls")
            if kind == "surface":
                from .candidate import validate_surface
                validate_surface(directory, row, identity)
            commands = json.loads((directory / "commands.json").read_text())
            if not commands or any(type(c.get("exit_code")) is not int for c in commands):
                raise Blocked("supplemental control lacks real command exit codes")
            if not (directory / "pty.log").stat().st_size:
                raise Blocked("supplemental PTY transcript is empty")
            if kind == "control" and label == "shift-delete-legacy":
                legacy_shift_delete(directory / "pty.log")
            cells = json.loads((directory / "cells.json").read_text())
            if cells.get("unsupported") or not cells.get("grid") or cells.get("pending"):
                raise Blocked("supplemental cell grid is incomplete")
            if verdict.get("synthetic") or verdict.get("native_capture_sha256") != hashes["native.png"]:
                raise Blocked("supplemental evidence is synthetic or its native capture changed")
            result.update(status="PASS", source=str(directory), hashes=hashes, assertions=assertions)
        except (Blocked, OSError, ValueError, KeyError, TypeError, AttributeError, IndexError) as error:
            result["reason"] = str(error)
        records.append(result)
    return records
