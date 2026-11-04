"""Offline doubles exercise the supplement producer, never establish native proof."""
import base64
from copy import deepcopy
import json
from itertools import product
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from . import control_native as native, png
from .case_identity import ARTIFACTS, configuration
from .cells import Screen
from .compositor import TOOL
from .evidence import Blocked, MARKER, digest
from .receipts import supplement


class M9ControlTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", Path(__file__).resolve().parents[2] / ".superpowers/m9-tui-06"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="controls.", dir=parent))
        (self.root / MARKER).write_text("owned offline native-control doubles\n")
        binary = self.root / "candidate"
        binary.write_bytes(b"\x7fELF offline double, never executed")
        self.identity = dict(commit="candidate", dirty_patch_sha256="patch", elf_sha256=digest(binary), binary=str(binary),
                             references={"board": "sha"}, untracked={})
        self.serial = 0

    def write(self, path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value) + "\n")

    def fixture(self, preset="default", positive=True, keyboard="legacy", chord=None, presses=None, state_id="tui-list-text", flags=()):
        self.serial += 1
        root = self.root / f"source-{self.serial}"
        case = root / f"foot/{state_id}/primary"
        case.mkdir(parents=True)
        self.write(root / "identity.json", self.identity)
        self.write(root / "manifest.json", {"mode": "native"})
        state = {"id": state_id, "steps": [], "predicate": {"contains": ["READY"]}}
        variant = {"id": "primary", "columns": 12, "rows": 5, "origin": [4, 9]}
        config = configuration("foot", preset, variant, state)
        images = ("native.png", "grid-crop.png", "diff.png", "overlay.png")
        for name in ARTIFACTS:
            if name not in images:
                self.write(case / name, {})
        for name in images:
            png.write(case / name, 12, 5, bytes([28, 18, 13, 255]) * 60)
        (case / "terminal.conf").write_text("font=monospace\n")
        self.write(case / "profile.json", dict(config["profile"], terminal="foot", path=str(case / "terminal.conf"),
                   sha256=digest(case / "terminal.conf"), calibration={"client": {"address": "0x123"}}))
        self.write(case / "capture-config.json", config)
        self.write(case / "candidate-settings.json", {"keys": preset})
        self.write(case / "fx/state/flea/ui.json", {"keys": preset})
        self.write(case / "state.json", {"requested": state, "variant": variant, "preset": preset})
        screen = Screen(12, 5)
        screen.feed(b"\x1b[HREADY")
        self.write(case / "cells.json", screen.snapshot())
        self.write(case / "commands.json", [{"argv": ["bash", "-c", "helper", "hypr_window_send_shortcut", "0x123", "SHIFT", "Delete"],
                   "start_ns": 100, "end_ns": 115, "exit_code": 0}])
        self.write(case / "launch-spec.json", {"argv": ["foot", "--config", str(case / "terminal.conf"), "relay"]})
        self.write(case / "relay-spec.json", {"argv": ["bwrap", "--unshare-all", "--ro-bind", self.identity["binary"], "/opt/flea",
                                                       "/opt/flea", "--tui", *flags, "/fx/v/work"]})
        shutdown = {"candidate_exit": 0, "termios_restored": True, "relay_termios_restored": True, "screen": screen.snapshot()}
        self.write(case / "relay-exit.json", shutdown)
        self.write(case / "launch-exit.json", {"exit_code": 0})
        self.write(case / "launch-start.json", {"pid": 101, "argv": native.load(case / "launch-spec.json")["argv"]})
        self.write(case / "relay-start.json", {"pid": 102, "candidate_pid": 103, "stdin_tty": True, "stdout_tty": True})
        replies = b"\x1b_Gi=7;OK\x1b\\\x1b[?1;4c" if positive else b"\x1b_Gi=7;ENOENT\x1b\\\x1b[?1;2c"
        if keyboard == "extended":
            replies += b"\x1b[?31u"
        keys = {"ctrl-enter": b"\x1b[13;5u", "O": b"O", "ctrl-shift-z": b"\x1b[122;6u", "ctrl-y": b"\x19",
                "ctrl-shift-n": b"\x1b[110;6u", "N": b"N", "ctrl-tab": b"\x1b[9;5u", "ctrl-pagedown": b"\x1b[6;5~", "X": b"X"}
        emitted = keys.get(chord, b"\x1b[3;2~")
        queries = b"\x1b_Gi=7,a=q;AAAA\x1b\\\x1b[c\x1b[?u"
        rows = [(10, "output", queries), (30, "terminal-input", replies), (35, "candidate-input", replies)]
        for index, (_, _, data) in enumerate(presses or [(None, None, emitted)]):
            rows += [(120 + 1000 * index, "terminal-input", data[:len(data)//2]),
                     (130 + 1000 * index, "terminal-input", data[len(data)//2:]), (140 + 1000 * index, "candidate-input", data)]
        (case / "pty.log").write_text("\n".join(json.dumps(dict(ns=ns, direction=direction,
            base64=base64.b64encode(data).decode())) for ns, direction, data in rows) + "\n")
        self.write(case / "capabilities.json", native.capabilities(case / "pty.log"))
        self.write(case / "decoder.json", {"synthetic": True, "status": "PASS", "input_hex": "1b5b333b327e"})
        verdict = dict(terminal="foot", state=state_id, variant="primary", status="BLOCKED",
                       terminal_exit={"exit_code": 0}, shutdown=shutdown,
                       behavior={"status": "PASS"}, cells={"geometry": {"status": "PASS"}, "color": {"status": "PASS"}},
                       media={"status": "PASS"}, edges={"status": "PASS"}, elf_sha256=self.identity["elf_sha256"],
                       artifact_sha256={name: digest(case / name) for name in (*ARTIFACTS, "terminal.conf", "decoder.json")})
        self.write(case / "verdict.json", verdict)
        dispatch = {"source": "compositor", "command": 0, "mods": "SHIFT", "key": "Delete", "deadline_ns": 200}
        if chord:
            selected = ("X", "SHIFT", "x") if chord == "X" else next(c for pair in native.PAIRS.values() for c in pair if c[0] == chord)
            dispatch.update(mods=selected[1], key=selected[2])
            commands = native.load(case / "commands.json")
            commands[0]["argv"][-2:] = selected[1:]
            self.write(case / "commands.json", commands)
            verdict["artifact_sha256"]["commands.json"] = digest(case / "commands.json")
            self.write(case / "verdict.json", verdict)
        proof = dict(capture=str(root), case=f"foot/{state_id}/primary", configuration=config,
                    sha256={name: digest(case / name) for name in ("launch-exit.json", "relay-exit.json", "launch-start.json", "relay-start.json")},
                    observed=True, observation="Controller double observed the specified behavior in this exact capture.",
                    artifacts={"native.png": digest(case / "native.png"), "decoder.json": digest(case / "decoder.json")},
                    decoder_artifact="decoder.json", dispatch=dispatch, events=[3, 4], hex=emitted.hex())
        if chord:
            proof["chord"] = chord
        if presses:
            commands = [{"argv": ["bash", "-c", "helper", "hypr_window_send_shortcut", "0x123", mods, key],
                         "start_ns": 100 + 1000 * index, "end_ns": 115 + 1000 * index, "exit_code": 0}
                        for index, (mods, key, _) in enumerate(presses)]
            self.write(case / "commands.json", commands)
            verdict["artifact_sha256"]["commands.json"] = digest(case / "commands.json")
            self.write(case / "verdict.json", verdict)
            proof["sequence"] = [dict(dispatch={"source": "compositor", "command": index, "mods": mods, "key": key,
                                                "deadline_ns": 200 + 1000 * index},
                                      events=[3 + 3 * index, 4 + 3 * index], hex=data.hex())
                                 for index, (mods, key, data) in enumerate(presses)]
            proof.update(proof["sequence"][0])
        self.reseal(proof)
        return proof

    def reseal(self, proof):
        case = self.case(proof)
        self.write(case / "capabilities.json", native.capabilities(case / "pty.log"))
        verdict = native.load(case / "verdict.json")
        verdict["artifact_sha256"] = {name: digest(case / name) for name in verdict["artifact_sha256"]}
        self.write(case / "verdict.json", verdict)
        self.write(Path(proof["capture"]) / "manifest.json",
                   {"mode": "native", "preset": proof["configuration"]["preset"], "records": [verdict]})

    def log(self, proof, rows, commands=None):
        case = self.case(proof)
        (case / "pty.log").write_text("\n".join(json.dumps(dict(ns=ns, direction=direction, base64=base64.b64encode(data).decode()))
                                               for ns, direction, data in rows) + "\n")
        if commands is not None:
            self.write(case / "commands.json", commands)
        self.reseal(proof)

    def shortcut(self, mods, key, start):
        return {"argv": ["bash", "-c", "helper", "hypr_window_send_shortcut", "0x123", mods, key],
                "start_ns": start, "end_ns": start + 3, "exit_code": 0}

    def negotiating(self, fragmented=True, key_during=True, candidate_fragmented=False):
        """Native replies split across reads with a compositor X between the Kitty and DA1 replies."""
        proof = self.fixture()
        queries = b"\x1b_Gi=7,a=q;AAAA\x1b\\\x1b[c\x1b[?u"
        kitty, da1 = b"\x1b_Gi=7;OK\x1b\\", b"\x1b[?1;4c"
        key_ns = 40 if key_during else 90
        raw = [(20, kitty[:7]), (30, kitty[7:])] if fragmented else [(25, kitty)]
        forwarded = [(55, "candidate-input", kitty[:7]), (57, "candidate-input", kitty[7:])] if candidate_fragmented \
            else [(55, "candidate-input", kitty)]
        rows = [(10, "output", queries)] + [(ns, "terminal-input", data) for ns, data in raw]
        rows += [(key_ns, "terminal-input", b"X"), (50 if key_during else 60, "terminal-input", da1),
                 *forwarded, (key_ns + 21, "candidate-input", b"X"), (70, "candidate-input", da1)]
        rows.sort(key=lambda row: row[0])
        event = next(i for i, row in enumerate(rows) if row[2] == b"X" and row[1] == "terminal-input")
        proof.update(dispatch={"source": "compositor", "command": 0, "mods": "SHIFT", "key": "X", "deadline_ns": 215},
                     events=[event], hex="58")
        self.log(proof, rows, [self.shortcut("SHIFT", "X", 15)])
        return proof

    def anchor(self, proof, name, value):
        """Write a proof anchor; an already sealed file is resealed, a new one stays unsealed until seal()."""
        case = self.case(proof)
        self.write(case / name, value)
        proof["artifacts"][name] = digest(case / name)
        if name in native.load(case / "verdict.json")["artifact_sha256"]:
            self.seal(proof, name)

    def seal(self, proof, name):
        case = self.case(proof)
        verdict = native.load(case / "verdict.json")
        verdict["artifact_sha256"][name] = digest(case / name)
        self.write(case / "verdict.json", verdict)
        self.reseal(proof)

    def sealed_anchor(self, proof, name, value):
        self.anchor(proof, name, value)
        self.seal(proof, name)

    def unseal(self, proof, name):
        """Drop one digest from the capture verdict and reseal the rest."""
        case = self.case(proof)
        verdict = native.load(case / "verdict.json")
        verdict["artifact_sha256"].pop(name)
        self.write(case / "verdict.json", verdict)
        self.reseal(proof)

    def signal_proof(self):
        """A shutdown-signal capture with its signal, census and relay start sealed; the positive check passes."""
        proof = self.fixture()
        commands = native.load(self.case(proof) / "commands.json") + [
            {"argv": ["kill", "-TERM", "103"], "start_ns": 160, "end_ns": 161, "exit_code": 0}]
        self.log(proof, self.rows_of(proof), commands)
        self.sealed_anchor(proof, "signal.json", dict(signal="TERM", pid=103, command=1))
        self.sealed_anchor(proof, "process-census.json", dict(remaining=[], checked=["candidate", "relay", "mpv", "decoder", "tmux"],
                           before_ns=150, after_close_ns=300, processes_before=[dict(pid=103, kind="candidate")]))
        self.seal(proof, "relay-start.json")
        native.check_proof("termios", "shutdown-signal", proof, self.case(proof))
        return proof

    def path_proof(self, path, preset="default", keyboard=None, state_id=None, flags=None, data=None):
        entry = native.KEY_PATHS[path]
        mode = keyboard or ("extended" if entry["mode"] == "extended" else "legacy")
        hexes = entry["hex"] or tuple(f"\x1b[{code};{mods}u".encode().hex() for code, mods in entry["csi"] or ())
        presses = [(mods, key, bytes.fromhex(data or hexes[i])) for i, (mods, key) in enumerate(entry["chords"])] or None
        state = state_id or sorted(entry["states"] or native.CONTEXT_STATES.get(entry["context"]) or {"tui-list-text"})[0]
        proof = self.fixture(preset, keyboard=mode, presses=presses, state_id=state,
                             flags=flags if flags is not None else (*(entry["flag"] or ()),
                                                                   *(["/fx/result"] if entry["flag"] in (("--cwd-file",), ("--chooser-file",)) else [])))
        proof["context"] = entry["context"]
        return proof

    def key_path_assertions(self, control):
        return {"key-path:" + path: [self.path_proof(path, preset) for preset in native.PRESETS]
                for path, entry in native.KEY_PATHS.items() if entry["control"] == control}

    def case(self, proof):
        return Path(proof["capture"]) / proof["case"]

    def produce(self, control, assertions, reviews=None):
        self.serial += 1
        document = dict(identity=self.identity, records=[dict(terminal="foot", id=control["id"], assertions=assertions)])
        output = self.root / f"output-{self.serial}"
        with patch.object(native, "controls", return_value=[control]):
            result = native.produce(document, self.identity, output, ["foot"], reviews)
        return result, output

    def test_m9_controls_cover_every_contract_assertion(self):
        contract = native.controls()
        planned = native.plan()
        self.assertEqual(len(planned["records"]), len(contract) * 5)
        for control in contract:
            row = next(r for r in planned["records"] if r["id"] == control["id"])
            self.assertTrue(set(control["checks"]) <= set(row["checks"]))
            for omitted in row["checks"]:
                with self.subTest(control=control["id"], omitted=omitted):
                    checks = {key: [] for key in row["checks"] if key != omitted}
                    result, _ = self.produce(control, checks)
                    self.assertEqual(result["status"], "BLOCKED")
                    self.assertIn("missing or unexpected control assertions", result["records"][0]["reason"])
        self.assertFalse(planned["yazi_opt_in"])
        self.assertTrue(native.plan(include_yazi=True)["yazi_opt_in"])
        self.assertEqual(planned["tmux_matrix"], {"outer": ["foot", "kitty"], "passthrough": [False, True],
                                               "color": ["RGB", "256"], "extended_keys": [False, True]})
        print("PASS test_m9_controls_cover_every_contract_assertion")

    def test_m9_controls_require_every_key_path(self):
        records = native.key_path_records()
        self.assertEqual(len(records), 180)
        self.assertEqual(native.records_sha256(records), native.KEY_PATH_AUDIT["sha256"])
        self.assertEqual(set(native.KEY_PATHS), set(native.KEY_PATH_AUDIT["ids"]))
        native.key_path_coverage()
        planned = {(r["terminal"], r["id"]): r["checks"] for r in native.plan()["records"]}
        for path, entry in native.KEY_PATHS.items():
            for terminal in native.TERMINALS:
                self.assertIn("key-path:" + path, planned[(terminal, entry["control"])])
        for path, contract in (("open-tab-enhanced", "ctrl-enter/O"), ("open-tab-plain", "ctrl-enter/O"),
                               ("redo-enhanced", "ctrl-shift-z/ctrl-y"), ("redo-plain", "ctrl-shift-z/ctrl-y"),
                               ("mkdir-enhanced", "ctrl-shift-n/N"), ("mkdir-plain", "ctrl-shift-n/N"),
                               ("focus-enhanced-listing", "ctrl-tab-keeps-ctrl-pagedown"),
                               ("tab-next-legacy-listing", "ctrl-tab-keeps-ctrl-pagedown"),
                               ("shift-delete-legacy", "legacy-CSI-3;2~"), ("X-reserved", "no-X-delete"),
                               ("E-editor", "E-editor"), ("cwd-file", "cwd-file-shell-wrapper"), ("yazi-opt-in", "yazi-opt-in")):
            self.assertEqual(native.KEY_PATHS[path]["contract"], contract)
        self.assertEqual(native.KEY_PATHS["shift-delete-legacy"]["hex"], ("1b5b333b327e",))
        self.assertEqual(native.KEY_PATHS["X-reserved"]["control"], "shift-delete-legacy")
        self.assertNotIn("yazi", native.PRESETS)
        for path in ("open-tab-plain", "cwd-file"):
            entries = dict(native.KEY_PATHS)
            entries.pop(path)
            with self.subTest(dropped=path), patch.object(native, "KEY_PATHS", entries), \
                    self.assertRaisesRegex(Blocked, "required key path has no control assertion: " + path):
                native.key_path_coverage()
        entries = dict(native.KEY_PATHS, **{"print": dict(native.KEY_PATHS["print"], control="unknown")})
        with patch.object(native, "KEY_PATHS", entries), self.assertRaisesRegex(Blocked, "no control assertion: print"):
            native.key_path_coverage()
        print("PASS test_m9_controls_require_every_key_path")

    def test_m9_controls_key_paths_need_native_emission_mode_context_and_presets(self):
        for path in ("redo-plain", "redo-enhanced", "shift-delete-legacy", "X-reserved", "focus-plain-pdf",
                     "tab-next-legacy-media", "yazi-tab-new-sequence", "cwd-file", "yazi-opt-in", "preview-3d"):
            proof = self.path_proof(path)
            with self.subTest(accepted=path):
                native.native_case(proof, self.identity, "foot")
                native.check_proof("key-path:" + path, native.KEY_PATHS[path]["control"], proof, self.case(proof))
        rejected = (("redo-plain", dict(keyboard="extended"), "keyboard mode"),
                    ("redo-enhanced", dict(keyboard="legacy"), "keyboard mode"),
                    ("open-tab-plain", dict(state_id="tui-quicklook-text"), "context"),
                    ("shift-delete-legacy", dict(data="1b5b333b3275"), "bytes do not match"),
                    ("cwd-file", dict(flags=()), "launch flag"))
        for path, change, reason in rejected:
            proof = self.path_proof(path, **change)
            with self.subTest(rejected=path, reason=reason), self.assertRaisesRegex(Blocked, reason):
                native.check_proof("key-path:" + path, native.KEY_PATHS[path]["control"], proof, self.case(proof))
        proof = self.path_proof("tab-close")
        proof["context"] = "media"
        with self.assertRaisesRegex(Blocked, "context"):
            native.check_proof("key-path:tab-close", "keys-presets", proof, self.case(proof))
        proof = self.path_proof("yazi-tab-new-sequence")
        proof["sequence"] = proof["sequence"][:1]
        with self.assertRaisesRegex(Blocked, "sequence"):
            native.check_proof("key-path:yazi-tab-new-sequence", "keys-presets", proof, self.case(proof))
        proof = self.path_proof("tab-number")
        proof["sequence"][0]["dispatch"]["source"] = "decoder"
        with self.assertRaisesRegex(Blocked, "compositor"):
            native.check_proof("key-path:tab-number", "keys-presets", proof, self.case(proof))
        with self.assertRaisesRegex(Blocked, "real candidate launch argv"):
            native.native_case(self.path_proof("cwd-file", flags=("--select", "/fx/a")), self.identity, "foot")
        proofs = [self.path_proof("redo-plain", preset) for preset in native.PRESETS]
        native.complete_variants("keys-presets", "key-path:redo-plain", proofs)
        with self.assertRaisesRegex(Blocked, "every preset"):
            native.complete_variants("keys-presets", "key-path:redo-plain", proofs[1:])
        with self.assertRaisesRegex(Blocked, "every preset"):
            native.complete_variants("keys-presets", "key-path:redo-plain", proofs, include_yazi=True)
        native.complete_variants("keys-presets", "key-path:redo-plain", proofs + [self.path_proof("redo-plain", "yazi")], include_yazi=True)

    def test_m9_controls_r1_zero_flag_reply_then_push_is_extended(self):
        proof = self.path_proof("redo-enhanced")
        case = self.case(proof)
        rows = native.load_lines(case / "pty.log")
        for row in rows:
            row["base64"] = base64.b64encode(base64.b64decode(row["base64"]).replace(b"\x1b[?31u", b"\x1b[?0u")).decode()
        self.log(proof, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in rows])
        self.assertEqual(native.keyboard_mode(case), "legacy", "a zero-flag reply without a push stays legacy")
        rows.insert(3, dict(ns=40, direction="output", base64=base64.b64encode(b"\x1b[>29u").decode()))
        proof["sequence"][0]["events"] = proof["events"] = [4, 5]
        self.log(proof, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in rows])
        native.native_case(proof, self.identity, "foot")
        self.assertEqual(native.keyboard_mode(case), "extended")
        native.check_proof("key-path:redo-enhanced", "keys-presets", proof, case)

    def test_m9_controls_r1_negotiation_checks_need_their_transport_events(self):
        plain = self.fixture()
        native.native_case(plain, self.identity, "foot")
        for check in ("fragmented", "interleaved", "ordinary-input-retained"):
            with self.subTest(unexercised=check), self.assertRaises(Blocked):
                native.check_proof(check, "query-fragmented", plain, self.case(plain))
        control = next(c for c in native.controls() if c["id"] == "query-fragmented")
        result, output = self.produce(control, {check: [plain] for check in control["checks"]})
        self.assertNotEqual(native.load(output / result["records"][0]["directory"] / "verdict.json")["behavior"], "PASS")
        exercised = self.negotiating(candidate_fragmented=True)
        native.native_case(exercised, self.identity, "foot")
        for check in ("fragmented", "interleaved", "ordinary-input-retained"):
            native.check_proof(check, "query-fragmented", exercised, self.case(exercised))
        whole = self.negotiating(fragmented=False)
        with self.assertRaisesRegex(Blocked, "fragmented"):
            native.check_proof("fragmented", "query-fragmented", whole, self.case(whole))
        late = self.negotiating(key_during=False)
        for check in ("interleaved", "ordinary-input-retained"):
            with self.subTest(late=check), self.assertRaisesRegex(Blocked, "negotiation"):
                native.check_proof(check, "query-fragmented", late, self.case(late))

    def test_m9_controls_r1_deadline_and_stale_ids_need_their_own_evidence(self):
        proof = self.fixture(positive=False)
        case = self.case(proof)
        for check, control in (("bounded-deadline", "query-timeout"), ("stale-before-query", "query-stale-id"),
                               ("wrong-id", "query-stale-id")):
            with self.subTest(missing=check), self.assertRaises(Blocked):
                native.check_proof(check, control, proof, case)
        rows = native.load_lines(case / "pty.log") + [dict(ns=150, direction="output", base64=base64.b64encode(b"\x1b[H").decode())]
        self.log(proof, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in rows])
        self.sealed_anchor(proof, "deadline.json", {"query_ns": 10, "deadline_ns": 500, "disabled_ns": 150})
        self.sealed_anchor(proof, "disabled-state.json", dict(hook="tui16-disabled-state", state="disabled", ns=150))
        native.check_proof("bounded-deadline", "query-timeout", proof, case)
        self.anchor(proof, "disabled-state.json", dict(hook="tui16-disabled-state", state="disabled", ns=151))
        with self.assertRaisesRegex(Blocked, "disabled_ns"):
            native.check_proof("bounded-deadline", "query-timeout", proof, case)
        self.anchor(proof, "disabled-state.json", dict(hook="tui16-disabled-state", state="disabled", ns=150))
        self.anchor(proof, "deadline.json", {"query_ns": 10, "deadline_ns": 100, "disabled_ns": 150})
        with self.assertRaisesRegex(Blocked, "deadline"):
            native.check_proof("bounded-deadline", "query-timeout", proof, case)
        current = self.fixture()
        stale = b"\x1b_Gi=7;OK\x1b\\".hex()
        for check, reply, extra in (("stale-before-query", stale, {"before_query": True}),
                                    ("wrong-id", b"\x1b_Gi=9;OK\x1b\\".hex(), {})):
            self.sealed_anchor(current, check + ".json", dict(synthetic=True, status="PASS", scenario=check, input_hex=reply, accepted=False, **extra))
            current["decoder_artifact"] = check + ".json"
            native.check_proof(check, "query-stale-id", current, self.case(current))
        self.anchor(current, "wrong-id.json", dict(synthetic=True, status="PASS", scenario="wrong-id", input_hex=stale, accepted=False))
        with self.assertRaisesRegex(Blocked, "wrong-id"):
            native.check_proof("wrong-id", "query-stale-id", current, self.case(current))

    def test_m9_controls_r1_capability_replies_are_forwarded_verbatim(self):
        negative = self.fixture()
        case = self.case(negative)
        rows = native.load_lines(case / "pty.log")
        fake = b"\x1b_Gi=7;ENOENT\x1b\\"
        rows[2]["base64"] = base64.b64encode(fake).decode()
        rows.insert(2, dict(ns=32, direction="setup-input", base64=base64.b64encode(fake).decode()))
        negative["events"] = [4, 5]
        self.log(negative, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in rows])
        native.native_case(negative, self.identity, "foot")
        with self.assertRaisesRegex(Blocked, "verbatim"):
            native.check_proof("native-negative", "query-native", negative, case)
        positive = self.fixture(positive=False)
        case = self.case(positive)
        rows = native.load_lines(case / "pty.log")
        for row, content in zip(rows, (b"\x1b_Gi=7,a=q;AAAA\x1b\\\x1b_Gi=8,a=q;AAAA\x1b\\\x1b[c\x1b[?u",
                                       b"\x1b_Gi=7;OK\x1b\\\x1b[?1;2c", b"\x1b_Gi=8;OK\x1b\\\x1b[?1;2c")):
            row["base64"] = base64.b64encode(content).decode()
        self.log(positive, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in rows])
        native.native_case(positive, self.identity, "foot")
        with self.assertRaisesRegex(Blocked, "verbatim"):
            native.check_proof("native-positive", "query-native", positive, case)

    def test_m9_controls_r1_cyrillic_needs_layout_switch_and_base_key(self):
        legacy = self.fixture()
        native.native_case(legacy, self.identity, "foot")
        with self.assertRaisesRegex(Blocked, "Cyrillic"):
            native.check_proof("Cyrillic-base-layout", "keys-emission", legacy, self.case(legacy))
        proof = self.fixture()
        letter = b"\x1b[1092::97u"
        rows = [(10, "output", b"\x1b_Gi=7,a=q;AAAA\x1b\\\x1b[c\x1b[?u"), (20, "terminal-input", b"\x1b[?0u"),
                (25, "candidate-input", b"\x1b[?0u"), (30, "output", b"\x1b[>29u"),
                (120, "terminal-input", letter[:5]), (130, "terminal-input", letter[5:]), (140, "candidate-input", letter)]
        proof.update(dispatch={"source": "compositor", "command": 3, "mods": "", "key": "a", "deadline_ns": 200},
                     events=[4, 5], hex=letter.hex())

        def readback(start, end, keymap):
            return {"argv": [TOOL, "-j", "devices"], "start_ns": start, "end_ns": end, "exit_code": 0,
                    "stdout": json.dumps({"keyboards": [{"name": "kb", "active_keymap": keymap}]})}

        switch = {"argv": [TOOL, "switchxkblayout", "current", "1"], "start_ns": 50, "end_ns": 60, "exit_code": 0, "stdout": "ok\n"}
        before, after = readback(20, 25, "English (US)"), readback(70, 75, "Russian")
        key = self.shortcut("", "a", 100)
        self.log(proof, rows, [before, switch, after, key])
        native.native_case(proof, self.identity, "foot")
        native.check_proof("Cyrillic-base-layout", "keys-emission", proof, self.case(proof))
        variants = (
            ("unchanged keymap", [before, switch, readback(70, 75, "English (US)"), key]),
            ("readback after the key", [before, switch, readback(120, 125, "Russian"), key]),
            ("no readback before the switch", [dict(before, exit_code=1), switch, after, key]),
            ("switch exit 1", [before, dict(switch, exit_code=1), after, key]),
            ("switch stdout not ok", [before, dict(switch, stdout="error\n"), after, key]),
            ("true instead of the compositor", [before, dict(switch, argv=["/usr/bin/true", "switchxkblayout", "current", "1"]), after, key]),
        )
        for name, commands in variants:
            with self.subTest(variant=name):
                self.log(proof, rows, commands)
                with self.assertRaisesRegex(Blocked, "switchxkblayout|readbacks"):
                    native.check_proof("Cyrillic-base-layout", "keys-emission", proof, self.case(proof))

    def test_m9_controls_r1_signal_shutdown_needs_a_dispatched_signal(self):
        proof = self.fixture()
        case = self.case(proof)
        census = {"remaining": [], "checked": ["candidate", "relay", "mpv", "decoder", "tmux"], "before_ns": 150, "after_close_ns": 300,
                  "processes_before": [{"pid": 103, "kind": "candidate"}]}
        self.sealed_anchor(proof, "process-census.json", census)
        self.seal(proof, "relay-start.json")
        control = next(c for c in native.controls() if c["id"] == "shutdown-signal")
        result, output = self.produce(control, {check: [proof] for check in control["checks"]})
        self.assertNotEqual(native.load(output / result["records"][0]["directory"] / "verdict.json")["behavior"], "PASS")
        with self.assertRaisesRegex(Blocked, "signal"):
            native.check_proof("termios", "shutdown-signal", proof, case)
        commands = native.load(case / "commands.json") + [{"argv": ["kill", "-TERM", "103"], "start_ns": 160, "end_ns": 161, "exit_code": 0}]
        self.log(proof, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in native.load_lines(case / "pty.log")], commands)
        self.sealed_anchor(proof, "signal.json", {"signal": "TERM", "pid": 103, "command": 1})
        native.check_proof("termios", "shutdown-signal", proof, case)
        self.anchor(proof, "signal.json", {"signal": "TERM", "pid": 102, "command": 1})
        with self.assertRaisesRegex(Blocked, "signal"):
            native.check_proof("termios", "shutdown-signal", proof, case)
        self.anchor(proof, "signal.json", {"signal": "TERM", "pid": 103, "command": 1})
        self.log(proof, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in native.load_lines(case / "pty.log")],
                 commands[:-1] + [dict(commands[-1], argv=["true", "-TERM", "103"])])
        with self.assertRaisesRegex(Blocked, "kill"):
            native.check_proof("termios", "shutdown-signal", proof, case)
        self.log(proof, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in native.load_lines(case / "pty.log")], commands)
        self.anchor(proof, "process-census.json", dict(census, before_ns=165))
        with self.assertRaisesRegex(Blocked, "census"):
            native.check_proof("termios", "shutdown-signal", proof, case)

    def test_m9_controls_r1_source_record_and_per_case_elf_bind_the_case(self):
        proof = self.fixture()
        case = self.case(proof)
        native.native_case(proof, self.identity, "foot")
        recorded = native.load(case / "verdict.json")
        changed = deepcopy(recorded)
        changed["behavior"]["status"] = "FAIL"
        self.write(Path(proof["capture"]) / "manifest.json", {"mode": "native", "preset": "default", "records": [changed]})
        with self.assertRaisesRegex(Blocked, "source manifest"):
            native.native_case(proof, self.identity, "foot")
        self.write(Path(proof["capture"]) / "manifest.json", {"mode": "native", "preset": "vim", "records": [recorded]})
        with self.assertRaisesRegex(Blocked, "source manifest"):
            native.native_case(proof, self.identity, "foot")
        stale = dict(recorded, elf_sha256="old-candidate-elf")
        self.write(case / "verdict.json", stale)
        self.write(Path(proof["capture"]) / "manifest.json", {"mode": "native", "preset": "default", "records": [stale]})
        with self.assertRaisesRegex(Blocked, "ELF"):
            native.native_case(proof, self.identity, "foot")

    def test_m9_controls_r1_review_binds_terminal_and_control(self):
        control = next(c for c in native.controls() if c["id"] == "query-timeout")
        proof = self.fixture(positive=False)
        assertions = {check: [proof] for check in control["checks"]}
        self.sealed_anchor(proof, "deadline.json", {"query_ns": 10, "deadline_ns": 500, "disabled_ns": 10})
        rows = native.load_lines(self.case(proof) / "pty.log") + [dict(ns=150, direction="output", base64=base64.b64encode(b"\x1b[H").decode())]
        self.log(proof, [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in rows])
        self.anchor(proof, "deadline.json", {"query_ns": 10, "deadline_ns": 500, "disabled_ns": 150})
        self.sealed_anchor(proof, "disabled-state.json", dict(hook="tui16-disabled-state", state="disabled", ns=150))
        pending, output = self.produce(control, assertions)
        verdict = native.load(output / pending["records"][0]["directory"] / "verdict.json")
        self.assertEqual(verdict["behavior"], "PASS", pending)
        decision = {"foot/query-timeout": dict(binding_sha256=verdict["binding_sha256"], **{g: "PASS" for g in native.GATES})}
        result, output = self.produce(control, assertions, decision)
        self.assertEqual(result["status"], "PASS")
        native.validate_record(output, result["records"][0], self.identity)
        for terminal in native.TERMINALS:
            if terminal != "foot":
                with self.subTest(relabelled=terminal), self.assertRaises(Blocked):
                    native.validate_record(output, dict(result["records"][0], terminal=terminal), self.identity)
        self.assertNotEqual(native.binding(self.identity, "foot", "query-timeout", {}),
                            native.binding(self.identity, "kitty", "query-timeout", {}))

    def test_m9_controls_reject_synthetic_key_emission(self):
        proof = self.fixture()
        case = self.case(proof)
        native.key_emission(case, proof, "1b5b333b327e")
        rows = native.load_lines(case / "pty.log")
        for direction in ("setup-input", "output", "candidate-input"):
            with self.subTest(direction=direction):
                changed = deepcopy(rows)
                changed[3]["direction"] = direction
                (case / "pty.log").write_text("\n".join(json.dumps(r) for r in changed))
                with self.assertRaisesRegex(Blocked, "synthetic or unrelated"):
                    native.key_emission(case, proof, "1b5b333b327e")
        (case / "pty.log").write_text("\n".join(json.dumps(r) for r in rows))
        proof["dispatch"]["source"] = "decoder"
        with self.assertRaisesRegex(Blocked, "compositor"):
            native.key_emission(case, proof)
        proof["dispatch"]["source"] = "compositor"
        self.write(Path(proof["capture"]) / "manifest.json", {"mode": "dry-run"})
        with self.assertRaisesRegex(Blocked, "synthetic"):
            native.native_case(proof, self.identity, "foot")
        print("PASS test_m9_controls_reject_synthetic_key_emission")

    def test_m9_controls_reject_unrestored_termios(self):
        proof = self.fixture()
        case = self.case(proof)
        native.native_case(proof, self.identity, "foot")
        original = native.load(case / "verdict.json")
        for field, value in (("termios_restored", False), ("relay_termios_restored", False), ("candidate_exit", 1), ("candidate_exit", -15)):
            changed = deepcopy(original)
            changed["shutdown"][field] = value
            self.write(case / "verdict.json", changed)
            with self.subTest(field=field, value=value), self.assertRaisesRegex(Blocked, "TTY restoration"):
                native.native_case(proof, self.identity, "foot")
        self.write(case / "verdict.json", original)
        changed = native.load(case / "relay-exit.json")
        changed["termios_restored"] = False
        self.write(case / "relay-exit.json", changed)
        with self.assertRaisesRegex(Blocked, "raw shutdown"):
            native.native_case(proof, self.identity, "foot")
        print("PASS test_m9_controls_reject_unrestored_termios")

    def test_m9_controls_startup_requires_the_exact_deliberate_failure(self):
        terminal = {"exit_code": 0}
        shutdown = {"candidate_exit": 2, "termios_restored": True, "relay_termios_restored": True}
        config = {"state": {"control": {"id": "shutdown-startup", "expected_candidate_exit": 2}}}
        self.assertIsNone(native.shutdown_error(terminal, shutdown, "shutdown-startup", config))
        for actual in (0, 1, -15):
            with self.subTest(actual=actual):
                self.assertIsNotNone(native.shutdown_error(terminal, dict(shutdown, candidate_exit=actual), "shutdown-startup", config))
        self.assertIsNotNone(native.shutdown_error(terminal, dict(shutdown, termios_restored=False), "shutdown-startup", config))
        self.assertIsNotNone(native.shutdown_error(terminal, shutdown, "shutdown-normal", config))

    def test_m9_controls_child_census_cannot_be_replaced_by_an_assertion(self):
        proof = self.fixture()
        case = self.case(proof)
        with self.assertRaisesRegex(Blocked, "child census"):
            native.check_proof("children-reaped", "shutdown-normal", proof, case)
        census = {"remaining": [], "checked": ["candidate", "relay", "mpv", "decoder", "tmux"],
                  "after_close_ns": 300, "processes_before": [{"pid": 102, "kind": "candidate"}]}
        self.sealed_anchor(proof, "process-census.json", census)
        native.check_proof("children-reaped", "shutdown-normal", proof, case)
        census["remaining"] = [{"pid": 103, "kind": "decoder"}]
        self.anchor(proof, "process-census.json", census)
        with self.assertRaisesRegex(Blocked, "leaked children"):
            native.check_proof("children-reaped", "shutdown-normal", proof, case)

    def test_m9_controls_bind_profile_and_candidate_identity(self):
        proof = self.fixture()
        native.native_case(proof, self.identity, "foot")
        for key in native.IDENTITY:
            changed = dict(self.identity, **{key: "stale"})
            with self.subTest(identity=key), self.assertRaisesRegex(Blocked, "identity mismatch"):
                native.native_case(proof, changed, "foot")
        for key, value in (("preset", "vim"), ("profile", {"font": "Another font", "font_size": 18})):
            changed = deepcopy(proof)
            changed["configuration"][key] = value
            with self.subTest(configuration=key), self.assertRaisesRegex(Blocked, "configuration differs"):
                native.native_case(changed, self.identity, "foot")
        changed = deepcopy(proof)
        changed["case"] = "../escape/primary"
        with self.assertRaisesRegex(Blocked, "terminal/state/variant"):
            native.native_case(changed, self.identity, "foot")
        print("PASS test_m9_controls_bind_profile_and_candidate_identity")

    def test_m9_controls_missing_artifacts_are_not_passing_skips(self):
        for name in (*ARTIFACTS, "terminal.conf", "relay-exit.json", "launch-exit.json"):
            proof = self.fixture()
            case = self.case(proof)
            removed = case / name
            removed.rename(case / "held-artifact")
            with self.subTest(name=name), self.assertRaises((Blocked, OSError, KeyError)):
                native.native_case(proof, self.identity, "foot")

    def test_m9_controls_pending_visual_review_cannot_pass(self):
        control = next(r for r in native.controls() if r["id"] == "shift-delete-legacy")
        proof = self.fixture()
        assertions = {key: [proof] for key in control["checks"]}
        assertions["no-X-delete"] = [self.fixture(chord="X")]
        assertions.update(self.key_path_assertions("shift-delete-legacy"))
        result, output = self.produce(control, assertions)
        row = result["records"][0]
        self.assertIn("controller geometry", row["reason"])
        verdict = native.load(output / row["directory"] / "verdict.json")
        self.assertEqual(verdict["behavior"], "PASS")
        self.assertEqual(verdict["native_visual"], "PENDING")
        decisions = {"foot/shift-delete-legacy": dict(binding_sha256=verdict["binding_sha256"], **{g: "PASS" for g in native.GATES})}
        result, approved = self.produce(control, assertions, decisions)
        self.assertEqual(result["status"], "PASS")
        accepted = supplement(approved, self.identity, ["foot"], [control], {"states": []})
        self.assertEqual(accepted[0]["status"], "PASS")
        native.validate_record(approved, result["records"][0], self.identity)
        nested = approved / result["records"][0]["directory"] / "captures/0/relay-exit.json"
        self.write(nested, {"termios_restored": False})
        with self.assertRaisesRegex(Blocked, "stale native control artifact"):
            native.validate_record(approved, result["records"][0], self.identity)
        decisions["foot/shift-delete-legacy"]["binding_sha256"] = "stale"
        result, _ = self.produce(control, assertions, decisions)
        self.assertEqual(result["status"], "BLOCKED")

    def test_m9_controls_query_replies_come_from_terminal_not_decoder(self):
        positive, negative = self.fixture(), self.fixture(positive=False)
        for check, proof in (("native-positive", positive), ("native-negative", negative), ("timeout-disables-feature", negative)):
            native.check_proof(check, "query-native", proof, self.case(proof))
        with self.assertRaisesRegex(Blocked, "positive capability"):
            native.check_proof("timeout-disables-feature", "query-timeout", positive, self.case(positive))
        case = self.case(positive)
        rows = native.load_lines(case / "pty.log")
        rows[1]["direction"] = rows[2]["direction"] = "setup-input"
        (case / "pty.log").write_text("\n".join(json.dumps(r) for r in rows))
        with self.assertRaisesRegex(Blocked, "no matching native positive"):
            native.check_proof("native-positive", "query-native", positive, case)

    def test_m9_controls_require_every_presets_pair(self):
        control = next(r for r in native.controls() if r["id"] == "keys-presets")
        proofs = {(p, mode): self.fixture(p, keyboard=mode) for p in native.PRESETS for mode in ("legacy", "extended")}
        assertions = {check: [proofs[(check, mode)] for mode in ("legacy", "extended")] if check in native.PRESETS
                      else list(proofs.values()) for check in control["checks"]}
        for check, (first, second) in native.PAIRS.items():
            assertions[check] = [self.fixture(p, keyboard=mode, chord=chord) for p in native.PRESETS
                                 for mode, chord in (("extended", first[0]), ("extended", second[0]), ("legacy", second[0]))]
        assertions.update(self.key_path_assertions("keys-presets"))
        complete, _ = self.produce(control, assertions)
        self.assertIn("controller geometry", complete["records"][0]["reason"])
        assertions["ctrl-enter/O"] = assertions["ctrl-enter/O"][:3]
        missing, _ = self.produce(control, assertions)
        self.assertIn("every shipped preset", missing["records"][0]["reason"])

    def test_m9_controls_tmux_requires_all_real_option_readbacks(self):
        proofs = []
        for outer, passthrough, color, extended in product(("foot", "kitty"), (False, True), ("RGB", "256"), (False, True)):
            proof = self.fixture()
            case = self.case(proof)
            (case / "tmux.conf").write_text("private tmux double\n")
            profile = native.load(case / "profile.json")
            profile.update(terminal="tmux", tmux_config_sha256=digest(case / "tmux.conf"))
            self.write(case / "profile.json", profile)
            self.write(case / "launch-spec.json", {"argv": [outer, "tmux", "-S", "/private/tmux.sock"]})
            commands = [{"argv": ["tmux", "-S", "/private/tmux.sock", "show-options", "-gv", option], "exit_code": 0, "stdout": value}
                        for option, value in (("allow-passthrough", "on" if passthrough else "off"),
                                              ("extended-keys", "on" if extended else "off"),
                                              ("terminal-features", "foot:RGB" if color == "RGB" else "foot"))]
            self.write(case / "commands.json", commands)
            self.assertEqual(native.tmux_context(case), (outer, passthrough, color, extended))
            proofs.append(proof)
        native.complete_variants("tmux-passthrough", "RGB", proofs)
        for omitted in range(16):
            with self.subTest(omitted=omitted), self.assertRaisesRegex(Blocked, "all 16 observed"):
                native.complete_variants("tmux-passthrough", "RGB", proofs[:omitted] + proofs[omitted+1:])
        case = self.case(proofs[0])
        commands = native.load(case / "commands.json")
        commands[0]["argv"][2] = "/wrong/server.sock"
        self.write(case / "commands.json", commands)
        with self.assertRaisesRegex(Blocked, "actual server option readbacks"):
            native.tmux_context(case)

    def test_m9_controls_cli_rejects_incomplete_and_unknown_requests(self):
        self.assertEqual(native.main(["--terminals", "unknown", "--plan"]), 2)
        self.assertEqual(native.main([]), 2)
        request = self.root / "request.json"
        identity = self.root / "identity.json"
        self.write(request, {"identity": self.identity, "records": []})
        self.write(identity, self.identity)
        output = self.root / "cli-output"
        self.assertEqual(native.main(["--observations", str(request), "--identity", str(identity), "--output", str(output), "--terminals", "foot"]), 2)
        manifest = native.load(output / "manifest.json")
        self.assertEqual(len(manifest["records"]), len(native.controls()))
        self.assertTrue(all(r["status"] == "BLOCKED" for r in manifest["records"]))

    def rows_of(self, proof):
        return [(r["ns"], r["direction"], base64.b64decode(r["base64"])) for r in native.load_lines(self.case(proof) / "pty.log")]

    def all_checks(self, label, proof):
        control = next(c for c in native.controls() if c["id"] == label)
        return {check: [proof] for check in control["checks"]}

    def must_block(self, label, assertions, reason):
        control = next(c for c in native.controls() if c["id"] == label)
        result, output = self.produce(control, assertions)
        record = result["records"][0]
        verdict = native.load(output / record["directory"] / "verdict.json")
        self.assertNotEqual(verdict["behavior"], "PASS", "attack reached PASS: " + record.get("reason", ""))
        self.assertIn(reason, record.get("reason", ""))

    def test_m9_controls_r2_f1_fragmentation_must_reach_candidate(self):
        proof = self.negotiating()
        case = self.case(proof)
        native.native_case(proof, self.identity, "foot", "query-fragmented")
        raw = native.tokens(case, "terminal-input")
        forwarded = native.tokens(case, "candidate-input")
        self.assertTrue(any(t["first_event"] != t["last_event"] for t in raw))
        self.assertTrue(all(t["first_event"] == t["last_event"] for t in forwarded))
        with self.assertRaisesRegex(Blocked, "in fragments"):
            native.check_proof("fragmented", "query-fragmented", proof, case)

    def test_m9_controls_r2_f1_interleave_must_reach_candidate(self):
        proof = self.negotiating()
        rows = [row for row in self.rows_of(proof) if row[1] != "candidate-input"]
        kitty, da1 = b"\x1b_Gi=7;OK\x1b\\", b"\x1b[?1;4c"
        rows += [(75, "candidate-input", kitty + da1), (90, "candidate-input", b"X")]
        rows.sort(key=lambda row: row[0])
        proof["events"] = [next(i for i, row in enumerate(rows) if row[1:] == ("terminal-input", b"X"))]
        self.log(proof, rows)
        native.native_case(proof, self.identity, "foot", "query-fragmented")
        with self.assertRaisesRegex(Blocked, "between two replies"):
            native.check_proof("interleaved", "query-fragmented", proof, self.case(proof))

    def test_m9_controls_r2_f1_query_fragmented_cannot_pass_without_candidate_transform(self):
        proof = self.negotiating()
        rows = [row for row in self.rows_of(proof) if row[1] != "candidate-input"]
        rows += [(75, "candidate-input", b"\x1b_Gi=7;OK\x1b\\\x1b[?1;4c"), (90, "candidate-input", b"X")]
        rows.sort(key=lambda row: row[0])
        proof["events"] = [next(i for i, row in enumerate(rows) if row[1:] == ("terminal-input", b"X"))]
        self.log(proof, rows)
        self.must_block("query-fragmented", self.all_checks("query-fragmented", proof), "in fragments")

    def test_m9_controls_r2_f2_deadline_must_observe_disabled_feature(self):
        proof = self.fixture(positive=False)
        self.log(proof, self.rows_of(proof) + [(150, "output", b"\x1b[HREADY")])
        self.sealed_anchor(proof, "deadline.json", dict(query_ns=10, disabled_ns=150, deadline_ns=500))
        self.must_block("query-timeout", self.all_checks("query-timeout", proof), "disabled-state observation hook")

    def test_m9_controls_r2_f3_signal_must_run_signal_sender(self):
        proof = self.fixture()
        case = self.case(proof)
        commands = native.load(case / "commands.json")
        commands.append(dict(argv=["/usr/bin/true", "-TERM", "103"], start_ns=160, end_ns=161, exit_code=0))
        self.log(proof, self.rows_of(proof), commands)
        self.sealed_anchor(proof, "signal.json", dict(signal="TERM", pid=103, command=1))
        self.sealed_anchor(proof, "process-census.json", dict(remaining=[], checked=["candidate", "relay", "mpv", "decoder", "tmux"],
                                                              after_close_ns=300, processes_before=[dict(pid=103, kind="candidate")]))
        self.seal(proof, "relay-start.json")
        self.must_block("shutdown-signal", self.all_checks("shutdown-signal", proof), "kill(1) sender")

    def test_m9_controls_r2_f4_cyrillic_must_run_hyprctl(self):
        proof = self.fixture()
        letter = b"\x1b[1092::97u"
        rows = [(10, "output", b"\x1b_Gi=7,a=q;AAAA\x1b\\\x1b[c\x1b[?u"), (20, "terminal-input", b"\x1b[?0u"),
                (25, "candidate-input", b"\x1b[?0u"), (30, "output", b"\x1b[>29u"),
                (120, "terminal-input", letter), (140, "candidate-input", letter)]
        layout = dict(argv=["/usr/bin/true", "switchxkblayout", "current", "1"], start_ns=50, end_ns=60, exit_code=0)
        proof.update(dispatch=dict(source="compositor", command=1, mods="", key="a", deadline_ns=200), events=[4], hex=letter.hex())
        self.log(proof, rows, [layout, self.shortcut("", "a", 100)])
        native.native_case(proof, self.identity, "foot", "keys-emission")
        with self.assertRaisesRegex(Blocked, f"needs a successful {TOOL} switchxkblayout with stdout ok before the key"):
            native.check_proof("Cyrillic-base-layout", "keys-emission", proof, self.case(proof))

    def test_m9_controls_r2_f5_stale_control_cannot_accept_fabricated_current_reply(self):
        proof = self.fixture(positive=False)
        rows = self.rows_of(proof)
        rows[0] = (10, "output", b"\x1b_Gi=7,a=q;AAAA\x1b\\\x1b_Gi=8,a=q;AAAA\x1b\\\x1b[c\x1b[?u")
        rows[1] = (30, "terminal-input", b"\x1b_Gi=7;OK\x1b\\\x1b[?1;2c")
        rows[2] = (35, "candidate-input", b"\x1b_Gi=8;OK\x1b\\\x1b[?1;2c")
        self.log(proof, rows)
        stale, wrong = deepcopy(proof), deepcopy(proof)
        self.anchor(stale, "stale-before-query.json", dict(synthetic=True, status="PASS", scenario="stale-before-query",
                                                           input_hex=b"\x1b_Gi=7;OK\x1b\\".hex(), accepted=False, before_query=True))
        stale["decoder_artifact"] = "stale-before-query.json"
        self.anchor(wrong, "wrong-id.json", dict(synthetic=True, status="PASS", scenario="wrong-id",
                                                 input_hex=b"\x1b_Gi=9;OK\x1b\\".hex(), accepted=False))
        wrong["decoder_artifact"] = "wrong-id.json"
        self.must_block("query-stale-id", {"stale-before-query": [stale], "wrong-id": [wrong], "matching-current-id": [proof]},
                        "not forwarded verbatim")

    def test_m9_controls_r2_f6_keyboard_push_pop_restores_previous_flags(self):
        proof = self.path_proof("redo-enhanced")
        case = self.case(proof)
        rows = [(ns, direction, data.replace(b"\x1b[?31u", b"\x1b[?0u")) for ns, direction, data in self.rows_of(proof)]
        rows += [(40, "output", b"\x1b[>29u"), (50, "output", b"\x1b[>0u"), (60, "output", b"\x1b[<u")]
        rows.sort(key=lambda row: row[0])
        proof["events"] = proof["sequence"][0]["events"] = [6, 7]
        self.log(proof, rows)
        native.native_case(proof, self.identity, "foot", "keys-presets")
        self.assertEqual(native.keyboard_mode(case), "extended", "pop restores pushed 29 flags before key")
        native.check_proof("key-path:redo-enhanced", "keys-presets", proof, case)

    def test_m9_controls_r2_f6_keyboard_set_and_pop_follow_the_stack(self):
        for name, output, mode in (("set-clears-flags", [(40, b"\x1b[>29u"), (50, b"\x1b[=0u")], "legacy"),
                                   ("pop-two-returns-to-base", [(40, b"\x1b[>29u"), (50, b"\x1b[>0u"), (60, b"\x1b[<u"), (70, b"\x1b[<u")], "legacy"),
                                   ("set-or-keeps-flags", [(40, b"\x1b[>29u"), (50, b"\x1b[=4;2u")], "extended")):
            with self.subTest(name):
                proof = self.path_proof("redo-enhanced")
                case = self.case(proof)
                rows = [(ns, direction, data.replace(b"\x1b[?31u", b"\x1b[?0u")) for ns, direction, data in self.rows_of(proof)]
                rows += [(ns, "output", data) for ns, data in output]
                rows.sort(key=lambda row: row[0])
                proof["events"] = proof["sequence"][0]["events"] = [i for i, row in enumerate(rows) if row[1] == "terminal-input" and row[0] >= 120]
                self.log(proof, rows)
                native.native_case(proof, self.identity, "foot", "keys-presets")
                self.assertEqual(native.keyboard_mode(case), mode, name)
                if mode == "extended":
                    native.check_proof("key-path:redo-enhanced", "keys-presets", proof, case)
                else:
                    with self.assertRaisesRegex(Blocked, "keyboard mode"):
                        native.check_proof("key-path:redo-enhanced", "keys-presets", proof, case)

    def test_m9_controls_r2_f6_keyboard_mode_must_be_at_key_time(self):
        proof = self.path_proof("redo-enhanced")
        rows = [(ns, direction, data.replace(b"\x1b[?31u", b"\x1b[?0u")) for ns, direction, data in self.rows_of(proof)]
        rows += [(150, "output", b"\x1b[>29u")]
        self.log(proof, rows)
        native.native_case(proof, self.identity, "foot", "keys-presets")
        with self.assertRaisesRegex(Blocked, "keyboard mode"):
            native.check_proof("key-path:redo-enhanced", "keys-presets", proof, self.case(proof))

    def test_m9_controls_r2_f7_verbatim_must_not_forward_before_native_arrival(self):
        proof = self.fixture()
        rows = self.rows_of(proof)
        reply = rows[1][2]
        rows[2] = (35, "candidate-input", reply[5:])
        rows.insert(1, (20, "candidate-input", reply[:5]))
        self.log(proof, rows)
        native.native_case(proof, self.identity, "foot", "query-native")
        with self.assertRaisesRegex(Blocked, "verbatim"):
            native.check_proof("native-positive", "query-native", proof, self.case(proof))

    def test_m9_controls_r2_verbatim_accepts_da1_variants(self):
        for da1 in (b"\x1b[?1;2c", b"\x1b[?62;4;6;22c", b"\x1b[?64;1;2;6;9;15;18;21;22c"):
            with self.subTest(da1=da1):
                proof = self.fixture()
                rows = self.rows_of(proof)
                for i in (1, 2):
                    rows[i] = (rows[i][0], rows[i][1], b"\x1b_Gi=7;OK\x1b\\" + da1)
                self.log(proof, rows)
                native.native_case(proof, self.identity, "foot", "query-native")
                native.check_proof("native-positive", "query-native", proof, self.case(proof))

    def test_m9_controls_r3_p1_stale_checks_bind_to_their_own_native_bytes(self):
        queries = b"\x1b_Gi=7,a=q;AAAA\x1b\\\x1b_Gi=8,a=q;AAAA\x1b\\\x1b[c\x1b[?u"
        good = self.fixture(positive=False)
        rows = self.rows_of(good)
        rows[0] = (10, "output", queries)
        rows[1] = (30, "terminal-input", b"\x1b_Gi=7;OK\x1b\\\x1b_Gi=8;OK\x1b\\\x1b[?1;2c")
        rows[2] = (35, "candidate-input", b"\x1b_Gi=7;OK\x1b\\\x1b_Gi=8;OK\x1b\\\x1b[?1;2c")
        self.log(good, rows)
        native.check_proof("matching-current-id", "query-stale-id", good, self.case(good))
        bad = self.fixture(positive=False)
        rows = self.rows_of(bad)
        rows[0] = (10, "output", queries)
        rows[1] = (30, "terminal-input", b"\x1b_Gi=7;OK\x1b\\\x1b[?1;2c")
        rows[2] = (35, "candidate-input", b"\x1b_Gi=8;OK\x1b\\\x1b[?1;2c")
        self.log(bad, rows)
        stale, wrong = deepcopy(bad), deepcopy(bad)
        self.anchor(stale, "stale-before-query.json", dict(synthetic=True, status="PASS", scenario="stale-before-query",
                    input_hex=b"\x1b_Gi=7;OK\x1b\\".hex(), accepted=False, before_query=True))
        stale["decoder_artifact"] = "stale-before-query.json"
        self.anchor(wrong, "wrong-id.json", dict(synthetic=True, status="PASS", scenario="wrong-id",
                    input_hex=b"\x1b_Gi=9;OK\x1b\\".hex(), accepted=False))
        wrong["decoder_artifact"] = "wrong-id.json"
        self.must_block("query-stale-id", {"stale-before-query": [stale], "wrong-id": [wrong], "matching-current-id": [good]},
                        "not forwarded verbatim")

    def test_m9_controls_r3_p2_disabled_hook_must_be_sealed(self):
        proof = self.fixture(positive=False)
        self.anchor(proof, "deadline.json", dict(query_ns=10, disabled_ns=150, deadline_ns=500))
        self.anchor(proof, "disabled-state.json", dict(hook="tui16-disabled-state", state="disabled", ns=150))
        sealed = native.load(self.case(proof) / "verdict.json")["artifact_sha256"]
        self.assertNotIn("disabled-state.json", sealed)
        self.assertFalse(any(r["ns"] == 150 for r in native.events(self.case(proof))))
        self.must_block("query-timeout", self.all_checks("query-timeout", proof), "not sealed")

    def test_m9_controls_r3_p3_push_split_across_relay_reads_is_still_a_push(self):
        proof = self.path_proof("redo-enhanced")
        rows = [(ns, d, b.replace(b"\x1b[?31u", b"\x1b[?0u")) for ns, d, b in self.rows_of(proof)]
        rows += [(40, "output", b"\x1b[H frame \x1b[>2"), (41, "output", b"9u")]
        rows.sort(key=lambda r: r[0])
        proof["events"] = proof["sequence"][0]["events"] = [i for i, r in enumerate(rows) if r[1] == "terminal-input" and r[0] >= 120]
        self.log(proof, rows)
        native.native_case(proof, self.identity, "foot", "keys-presets")
        self.assertEqual(native.keyboard_mode(self.case(proof)), "extended", "pushed 29 flags split over two reads")

    def test_m9_controls_r3_p4_layout_readback_must_be_sealed_and_observed(self):
        proof = self.fixture()
        letter = b"\x1b[1092::97u"
        rows = [(10, "output", b"\x1b_Gi=7,a=q;AAAA\x1b\\\x1b[c\x1b[?u"), (20, "terminal-input", b"\x1b[?0u"),
                (25, "candidate-input", b"\x1b[?0u"), (30, "output", b"\x1b[>29u"),
                (120, "terminal-input", letter), (140, "candidate-input", letter)]
        layout = dict(argv=[TOOL, "switchxkblayout", "current", "1"], start_ns=50, end_ns=60, exit_code=0, stdout="error\n")
        proof.update(dispatch=dict(source="compositor", command=1, mods="", key="a", deadline_ns=200), events=[4], hex=letter.hex())
        self.log(proof, rows, [layout, self.shortcut("", "a", 100)])
        self.anchor(proof, "layout.json", dict(before="x", after="y", ns=80))
        self.assertNotIn("layout.json", native.load(self.case(proof) / "verdict.json")["artifact_sha256"])
        with self.assertRaisesRegex(Blocked, "switchxkblayout"):
            native.check_proof("Cyrillic-base-layout", "keys-emission", proof, self.case(proof))


    def test_m9_controls_r3_every_hook_read_requires_the_capture_seal(self):
        good = self.fixture(positive=False)
        self.anchor(good, "stale-before-query.json", dict(synthetic=True, status="PASS", scenario="stale-before-query",
                    input_hex=b"\x1b_Gi=7;OK\x1b\\".hex(), accepted=False, before_query=True))
        good["decoder_artifact"] = "stale-before-query.json"
        with self.assertRaisesRegex(Blocked, "not sealed"):
            native.check_proof("stale-before-query", "query-stale-id", good, self.case(good))
        deadline = self.fixture(positive=False)
        self.sealed_anchor(deadline, "deadline.json", dict(query_ns=10, disabled_ns=150, deadline_ns=500))
        self.log(deadline, self.rows_of(deadline) + [(150, "output", b"\x1b[HREADY")])
        self.anchor(deadline, "disabled-state.json", dict(hook="tui16-disabled-state", state="disabled", ns=150))
        with self.assertRaisesRegex(Blocked, "not sealed"):
            native.check_proof("bounded-deadline", "query-timeout", deadline, self.case(deadline))
        signal = self.fixture()
        case = self.case(signal)
        commands = native.load(case / "commands.json") + [{"argv": ["kill", "-TERM", "103"], "start_ns": 160, "end_ns": 161, "exit_code": 0}]
        self.log(signal, self.rows_of(signal), commands)
        self.anchor(signal, "signal.json", dict(signal="TERM", pid=103, command=1))
        self.anchor(signal, "process-census.json", dict(remaining=[], checked=["candidate", "relay", "mpv", "decoder", "tmux"],
                    before_ns=150, after_close_ns=300, processes_before=[dict(pid=103, kind="candidate")]))
        with self.assertRaisesRegex(Blocked, "not sealed"):
            native.check_proof("termios", "shutdown-signal", signal, case)
        self.seal(signal, "signal.json")
        with self.assertRaisesRegex(Blocked, "not sealed"):
            native.check_proof("termios", "shutdown-signal", signal, case)

    def test_m9_controls_r3_disabled_ns_must_be_a_sealed_output_event(self):
        proof = self.fixture(positive=False)
        self.sealed_anchor(proof, "deadline.json", dict(query_ns=10, disabled_ns=150, deadline_ns=500))
        self.sealed_anchor(proof, "disabled-state.json", dict(hook="tui16-disabled-state", state="disabled", ns=150))
        with self.assertRaisesRegex(Blocked, "bounded deadline was not observed"):
            native.check_proof("bounded-deadline", "query-timeout", proof, self.case(proof))

    def test_m9_controls_r4_q1_synthetic_separated_needs_a_sealed_decoder(self):
        proof = self.fixture()
        self.unseal(proof, "decoder.json")
        with self.assertRaisesRegex(Blocked, "not sealed by the capture verdict: decoder.json"):
            native.check_proof("synthetic-separated", "shift-delete-legacy", proof, self.case(proof))

    def test_m9_controls_r4_q2_children_reaped_needs_a_sealed_census(self):
        proof = self.fixture()
        self.anchor(proof, "process-census.json", {"remaining": [], "checked": ["candidate", "relay", "mpv", "decoder", "tmux"],
                    "after_close_ns": 300, "processes_before": [{"pid": 102, "kind": "candidate"}]})
        with self.assertRaisesRegex(Blocked, "not sealed by the capture verdict: process-census.json"):
            native.check_proof("children-reaped", "shutdown-normal", proof, self.case(proof))

    def test_m9_controls_r4_q3_each_signal_hook_needs_its_own_seal(self):
        for name in ("signal.json", "relay-start.json", "process-census.json"):
            with self.subTest(unsealed=name):
                proof = self.signal_proof()
                self.unseal(proof, name)
                with self.assertRaisesRegex(Blocked, "not sealed by the capture verdict: " + name):
                    native.check_proof("termios", "shutdown-signal", proof, self.case(proof))

    def test_m9_controls_r4_q4_push_completed_after_the_key_is_not_in_force(self):
        proof = self.path_proof("redo-enhanced")
        rows = [(ns, d, b.replace(b"\x1b[?31u", b"\x1b[?0u")) for ns, d, b in self.rows_of(proof)]
        rows += [(40, "output", b"\x1b[>2"), (150, "output", b"9u")]
        rows.sort(key=lambda r: r[0])
        proof["events"] = proof["sequence"][0]["events"] = [i for i, r in enumerate(rows) if r[1] == "terminal-input" and r[0] >= 120]
        self.log(proof, rows)
        key = native.key_time(self.case(proof), proof)
        self.assertEqual(native.keyboard_mode(self.case(proof), key), "legacy", "push finished at 150, key at 120")

if __name__ == "__main__":
    unittest.main()
