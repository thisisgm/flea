"""Compose the existing native battery and accepted supplement producers."""
import base64
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys
import time

from .bundle.outputs import regular, digest, load, output_proof, cups_proof, validate_operations
from .adapters import TERMINALS
from .evidence import Blocked, Commands, absolute, output_root, save, source_identity

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
LIFECYCLE = ("launch-exit.json", "relay-exit.json", "launch-start.json", "relay-start.json")
HOOKS = ("deadline.json", "disabled-state.json", "signal.json", "process-census.json", "cleanup.json",
         "decoder-replay.json", "dropped-responses.json", "catalog-case.json", "picture-steps.json", "tool-policy.json")
SURFACE_REFERENCES = tuple("boards/" + name + "." + suffix for name in
                           ("filter-close", "search-contents", "jobs", "open-refused") for suffix in ("png", "html"))




def inventory(spec, terminals=TERMINALS):
    controls = load(HERE / "controls.json")["controls"]
    calendar = load(HERE / "calendar.json")
    surfaces = load(HERE / "surfaces.json")["states"]
    requested = [{"terminal": t, "state": s["id"], "variant": v["id"]}
                 for t in terminals for s in spec["states"] for v in s["variants"]]
    result = {"requested": requested}
    for kind, rows in (("controls", controls), ("calendar", [{"id": label} for label in calendar["states"]]),
                       ("surfaces", surfaces)):
        result[kind] = [dict(row, terminal=t) for t in terminals for row in rows]
    result["counts"] = {"boards": len(spec["states"]), "pairs": len(terminals) * len(spec["states"]),
                        "captures": len(requested), **{k: len(result[k]) for k in ("calendar", "controls", "surfaces")}}
    return result


def completion(manifest, required):
    """Require the exact nonempty inventory; counts and claimed completion flags are not evidence."""
    problems = []
    if manifest.get("mode") != "native" or manifest.get("smoke") or manifest.get("display_reason"):
        problems.append("a complete native run is required")
    if not manifest.get("candidate_inputs"):
        problems.append("candidate catalog/keymap input identity is missing")
    if manifest.get("candidate_inputs", {}).get("missing_surface_references"):
        problems.append("supplemental surface reference identity is incomplete")
    groups = (("requested", required["requested"], manifest.get("requested", []), ("terminal", "state", "variant")),
              ("records", required["requested"], manifest.get("records", []), ("terminal", "state", "variant")),
              ("controls", required["controls"], manifest.get("controls", []), ("terminal", "id")),
              ("calendar", required["calendar"], manifest.get("calendar", {}).get("records", []), ("terminal", "id")),
              ("surfaces", required["surfaces"], manifest.get("additional_surfaces", []), ("terminal", "id")))
    for group, expected, actual, keys in groups:
        wanted = {tuple(row[k] for k in keys) for row in expected}
        held = [tuple(row.get(k) for k in keys) for row in actual]
        if not wanted or len(held) != len(wanted) or set(held) != wanted:
            problems.append(group + " inventory is missing, duplicated or unexpected")
        if group != "requested" and any(row.get("status") != "PASS" for row in actual):
            problems.append(group + " has failed or unproven records")
    return {"complete_reference_matrix": not problems, "completion_blockers": problems}


def input_identity(repo=REPO, extra=None, boards=None):
    """Bind all shipped catalog inputs, the source index, policy and the actual keymap."""
    repo = absolute(repo)
    required = ["keys.toml", "i18n/source-index.json", "i18n/translation-manifest.json", "i18n/en/flea.json"]
    names = set(required) | {str(p.relative_to(repo)) for p in (repo / "i18n").rglob("*.json")}
    files = {name: digest(absolute(repo / name)) for name in sorted(names)}
    auxiliary = {}
    for name, path in sorted((extra or {}).items()):
        path = absolute(Path(path))
        auxiliary[name] = {"path": str(path), "sha256": digest(path)}
    references = {name: digest(absolute(Path(boards) / name)) for name in SURFACE_REFERENCES
                  if boards is not None and (Path(boards) / name).is_file()}
    return {"files": files, "auxiliary": auxiliary, "surface_references": references,
            "missing_surface_references": [name for name in SURFACE_REFERENCES if name not in references]}


def bind_identity(identity, extra=None, boards=None):
    return dict(identity, candidate_inputs=input_identity(REPO, extra, boards))


def catalog_artifacts(case):
    if not (case / "catalog-case.json").is_file():
        return {}
    receipt = load(case / "catalog-case.json")
    identity = load(case / "candidate-identity.json")
    inputs = identity.get("candidate_inputs")
    if not inputs or receipt.get("candidate_inputs") != inputs or not receipt.get("artifacts"):
        raise Blocked("catalog case lacks its candidate input identity or private artifacts")
    result = {}
    for name, sha in receipt["artifacts"].items():
        path = absolute(case / "fx" / name)
        if not path.is_relative_to(case / "fx") or digest(path) != sha:
            raise Blocked("retained catalog input is missing or changed")
        result[str(path.relative_to(case))] = sha
    return result


def seal_extra(case, hashes):
    """Seal real lifecycle and hook files; an absent hook never becomes a positive receipt."""
    missing = []
    for name in LIFECYCLE + HOOKS:
        path = absolute(case / name)
        if path.is_file():
            hashes[name] = digest(path)
        elif name in LIFECYCLE:
            missing.append(name)
    for path in case.glob("*.json"):
        if path.name != "verdict.json":
            hashes[path.name] = digest(absolute(path))
    for directory in ("cups", "pictures"):
        for path in (case / directory).glob("*"):
            hashes[str(path.relative_to(case))] = digest(absolute(path))
    for path in (case / "fx/runtime/picture-native").glob("*"):
        hashes[str(path.relative_to(case))] = digest(absolute(path))
    hashes.update(catalog_artifacts(case))
    return missing


def verify_extra(case, hashes):
    for name in LIFECYCLE:
        if name not in hashes:
            raise Blocked("missing sealed lifecycle artifact: " + name)
    if any(path.name not in hashes for path in case.glob("*.json") if path.name != "verdict.json"):
        raise Blocked("retained case contains an unsealed hook or receipt")
    if any(hashes.get(name) != sha for name, sha in catalog_artifacts(case).items()):
        raise Blocked("catalog private input is not sealed by its capture")
    for name in hashes:
        path = absolute(case / name)
        if not path.is_relative_to(case):
            raise Blocked("retained artifact escaped its case")
        if digest(absolute(case / name)) != hashes[name]:
            raise Blocked("retained lifecycle or hook artifact changed: " + name)


def picture_frame(terminal):
    from .bundle.pictures import frame
    return frame(terminal)


def picture_steps(state, journal, log):
    contract = state["picture_steps"]
    presses = [row for row in journal if row.get("key") == contract["key"]]
    if len(presses) != contract["presses"] or contract["orbit_degrees"] != 15 * len(presses):
        raise Blocked("3D specimen requires two 15-degree orbit presses")
    events = [json.loads(line) for line in log.read_text().splitlines()]
    previous = None
    for press in presses:
        from .bundle.pictures import verify
        before, after = press.get("before_picture", {}), press.get("after_picture", {})
        verify(log.parent, before)
        verify(log.parent, after)
        if (press.get("delivery") != "compositor" or not before.get("sha256") or not after.get("sha256")
                or before["sha256"] == after["sha256"] or before.get("mode") != after.get("mode")
                or after.get("end_ns", 0) <= press["ns"] or after.get("observed_ns", 0) <= press["ns"]
                or previous is not None and (before.get("sha256"), before.get("mode")) != (previous["sha256"], previous["mode"])):
            raise Blocked("3D orbit lacks distinct presented before/after pictures")
        incoming = b"".join(base64.b64decode(row["base64"]) for row in events
                            if row["direction"] == "terminal-input" and press["ns"] <= row["ns"] <= after["end_ns"])
        if not re.search(rb"\x1b(?:O|\[[0-9;:]*)C", incoming):
            raise Blocked("3D orbit lacks actual native Right input; TUI15 owns enhanced-key evidence")
        previous = after
    return {"status": "PASS", "contract": contract, "presses": presses}


def prerequisites(spec, supplied):
    """Missing candidate routes remain owner/API blockers, including private CUPS traffic."""
    needs = {need for state in spec["states"] for need in state["requires"]}
    contracts = {"cups-sink": ("Root/TUI09", "private capability-scoped CUPS sink with actual request/output receipts"),
                 "progress-fixture": ("Root/backend operations", "private copy barrier with actual unfinished operation and cancel/reap"),
                 "EXT-SURFACES": ("Root/G2/backend owners", "accepted close-match, contents, three Jobs and mixed open-refused routes/fixtures")}
    missing = [{"owner": owner, "contract": text} for key, (owner, text) in contracts.items()
               if key not in supplied]
    for need in sorted(needs - set(supplied) - {"graphics-negative", "model-source"} - set(contracts)):
        missing.append({"owner": "Root/candidate fixture owner", "contract": need})
    return missing


def validate_surface(directory, row, identity):
    """External surfaces need an accepted route and actual assertion-bound case evidence."""
    from .control_native import check_proof, native_case
    verdict = load(directory / "verdict.json")
    route = verdict.get("route") or {}
    if route.get("status") != "ACCEPTED" or not route.get("owner") or not route.get("api"):
        raise Blocked("EXT-SURFACES owner must supply the accepted route/API")
    definition = next(s for s in load(HERE / "surfaces.json")["states"] if s["id"] == row["id"])
    checks = load(directory / "surface-checks.json")
    if set(checks) != set(definition["checks"]) or row["sha256"].get("surface-checks.json") != digest(directory / "surface-checks.json"):
        raise Blocked("surface assertion inventory is missing or stale")
    for check, proof in checks.items():
        case, _ = native_case(proof, identity, row["terminal"])
        if load(absolute(Path(proof["capture"])) / "identity.json").get("candidate_inputs") != identity.get("candidate_inputs"):
            raise Blocked("surface capture candidate inputs changed or were not recorded")
        check_proof(check, row["id"], proof, case)
    if row["id"] == "tui-open-refused":
        proof = checks["zero-app-launches"]
        audit = load(absolute(Path(proof["capture"]) / proof["case"]) / "application-launches.json")
        if (proof["artifacts"].get("application-launches.json") != digest(
                absolute(Path(proof["capture"]) / proof["case"]) / "application-launches.json")
                or audit.get("launches") != [] or audit.get("selection") != "mixed-files-and-folder"
                or audit.get("observed") is not True):
            raise Blocked("mixed refused selection needs an observed zero-application-launch audit")


def validate_control_inputs(directory, identity):
    from .control_native import sealed
    if "candidate_inputs" not in identity:
        return
    for records in load(directory / "checks.json").values():
        for record in records:
            case = absolute(directory / record["capture"])
            if not case.is_relative_to(directory / "captures"):
                raise Blocked("control capture escaped its bundle")
            if sealed(case, "candidate-identity.json").get("candidate_inputs") != identity["candidate_inputs"]:
                raise Blocked("control capture candidate inputs changed or were not recorded")
            verify_extra(case, load(case / "verdict.json").get("artifact_sha256", {}))


def capture_calendar(args):
    """Record auxiliary inputs before a new accepted calendar run, never bind historical captures retroactively."""
    from types import SimpleNamespace
    from . import calendar_native
    from .control_native import same_identity
    root = output_root(args.output)
    extra = load(args.identity_inputs) if args.identity_inputs else None
    before = input_identity(REPO, extra, args.boards)
    save(root.child("capture-inputs.json"), {"candidate_inputs": before, "start_ns": time.time_ns()})
    native_args = SimpleNamespace(**vars(args))
    native_args.output, native_args.original_driver = root.child("raw"), None
    code = calendar_native.run(native_args)
    raw = load(root.child("raw/manifest.json"))
    identity = bind_identity(source_identity(REPO, absolute(args.binary), absolute(args.boards), Commands()), extra, args.boards)
    same_identity(raw["identity"], identity)
    if identity["candidate_inputs"] != before:
        raise Blocked("calendar auxiliary inputs changed during capture")
    save(root.child("capture-finish.json"), {"candidate_inputs": before, "end_ns": time.time_ns(), "exit_code": code})
    for row in raw["records"]:
        directory = calendar_native.record_directory(root.child("raw"), row)
        for path in directory.rglob("*"):
            absolute(path)
            if not path.is_file() and not path.is_dir():
                raise Blocked("special file in calendar capture")
        target = root.child(row["directory"])
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(directory, target)
    save(root.child("manifest.json"), dict(raw, identity=identity,
         raw_manifest_sha256=digest(root.child("raw/manifest.json")),
         capture_inputs_sha256=digest(root.child("capture-inputs.json")),
         capture_finish_sha256=digest(root.child("capture-finish.json"))))
    return code


def calendar_capsule(source, identity, manifest):
    """Verify this unit's before/after receipts without restamping the accepted producer's raw manifest."""
    from .control_native import same_identity
    receipts = {"capture-inputs.json": "capture_inputs_sha256", "capture-finish.json": "capture_finish_sha256",
                "raw/manifest.json": "raw_manifest_sha256"}
    for name, key in receipts.items():
        if manifest.get(key) != digest(absolute(source / name)):
            raise Blocked("calendar capsule receipt is missing or changed: " + name)
    before, after = load(source / "capture-inputs.json"), load(source / "capture-finish.json")
    if (before.get("candidate_inputs") != identity["candidate_inputs"]
            or after.get("candidate_inputs") != identity["candidate_inputs"]
            or type(before.get("start_ns")) is not int or type(after.get("end_ns")) is not int
            or not 0 < before["start_ns"] < after["end_ns"] or type(after.get("exit_code")) is not int
            or after["exit_code"] not in (0, 2)):
        raise Blocked("calendar capsule input identity or capture lifecycle is invalid")
    same_identity(manifest["identity"], identity)
    if manifest["identity"].get("candidate_inputs") != identity["candidate_inputs"]:
        raise Blocked("calendar capsule candidate inputs changed")
    raw = load(source / "raw/manifest.json")
    same_identity(raw["identity"], identity)
    # Visual review may update verdict hashes, but cannot replace original captured artifacts or rows.
    def inventory(rows):
        return [{key: row[key] for key in ("terminal", "kind", "id", "directory", "sha256")} for row in rows]
    if not raw.get("records") or inventory(raw["records"]) != inventory(manifest["records"]):
        raise Blocked("calendar capsule differs from the original captured inventory")
    return tuple(receipts)


def validate_calendar_inputs(source, identity, manifest):
    if "candidate_inputs" not in identity:
        return
    calendar = [row for row in manifest.get("records", []) if row["kind"] == "calendar"]
    held = []
    for capsule in manifest.get("calendar_capsules", []):
        directory = absolute(source / capsule["directory"])
        if not directory.is_relative_to(source / "provenance") or not capsule.get("sha256"):
            raise Blocked("calendar capsule escaped supplemental provenance")
        for name in ("manifest.json", "capture-inputs.json", "capture-finish.json", "raw/manifest.json"):
            if capsule["sha256"].get(name) != digest(absolute(directory / name)):
                raise Blocked("joined calendar capsule receipt is missing or changed: " + name)
        producer = load(directory / "manifest.json")
        calendar_capsule(directory, identity, producer)
        held.extend(producer["records"])
    if calendar != held:
        raise Blocked("calendar records lack their exact retained capture provenance")




def join_supplements(output, identity, sources):
    """Copy producer bundles without changing record names, verdicts or captured bytes."""
    from .control_native import same_identity
    root = output_root(output)
    records, seen, capsules = [], set(), []
    for source in sources:
        source = absolute(source)
        manifest = load(source / "manifest.json")
        same_identity(manifest["identity"], identity)
        if manifest["identity"].get("candidate_inputs") != identity["candidate_inputs"]:
            raise Blocked("supplement candidate inputs changed")
        if any(row["kind"] == "calendar" for row in manifest["records"]):
            names = ("manifest.json",) + calendar_capsule(source, identity, manifest)
            destination = root.directory("provenance/calendar-" + str(len(capsules)))
            for name in names:
                target = absolute(destination / name)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(absolute(source / name), target)
            capsules.append({"directory": str(destination.relative_to(root.path)),
                             "sha256": {name: digest(destination / name) for name in names}})
        for row in manifest["records"]:
            key = (row["terminal"], row["kind"], row["id"])
            if key in seen:
                raise Blocked("duplicate supplement record")
            relative = Path(row["directory"])
            if relative.parts != (row["terminal"], row["id"]):
                raise Blocked("supplement directory identifies another record")
            directory = absolute(source / relative)
            for path in directory.rglob("*"):
                absolute(path)
                if not (path.is_file() or path.is_dir()):
                    raise Blocked("special file in supplemental evidence")
            destination = root.child(relative)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(directory, destination)
            records.append(row)
            seen.add(key)
    joined = {"schema": 1, "identity": identity, "records": records, "calendar_capsules": capsules}
    validate_calendar_inputs(root.path, identity, joined)
    save(root.child("manifest.json"), joined)
    return root.path


def catalog_case(*, args, root, display, terminal, state, variant, colors, identity, profiles,
                 reviews, overrides, seams, language, keyboard_group, prepare):
    """I50-27 uses the caller's display lease and supplies its actual private locale/keyboard preparation."""
    from .driver import execute_pair
    if (not language or not keyboard_group or not callable(prepare)
            or not isinstance(identity.get("candidate_inputs"), dict) or not identity["candidate_inputs"]):
        raise Blocked("catalog case needs language, actual keyboard group and private fixture preparation")
    def setup(fx):
        receipt = prepare(fx)
        if (receipt.get("language") != language or receipt.get("keyboard_group") != keyboard_group
                or receipt.get("candidate_inputs") != identity.get("candidate_inputs") or not receipt.get("artifacts")):
            raise Blocked("catalog preparation identity mismatch")
        for name, sha in receipt["artifacts"].items():
            path = absolute(fx / name)
            if not path.is_relative_to(fx) or digest(path) != sha:
                raise Blocked("catalog private input identity mismatch")
        return receipt
    return execute_pair(args, root, display, terminal, state, variant, colors, identity,
                        profiles, reviews, overrides, seams, fixture_setup=setup)


def main(argv=None):
    from . import driver, receipts
    parser = driver.parser()
    parser.description = __doc__
    parser.add_argument("--calendar-bundle", type=Path)
    parser.add_argument("--controls-bundle", type=Path)
    parser.add_argument("--surface-bundle", type=Path)
    parser.add_argument("--capture-calendar", action="store_true", help="capture fresh calendar with input identity recorded before launch")
    args = parser.parse_args(argv)
    try:
        if args.preflight:
            if args.capture_calendar or args.reconcile or args.smoke or args.dry_run or args.states:
                raise Blocked("preflight cannot be combined with capture, reconciliation or diagnostic modes")
            return driver.main(["--boards", str(args.boards), "--preflight"])
        if args.capture_calendar and (args.dry_run or args.reconcile or args.smoke or args.states or args.list or args.supplement):
            raise Blocked("calendar capture cannot be combined with another action or diagnostic mode")
        spec = driver.contract(absolute(args.boards))
        required = inventory(spec)
        if args.list:
            print(json.dumps(dict(required, native_acceptance=False, prerequisites=prerequisites(spec, {})), indent=2))
            return 0
        if args.capture_calendar:
            if not args.binary or not args.output:
                raise Blocked("calendar capture requires binary and fresh marked output")
            return capture_calendar(args)
        if args.smoke or args.states or args.dry_run or driver.selection(args.terminals, TERMINALS) != list(TERMINALS):
            raise Blocked("candidate composition requires the complete native inventory")
        if not all((args.binary, args.output, args.fixtures, args.calendar_bundle, args.controls_bundle, args.surface_bundle)):
            raise Blocked("candidate composition needs binary, fixtures, fresh output and all three real supplement bundles")
        root = output_root(args.output)
        extra = driver.load(args.identity_inputs) if args.identity_inputs else None
        identity = bind_identity(source_identity(REPO, absolute(args.binary), absolute(args.boards), Commands()), extra, args.boards)
        if identity["candidate_inputs"]["missing_surface_references"]:
            raise Blocked("candidate requires all four supplemental PNG/HTML reference identities")
        save(root.child("identity.json"), identity)
        supplement = join_supplements(root.child("supplement"), identity,
                                      (args.calendar_bundle, args.controls_bundle, args.surface_bundle))
        additional = receipts.supplement(supplement, identity, TERMINALS, driver.load(HERE / "controls.json")["controls"],
                                         driver.load(HERE / "calendar.json"), driver.load(HERE / "surfaces.json")["states"])
        if not additional or any(row["status"] != "PASS" for row in additional):
            save(root.child("prerequisites.json"), additional)
            raise Blocked("supplement producer gates or controller reviews are incomplete")
        command = ["--binary", str(args.binary), "--boards", str(args.boards), "--fixtures", str(args.fixtures),
                   "--output", str(root.child("battery")), "--supplement", str(supplement), "--preset", args.preset,
                   "--timeout", str(args.timeout)]
        for flag in ("profiles", "steps", "seams", "reviews", "reconcile", "identity_inputs"):
            if value := getattr(args, flag):
                command += ["--" + flag.replace("_", "-"), str(value)]
        save(root.child("commands.json"), [{"module": "tui_battery.driver", "argv": command}])
        code = driver.main(command)
        manifest = load(root.child("battery/manifest.json"))
        manifest.update(completion(manifest, required))
        save(root.child("manifest.json"), manifest)
        return 0 if code == 0 and manifest["complete_reference_matrix"] else (1 if code == 1 else 2)
    except (Blocked, OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps({"status": "BLOCKED", "native_acceptance": False, "reason": str(error)}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
