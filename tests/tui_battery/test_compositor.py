"""Pins the real tool name that recorded native commands carry."""
import json
import unittest

from .compositor import TOOL, command_matches, keymaps


class CompositorIdentityTests(unittest.TestCase):
    def test_tool_is_the_installed_hyprctl(self):
        self.assertEqual(TOOL, "hyprctl")

    def test_switch_matches_only_the_real_tool(self):
        switch = {"argv": ["/usr/bin/hyprctl", "switchxkblayout", "current", "1"]}
        self.assertTrue(command_matches(switch, "switchxkblayout"))
        self.assertFalse(command_matches(dict(switch, argv=["/usr/bin/true", "switchxkblayout", "current", "1"]), "switchxkblayout"))

    def test_keymaps_read_a_recorded_devices_reply(self):
        reply = {"argv": ["/usr/bin/hyprctl", "-j", "devices"], "exit_code": 0,
                 "stdout": json.dumps({"keyboards": [{"name": "kb", "active_keymap": "Russian"}]})}
        self.assertEqual(keymaps(reply), {"kb": "Russian"})
        self.assertIsNone(keymaps(dict(reply, exit_code=1)))
        self.assertIsNone(keymaps(dict(reply, argv=["/usr/bin/true", "-j", "devices"])))


if __name__ == "__main__":
    unittest.main()
