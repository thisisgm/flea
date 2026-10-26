#!/usr/bin/python3
"""Exercise the prepared external payload against a minimal synthetic preimage."""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('payload', HERE / 'prove_ci_runner.py')
payload = importlib.util.module_from_spec(spec)
spec.loader.exec_module(payload)
ROOT = Path(sys.argv.pop()).resolve()


class PreparedPayloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        ROOT.mkdir()
        (ROOT / '.flea-test-sandbox').touch()
        cls.preimage = HERE / 'data/prove-ci-preimage'
        for line in (cls.preimage / 'SHA256SUMS').read_text().splitlines():
            digest, name = line.split('  ', 1)
            if hashlib.sha256((cls.preimage / name).read_bytes()).hexdigest() != digest:
                raise AssertionError('reviewed preimage changed: ' + name)
        cls.output = ROOT / 'prepared'
        payload.prepare(cls.preimage, cls.output)
        cls.candidate = cls.output / 'candidate'
        cls.fixture = ROOT / 'payload-fixtures'
        cls.fixture.mkdir()
        (cls.fixture / '.flea-test-sandbox').touch()
        # Run fixtures against function bodies read from the produced candidate, including the staged helper.
        for table, name in [(payload.LANE_FUNCTIONS, 'bin/flea-ci-lane'),
                            (payload.HOST_FUNCTIONS, 'bin/flea-ci')]:
            text = (cls.candidate / name).read_text()
            for function in table:
                # Sample function body: step() {\n  local name=$1\n}
                match = re.search(rf'^{function}\(\) \{{\n.*?^\}}', text, re.M | re.S)
                if not match:
                    raise AssertionError(f'prepared candidate is missing {function}')
                table[function] = match[0]
        payload.HELPER = (cls.candidate / 'bin/flea-ci-workspace.py').read_text()
        with contextlib.redirect_stdout(io.StringIO()):
            cls.fixture_rc = payload.fixtures(cls.fixture)

    def test_prepared_patch_applies_and_every_candidate_hash_matches(self):
        applied = ROOT / 'applied'
        shutil.copytree(self.preimage, applied)
        subprocess.run(['patch', '--batch', '--fuzz=0', '-p1', '-d', str(applied),
                        '-i', str(self.output / 'ops.patch')], check=True, capture_output=True)
        for name in json.loads((self.output / 'patch-hashes.json').read_text()):
            self.assertEqual((applied / name).read_bytes(), (self.candidate / name).read_bytes())
        for name in ['flea-ci', 'flea-ci-lane', 'flea-ci-unit']:
            subprocess.run(['bash', '-n', str(applied / 'bin' / name)], check=True)
        host = (applied / 'bin/flea-ci').read_text()
        self.assertLess(host.index('cp "$ci_home/bin/flea-ci-workspace.py"'), host.index('  stage_receipt\n'))

    def test_the_produced_payload_passes_its_behavior_fixtures(self):
        self.assertEqual(self.fixture_rc, 0)

    def test_both_profiles_have_their_own_successful_audit(self):
        log = (self.fixture / 'test-both-profiles.log').read_text()
        # Sample audit result: RESULT cargo-test-release rc=0 child_rc=0 workspace_audit=0
        records = re.findall(r'^RESULT (cargo-test(?:-release)?) .*workspace_audit=(\d+)', log, re.M)
        self.assertEqual(records, [('cargo-test', '0'), ('cargo-test-release', '0')])

    def test_cargo_stderr_does_not_corrupt_structured_inputs(self):
        cargo = self.fixture / 'bin/cargo'
        original = cargo.read_text()
        cargo.write_text(original.replace("printf '%s\\n'", "echo 'Blocking waiting for file lock on package cache' >&2\nprintf '%s\\n'", 1))
        try:
            test = subprocess.run(['bash', str(self.fixture / 'test-both-profiles.sh')], capture_output=True, text=True)
            # Sample audit result: RESULT cargo-test-release rc=0 child_rc=0 workspace_audit=0
            records = re.findall(r'^RESULT (cargo-test(?:-release)?) .*workspace_audit=(\d+)', test.stdout, re.M)
            self.assertEqual(records, [('cargo-test', '0'), ('cargo-test-release', '0')])
            lint = subprocess.run(['bash', str(self.fixture / 'lint-execution.sh')], capture_output=True, text=True)
            self.assertRegex(lint.stdout, r'RESULT core-dependencies rc=0 child_rc=0')
        finally:
            cargo.write_text(original)

    def test_dispatch_anchor_drift_is_refused(self):
        names = ['bin/flea-ci', 'bin/flea-ci-lane', 'bin/flea-ci-unit', 'layouts/default.layout']
        files = {name: (self.preimage / name).read_text() for name in names}
        self.assertIn('qmllint-gate|file-budget', files['bin/flea-ci-lane'])
        files['bin/flea-ci-lane'] = files['bin/flea-ci-lane'].replace('qmllint-gate|file-budget', 'qmllint-gate  |  file-budget')
        with self.assertRaisesRegex(ValueError, 'anchor'):
            payload.transform(files)

    def run_mutated_fixtures(self, name, table, function, changed):
        root = ROOT / name
        root.mkdir()
        (root / '.flea-test-sandbox').touch()
        with patch.dict(table, {function: changed}), contextlib.redirect_stdout(io.StringIO()):
            rc = payload.fixtures(root)
        self.assertEqual(rc, 1)
        return [check['name'] for check in json.loads((root / 'checks.json').read_text())
                if not check['passed']]

    def test_wrong_advisory_link_fails_positive_lint_control(self):
        original = payload.LANE_FUNCTIONS['deny_offline']
        wrong = original.replace('$root/advisory-db-3157b0e258782691', '$root/advisory-db-wrong')
        self.assertNotEqual(wrong, original)
        self.assertEqual(self.run_mutated_fixtures('wrong-link', payload.LANE_FUNCTIONS,
                                                  'deny_offline', wrong), ['all-lint-results-successful'])

    def test_incomplete_workspace_cannot_publish_prep_done(self):
        original = payload.LANE_FUNCTIONS['lane_prep']
        loose = original.replace('gui_compiled=1$', 'gui_compiled=[01]$')
        self.assertNotEqual(loose, original)
        self.assertEqual(self.run_mutated_fixtures('loose-prep', payload.LANE_FUNCTIONS,
                                                  'lane_prep', loose), ['prep-refuses-incomplete-workspace'])

    def test_failed_prep_must_mark_waiting_suites_notrun(self):
        original = payload.HOST_FUNCTIONS['release_waiting']
        loose = original.replace('state[$key]=notrun', 'state[$key]=waiting')
        self.assertNotEqual(loose, original)
        self.assertEqual(self.run_mutated_fixtures('loose-scheduler', payload.HOST_FUNCTIONS,
                                                  'release_waiting', loose),
                         ['failed-prep-lane-notrun', 'missing-prep-marker-notrun'])

    def test_patch_applies_without_final_preimage_newlines(self):
        preimage = ROOT / 'no-newlines'
        shutil.copytree(self.preimage, preimage)
        for name in ['bin/flea-ci', 'bin/flea-ci-lane', 'bin/flea-ci-unit', 'layouts/default.layout']:
            path = preimage / name
            path.write_bytes(path.read_bytes().rstrip(b'\n'))
        (preimage / 'layouts/default.layout').write_bytes(b'west qml qmllint-gate')
        output = ROOT / 'no-newlines-prepared'
        payload.prepare(preimage, output)
        subprocess.run(['patch', '--batch', '--fuzz=0', '-p1', '-d', str(preimage),
                        '-i', str(output / 'ops.patch')], check=True, capture_output=True)
        for name in json.loads((output / 'patch-hashes.json').read_text()):
            self.assertEqual((preimage / name).read_bytes(), (output / 'candidate' / name).read_bytes())

    def test_drm_inventory_operand_is_path_independent_and_unique(self):
        files = {name: (self.preimage / name).read_text() for name in
                 ['bin/flea-ci', 'bin/flea-ci-lane', 'bin/flea-ci-unit', 'layouts/default.layout']}
        # Sample inventory operand: /srv/ci/dri-suites.txt 2>/dev/null
        files['bin/flea-ci-unit'] = re.sub(r'\S*/dri-suites\.txt(?= 2>/dev/null)',
                                         '/srv/alternate-ci/dri-suites.txt', files['bin/flea-ci-unit'])
        self.assertIn('"$run_dir/in/dri-suites.txt"', payload.transform(files)['bin/flea-ci-unit'])
        files['bin/flea-ci-unit'] += '\n/usr/bin/grep -qx -- "$s" /srv/duplicate/dri-suites.txt 2>/dev/null\n'
        with self.assertRaisesRegex(ValueError, 'DRM inventory'):
            payload.transform(files)

    def test_public_preimage_is_small_and_neutral(self):
        names = ['bin/flea-ci', 'bin/flea-ci-lane', 'bin/flea-ci-unit', 'layouts/default.layout']
        text = '\n'.join((self.preimage / name).read_text() for name in names)
        self.assertLess(len(text.splitlines()), 300)
        self.assertNotRegex(text, r'/home/|ultra-[0-9]+|RULING|GM 20[0-9]{2}|192\.168\.')
        self.assertNotRegex((HERE / 'prove_ci_runner.py').read_text(), r'/home/[^/\s]+/[^/\s]*ops/')


if __name__ == '__main__':
    unittest.main()
