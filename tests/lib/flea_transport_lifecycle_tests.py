"""Deterministic controls for processes disappearing during transport cleanup."""
import errno
import os
import signal
import unittest
from unittest import mock

import flea_transport_lifecycle as lifecycle


class ExitRaces(unittest.TestCase):
    def test_reap_skips_a_disappearing_proc_entry(self):
        # CITRI2's repro-exit-race injects ESRCH for an unrelated /proc entry.
        vanished = mock.Mock(name="entry")
        vanished.name = "2147483000"
        zombie = mock.Mock(name="zombie")
        zombie.name = "2147482999"
        for error in (FileNotFoundError, ProcessLookupError):
            with self.subTest(error=error), mock.patch.object(lifecycle, "Path") as proc, \
                    mock.patch.object(lifecycle, "identity", side_effect=[error(),
                        dict(group=2147483001, parent=os.getpid(), state="Z")]), \
                    mock.patch.object(lifecycle.os, "waitpid", return_value=(2147482999, 0)) as wait:
                proc.return_value.iterdir.return_value = [vanished, zombie]
                lifecycle.reap_descendants(dict(group=2147483001))
                wait.assert_called_once_with(2147482999, os.WNOHANG)

    def test_reap_does_not_hide_unreadable_processes(self):
        entry = mock.Mock()
        entry.name = "2147483000"
        with mock.patch.object(lifecycle, "Path") as proc, \
                mock.patch.object(lifecycle, "identity", side_effect=PermissionError):
            proc.return_value.iterdir.return_value = [entry]
            with self.assertRaises(PermissionError):
                lifecycle.reap_descendants(dict(group=2147483001))

    def test_reap_keeps_process_ownership_checks(self):
        entries = [mock.Mock(), mock.Mock(), mock.Mock()]
        for pid, entry in enumerate(entries, 10):
            entry.name = str(pid)
        identities = [dict(group=41, parent=os.getpid(), state="Z"),
                      dict(group=42, parent=1234, state="Z"),
                      dict(group=42, parent=os.getpid(), state="S")]
        with mock.patch.object(lifecycle, "Path") as proc, \
                mock.patch.object(lifecycle, "identity", side_effect=identities), \
                mock.patch.object(lifecycle.os, "waitpid") as wait:
            proc.return_value.iterdir.return_value = entries
            lifecycle.reap_descendants(dict(group=42))
            wait.assert_not_called()

    def test_natural_exit_accepts_disappearance_and_zombies(self):
        for current, expected in ((dict(state="Z"), True), (dict(state="S"), False)):
            with mock.patch.object(lifecycle, "identity", return_value=current):
                self.assertEqual(lifecycle.exited(42), expected)
        for error in (FileNotFoundError, ProcessLookupError):
            with mock.patch.object(lifecycle, "identity", side_effect=error):
                self.assertTrue(lifecycle.exited(42))
        with mock.patch.object(lifecycle, "identity", side_effect=PermissionError):
            with self.assertRaises(PermissionError):
                lifecycle.exited(42)

    def test_owner_exit_accepts_disappearance_reuse_and_zombies(self):
        for current, expected in ((dict(state="S", start="7"), False), (dict(state="S", start="8"), True),
                                  (dict(state="Z", start="7"), True)):
            with self.subTest(current=current), mock.patch.object(lifecycle, "identity", return_value=current):
                self.assertEqual(lifecycle.exited(42, "7"), expected)
        for error in (FileNotFoundError, ProcessLookupError):
            with self.subTest(error=error), mock.patch.object(lifecycle, "identity", side_effect=error):
                self.assertTrue(lifecycle.exited(42, "7"))
        with mock.patch.object(lifecycle, "identity", side_effect=PermissionError):
            with self.assertRaises(PermissionError):
                lifecycle.exited(42, "7")

    def test_signal_closes_its_handle_when_identity_disappears(self):
        for stage in ("identity", "verify"):
            for error in (FileNotFoundError, ProcessLookupError):
                current = dict(start="7", state="S", group=42, session=42)
                with self.subTest(stage=stage, error=error), \
                        mock.patch.object(lifecycle.os, "pidfd_open", return_value=99), \
                        mock.patch.object(lifecycle.os, "close") as close, \
                        mock.patch.object(lifecycle, "identity", side_effect=error if stage == "identity" else None,
                                          return_value=current), \
                        mock.patch.object(lifecycle.signal, "pidfd_send_signal") as send:
                    self.assertFalse(lifecycle.signal_if_matches(42, "7", signal.SIGTERM, group=42,
                                     verify=mock.Mock(side_effect=error)))
                    close.assert_called_once_with(99)
                    send.assert_not_called()

    def test_signal_refuses_a_reused_or_foreign_process(self):
        for current in (dict(start="8", state="S", group=42, session=42),
                        dict(start="7", state="S", group=43, session=43),
                        dict(start="7", state="Z", group=42, session=42)):
            with mock.patch.object(lifecycle.os, "pidfd_open", return_value=99), \
                    mock.patch.object(lifecycle.os, "close"), \
                    mock.patch.object(lifecycle, "identity", return_value=current), \
                    mock.patch.object(lifecycle.signal, "pidfd_send_signal") as send:
                self.assertFalse(lifecycle.signal_if_matches(42, "7", signal.SIGTERM, group=42))
                send.assert_not_called()

    def test_drain_waits_after_an_unrelated_process_disappears(self):
        entry = mock.Mock()
        entry.name = "2147483000"
        child = mock.Mock()
        child.wait.return_value = 0
        with mock.patch.object(lifecycle, "Path") as proc, \
                mock.patch.object(lifecycle, "identity", side_effect=ProcessLookupError(errno.ESRCH, "No such process")):
            proc.return_value.iterdir.side_effect = lambda: iter([entry])
            outcome = lifecycle.drain(child, dict(group=42, pid=42), lambda: [], mock.Mock(),
                                      terminate=False, grace=1)
        self.assertEqual(outcome, dict(returncode=0, forced=False))
        child.wait.assert_called_once_with(timeout=5)


def check_exit_races(check):
    result = unittest.TextTestRunner(verbosity=1).run(unittest.defaultTestLoader.loadTestsFromTestCase(ExitRaces))
    check(result.wasSuccessful(), "teardown-process-disappearance-controls")


if __name__ == "__main__":
    unittest.main()
