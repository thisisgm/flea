//! The catalog dir tests load, so no test depends on which languages ship; flea-gui includes this file by path.
use std::path::{Path, PathBuf};

/// English only through a link to i18n/en, so it needs no writes, which a helper sandboxed on a read-only root needs.
pub fn english_only() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|dir| dir.join("src/i18n/fixtures/english-only"))
        .find(|dir| dir.join("en/flea.json").is_file())
        .expect("the repo's src/i18n/fixtures/english-only/en/flea.json")
}
