//! The catalog dir tests load, so no test depends on which languages ship; flea-gui includes this file by path.
// Each including crate uses a different subset of these helpers.
#![allow(dead_code)]
// super is crate::i18n in flea and the crate root in flea-gui, which imports flea::i18n's I18n and Paths for this file.
use super::{I18n, Paths};
use std::path::{Path, PathBuf};

/// English only through a link to i18n/en, so it needs no writes, which a helper sandboxed on a read-only root needs.
pub fn english_only() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|dir| dir.join("src/i18n/fixtures/english-only"))
        .find(|dir| dir.join("en/flea.json").is_file())
        .expect("the repo's src/i18n/fixtures/english-only/en/flea.json")
}

/// The default Omarchy root with English only and no language file, so the host's language never reaches a test.
pub fn english_paths() -> Paths {
    Paths {
        language_file: PathBuf::from("/nonexistent/flea-test/omarchy/language"),
        catalogs: vec![english_only()],
        ..Paths::default()
    }
}

pub fn english() -> I18n {
    I18n::load(english_paths())
}

/// english() as the flea library's type, for the flea binary, whose own i18n module is a separate copy.
pub fn library_english() -> flea::i18n::I18n {
    let Paths {
        omarchy,
        language_file,
        catalogs,
    } = english_paths();
    flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy,
        language_file,
        catalogs,
    })
}
