"""Relay setup, resize serialization, final output and teardown regressions."""
import inspect
import os
from pathlib import Path
import signal
import tempfile
import unittest

from . import relay
from .evidence import MARKER
from .relay_support import run_relay

NAMED_BUDGETS = ("READ_BYTES", "COMMAND_BYTES", "RPC_ANSWER_BYTES", "POLL_SECONDS", "FLUSH_SECONDS",
                 "ANSWER_SECONDS", "SHUTDOWN_SECONDS", "SHUTDOWN_POLL_SECONDS")
LITERAL_BUDGETS = ("65536", "1024 * 1024", "8 * 1024 * 1024", ".05", ".15", ".02", "settimeout(2)", "timeout=2", "+ 3")


class RelayTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", Path(__file__).resolve().parents[2] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="relay.", dir=parent))
        (self.root / MARKER).write_text("relay regression\n")

    def test_budgets_are_named_not_literal(self):
        for name in NAMED_BUDGETS:
            with self.subTest(name=name):
                self.assertTrue(hasattr(relay, name))
        for function in (relay.serve, relay.rpc):
            source = inspect.getsource(function)
            for literal in LITERAL_BUDGETS:
                with self.subTest(function=function.__name__, literal=literal):
                    self.assertNotIn(literal, source)

    def test_parsers_carry_sample_input(self):
        for function, parser in ((relay.serve, "request = json.loads(raw)"), (relay.rpc, "return json.loads(chunks)")):
            with self.subTest(parser=parser):
                lines = inspect.getsource(function).splitlines()
                index = next(i for i, line in enumerate(lines) if parser in line)
                self.assertIn("Sample input:", lines[index - 1])

    def test_resize_during_wide_character_feed_is_serialized(self):
        result = run_relay(self.root, "resize-feed")
        self.assertTrue(result["injected"])
        self.assertIsNone(result["error"])
        self.assertEqual(result["code"], 0)
        kinds = [event["direction"] for event in result["events"]]
        self.assertIn("resize", kinds)
        self.assertGreater(kinds.index("resize"), kinds.index("output"))
        # The pending resize is applied inside the loop before the child is reaped, not by the shutdown block.
        self.assertTrue(result["applied_before_reap"])

    def test_setup_failure_restores_terminal_and_closes_resources(self):
        for stage, message in (("start-failure", "File exists"), ("log-failure", "graphics log creation refused")):
            with self.subTest(stage=stage):
                case = self.root / stage
                case.mkdir()
                result = run_relay(case, stage)
                self.assertIn(message, str(result["error"]))
                self.assertTrue(result["restored"])
                self.assertTrue(result["killed"])
                self.assertTrue(result["server_closed"] and result["master_closed"] and result["logs_closed"])
                if stage == "start-failure":
                    # Handlers exist here, so the restore is what is under test.
                    self.assertTrue(result["handlers_installed"])
                    self.assertTrue(result["handlers_restored"])
                else:
                    # The log opens fail before any handler is installed, so there is nothing to restore.
                    self.assertEqual(result["handlers_installed"], [])

    def test_reaped_child_final_output_is_drained(self):
        result = run_relay(self.root, "drain")
        self.assertIsNone(result["error"])
        self.assertEqual(result["code"], 0)
        self.assertFalse(result["unread"])
        self.assertIn(b"FINAL\x1b[?1049l\x1b[?25h", result["forwarded"])

    def test_resize_handler_is_restored_before_master_closes(self):
        result = run_relay(self.root, "resize-close")
        self.assertIsNone(result["error"])
        self.assertTrue(result["server_closed"] and result["master_closed"] and result["logs_closed"])
        self.assertTrue(result["winch_restored_at_master_close"])
        self.assertTrue(result["handlers_restored"])

    def test_every_close_runs_and_stop_handlers_are_restored_when_a_close_raises(self):
        # The two-close stage pins that the first error is the one raised once every close has run.
        for stage, message in (("master-close-failure", "master close refused"),
                               ("server-close-failure", "server close refused"),
                               ("two-close-failure", "master close refused")):
            with self.subTest(stage=stage):
                case = self.root / stage
                case.mkdir()
                result = run_relay(case, stage)
                self.assertEqual(str(result["error"]), message)
                self.assertTrue(result["master_closed"] and result["server_closed"])
                self.assertEqual(result["logs_opened"], 2)
                self.assertTrue(result["logs_closed"])
                self.assertEqual(result["clients_closed"], [True, True])
                self.assertEqual(result["handlers_installed"], ["SIGHUP", "SIGINT", "SIGTERM", "SIGWINCH"])
                self.assertTrue(result["restored"])
                self.assertTrue(result["handlers_restored"])

    def test_stop_signals_stay_routed_until_the_child_is_reaped(self):
        case = self.root / "teardown"
        case.mkdir()
        result = run_relay(case, "teardown")
        self.assertIsNone(result["error"])
        self.assertTrue(result["killed"])
        # A stop signal during the reap wait must reach stop(), not the default action that ends the relay.
        self.assertTrue(result["stop_routed_during_teardown"])
        self.assertTrue(result["restored"])
        self.assertTrue(result["handlers_restored"])


if __name__ == "__main__":
    unittest.main()
