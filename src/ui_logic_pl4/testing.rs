//! Test fixtures shared by this module's suites.
use crate::i18n::{I18n, Paths};
use crate::jsondoc::Json;
use std::path::PathBuf;

// The source tree's English catalog with no Omarchy roster, so every string reads as shipped.
pub fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent"),
        language_file: PathBuf::from("/nonexistent"),
        catalogs: vec![crate::i18n::test_catalogs::english_only()],
    })
}

pub fn record(label: &str, path: &str) -> Json {
    Json::Obj(vec![
        ("label".into(), Json::Str(label.into())),
        ("path".into(), Json::Str(path.into())),
    ])
}
