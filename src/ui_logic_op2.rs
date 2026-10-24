//! Archive and flash cards shared by every front end: extension tables, card sentences, flash endings and pane decisions.
pub mod archive;
pub mod cards;
pub mod flash;
pub mod flashcard;
pub mod flashendings;
pub mod flashpane;
pub mod ops;
pub mod wire;

#[cfg(test)]
mod archive_tests;
#[cfg(test)]
mod cards_tests;
#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod flash_tests;
#[cfg(test)]
mod flashpane_tests;
#[cfg(test)]
mod ops_tests;
#[cfg(test)]
mod wire_tests;

/// The shipped English catalog over a fixture Omarchy holding the shared words this unit references.
#[cfg(test)]
pub(crate) fn english() -> crate::i18n::I18n {
    language("/nonexistent/language")
}

/// The same catalogs with the language file named, so a test can select German.
#[cfg(test)]
pub(crate) fn language(file: &str) -> crate::i18n::I18n {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui_logic_op2/fixtures");
    crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: fixtures.clone(),
        language_file: fixtures.join(file),
        catalogs: Vec::new(),
    })
}
