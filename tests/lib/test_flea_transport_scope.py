"""The scope receipt authenticates one exact child and rejects other launch members."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import flea_transport_scope as scope
import flea_transport as transport


class ScopeReceiptTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='flea-scope.')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / '.flea-test-sandbox').write_text('owned scope receipt control\n')
        self.binary = str(Path('/proc/self/exe').resolve())
        self.record = dict(root=str(self.root), binary=self.binary, gui=self.binary)
        self.current = scope.identity(os.getpid())
        self.receipt = dict(pid=os.getpid(), start=self.current['start'], binary=self.binary)

    def write(self):
        path = self.root / 'scope-launch.json'
        path.write_text(json.dumps(self.receipt))
        path.chmod(0o600)

    def test_exact_candidate_is_accepted(self):
        self.write()
        self.assertEqual(scope.admit(os.getpid(), self.record)['start'], self.current['start'])

    def test_other_descendant_is_rejected(self):
        self.receipt['pid'] += 1
        self.write()
        with self.assertRaisesRegex(ValueError, 'launch receipt'):
            scope.admit(os.getpid(), self.record)

    def test_reused_pid_is_rejected(self):
        self.receipt['start'] = str(int(self.current['start']) + 1)
        self.write()
        with self.assertRaisesRegex(ValueError, 'launch receipt'):
            scope.admit(os.getpid(), self.record)

    def test_other_binary_is_rejected(self):
        self.receipt['binary'] = '/another/build/flea'
        self.write()
        with self.assertRaisesRegex(ValueError, 'launch receipt'):
            scope.admit(os.getpid(), self.record)

    def test_exec_replacement_is_rejected(self):
        self.record.update(binary='/another/build/flea', gui='/another/build/flea-gui')
        self.receipt['binary'] = self.record['binary']
        self.write()
        with self.assertRaisesRegex(ValueError, 'candidate executable'):
            scope.admit(os.getpid(), self.record)

    def test_symlink_receipt_is_rejected(self):
        self.write()
        path = self.root / 'scope-launch.json'
        target = self.root / 'other.json'
        path.rename(target)
        path.symlink_to(target)
        with self.assertRaisesRegex(ValueError, 'unsafe'):
            scope.admit(os.getpid(), self.record)

    def test_group_writable_receipt_is_rejected(self):
        self.write()
        (self.root / 'scope-launch.json').chmod(0o620)
        with self.assertRaisesRegex(ValueError, 'unsafe'):
            scope.admit(os.getpid(), self.record)

    def test_writer_refuses_a_command_that_is_not_the_candidate(self):
        self.root.chmod(0o700)
        (self.root / 'options.json').write_text(json.dumps(dict(binary='/build/flea')))
        with patch.dict(os.environ, FLEA_TEST_WINDOW=str(self.root)), \
                patch.object(scope.sys, 'argv', ['helper', str(self.root), '/bin/true']), \
                patch.object(scope.os, 'execv') as execute:
            with self.assertRaisesRegex(ValueError, 'not the admitted candidate'):
                scope.main()
            execute.assert_not_called()
        self.assertFalse((self.root / 'scope-launch.json').exists())

    def test_writer_refuses_another_launch_root(self):
        self.root.chmod(0o700)
        (self.root / 'options.json').write_text(json.dumps(dict(binary='/bin/true')))
        with patch.dict(os.environ, FLEA_TEST_WINDOW='/tmp/another-window'), \
                patch.object(scope.sys, 'argv', ['helper', str(self.root), '/bin/true']), \
                patch.object(scope.os, 'execv') as execute:
            with self.assertRaisesRegex(ValueError, 'not this launch'):
                scope.main()
            execute.assert_not_called()

    def test_writer_keeps_receipt_private_without_changing_candidate_umask(self):
        (self.root / 'options.json').write_text(json.dumps(dict(binary='/bin/true')))
        observed = []

        def execute(binary, arguments):
            current = os.umask(0o022)
            os.umask(current)
            observed.append(current)

        previous = os.umask(0o022)
        try:
            with patch.dict(os.environ, FLEA_TEST_WINDOW=str(self.root)), \
                    patch.object(scope.sys, 'argv', ['helper', str(self.root), '/bin/true']), \
                    patch.object(scope.os, 'execv', side_effect=execute):
                scope.main()
        finally:
            os.umask(previous)
        self.assertEqual(observed, [0o022])
        self.assertEqual((self.root / 'scope-launch.json').stat().st_mode & 0o777, 0o600)

    def test_manager_environment_does_not_replace_child_session(self):
        environment = dict(XDG_RUNTIME_DIR='/private/runtime', DBUS_SESSION_BUS_ADDRESS='unix:path=/private/bus')
        info = type('Info', (), dict(st_mode=0o140600, st_uid=os.getuid()))()
        with patch.object(Path, 'lstat', return_value=info):
            command, manager = scope.launch(self.root, ['systemd-run', '--user', '--scope'], ['/build/flea', '--gui'], environment)
        self.assertIn('--setenv=XDG_RUNTIME_DIR=/private/runtime', command)
        self.assertIn('--setenv=DBUS_SESSION_BUS_ADDRESS=unix:path=/private/bus', command)
        self.assertEqual(manager['DBUS_SESSION_BUS_ADDRESS'], f'unix:path=/run/user/{os.getuid()}/bus')
        self.assertEqual(environment['XDG_RUNTIME_DIR'], '/private/runtime')


class ComponentFixtureTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='flea-component.')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / '.flea-test-sandbox').write_text('owned component fixture control\n')
        self.binary = self.root / 'debug/flea'
        self.fixture = self.root / 'debug/examples/html-lifecycle'
        self.fixture.parent.mkdir(parents=True)

    def reject(self, reason, **options):
        with patch.object(transport, 'built_binary', return_value=str(self.binary)), \
                patch.object(transport.subprocess, 'Popen') as spawn:
            with self.assertRaisesRegex(transport.TransportError, reason):
                transport.Window.start(self.root, fixture='html-lifecycle', frontend='gpui', **options)
            spawn.assert_not_called()

    def test_missing_matching_fixture_is_rejected(self):
        self.reject('missing matching')

    def test_fixture_symlink_cannot_escape_the_candidate_build(self):
        self.fixture.symlink_to('/bin/true')
        self.reject('missing matching')

    def test_matching_fixture_is_admitted_as_the_launch_command(self):
        self.fixture.write_text('#!/bin/sh\n')
        self.fixture.chmod(0o700)
        with patch.object(transport, 'built_binary', return_value=str(self.binary)), \
                patch.object(transport.subprocess, 'Popen', side_effect=RuntimeError('supervisor spawn')) as spawn:
            with self.assertRaisesRegex(RuntimeError, 'supervisor spawn'):
                transport.Window.start(self.root, parent=self.root, fixture='html-lifecycle', frontend='gpui')
            spawn.assert_called_once()
        options = [json.loads(path.read_text()) for path in self.root.glob('w.*/options.json')]
        self.assertEqual([option['fixture'] for option in options], [str(self.fixture)])

    def test_component_fixture_cannot_reuse_a_picker_or_scope_launch(self):
        self.reject('unsupported', picker=True)
        self.reject('unsupported', wrap=['systemd-run', '--scope'])


if __name__ == '__main__':
    unittest.main()
