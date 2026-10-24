//! The English catalog every pk2 sentence test resolves through.
use crate::i18n::{I18n, Paths};

pub fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-pk2");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}
