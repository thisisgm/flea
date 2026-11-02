use super::*;
use crate::{
    i18n::Paths,
    jsondoc::Json,
    tui::{cells, editor::Editor, keymap::Map, model::Model, render, theme::Theme},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) struct Fixture(PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/m9-tui-12");
        std::fs::create_dir_all(&parent).unwrap();
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("words.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = Self(PathBuf::from(
            String::from_utf8(made.stdout).unwrap().trim(),
        ));
        std::fs::write(root.0.join(".flea-test-sandbox"), "TUI12 words fixture\n").unwrap();
        root.write(
            "omarchy/default/i18n/locales.json",
            include_str!("../i18n/fixtures/locales.json"),
        );
        root.write("language", "en\n");
        root.write("catalog/en/flea.json", r#"{"tui.frame.keys":"%1 keys","tui.frame.selected":{"one":"%1 selected","other":"%1 selected"},"tui.layout.make_the_window_at_least_12_columns_wide_and_5_rows_tall":"Make the window at least 12 columns wide and 5 rows tall."}"#);
        root.write("catalog/ru/flea.json", r#"{"items.count":{"one":"%1 предмет","few":"%1 предмета","many":"%1 предметов","other":"%1 предмета"},"prompt.rename":"Переименовать","keys.title":"Сочетания клавиш","window.column_name":"Имя","tui.frame.keys":"%1 клавиши","paste.nothing":"Буфер пуст","paste.waiting":"Дождитесь ответа"}"#);
        root.write("catalog/ja/flea.json", r#"{"items.count":{"other":"%1 項目"},"window.column_name":"名前","prompt.rename":"名前を変更する","keys.title":"キーバインド","keys.keymapSheet":"キーバインド","tui.frame.keys":"%1 キーバインド","paste.nothing":"何もありません","paste.waiting":"回答を待ってください"}"#);
        root
    }
    pub(super) fn write(&self, name: &str, body: &str) {
        assert!(
            self.0.is_absolute()
                && self.0.join(".flea-test-sandbox").is_file()
                && Path::new(name)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
        );
        let file = self.0.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let replacement = file.with_file_name(".words-replacement");
        std::fs::write(&replacement, body).unwrap();
        std::fs::rename(replacement, file).unwrap();
    }
    fn catalog(&self) -> I18n {
        I18n::load(Paths {
            omarchy: self.0.join("omarchy"),
            language_file: self.0.join("language"),
            catalogs: vec![self.0.join("catalog")],
        })
    }
    pub(super) fn model(&self) -> Model {
        let mut model = Model::new("/fx/写真".into(), &Json::Null);
        model.i18n = self.catalog();
        model.prompts.i18n = model.i18n.clone();
        model.columns = 130;
        model.height = 27;
        model.hints = crate::tui::keymap::Hints::new(&Map::load(), "default");
        model
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/m9-tui-12");
        assert!(
            self.0.is_absolute()
                && !self.0.as_os_str().is_empty()
                && self.0.starts_with(&parent)
                && self.0 != parent
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn tui_words_frame_counts_use_shared_plural_and_locale() {
    let f = Fixture::new();
    let mut m = f.model();
    f.write("language", "ru\n");
    assert!(m.refresh_locale(&[]));
    m.total = 22;
    let line = crate::tui::layout::footer(&m, &Theme::from_text(""), 130, Duration::ZERO);
    assert!(line.contains("22 предмета"), "{line}");
    m.total = 25;
    assert!(
        crate::tui::layout::footer(&m, &Theme::from_text(""), 130, Duration::ZERO)
            .contains("25 предметов")
    );
    f.write("language", "ja\n");
    assert!(m.refresh_locale(&[]));
    m.total = 12;
    assert!(
        crate::tui::layout::footer(&m, &Theme::from_text(""), 130, Duration::ZERO)
            .contains("12 項目")
    );
    assert_eq!(m.path, Path::new("/fx/写真"));
    let (mut wire, reader) = crate::tui::wire::echo_wire();
    m.pending = None;
    m.total = 2;
    m.rows
        .insert(0, render::frame_tests::entry("写真", true, 1));
    m.rows
        .insert(1, render::frame_tests::entry("見積書.pdf", false, 2));
    crate::tui::actions::key(
        &mut m,
        &crate::tui::input::Key::character('j', ""),
        &Map::load(),
        &mut wire,
    )
    .unwrap();
    assert_eq!(m.cursor, 1);
    crate::tui::wire::finish(wire, reader);
}

#[test]
fn tui_words_prompt_and_sheet_fit_display_cells() {
    let f = Fixture::new();
    let mut m = f.model();
    f.write("language", "ja\n");
    m.refresh_locale(&[]);
    m.editor = Some(Editor::new(
        "rename",
        "見積書_e\u{301}.pdf".into(),
        "/fx/写真/見積書_e\u{301}.pdf".into(),
    ));
    let theme = Theme::from_text("");
    for columns in [0, 1, 5, 12, 27, 130] {
        let (line, caret) = crate::tui::prompts::status(&m, &theme, columns, Duration::ZERO);
        assert_eq!(
            cells::text_width(&render::frame_tests::plain(&line)),
            columns
        );
        assert!(caret.map_or(true, |caret| caret < columns));
        m.columns = columns;
        let card = crate::tui::render::key_sheet::card(&m, &Map::load());
        if columns >= 12 {
            assert!(card.x + card.width + 2 <= columns);
        }
    }
    assert_eq!(m.editor.as_ref().unwrap().value, "見積書_e\u{301}.pdf");
}

#[test]
fn tui_words_live_switch_refreshes_held_labels_atomically() {
    let f = Fixture::new();
    let mut m = f.model();
    m.say_key("paste.nothing");
    m.fail_key("paste.nothing");
    m.fail_key("paste.waiting");
    let at = m.message_at;
    let callback: fn(&mut Model) = |m| {
        assert_eq!(m.i18n.language(), "ja");
        assert_eq!(m.prompts.i18n.language(), "ja");
        assert_eq!(m.message, "何もありません");
        assert_eq!(m.error, "何もありません");
        assert_eq!(m.errors.front().unwrap(), "回答を待ってください");
        assert_eq!(
            m.locale_revision, 0,
            "revision publishes after all projections"
        );
    };
    f.write("language", "ja\n");
    assert!(m.refresh_locale(&[callback]));
    assert_eq!(m.locale_revision, 1);
    m.sheet = true;
    let frame = render::frame(
        &m,
        &Theme::from_text(""),
        &Map::load(),
        130,
        31,
        Duration::ZERO,
    );
    assert!(frame.contains("キーバインド") && !frame.contains("Keyboard Shortcuts"));
    assert_eq!(m.message_at, at);
    assert!(!m.refresh_locale(&[|_| panic!("no duplicate refresh")]));
    m.message = "external diagnostic 2025.pdf".into();
    f.write("language", "ru\n");
    assert!(m.refresh_locale(&[]));
    assert_eq!(m.message, "external diagnostic 2025.pdf");
    m.dismiss_error();
    assert_eq!(m.error, "Дождитесь ответа");
}

#[test]
fn tui_words_keycaps_follow_shared_layout_policy() {
    let f = Fixture::new();
    let mut m = f.model();
    let map = Map::load();
    let snapshot = flea::keys::display::KeycapSnapshot::new("ru", None, None, 7).unwrap();
    m.set_keycaps(snapshot, &map).unwrap();
    assert_eq!(
        m.keycap_labels["j / down"],
        flea::keys::display::chord_label("j / down", &snapshot)
            .unwrap()
            .text
    );
    let before = m.keycap_labels.clone();
    f.write("language", "ja\n");
    m.refresh_locale(&[]);
    assert_eq!(
        m.keycap_labels, before,
        "language cannot select a keyboard layout"
    );
    assert_eq!(
        map.action(&crate::tui::input::Key::character('j', ""), "default"),
        "cursorDown"
    );
    assert!(flea::keys::display::KeycapSnapshot::new("unknown", None, None, 8).is_err());
}

#[test]
fn tui_words_rtl_frame_maps_cells_without_reversing_data() {
    let f = Fixture::new();
    let mut m = f.model();
    f.write("language", "ar\n");
    assert!(m.refresh_locale(&[]));
    assert!(crate::tui::layout::rtl(&m));
    for start in 0..m.columns {
        let visual = crate::tui::layout::visual_start(&m, start, 1);
        assert_eq!(visual, start, "HIG tui-lang-ar keeps the pane coordinates");
    }
    let entry = render::frame_tests::entry("تقرير_2025.pdf", false, 1280);
    let theme = Theme::from_text("");
    for width in [0, 1, 4, 5, 12, 51] {
        let out = cells::row(
            &entry,
            width,
            Some("1.3 kB"),
            true,
            cells::RowStyle {
                rtl: true,
                cursor: true,
                focused: true,
                ..Default::default()
            },
            &theme,
            "2025",
        );
        let shown = cells::clean(&render::frame_tests::plain(&out));
        assert_eq!(cells::text_width(&shown), width, "{shown:?}");
        if width == 51 {
            assert!(shown.contains("تقرير_2025.pdf"));
        }
    }
    let date = "2025-06-14T09:30:59";
    assert_eq!(
        crate::tui::calendar::localized_value(&Json::Str(date.into()), m.i18n.locale()),
        "2025-06-14 09:30"
    );
    assert_eq!(entry.name, "تقرير_2025.pdf");
}

#[test]
fn tui_words_review_raw_error_identity_survives_colliding_translations() {
    let f = Fixture::new();
    let mut m = f.model();
    m.fail_key("paste.nothing");
    m.fail("何もありません".into());
    assert_eq!(m.errors.len(), 1);
    f.write("language", "ja\n");
    assert!(m.refresh_locale(&[]));
    assert_eq!(m.error, "何もありません");
    assert_eq!(m.errors.front().unwrap(), "何もありません");
    f.write("language", "ru\n");
    assert!(m.refresh_locale(&[]));
    assert_eq!(m.error, "Буфер пуст");
    m.dismiss_error();
    assert_eq!(
        m.error, "何もありません",
        "external diagnostics retain their own identity"
    );
}

#[test]
fn tui_words_review_dismissed_errors_release_semantic_storage() {
    let f = Fixture::new();
    let mut m = f.model();
    for _ in 0..100 {
        m.fail_key("paste.nothing");
        m.fail_key("paste.waiting");
        m.dismiss_error();
        m.dismiss_error();
        assert!(m.error.is_empty() && m.errors.is_empty());
        assert_eq!(
            m.words.retained_errors(),
            0,
            "dismissal releases semantic labels without a locale switch"
        );
    }
}

#[test]
fn tui_words_review_successful_launch_releases_held_label() {
    let f = Fixture::new();
    let mut m = f.model();
    let mut child = std::process::Command::new("true")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    assert!(child.wait().unwrap().success());
    m.launched("keys.openTerminal", child);
    assert_eq!(m.words.launches.len(), 1);
    m.poll_launches();
    assert!(m.launches.is_empty());
    assert_eq!(
        m.words.launches.len(),
        0,
        "success cleans its held label without a language change"
    );
    assert!(m.error.is_empty());
}

#[test]
fn tui_words_review_taildrop_fallback_keeps_semantic_identity() {
    let f = Fixture::new();
    f.write(
        "catalog/en/flea.json",
        r#"{"tui.model.taildrop_device_is_unreachable":"Taildrop device is unreachable"}"#,
    );
    f.write(
        "catalog/ja/flea.json",
        r#"{"tui.model.taildrop_device_is_unreachable":"到達できません"}"#,
    );
    let mut m = f.model();
    let (mut wire, reader) = crate::tui::wire::echo_wire();
    m.menu_action = "taildropRefresh".into();
    m.advance_taildrop(&mut wire).unwrap();
    crate::tui::wire::finish(wire, reader);
    assert_eq!(m.error, "Taildrop device is unreachable");
    f.write("language", "ja\n");
    assert!(m.refresh_locale(&[]));
    assert_eq!(m.error, "到達できません");
    assert!(m.menu_action.is_empty() && m.taildrop_target.is_none());
}

#[test]
fn tui_words_review_permission_digits_use_locale() {
    let f = Fixture::new();
    let mut m = f.model();
    let mut entry = render::frame_tests::entry("mode.txt", false, 1);
    entry.mode = 0o100755;
    m.detail.mode = crate::tui::detail::Column::Mode;
    assert_eq!(crate::tui::detail::value(&m, 0, &entry), "0755");
    f.write("language", "fa\n");
    assert!(m.refresh_locale(&[]));
    assert_eq!(crate::tui::detail::value(&m, 0, &entry), "۰۷۵۵");
    assert_eq!(entry.mode, 0o100755);
}
