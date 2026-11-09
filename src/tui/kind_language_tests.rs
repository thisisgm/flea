use crate::backend::kind::Kinds;
use crate::backend::{
    aliases::Aliases, icons::Names, listing::Listing, meta::Meta, mime::Db, rows, testdir::TestDir,
    thumbspec::Thumbnailers,
};
use crate::jsondoc::{self, Json};
use std::path::PathBuf;
use std::process::Command;

const MIME_ROOT: &str = "/usr/share/mime";

fn run(code: &str, name: &str) {
    if std::env::var("SOL_KIND_LANG_CHILD").as_deref() == Ok(code) {
        surfaces(code);
        return;
    }
    let root = TestDir::new("kind-lang");
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../i18n/fixtures/locales.json"),
    );
    root.file("language", code);
    for (language, text) in [
        ("en", include_str!("../../i18n/en/flea.json")),
        ("ja", include_str!("../../i18n/ja/flea.json")),
        ("de", include_str!("../../i18n/de/flea.json")),
        ("fr", include_str!("../../i18n/fr/flea.json")),
        ("ar", include_str!("../../i18n/ar/flea.json")),
    ] {
        std::fs::create_dir_all(root.join(&format!("catalog/{language}"))).unwrap();
        root.file(&format!("catalog/{language}/flea.json"), text);
    }
    let module = module_path!().split_once("::").unwrap().1;
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("{module}::{name}"), "--nocapture"])
        .env("SOL_KIND_LANG_CHILD", code)
        .env("OMARCHY_PATH", root.join("omarchy"))
        .env("OMARCHY_LANGUAGE_FILE", root.join("language"))
        .env("FLEA_I18N", root.join("catalog"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{code}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "child ran no test"
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}

// Sample input: <comment xml:lang="de">PNG-Bild</comment> supplies this fixture's expected value.
fn expected(mime: &str, code: &str) -> String {
    if mime == "text/markdown" && code == "ja" {
        return "Markdown 文書".into();
    }
    let xml =
        std::fs::read_to_string(PathBuf::from(MIME_ROOT).join(format!("{mime}.xml"))).unwrap();
    if code != "en" {
        let opening = format!("<comment xml:lang=\"{code}\">");
        if let Some((_, text)) = xml.split_once(&opening) {
            return text.split_once("</comment>").unwrap().0.into();
        }
    }
    xml.split_once("<comment>")
        .unwrap()
        .1
        .split_once("</comment>")
        .unwrap()
        .0
        .into()
}

fn surfaces(code: &str) {
    let (mime, icons) = (
        Db::from_str("50:image/png:*.png\n50:text/markdown:*.md\n50:application/pdf:*.pdf\n"),
        Names::from_str(""),
    );
    let aliases = Aliases::from_str("");
    let thumbs = Thumbnailers::from_entries(&[], &aliases);
    let mut kinds = Kinds::new();
    let mut listing = Listing::new();
    for name in ["photo.png", "todo.md", "paper.pdf"] {
        listing.push(name, false);
    }
    let meta = Meta {
        birth: None,
        size: 48,
        mtime: 0,
        mode: 0o100644,
        target_is_dir: false,
        target: String::new(),
        dev: 0,
    };
    let line = rows::rows_line(
        std::path::Path::new("/nonexistent"),
        &listing,
        &vec![meta; 3],
        0,
        0.0,
        &mime,
        &icons,
        &aliases,
        &thumbs,
        &mut kinds,
    );
    let reply = jsondoc::parse(&line).unwrap();
    let dictionary = reply.get("kinds").and_then(Json::as_array).unwrap();
    let rows = reply.get("rows").and_then(Json::as_array).unwrap();
    let catalog = flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: std::env::var_os("OMARCHY_PATH").unwrap().into(),
        language_file: std::env::var_os("OMARCHY_LANGUAGE_FILE").unwrap().into(),
        catalogs: vec![std::env::var_os("FLEA_I18N").unwrap().into()],
    });
    let mut failures = Vec::new();
    for (index, mime) in ["image/png", "text/markdown", "application/pdf"]
        .into_iter()
        .enumerate()
    {
        let expected = expected(mime, code);
        let row = crate::tui::model::Row::parse(&rows[index], dictionary);
        let gui_row = flea::ui_logic_preview_core::facts::Row {
            name: row.name.clone(),
            is_dir: false,
            size: 48,
            mtime: 0,
            mode: 0o100644,
            icon: row.icon.clone(),
        };
        let state =
            flea::ui_logic_preview_core::facts::state(Some(&gui_row), 1, false, "", &row.kind);
        let extra = flea::ui_logic_preview_core::facts::Extra::default();
        let facts = flea::ui_logic_preview_core::facts::facts(
            state, &gui_row, None, &row.kind, &extra, &catalog,
        );
        if facts[0].value != expected {
            failures.push(format!(
                "GUI {code} {mime}: {:?} != {expected:?}",
                facts[0].value
            ));
        }
        let compact = flea::ui_logic_preview_core::facts::compact_line(
            state,
            Some(&gui_row),
            None,
            &row.kind,
            &extra,
            &catalog,
        );
        if compact.0 != expected {
            failures.push(format!(
                "GUI compact {code} {mime}: {:?} != {expected:?}",
                compact.0
            ));
        }
        let mut model = crate::tui::model::Model::new("/nonexistent".into(), &Json::Null);
        model.rows.insert(0, row);
        model.preview_visible = true;
        crate::tui::preview::load(&mut model, false);
        if !model.preview[1].starts_with(&format!("{expected} · ")) {
            failures.push(format!(
                "TUI {code} {mime}: {:?} lacks {expected:?}",
                model.preview[1]
            ));
        }
        println!("{code} {mime}: GUI fact, GUI compact and TUI head = {expected}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn kind_lang_ten_thousand_rows_read_each_mime_once() {
    let (mime, icons) = (
        Db::from_str("50:image/png:*.png\n50:application/pdf:*.pdf\n"),
        Names::from_str(""),
    );
    let aliases = Aliases::from_str("");
    let thumbs = Thumbnailers::from_entries(&[], &aliases);
    let mut kinds = Kinds::new();
    let mut listing = Listing::new();
    for row in 0..10_000 {
        listing.push(
            &format!("{row}.{}", if row % 2 == 0 { "png" } else { "pdf" }),
            false,
        );
    }
    let meta = Meta {
        birth: None,
        size: 48,
        mtime: 0,
        mode: 0o100644,
        target_is_dir: false,
        target: String::new(),
        dev: 0,
    };
    let metas = vec![meta; 350];
    for start in (0..10_000).step_by(350) {
        let _ = rows::rows_line(
            std::path::Path::new("/nonexistent"),
            &listing,
            &metas,
            start,
            0.0,
            &mime,
            &icons,
            &aliases,
            &thumbs,
            &mut kinds,
        );
    }
    assert_eq!(
        kinds.reads(),
        2,
        "10k rows keep the original once-per-MIME XML reads"
    );
    println!(
        "10,000 rows, windows at most 350, XML reads = {}",
        kinds.reads()
    );
}

#[test]
fn kind_lang_japanese_gui_and_tui() {
    run("ja", "kind_lang_japanese_gui_and_tui");
}

#[test]
fn kind_lang_german_gui_and_tui() {
    run("de", "kind_lang_german_gui_and_tui");
}

#[test]
fn kind_lang_french_gui_and_tui() {
    run("fr", "kind_lang_french_gui_and_tui");
}

#[test]
fn kind_lang_arabic_pdf_gui_and_tui() {
    run("ar", "kind_lang_arabic_pdf_gui_and_tui");
}

#[test]
fn kind_lang_english_is_byte_identical_gui_and_tui() {
    run("en", "kind_lang_english_is_byte_identical_gui_and_tui");
}
