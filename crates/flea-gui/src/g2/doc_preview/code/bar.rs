//! Quick Look's bar for the text kinds as the host's chrome bar draws it, every string already through the catalog.
use super::Mark;
use flea::i18n::I18n;
use flea::ui_logic_doc_preview::html::View;

/// Rendered and Source as two bare words before close: the view in effect in foreground, the other muted (ql-html CHANGES).
#[derive(Clone, Debug, PartialEq)]
pub struct ViewWords {
    pub rendered: String,
    pub source: String,
    pub active: View,
}

/// Left to right: the row's mark, the name (middle-elided by the host), the count once known, the view words, close.
#[derive(Clone, Debug, PartialEq)]
pub struct KindBar {
    pub mark: Mark,
    pub name: String,
    pub count: Option<String>,
    pub views: Option<ViewWords>,
    pub close: String,
}

impl KindBar {
    pub fn new(
        mark: Mark,
        path: &str,
        count: Option<String>,
        views: Option<View>,
        i18n: &I18n,
    ) -> Self {
        KindBar {
            mark,
            name: path.rsplit('/').next().unwrap_or("").to_string(),
            count,
            views: views.map(|active| ViewWords {
                rendered: i18n.tr("document.view_rendered"),
                source: i18n.tr("document.view_source"),
                active,
            }),
            close: i18n.tr("document.pdf_close"),
        }
    }
}
