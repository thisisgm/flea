#!/usr/bin/env bash
# Checks inventory identity, the mounted runner envelope and proof write boundaries.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
parent="$repo/.superpowers/m9-uish-01"
[[ ! -L "$repo/.superpowers" && ! -L "$parent" ]] || exit 2
mkdir -p "$parent"
root=$(mktemp -d "$parent/controls.XXXXXXXX")
printf 'flea inventory controls\n' > "$root/.flea-test-sandbox"
cat > "$root/controls.py" <<'PY'
import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

REPO = Path(sys.argv[1])
ROOT = Path(sys.argv[2])
sys.path.insert(0, str(REPO / "tests/lib"))
import flea_uish_inventory as inventory
import flea_uish_candidate as candidate


class Controls(unittest.TestCase):
    def setUp(self):
        self.root = ROOT / self.id().rsplit(".", 1)[1]
        self.root.mkdir()
        (self.root / ".flea-test-sandbox").write_text("flea owned inventory control\n")
        self.repo = self.root / "source"
        self.repo.mkdir()
        sources = inventory.oracle_sources(REPO)
        paths = set(sources) | set(inventory.BATCHES) | {
            "tests/ui-uish/manifest.sh", "tests/lib/flea_uish_inventory.py"}
        for path in paths:
            source = REPO / path
            if source.exists():
                destination = self.repo / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)

    def rejected(self, label, mutate):
        mutate()
        result = subprocess.run([sys.executable, "-B", str(self.repo / "tests/lib/flea_uish_inventory.py"),
                                 "--repo", str(self.repo), "--check"], capture_output=True, text=True,
                                timeout=15, stdin=subprocess.DEVNULL, start_new_session=True)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertTrue(result.stderr.startswith("REFUSED:"), result.stderr)
        print(f"CONTROL {label} rc={result.returncode} cause={result.stderr.strip()}")

    def ui(self, *arguments, driver=None, frontend="gpui"):
        environment = {"PATH": os.environ["PATH"], "FLEA_TEST_FRONTEND": frontend}
        if driver is not None:
            environment["FLEA_UI_DRIVER"] = driver
        return subprocess.run(["bash", str(self.repo / "tests/ui.sh"), *arguments],
                              env=environment, capture_output=True, text=True, timeout=15,
                              stdin=subprocess.DEVNULL, start_new_session=True)

    def test_inventory(self):
        result = inventory.check(self.repo)
        self.assertEqual((result["wanted"], result["inline"], result["sourced"], result["declared_overrides"]),
                         (111, 84, 27, 84))
        self.assertGreaterEqual(result["installed_overrides"], 68)
        print("INVENTORY " + json.dumps(result, sort_keys=True))

    def test_missing_definition(self):
        path = self.repo / "tests/ui.sh"
        self.rejected("missing", lambda: path.write_text(path.read_text().replace("case_cursor()", "absent_cursor()", 1)))

    def test_duplicate_override(self):
        path = self.repo / "tests/ui-uish/launch.sh"
        self.rejected("duplicate", lambda: path.write_text(path.read_text() + "\ncase_select() { :; }\n"))

    def test_reordered_wanted(self):
        path = self.repo / "tests/ui.sh"
        self.rejected("reordered", lambda: path.write_text(path.read_text().replace("wanted=(cursor scroll ", "wanted=(scroll cursor ", 1)))

    def test_opt_in_capture(self):
        path = self.repo / "tests/ui.sh"
        self.rejected("capture", lambda: path.write_text(path.read_text().replace("wanted=(cursor scroll ", "wanted=(cursor cap_raw_grid scroll ", 1)))

    def test_by_name_only(self):
        path = self.repo / "tests/ui.sh"
        self.rejected("by-name-only", lambda: path.write_text(path.read_text().replace("wanted=(cursor scroll ", "wanted=(readme scroll ", 1)))

    def test_oracle_unchanged(self):
        path = self.repo / "tests/ui.sh"
        self.rejected("oracle", lambda: path.write_text(path.read_text().replace("case_cursor() {", "case_cursor() {\n    :", 1)))

    def test_native_final_checks_unchanged(self):
        path = self.repo / "tests/ui.sh"
        self.rejected("native-checks", lambda: path.write_text(path.read_text().replace("+ 3 ))", "+ 2 ))", 1)))

    def test_duplicate_override_alternate_syntax(self):
        path = self.repo / "tests/ui-uish/launch.sh"
        self.rejected("duplicate-function", lambda: path.write_text(path.read_text() + "\nfunction case_select { :; }\n"))

    def test_runtime_gpui_and_qml_modules(self):
        for frontend in ("qml", "gpui"):
            script = ('repo=$1; test_frontend=$2; case_select() { printf oracle; }; '
                      '. "$repo/tests/ui-uish/manifest.sh"; flea_uish_load || exit; declare -f case_select')
            result = subprocess.run(["bash", "-c", script, "controls", str(self.repo), frontend],
                                    capture_output=True, text=True, timeout=15, stdin=subprocess.DEVNULL)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual("printf oracle" in result.stdout, frontend == "qml")

    def test_native_default_and_explicit(self):
        tools = self.repo / "tools"
        tools.mkdir()
        (tools / "flea-ipc").write_text("exit 23\n")
        for driver in (None, "native"):
            result = self.ui("cursor", driver=driver)
            self.assertEqual(result.returncode, 23, result.stderr)
            self.assertNotIn("HEADLESS_RUN", result.stdout)

    def test_selector_before_native_setup(self):
        result = self.ui("cursor", driver="broken")
        self.assertEqual(result.returncode, 1)
        self.assertIn("unknown UI driver: broken", result.stderr)
        self.assertNotIn("omarchy", result.stderr)

    def test_missing_headless_refuses_without_native_receipt(self):
        result = self.ui("cursor", driver="headless")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("UNSUPPORTED headless", result.stderr)
        self.assertNotIn("NATIVE_RUN", result.stdout + result.stderr)
        self.assertNotIn("PASS ", result.stdout)
        self.assertNotIn("omarchy", result.stderr)

    def test_headless_handoff_preserves_arguments_and_exit(self):
        helper = self.repo / "tests/lib/flea_uish_headless.py"
        helper.write_text('import sys\nprint(repr(sys.argv[1:]))\nprint("PASS control")\nraise SystemExit(7)\n')
        result = self.ui("cursor", "rows", driver="headless")
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertIn("'run-ui', '--repo'", result.stdout)
        self.assertIn("'--', 'cursor', 'rows'", result.stdout)
        self.assertIn("HEADLESS_PASS control", result.stdout)
        self.assertNotIn("NATIVE_RUN", result.stdout + result.stderr)
        self.assertFalse(any(line.startswith("PASS ") for line in result.stdout.splitlines()))

    def test_headless_default_forwards_all_111_in_order(self):
        helper = self.repo / "tests/lib/flea_uish_headless.py"
        helper.write_text('import json,sys\nprint(json.dumps(sys.argv[sys.argv.index("--")+1:]))\n')
        result = self.ui(driver="headless")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout.splitlines()[-1]), list(inventory.ORACLE))

    def test_headless_native_receipt_rejected(self):
        helper = self.repo / "tests/lib/flea_uish_headless.py"
        helper.write_text('print("NATIVE_RUN_END frontend=gpui failures=0")\n')
        result = self.ui("cursor", driver="headless")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("attempted a native receipt", result.stderr)
        self.assertNotIn("NATIVE_RUN", result.stdout)

    @unittest.skipIf(os.geteuid() == 0, "root reads a mode 0111 file, so no PermissionError")
    def test_execute_only_driver_refused_before_spawn(self):
        import flea_uish_headless as headless
        from unittest import mock
        driver = self.root / "execute-only-driver"
        driver.write_text("#!/bin/sh\n")
        driver.chmod(0o111)
        with mock.patch.object(headless.subprocess, "Popen") as spawn, \
                mock.patch.dict(os.environ, {"FLEA_BIN": "/bin/true"}):
            with self.assertRaises(PermissionError):
                headless.Host(driver)
        spawn.assert_not_called()

    def test_failed_host_start_removes_its_root(self):
        import flea_uish_headless as headless
        from unittest import mock
        driver = self.root / "sleeping-driver"
        driver.write_text("#!/bin/sh\nexec sleep 30\n")
        driver.chmod(0o700)
        parent = REPO / ".superpowers/m9-uish-02"
        before = set(parent.glob("host.*")) if parent.is_dir() else set()
        seen, remove = [], headless.Host.remove_root

        def removing(host, error=None):
            seen.append((host.root, host.root.is_dir() and host.root not in before))
            remove(host, error)

        with mock.patch.object(headless, "identity", side_effect=FileNotFoundError("gone")) as identity, \
                mock.patch.object(headless.Host, "remove_root", removing), \
                mock.patch.dict(os.environ, {"FLEA_BIN": "/bin/true"}):
            with self.assertRaises(FileNotFoundError):
                headless.Host(driver)
        identity.assert_called_once()
        self.assertTrue(seen and seen[0][1], f"no new host root existed before removal: {seen}")
        self.assertTrue(seen[0][0].name.startswith("host.") and not seen[0][0].exists())
        self.assertEqual(set(parent.glob("host.*")), before, "a failed start left its host root")

    def test_failed_host_keeps_its_log_tail(self):
        import flea_uish_headless as headless
        from unittest import mock
        parent = REPO / ".superpowers/m9-uish-02"
        before = set(parent.glob("host.*")) if parent.is_dir() else set()
        hello = 'UISH\t{"sequence": null, "result": {"ok": true, "value": {"protocol": "flea-uish-headless-v1"}}}'
        greet = "printf '%s\\n' '" + hello + "'\nread -r line\n"
        said = "flea-uish-headless-v1"

        def body_raises():
            raise RuntimeError("body failed")

        # A failed start, a failed teardown and a raise inside the body each carry the tail exactly once.
        cases = (("start", "printf 'driver said this\\n'\nexec sleep 30\n", TimeoutError, "driver said this", None),
                 ("teardown", greet + "exit 1\n", headless.TransportError, said, None),
                 ("body", greet + "exec sleep 30\n", RuntimeError, said, body_raises))
        for name, script, kind, said_here, body in cases:
            stderr, error = self.host_run(headless, mock, name, script, body, kind)
            notes = "\n".join(getattr(error, "__notes__", ()))
            self.assertEqual(notes.count(said_here), 1, f"{name}: the tail is not on the error exactly once")
            self.assertEqual(stderr.count(said_here), 1, f"{name}: the tail is not printed exactly once")
            self.assertEqual(set(parent.glob("host.*")), before, f"{name}: the marked root was kept")
        # A clean host inside an unrelated handler leaves that exception and stderr alone.
        try:
            raise ValueError("unrelated")
        except ValueError as foreign:
            stderr, error = self.host_run(headless, mock, "clean", greet + "exit 0\n", None, None)
            self.assertIsNone(error)
            self.assertEqual(getattr(foreign, "__notes__", []), [], "a clean host noted a foreign exception")
            self.assertEqual(stderr, "", "a clean host printed a log tail")
        self.assertEqual(set(parent.glob("host.*")), before, "clean: the marked root was kept")

    def test_backend_defaults_to_the_drivers_profile(self):
        import flea_uish_headless as headless
        from unittest import mock
        hello = 'UISH\t{"sequence": null, "result": {"ok": true, "value": {"protocol": "flea-uish-headless-v1"}}}'
        target = self.root / "target"
        driver = target / "release/deps/uish_driver-0"
        driver.parent.mkdir(parents=True)
        driver.write_text("#!/bin/sh\nprintf '%s\\n' '" + hello + "'\nread -r line\nexit 0\n")
        driver.chmod(0o700)
        backend = target / "release/flea"
        backend.write_text("#!/bin/sh\n")
        backend.chmod(0o700)
        decoy = self.root / "decoy"
        for profile in ("debug", "release"):
            (decoy / profile).mkdir(parents=True)
            (decoy / profile / "flea").write_text("#!/bin/sh\n")
            (decoy / profile / "flea").chmod(0o700)
        # A flea under CARGO_TARGET_DIR is never the backend; only the one beside the driver's profile is.
        with mock.patch.dict(os.environ, {"CARGO_TARGET_DIR": str(decoy)}):
            os.environ.pop("FLEA_BIN", None)
            with headless.Host(driver, timeout=1) as host:
                self.assertEqual(host.source["backend"], str(backend.resolve()))

    def host_run(self, headless, mock, name, script, body, kind):
        driver = self.root / (name + "-driver")
        driver.write_text("#!/bin/sh\n" + script)
        driver.chmod(0o700)
        stderr, error = io.StringIO(), None
        with mock.patch.dict(os.environ, {"FLEA_BIN": "/bin/true"}), contextlib.redirect_stderr(stderr):
            try:
                with headless.Host(driver, timeout=1):
                    if body:
                        body()
            except BaseException as raised:
                error = raised
        if kind is None:
            self.assertIsNone(error, f"{name}: {error!r}")
        else:
            self.assertIsInstance(error, kind, f"{name}: {error!r}")
        return stderr.getvalue(), error

    def test_headless_qml_refused(self):
        result = self.ui("cursor", driver="headless", frontend="qml")
        self.assertEqual(result.returncode, 2)
        self.assertIn("requires FLEA_TEST_FRONTEND=gpui", result.stderr)

    def test_join_outside_allowlist(self):
        patch = ("diff --git a/src/main.rs b/src/main.rs\nindex " + "a" * 40 + ".." + "b" * 40 + " 100644\n").encode()
        from unittest.mock import patch as mock_patch
        parent = self.repo / ".superpowers/m9-uish-01"
        parent.mkdir(parents=True)
        (parent / candidate.MARKER).write_text("owned join control\n")
        join = parent / "JOIN.patch"
        join.write_bytes(patch)
        arguments = ["flea_uish_candidate.py", "--repo", str(self.repo), "--unit", "m9-uish-01",
                     "--join", str(join), "--output", str(parent / "proof")]
        diagnostic = io.StringIO()
        with mock_patch.object(sys, "argv", arguments), mock_patch.object(candidate, "snapshot", return_value={}), \
                mock_patch.object(candidate, "changes", return_value=[]), contextlib.redirect_stderr(diagnostic):
            exit_code = candidate.main()
        self.assertEqual(exit_code, 2)
        self.assertIn("unlisted or repeated join target", diagnostic.getvalue())
        self.assertFalse((parent / "proof").exists())
        print(f"CONTROL unlisted-join rc={exit_code} cause={diagnostic.getvalue().strip()}")

    def test_join_exact_blob_and_traversal(self):
        policy = {"joins": ["src/state.rs"]}
        entry = {"src/state.rs": {"base_blob": "c" * 40, "origin": "base"}}
        patch = ("diff --git a/src/state.rs b/src/state.rs\nindex " + "a" * 40 + ".." + "b" * 40 + " 100644\n").encode()
        with self.assertRaisesRegex(candidate.CandidateError, "exact base"):
            candidate.join_targets(patch, policy, entry)
        for path in ("../outside", "/outside", "src/../../outside", ".git/config"):
            with self.assertRaises(candidate.CandidateError):
                candidate.allowed(path, ["src/**"])
        with self.assertRaisesRegex(candidate.CandidateError, "mismatched base"):
            candidate.snapshot(self.repo, "0" * 40)

    def test_unlisted_leaf_refused(self):
        from unittest.mock import patch
        with patch.object(candidate, "git", side_effect=[b"src/unlisted.rs\0", b""]):
            with self.assertRaisesRegex(candidate.CandidateError, "unlisted leaf"):
                candidate.changes(self.repo, candidate.POLICIES["m9-uish-01"], {})

    def test_join_header_cannot_redirect_target(self):
        policy = {"joins": ["src/state.rs"]}
        entries = {"src/state.rs": {"base_blob": "a" * 40, "origin": "base"}}
        patch = ("diff --git a/src/state.rs b/src/state.rs\nindex " + "a" * 40 + ".." + "b" * 40
                 + " 100644\n--- a/src/unlisted.rs\n+++ b/src/unlisted.rs\n@@ -1 +1 @@\n-old\n+new\n").encode()
        with self.assertRaisesRegex(candidate.CandidateError, "filename header"):
            candidate.join_targets(patch, policy, entries)

    def test_candidate_write_guards_and_modes(self):
        unit = "m9-uish-01"
        parent = self.repo / ".superpowers" / unit
        parent.mkdir(parents=True)
        outside = self.root / "sentinel"
        outside.mkdir()
        sentinel = outside / "keep"
        sentinel.write_text("keep")
        candidate.validate_output(self.repo, unit, parent / "proof")
        (parent / candidate.MARKER).write_text("owned\n")
        with self.assertRaisesRegex(candidate.CandidateError, "outside"):
            candidate.validate_output(self.repo, unit, outside / "proof")
        (parent / "escape").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(candidate.CandidateError, "symlink"):
            candidate.validate_output(self.repo, unit, parent / "escape/proof")
        proof = parent / "proof"
        candidate.validate_output(self.repo, unit, proof)
        proof.mkdir()
        with self.assertRaisesRegex(candidate.CandidateError, "already exists"):
            candidate.validate_output(self.repo, unit, proof)
        with self.assertRaisesRegex(candidate.CandidateError, "marker"):
            candidate.write_entry(proof, "unmarked", {"mode": "100644", "data": b"refuse"})
        (proof / candidate.MARKER).write_text("owned proof\n")
        candidate.write_entry(proof, "tool", {"mode": "100755", "data": b"#!/bin/sh\nexit 0\n"})
        candidate.write_entry(proof, "link", {"mode": "120000", "data": b"tool"})
        self.assertEqual((proof / "tool").stat().st_mode & 0o777, 0o755)
        self.assertEqual(os.readlink(proof / "link"), "tool")
        (proof / "escape").symlink_to(outside, target_is_directory=True)
        with self.assertRaises(candidate.CandidateError):
            candidate.write_entry(proof, "escape/keep", {"mode": "100644", "data": b"changed"})
        self.assertEqual(sentinel.read_text(), "keep")


result = unittest.main(argv=[sys.argv[0]], verbosity=2, exit=False)
if result.result.testsRun == 0 or not result.result.wasSuccessful():
    raise SystemExit(1)
print(f"PASS ui-uish-inventory {result.result.testsRun} controls")
PY
exec setsid -w env -i PATH="$PATH" python3 -B "$root/controls.py" "$repo" "$root" </dev/null
