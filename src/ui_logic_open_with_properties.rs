//! Open With, Permissions, Properties, bulk rename, the run card and the opener lane, shared by every front end.
pub mod bulkrename;
pub mod opener;
pub mod openwith;
pub mod permissions;
pub mod properties;
pub mod runfile;
pub mod viewerorder;

#[cfg(test)]
mod bulkrename_tests;
#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod openwith_tests;
#[cfg(test)]
mod permissions_store_tests;
#[cfg(test)]
mod permissions_tests;
#[cfg(test)]
mod properties_tests;
#[cfg(test)]
mod runfile_tests;

/// The shipped English catalog over a fixture Omarchy holding the shared words this unit references.
#[cfg(test)]
pub(crate) fn english() -> crate::i18n::I18n {
    language("/nonexistent/language")
}

/// The same catalogs with the language file named, so a test can select German.
#[cfg(test)]
pub(crate) fn language(file: &str) -> crate::i18n::I18n {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/ui_logic_open_with_properties/fixtures");
    crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: fixtures.clone(),
        language_file: fixtures.join(file),
        catalogs: Vec::new(),
    })
}
