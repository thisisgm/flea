"""Full-CLI regressions for the native architecture reviews."""
import json
import os
import subprocess
import unittest

from gpui_lint_tests import LintTests, LIST, REPO


CASES = {
    "executor_expression": [
        ("thread_helper", 'std::thread::spawn(move || worker());', "", 'fn worker() { replies.recv(); }', 0),
        ("named_thread_helper", 'std::thread::Builder::new().name("worker".into()).spawn(move || worker());', "", 'fn worker() { replies.recv(); stream.write_all(bytes); std::thread::sleep(delay); }', 0),
        ("thread_alias", 'threads::Builder::new().name("worker".into()).spawn(move || worker());', "use std::thread as threads;", 'fn worker() { replies.recv(); }', 0),
        ("background_helper", 'cx.background_executor().spawn(async move || worker());', "", 'fn worker() { std::fs::read("x"); }', 0),
        ("foreground_helper", 'cx.spawn(move || worker());', "", 'fn worker() { replies.recv(); }', 1),
        ("ordinary_spawn", 'executor.spawn(move || worker());', "", 'fn worker() { replies.recv(); }', 1),
        ("direct_helper", 'worker();', "", 'fn worker() { replies.recv(); }', 1),
        ("helper_on_both_threads", 'std::thread::spawn(move || worker()); worker();', "", 'fn worker() { replies.recv(); }', 1),
        ("argument_is_foreground", 'std::thread::Builder::new().name(std::fs::read_to_string("x").unwrap()).spawn(move || worker());', "", 'fn worker() { replies.recv(); }', 1),
        ("argument_factory_is_foreground", 'std::thread::spawn(make(worker()));', "", 'fn worker() { replies.recv(); }', 1),
    ],
    "color_forms": [
        ("return_type", 'theme.background;', "", 'fn ink() -> gpui::Hsla { theme.foreground.opacity(0.08) }', 0),
        ("reference_return_type", 'theme.background;', "", 'fn ink() -> &gpui::Hsla { log(0.08); theme.foreground }', 0),
        ("typed_binding", 'let ink: gpui::Hsla = theme.background; paint(ink);', "", "", 0),
        ("appearance_variant", 'gpui::WindowBackgroundAppearance::Transparent;', "", "", 0),
        ("named_functions", 'gpui::black(); gpui::white();', "", "", 1),
        ("transparent_function", 'gpui::transparent_black();', "", "", 0),
        ("color_constant", 'Color::RED;', "", "", 1),
        ("associated_constant", 'let ink = gpui::Hsla::BLACK;', "", "", 1),
        ("function_value", 'items.map(gpui::black);', "", "", 1),
        ("associated_constructor", 'gpui::Hsla::new(0.5, 0., 0., 1.);', "", "", 1),
        ("non_colour_variant", 'gpui::FontWeight::Black;', "", "", 0),
        ("rgba_struct", 'gpui::Rgba { r: 0.5, g: 0., b: 0., a: 1. };', "", "", 1),
        ("hsla_struct", 'gpui::Hsla { h: 0.5, s: 0., l: 0., a: 1. };', "", "", 1),
        ("closure_struct", 'let ink = |l| gpui::Hsla { h: 0.5, s: 0., l, a: 1. };', "", "", 1),
        ("return_literal", 'theme.background;', "", 'fn ink() -> gpui::Hsla { gpui::Hsla { h: 0.5, s: 0., l: 0., a: 1. } }', 1),
    ],
    "executor_receiver": [
        ("foreground", 'cx.spawn(async move |_| { std::fs::read("x"); });', "", "", 1),
        ("background", 'cx.background_executor().spawn(async move { std::fs::read("x"); });', "", "", 0),
        ("named_thread", 'std::thread::Builder::new().name("worker".into()).spawn(move || { std::fs::read("x"); });', "", "", 0),
        ("builder_argument", 'std::thread::Builder::new().name(std::fs::read_to_string("x").unwrap()).spawn(move || {});', "", "", 1),
        ("sibling", 'let jobs = (cx.background_executor(), cx.spawn(async move |_| { std::fs::read("x"); }));', "", "", 1),
        ("thread_sibling", 'let jobs = (std::thread::Builder::new(), cx.spawn(async move |_| { std::fs::read("x"); }));', "", "", 1),
        ("argument", 'cx.background_executor().spawn({ std::fs::read("x"); async move {} });', "", "", 1),
        ("nested_argument", 'cx.background_executor().spawn(make(async move { std::fs::read("x"); }));', "", "", 1),
    ],
    "import_scope": [
        ("outer", 'disk::read("x");', "use std::fs as disk;", "", 1),
        ("test_shadow", 'disk::read("x");', "use std::fs as disk;", "#[cfg(test)] mod tests { use std::path as disk; }", 1),
        ("block_shadow", '{ use std::path as disk; disk::Path::new("x"); } disk::read("x");', "use std::fs as disk;", "", 1),
        ("block_import", '{ use std::fs as disk; disk::read("x"); }', "", "", 1),
        ("local_path", '{ use std::path as disk; disk::Path::new("x"); }', "use std::fs as disk;", "", 0),
        ("module_import", "theme.background;", "", 'mod production { use std::fs as disk; fn paint() { disk::read("x"); } } mod sibling { use std::path as disk; }', 1),
        ("parent_import", "theme.background;", "use std::fs as disk;", 'mod production { use super::disk; fn paint() { disk::read("x"); } }', 1),
        ("parent_glob", "theme.background;", "use std::fs as disk;", 'mod production { use super::*; fn paint() { disk::read("x"); } }', 1),
        ("sibling_glob_isolation", "theme.background;", "use std::fs as disk;", 'mod sibling { use std::path as disk; } mod production { use super::*; fn paint() { disk::read("x"); } }', 1),
    ],
    "color_bindings": [
        ("literal", "gpui::rgb(0xff1122);", "", "", 1),
        ("local_value", "let ink = 0xff1122; gpui::rgb(ink);", "", "", 1),
        ("local_callable", "let make = gpui::rgb; make(0xff1122);", "", "", 1),
        ("alias_chain", "let first = 0xff1122; let ink = first; let make = gpui::rgb; let again = make; again(ink);", "", "", 1),
        ("parenthesized_callable", "let make = (gpui::rgb); make(0xff1122);", "", "", 1),
        ("assigned_value", "let mut ink = theme.background; ink = 0xff1122; gpui::rgb(ink);", "", "", 1),
        ("shadow_restore", "let ink = 0xff1122; { let ink = theme.background; paint(ink); } gpui::rgb(ink);", "", "", 1),
        ("theme", "let ink = theme.background; let make = gpui::rgb; make(ink);", "", "", 0),
        ("unused_literal", "let ink = 0xff1122; { let ink = theme.background; gpui::rgb(ink); }", "", "", 1),
        ("theme_shadow", "let ink = theme.raw; { let ink = theme.background; gpui::rgb(ink); }", "", "", 0),
    ],
    "color_lexical": [
        ("nested", "let mut ink = theme.raw; { ink = 0xff1122; } gpui::rgb(ink);", "", "", 1),
        ("conditional", "let mut ink = theme.raw; if enabled { ink = 0xff1122; } gpui::rgb(ink);", "", "", 1),
        ("loop", "let mut ink = theme.raw; for _ in rows { ink = 0xff1122; } gpui::rgb(ink);", "", "", 1),
        ("match_arm", "let mut ink = theme.raw; match state { Ready => { ink = 0xff1122; }, _ => {} } gpui::rgb(ink);", "", "", 1),
        ("tuple", "let (ink,) = (0xff1122,); gpui::rgb(ink);", "", "", 1),
        ("eight_digits", "let unused = 0xff112233;", "", "", 1),
        ("small_constructor_argument", "gpui::rgb(1); gpui::rgba(2); gpui::hsla(0., 0., 0., 1.);", "", "", 1),
        ("conditional_theme", "let mut ink = theme.raw; if enabled { ink = theme.other; } gpui::rgb(ink);", "", "", 0),
        ("loop_theme", "let mut ink = theme.raw; for value in theme.values { ink = value; } gpui::rgb(ink);", "", "", 0),
        ("match_theme", "let ink = match state { Ready => theme.raw, _ => theme.other }; gpui::rgb(ink);", "", "", 0),
        ("other_widths", "let short = 0x12345; let long = 0x1234567; theme.background;", "", "", 0),
        ("opaque_hex", 'let note = r#"0xff1122 0xff112233 gpui::rgb(1)"#; // 0xff1122\ntheme.background;', "", "", 0),
    ],
    "qualified_blocking": [
        ("method_join", "worker.join();", "", "", 1),
        ("qualified_join", "std::thread::JoinHandle::join(worker);", "", "", 1),
        ("aliased_join", "Worker::join(worker);", "use std::thread::JoinHandle as Worker;", "", 1),
        ("generic_join", "std::thread::JoinHandle::<()>::join(worker);", "", "", 1),
        ("type_join", "<std::thread::JoinHandle<()>>::join(worker);", "", "", 1),
        ("type_aliased_join", "<Worker<()>>::join(worker);", "use std::thread::JoinHandle as Worker;", "", 1),
        ("method_recv", "replies.recv();", "", "", 1),
        ("qualified_recv", "std::sync::mpsc::Receiver::recv(replies);", "", "", 1),
        ("aliased_recv", "Replies::recv(replies);", "use std::sync::mpsc::Receiver as Replies;", "", 1),
        ("generic_recv", "std::sync::mpsc::Receiver::<Message>::recv(replies);", "", "", 1),
        ("type_recv", "<std::sync::mpsc::Receiver<Message>>::recv(replies);", "", "", 1),
        ("path_join", 'std::path::Path::join(path, "child");', "", "", 0),
        ("type_path_join", '<std::path::Path>::join(path, "child");', "", "", 0),
        ("async_recv", "replies.recv().await; async_channel::Receiver::recv(replies).await;", "", "", 0),
    ],
}


def probe(case, body, prefix, suffix):
    case.write("crates/flea-gui/src/list.rs", prefix + LIST.replace("theme.background;", body) + suffix)
    case.write("tools/gpui-lint.json", json.dumps(case.policy))
    case.write("metadata.json", json.dumps(case.metadata))
    case.write("bin/cargo", '''#!/usr/bin/env python3
from pathlib import Path
import sys
assert sys.argv[1:] == ["metadata", "--locked", "--format-version", "1"]
print(Path("metadata.json").read_text())
''')
    (case.root / "bin/cargo").chmod(0o755)
    env = dict(os.environ, PATH=str(case.root / "bin") + os.pathsep + os.environ["PATH"],
               GIT_CEILING_DIRECTORIES=str(case.root.parent), PYTHONDONTWRITEBYTECODE="1")
    return subprocess.run([str(REPO / "tools/flea-gpui-lint")], cwd=case.root,
                          env=env, capture_output=True, text=True)


class ReviewTests(unittest.TestCase):
    def check_cases(self, group):
        for name, body, prefix, suffix, expected in CASES[group]:
            with self.subTest(case=name):
                case = LintTests("test_blocking_primitives_and_nonblocking_controls")
                case.setUp()
                result = probe(case, body, prefix, suffix)
                print(f"REVIEW {group}/{name}: exit={result.returncode} expected={expected}", flush=True)
                self.assertEqual(expected, result.returncode, result.stdout + result.stderr)
                if expected:
                    message = "literal colour" if group.startswith("color_") else "UI blocking call"
                    self.assertIn(message, result.stdout)

    def test_executor_receiver(self):
        self.check_cases("executor_receiver")

    def test_executor_expression(self):
        self.check_cases("executor_expression")

    def test_color_forms(self):
        self.check_cases("color_forms")

    def test_import_scope(self):
        self.check_cases("import_scope")

    def test_color_bindings(self):
        self.check_cases("color_bindings")

    def test_color_lexical(self):
        self.check_cases("color_lexical")

    def test_color_hex_policy(self):
        entry = {"symbol": "flea_gui::list::WIRE_MASK", "literal": "0xffffff",
                 "reason": "Fixture bit mask for a packed protocol field, not a colour."}
        declaration = "const WIRE_MASK: u32 = 0xffffff;"
        cases = [
            ("named_mask", "theme.background;", declaration, [entry], 0),
            ("unlisted_mask", "theme.background;", declaration, [], 1),
            ("wrong_name", "theme.background;", "const OTHER: u32 = 0xffffff;", [entry], 1),
            ("wrong_value", "theme.background;", "const WIRE_MASK: u32 = 0xeeeeee;", [entry], 1),
            ("local_not_constant", "let WIRE_MASK = 0xffffff;", "", [entry], 1),
            ("constructor_still_blocked", "gpui::rgb(0xffffff);", declaration, [entry], 1),
            ("reason_required", "theme.background;", declaration, [dict(entry, reason="")], 2),
            ("wildcard_rejected", "theme.background;", declaration, [dict(entry, symbol="flea_gui::*")], 2),
        ]
        for name, body, prefix, entries, expected in cases:
            with self.subTest(case=name):
                case = LintTests("test_blocking_primitives_and_nonblocking_controls")
                case.setUp()
                case.policy["non_color_hex_constants"] = entries
                result = probe(case, body, prefix, "")
                print(f"REVIEW color_hex_policy/{name}: exit={result.returncode} expected={expected}", flush=True)
                self.assertEqual(expected, result.returncode, result.stdout + result.stderr)
                if expected == 2:
                    self.assertIn("non-colour hex exceptions need", result.stdout)

    def test_color_module_controls(self):
        for name, palette in [("palette_exempt", True), ("non_surface_hex", False)]:
            with self.subTest(case=name):
                case = LintTests("test_blocking_primitives_and_nonblocking_controls")
                case.setUp()
                body = "fn parse() { gpui::rgb(0xff1122); }" if palette else "const MASK: u32 = 0xffffff;"
                case.write("crates/flea-gui/src/palette.rs", body)
                if palette:
                    case.policy["color_modules"] = [{"module": "flea_gui::palette", "kind": "palette",
                                                     "reason": "Fixture owns conversion of palette input to theme roles."}]
                result = probe(case, "theme.background;", "", "")
                print(f"REVIEW color_module_controls/{name}: exit={result.returncode} expected=0", flush=True)
                self.assertEqual(0, result.returncode, result.stdout + result.stderr)

    def test_qualified_blocking(self):
        self.check_cases("qualified_blocking")


if __name__ == "__main__":
    unittest.main(defaultTest="ReviewTests", verbosity=2)
