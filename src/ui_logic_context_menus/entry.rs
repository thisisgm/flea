//! One menu row as data: what it says, which mark it wears, whether it can act, and the flyout it opens.
use crate::i18n::I18n;
use std::borrow::Cow;

pub type Id = Cow<'static, str>;

/// A visible string, resolved through the catalog only when a presenter draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    Key(&'static str),
    // A cardinal plural message with the count as %1.
    Count(&'static str, i64),
    // A template whose %1 is data, such as a storage class word.
    KeyArg(&'static str, &'static str),
    // A name the user or a provider owns: a peer, an application, a script, a tag, a version.
    Data(String),
}

impl Text {
    pub fn resolve(&self, i18n: &I18n) -> String {
        match self {
            Self::Key(key) => i18n.tr(key),
            Self::Count(key, count) => i18n.trn(key, *count),
            Self::KeyArg(key, arg) => i18n.tr_args(key, &[&i18n.tr(arg)]),
            Self::Data(text) => text.clone(),
        }
    }

    pub fn key(&self) -> Option<&'static str> {
        match self {
            Self::Key(key) | Self::Count(key, _) | Self::KeyArg(key, _) => Some(key),
            Self::Data(_) => None,
        }
    }
}

/// A brand's own reproduced mark, never a cut glyph; its drawing belongs to the provider that owns the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brand {
    Tailscale,
    Dropbox,
    LocalSend,
    AirDrop,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub separator: bool,
    pub id: Id,
    pub action: Id,
    pub label: Option<Text>,
    pub glyph: Option<&'static str>,
    pub brand: Option<Brand>,
    // An application's own Icon= name or path, drawn in the mark slot by Open With rows.
    pub icon: Option<String>,
    // A tag flyout row's colour, 1 to 7, drawn as the filled square.
    pub tag: Option<u8>,
    /// A tag's selection state, painted beside its digit hint without replacing its colour square.
    pub tag_state: Option<crate::ui_logic_rail_places::tags::State>,
    // A flyout row that says what is in effect: Some(true) draws the checkmark, Some(false) its empty slot.
    pub check: Option<bool>,
    pub disabled: bool,
    // A muted row beside the busy mark that says the menu is still asking, such as Looking for hosts; never the cursor.
    pub busy: bool,
    pub errored: bool,
    pub danger: bool,
    pub hint: Option<Text>,
    pub hint_wrap: bool,
    pub hint_square: bool,
    // A flyout leaf's letter, drawn only while key hints are on.
    pub key_hint: Option<&'static str>,
    pub submenu: Option<Vec<Entry>>,
}

impl Entry {
    pub fn separator() -> Self {
        Self {
            separator: true,
            ..Self::default()
        }
    }

    pub fn row(id: impl Into<Id>, label: Text, glyph: &'static str) -> Self {
        let id = id.into();
        Self {
            action: id.clone(),
            id,
            label: Some(label),
            glyph: Some(glyph),
            ..Self::default()
        }
    }

    pub fn leaf(id: impl Into<Id>, label: Text) -> Self {
        let id = id.into();
        Self {
            action: Id::Borrowed(""),
            id,
            label: Some(label),
            ..Self::default()
        }
    }

    pub fn with_glyph(mut self, glyph: &'static str) -> Self {
        self.glyph = Some(glyph);
        self
    }

    pub fn with_key_hint(mut self, letter: &'static str) -> Self {
        self.key_hint = Some(letter);
        self
    }

    pub fn has_submenu(&self) -> bool {
        self.submenu.is_some()
    }

    // A leaf's letter rides the menu keymap, so its hint is the leaf's own; a row's is its action's binding.
    pub fn key_text(&self, preset: &str, key_hints: bool) -> &'static str {
        match self.key_hint {
            _ if !key_hints || self.separator => "",
            Some(letter) => letter,
            // Enter activates the focused menu row, so it is not one row's own shortcut.
            None if !self.action.is_empty() => {
                match crate::keys::presentation::hint_for(preset, &self.action) {
                    "enter" | "return" => "",
                    key => key,
                }
            }
            None => "",
        }
    }

    // A separator, a disabled row or a busy row is never the cursor.
    pub fn steppable(&self) -> bool {
        !self.separator && !self.disabled && !self.busy
    }
}

/// Every mark a menu row, flyout leaf or chevron names by its Icons.js name, in the order the window's cut table draws them; that table is pinned to this list.
pub const MARK_NAMES: &[&str] = &[
    "app-window",
    "archive",
    "archive-out",
    "check",
    "checkmark",
    "chevron-right",
    "clipboard",
    "code",
    "columns",
    "contrast",
    "copy",
    "download",
    "drive",
    "eject",
    "eye",
    "eye-off",
    "file",
    "file-plus",
    "file-text",
    "folder",
    "folder-open",
    "folder-plus",
    "globe",
    "image",
    "images",
    "info",
    "key",
    "lock",
    "minus",
    "network",
    "play",
    "plus",
    "printer",
    "rename",
    "scissors",
    "server",
    "shelf",
    "sliders",
    "sort",
    "star",
    "symlink",
    "tag",
    "terminal",
    "trash",
    "type",
    "undo",
    "bot",
    "zap",
    "cloud-upload",
    "x",
    "copy-x",
    "smartphone",
    "laptop",
];
