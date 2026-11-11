//! The sentences the Markdown view writes, from the catalog.

use flea::i18n::I18n;
use flea::ui_logic_doc_preview::markdown::NESTING_LIMIT;
use std::sync::Arc;

/// The sentences the view writes, from the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct Words {
    pub remote: Arc<str>,
    pub deep: String,
    pub too_large: String,
    pub unreadable: String,
    pub binary: String,
    pub large: String,
}

impl Words {
    pub fn of(i18n: &I18n) -> Self {
        Words {
            remote: i18n.tr("document.markdown_remote_image").into(),
            deep: i18n.tr_args("document.markdown_too_deep", &[&NESTING_LIMIT.to_string()]),
            too_large: i18n.tr("document.markdown_too_large"),
            unreadable: i18n.tr("document.markdown_unreadable"),
            binary: i18n.tr("document.markdown_binary"),
            large: i18n.tr("document.markdown_large"),
        }
    }
}
