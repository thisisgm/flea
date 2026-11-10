use super::{
    listing_checks::{listing_action, press},
    resolved_preset, sequence, Key, Map,
};
use crate::{
    jsondoc,
    tui::{
        action_key_tests::{drain, listing},
        cwd,
        graphics::Protocol,
        media::Player,
        model::{Model, Tab},
        pdf::Pdf,
        render::{
            menu_layout::{self, Sub},
            menu_run,
        },
        wire::{self, text},
    },
};
use std::path::{Path, PathBuf};

fn answer(m: &mut Model, w: &mut wire::Wire, line: &str) {
    m.receive(
        jsondoc::parse(&line.replace("ID", &m.action_id.to_string())).unwrap(),
        w,
    )
    .unwrap();
}
fn registry(m: &mut Model, w: &mut wire::Wire) {
    answer(
        m,
        w,
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true}"#,
    );
    answer(
        m,
        w,
        r#"{"t":"menuaction","op":"applications","id":ID,"ok":true,"applications":[{"id":"first.desktop","label":"First","default":true},{"id":"second.desktop","label":"Second"}]}"#,
    );
    drain(w);
}
pub(super) fn open_with(keys: &[Key], action: &str, from_listing: bool, map: &Map) {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    m.cursor = 1;
    if from_listing {
        press(&mut m, &keys[..1], map, &mut w);
        assert!(m.menu && !m.menu_ready);
        registry(&mut m, &mut w);
        press(&mut m, &keys[1..], map, &mut w);
    } else {
        menu_run::open(&mut m, &mut w).unwrap();
        registry(&mut m, &mut w);
        let rows = menu_layout::entries(&m, map);
        m.menu_cursor = rows
            .iter()
            .position(|row| row.as_ref().is_some_and(|row| row.id == "openwith"))
            .unwrap();
        press(&mut m, &[Key::named("Right", "")], map, &mut w);
        assert_eq!(m.menu_view.sub, Sub::OpenWith);
        if action == "cursorUp" {
            m.menu_cursor = 1;
        }
        press(&mut m, keys, map, &mut w);
    }
    let sent = drain(&mut w);
    match action {
        "openWith" => {
            let request = sent
                .iter()
                .find(|v| text(v, "op") == "openWith")
                .expect("no application launch request");
            assert_eq!(text(request, "application"), "first.desktop");
            assert_eq!(wire::count(request, "id"), m.action_id);
        }
        "cursorUp" => assert_eq!(m.menu_cursor, 0),
        "cursorDown" => assert_eq!(m.menu_cursor, 1),
        "close" => assert_eq!(m.menu_view.sub, Sub::Root),
        "ignored" => {
            assert_eq!(m.menu_view.sub, Sub::OpenWith);
            assert_eq!(m.menu_cursor, 0);
        }
        other => panic!("no Open With assertion for {other}"),
    }
    assert_eq!((m.path, m.cursor), (PathBuf::from("/listing"), 1));
    assert!(m.selected.is_empty() && m.clipboard.is_empty() && m.edit_file.is_none());
    assert!(m.launches.is_empty());
    if action != "openWith" {
        assert!(sent.is_empty());
    }
    wire::finish(w, reader);
}

fn preview_model(w: &mut wire::Wire, preset: &str, context: &str) -> Model {
    let mut m = listing(w, preset);
    m.preview_focus = true;
    if context == "pdf" || context == "office" {
        let mut pdf = Pdf::new(
            "/nonexistent/flea-parity.pdf".into(),
            Protocol::None,
            40,
            20,
            (0, 0),
            "#000000",
            false,
        );
        pdf.source = "/nonexistent/document.docx".into();
        pdf.pages = 3;
        m.pdf = Some(pdf);
    }
    if context == "media" {
        m.player = Some(Player::resting(
            Path::new("/nonexistent/flea-parity.mp4"),
            Protocol::None,
            false,
            (0, 0, 40, 20),
            (130, 31),
            (0, 0),
        ));
    }
    m
}
pub(super) fn appendix(row: &[&str], map: &Map) {
    let presets: &[&str] = if row[9] == "all five" {
        &["default", "vim", "mac", "windows", "yazi"]
    } else {
        &[row[9]]
    };
    for preset in presets {
        let keys = sequence(row[4]);
        match row[3] {
            "listing" => {
                if row[5] != "reserved" {
                    assert_eq!(
                        resolved_preset(map, &keys, preset, "listing"),
                        row[5],
                        "{} {preset}",
                        row[8]
                    );
                }
                if row[5] == "reserved" {
                    assert!(map.action(&keys[0], preset).is_empty());
                }
                listing_action(&keys, row[5], preset, map);
                if row[8] == "tab-number" {
                    tab_digits(preset, map);
                }
            }
            "preview" | "pdf" | "media" => {
                assert_eq!(resolved_preset(map, &keys, preset, row[3]), row[5]);
                let (mut w, reader) = wire::echo_wire();
                let mut m = preview_model(&mut w, preset, row[3]);
                for path in ["/second", "/third"] {
                    m.tabs.push(Tab {
                        path: path.into(),
                        cursor: 0,
                        back: Vec::new(),
                        forward: Vec::new(),
                    });
                }
                press(&mut m, &keys, map, &mut w);
                match row[5] {
                    "focusPreview" => assert!(!m.preview_focus),
                    "focusNext" if row[3] == "pdf" => {
                        assert_eq!(m.pdf.as_ref().unwrap().control, 1)
                    }
                    "focusNext" if row[3] == "media" => {
                        assert_eq!(m.player.as_ref().unwrap().control, 1)
                    }
                    "focusNext" => assert!(m.preview_focus),
                    "tabNext" | "tabPrevious" => {
                        let (tab, path) = if row[5] == "tabNext" {
                            (1, "/second")
                        } else {
                            (2, "/third")
                        };
                        assert_eq!(m.tab, tab, "{} {preset}", row[8]);
                        assert_eq!(m.pending.as_deref(), Some(Path::new(path)));
                    }
                    other => panic!("unverified preview action {other}"),
                }
                assert_eq!(m.cursor, 0);
                assert!(
                    m.selected.is_empty()
                        && m.clipboard.is_empty()
                        && m.launches.is_empty()
                        && m.edit_file.is_none()
                );
                assert!(drain(&mut w)
                    .iter()
                    .all(|v| matches!(text(v, "c"), "list" | "modelcancel")));
                wire::finish(w, reader);
            }
            "model3d" => {
                assert_eq!(row[5], "preview-3d", "unverified 3D action {}", row[8]);
                let (mut w, reader) = wire::echo_wire();
                let mut m = listing(&mut w, preset);
                m.rows.get_mut(&0).unwrap().name = "part.obj".into();
                m.rows.get_mut(&0).unwrap().directory = false;
                m.rows.get_mut(&0).unwrap().kind = "OBJ 3D model".into();
                m.quicklook = true;
                press(&mut m, &keys, map, &mut w);
                assert_eq!(m.model3d.controls.orbit, 1);
                assert_eq!(m.cursor, 0);
                assert!(drain(&mut w).is_empty());
                wire::finish(w, reader);
            }
            "office" => {
                assert_eq!(
                    row[5], "preview-office",
                    "unverified Office action {}",
                    row[8]
                );
                let (mut w, reader) = wire::echo_wire();
                let mut m = preview_model(&mut w, preset, "office");
                press(&mut m, &keys, map, &mut w);
                assert_eq!(m.pdf.as_ref().unwrap().page, 2);
                assert_eq!(
                    m.pdf.as_ref().unwrap().source,
                    PathBuf::from("/nonexistent/document.docx")
                );
                assert_eq!(m.cursor, 0);
                assert!(drain(&mut w).is_empty());
                wire::finish(w, reader);
            }
            "startup/exit" => {
                assert_eq!(row[4], "[]", "{} is a startup option, not a key", row[8]);
                options(row[5], preset, map);
            }
            other => panic!("unverified appendix context {other}"),
        }
    }
}
// Every digit selects its own tab, and each digit past the last open tab switches nothing and lists nothing.
fn tab_digits(preset: &str, map: &Map) {
    for digit in 1..=9 {
        listing_action(
            &[super::key(&digit.to_string())],
            &format!("tab{digit}"),
            preset,
            map,
        );
    }
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, preset);
    m.tabs.push(Tab {
        path: "/second".into(),
        cursor: 0,
        back: Vec::new(),
        forward: Vec::new(),
    });
    for digit in m.tabs.len() + 1..=9 {
        press(&mut m, &[super::key(&digit.to_string())], map, &mut w);
        assert_eq!(
            (m.tab, m.tabs.len(), m.path.clone()),
            (0, 2, PathBuf::from("/listing")),
            "{preset}: digit {digit} past the last tab switched"
        );
        assert!(
            m.error.is_empty() && m.pending.is_none(),
            "{preset}: digit {digit}: {}",
            m.error
        );
    }
    assert!(
        drain(&mut w).is_empty(),
        "{preset}: a digit past the last tab listed"
    );
    wire::finish(w, reader);
}
// A mktemp -d root under the temp dir with a marker file, removed on drop even when an assertion panics.
struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Sandbox {
        let parent = std::env::temp_dir();
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("flea-tui15.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        assert!(root.is_absolute() && root.parent() == Some(parent.as_path()));
        std::fs::write(
            root.join(".flea-test-sandbox"),
            "owned TUI15 parity proof\n",
        )
        .unwrap();
        Sandbox(root)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let root = &self.0;
        if root.is_absolute()
            && !root.as_os_str().is_empty()
            && root.parent() == Some(std::env::temp_dir().as_path())
            && root.join(".flea-test-sandbox").is_file()
        {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
#[test]
fn tui_parity_sandbox_is_removed_when_a_check_panics() {
    let payload = std::panic::catch_unwind(|| {
        let sandbox = Sandbox::new();
        std::panic::panic_any(sandbox.0.clone());
    })
    .unwrap_err();
    let root = payload.downcast::<PathBuf>().unwrap();
    assert!(
        root.parent() == Some(std::env::temp_dir().as_path()) && !root.exists(),
        "{}",
        root.display()
    );
}
fn options(action: &str, preset: &str, map: &Map) {
    match action {
        "keymapOption" => {
            let mut options = cwd::Options::default();
            assert!(options
                .take(&["--tui-keymap".into(), preset.into()], &mut 0)
                .unwrap());
            assert_eq!(options.keymap.as_deref(), Some(preset));
            assert!(options.validate(true, false).is_ok());
            assert!(options.validate(false, false).is_err());
        }
        "chooserFile" => {
            let sandbox = Sandbox::new();
            let (mut w, reader) = wire::echo_wire();
            let mut m = listing(&mut w, preset);
            m.choosing = true;
            m.cursor = 1;
            press(&mut m, &[Key::named("Return", "")], map, &mut w);
            assert!(m.choose_pending && !m.quit);
            assert_eq!(text(&drain(&mut w)[0], "c"), "paths");
            m.receive(
                jsondoc::parse(r#"{"t":"paths","paths":["/listing/file"]}"#).unwrap(),
                &mut w,
            )
            .unwrap();
            assert!(m.quit);
            assert_eq!(m.chosen.as_ref().unwrap(), &["/listing/file"]);
            let output = sandbox.0.join("chosen");
            let options = cwd::Options {
                chooser_file: Some(output.clone()),
                ..Default::default()
            };
            assert_eq!(
                options
                    .finish(
                        m.last_directory.as_deref(),
                        m.chosen.as_deref(),
                        m.quit,
                        m.no_cwd
                    )
                    .unwrap(),
                0
            );
            assert_eq!(std::fs::read_to_string(&output).unwrap(), "/listing/file\n");
            wire::finish(w, reader);
        }
        "cwdFile" => {
            // q hands the last entered folder over; Q quits and leaves the caller's output untouched.
            let sandbox = Sandbox::new();
            let output = sandbox.0.join("cwd");
            let options = cwd::Options {
                cwd_file: Some(output.clone()),
                ..Default::default()
            };
            for (quit, written) in [('q', "/listing\n"), ('Q', "kept\n")] {
                std::fs::write(&output, "kept\n").unwrap();
                let (mut w, reader) = wire::echo_wire();
                let mut m = listing(&mut w, preset);
                assert_eq!(m.last_directory.as_deref(), Some(Path::new("/listing")));
                press(&mut m, &[Key::character(quit, "")], map, &mut w);
                assert!(m.quit && m.no_cwd == (quit == 'Q'), "{preset} {quit}");
                assert_eq!(
                    options
                        .finish(
                            m.last_directory.as_deref(),
                            m.chosen.as_deref(),
                            m.quit,
                            m.no_cwd
                        )
                        .unwrap(),
                    0
                );
                assert_eq!(
                    std::fs::read_to_string(&output).unwrap(),
                    written,
                    "{preset} {quit}"
                );
                wire::finish(w, reader);
            }
        }
        other => panic!("unverified startup option {other}"),
    }
}

#[test]
fn tui_parity_required_actions_never_use_unavailable_fallback() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        for (action, key) in [
            ("searchContents", "<C-g>"),
            ("detailNext", "I"),
            ("print", "<C-p>"),
            ("convert", "F"),
            ("editFile", "E"),
        ] {
            listing_action(&[super::key(key)], action, preset, &map);
        }
    }
}

#[test]
fn tui_parity_contexts_do_not_hijack_listing() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        // What each key leaves in the status row when the listing itself takes it.
        let leaked: std::collections::BTreeMap<&str, (String, String)> =
            ["<C-p>", "E", "I", "F", "<S-Delete>", "x", "p", "d", "t"]
                .into_iter()
                .map(|key| {
                    let (mut w, reader) = wire::echo_wire();
                    let mut m = listing(&mut w, preset);
                    press(&mut m, &[super::key(key)], &map, &mut w);
                    drain(&mut w);
                    let left = (m.message.clone(), m.error.clone());
                    wire::finish(w, reader);
                    (key, left)
                })
                .collect();
        for context in [
            "editor",
            "properties",
            "sheet",
            "preview",
            "confirm",
            "menu",
            "print",
            "convert",
            "combine",
            "chooser",
        ] {
            for key in ["<C-p>", "E", "I", "F", "<S-Delete>", "x", "p", "d", "t"] {
                let (mut w, reader) = wire::echo_wire();
                let mut m = listing(&mut w, preset);
                match context {
                    "editor" => {
                        m.editor = Some(crate::tui::editor::Editor::new(
                            "filter",
                            "typed".into(),
                            m.path.clone(),
                        ))
                    }
                    "properties" | "sheet" => {
                        m.sheet = true;
                        if context == "properties" {
                            m.properties = Some(vec![("Path".into(), "/listing/file".into())]);
                        }
                    }
                    "preview" => m.preview_focus = true,
                    "confirm" => {
                        m.deletion = Some(crate::tui::model::Deletion {
                            token: 7,
                            count: 1,
                            ..Default::default()
                        })
                    }
                    "menu" => m.menu = true,
                    "print" | "convert" | "combine" => {
                        m.rows.get_mut(&0).unwrap().directory = false;
                        m.rows.get_mut(&0).unwrap().name = "photo.png".into();
                        match context {
                            "print" => crate::tui::print::open(&mut m, &mut w, false).unwrap(),
                            "convert" => crate::tui::convert::open(&mut m, &mut w, false).unwrap(),
                            _ => {
                                m.rows.get_mut(&1).unwrap().name = "second.png".into();
                                m.selected.extend([0, 1]);
                                crate::tui::combine::open(&mut m, &mut w, false).unwrap();
                            }
                        }
                        assert!(m.error.is_empty(), "{context}: {}", m.error);
                        drain(&mut w);
                    }
                    "chooser" => {
                        m.choosing = true;
                        m.choose_pending = true;
                    }
                    _ => unreachable!(),
                }
                let selected = m.selected.clone();
                let cards = (
                    m.printing.card.is_some(),
                    m.converting.card.is_some(),
                    m.combining.card.is_some(),
                );
                let (detail, message, error) = (m.detail.mode, m.message.clone(), m.error.clone());
                press(&mut m, &[super::key(key)], &map, &mut w);
                assert_eq!(m.detail.mode, detail, "{preset} {context} {key}");
                assert!(
                    m.key_arm.is_empty(),
                    "{preset} {context} {key}: armed {}",
                    m.key_arm
                );
                let (leaked_message, leaked_error) = &leaked[key];
                assert!(
                    m.message == message || m.message != *leaked_message,
                    "{preset} {context} {key}: {}",
                    m.message
                );
                assert!(
                    m.error == error || m.error != *leaked_error,
                    "{preset} {context} {key}: {}",
                    m.error
                );
                assert_eq!(
                    (m.path, m.cursor, m.tabs.len()),
                    (PathBuf::from("/listing"), 0, 1),
                    "{preset} {context} {key}"
                );
                assert_eq!(m.selected, selected, "{preset} {context} {key}");
                assert!(m.clipboard.is_empty() && m.launches.is_empty() && m.edit_file.is_none());
                assert!(!m.pending_clipboard && m.pending.is_none());
                assert_eq!(
                    (
                        m.printing.card.is_some(),
                        m.converting.card.is_some(),
                        m.combining.card.is_some()
                    ),
                    cards
                );
                // The menu's own Copy As letter p asks for its targets, as the window's does; nothing else is sent.
                let ok = |c: &str| c == "filter" || (context, key, c) == ("menu", "p", "paths");
                let quiet = drain(&mut w).iter().all(|v| ok(text(v, "c")));
                assert!(quiet, "{preset} {context} {key}");
                wire::finish(w, reader);
            }
        }
    }
}

#[test]
fn tui_parity_sheet_and_protocol_alternatives_match_candidate() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        let mut m = crate::tui::render::frame_tests::model();
        m.preset = preset.into();
        m.sheet = true;
        let mut output = String::new();
        let theme = crate::tui::theme::Theme::from_text("");
        crate::tui::render::key_sheet::draw(&mut output, &m, &theme, &map);
        for pair in [
            "ctrl-enter / O",
            "ctrl-shift-z / ctrl-y",
            "ctrl-shift-n / N",
            "tab / ctrl-tab",
            "shift-delete",
        ] {
            assert!(output.contains(pair), "{preset}: {pair}");
        }
        if preset == "yazi" {
            for (keys, label) in [("t t", "keys.tabNew"), (", a", "keys.sortNext")] {
                let binding = format!("{keys}  {}{}", theme.foreground, m.i18n.tr(label));
                assert!(output.contains(&binding), "{preset}: {binding:?}");
            }
        }
        for (enhanced, plain, action) in [
            ("<C-Enter>", "O", "openTab"),
            ("<C-S-z>", "<C-y>", "redo"),
            ("<C-S-n>", "N", "newFolder"),
        ] {
            for input in [enhanced, plain] {
                listing_action(&[super::key(input)], action, preset, &map);
            }
        }
        let mut decoder = crate::tui::input::Decoder::default();
        let keys = decoder.feed(b"\x1b[3;2~", false);
        assert_eq!(keys.len(), 1);
        listing_action(&keys, "deletePermanently", preset, &map);
    }
}

#[test]
fn tui_parity_status_jobs_cancel_through_the_existing_escape_route() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    m.receive(
        jsondoc::parse(r#"{"t":"jobs","newest":12,"unfinished":2,"head":"Extracting 1 of 3"}"#)
            .unwrap(),
        &mut w,
    )
    .unwrap();
    assert_eq!(m.active_operation(), 12);
    assert!(m.transfer.contains("Extracting 1 of 3") && m.transfer.contains("2 jobs"));
    press(&mut m, &[Key::named("Escape", "")], &Map::load(), &mut w);
    let sent = drain(&mut w);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (text(&sent[0], "c"), wire::count(&sent[0], "id")),
        ("transfercancel", 12)
    );
    assert!(m.launches.is_empty() && m.edit_file.is_none());
    wire::finish(w, reader);
}

#[test]
fn tui_parity_terminal_route_child() {
    let Some(root) = std::env::var_os("FLEA_PARITY_ROUTE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    for name in ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"] {
        assert!(PathBuf::from(std::env::var_os(name).unwrap()).starts_with(&root));
    }
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    m.path = root.join("folder");
    press(&mut m, &[Key::character('T', "ctrl")], &Map::load(), &mut w);
    assert_eq!(m.launches.len(), 1);
    let (_, mut child) = m.launches.pop().unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(
        std::fs::read_to_string(root.join("terminal.args")).unwrap(),
        format!("--dir={}\n", root.join("folder").display())
    );
    assert!(drain(&mut w).is_empty());
    wire::finish(w, reader);
}

#[test]
fn tui_parity_terminal_refusals_name_a_real_launch_route() {
    let map = Map::load();
    let rows: Vec<Vec<&str>> = include_str!("parity.tsv")
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|row| row.len() == 10 && row[7].contains("Ctrl+T opens a terminal in this folder"))
        .collect();
    assert_eq!(
        rows.iter().map(|row| row[8]).collect::<Vec<_>>(),
        ["yazi-055", "yazi-056"]
    );
    for row in &rows {
        assert_eq!(
            super::resolved(&map, &sequence(row[1]), "listing"),
            format!("unsupported:{}", row[7])
        );
    }
    assert_eq!(
        map.in_context(&Key::character('T', "ctrl"), "yazi", "listing"),
        "openTerminal"
    );
    let sandbox = Sandbox::new();
    let root = sandbox.0.clone();
    for dir in ["bin", "folder", "home", "data", "config", "cache"] {
        std::fs::create_dir(root.join(dir)).unwrap();
    }
    let script = root.join("bin/xdg-terminal-exec");
    std::fs::write(&script, "#!/bin/sh\ncase ${FLEA_PARITY_ROUTE_ROOT:?} in /*) ;; *) exit 2 ;; esac\n[ -f \"$FLEA_PARITY_ROUTE_ROOT/.flea-test-sandbox\" ] || exit 2\nprintf '%s\\n' \"$@\" > \"$FLEA_PARITY_ROUTE_ROOT/terminal.args\"\n").unwrap();
    flea::executable_file::write(&script, &std::fs::read(&script).unwrap(), 0o700).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tui::parity::tests::candidate::tui_parity_terminal_route_child",
            "--nocapture",
        ])
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("FLEA_PARITY_ROUTE_ROOT", &root)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        root.join("terminal.args").is_file(),
        "the child must actually dispatch Ctrl+T"
    );
}
