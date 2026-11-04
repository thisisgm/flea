"""Exact cursor identity and the settle path's stale-frame negative control."""
import inspect
import unittest
from unittest.mock import patch

from . import scenarios
from .cells import Screen


class ScenarioTests(unittest.TestCase):
    def test_cursor_requires_exact_filename_in_name_field(self):
        for actual, wanted, accepted in (("todo.md", "todo.md", True), ("todo.md.bak", "todo.md", False),
                                         ("old-todo.md", "todo.md", False), ("todo.md backup", "todo.md", False),
                                         ("議事録.md", "議事録.md", True)):
            with self.subTest(actual=actual, wanted=wanted):
                screen = Screen(80, 7)
                screen.feed(("\x1b[H\x1b[3;1H│▎x " + actual.ljust(30) + "   99 B  │").encode())
                self.assertEqual(scenarios.highlighted(screen.snapshot(), wanted), accepted)
        screen = Screen(80, 7)
        screen.feed("\x1b[H\x1b[3;1H│▎x other.txt               todo.md  │".encode())
        self.assertFalse(scenarios.highlighted(screen.snapshot(), "todo.md"))

    def test_settle_budgets_are_named_not_literal(self):
        for name in ("SETTLE_STABLE_SECONDS", "SETTLE_REFUSAL_SECONDS", "LISTING_PREFIX_CELLS"):
            with self.subTest(name=name):
                self.assertTrue(hasattr(scenarios, name))
        self.assertIn("SETTLE_STABLE_SECONDS", inspect.getsource(scenarios.settled))
        self.assertIn("SETTLE_REFUSAL_SECONDS", inspect.getsource(scenarios.negatives))
        self.assertIn("LISTING_PREFIX_CELLS", inspect.getsource(scenarios.listing_name))

    def test_pty_log_parser_carries_sample_input(self):
        lines = inspect.getsource(scenarios.legacy_shift_delete).splitlines()
        index = next(i for i, line in enumerate(lines) if "row = json.loads(line)" in line)
        self.assertIn("Sample input:", lines[index - 1])

    def test_stale_snapshot_cannot_satisfy_settle_negative(self):
        screen = Screen(20, 4)
        screen.feed(b"\x1b[HREADY")
        with patch.object(scenarios, "settled", return_value=screen.snapshot()):
            with self.assertRaisesRegex(AssertionError, "stale frame bypass"):
                scenarios.negatives()
