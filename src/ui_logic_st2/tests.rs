use super::manifest::{self, Invalid};
use super::pages::{self, Row, Update};
use super::schema::{self, Kind, Value};
use super::wire::{self, Listing, Outcome, Plugin, Reply, Source};
use super::{builtin, shelf, store};
use crate::i18n::{I18n, Paths};
use crate::jsondoc::{self, Json};
use std::path::PathBuf;

// English only: no Omarchy roster, no language file and no catalog root beside the compiled English.
fn english() -> I18n {
    I18n::load(Paths { omarchy: PathBuf::from("/nonexistent"), language_file: PathBuf::from("/nonexistent"), catalogs: Vec::new() })
}

pub const CHECKSUMS: &str = r#"{"schemaVersion":1,"id":"mtorvik.checksums","name":"Checksums","version":"0.2.0",
"author":"mtorvik","license":"MIT","description":"Compute and verify a file's checksum from the file menu.",
"kinds":["menu-action"],"entryPoints":{"run":"bin/flea-checksums"},
"menuActions":[{"id":"verify","label":"Verify Checksum…","glyph":"hash","when":{"minItems":1}}],
"access":["selection","exec:sha256sum","exec:sha1sum","exec:md5sum"],
"schema":[{"key":"sum","type":"choice","label":"Sum","options":[{"value":"sha256","label":"SHA-256"},{"value":"sha1","label":"SHA-1"},{"value":"md5","label":"MD5"}],"defaultValue":"sha256"},
{"key":"copy","type":"boolean","label":"Copy the sum to the clipboard","defaultValue":true}]}"#;

fn checksums(enabled: bool, failure: Option<Invalid>) -> Plugin {
    let m = manifest::parse(CHECKSUMS, false).unwrap();
    let settings = schema::settings(&m.schema, None);
    Plugin { manifest: m, source: Source::Git("https://github.com/mtorvik/flea-checksums".into()), enabled, failure, mark_path: String::new(), settings, shelf: None }
}

fn builtins(state: &Json) -> Vec<Plugin> {
    builtin::manifests()
        .into_iter()
        .map(|(m, on)| Plugin {
            enabled: store::enabled(state, &m.id, on), source: Source::Builtin, failure: None, mark_path: String::new(),
            settings: Vec::new(), shelf: (m.id == builtin::SHELF).then(|| shelf::input(state, Vec::new())), manifest: m,
        })
        .collect()
}

fn refusal(text: &str) -> &'static str {
    manifest::parse(text, false).unwrap_err().code
}

#[test]
fn st2_a_manifest_reads_the_spec_example_and_its_schema() {
    let m = manifest::parse(CHECKSUMS, false).unwrap();
    assert_eq!((m.id.as_str(), m.name.as_str(), m.version.as_str(), m.run.as_str()), ("mtorvik.checksums", "Checksums", "0.2.0", "bin/flea-checksums"));
    assert_eq!(m.glyph, "hash", "a row without its own SVG takes the glyph its block names");
    assert_eq!(m.schema.len(), 2);
    assert!(matches!(&m.schema[0].kind, Kind::Choice { default, options } if default == "sha256" && options.len() == 3));
}

#[test]
fn st2_a_manifest_refuses_each_broken_shape_by_its_own_code() {
    let swap = |from: &str, to: &str| CHECKSUMS.replacen(from, to, 1);
    assert_eq!(refusal("{"), "json");
    assert_eq!(refusal("[]"), "json");
    assert_eq!(refusal(&swap("\"schemaVersion\":1", "\"schemaVersion\":2")), "schema_version");
    assert_eq!(refusal(&swap("mtorvik.checksums", "Checksums")), "id");
    assert_eq!(refusal(&swap("mtorvik.checksums", "checksums")), "id", "an id is namespaced");
    assert_eq!(refusal(&swap("mtorvik.checksums", "flea.checksums")), "reserved");
    assert_eq!(refusal(&swap("\"kinds\":[\"menu-action\"]", "\"kinds\":[\"window\"]")), "kind");
    assert_eq!(refusal(&swap("\"kinds\":[\"menu-action\"]", "\"kinds\":[\"place\"]")), "block");
    assert_eq!(refusal(&swap("bin/flea-checksums", "../escape")), "run");
    assert_eq!(refusal(&swap("bin/flea-checksums", "/usr/bin/sh")), "run");
    assert_eq!(refusal(&swap("\"entryPoints\":{\"run\":\"bin/flea-checksums\"},", "")), "missing");
    assert_eq!(refusal(&swap("\"defaultValue\":\"sha256\"", "\"defaultValue\":\"crc\"")), "field");
    assert_eq!(refusal(&swap("\"key\":\"copy\"", "\"key\":\"sum\"")), "field", "a key is declared once");
    assert_eq!(refusal(&swap("\"version\":\"0.2.0\"", &format!("\"version\":\"{}\"", "9".repeat(65)))), "long");
    assert_eq!(refusal(&swap("\"kinds\":[\"menu-action\"]", "\"kinds\":[]")), "missing");
    assert!(manifest::parse(&swap("mtorvik.checksums", "flea.checksums"), true).is_ok(), "only a built-in takes flea.*");
}

#[test]
fn st2_every_stored_value_is_clamped_on_read() {
    let field = schema::field(&jsondoc::parse(r#"{"key":"t","type":"integer","label":"T","min":2,"max":60,"step":5,"defaultValue":12}"#).unwrap()).unwrap();
    let at = |raw: &str| schema::clamp(&field, Some(&jsondoc::parse(raw).unwrap()));
    assert_eq!(at("1"), Value::Int(2));
    assert_eq!(at("600"), Value::Int(60), "max stays reachable off the step grid");
    assert_eq!(at("13"), Value::Int(12), "a value lands on the nearest step");
    assert_eq!(at("\"9\""), Value::Int(12), "a wrong type falls back to the default");
    assert_eq!(schema::clamp(&field, None), Value::Int(12));
    let m = manifest::parse(CHECKSUMS, false).unwrap();
    let stored = jsondoc::parse(r#"{"sum":"crc","copy":"yes"}"#).unwrap();
    assert_eq!(schema::settings(&m.schema, Some(&stored)), vec![("sum".into(), Value::Choice("sha256".into())), ("copy".into(), Value::Bool(true))]);
    for bad in [r#"{"key":"x","type":"integer","label":"X","min":5,"max":1,"defaultValue":3}"#, r#"{"key":"x","type":"boolean","label":"X"}"#,
                r#"{"key":"x y","type":"boolean","label":"X","defaultValue":true}"#, r#"{"key":"x","type":"colour","label":"X","defaultValue":1}"#] {
        assert!(schema::field(&jsondoc::parse(bad).unwrap()).is_err(), "{bad}");
    }
}

#[test]
fn st2_the_shelf_switch_is_shelf_enabled_and_the_rest_ship_as_drawn() {
    let fresh = jsondoc::parse("{}").unwrap();
    let states: Vec<(String, bool)> = builtins(&fresh).iter().map(|p| (p.manifest.name.clone(), p.enabled)).collect();
    assert_eq!(states, [("Shelf".into(), false), ("LocalSend".into(), true), ("Tailscale".into(), true), ("Dropbox".into(), true),
                        ("AirDrop".into(), false)]);
    let on = jsondoc::parse(r#"{"shelf":{"enabled":true},"plugins":{"flea.dropbox":{"enabled":false}}}"#).unwrap();
    assert!(store::enabled(&on, builtin::SHELF, false));
    assert!(!store::enabled(&on, "flea.dropbox", true));
    assert!(!store::enabled(&fresh, "mtorvik.checksums", false), "an added plugin arrives off");
    assert_eq!(wire::compact(&store::enable_patch(builtin::SHELF, true)), r#"{"shelf":{"enabled":true}}"#);
    assert_eq!(wire::compact(&store::enable_patch("a.b", false)), r#"{"plugins":{"a.b":{"enabled":false}}}"#);
    assert_eq!(wire::compact(&store::setting_patch("a.b", "n", &Value::Int(3))), r#"{"plugins":{"a.b":{"settings":{"n":3}}}}"#);
    assert_eq!(wire::compact(&store::setting_patch(builtin::SHELF, "rail", &Value::Choice("left".into()))), r#"{"shelf":{"rail":"left"}}"#);
    assert_eq!(wire::compact(&store::forget_patch("a.b")), r#"{"plugins":{"a.b":null}}"#);
}

#[test]
fn st2_a_listing_and_an_outcome_survive_the_wire() {
    let mut plugins = builtins(&jsondoc::parse(r#"{"shelf":{"recent":5,"rail":"left"}}"#).unwrap());
    plugins[0].shelf.as_mut().unwrap().pins = shelf::pinned(r#"{"items":[{"path":"/home/gm/Invoices","folder":true,"pinned":true}]}"#);
    plugins.push(checksums(true, Some(Invalid::new("start", "bin/flea-checksums"))));
    let listing = Listing { id: 7, plugins };
    let line = wire::listing_line(&listing);
    assert!(!line.contains('\n'), "one wire line");
    let Ok(Reply::Listing(back)) = wire::parse(&line) else { panic!("{line}") };
    assert_eq!(back.id, 7);
    assert_eq!(back.plugins.len(), 6);
    assert_eq!(back.plugins[4].manifest.schema, listing.plugins[4].manifest.schema);
    assert_eq!(back.plugins[5].manifest.schema, listing.plugins[5].manifest.schema);
    assert_eq!(back.plugins[5].failure, Some(Invalid::new("start", "bin/flea-checksums")));
    assert_eq!(back.plugins[0].shelf, listing.plugins[0].shelf);
    let outcome = Outcome { id: 8, op: "add".into(), plugin: "a.b".into(), error: Some(Invalid::new("exists", "a.b")), version: String::new() };
    let Ok(Reply::Outcome(o)) = wire::parse(&wire::outcome_line(&outcome)) else { panic!() };
    assert_eq!(o, outcome);
    assert!(wire::parse(r#"{"t":"pluginop","id":1,"op":"add","plugin":"","ok":false,"code":"rm -rf","detail":"","version":""}"#).is_err(), "an unknown code is malformed");
    assert!(wire::parse(r#"{"t":"plugins","id":-1,"plugins":[]}"#).is_err());
    assert!(wire::parse(r#"{"t":"plugins","id":1,"plugins":[{"id":"a.b"}]}"#).is_err(), "a truncated plugin is malformed");
    assert!(wire::parse(r#"{"t":"rows"}"#).is_err());
    assert_eq!(wire::request_line(3, "set", "a.b", vec![("key", Json::Str("n".into())), ("value", Json::Num("4".into()))]),
               r#"{"c":"plugins","op":"set","id":3,"plugin":"a.b","key":"n","value":4}"#);
}

#[test]
fn st2_the_installed_list_is_the_board_rows_with_a_failure_in_its_own_word() {
    let i18n = english();
    let mut plugins = builtins(&jsondoc::parse("{}").unwrap());
    plugins.push(checksums(true, Some(Invalid::new("start", "bin/flea-checksums"))));
    let rows = pages::list_rows(&plugins, &i18n);
    assert_eq!(rows[0], Row::Group { id: "plugins.installed".into(), label: "Installed".into(), switch: None, action: Some("Add Plugin…".into()) });
    let drawn: Vec<(String, bool, Option<String>)> = rows[1..].iter().map(|r| match r {
        Row::Plugin { name, on, failure, .. } => (name.clone(), *on, failure.clone()),
        other => panic!("{other:?}"),
    }).collect();
    assert_eq!(drawn, [("Shelf".into(), false, None), ("LocalSend".into(), true, None), ("Tailscale".into(), true, None),
                       ("Dropbox".into(), true, None), ("AirDrop".into(), false, None),
                       ("Checksums".into(), true, Some("failed to start".into()))]);
    assert!(rows[1..].iter().all(Row::focusable));
}

#[test]
fn st2_a_git_plugin_page_is_the_board_sequence() {
    let i18n = english();
    let rows = pages::page_rows(&checksums(true, None), &Update::Current("0.2.0".into()), &i18n);
    assert_eq!(rows[0], Row::Description { text: "Compute and verify a file's checksum from the file menu.".into() });
    assert_eq!(rows[1], Row::Group { id: "mtorvik.checksums:enabled".into(), label: "Checksums".into(), switch: Some(true), action: None });
    assert!(matches!(&rows[2], Row::Choice { labels, selected, available: true, .. } if labels == &["SHA-256", "SHA-1", "MD5"] && selected == "sha256"));
    assert!(matches!(&rows[3], Row::Check { label, on: true, .. } if label == "Copy the sum to the clipboard"));
    assert_eq!(rows[5..7], [Row::Fact { text: "Reads the files you choose".into(), value: String::new() }, Row::Fact { text: "Runs sha256sum, sha1sum or md5sum".into(), value: String::new() }]);
    assert_eq!(rows[8], Row::Fact { text: "Installed from".into(), value: "github.com/mtorvik/flea-checksums".into() });
    assert!(matches!(&rows[9], Row::Action { id, value, chevron: true, .. } if id == pages::UPDATE_ID && value == "Up to date · 0.2.0"));
    assert!(matches!(&rows[10], Row::Action { label, chevron: false, .. } if label == "Remove plugin…"));
    assert_eq!(rows[11], Row::Rule);
    assert!(matches!(&rows[12], Row::Action { label, value, chevron: true, .. } if label == "Back to Plugins" && value == "Back"));
    let off = pages::page_rows(&checksums(false, None), &Update::Checking, &english());
    assert!(matches!(&off[2], Row::Choice { available: false, .. }), "a switched-off plugin greys its settings");
    assert!(matches!(&off[9], Row::Action { value, chevron: false, .. } if value == "Checking"), "Enter does nothing while it checks");
}

#[test]
fn st2_a_built_in_page_has_no_source_group() {
    let i18n = english();
    let localsend = &builtins(&jsondoc::parse("{}").unwrap())[1];
    let rows = pages::page_rows(localsend, &Update::Idle, &i18n);
    assert!(!rows.iter().any(|r| r.id() == pages::UPDATE_ID || r.id() == pages::REMOVE_ID));
    assert!(rows.contains(&Row::Fact { text: "Runs localsend".into(), value: String::new() }));
    assert!(rows.contains(&Row::Fact { text: "Uses the network".into(), value: String::new() }));
}

#[test]
fn st2_origins_show_without_scheme_or_suffix() {
    assert_eq!(pages::shown_origin("https://github.com/mtorvik/flea-checksums.git"), "github.com/mtorvik/flea-checksums");
    assert_eq!(pages::shown_origin("git@github.com:me/p.git"), "github.com/me/p");
    assert_eq!(pages::shown_origin("/home/gm/p/"), "/home/gm/p");
}

#[test]
fn st2_add_waits_for_text_and_refuses_while_busy() {
    let mut card = pages::AddCard::default();
    assert!(!card.can_add());
    card.text = "   ".into();
    assert!(!card.can_add());
    card.text = "https://github.com/mtorvik/flea-checksums".into();
    assert!(card.can_add());
    card.busy = true;
    assert!(!card.can_add());
}

#[test]
fn st2_every_refusal_code_has_its_sentence() {
    let i18n = english();
    for code in wire::FAILURE_CODES.iter().chain(super::ERROR_CODES.iter()) {
        let key = super::sentence_key(code);
        assert_ne!(i18n.tr(&key), key, "{key} has English");
    }
}

#[test]
fn st2_a_disabled_or_failed_plugin_contributes_nothing() {
    let state = jsondoc::parse("{}").unwrap();
    let mut plugins = builtins(&state);
    plugins.push(checksums(true, Some(Invalid::new("start", "bin/flea-checksums"))));
    plugins.push(checksums(false, None));
    let ids: Vec<&str> = pages::contributing(&plugins).map(|p| p.manifest.id.as_str()).collect();
    assert_eq!(ids, ["flea.localsend", "flea.tailscale", "flea.dropbox"], "the Shelf ships off, a failed start and a switched-off plugin draw no rows anywhere");
}
