"""Offline interleavings at the Hyprland dispatch and keyboard boundary, modelled on the 0.56 Lua config."""

import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import unittest

REPO = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("wayland_safety", REPO / "tools/flea-bench-thumbs-lib/wayland.py")
wayland = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wayland)
ROOT = Path(os.environ["FLEA_B7_TEST_ROOT"])
FOCUS = 'hl.dsp.focus({ window = "address:0xb700" })'
CLOSE = 'hl.dsp.window.close({ window = "address:0xb700" })'


class Dispatch(unittest.TestCase):
    def simulate(self, *, active=None, steal_before_wtype=False, closes_on_wtype=False, answer=None, xwayland=False):
        owned = dict(address="0xb700", pid=123, xwayland=xwayland)
        foreign = dict(address="0xdead", pid=456, xwayland=False)
        state = dict(active=active or owned, clients=[owned], delivered=[], argv=[])
        display = wayland.Display(ROOT)

        def command(arguments):
            state["argv"].append(arguments)
            if arguments == ["hyprctl", "-j", "activewindow"]:
                return json.dumps(state["active"])
            if arguments == ["hyprctl", "-j", "clients"]:
                return json.dumps(state["clients"])
            if arguments[:2] == ["hyprctl", "dispatch"]:
                if len(arguments) != 3:
                    raise subprocess.CalledProcessError(7, arguments, "", "')' expected near 'focuswindow'")
                if arguments[2] == CLOSE:
                    state["closed"] = state.get("closed", 0) + 1
                    return "ok"
                if arguments[2] != FOCUS:
                    raise AssertionError(arguments)
                if answer:
                    return answer
                state["active"] = owned
                return "ok"
            if arguments[0] == "wtype":
                if closes_on_wtype:
                    state["delivered"].append(state["active"]["address"])
                    state["clients"], state["active"] = [], {}
                    return ""
                if steal_before_wtype:
                    state["active"] = foreign
                state["delivered"].append(state["active"]["address"])
                return ""
            raise AssertionError(arguments)

        display.command = command
        display.owned = lambda client, token: client.get("address") == owned["address"] and client.get("pid") == owned["pid"] and not xwayland
        return display, owned, state

    def test_focus_uses_the_lua_form_and_the_legacy_form_is_rejected(self):
        display, owned, state = self.simulate(active=dict(address="0xdead", pid=456, xwayland=False))
        display.focus(owned, "owned-token")
        self.assertIn(["hyprctl", "dispatch", FOCUS], state["argv"])
        with self.assertRaises(subprocess.CalledProcessError) as caught:
            display.command(["hyprctl", "dispatch", "focuswindow", "address:0xb700"])
        self.assertEqual(caught.exception.returncode, 7)

    def test_chords_go_through_wtype_to_the_verified_focused_window(self):
        display, owned, state = self.simulate()
        display.key(owned, "owned-token", "ctrl+3", "Right", "ctrl+shift+q")
        wtypes = [argv for argv in state["argv"] if argv[0] == "wtype"]
        self.assertEqual(wtypes, [["wtype", "-M", "ctrl", "-k", "3", "-m", "ctrl"], ["wtype", "-k", "Right"],
                                  ["wtype", "-M", "ctrl", "-M", "shift", "-k", "q", "-m", "shift", "-m", "ctrl"]])
        self.assertEqual(state["delivered"], ["0xb700"] * 3)
        self.assertFalse(any(argv[:2] == ["hyprctl", "dispatch"] for argv in state["argv"]))

    def test_focus_change_between_check_and_delivery_fails_the_sample(self):
        display, owned, state = self.simulate(steal_before_wtype=True)
        with self.assertRaisesRegex(ValueError, "may have reached another window"):
            display.key(owned, "owned-token", "ctrl+q")
        self.assertEqual(len(state["delivered"]), 1)

    def test_a_quit_chord_that_closes_the_owned_window_is_not_a_focus_failure(self):
        display, owned, state = self.simulate(closes_on_wtype=True)
        display.key(owned, "owned-token", "ctrl+q")
        self.assertEqual(state["delivered"], ["0xb700"])
        with self.assertRaises(ValueError):
            display.key(owned, "owned-token", "ctrl+q")
        self.assertEqual(state["delivered"], ["0xb700"])

    def test_close_goes_to_the_verified_owned_address_in_the_lua_form(self):
        display, owned, state = self.simulate()
        display.close(owned, "owned-token")
        self.assertEqual(state["closed"], 1)
        self.assertIn(["hyprctl", "dispatch", CLOSE], state["argv"])

    def test_close_is_refused_for_a_reused_address_and_an_xwayland_window(self):
        display, owned, state = self.simulate(xwayland=True)
        with self.assertRaises(ValueError):
            display.close(owned, "owned-token")
        display, owned, state = self.simulate(active=dict(address="0xb700", pid=456, xwayland=False))
        state["clients"] = [dict(address="0xb700", pid=456, xwayland=False)]
        with self.assertRaises(ValueError):
            display.close(owned, "owned-token")
        self.assertEqual(state.get("closed", 0), 0)

    def test_a_foreign_active_window_receives_nothing(self):
        display, owned, state = self.simulate(active=dict(address="0xdead", pid=456, xwayland=False))
        with self.assertRaises(ValueError):
            display.key(owned, "owned-token", "ctrl+q")
        self.assertEqual(state["delivered"], [])

    def test_same_sample_replacement_pid_receives_no_input(self):
        display, owned, state = self.simulate()
        state["active"] = dict(owned, pid=456)
        display.owned = lambda client, token: client.get("pid") in (123, 456)
        with self.assertRaises(ValueError):
            display.key(owned, "owned-token", "ctrl+q")
        self.assertEqual(state["delivered"], [])

    def test_a_focus_miss_answered_with_exit_zero_is_refused(self):
        display, owned, state = self.simulate(active=dict(address="0xdead", pid=456, xwayland=False),
                                              answer="warning: =[C]:-1: hl.focus: window not found")
        with self.assertRaisesRegex(ValueError, "did not accept"):
            display.focus(owned, "owned-token")
        self.assertEqual(state["delivered"], [])

    def test_xwayland_window_is_refused(self):
        display, owned, state = self.simulate(xwayland=True)
        with self.assertRaises(ValueError):
            display.key(owned, "owned-token", "ctrl+q")
        with self.assertRaises(ValueError):
            display.focus(owned, "owned-token")
        self.assertEqual(state["delivered"], [])
        self.assertFalse(any(argv[:2] == ["hyprctl", "dispatch"] for argv in state["argv"]))

    def test_reused_window_address_cannot_receive_a_chord_or_a_focus(self):
        display, owned, state = self.simulate(active=dict(address="0xb700", pid=456, xwayland=False))
        state["clients"] = [dict(address="0xb700", pid=456, xwayland=False)]
        with self.assertRaises(ValueError):
            display.key(owned, "owned-token", "ctrl+q")
        with self.assertRaises(ValueError):
            display.focus(owned, "owned-token")
        self.assertEqual(state["delivered"], [])
        self.assertFalse(any(argv[:2] == ["hyprctl", "dispatch"] for argv in state["argv"]))

    def test_unsupported_chords_never_reach_the_keyboard(self):
        display, owned, state = self.simulate()
        for chord in ("super+Return", "ctrl+shift", "ctrl+a b", "meta+q"):
            with self.assertRaises(ValueError):
                display.key(owned, "owned-token", chord)
        self.assertEqual(state["delivered"], [])


class Sweep(unittest.TestCase):
    def test_no_benchmark_tool_uses_a_legacy_hyprland_dispatcher_or_keyword(self):
        legacy = re.compile(r"\b(focuswindow|sendshortcut|tagwindow|movewindow|resizewindow|resizewindowpixel|setfloating|togglefloating|movecursor|centerwindow|movefocus|workspace)\b")
        files = [*REPO.glob("tools/flea-bench-thumbs*"), *REPO.glob("tools/flea-bench-thumbs-lib/*.py"), REPO / "tools/flea-bench-keys", REPO / "tools/flea-field-bench"]
        scanned = 0
        for path in files:
            if not path.is_file():
                continue
            scanned += 1
            for number, line in enumerate(path.read_text().splitlines(), 1):
                if path.suffix == ".md" or "hyprctl" not in line and "dispatch" not in line and "keyword" not in line:
                    continue
                self.assertIsNone(re.search(r"hyprctl\s+keyword", line), f"{path}:{number}")
                if re.search(r"hyprctl.*dispatch|\"dispatch\"|'dispatch'", line):
                    self.assertIsNone(legacy.search(line), f"{path}:{number}: {line.strip()}")
        self.assertGreaterEqual(scanned, 8)


class Discovery(unittest.TestCase):
    def sleeper(self, **options):
        process = subprocess.Popen(["sleep", "30"], **options)
        self.addCleanup(process.wait)
        self.addCleanup(process.kill)
        return process

    class Launched:
        pid = os.getpid()

        def poll(self):
            return None

    def display(self, clients):
        display = wayland.Display(ROOT)
        display.argv = []

        def command(arguments):
            display.argv.append(arguments)
            if arguments == ["hyprctl", "-j", "clients"]:
                return json.dumps(clients)
            raise AssertionError(arguments)

        display.command = command
        return display

    def test_a_window_with_a_previous_entrants_address_is_never_dispatched_to(self):
        stale = dict(address="0x564ba3c0d8f0", pid=os.getpid(), xwayland=False, **{"class": "org.gnome.Nautilus"})
        display = self.display([stale])
        with self.assertRaises(ValueError) as caught:
            display.wait_window("fresh-token", self.Launched(), 0.3)
        message = str(caught.exception)
        self.assertIn("no owned entrant window mapped", message)
        self.assertIn("0x564ba3c0d8f0/pid=%d/class=org.gnome.Nautilus/owned=False" % os.getpid(), message)
        self.assertFalse(any(argv[:2] == ["hyprctl", "dispatch"] for argv in display.argv))

    def test_the_marker_alone_does_not_own_a_window_outside_the_launched_tree(self):
        token = "tree-token"
        child = self.sleeper(env={**os.environ, "FLEA_B7_BENCH": token})
        client = dict(address="0xb701", pid=child.pid, xwayland=False)
        display = wayland.Display(ROOT)
        display.roots[token] = os.getpid()
        self.assertTrue(display.owned(client, token))
        sibling = self.sleeper()
        display.roots[token] = sibling.pid
        self.assertFalse(display.owned(client, token))

    def test_descends_from_follows_the_host_parent_chain(self):
        child = self.sleeper()
        self.assertTrue(wayland.descends_from(child.pid, os.getpid()))
        self.assertTrue(wayland.descends_from(child.pid, child.pid))
        self.assertFalse(wayland.descends_from(os.getpid(), child.pid))
        self.assertFalse(wayland.descends_from(0, os.getpid()))

    def test_xwayland_is_rejected_by_production_ownership(self):
        token = "native-only-token"
        child = self.sleeper(env={**os.environ, "FLEA_B7_BENCH": token})
        client = dict(address="0xb702", pid=child.pid, xwayland=False)
        display = wayland.Display(ROOT)
        display.roots[token] = os.getpid()
        self.assertTrue(display.owned(client, token))
        client["xwayland"] = True
        self.assertFalse(display.owned(client, token))


if __name__ == "__main__":
    result = unittest.main(exit=False)
    print(f"bench-thumbs-safety: {result.result.testsRun} offline contracts, {len(result.result.failures) + len(result.result.errors)} failed")
    raise SystemExit(not result.result.wasSuccessful())
