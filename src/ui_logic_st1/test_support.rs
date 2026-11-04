// English from the compiled-in catalog alone: no Omarchy tree, no language file, no installed catalogs.
use crate::i18n::{I18n, Paths};
use std::path::PathBuf;

pub(crate) fn english() -> I18n {
    let missing = PathBuf::from("/nonexistent/flea-st1-test");
    I18n::load(Paths {
        omarchy: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/i18n/fixtures/omarchy"),
        language_file: missing.join("language"),
        catalogs: Vec::new(),
    })
}

thread_local! {
    static ENGLISH: I18n = english();
}

// Sample input: say(&Text::Key("settings.section.view")) is "View".
pub(crate) fn say(text: &super::Text) -> String {
    ENGLISH.with(|i18n| text.resolve(i18n))
}
