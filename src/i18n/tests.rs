use super::*;
use std::fs;
use std::process::Command;

#[test]
fn kind_lang_mime_override_never_uses_english_fallback() {
    let root = crate::backend::testdir::TestDir::new("mime-override");
    fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    fs::create_dir_all(root.join("catalog/ja")).unwrap();
    root.file("omarchy/default/i18n/locales.json", include_str!("fixtures/locales.json"));
    root.file("language", "de");
    let paths = Paths { omarchy: root.join("omarchy"), language_file: root.join("language"), catalogs: vec![root.join("catalog")] };
    let mut catalog = I18n::load(paths);
    assert_eq!(catalog.mime_comment("text/markdown"), None);
    root.file("language", "ja");
    root.file("catalog/ja/flea.json", r#"{"mime.markdown":"Markdown 文書"}"#);
    assert!(catalog.poll_changed());
    assert_eq!(catalog.mime_comment("text/markdown"), Some("Markdown 文書"));
    assert_eq!(catalog.mime_comment("image/png"), None);
    root.file("catalog/ja/flea.json", r#"{"mime.markdown":""}"#);
    assert!(catalog.poll_changed());
    assert_eq!(catalog.mime_comment("text/markdown"), None);
}

// All mutations, including renames and cleanup, stay in a marked mktemp root owned by this test.
pub(super) struct Fixture(PathBuf);

impl Fixture {
    pub(super) fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/l1");
        fs::create_dir_all(&base).unwrap();
        let output = Command::new("mktemp").arg("-d").arg(base.join("i18n.XXXXXX")).output().unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with(&base) && path != base);
        fs::write(path.join(".i18n-test-root"), b"flea i18n test\n").unwrap();
        Self(path)
    }

    pub(super) fn path(&self, name: &str) -> PathBuf {
        assert!(self.0.is_absolute() && self.0.join(".i18n-test-root").is_file());
        assert!(Path::new(name).components().all(|c| matches!(c, std::path::Component::Normal(_))));
        self.0.join(name)
    }

    pub(super) fn write(&self, name: &str, text: &str) {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    pub(super) fn paths(&self) -> Paths {
        Paths {
            omarchy: self.path("omarchy"), language_file: self.path("config/omarchy/language"),
            catalogs: vec![self.path("installed/i18n"), self.path("source/i18n")],
        }
    }

    pub(super) fn registry(&self) {
        self.write("omarchy/default/i18n/locales.json", include_str!("fixtures/locales.json"));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/l1");
        assert!(self.0.is_absolute() && !self.0.as_os_str().is_empty() && self.0.starts_with(&base) && self.0 != base);
        assert!(self.0.join(".i18n-test-root").is_file());
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn missing_or_unknown_language_is_english_and_missing_keys_stay_identifiable() {
    let f = Fixture::new();
    f.registry();
    let mut i = I18n::load(f.paths());
    assert_eq!(i.language(), "en");
    assert_eq!(i.trn("items.count", 1), "1 item");
    assert_eq!(i.trn("items.count", 12), "12 items");
    assert_eq!(i.tr("does.not.exist"), "does.not.exist");
    assert!(!i.poll_changed());
    for value in ["xx", "../../outside", "", "de/../en"] {
        f.write("config/omarchy/language", value);
        assert!(i.poll_changed());
        assert_eq!(i.language(), "en");
    }
}

#[test]
fn selected_catalogs_layer_over_source_with_per_string_and_english_plural_fallback() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "ru\n");
    f.write("source/i18n/ru/flea.json", r#"{"folder.empty":"Пусто","operation.failed":"Не удалось %1: %2"}"#);
    f.write("installed/i18n/ru/flea.json", r#"{"folder.empty":"","items.count":{"one":"%1 файл","few":"%1 файла","many":"%1 файлов","other":"%1 файла"},"selection.count":{},"app.name":false,"unowned":"No"}"#);
    let i = I18n::load(f.paths());
    assert_eq!(i.language(), "ru");
    assert_eq!(i.tr("folder.empty"), "This folder is empty");
    assert_eq!(i.tr("app.name"), "Flea");
    assert_eq!(i.tr_args("operation.failed", &["open", "denied"]), "Не удалось open: denied");
    assert_eq!(i.trn("items.count", 21), "21 файл");
    assert_eq!(i.trn("items.count", 22), "22 файла");
    assert_eq!(i.trn("items.count", 25), "25 файлов");
    assert_eq!(i.trn_args("selection.count", 21, &["Work"]), "21 items selected in Work");
    assert_eq!(i.tr("unowned"), "unowned");
}

#[test]
fn missing_translation_forms_use_other_before_english() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "ar");
    f.write("installed/i18n/ar/flea.json", r#"{"items.count":{"other":"%1 translated"}}"#);
    let i = I18n::load(f.paths());
    assert_eq!(i.trn("items.count", 2), "2 translated");
    assert!(i.locale().right_to_left);
    assert_eq!(i.tr("folder.empty"), "This folder is empty");
}

#[test]
fn a_gui_line_opening_in_latin_takes_the_rtl_locales_direction() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "ar");
    f.write(
        "installed/i18n/en/flea.json",
        r#"{"copy.done":"Copied %1","copy.lead":"Copied %1","isolated":"Copied"}"#,
    );
    f.write(
        "installed/i18n/ar/flea.json",
        r#"{"app.name":"Flea","copy.done":"Flea نسخ %1","copy.lead":"%1 منسوخ","items.count":{"other":"Flea فيه %1"},"isolated":"\u2068Flea\u2069 نسخ"}"#,
    );
    let tui = I18n::load(f.paths());
    let mut gui = tui.clone();
    gui.mark_paragraphs();
    assert_eq!(tui.tr_args("copy.done", &["a.txt"]), "Flea نسخ a.txt");
    assert_eq!(gui.tr_args("copy.done", &["a.txt"]), "\u{200f}Flea نسخ a.txt");
    assert_eq!(gui.trn("items.count", 3), "\u{200f}Flea فيه 3");
    // A line that opens in its own script, in a placeholder, in an isolate or in Latin alone keeps its text.
    assert_eq!(gui.tr("app.name"), "Flea");
    assert_eq!(gui.tr_args("copy.lead", &["a.txt"]), "a.txt منسوخ");
    assert_eq!(gui.tr("isolated"), "\u{2068}Flea\u{2069} نسخ");
    assert_eq!(gui.tr("folder.empty"), "This folder is empty");
    gui.reload();
    assert_eq!(gui.tr_args("copy.done", &["a.txt"]), "\u{200f}Flea نسخ a.txt");
}

#[test]
fn substitution_handles_reordering_double_digit_tokens_and_literal_replacement_text() {
    assert_eq!(substitute("%2 then %1", &["A", "B"]), "B then A");
    assert_eq!(substitute("%1 %2", &["%2", "A&B"]), "%2 A&B");
    let args = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
    assert_eq!(substitute("%10/%1/%11/%0/%/é", &args), "j/a/%11/%0/%/é");
    assert_eq!(substitute("%999999999999999999999999", &args), "%999999999999999999999999");
}

#[test]
fn omarchy_words_include_flat_keys_menu_fields_key_names_and_english_fallback() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "de");
    f.write("omarchy/default/i18n/en/shell.json", r#"{"common.search":"Search","common.cancel":"Cancel","count":{"one":"%1 device","other":"%1 devices"}}"#);
    f.write("omarchy/default/i18n/de/shell.json", r#"{"common.search":"Suchen","count":{"other":"%1 Geräte"}}"#);
    f.write("omarchy/default/i18n/en/menu.json", r#"{"apps.fileManager":{"label":"File manager","hint":"Open %1"}}"#);
    f.write("omarchy/default/i18n/de/menu.json", r#"{"apps.fileManager":{"label":"Dateimanager"}}"#);
    f.write("omarchy/default/i18n/en/keybindings.json", r#"{"Close window":"Close window"}"#);
    f.write("omarchy/default/i18n/de/keybindings.json", r#"{"Close window":"Fenster schließen"}"#);
    f.write("omarchy/default/i18n/en/cli.json", r#"{"common.yes":"Yes"}"#);
    f.write("source/i18n/en/flea.json", r#"{"search":"omarchy:shell.common.search"}"#);
    let i = I18n::load(f.paths());
    assert_eq!(i.tr("omarchy:shell.common.search"), "Suchen");
    assert_eq!(i.tr("omarchy:shell.common.cancel"), "Cancel");
    assert_eq!(i.tr("search"), "Suchen");
    assert_eq!(i.trn("omarchy:shell.count", 2), "2 Geräte");
    assert_eq!(i.tr("omarchy:menu.apps.fileManager.label"), "Dateimanager");
    assert_eq!(i.tr_args("omarchy:menu.apps.fileManager.hint", &["Work"]), "Open Work");
    assert_eq!(i.tr("omarchy:keybindings.Close window"), "Fenster schließen");
    assert_eq!(i.tr("omarchy:cli.common.yes"), "Yes");
    assert_eq!(i.tr("omarchy:shell.missing"), "omarchy:shell.missing");
    assert_eq!(i.tr("omarchy:../outside"), "omarchy:../outside");
}

// EY-317: Omarchy 4.0.x ships no shell strings, so each omarchy: reference draws Omarchy's English word, never its key.
const OMARCHY_REFERENCES: [(&str, &str); 17] = [
    ("dialogs.output.cancel", "Cancel"), ("archive.cancel", "Cancel"),
    ("device.unlock.passphrase", "Passphrase"), ("device.menu.open", "Open"),
    ("device.unlock.cancel", "Cancel"), ("picker.portal.cancel", "Cancel"),
    ("places.heading.network", "Network"), ("plugins.add.cancel", "Cancel"),
    ("jobs.button.cancel", "Cancel"), ("card.cancel", "Cancel"),
    ("trash.confirm.cancel", "Cancel"), ("send.rclone.cancel", "Cancel"),
    ("dialogs.file.cancel", "Cancel"), ("dialogs.file.apply", "Apply"),
    ("network.cancel", "Cancel"), ("keys.label.8dd6857baf02", "Mute"),
    ("keys.action.mute", "Mute"),
];

#[test]
fn omarchy_references_draw_english_words_without_omarchy_catalogs() {
    let english = include_str!("../../i18n/en/flea.json");
    assert_eq!(english.matches("\"omarchy:").count(), OMARCHY_REFERENCES.len(), "every omarchy: reference is pinned here");
    let f = Fixture::new();
    f.write("config/omarchy/language", "de");
    // No Omarchy root at all, as on 4.0.x: no locales.json, so the roster is English only and the locale stays en.
    let i = I18n::load(f.paths());
    assert_eq!(i.language(), "en");
    for (key, word) in OMARCHY_REFERENCES {
        assert_eq!(i.tr(key), word, "{key}");
    }
    f.registry();
    let i = I18n::load(f.paths());
    for (key, word) in OMARCHY_REFERENCES {
        assert_eq!(i.tr(key), word, "{key} with a roster and no shell catalog");
    }
    f.write("omarchy/default/i18n/en/shell.json", r#"{"common.cancel":"Dismiss","common.apply":"Apply"}"#);
    f.write("omarchy/default/i18n/de/shell.json", r#"{"common.apply":"Anwenden"}"#);
    f.write("omarchy/default/i18n/en/keybindings.json", r#"{"Mute":"Mute"}"#);
    f.write("omarchy/default/i18n/de/keybindings.json", r#"{"Mute":"Stumm"}"#);
    let i = I18n::load(f.paths());
    assert_eq!(i.language(), "de");
    assert_eq!(i.tr("card.cancel"), "Dismiss", "Omarchy's English word wins");
    assert_eq!(i.tr("dialogs.file.apply"), "Anwenden", "Omarchy's translation wins");
    assert_eq!(i.tr("keys.action.mute"), "Stumm");
    assert_eq!(i.tr("device.menu.open"), "Open");
    assert_eq!(i.tr("omarchy:shell.missing"), "omarchy:shell.missing", "an unlisted reference stays identifiable");
}

#[test]
fn watcher_follows_creation_atomic_replacement_removal_and_catalog_updates() {
    let f = Fixture::new();
    f.registry();
    let mut i = I18n::load(f.paths());
    f.write("config/omarchy/language", "de");
    assert!(i.poll_changed());
    assert_eq!(i.language(), "de");
    assert!(!i.poll_changed());
    f.write("installed/i18n/de/flea.json", r#"{"folder.empty":"Leer"}"#);
    assert!(i.poll_changed());
    assert_eq!(i.tr("folder.empty"), "Leer");
    f.write("config/omarchy/next", "fa");
    fs::rename(f.path("config/omarchy/next"), f.path("config/omarchy/language")).unwrap();
    assert!(i.poll_changed());
    assert_eq!(i.language(), "fa");
    assert_eq!(i.trn("items.count", 2), "۲ items");
    fs::remove_file(f.path("config/omarchy/language")).unwrap();
    assert!(i.poll_changed());
    assert_eq!(i.language(), "en");
}

#[test]
fn malformed_catalogs_and_registry_do_not_break_english_lookup() {
    let f = Fixture::new();
    f.write("omarchy/default/i18n/locales.json", "{broken");
    f.write("installed/i18n/en/flea.json", "[1,2,3]");
    let i = I18n::load(f.paths());
    assert_eq!(i.language(), "en");
    assert_eq!(i.tr("folder.empty"), "This folder is empty");
    f.write("omarchy/default/i18n/locales.json", r#"{"../bad":{},"de":{"name":"Deutsch","zeroDigit":"bad"}}"#);
    let i = I18n::load(f.paths());
    assert_eq!(i.roster().len(), 2);
    assert_eq!(i.roster().iter().find(|l| l.code == "de").unwrap().zero_digit, '0');
}

#[test]
fn roster_fields_digits_decimal_sizes_and_grouping_are_preserved() {
    let roster = locale::parse(include_str!("fixtures/locales.json"), Path::new("fixtures/locales.json"));
    assert_eq!(roster.len(), 50);
    let fa = roster.iter().find(|l| l.code == "fa").unwrap();
    assert_eq!(fa.name, "فارسی");
    assert_eq!(fa.english_name, "Persian");
    assert_eq!(fa.format_locale, "fa-IR");
    assert_eq!(fa.glibc_locale, "fa_IR.UTF-8");
    assert!(fa.right_to_left);
    assert_eq!(fa.format_number(22.14, 1), "۲۲٫۱");
    assert_eq!(fa.format_size(22_100), "۲۲٫۱ kB");
    assert_eq!(fa.format_size(999), "۹۹۹ B");
    assert_eq!(fa.localize_number("-12.5"), "-۱۲٫۵");
    assert_eq!(fa.localize_number("file1.txt"), "file1.txt");
    assert_eq!(fa.localize_number("1.2.3"), "1.2.3");
    let bn = roster.iter().find(|l| l.code == "bn").unwrap();
    assert_eq!(bn.localize_number("120"), "১২০");
    let de = roster.iter().find(|l| l.code == "de").unwrap();
    assert_eq!(de.format_size(22_100), "22,1 kB");
    let custom = Locale { separator: ",".into(), grouping: vec![3, 2], ..Locale::default() };
    assert_eq!(custom.format_count(1_234_567), "12,34,567");
    assert_eq!(Locale::default().format_count(i64::MIN), "-9,223,372,036,854,775,808");
    assert_eq!(Locale::default().format_size(1_000), "1.0 kB");
}

#[test]
fn dates_keep_fixed_fields_and_use_digits_without_mutating_global_locale() {
    let locale = Locale { glibc_locale: "C".into(), ..Locale::default() };
    let date = locale.format_date(1_750_000_000).unwrap();
    assert_eq!(date.len(), 16);
    assert_eq!(&date[4..5], "-");
    assert_eq!(&date[7..8], "-");
    assert_eq!(&date[10..11], " ");
    assert_eq!(&date[13..14], ":");
    assert!(date.chars().all(|c| c.is_ascii_digit() || "- :".contains(c)));
    let fa = Locale { zero_digit: '۰', glibc_locale: "missing_FLEA_TEST".into(), ..locale };
    let persian = fa.format_date(1_750_000_000).unwrap();
    assert_eq!(persian.chars().count(), 16);
    assert!(persian.chars().all(|c| ('۰'..='۹').contains(&c) || "- :".contains(c)));
    assert_eq!(fa.format_date(i64::MAX), None);
}

#[path = "test_contracts.rs"]
mod contracts;

#[test]
fn without_omarchy_the_shipped_catalogs_and_the_session_language_choose() {
    let f = Fixture::new();
    f.write("source/i18n/es-MX/flea.json", r#"{"folder.empty":"Carpeta vacía"}"#);
    let mut paths = f.paths();
    paths.omarchy = f.path("no-omarchy");
    let mut roster = super::locale::from_catalogs(&paths.catalogs);
    assert_eq!(roster.iter().map(|l| l.code.as_str()).collect::<Vec<_>>(), ["en", "es-MX"]);
    let picked = super::locale::pick_from_session(&mut roster, |key| (key == "LANG").then(|| "es_ES.UTF-8".to_owned()));
    assert_eq!(picked.as_deref(), Some("es-MX"));
}

#[test]
fn without_omarchy_its_language_file_is_ignored_and_a_new_catalog_is_seen() {
    let f = Fixture::new();
    f.write("source/i18n/fa/flea.json", r#"{"folder.empty":"خالی"}"#);
    f.write("config/omarchy/language", "fa");
    let mut paths = f.paths();
    paths.omarchy = f.path("no-omarchy");
    let mut i = I18n::load(paths);
    // The session decides, and no test runner's session language is Persian.
    assert_ne!(i.language(), "fa");
    assert!(i.roster().iter().any(|l| l.code == "fa" && l.right_to_left));
    assert!(!i.roster().iter().any(|l| l.code == "de"));
    f.write("installed/i18n/de/flea.json", r#"{"folder.empty":"Leer"}"#);
    assert!(i.poll_changed());
    assert!(i.roster().iter().any(|l| l.code == "de"));
}
