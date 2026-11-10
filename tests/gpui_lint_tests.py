"""Mutants and controls for each native lint rule, using marked fixture roots."""
import ast
import copy
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))
from gpui_lint.cargo import native_sources, source_checks, tracked
from gpui_lint.checks import source_checks as rust_checks
from gpui_lint.lex import tokens

POLICY_CHECKS = runpy.run_path(str(REPO / "tools/flea-gpui-lint"))["policy_checks"]
FORK = {"url": "https://github.com/thisisgm/gpui-omarchy", "rev": "a" * 40}
SOURCE = f"git+{FORK['url']}?rev={FORK['rev']}#{FORK['rev']}"
LIST = """const ROW_HEIGHT: f32 = 31.;
struct ListElement;
impl Element for ListElement {
    fn request_layout() {}
    fn prepaint() {}
    fn paint() { theme.background; }
}
"""


class LintTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ["FLEA_LINT_TEST_ROOT"])
        assert parent.is_absolute() and (parent / ".flea-test-sandbox").is_file()
        self.root = Path(tempfile.mkdtemp(prefix="case-", dir=parent))
        (self.root / ".flea-test-sandbox").touch()
        self.policy = {"fork": FORK, "ui_callbacks": [], "blocking_helpers": [],
                       "blocking_exceptions": [], "color_modules": [], "hot_surfaces": [
                           {"name": "list", "path": "crates/flea-gui/src/list.rs", "element": "ListElement", "row_height": "ROW_HEIGHT"},
                           *({"name": name, "path": f"crates/flea-gui/src/{name}.rs", "pending": True}
                             for name in ("columns", "grid", "long_menus"))]}
        self.write("Cargo.toml", f'''[workspace]
members = ["crates/flea-gui"]
[workspace.dependencies]
gpui = {{ package = "gpui-pre", git = "{FORK['url']}", rev = "{FORK['rev']}" }}
''')
        self.write("Cargo.lock", f'''version = 4
[[package]]
name = "gpui-pre"
version = "0.3.9"
source = "{SOURCE}"
''')
        self.write("crates/flea-gui/Cargo.toml", '[package]\nname = "flea-gui"\nversion = "0.4.0"\n[dependencies]\ngpui.workspace = true\n')
        self.write("crates/flea-gui/src/list.rs", LIST)
        self.metadata = {"workspace_root": str(self.root), "workspace_members": ["native"],
                         "resolve": {"nodes": [{"id": "native"}]}, "packages": [
                             {"id": "native", "name": "flea-gui", "version": "0.4.0", "source": None,
                              "manifest_path": str(self.root / "crates/flea-gui/Cargo.toml"),
                              "targets": [{"kind": ["lib"], "src_path": str(self.root / "crates/flea-gui/src/lib.rs")}]},
                             {"id": "gpui", "name": "gpui-pre", "version": "0.3.9", "source": SOURCE}]}

    def write(self, path, text):
        dest = self.root / path
        assert dest.is_relative_to(self.root)
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text)

    def paths(self):
        return [p.relative_to(self.root) for p in self.root.rglob("*") if p.is_file()]

    def errors(self):
        units = native_sources(self.root, self.paths(), self.metadata)
        return rust_checks(units, self.policy)

    def check_body(self, code):
        self.write("crates/flea-gui/src/list.rs", LIST.replace("theme.background;", code))
        return self.errors()

    def test_ci_green4_port_methods_do_not_reach_unrelated_free_functions(self):
        self.write("crates/flea-gui/src/worker.rs", '''
fn collect() { std::thread::sleep(delay); }
fn read() { std::fs::read("x"); }
fn store() { std::fs::write("x", bytes); }
''')
        self.assertEqual(self.check_body('items.collect(); entity.read(cx); flag.store(value);'), [])
        self.assertTrue(self.check_body('crate::worker::collect();'))
        self.assertTrue(self.check_body('crate::worker::read();'))
        self.assertTrue(self.check_body('crate::worker::store();'))

    def test_ci_green4_port_receiver_methods_and_local_helpers_still_block(self):
        self.write("crates/flea-gui/src/worker.rs", '''
struct Reader;
impl Reader { fn read(&self) { std::fs::read("x"); } }
''')
        self.assertTrue(self.check_body('reader.read(cx);'))
        self.write("crates/flea-gui/src/list.rs", LIST.replace("theme.background;", 'read();')
                   + '\nfn read() { std::fs::read("x"); }\n')
        self.assertTrue(self.errors())

    def test_ci_green4_port_boolean_background_is_not_a_colour_setter(self):
        self.assertEqual(self.check_body('click(if background { 300. } else { 45. });'), [])
        self.assertTrue(self.check_body('div().background(300.);'))
        self.assertTrue(self.check_body('if background { gpui::rgb(1); }'))

    def test_ci_green4_port_exact_method_exception_survives_resolution(self):
        self.write("crates/flea-gui/src/worker.rs", 'impl Owner { fn output(&self) {} }')
        self.assertTrue(self.check_body('owner.output();'))
        self.policy["blocking_exceptions"] = [{"caller": "flea_gui::list::ListElement::paint",
                                               "symbol": "output", "reason": "Fixture's non-I/O method."}]
        self.assertEqual(self.check_body('owner.output();'), [])
        self.policy["blocking_exceptions"][0]["caller"] = "flea_gui::other::paint"
        self.assertTrue(self.check_body('owner.output();'))

    def test_comments_strings_chars_and_raw_identifiers(self):
        code = '''// std::fs::read("x"); div(); rgb(0xff); }
        /* outer /* std::thread::sleep(1); */ red() */
        let text = r####"}; std::fs::read("x"); rgb(1)"####;
        let bytes = br#"div()"#; let c = cr#"red()"#;
        let escaped = "\\\"}; fs::read()"; let character = '}';
        let byte = b'\\x7d'; let unicode = '\\u{007d}'; let quote = '\\'';
        let lifetime: &'a str = "std::fs";
        theme.background;'''
        self.assertEqual([], self.check_body(code))
        self.assertTrue(any("blocking" in e for e in self.check_body('std::r#fs::read("x");')))
        for text in ('/* open', 'r##"open', '"open', 'fn paint() {'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                from gpui_lint.lex import pairs
                pairs(tokens(text))

    def test_blocking_primitives_and_nonblocking_controls(self):
        self.assertEqual([], self.check_body('theme.background; let p = path.join("child");'))
        self.assertEqual([], self.check_body('std::process::exit(2); replies.recv().await;'))
        self.assertEqual([], self.check_body('let first = theme.background; if let Some(value) = next { let later = value; }'))
        self.assertEqual([], self.check_body('let state = pane.read(cx); let held = view.read(self.cx);'))
        for code in ('std::fs::read("x");', 'std::net::TcpStream::connect("x");',
                     'std::thread::sleep(duration);', 'worker.join();', 'child.wait();',
                     'command.output();', 'stream.read_to_end(&mut bytes);', 'executor.block_on(future);',
                     'std::io::Read::read_to_string(stream, buffer);', 'reqwest::blocking::get("host");',
                     'stream.read(&mut buffer);', 'stream.read(cx_buffer);'):
            with self.subTest(code=code):
                self.assertTrue(any("blocking" in e for e in self.check_body(code)), code)

    def test_import_aliases_and_transitive_helpers(self):
        for use, code in (
            ('use std::fs as disk;', 'disk::read("x");'),
            ('use std::{fs::{self as disk, read as load}, thread::sleep as nap};', 'load("x");'),
            ('use std::thread as worker;', 'worker::sleep(duration);'),
            ('use std::fs::*;', 'read("x");'),
            ('extern crate std as native;', 'native::fs::read("x");'),
            ('use std::fs::File as Handle;', 'Handle::open("x");')):
            with self.subTest(use=use):
                self.write("crates/flea-gui/src/list.rs", use + LIST.replace("theme.background;", code))
                self.assertTrue(any("blocking" in e for e in self.errors()))
        self.write("crates/flea-gui/src/list.rs", LIST + 'fn helper() { std::fs::read("x"); }')
        self.assertEqual([], self.errors())
        self.write("crates/flea-gui/src/list.rs", LIST.replace("theme.background;", "helper();") + 'fn helper() { std::fs::read("x"); }')
        self.assertTrue(any("blocking" in e for e in self.errors()))
        self.assertTrue(any("blocking" in e for e in self.check_body('let load = std::fs::read; load("x");')))

    def test_process_spawn_with_command_alias_and_parameter(self):
        self.assertEqual([], self.check_body('std::process::Command::new("helper");'))
        for code in ('std::process::Command::new("helper").spawn();',
                     'let mut command = std::process::Command::new("helper"); command.spawn();'):
            self.assertTrue(any("blocking" in e for e in self.check_body(code)), code)
        self.write("crates/flea-gui/src/list.rs", 'use std::process::Command as Process;' + LIST + 'fn callback(command: &mut Process) { command.spawn(); }')
        self.policy["ui_callbacks"] = ["flea_gui::list::callback"]
        self.assertTrue(any("blocking" in e for e in self.errors()))
        self.policy["ui_callbacks"] = []
        self.write("crates/flea-gui/src/list.rs", LIST.replace("theme.background;", "command().spawn();") + 'fn command() -> std::process::Command { std::process::Command::new("helper") }')
        self.assertTrue(any("blocking" in e for e in self.errors()))

    def test_registered_callbacks_helpers_and_exact_exceptions(self):
        self.write("crates/flea-gui/src/callback.rs", 'fn update() { crate::slow::probe(); }')
        self.assertEqual([], self.errors())
        self.policy["ui_callbacks"] = ["flea_gui::callback::update"]
        self.policy["blocking_helpers"] = ["flea_gui::slow::probe"]
        self.assertTrue(any("blocking" in e for e in self.errors()))
        self.policy["blocking_exceptions"] = [{"caller": "flea_gui::callback::update", "symbol": "flea_gui::slow::probe", "reason": "Fixture-only probe of the exception contract."}]
        POLICY_CHECKS(self.policy)
        self.assertEqual([], self.errors())
        self.policy["blocking_exceptions"][0]["symbol"] = "flea_gui::slow::*"
        with self.assertRaises(ValueError):
            POLICY_CHECKS(self.policy)
        self.policy["blocking_exceptions"][0]["symbol"] = "flea_gui::slow::probe"
        self.policy["blocking_exceptions"][0]["reason"] = ""
        with self.assertRaises(ValueError):
            POLICY_CHECKS(self.policy)

    def test_a_method_on_a_field_reaches_its_declared_type_when_many_types_share_its_name(self):
        self.write("crates/flea-gui/src/air.rs", '''
pub struct Shared;
impl Shared { pub fn claim(&mut self) { std::fs::remove_file("x"); } }
pub struct Pids;
impl Pids { pub fn claim(&mut self) {} pub fn turn_on(&mut self) {} }
pub struct AirDrop { pub(crate) shared: Shared, pids: Option<Pids> }
pub struct Menu { pub providers: Providers }
pub struct Providers { pub airdrop: AirDrop }
pub struct Page { pub airdrop: bool }
impl AirDrop {
    pub fn turn_on(&mut self) { self.shared.claim(); }
    pub fn quiet(&mut self) { self.pids.claim(); }
}
fn update(shell: &mut Shell) { let air = &mut shell.menu.providers.airdrop; air.turn_on(); }
''')
        self.policy["ui_callbacks"] = ["flea_gui::air::AirDrop::turn_on"]
        self.assertTrue(any("Shared::claim" in e for e in self.errors()))
        self.policy["ui_callbacks"] = ["flea_gui::air::update"]
        self.assertTrue(any("Shared::claim" in e for e in self.errors()), "a local bound to a field chain keeps its type")
        self.policy["ui_callbacks"] = ["flea_gui::air::AirDrop::quiet"]
        self.assertEqual([], self.errors(), "a wrapped field type is not guessed")

    def test_a_method_exception_names_the_method_as_written_on_a_typed_field(self):
        self.write("crates/flea-gui/src/pane.rs", '''
pub struct Outside;
impl Outside { fn status(&self) {} }
pub struct Pane { outside: Outside }
impl Pane { fn pump(&mut self) { self.outside.status(); } }
''')
        self.policy["ui_callbacks"] = ["flea_gui::pane::Pane::pump"]
        self.assertTrue(any("blocking" in e for e in self.errors()))
        self.policy["blocking_exceptions"] = [{"caller": "flea_gui::pane::Pane::pump", "symbol": "status",
                                               "reason": "Fixture's queued status message."}]
        self.assertEqual([], self.errors())

    def test_a_field_type_that_declares_no_such_method_keeps_the_name_as_written(self):
        self.write("crates/flea-gui/src/nav.rs", '''
pub struct Store;
impl Store { pub fn load(&self) { std::fs::read("x"); } }
pub struct Navigator;
mod sort { impl super::Navigator { pub fn resort(&mut self) { std::fs::read("y"); } } }
pub struct Host { store: std::sync::Arc<Store>, nav: Navigator }
impl Host {
    fn open(&self) { self.store.load(); }
    fn settle(&mut self) { self.nav.resort(); }
}
''')
        self.policy["ui_callbacks"] = ["flea_gui::nav::Host::open"]
        self.assertTrue(any("Store::load" in e for e in self.errors()), "a wrapper's method is reached by its name")
        self.policy["ui_callbacks"] = ["flea_gui::nav::Host::settle"]
        self.assertTrue(any("resort" in e for e in self.errors()), "a method declared under a path is reached by its name")

    def test_background_boundaries_and_async_ui_mutant(self):
        for code in ('std::thread::spawn(move || { std::fs::read("x"); });',
                     'cx.background_executor().spawn(async move { std::fs::read("x"); });',
                     'cx.background_executor().spawn(async move || { std::fs::read("x"); });',
                     'std::thread::Builder::new().spawn(move || { std::fs::read("x"); });'):
            with self.subTest(code=code):
                self.assertEqual([], self.check_body(code))
        self.assertTrue(any("blocking" in e for e in self.check_body('cx.spawn(async move || { std::fs::read("x"); });')))

    def test_image_pixels_are_data_but_gpui_color_aliases_stay_rejected(self):
        for code in ('image::Rgba([200, 100, 50, 255]);',
                     'use image::Rgb as Pixel; Pixel([200, 100, 50]);',
                     'fn image() {} image::Rgba([200, 100, 50, 255]);'):
            with self.subTest(code=code):
                self.assertEqual([], self.check_body(code))
        for code in ('use gpui as image; image::Rgba { r: 1., g: 0., b: 0., a: 1. };',
                     'use gpui::Rgba as Pixel; Pixel { r: 1., g: 0., b: 0., a: 1. };',
                     'mod image { pub use gpui::Rgba; } image::Rgba { r: 1., g: 0., b: 0., a: 1. };'):
            with self.subTest(code=code):
                self.assertTrue(any("literal colour" in error for error in self.check_body(code)))

    def test_literal_colors_theme_control_and_module_exemptions(self):
        self.assertEqual([], self.check_body('paint(theme.background);'))
        for code in ('rgb(0xff1122);', 'rgba(1);', 'hsla(1., 0., 0., 1.);', 'Hsla { l: 0.5, ..theme.ink };',
                     'gpui::white();', 'Color::RED;', 'div.bg("rebeccapurple");', 'div.text_color("#fff");'):
            with self.subTest(code=code):
                self.assertTrue(any("literal colour" in e for e in self.check_body(code)), code)
        self.write("crates/flea-gui/src/palette.rs", "fn parse() { rgb(0xff1122); }")
        self.check_body("theme.background;")
        self.assertTrue(any("literal colour" in e for e in self.errors()))
        self.policy["color_modules"] = [{"module": "flea_gui::palette", "kind": "palette", "reason": "Parses external palette values into typed theme roles."}]
        POLICY_CHECKS(self.policy)
        self.assertEqual([], self.errors())
        self.assertTrue(any("literal colour" in e for e in self.check_body("rgb(0xff1122);")))
        self.write("crates/flea-gui/src/list.rs", 'const BASE: u32 = 0xff1122; const INK: u32 = BASE;' + LIST.replace("theme.background;", "rgb(INK);"))
        self.assertTrue(any("literal colour" in e for e in self.errors()))

    def test_hot_surface_child_tree_and_empty_scan(self):
        self.assertEqual([], self.errors())
        for code in ("for row in rows { div().child(row); }", "rows.map(|row| div().children(row));"):
            with self.subTest(code=code):
                self.assertTrue(any("child tree" in e for e in self.check_body(code)))
        self.write("crates/flea-gui/src/row.rs", "fn build() { div(); }")
        self.assertTrue(any("child tree" in e for e in self.check_body("crate::row::build();")))
        self.check_body("theme.background;")
        self.write("crates/flea-gui/src/columns.rs", "fn new() {}")
        self.assertTrue(any("pending surface exists" in e for e in self.errors()))
        self.policy["hot_surfaces"] = []
        self.assertTrue(any("empty" in e for e in self.errors()))
        self.assertTrue(any("empty" in e for e in rust_checks([], self.policy)))

    def test_ui_reach_stays_in_its_compilation_target(self):
        # A test crate's method named like an iterator method is not the paint path's .next().
        self.write("crates/flea-gui/tests/probe.rs", "mod transport;")
        self.write("crates/flea-gui/tests/probe/transport.rs", "impl IteratorProbe { fn next(&mut self) { replies.recv_blocking(); } }")
        self.assertEqual([], self.check_body("rows.iter().next();"))
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("replies.recv_blocking();")))
        # The same receiver method inside the product crate is still reached and flagged.
        (self.root / "crates/flea-gui/tests/probe/transport.rs").unlink()
        (self.root / "crates/flea-gui/tests/probe.rs").unlink()
        self.write("crates/flea-gui/src/row.rs", "impl IteratorProbe { fn next(&mut self) { replies.recv_blocking(); } }")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("rows.iter().next();")))
        self.write("crates/flea-gui/src/window.rs", "fn open() { replies.recv_blocking(); }")
        self.policy["ui_callbacks"] = ["flea_gui::window::open"]
        self.assertTrue(any("blocking call recv_blocking in flea_gui::window::open" in e for e in self.errors()))

    def test_ui_reach_follows_another_workspace_package(self):
        # A helper in another native package's library is reached through the paint path and flagged.
        self.write("crates/helper/Cargo.toml", '[package]\nname = "helper"\nversion = "0.4.0"\n')
        self.write("crates/helper/src/lib.rs", "pub impl IteratorProbe { fn next(&mut self) { replies.recv_blocking(); } }")
        self.metadata["workspace_members"].append("helper")
        self.metadata["packages"].append({"id": "helper", "name": "helper", "version": "0.4.0", "source": None,
                                          "manifest_path": str(self.root / "crates/helper/Cargo.toml"),
                                          "targets": [{"kind": ["lib"], "src_path": str(self.root / "crates/helper/src/lib.rs")}]})
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("rows.iter().next();")))
        # The same helper in that package's integration test crate or binaries is not.
        (self.root / "crates/helper/src/lib.rs").unlink()
        for path in ("crates/helper/tests/probe.rs", "crates/helper/src/main.rs", "crates/helper/src/bin/tool.rs",
                     "crates/helper/src/bin/tool/main.rs"):
            self.write(path, "impl IteratorProbe { fn next(&mut self) { replies.recv_blocking(); } }")
            self.assertEqual([], self.check_body("rows.iter().next();"), path)
            (self.root / path).unlink()
        # The lint treats one package's src/ files as one target (a path cannot tell a binary's module from the library's), so this reach is followed.
        self.write("crates/flea-gui/src/main.rs", "impl IteratorProbe { fn next(&mut self) { replies.recv_blocking(); } }")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("rows.iter().next();")))

    def test_test_only_items_join_no_ui_reach(self):
        # A #[cfg(test)] fn, impl, file module or #[path] module is never compiled into the paint path.
        self.write("crates/flea-gui/src/lib.rs", "mod list; mod probe;")
        blocking = "replies.recv_blocking();"
        self.write("crates/flea-gui/src/probe.rs", f"#[cfg(test)]\npub(super) impl ShapeProbe {{ fn shape(&self) {{ {blocking} }} }}")
        self.assertEqual([], self.check_body("text.shape();"))
        self.write("crates/flea-gui/src/probe.rs", f"#[cfg(test)]\nimpl Recorder {{ fn shape(&self) {{ {blocking} }} }}")
        self.assertEqual([], self.check_body("text.shape();"))
        self.write("crates/flea-gui/src/probe.rs", f"impl Recorder {{ fn shape(&self) {{ {blocking} }} }}")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))
        self.write("crates/flea-gui/src/probe.rs", "mod support { mod fake; }")
        self.write("crates/flea-gui/src/probe/support/fake.rs", f"impl ShapeProbe {{ fn shape(&self) {{ {blocking} }} }}")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))
        self.write("crates/flea-gui/src/probe.rs", "#[cfg(test)]\nmod support { mod fake; }")
        self.assertEqual([], self.check_body("text.shape();"))
        (self.root / "crates/flea-gui/src/probe/support/fake.rs").unlink()
        self.write("crates/flea-gui/src/lib.rs", "#[cfg(test)]\nmod gate;\nmod probe;")
        self.write("crates/flea-gui/src/gate.rs", "mod receipt;")
        self.write("crates/flea-gui/src/probe.rs", "#[cfg(test)]\n#[path = \"other/bridge.rs\"]\nmod bridge;")
        self.write("crates/flea-gui/src/gate/receipt.rs", f"impl ShapeProbe {{ fn shape(&self) {{ {blocking} }} }}")
        self.assertEqual([], self.check_body("text.shape();"))
        (self.root / "crates/flea-gui/src/gate/receipt.rs").unlink()
        self.write("crates/flea-gui/src/other/bridge.rs", f"impl ShapeProbe {{ fn shape(&self) {{ {blocking} }} }}")
        self.assertEqual([], self.check_body("text.shape();"))
        # Without the attribute the same file is product code and is reached.
        self.write("crates/flea-gui/src/probe.rs", "#[path = \"other/bridge.rs\"]\nmod bridge;")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))
        # The attribute ends with its item, and a test-only twin never hides the product function.
        (self.root / "crates/flea-gui/src/other/bridge.rs").unlink()
        for item in ("use std::fs;", "fn helper() {}", "struct Fake {}"):
            self.write("crates/flea-gui/src/probe.rs", f"#[cfg(test)]\n{item}\nimpl ShapeProbe {{ fn shape(&self) {{ {blocking} }} }}")
            self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")), item)
        self.write("crates/flea-gui/src/probe.rs", f"#[cfg(not(test))]\npub fn hold() {{ {blocking} }}\n#[cfg(test)]\npub fn hold() {{}}")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("crate::probe::hold();")))

    def test_product_and_test_declarations_keep_shared_code_visible(self):
        self.write("crates/flea-gui/src/lib.rs", "#[cfg(not(test))]\nmod probe;\n#[cfg(test)]\nmod probe;")
        self.write("crates/flea-gui/src/probe.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))

    def test_product_and_test_path_alias_keep_shared_code_visible(self):
        self.write("crates/flea-gui/src/lib.rs", 'mod probe;\n#[cfg(test)]\n#[path = "probe.rs"]\nmod probe_tests;')
        self.write("crates/flea-gui/src/probe.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))

    def test_conditional_product_path_keeps_shared_code_visible(self):
        self.write("crates/flea-gui/src/shared.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        for attribute in ('#[cfg_attr(unix, path="shared.rs")]',
                          '#[cfg_attr(unix, allow(dead_code), path="shared.rs")]',
                          '#[cfg_attr(unix, cfg_attr(feature="live", path="shared.rs"))]'):
            with self.subTest(attribute=attribute):
                self.write("crates/flea-gui/src/lib.rs", attribute + ' mod live; #[cfg(test)] #[path="shared.rs"] mod fixture;')
                self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))

    def test_conditional_test_path_stays_test_only(self):
        self.write("crates/flea-gui/src/lib.rs", '#[cfg_attr(test, path="fixture.rs")] mod live;')
        self.write("crates/flea-gui/src/live.rs", "fn harmless() {}")
        self.write("crates/flea-gui/src/fixture.rs", "fn paint() { std::thread::park(); }")
        self.assertEqual([], self.errors())

    def test_conditional_product_path_keeps_its_fallback_test_only(self):
        self.write("crates/flea-gui/src/lib.rs", '#[cfg_attr(not(test), path="live.rs")] mod fixture;')
        self.write("crates/flea-gui/src/live.rs", "fn harmless() {}")
        self.write("crates/flea-gui/src/fixture.rs", "fn paint() { std::thread::park(); }")
        self.assertEqual([], self.errors())

    def test_conditional_inline_path_keeps_product_child_visible(self):
        self.write("crates/flea-gui/src/lib.rs", '#[cfg_attr(unix, path="live")] mod owner { mod shared; } #[cfg(test)] #[path="live/shared.rs"] mod fixture;')
        self.write("crates/flea-gui/src/live/shared.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))

    def test_inline_test_path_cannot_hide_a_sibling_product_file(self):
        self.write("crates/flea-gui/src/lib.rs", 'mod probe;\n#[cfg(test)]\nmod support { #[path = "probe.rs"] mod fake; }')
        self.write("crates/flea-gui/src/probe.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        self.write("crates/flea-gui/src/support/probe.rs", "fn harmless() {}")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))

    def test_inline_test_path_excludes_the_actual_nested_file(self):
        self.write("crates/flea-gui/src/lib.rs", 'mod support { #[cfg(test)] #[path = "fake.rs"] mod fake; }')
        self.write("crates/flea-gui/src/support/fake.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        self.assertEqual([], self.check_body("text.shape();"))

    def test_non_mod_rs_inline_test_path_uses_its_module_directory(self):
        self.write("crates/flea-gui/src/lib.rs", "mod probe;")
        self.write("crates/flea-gui/src/probe.rs", 'mod support { #[cfg(test)] #[path = "fake.rs"] mod fake; }')
        self.write("crates/flea-gui/src/probe/support/fake.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        self.assertEqual([], self.check_body("text.shape();"))

    def test_external_test_module_propagates_to_its_path_child(self):
        self.write("crates/flea-gui/src/lib.rs", "#[cfg(test)]\nmod gate;")
        self.write("crates/flea-gui/src/gate.rs", '#[path = "helper.rs"]\nmod helper;')
        self.write("crates/flea-gui/src/helper.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        self.assertEqual([], self.check_body("text.shape();"))

    def test_shared_external_module_keeps_its_path_child_in_product(self):
        self.write("crates/flea-gui/src/lib.rs", "#[cfg(not(test))]\nmod gate;\n#[cfg(test)]\nmod gate;")
        self.write("crates/flea-gui/src/gate.rs", '#[path = "helper.rs"]\nmod helper;')
        self.write("crates/flea-gui/src/helper.rs", "impl ShapeProbe { fn shape(&self) { replies.recv_blocking(); } }")
        self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("text.shape();")))

    def test_path_owned_module_uses_containing_directory_for_child(self):
        self.write("crates/flea-gui/src/lib.rs", '#[cfg(test)] #[path="support/entry.rs"] mod fixture;')
        self.write("crates/flea-gui/src/support/entry.rs", "mod helper;")
        self.write("crates/flea-gui/src/support/helper.rs", "fn paint() { std::thread::park(); }")
        self.assertEqual([], self.errors())

    def test_path_owned_module_uses_containing_directory_for_inline_child(self):
        self.write("crates/flea-gui/src/lib.rs", '#[cfg(test)] #[path="fixtures/gate.rs"] mod gate;')
        self.write("crates/flea-gui/src/fixtures/gate.rs", 'mod support { #[path="helper.rs"] mod helper; }')
        self.write("crates/flea-gui/src/fixtures/support/helper.rs", "fn paint() { std::thread::park(); }")
        self.assertEqual([], self.errors())

    def test_shared_file_keeps_product_normal_and_test_owned_children_separate(self):
        self.write("crates/flea-gui/src/lib.rs", 'mod entry; #[cfg(test)] #[path="entry.rs"] mod fixture;')
        self.write("crates/flea-gui/src/entry.rs", "mod helper;")
        self.write("crates/flea-gui/src/entry/helper.rs", "fn product() {}")
        self.write("crates/flea-gui/src/helper.rs", "fn paint() { std::thread::park(); }")
        self.assertEqual([], self.errors())

    def test_shared_file_keeps_product_owned_and_test_normal_children_separate(self):
        self.write("crates/flea-gui/src/lib.rs", '#[path="entry.rs"] mod live; #[cfg(test)] mod entry;')
        self.write("crates/flea-gui/src/entry.rs", "mod helper;")
        self.write("crates/flea-gui/src/entry/helper.rs", "fn paint() { std::thread::park(); }")
        self.write("crates/flea-gui/src/helper.rs", "fn product() {}")
        self.assertEqual([], self.errors())

    def test_custom_cargo_target_owns_its_containing_directory(self):
        target = self.root / "crates/flea-gui/src/app.rs"
        self.metadata["packages"][0]["targets"][0]["src_path"] = str(target)
        manifest = self.root / "crates/flea-gui/Cargo.toml"
        self.write("crates/flea-gui/Cargo.toml", manifest.read_text() + '\n[lib]\npath="src/app.rs"\n')
        self.write("crates/flea-gui/src/app.rs", "#[cfg(test)] mod helper;")
        self.write("crates/flea-gui/src/helper.rs", "fn paint() { std::thread::park(); }")
        self.assertEqual([], self.errors())

    def test_test_descendant_named_lib_is_no_cargo_root(self):
        self.write("crates/flea-gui/src/lib.rs", "#[cfg(test)] mod gate;")
        self.write("crates/flea-gui/src/gate.rs", '#[path="fixtures/lib.rs"] mod helper;')
        self.write("crates/flea-gui/src/fixtures/lib.rs", "fn paint() { std::thread::park(); }")
        self.assertEqual([], self.errors())

    def test_test_descendant_named_main_is_no_cargo_root(self):
        self.write("crates/flea-gui/src/lib.rs", '#[cfg(test)] #[path="support/main.rs"] mod fixture;')
        self.write("crates/flea-gui/src/support/main.rs", 'fn paint() { std::fs::read("x"); }')
        self.assertEqual([], self.errors())

    def test_normal_module_named_lib_keeps_its_stem_directory(self):
        self.write("crates/flea-gui/src/lib.rs", "#[cfg(test)] mod outer;")
        self.write("crates/flea-gui/src/outer/mod.rs", "mod lib;")
        self.write("crates/flea-gui/src/outer/lib.rs", "mod helper;")
        self.write("crates/flea-gui/src/outer/lib/helper.rs", "fn paint() { std::thread::park(); }")
        self.assertEqual([], self.errors())

    def test_test_only_element_cannot_satisfy_product_registration(self):
        body = LIST.replace("const ROW_HEIGHT: f32 = 31.;", "")
        self.write("crates/flea-gui/src/list.rs", "const ROW_HEIGHT: f32 = 31.;\n#[cfg(test)] mod tests {\n" + body + "\n}")
        self.assertTrue(any("registered hot surface has no custom Element" in e for e in self.errors()))

    def test_inner_test_attributes_cannot_supply_product_elements(self):
        body = LIST.replace("const ROW_HEIGHT: f32 = 31.;", "")
        for source in ("const ROW_HEIGHT: f32 = 31.;\nmod tests { #![cfg(test)]\n" + body + "\n}",
                       "#![cfg(test)]\n" + LIST):
            with self.subTest(source=source):
                self.write("crates/flea-gui/src/list.rs", source)
                self.assertTrue(any("registered hot surface has no custom Element" in e for e in self.errors()))

    def test_inner_test_scope_excludes_its_blocking_callback(self):
        self.write("crates/flea-gui/src/list.rs", LIST + "\nmod tests { #![allow(dead_code)] #![cfg(test)] fn paint() { std::thread::park(); } }")
        self.assertEqual([], self.errors())

    def test_inner_test_function_is_not_a_product_callback_or_method(self):
        self.write("crates/flea-gui/src/list.rs", LIST + "\nmod tests { fn paint() { #![allow(dead_code)] #![cfg(test)] std::thread::park(); } }")
        self.assertEqual([], self.errors())
        self.write("crates/flea-gui/src/list.rs", LIST.replace("fn paint() {", "fn paint() { #![cfg(test)]"))
        self.assertTrue(any("custom Element must implement layout, prepaint and paint" in e for e in self.errors()))

    def test_test_attribute_function_is_not_a_product_callback(self):
        self.write("crates/flea-gui/src/list.rs", LIST + "\nmod tests { #[test] fn paint() { std::thread::park(); } }")
        self.assertEqual([], self.errors())

    def test_test_only_method_cannot_satisfy_product_registration(self):
        self.write("crates/flea-gui/src/list.rs", LIST.replace("fn paint()", "#[cfg(test)] fn paint()"))
        self.assertTrue(any("custom Element must implement layout, prepaint and paint" in e for e in self.errors()))

    def test_test_only_file_cannot_satisfy_product_registration(self):
        self.write("crates/flea-gui/src/lib.rs", "#[cfg(test)] mod list;")
        self.assertTrue(any("registered hot surface has no custom Element" in e for e in self.errors()))

    def test_test_element_twin_keeps_product_registration(self):
        body = LIST.replace("const ROW_HEIGHT: f32 = 31.;", "")
        self.write("crates/flea-gui/src/list.rs", LIST + "\n#[cfg(test)] mod tests {\n" + body + "\n}")
        self.assertEqual([], self.errors())

    def test_cargo_test_targets_cannot_supply_product_elements(self):
        self.write("crates/flea-gui/src/lib.rs", "#[cfg(test)] mod list;")
        targets = self.metadata["packages"][0]["targets"]
        for kind in ("test", "bench"):
            with self.subTest(kind=kind):
                targets[1:] = [{"kind": [kind], "src_path": str(self.root / "crates/flea-gui/src/list.rs")}]
                self.assertTrue(any("registered hot surface has no custom Element" in e for e in self.errors()))

    def test_cargo_test_target_descendants_stay_test_only(self):
        self.write("crates/flea-gui/src/probe.rs", '#[path="fixtures/helper.rs"] mod helper;')
        self.write("crates/flea-gui/src/fixtures/helper.rs", "fn paint() { std::thread::park(); }")
        targets = self.metadata["packages"][0]["targets"]
        for kind in ("test", "bench"):
            with self.subTest(kind=kind):
                targets[1:] = [{"kind": [kind], "src_path": str(self.root / "crates/flea-gui/src/probe.rs")}]
                self.assertEqual([], self.errors())

    def test_cargo_test_target_keeps_its_product_module_alias(self):
        self.write("crates/flea-gui/src/lib.rs", "mod list;")
        targets = self.metadata["packages"][0]["targets"]
        for kind in ("test", "bench"):
            with self.subTest(kind=kind):
                targets[1:] = [{"kind": [kind], "src_path": str(self.root / "crates/flea-gui/src/list.rs")}]
                self.assertEqual([], self.errors())
                self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("replies.recv_blocking();")))
                self.check_body("theme.background;")

    def test_cargo_test_target_keeps_its_product_target_alias(self):
        targets = self.metadata["packages"][0]["targets"]
        targets[0]["src_path"] = str(self.root / "crates/flea-gui/src/list.rs")
        targets[0]["test"] = True
        for kind in ("test", "bench"):
            with self.subTest(kind=kind):
                targets[1:] = [{"kind": [kind], "src_path": targets[0]["src_path"]}]
                self.assertEqual([], self.errors())
                self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("replies.recv_blocking();")))
                self.check_body("theme.background;")

    def test_cargo_product_target_with_test_enabled_stays_product(self):
        target = self.metadata["packages"][0]["targets"][0]
        target["src_path"] = str(self.root / "crates/flea-gui/src/list.rs")
        target["test"] = True
        for kind in ("lib", "bin", "example", "proc-macro"):
            with self.subTest(kind=kind):
                target["kind"] = [kind]
                self.assertEqual([], self.errors())
                self.assertTrue(any("blocking call recv_blocking" in e for e in self.check_body("replies.recv_blocking();")))
                self.check_body("theme.background;")

    def test_row_height_may_live_in_a_sibling_the_element_uses(self):
        self.policy["hot_surfaces"][0]["row_height_path"] = "crates/flea-gui/src/list/pane.rs"
        moved = LIST.replace("const ROW_HEIGHT: f32 = 31.;", "use pane::ROW_HEIGHT;")
        self.write("crates/flea-gui/src/list.rs", moved)
        self.write("crates/flea-gui/src/list/pane.rs", "pub const ROW_HEIGHT: f32 = 31.;")
        self.assertEqual([], self.errors())
        self.write("crates/flea-gui/src/list/pane.rs", "pub const OTHER: f32 = 31.;")
        self.assertTrue(any("missing fixed row height ROW_HEIGHT" in e for e in self.errors()))
        self.write("crates/flea-gui/src/list/pane.rs", "pub const ROW_HEIGHT: f32 = 31.;")
        self.write("crates/flea-gui/src/list.rs", LIST.replace("const ROW_HEIGHT: f32 = 31.;", ""))
        self.assertTrue(any("does not use its fixed row height" in e for e in self.errors()))
        del self.policy["hot_surfaces"][0]["row_height_path"]
        self.write("crates/flea-gui/src/list.rs", moved)
        self.assertTrue(any("missing fixed row height ROW_HEIGHT" in e for e in self.errors()))

    def test_manifest_metadata_and_lock_sources(self):
        def check(meta=None):
            return source_checks(self.root, self.paths(), meta or self.metadata, self.policy)
        self.assertEqual([], check())
        for bad in (None, "registry+https://github.com/rust-lang/crates.io-index",
                    SOURCE.replace("a" * 40, "b" * 40), SOURCE.replace("thisisgm", "unknown")):
            metadata = copy.deepcopy(self.metadata)
            metadata["packages"][1]["source"] = bad
            self.assertTrue(check(metadata), bad)
        self.write("crates/flea-gui/Cargo.toml", '[dependencies]\ngpui = { path = "../vendor/gpui" }\n')
        self.assertTrue(check())
        self.write("crates/flea-gui/Cargo.toml", '[dependencies]\ngpui.workspace = true\n')
        self.assertEqual([], check())
        self.write("Cargo.lock", '[ [package] ]\n')
        with self.assertRaises(ValueError):
            check()

    def test_patches_vendor_unknown_git_and_second_pin(self):
        def check():
            return source_checks(self.root, self.paths(), self.metadata, self.policy)
        self.assertEqual([], check())
        original = (self.root / "Cargo.toml").read_text()
        for suffix in ('\n[patch.crates-io]\ngpui = { path = "vendor/gpui" }\n',
                       '\n[replace]\n"gpui:0.3.9" = { path = "vendor/gpui" }\n'):
            self.write("Cargo.toml", original + suffix)
            self.assertTrue(check())
        self.write("Cargo.toml", original)
        self.write(".cargo/config.toml", '[source.crates-io]\nreplace-with = "vendored-sources"\n')
        self.assertTrue(check())
        self.write(".cargo/config.toml", "")
        self.assertEqual([], check())
        self.metadata["packages"].append({"name": "extra", "id": "extra", "source": "git+https://unknown/repo#abc"})
        self.assertTrue(check())

    def test_symlink_native_source_is_rejected(self):
        self.assertEqual([], source_checks(self.root, self.paths(), self.metadata, self.policy))
        self.write("ui/boot/keep", "frozen reference\n")
        reference = self.root / "ui/boot/Ui"
        reference.symlink_to(self.root.parent)
        paths = self.paths() + [reference.relative_to(self.root)]
        self.assertEqual([], source_checks(self.root, paths, self.metadata, self.policy))
        link = self.root / "crates/flea-gui/src/link.rs"
        link.symlink_to(self.root / "crates/flea-gui/src/list.rs")
        self.assertTrue(any("symlink" in e for e in source_checks(self.root, self.paths(), self.metadata, self.policy)))
        with self.assertRaises(ValueError):
            native_sources(self.root, self.paths(), self.metadata)

    def test_tracked_native_inventory(self):
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        self.assertEqual(1, len(native_sources(self.root, tracked(self.root), self.metadata)))
        self.write("crates/flea-gui/src/new.rs", "fn paint() { std::fs::read(\"x\"); }")
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        units = native_sources(self.root, tracked(self.root), self.metadata)
        self.assertEqual(2, len(units))
        self.assertTrue(any("blocking" in e for e in rust_checks(units, self.policy)))

    def test_archived_source_inventory_has_the_same_controls(self):
        with patch.dict(os.environ, {"GIT_CEILING_DIRECTORIES": str(self.root.parent)}):
            units = native_sources(self.root, tracked(self.root), self.metadata)
            self.assertEqual(1, len(units))
            self.assertEqual([], rust_checks(units, self.policy))
            self.check_body('std::fs::read("x");')
            units = native_sources(self.root, tracked(self.root), self.metadata)
            self.assertTrue(any("blocking" in e for e in rust_checks(units, self.policy)))

    def test_complete_cli_passes_control_and_rejects_mutant(self):
        self.write("tools/gpui-lint.json", json.dumps(self.policy))
        self.write("metadata.json", json.dumps(self.metadata))
        self.write("bin/cargo", '''#!/usr/bin/env python3
from pathlib import Path
import sys
assert sys.argv[1:] == ["metadata", "--locked", "--format-version", "1"]
print(Path("metadata.json").read_text())
''')
        (self.root / "bin/cargo").chmod(0o755)
        env = dict(os.environ, PATH=str(self.root / "bin") + os.pathsep + os.environ["PATH"],
                   GIT_CEILING_DIRECTORIES=str(self.root.parent))
        command = [str(REPO / "tools/flea-gpui-lint")]
        control = subprocess.run(command, cwd=self.root, env=env, capture_output=True, text=True)
        self.assertEqual(0, control.returncode, control.stdout + control.stderr)
        self.assertIn("PASS files=1 findings=0", control.stdout)
        self.check_body('std::fs::read("x");')
        mutant = subprocess.run(command, cwd=self.root, env=env, capture_output=True, text=True)
        self.assertEqual(1, mutant.returncode, mutant.stdout + mutant.stderr)
        self.assertIn("UI blocking call std::fs::read", mutant.stdout)

    def test_clippy_workspace_ratchet_and_independent_strict_gate(self):
        bindir = self.root / "bin"
        bindir.mkdir()
        self.write("baseline.txt", "clippy::seed_style|style|1\n")
        self.write("bin/clippy-driver", '''#!/usr/bin/env python3
print("Lint groups loaded by this crate:")
print(" clippy::correctness  clippy::seed_correctness")
print(" clippy::suspicious  clippy::seed_suspicious")
print(" clippy::style  clippy::seed_style")
''')
        self.write("bin/cargo", '''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
assert args[0] == "clippy" and "--locked" in args and "--all-targets" in args
with pathlib.Path(os.environ["CALLS"]).open("a") as log:
    log.write(json.dumps(args) + "\\n")
if "--workspace" not in args:
    assert args[args.index("-p") + 1] == "flea-gui" and args[-2:] == ["-D", "warnings"]
    sys.exit(int(os.environ["STRICT_RC"]))
span = [{"is_primary": True, "file_name": "src/lib.rs", "line_start": 1, "column_start": 1}]
for name in ("flea", "flea-gui"):
    package_id = "path+file:///fixture/" + name + "#0.4.0" if os.environ.get("SHORT_ID") else "path+file:///fixture#" + name + "@0.4.0"
    print(json.dumps({"reason": "compiler-message", "package_id": package_id, "message": {"level": "warning", "code": {"code": "clippy::seed_style"}, "spans": span, "message": "seed"}}))
print(json.dumps({"reason": "compiler-artifact", "target": {"name": "flea"}}))
print(json.dumps({"reason": "build-finished", "success": True}))
''')
        for name in ("cargo", "clippy-driver"):
            (bindir / name).chmod(0o755)
        env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ["PATH"],
                   CALLS=str(self.root / "calls.jsonl"), STRICT_RC="0")
        args = [str(REPO / "tools/flea-clippy"), str(self.root / "baseline.txt"), str(self.root / "clippy")]
        control = subprocess.run(args, cwd=self.root, env=env, capture_output=True, text=True)
        self.assertEqual(0, control.returncode, control.stdout)
        self.assertIn("findings=1", control.stdout)
        self.assertIn("flea-gui: PASS", control.stdout)
        env["STRICT_RC"] = "1"
        mutant = subprocess.run(args, cwd=self.root, env=env, capture_output=True, text=True)
        self.assertEqual(1, mutant.returncode, mutant.stdout)
        self.assertIn("flea-gui: FAIL cargo_rc=1", mutant.stdout)
        calls = [json.loads(line) for line in (self.root / "calls.jsonl").read_text().splitlines()]
        self.assertEqual(4, len(calls))
        self.assertTrue(all("--workspace" in args for args in calls[::2]))
        env.update(STRICT_RC="0", SHORT_ID="1")
        control = subprocess.run(args, cwd=self.root, env=env, capture_output=True, text=True)
        self.assertEqual(0, control.returncode, control.stdout)
        self.assertIn("findings=1", control.stdout)

    def test_hook_guard_covers_native_sources_and_license_policy(self):
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        command = [str(REPO / "tools/flea-hook-check"), "guard"]
        control = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
        self.assertEqual(0, control.returncode, control.stderr)
        for name in ("crates/flea-gui/src/list.rs", "deny.toml"):
            self.write(name, "changed\n")
            mutant = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
            self.assertEqual(1, mutant.returncode, mutant.stderr)
            self.assertIn(name + " is not fully staged", mutant.stderr)
            subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
            control = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
            self.assertEqual(0, control.returncode, control.stderr)

    def use_check(self, prefix, code):
        self.write("crates/flea-gui/src/list.rs", prefix + LIST.replace("theme.background;", code))
        return self.errors()

    def test_digit_separators_and_integer_suffixes_are_colour_literals(self):
        for code in ("let c = 0x1e_29_3b;", "let c = 0x1e293b_u32;", "let c = 0x1e293bu8;", "let c = 0x1e_29_3b_usize;"):
            with self.subTest(code=code):
                self.assertTrue(any("colour-sized hex" in e for e in self.check_body(code)), code)
        self.assertEqual([], self.check_body("let c = 0x12345_u32;"))

    def test_text_color_reads_only_direct_arguments(self):
        self.assertTrue(any("literal colour text_color" in e for e in self.check_body("label.text_color(0x112233);")))
        self.assertEqual([], self.check_body("label.text_color(theme.with_alpha(0.5));"))

    def test_named_colours_resolve_through_gpui_glob_and_list_imports(self):
        for prefix, code in (("use gpui::*;\n", "white();"), ("use gpui::{self, *};\n", "black();"),
                             ("use gpui::{white, black};\n", "black();"), ("use gpui::white;\n", "white();")):
            with self.subTest(prefix=prefix):
                self.assertTrue(any("literal colour" in e for e in self.use_check(prefix, code)), prefix)
        self.assertEqual([], self.use_check("use gpui::*;\nfn white() -> u32 { 0 }\n", "white();"))
        self.assertEqual([], self.use_check("use palette::*;\n", "white();"))

    def scoped_check(self, source):
        self.write("crates/flea-gui/src/list.rs", source + LIST)
        return self.errors()

    def test_glob_shadowing_follows_rust_scopes(self):
        # The rule is the bindings.py docstring, a lexical approximation of Rust glob scopes that follows only what one file shows.
        reported = {
            "impl method": "use gpui::*;\nstruct S;\nimpl S { fn white(&self) {} }\nfn a() { white(); }\n",
            "trait method": "use gpui::*;\ntrait T { fn white(&self) {} }\nfn a() { white(); }\n",
            "impl method called bare from inside its own impl": "use gpui::*;\nstruct S;\nimpl S { fn white(&self) {} fn go(&self) { white(); } }\n",
            "trait method called bare from inside its own trait": "use gpui::*;\ntrait T { fn white(&self) {} fn go(&self) { white(); } }\n",
            "let in another function": "use gpui::*;\nfn b() { let white = 1; }\nfn a() { white(); }\n",
            "nested mod with use super": "use gpui::*;\nmod inner { use super::*; fn a() { white(); } }\n",
            "1 fn-local use super in a nested mod takes the parent glob": "use gpui::*;\nmod inner { fn a() { use super::*; white(); } }\n",
            "2 nested fn item does not see the enclosing let": "use gpui::*;\nfn a() { let white = 1; fn b() { white(); } }\n",
            "2 method body does not see the enclosing let": "use gpui::*;\nfn a() { let white = 1; struct S; impl S { fn go(&self) { white(); } } }\n",
            "4 absolute glob path": "use ::gpui::*;\nfn a() { white(); }\n",
            "4 glob inside a use group": "use {gpui::*, other::black};\nfn a() { white(); }\n",
            "5 use crate glob": "use gpui::*;\nmod inner { use crate::*; fn a() { white(); } }\n",
            "5 use self path glob": "mod inner { mod colors { pub use gpui::*; } use self::colors::*; fn a() { white(); } }\n",
            "5 use super path glob": "mod colors { pub use gpui::*; }\nmod inner { use super::colors::*; fn a() { white(); } }\n",
            "5 use child module glob": "mod colors { pub use gpui::*; }\nuse colors::*;\nfn a() { white(); }\n",
            "6 block glob before an outer item": "fn white() {}\nfn a() { use gpui::*; white(); }\n",
            "6 block glob before an outer let": "fn a() { let white = 1; { use gpui::*; white(); } }\n",
            "r5-1 array type in a nested fn signature": "use gpui::*;\nfn a() { let white = 1; fn b(rows: [u8; 4]) { white(); } }\n",
            "r5-1 array type in an impl header": "use gpui::*;\ntrait T {}\nimpl T for [u8; 4] { fn white(&self) {} fn go(&self) { white(); } }\n",
            "r5-2 glob through a name the file never declares": "use gpui::*;\nmod inner { use self::unseen::*; fn a() { white(); } }\n",
        }
        for name, source in reported.items():
            with self.subTest(case=name):
                self.assertTrue(any("literal colour gpui::white" in e for e in self.scoped_check(source)), source)
        with self.subTest(case="sibling function sees no fn-local item"):
            sibling = self.scoped_check("use gpui::*;\nfn a() { fn white() {} white(); }\nfn b() { white(); }\n")
            self.assertEqual(1, sum("literal colour gpui::white" in e for e in sibling), sibling)

        quiet = {
            "earlier let in the same function": "use gpui::*;\nfn a() { let white = 1; white(); }\n",
            "module fn declared later": "use gpui::*;\nfn a() { white(); }\nfn white() {}\n",
            "fn-local item inside its own function": "use gpui::*;\nfn a() { fn white() {} white(); }\n",
            "impl method with no bare call": "use gpui::*;\nstruct S;\nimpl S { fn white(&self) {} }\n",
            "nested mod without a glob does not inherit the parent glob": "use gpui::*;\nmod inner { fn a() { white(); } }\n",
            "nested mod with use super and its own white": "use gpui::*;\nmod inner { use super::*; fn white() {} fn a() { white(); } }\n",
            "explicit use without a glob": "use other::white;\nfn a() { white(); }\n",
            "explicit list use without a glob": "use other::{white};\nfn a() { white(); }\n",
            "explicit self and rename use": "use other::{self, black as white};\nfn a() { white(); }\n",
            "fn-local explicit use": "fn a() { use other::white; white(); }\n",
            "explicit use beside the glob": "use gpui::*;\nuse other::white;\nfn a() { white(); }\n",
            "1 fn-local use super whose parent holds no glob": "mod view { use gpui::*; }\nmod inner { fn a() { use super::*; white(); } }\n",
            "2 nested fn item sees the enclosing items": "use gpui::*;\nfn a() { fn white() {} fn b() { white(); } }\n",
            "2 closure sees the enclosing let": "use gpui::*;\nfn a() { let white = 1; let f = || { white(); }; }\n",
            "3 extern block fn binds at the enclosing level": 'use gpui::*;\nextern "C" { fn white(); }\nfn a() { white(); }\n',
            "3 extern block without an ABI": "use gpui::*;\nunsafe extern { fn white(); }\nfn a() { white(); }\n",
            "5 use super path to a module without a gpui glob": "mod view { use gpui::*; }\nmod colors { pub fn tint() {} }\nmod inner { use super::colors::*; fn a() { white(); } }\n",
            "5 use crate glob in a file without a gpui glob": "mod colors { pub fn tint() {} }\nmod inner { use crate::*; fn a() { white(); } }\n",
            "6 same-block item before the block glob": "fn a() { use gpui::*; fn white() {} white(); }\n",
            "6 later let before the block glob": "fn a() { use gpui::*; let white = 1; white(); }\n",
            "r5-1 const pointer type in a let": "use gpui::*;\nfn a() { let white = 1; let p: *const u8 = { white(); q }; }\n",
            "r5-1 static lifetime in a let": "use gpui::*;\nfn a() { let white = 1; let s: &'static str = { white(); q }; }\n",
            "r5-2 glob through an enum the module declares": "use gpui::*;\nmod inner { enum Mode { A } use self::Mode::*; fn a() { white(); } }\n",
        }
        for name, source in quiet.items():
            with self.subTest(case=name):
                self.assertEqual([], self.scoped_check(source), source)

    def test_binding_parsers_carry_a_sample_input(self):
        path = REPO / "tools/gpui_lint/bindings.py"
        lines = path.read_text().splitlines()
        tree = ast.parse("\n".join(lines))
        methods = {id(f) for c in ast.walk(tree) if isinstance(c, ast.ClassDef) for f in c.body}
        for node in ast.walk(tree):
            if isinstance(node, ast.FunctionDef) and id(node) not in methods:
                with self.subTest(function=node.name):
                    self.assertTrue(lines[node.lineno - 2].lstrip().startswith("# Sample input: "), node.name)

    def test_tracked_fails_closed_on_a_broken_checkout(self):
        with patch.dict(os.environ, {"GIT_CEILING_DIRECTORIES": str(self.root.parent)}):
            (self.root / ".git").mkdir()
            with self.assertRaises(ValueError):
                tracked(self.root)

    def test_malformed_policy_sections_fail_before_field_access(self):
        for section, value in (("blocking_exceptions", ["flea_gui::slow::probe"]),
                               ("color_modules", "palette"),
                               ("non_color_hex_constants", [1])):
            with self.subTest(section=section):
                with self.assertRaises(ValueError):
                    POLICY_CHECKS(dict(self.policy, **{section: value}))

    def test_malformed_policy_exits_two_without_traceback(self):
        self.write("tools/gpui-lint.json", json.dumps(dict(self.policy, blocking_exceptions=["flea_gui::slow::probe"])))
        env = dict(os.environ, GIT_CEILING_DIRECTORIES=str(self.root.parent))
        result = subprocess.run([str(REPO / "tools/flea-gpui-lint")], cwd=self.root, env=env, capture_output=True, text=True)
        self.assertEqual(2, result.returncode, result.stdout + result.stderr)
        self.assertIn("flea-gpui-lint: ERROR", result.stdout)
        self.assertNotIn("Traceback", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
