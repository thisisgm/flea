//! Settings pages, Settings search and the keyboard sheet: pure logic the GPUI window and the TUI share.
use crate::i18n::I18n;

pub mod about;
pub mod caps;
pub mod edit;
pub mod menus;
pub mod pages;
pub mod rows;
pub mod search;
pub mod sheet;
pub mod sheet_keyboard;
pub mod sheetkeys;
pub mod textsize;
pub mod update;
pub mod uistate;
pub mod visible;
pub mod words;

#[cfg(test)]
mod test_support;
#[cfg(test)]
#[path = "ui_logic_st1/keymap_tests.rs"]
mod keymap_tests;
#[cfg(test)]
#[path = "ui_logic_st1/keymap_more_tests.rs"]
mod keymap_more_tests;

/// A visible string kept unresolved until a catalog is at hand, so a language change re-reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Text {
    #[default]
    Empty,
    Key(&'static str),
    Args(&'static str, Vec<Text>),
    Count(&'static str, i64, Vec<Text>),
    // Items joined by the catalog's list separator.
    List(Vec<Text>),
    Data(String),
    // A key as the preset spells it, isolated for right-to-left text when resolved.
    Keycap(String),
}

impl Text {
    pub fn data(value: impl Into<String>) -> Self {
        Text::Data(value.into())
    }

    pub fn args(key: &'static str, args: Vec<Text>) -> Self {
        Text::Args(key, args)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Text::Empty => true,
            Text::Data(value) | Text::Keycap(value) => value.is_empty(),
            _ => false,
        }
    }

    /// The catalog key this text reads, or None for data and the empty text.
    pub fn key(&self) -> Option<&'static str> {
        match self {
            Text::Key(key) | Text::Args(key, _) | Text::Count(key, _, _) => Some(key),
            _ => None,
        }
    }

    pub fn resolve(&self, i18n: &I18n) -> String {
        let all = |args: &[Text]| args.iter().map(|arg| arg.resolve(i18n)).collect::<Vec<_>>();
        match self {
            Text::Empty => String::new(),
            Text::Key(key) => i18n.tr(key),
            Text::Args(key, args) => {
                let values = all(args);
                i18n.tr_args(key, &values.iter().map(String::as_str).collect::<Vec<_>>())
            }
            Text::Count(key, count, args) => {
                let values = all(args);
                i18n.trn_args(key, *count, &values.iter().map(String::as_str).collect::<Vec<_>>())
            }
            Text::List(items) => all(items).join(&i18n.tr("settings.list.separator")),
            Text::Data(value) => value.clone(),
            Text::Keycap(keys) => i18n.keycap(keys),
        }
    }
}
