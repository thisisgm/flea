"""Apply native visual decisions to retained captures without recapturing them."""
import json
from pathlib import Path

from .evidence import Blocked, absolute, digest, save
from .scenarios import visual_review
from .case_identity import verify
from .fallback import representation_status, verify_sources


def pixel_status(pixels, visual):
    if visual["status"] != "PASS":
        return visual["status"]
    if pixels["status"] == "REVIEW":
        return "PASS"
    constraint = visual.get("review", {}).get("geometry_constraint", {})
    if (constraint.get("native_size") == pixels.get("native_size")
            and constraint.get("reference_size") == pixels.get("reference_size") and constraint.get("reason")):
        return "PASS"
    return pixels["status"]


def shutdown_failure(terminal_exit, shutdown):
    if not isinstance(terminal_exit, dict) or type(terminal_exit.get("exit_code")) is not int or terminal_exit["exit_code"] != 0:
        return "terminal process did not exit successfully"
    if not isinstance(shutdown, dict):
        return "shutdown record is not an object"
    if (type(shutdown.get("candidate_exit")) is not int or shutdown["candidate_exit"] != 0
            or shutdown.get("termios_restored") is not True or shutdown.get("relay_termios_restored") is not True):
        return "candidate exit or TTY restoration failed"
    return None


def retained(source, identity, planned, boards, reviews, configurations=None):
    source = absolute(source)
    saved_identity = json.loads((source / "identity.json").read_text())
    for key in ("commit", "dirty_patch_sha256", "elf_sha256", "references", "untracked", "candidate_inputs"):
        if key == "candidate_inputs" and key not in identity:
            continue
        if saved_identity.get(key) != identity[key]:
            raise Blocked("retained capture identity changed: " + key)
    results = []
    for row in planned:
        relative = f"{row['terminal']}/{row['state']}/{row['variant']}"
        directory = absolute(source / relative)
        result = dict(row, status="BLOCKED", evidence=str(directory))
        try:
            verdict = json.loads((directory / "verdict.json").read_text())
            if any(verdict.get(k) != row[k] for k in ("terminal", "state", "variant")):
                raise Blocked("retained verdict identifies another case")
            if verdict.get("fixture_retain_reason") or verdict.get("sealing_reason") or verdict.get("incomplete_artifacts"):
                raise Blocked("retained capture has a permanent fixture or sealing failure")
            hashes = verdict.get("artifact_sha256", {})
            failure = shutdown_failure(verdict.get("terminal_exit"), verdict.get("shutdown", {}))
            if failure:
                result["status"] = "FAIL"
                raise Blocked(failure)
            from .bundle.outputs import load
            raw_exit, raw_shutdown = load(directory / "launch-exit.json"), load(directory / "relay-exit.json")
            failure = shutdown_failure(raw_exit, raw_shutdown)
            if failure or raw_exit.get("exit_code") != verdict["terminal_exit"]["exit_code"] or any(
                    raw_shutdown.get(key) != verdict["shutdown"].get(key)
                    for key in ("candidate_exit", "termios_restored", "relay_termios_restored")):
                result["status"] = "FAIL"
                raise Blocked(failure or "raw shutdown receipt disagrees with passing verdict")
            verify(directory, hashes, (configurations or {}).get(relative))
            if "candidate_inputs" in identity:
                from .candidate import verify_extra
                verify_extra(directory, hashes)
            visual = visual_review(directory / "native.png", reviews, digest(boards / "boards" / (row["state"] + ".png")))
            checks = [verdict["behavior"]["status"], verdict["cells"]["geometry"]["status"],
                      verdict["cells"]["color"]["status"], pixel_status(verdict["pixels"], visual),
                      (verdict.get("edges") or {}).get("status", "BLOCKED")]
            media = json.loads((directory / "media.json").read_text())
            verify_sources(media, directory / "fx")
            media_status = representation_status(media, visual.get("review", {}))
            checks.append(media_status)
            if verdict.get("reason") not in (None, "", "controller native visual review required") or verdict.get("shutdown_reason"):
                raise Blocked(verdict.get("reason") or verdict.get("shutdown_reason"))
            result.update(status="PASS" if checks == ["PASS"] * 6 else ("FAIL" if "FAIL" in checks else "BLOCKED"),
                          visual=visual, checks=checks, artifact_sha256=hashes)
        except (Blocked, OSError, ValueError, KeyError, TypeError, AttributeError, IndexError) as error:
            result["reason"] = str(error)
        results.append(result)
    return results
