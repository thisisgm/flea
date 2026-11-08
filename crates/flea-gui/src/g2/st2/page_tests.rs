// The Plugins section driven by the backend's own reply lines: list, page, Add, Update, Remove, failure and stale replies.
use super::page::{Card, Key, PluginsPage};
use super::seam::{Effect, PluginsWire};
use flea::i18n::{I18n, Paths};
use flea::jsondoc;
use flea::ui_logic_st2::manifest::{self, Invalid};
use flea::ui_logic_st2::pages::{Row, BACK_ID, REMOVE_ID, UPDATE_ID};
use flea::ui_logic_st2::wire::{self, Listing, Outcome, Plugin, Source};
use flea::ui_logic_st2::{builtin, schema, shelf, store};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

pub(super) fn english() -> Arc<I18n> {
    Arc::new(I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent"),
        language_file: PathBuf::from("/nonexistent"),
        catalogs: Vec::new(),
    }))
}

/// The wire stand-in: records every request line, or refuses them all as a dead backend does.
#[derive(Default)]
pub(super) struct Recorder {
    pub lines: RefCell<Vec<String>>,
    pub dead: bool,
}

impl PluginsWire for Recorder {
    fn send(&self, line: String) -> Result<(), String> {
        if self.dead {
            return Err("backend gone".into());
        }
        self.lines.borrow_mut().push(line);
        Ok(())
    }
}

impl Recorder {
    pub fn last(&self) -> String {
        self.lines.borrow().last().cloned().unwrap_or_default()
    }

    // The id the last request carried, which its replies echo.
    pub fn last_id(&self) -> u64 {
        let doc = jsondoc::parse(&self.last()).unwrap();
        doc.get("id").and_then(|v| v.as_f64()).unwrap() as u64
    }
}

pub(super) const CHECKSUMS: &str = r#"{"schemaVersion":1,"id":"mtorvik.checksums","name":"Checksums","version":"0.2.0",
"author":"mtorvik","license":"MIT","description":"Compute and verify a file's checksum from the file menu.",
"kinds":["menu-action"],"entryPoints":{"run":"bin/flea-checksums"},
"menuActions":[{"id":"verify","label":"Verify Checksum…","glyph":"hash","when":{"minItems":1}}],
"access":["selection","exec:sha256sum","exec:sha1sum","exec:md5sum"],
"schema":[{"key":"sum","type":"choice","label":"Sum","options":[{"value":"sha256","label":"SHA-256"},{"value":"sha1","label":"SHA-1"},{"value":"md5","label":"MD5"}],"defaultValue":"sha256"},
{"key":"copy","type":"boolean","label":"Copy the sum to the clipboard","defaultValue":true}]}"#;

pub(super) fn checksums(enabled: bool, failure: Option<Invalid>) -> Plugin {
    let m = manifest::parse(CHECKSUMS, false).unwrap();
    let settings = schema::settings(&m.schema, None);
    Plugin {
        manifest: m,
        source: Source::Git("https://github.com/mtorvik/flea-checksums".into()),
        enabled,
        failure,
        mark_path: String::new(),
        settings,
        shelf: None,
    }
}

pub(super) fn builtins(state: &str) -> Vec<Plugin> {
    let state = jsondoc::parse(state).unwrap();
    builtin::manifests()
        .into_iter()
        .map(|(m, on)| Plugin {
            enabled: store::enabled(&state, &m.id, on),
            source: Source::Builtin,
            failure: None,
            mark_path: String::new(),
            settings: Vec::new(),
            shelf: (m.id == builtin::SHELF).then(|| shelf::input(&state, Vec::new())),
            manifest: m,
        })
        .collect()
}

pub(super) fn listing(id: u64, plugins: Vec<Plugin>) -> String {
    wire::listing_line(&Listing { id, plugins })
}

fn outcome(id: u64, op: &str, plugin: &str, error: Option<Invalid>, version: &str) -> String {
    wire::outcome_line(&Outcome {
        id,
        op: op.into(),
        plugin: plugin.into(),
        error,
        version: version.into(),
    })
}

pub(super) fn labels(page: &PluginsPage) -> Vec<String> {
    page.rows()
        .iter()
        .map(|r| match r {
            Row::Group { label, .. } => format!("#{label}"),
            Row::Plugin {
                name,
                failure: Some(word),
                ..
            } => format!("{name} ({word})"),
            Row::Plugin { name, .. }
            | Row::Check { label: name, .. }
            | Row::Choice { label: name, .. }
            | Row::Action { label: name, .. } => name.clone(),
            Row::Fact { text, value } if value.is_empty() => text.clone(),
            Row::Fact { text, value } => format!("{text}: {value}"),
            Row::Description { text } | Row::Hint { text } => format!("~{text}"),
            Row::Rule => "--".into(),
            other => format!("{other:?}"),
        })
        .collect()
}

fn cursor_id(page: &PluginsPage) -> String {
    page.rows()[page.cursor().unwrap()].id().to_string()
}

pub(super) fn opened(wire: &Recorder, plugins: Vec<Plugin>) -> PluginsPage {
    let mut page = PluginsPage::new(english());
    page.start(wire);
    assert!(page.receive(&listing(wire.last_id(), plugins)));
    page
}

pub(super) fn go(page: &mut PluginsPage, wire: &Recorder, id: &str) {
    let index = page
        .rows()
        .iter()
        .position(|r| r.id() == id)
        .unwrap_or_else(|| panic!("no row {id}"));
    while page.cursor() != Some(index) {
        let before = page.cursor();
        page.key(
            if before.unwrap() < index {
                Key::Down
            } else {
                Key::Up
            },
            wire,
        );
        assert_ne!(page.cursor(), before, "the cursor reaches {id}");
    }
}

#[test]
fn st2_gui_the_installed_list_is_the_board_and_ships_the_shelf_off() {
    let wire = Recorder::default();
    let mut page = PluginsPage::new(english());
    page.start(&wire);
    assert_eq!(wire.last(), r#"{"c":"plugins","op":"list","id":1}"#);
    assert!(
        page.rows().is_empty() && page.cursor().is_none(),
        "nothing is drawn before the backend answers"
    );
    assert!(page.receive(&listing(1, builtins("{}"))));
    assert_eq!(
        labels(&page).join(" "),
        "#Installed Shelf LocalSend Tailscale Dropbox AirDrop"
    );
    assert_eq!(page.title(), "Plugins");
    let on: Vec<bool> = page
        .rows()
        .iter()
        .filter_map(|r| match r {
            Row::Plugin { on, .. } => Some(*on),
            _ => None,
        })
        .collect();
    assert_eq!(
        on,
        [false, true, true, true, false],
        "LocalSend, Tailscale and Dropbox on, the Shelf and AirDrop off"
    );
    assert!(matches!(&page.rows()[0], Row::Group { action: Some(a), .. } if a == "Add Plugin…"));
    assert_eq!(
        cursor_id(&page),
        "plugins.installed",
        "the heading carrying Add Plugin… is the first stop"
    );
}

#[test]
fn st2_gui_space_switches_a_plugin_and_enter_opens_its_page_and_esc_returns() {
    let wire = Recorder::default();
    let mut page = opened(&wire, builtins("{}"));
    page.key(Key::Down, &wire);
    assert_eq!(cursor_id(&page), "flea.shelf");
    assert_eq!(page.key(Key::Space, &wire), Effect::None);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"enable","id":2,"plugin":"flea.shelf"}"#
    );
    page.key(Key::Down, &wire);
    page.key(Key::Enter, &wire);
    assert_eq!(page.title(), "LocalSend");
    assert_eq!(
        page.rows()[0],
        Row::Description {
            text: "Send files to nearby devices with LocalSend.".into()
        }
    );
    assert!(
        !page
            .rows()
            .iter()
            .any(|r| r.id() == UPDATE_ID || r.id() == REMOVE_ID),
        "a built-in has no Update or Remove"
    );
    assert_eq!(page.key(Key::Escape, &wire), Effect::None);
    assert_eq!(
        (page.title().as_str(), cursor_id(&page).as_str()),
        ("Plugins", "flea.localsend"),
        "esc returns to the row it opened from"
    );
    assert_eq!(
        page.key(Key::Escape, &wire),
        Effect::Close,
        "esc on the list is the host's"
    );
}

#[test]
fn st2_gui_stale_malformed_and_unasked_replies_change_nothing() {
    let wire = Recorder::default();
    let mut page = opened(&wire, builtins("{}"));
    page.key(Key::Down, &wire);
    page.key(Key::Space, &wire);
    let switched = wire.last_id();
    assert!(page.receive(&listing(
        switched,
        builtins(r#"{"shelf":{"enabled":true}}"#)
    )));
    assert!(
        !page.receive(&listing(switched - 1, builtins("{}"))),
        "an older listing never overwrites a newer one"
    );
    assert!(matches!(&page.rows()[1], Row::Plugin { on: true, .. }));
    for line in [
        "",
        "{",
        r#"{"t":"plugins","id":9}"#,
        r#"{"t":"pluginop","id":9,"op":"add","plugin":"","ok":"yes"}"#,
        r#"{"t":"list"}"#,
    ] {
        assert!(!page.receive(line), "{line:?} is not a plugins reply");
    }
    assert!(
        !page.receive(&outcome(77, "add", "a.b", None, "")),
        "an outcome nobody asked for is dropped"
    );
    assert_eq!(labels(&page).len(), 6);
}

#[test]
fn st2_gui_add_waits_for_text_shows_the_refusal_and_lands_on_the_new_plugin_switched_off() {
    let wire = Recorder::default();
    let mut page = opened(&wire, builtins("{}"));
    page.key(Key::Enter, &wire);
    assert!(matches!(page.card(), Card::Add(card) if card.text.is_empty()));
    let sent = wire.lines.borrow().len();
    page.key(Key::Enter, &wire);
    assert_eq!(
        wire.lines.borrow().len(),
        sent,
        "Add is unavailable until something is typed"
    );
    page.key(Key::Text("file:///nowhere".into()), &wire);
    page.key(Key::Enter, &wire);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"add","id":2,"source":"file:///nowhere"}"#
    );
    assert!(matches!(page.card(), Card::Add(card) if card.busy && !card.can_add()));
    page.key(Key::Text("x".into()), &wire);
    assert!(
        matches!(page.card(), Card::Add(card) if card.text == "file:///nowhere"),
        "a busy card takes no typing"
    );
    assert!(page.receive(&outcome(
        2,
        "add",
        "",
        Some(Invalid::new("git", "file:///nowhere")),
        ""
    )));
    assert!(
        matches!(page.card(), Card::Add(card) if !card.busy && card.error.as_deref() == Some("Git could not fetch file:///nowhere"))
    );
    page.key(Key::Backspace, &wire);
    assert!(
        matches!(page.card(), Card::Add(card) if card.error.is_none() && card.text == "file:///nowher"),
        "editing clears the refusal"
    );
    page.key(Key::Escape, &wire);
    assert_eq!(*page.card(), Card::None);
    page.key(Key::Enter, &wire);
    page.key(
        Key::Text("https://github.com/mtorvik/flea-checksums".into()),
        &wire,
    );
    page.key(Key::Enter, &wire);
    let id = wire.last_id();
    assert!(page.receive(&outcome(id, "add", "mtorvik.checksums", None, "0.2.0")));
    assert_eq!(*page.card(), Card::None);
    let mut plugins = builtins("{}");
    plugins.push(checksums(false, None));
    assert!(page.receive(&listing(id, plugins)));
    assert_eq!(cursor_id(&page), "mtorvik.checksums");
    assert!(
        matches!(&page.rows()[5], Row::Plugin { on: false, .. }),
        "a plugin added joins the list switched off"
    );
}

#[test]
fn st2_gui_a_cancelled_add_ignores_its_late_answer() {
    let wire = Recorder::default();
    let mut page = opened(&wire, builtins("{}"));
    page.key(Key::Enter, &wire);
    page.key(Key::Text("/home/gm/plugins/x".into()), &wire);
    page.key(Key::Enter, &wire);
    let id = wire.last_id();
    page.key(Key::Escape, &wire);
    assert!(!page.receive(&outcome(
        id,
        "add",
        "",
        Some(Invalid::new("not_found", "/home/gm/plugins/x")),
        ""
    )));
    assert_eq!(
        *page.card(),
        Card::None,
        "no card reopens for an answer to a card already closed"
    );
}

#[test]
fn st2_gui_the_git_page_is_the_board_and_its_controls_send_the_set_requests() {
    let wire = Recorder::default();
    let mut plugins = builtins("{}");
    plugins.push(checksums(true, None));
    let mut page = opened(&wire, plugins);
    go(&mut page, &wire, "mtorvik.checksums");
    page.key(Key::Enter, &wire);
    assert_eq!(page.title(), "Checksums");
    assert_eq!(
        labels(&page),
        [
            "~Compute and verify a file's checksum from the file menu.",
            "#Checksums",
            "Sum",
            "Copy the sum to the clipboard",
            "#Access",
            "Reads the files you choose",
            "Runs sha256sum, sha1sum or md5sum",
            "#Source",
            "Installed from: github.com/mtorvik/flea-checksums",
            "Update",
            "Remove plugin…",
            "--",
            "Back to Plugins",
        ]
    );
    assert_eq!(
        cursor_id(&page),
        "mtorvik.checksums:enabled",
        "the heading's switch is the first stop"
    );
    page.key(Key::Down, &wire);
    page.key(Key::Right, &wire);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"set","id":2,"plugin":"mtorvik.checksums","key":"sum","value":"sha1"}"#
    );
    page.key(Key::Left, &wire);
    assert_eq!(
        wire.lines.borrow().len(),
        2,
        "Left at the first value stays there"
    );
    page.pick(2, "md5", &wire);
    assert!(wire.last().ends_with(r#""key":"sum","value":"md5"}"#));
    page.key(Key::Down, &wire);
    page.key(Key::Space, &wire);
    assert!(wire.last().ends_with(r#""key":"copy","value":false}"#));
    page.key(Key::Up, &wire);
    page.key(Key::Up, &wire);
    page.key(Key::Space, &wire);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"disable","id":5,"plugin":"mtorvik.checksums"}"#,
        "the heading's switch is the plugin's enable"
    );
}

#[test]
fn st2_gui_update_checks_then_shows_the_version_or_could_not_check() {
    let wire = Recorder::default();
    let mut plugins = builtins("{}");
    plugins.push(checksums(true, None));
    let mut page = opened(&wire, plugins);
    go(&mut page, &wire, "mtorvik.checksums");
    page.key(Key::Enter, &wire);
    let value = |page: &PluginsPage| {
        page.rows()
            .iter()
            .find_map(|r| match r {
                Row::Action { id, value, .. } if id == UPDATE_ID => Some(value.clone()),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(
        value(&page),
        "Check",
        "nothing reaches the network when the page opens"
    );
    go(&mut page, &wire, UPDATE_ID);
    page.key(Key::Enter, &wire);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"update","id":2,"plugin":"mtorvik.checksums"}"#
    );
    assert_eq!(value(&page), "Checking");
    assert!(page.receive(&outcome(2, "update", "mtorvik.checksums", None, "0.2.0")));
    assert_eq!(value(&page), "Up to date · 0.2.0");
    page.key(Key::Enter, &wire);
    assert!(page.receive(&outcome(
        3,
        "update",
        "mtorvik.checksums",
        Some(Invalid::new("git", "origin")),
        ""
    )));
    assert_eq!(value(&page), "Could not check");
}

#[test]
fn st2_gui_remove_asks_first_then_returns_to_the_list() {
    let wire = Recorder::default();
    let mut plugins = builtins("{}");
    plugins.push(checksums(true, None));
    let mut page = opened(&wire, plugins);
    go(&mut page, &wire, "mtorvik.checksums");
    page.key(Key::Enter, &wire);
    go(&mut page, &wire, REMOVE_ID);
    let sent = wire.lines.borrow().len();
    page.key(Key::Enter, &wire);
    assert!(matches!(page.card(), Card::Remove { name, busy: false, .. } if name == "Checksums"));
    assert_eq!(
        wire.lines.borrow().len(),
        sent,
        "Remove plugin… asks before it removes"
    );
    page.key(Key::Enter, &wire);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"remove","id":2,"plugin":"mtorvik.checksums"}"#
    );
    assert!(page.receive(&outcome(
        2,
        "remove",
        "mtorvik.checksums",
        Some(Invalid::new("write", "busy disk")),
        ""
    )));
    assert!(
        matches!(page.card(), Card::Remove { busy: false, error: Some(e), .. } if e == "The plugin could not be saved: busy disk")
    );
    page.key(Key::Enter, &wire);
    assert!(page.receive(&outcome(3, "remove", "mtorvik.checksums", None, "")));
    assert!(page.receive(&listing(3, builtins("{}"))));
    assert_eq!(
        (page.title().as_str(), labels(&page).len(), page.card()),
        ("Plugins", 6, &Card::None)
    );
}

#[test]
fn st2_gui_a_failed_plugin_keeps_its_row_word_and_page() {
    let wire = Recorder::default();
    let mut plugins = builtins("{}");
    plugins.push(checksums(
        true,
        Some(Invalid::new("start", "bin/flea-checksums")),
    ));
    let mut page = opened(&wire, plugins);
    assert_eq!(labels(&page)[6], "Checksums (failed to start)");
    go(&mut page, &wire, "mtorvik.checksums");
    page.key(Key::Enter, &wire);
    assert_eq!(
        page.title(),
        "Checksums",
        "its chevron stays, so Enter still opens its page"
    );
    assert!(
        page.rows().iter().any(|r| r.id() == UPDATE_ID)
            && page.rows().iter().any(|r| r.id() == REMOVE_ID)
    );
}

#[test]
fn st2_gui_the_shelf_page_holds_its_rows_greyed_until_its_one_switch_is_on() {
    let wire = Recorder::default();
    let mut page = opened(&wire, builtins("{}"));
    go(&mut page, &wire, "flea.shelf");
    page.key(Key::Enter, &wire);
    assert_eq!(
        labels(&page),
        [
            "~Collect files from anywhere into one pile on the bar.",
            "#Shelf",
            "Show in bar",
            "Edge rail",
            "~Super+D opens it from anywhere once you add the bind from the README.",
            "#Recent captures",
            "Screenshots",
            "Recordings",
            "Show",
            "#Pinned",
            "#Access",
            "Reads the files you put on the shelf",
            "Adds the shelf to the Omarchy bar",
            "Watches your screenshot and recording folders",
            "--",
            "Back to Plugins",
        ]
    );
    go(&mut page, &wire, "shelf.bar");
    let sent = wire.lines.borrow().len();
    page.key(Key::Space, &wire);
    assert_eq!(wire.lines.borrow().len(), sent, "a greyed row does nothing");
    go(&mut page, &wire, "shelf.enabled");
    page.key(Key::Space, &wire);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"enable","id":2,"plugin":"flea.shelf"}"#,
        "the SHELF heading's switch is the plugin's enable"
    );
    assert!(page.receive(&listing(2, builtins(r#"{"shelf":{"enabled":true}}"#))));
    go(&mut page, &wire, "shelf.rail");
    page.key(Key::Right, &wire);
    assert_eq!(
        wire.last(),
        r#"{"c":"plugins","op":"set","id":3,"plugin":"flea.shelf","key":"rail","value":"left"}"#
    );
    go(&mut page, &wire, "pinFolder");
    assert_eq!(page.key(Key::Enter, &wire), Effect::PinFolder);
    go(&mut page, &wire, BACK_ID);
    page.key(Key::Enter, &wire);
    assert_eq!(cursor_id(&page), "flea.shelf");
}

#[test]
fn st2_gui_a_dead_backend_settles_every_busy_state() {
    let mut wire = Recorder::default();
    let mut plugins = builtins("{}");
    plugins.push(checksums(true, None));
    let mut page = opened(&wire, plugins);
    go(&mut page, &wire, "mtorvik.checksums");
    page.key(Key::Enter, &wire);
    go(&mut page, &wire, UPDATE_ID);
    page.key(Key::Enter, &wire);
    page.failed();
    assert!(page
        .rows()
        .iter()
        .any(|r| matches!(r, Row::Action { value, .. } if value == "Could not check")));
    assert!(
        !page.receive(&outcome(2, "update", "mtorvik.checksums", None, "0.2.0")),
        "nothing pending survives the backend"
    );
    page.key(Key::Escape, &wire);
    go(&mut page, &wire, "plugins.installed");
    page.key(Key::Enter, &wire);
    page.key(Key::Text("/x".into()), &wire);
    wire.dead = true;
    page.key(Key::Enter, &wire);
    assert!(
        matches!(page.card(), Card::Add(card) if !card.busy),
        "a request the backend never took leaves the card usable"
    );
}
