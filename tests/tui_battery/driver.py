"""Command-line entry point for the native terminal regression matrix."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import time
import uuid

from .adapters import Adapter, TERMINALS, font_profile, owned_identity
from .geometry import COLUMNS, ROWS, CELL_WIDTH, CELL_HEIGHT
from .evidence import Blocked, Commands, MARKER, absolute, digest, output_root, save, source_identity
from .fixtures import BUILT_IN_FEATURES, build, inventory, namespace, retain
from .keys import Keymap
from .native import WINDOW_ROUNDING, Display, DisplayLost, Terminal
from .edges import ground_edges
from .references import Board, compare_pixels, separator
from .scenarios import file_assertions, legacy_shift_delete, negatives, settled, steps, visual_review
from .protocol import capabilities
from .receipts import supplement
from .dynamic import masks as dynamic_masks
from .reconcile import retained, pixel_status, shutdown_failure
from .media import inspect as inspect_media, transport_for
from .case_identity import configuration, seal, verify as verify_case
from . import fake, png, preflight

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
# The smoke run proves the native path with one plain listing per terminal.
SMOKE_STATE = "tui-list-text"
REVIEW_PENDING = "controller native visual review required"


# Sample input: {"foot":{"font":"JetBrainsMono Nerd Font","font_size":14}}.
def load(path):
    value = json.loads(Path(path).read_text())
    if not isinstance(value, dict):
        raise Blocked("JSON object required: " + str(path))
    return value


def selection(value, choices):
    selected = value.split(",") if value else list(choices)
    if not selected or len(set(selected)) != len(selected) or any(x not in choices for x in selected):
        raise Blocked("filter contains unknown or duplicate values: " + str(value))
    return selected


def contract(boards, state_filter=None):
    spec = load(HERE / "states.json")
    ids = [state["id"] for state in spec["states"]]
    if len(ids) != 61 or len(set(ids)) != 61:
        raise Blocked("state contract requires 61 unique states")
    spec["primary_ids"] = list(ids)
    spec["states"] += spec["additional_states"]
    ids = [state["id"] for state in spec["states"]]
    if len(set(ids)) != len(ids):
        raise Blocked("duplicate additional state")
    found = {p.stem for p in (boards / "boards").glob("tui-*.png")}
    expected = set(ids) | {row["id"] for row in spec["excluded"]}
    if found != expected:
        raise Blocked("reference inventory mismatch: " + json.dumps({"missing": sorted(expected - found), "unexpected": sorted(found - expected)}))
    for name in expected:
        if not (boards / "boards" / (name + ".html")).is_file():
            raise Blocked("missing reference HTML: " + name)
    selected = selection(state_filter, ids)
    spec["states"] = [s for s in spec["states"] if s["id"] in selected]
    return spec


def palette(boards):
    board = Board(boards / "boards/tui-list-text.html")
    return {name: board.ink("var(--" + name + ")") for name in ("bg", "fg", "accent", "muted", "meta", "error", "green", "cyan")}


def private_theme(fx, colors):
    path = fx / "home/.local/state/omarchy/current/theme/colors.toml"
    path.parent.mkdir(parents=True)
    values = dict(colors, background=colors["bg"], foreground=colors["fg"], urgent=colors["error"])
    path.write_text("\n".join(f'{key} = "{value}"' for key, value in values.items()) + "\n")
    return digest(path)


def font_identity(commands, name, environment):
    result = commands.run(["fc-match", "-f", "%{file}\n", name], env=environment)
    paths = [Path(p) for p in result.stdout.decode().splitlines()]
    if not paths or not all(p.is_file() for p in paths):
        raise Blocked("fontconfig did not resolve the private terminal font")
    inventory = commands.run(["fc-list"], env=environment)
    return {"font_files": {str(p): digest(p) for p in paths},
            "fontconfig_inventory_sha256": hashlib.sha256(inventory.stdout).hexdigest(),
            "fontconfig_version": commands.run(["fc-match", "--version"], env=environment).stderr.decode(errors="replace")}


def close_terminal(terminal, case):
    """Reuse the accepted pidfd owner through close and exceptional group cleanup."""
    from .calendar_native import hold_group, reap_group, shielded
    anchor = None
    with shielded():
        try:
            anchor = hold_group(case)
            return terminal.close()
        finally:
            try:
                cleanup = reap_group(case, anchor)
                save(case / "cleanup.json", cleanup)
                if cleanup["signals"] or cleanup["live"]:
                    raise Blocked("terminal group outlived close: " + json.dumps(cleanup))
            finally:
                if anchor is not None:
                    os.close(anchor)


def seal_case(case, verdict, capture_config):
    try:
        verdict["artifact_sha256"], missing = seal(case)
        from .candidate import seal_extra
        missing += seal_extra(case, verdict["artifact_sha256"])
        if missing:
            verdict["incomplete_artifacts"] = missing
            verdict.setdefault("reason", "capture identity or evidence is incomplete")
            if verdict["status"] == "PASS":
                verdict["status"] = "BLOCKED"
        elif verdict["status"] == "PASS":
            verify_case(case, verdict["artifact_sha256"], capture_config)
    except (Blocked, OSError, ValueError, KeyError) as error:
        verdict.update(artifact_sha256={}, sealing_reason=str(error))
        verdict.setdefault("reason", str(error))
        if verdict["status"] == "PASS":
            verdict["status"] = "BLOCKED"


def execute_pair(args, root, display, terminal_name, state, variant, colors, identity, profiles, reviews, overrides, seams,
                 fixture_setup=None):
    relative = f"{terminal_name}/{state['id']}/{variant['id']}"
    case = root.directory(relative)
    (case / MARKER).write_text(uuid.uuid4().hex + "\n")
    commands, journal, terminal, lost, fx, cups, interrupted = Commands(), [], None, None, None, None, None
    verdict = {"terminal": terminal_name, "state": state["id"], "variant": variant["id"], "status": "BLOCKED"}
    save(case / "candidate-identity.json", identity)
    capture_config = None
    try:
        adapter = Adapter(terminal_name, case, owned_identity(), colors, profiles.get(terminal_name))
        packages = adapter.preflight(commands)
        profile = adapter.configure()
        profile["config_check"] = adapter.validate(commands)
        fonts = font_identity(commands, profile["font"], adapter.environment())
        fx, fixture = build(case, state["family"], args.fixtures)
        if fixture_setup:
            save(case / "catalog-case.json", fixture_setup(fx))
        available = (set(fixture.get("features", [])) - {"cups-sink"}) | BUILT_IN_FEATURES
        seam = seams.get(state["id"], {})
        available.update(set(seam.get("features", [])) - {"cups-sink"})
        if seam.get("private_cups"):
            if seam["private_cups"] is not True or type(seam.get("cups_reject", False)) is not bool:
                raise Blocked("private CUPS fixture flags must be booleans")
            from .bundle.cups import PrivateSink
            cups = PrivateSink(case, fx, reject=seam.get("cups_reject", False))
            cups.start()
            available.add("cups-sink")
        missing = set(state["requires"]) - available
        if missing:
            raise Blocked("fixture integration required: " + ", ".join(sorted(missing)))
        environment = seam.get("environment", {})
        if any(not key.startswith("FLEA_TUI_") for key in environment):
            raise Blocked("test seams may only set FLEA_TUI_ variables")
        theme_hash = private_theme(fx, colors)
        settings = fx / "state/flea/ui.json"
        settings.parent.mkdir(parents=True, exist_ok=True)
        content = json.loads(settings.read_text()) if settings.is_file() else {}
        content["keys"] = args.preset
        settings.write_text(json.dumps(content) + "\n")
        save(case / "candidate-settings.json", content)
        active = dict(state, steps=overrides.get(state["id"], state["steps"]))
        capture_config = configuration(terminal_name, args.preset, variant, state,
                                       profiles.get(terminal_name), active["steps"], seam)
        save(case / "capture-config.json", capture_config)
        options = {}
        if cups:
            options["cups_server"] = "/fx/runtime/cups.sock"
        if seam.get("tool_policy"):
            options["tool_policy"] = seam["tool_policy"]
        candidate = namespace(args.binary, fx, state["cwd"], environment, **options)
        if seam.get("tool_policy"):
            save(case / "tool-policy.json", load(fx / "data/tool-policy.json"))
        keymap = Keymap(REPO / "keys.toml", args.preset)
        terminal = Terminal(display, adapter, case, commands, args.timeout, keymap.resolve("quit"))
        start = terminal.launch(candidate, dict(PATH="/usr/bin:/bin", LANG="C.UTF-8", PYTHONPATH=str(REPO / "tests")),
                                graphics_negative="graphics-negative" in state["requires"])
        initial = settled(terminal, {"contains": [Path(state["cwd"]).name]}, args.timeout)
        rows, columns, width, height = initial["pty"]
        if rows != ROWS or columns != COLUMNS or not width or not height:
            raise Blocked(f"diagnostic PTY must report {COLUMNS}x{ROWS} cells and nonzero pixel metrics; "
                          f"it reports {columns}x{rows} cells and {width}x{height} px")
        calibration_image = display.capture(case / "calibration.png")
        client = terminal.client
        ox, oy = display.monitor["x"], display.monitor["y"]
        client_rect = [client["at"][0] - ox, client["at"][1] - oy, *client["size"]]
        rect = separator(calibration_image, client_rect, initial["pty"], initial["grid"])
        calibration = {"cell": [width // columns, height // rows],
                       "padding": [client["size"][0] - width, client["size"][1] - height],
                       "origin_padding": [rect[0] - client_rect[0], rect[1] - client_rect[1]],
                       "pty": initial["pty"], "client": client, "grid": rect,
                       "browser_cell": [CELL_WIDTH, CELL_HEIGHT], "resampling_allowed": False}
        save(case / "profile.json", dict(profile, terminal=terminal_name, calibrated=True, calibration=calibration, fonts=fonts,
             packages=packages, theme_sha256=theme_hash, original_rounding=display.original_rounding))
        if state.get("picture_steps"):
            terminal.fixture = fx
            from .bundle.pictures import configure
            configure(terminal, Board(args.boards / "boards" / (state["id"] + ".html")),
                      Board(args.boards / "boards/tui-3d-column.html"), rect, calibration["cell"])
        steps(terminal, active, fx, keymap, journal, args.timeout)
        if (variant["columns"], variant["rows"]) != (COLUMNS, ROWS):
            terminal.resize(variant["columns"], variant["rows"], calibration)
        rule = state["predicate"]
        if state["id"] == "tui-long":
            rule = dict(rule, contains=rule["contains"] + (["┃"] if variant["id"] != "rest" else []),
                        absent=["┃"] if variant["id"] == "rest" else [])
        if state["id"] == "tui-small":
            rule = {"contains": ["wide"]}
        if state.get("picture_steps"):
            from .candidate import picture_steps
            save(case / "picture-steps.json", picture_steps(state, journal, case / "pty.log"))
        snapshot = settled(terminal, rule, args.timeout, transient=state["id"] in ("tui-long", "tui-progress"))
        if snapshot["unsupported"]:
            raise Blocked("cell parser encountered unsupported output: " + str(snapshot["unsupported"]))
        save(case / "cells.json", snapshot)
        image = display.capture(case / "native.png")
        client = display.client(adapter.identity)
        client_rect = [client["at"][0] - ox, client["at"][1] - oy, *client["size"]]
        if state["id"] == "tui-small":
            px, py = calibration["origin_padding"]
            rect = [client_rect[0] + px, client_rect[1] + py, snapshot["pty"][2], snapshot["pty"][3]]
        else:
            rect = separator(image, client_rect, snapshot["pty"], snapshot["grid"])
        edges = ground_edges(image, client_rect, rect, calibration["cell"], WINDOW_ROUNDING)
        ref = args.boards / "boards" / (state["id"] + ".html")
        board = Board(ref)
        masks, facts = dynamic_masks(board, snapshot, candidate, state["cwd"], commands)
        save(case / "dynamic-facts.json", {"mask": masks, "facts": facts})
        caps = capabilities(case / "pty.log")
        caps["candidate_received"] = capabilities(case / "pty.log", "candidate-input")
        caps["forced_fallback"] = "graphics-negative" in state["requires"]
        save(case / "capabilities.json", caps)
        visual = visual_review(case / "native.png", reviews, digest(ref.with_suffix(".png")))
        media, alternate, media_masks = inspect_media(state, board, snapshot, image, rect, caps, case, fx, args.boards,
            visual.get("review", {}), adapter=terminal_name,
            expected_transport=transport_for(terminal_name, caps["forced_fallback"]), commands=commands)
        save(case / "media.json", media)
        comparison = board.compare(snapshot, masks=masks + media_masks, origin=variant["origin"],
                                   resting=variant["id"] == "rest", picture_reference=alternate)
        reference_rect = [*variant["origin"], round(variant["columns"] * CELL_WIDTH), variant["rows"] * CELL_HEIGHT]
        pixels = compare_pixels(case / "native.png", ref.with_suffix(".png"), rect, case, reference_rect)
        behavior = file_assertions(state, fx)
        if state["id"] in ("tui-delete", "tui-delete-focus"):
            behavior["legacy_shift_delete"] = legacy_shift_delete(case / "pty.log")
        verdict.update(behavior=behavior, cells=comparison, pixels=pixels, visual=visual, media=media, edges=edges,
                       predicate={"status": "PASS", "rule": rule}, elf_sha256=identity["elf_sha256"])
        if state["id"] == "tui-sort-size":
            expected = board.cells()
            rows = ["".join(expected[(x, y)]["ch"] for x in range(COLUMNS)) for y in range(2, ROWS - 2)]
            positions = {name: next((i for i, line in enumerate(rows)
                                    if len(line.split("│")) >= 3 and re.search(r"(?:^| )" + re.escape(name) + r"(?: |$)", line.split("│")[1])), 99)
                         for name in ("A", "Code", "Docs", "Empty", "Locked", "Many", "Media")}
            if list(sorted(positions, key=positions.get)) != list(positions):
                raise Blocked("size-sort reference folder order conflicts with the name-order ruling")
        if state["id"] == "tui-keys" and re.search(r"shift-delete\s*/\s*X", "".join(r["text"] for r in board.runs), re.I):
            raise Blocked("keys reference still advertises the removed X delete alternative")
        statuses = [behavior["status"], comparison["geometry"]["status"], comparison["color"]["status"], pixel_status(pixels, visual), visual["status"], media["status"], edges["status"]]
        verdict["status"] = "FAIL" if "FAIL" in statuses else ("PASS" if statuses == ["PASS"] * 7 else "BLOCKED")
        if edges["status"] == "FAIL":
            verdict["reason"] = edges["reason"]
    except (Blocked, ValueError, OSError, KeyError, AssertionError, RuntimeError) as error:
        verdict["reason"] = str(error)
        lost = error if isinstance(error, DisplayLost) else None
        if terminal and terminal.client and not (case / "native.png").exists():
            try:
                display.capture(case / "failure.png")
                save(case / "last-cells.json", terminal.snapshot())
            except (Blocked, OSError, ValueError, RuntimeError) as capture_error:
                verdict["failure_capture_reason"] = str(capture_error)
    finally:
        from .calendar_native import shielded
        with shielded():
            if terminal and terminal.launched:
                try:
                    verdict["terminal_exit"] = close_terminal(terminal, case)
                    shutdown = load(case / "relay-exit.json")
                    verdict["shutdown"] = {k: v for k, v in shutdown.items() if k != "screen"}
                    failure = shutdown_failure(verdict["terminal_exit"], shutdown)
                    if failure:
                        verdict.update(status="FAIL", shutdown_reason=failure)
                except (Blocked, OSError, ValueError, RuntimeError) as error:
                    verdict.update(status="FAIL", shutdown_reason=str(error))
                except (KeyboardInterrupt, SystemExit) as error:
                    interrupted = error
                    verdict.update(status="FAIL", shutdown_reason="terminal cleanup interrupted: " + type(error).__name__)
            if cups:
                try:
                    receipt = cups.close()
                    if receipt["errors"]:
                        raise Blocked("private CUPS receiver failed: " + "; ".join(receipt["errors"]))
                except (Blocked, OSError, ValueError, RuntimeError) as error:
                    verdict.update(status="FAIL", shutdown_reason=str(error))
                except (KeyboardInterrupt, SystemExit) as error:
                    interrupted = interrupted or error
                    verdict.update(status="FAIL", shutdown_reason="private CUPS cleanup interrupted: " + type(error).__name__)
            if fx:
                try:
                    retain(fx, case)
                except (Blocked, OSError) as error:
                    verdict["fixture_retain_reason"] = str(error)
                    verdict.setdefault("reason", str(error))
                    if verdict["status"] == "PASS":
                        verdict["status"] = "BLOCKED"
            save(case / "state.json", {"requested": state, "variant": variant, "preset": args.preset, "input": journal})
            save(case / "commands.json", commands.records)
            seal_case(case, verdict, capture_config)
            save(case / "verdict.json", verdict)
    if interrupted:
        raise interrupted
    if lost:
        raise lost
    return verdict


def review_pending(verdict):
    """A state whose every machine check passed and that now waits only on the controller's look at native.png."""
    checks = (verdict.get("behavior"), (verdict.get("cells") or {}).get("geometry"), (verdict.get("cells") or {}).get("color"),
              verdict.get("media"), verdict.get("edges"))
    return (verdict["status"] == "BLOCKED" and (verdict.get("visual") or {}).get("reason") == REVIEW_PENDING
            and all((check or {}).get("status") == "PASS" for check in checks)
            and not verdict.get("shutdown_reason") and not verdict.get("incomplete_artifacts")
            and not verdict.get("fixture_retain_reason") and not verdict.get("sealing_reason"))


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--binary", type=Path)
    result.add_argument("--boards", type=Path, required=True)
    result.add_argument("--output", type=Path)
    result.add_argument("--fixtures", type=Path)
    result.add_argument("--terminals", default=",".join(TERMINALS))
    result.add_argument("--states")
    result.add_argument("--profiles", type=Path)
    result.add_argument("--steps", type=Path, help="logical setup steps keyed by state, for integrated keymap changes")
    result.add_argument("--reviews", type=Path, help="native visual approvals keyed by terminal/state/variant and image hashes")
    result.add_argument("--seams", type=Path, help="explicit test-only fixture features and FLEA_TUI_ environment by state")
    result.add_argument("--identity-inputs", type=Path, help="additional named catalog/roster/font input paths to hash")
    result.add_argument("--supplement", type=Path, help="identity-matched controller control/calendar evidence bundle")
    result.add_argument("--reconcile", type=Path, help="review retained captures from this run directory instead of launching terminals")
    result.add_argument("--preset", choices=("default", "vim", "mac", "windows", "yazi"), default="default")
    result.add_argument("--timeout", type=float, default=15)
    result.add_argument("--dry-run", action="store_true")
    result.add_argument("--list", action="store_true")
    result.add_argument("--smoke", action="store_true", help=f"one state ({SMOKE_STATE}) per terminal, to prove the native path")
    result.add_argument("--preflight", action="store_true", help="refuse in seconds unless the display is free for the battery")
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    root = None
    try:
        if not 0 < args.timeout <= 120:
            raise Blocked("timeout must be in (0, 120] seconds")
        if args.preflight:
            print(json.dumps(preflight.display(Commands())))
            return 0
        if args.smoke:
            if args.states:
                raise Blocked("--smoke selects its own state")
            args.states = SMOKE_STATE
        args.boards = absolute(args.boards)
        spec = contract(args.boards, args.states)
        terminals = selection(args.terminals, TERMINALS)
        controls = load(HERE / "controls.json")["controls"]
        calendar = load(HERE / "calendar.json")
        surfaces = load(HERE / "surfaces.json")["states"]
        planned = [{"terminal": t, "state": s["id"], "variant": v["id"]}
                   for t in terminals for s in spec["states"] for v in s["variants"]]
        from .candidate import inventory as candidate_inventory, completion, bind_identity
        complete_spec = contract(args.boards, None)
        required = candidate_inventory(complete_spec)
        manifest = {"schema": 1, "mode": "dry-run" if args.dry_run else "native", "smoke": args.smoke, "preset": args.preset, "excluded": spec["excluded"],
                    "reserved": spec["reserved"], "requested": planned,
                    "required_primary_records": len(complete_spec["primary_ids"]) * len(TERMINALS),
                    "required_additional_records": len(complete_spec["additional_states"]) * len(TERMINALS),
                    "required_minimum_native_captures": required["counts"]["captures"], "complete_inventory": required,
                    "records": [], "controls": [], "calendar": calendar,
                    "additional_surfaces": surfaces,
                    "masking_policy": "Backend-derived right-anchored disk cells and source-derived media cells; every character, width, space and rendition is checked.",
                    "tolerance_policy": "Cell text, geometry, focus and ink roles are exact; native antialiasing needs controller review."}
        manifest["controls"] = [dict(row, terminal=t, kind="control", status="BLOCKED", reason="not run") for t in terminals for row in controls]
        manifest["calendar"]["records"] = [{"terminal": t, "kind": "calendar", "id": label, "status": "BLOCKED", "reason": "not run"}
                                             for t in terminals for label in calendar["states"]]
        manifest["registered_surfaces"] = surfaces
        manifest["additional_surfaces"] = [dict(row, terminal=t, kind="surface") for t in terminals for row in surfaces]
        manifest["logical_pairs_requested"] = len(terminals) * len(spec["states"])
        fixtures_ready = True
        if args.fixtures:
            manifest["fixture_families"] = inventory(args.fixtures, spec["states"])
            fixtures_ready = all(row["status"] == "present" for row in manifest["fixture_families"].values())
        if args.list:
            print(json.dumps(manifest, ensure_ascii=False, indent=2))
            return 0 if fixtures_ready else 2
        if not args.output:
            raise Blocked("--output is required beneath a marked mktemp root")
        root = output_root(args.output)
        colors = palette(args.boards)
        commands = Commands()
        if args.dry_run:
            manifest["negative_controls"] = negatives()
            for terminal in terminals:
                case = root.directory(terminal + "/adapter")
                receipt = fake.adapter_diagnostic(terminal, case, commands, colors)
                manifest["records"].append(dict(receipt, status="ADAPTER-PASS"))
            manifest.update(native_acceptance=False, native_captures=0, native_accepted=0, planned_state_variants=len(planned),
                            status="DRY-RUN-PASS" if fixtures_ready else "BLOCKED")
            code = 0 if fixtures_ready else 2
        else:
            if not args.binary or not args.binary.is_absolute() or not os.access(args.binary, os.X_OK):
                raise Blocked("--binary must name an absolute executable candidate")
            with args.binary.open("rb") as candidate_file:
                if candidate_file.read(4) != b"\x7fELF":
                    raise Blocked("--binary must name the candidate ELF, not a wrapper")
            if os.geteuid() == 0:
                raise Blocked("native Locked proof requires an unprivileged controller")
            identity = source_identity(REPO, args.binary, args.boards, commands)
            identity = bind_identity(identity, load(args.identity_inputs) if args.identity_inputs else None, args.boards)
            manifest["candidate_inputs"] = identity["candidate_inputs"]
            save(root.child("identity.json"), identity)
            profiles = load(args.profiles) if args.profiles else {}
            for name, profile in profiles.items():
                if name not in TERMINALS or not isinstance(profile, dict):
                    raise Blocked(f"invalid private terminal profile: {name}")
                font_profile(profile)
            reviews = load(args.reviews) if args.reviews else {}
            overrides = load(args.steps) if args.steps else {}
            seams = load(args.seams) if args.seams else {}
            try:
                if args.reconcile:
                    configurations = {f"{t}/{s['id']}/{v['id']}": configuration(t, args.preset, v, s,
                        profiles.get(t), overrides.get(s["id"], s["steps"]), seams.get(s["id"]))
                        for t in terminals for s in spec["states"] for v in s["variants"]}
                    manifest["records"] = retained(args.reconcile, identity, planned, args.boards, reviews, configurations)
                else:
                    manifest["display_preflight"] = preflight.display(commands)
                    with Display(commands, uuid.uuid4().hex) as display:
                        for terminal in terminals:
                            for state in spec["states"]:
                                for variant in state["variants"]:
                                    verdict = execute_pair(args, root, display, terminal, state, variant, colors, identity, profiles, reviews, overrides, seams)
                                    manifest["records"].append(verdict)
            except Blocked as error:
                manifest["display_reason"] = str(error)
                recorded = {(v["terminal"], v["state"], v["variant"]) for v in manifest["records"]}
                for row in planned:
                    if (row["terminal"], row["state"], row["variant"]) not in recorded:
                        manifest["records"].append(dict(row, status="BLOCKED", reason=str(error)))
            additional = supplement(args.supplement, identity, terminals, controls, calendar, surfaces)
            manifest["controls"] = [r for r in additional if r["kind"] == "control"]
            manifest["calendar"]["records"] = [r for r in additional if r["kind"] == "calendar"]
            manifest["additional_surfaces"] = [r for r in additional if r["kind"] == "surface"]
            counts = Counter(row["status"] for row in manifest["records"])
            accepted = all(r["status"] == "PASS" for r in manifest["records"] + additional) and not manifest.get("display_reason")
            manifest.update(counts=dict(counts), native_acceptance=accepted, native_accepted=counts["PASS"],
                            native_captures=sum(1 for _ in (args.reconcile or root.path).glob("*/*/*/native.png")),
                            complete_primary_matrix=accepted and set(spec["primary_ids"]).issubset(s["id"] for s in spec["states"]) and len(terminals) == 5,
                            status="PASS" if accepted else ("FAIL" if counts["FAIL"] else "BLOCKED"))
            manifest.update(completion(manifest, required))
            code = 0 if accepted else (1 if counts["FAIL"] else 2)
            if args.smoke and not accepted and not manifest.get("display_reason") and manifest["records"] \
                    and all(r["status"] == "PASS" or review_pending(r) for r in manifest["records"]):
                manifest["status"], code = "SMOKE-PASS", 0
        save(root.child("commands.json"), commands.records)
        save(root.child("manifest.json"), manifest)
        (root.child("RESULT.md")).write_text(f"Mode: {manifest['mode']}. Status: {manifest['status']}.\n"
            f"Requested state variants: {len(planned)}. Native accepted: {manifest['native_accepted']}.\n")
        print(json.dumps({"output": str(root.path), "status": manifest["status"], "native_accepted": manifest["native_accepted"]}))
        return code
    except (Blocked, OSError, ValueError, KeyError, AssertionError, RuntimeError) as error:
        receipt = {"status": "BLOCKED", "reason": str(error), "native_acceptance": False}
        if root and not (root.path / "manifest.json").exists():
            save(root.child("manifest.json"), receipt)
        print(json.dumps(receipt), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
