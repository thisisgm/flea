//! The rail content handed to the G1.11 host: each section's capitals and each row's text and mark, resolved once per change.
use super::seams::ThemeRoles;
use flea::i18n::I18n;
use flea::ui_logic_rail_places::order::{Heading, RailSection};
use flea::ui_logic_rail_places::rail::{Group, Kind, RailEntry, Square};
use flea::ui_logic_rail_places::tags;
use flea::ui_logic_rail_places::tagtheme::{tag_theme, TagTheme};
use gpui::{Hsla, Rgba, SharedString};

/// A tag's mark: the 14px swatch in its colour; with no resolved colour the painter takes the window theme's tag ink at `index`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TagMark {
    pub fill: Option<Hsla>,
    /// The tag's place in the seven, 0 for Red.
    pub index: usize,
    pub digit: &'static str,
    pub fallback: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowMark {
    None,
    Square(Square),
    Tag(TagMark),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawnRow {
    pub label: SharedString,
    pub glyph: &'static str,
    pub mark: RowMark,
    /// The row's place in the flat rail, so the host can read its mount dot without re-reading the data.
    pub group: Group,
    pub kind: Kind,
    pub mounted: Option<bool>,
    /// A mounted removable, loop or phone row, whose slot draws the eject mark in place of the square.
    pub releasable: bool,
    /// A favourite's diagnostic, carried for its tooltip; the row still draws.
    pub error: Option<SharedString>,
    /// A device's size in the locale's SI form, empty when lsblk gave none; places.driveSize shows it.
    pub size: SharedString,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawnSection {
    /// Empty when the section draws no heading, as PLACES over a lone Recent row.
    pub heading: SharedString,
    /// A caseless language's heading draws untracked, as its capitals are none.
    pub caseless: bool,
    pub rows: Vec<DrawnRow>,
}

/// "#rrggbb" to a colour; anything else is none.
pub fn ink(hex: &str) -> Option<Hsla> {
    Rgba::try_from(hex).ok().map(Hsla::from)
}

/// The listing and menu tag colours for a theme, decided once per theme load.
pub fn theme_tags(theme: &ThemeRoles) -> TagTheme {
    tag_theme(|role| theme.role(role), &theme.background, &theme.surface)
}

/// The ground a tag mark draws on, which picks the colours lifted to 3:1 against it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// The listing background, where a row's mark sits.
    Listing,
    /// The surface the rail and every menu draw on.
    Surface,
}

pub fn tag_mark(theme: &TagTheme, name: &str, ground: Ground) -> TagMark {
    let at = tags::index_of(name).unwrap_or(0);
    let hex = match ground {
        Ground::Listing => &theme.colors[at],
        Ground::Surface => &theme.menu_colors[at],
    };
    TagMark {
        fill: ink(hex),
        index: at,
        digit: tags::key_for(name),
        fallback: theme.fallback,
    }
}

/// Section titles keep Omarchy's OEM capitals in the language's own rules, none in a caseless language, so a plugin title reads like a built-in one; an empty title draws no heading.
pub fn heading_text(catalog: &I18n, heading: &Heading) -> SharedString {
    let language = catalog.language();
    let upper = |text: &str| {
        if crate::g2::status_feedback::caption::caseless(language) {
            text.to_owned()
        } else {
            flea::ui_logic_settings_pages::caps::heading(text, language)
        }
    };
    match heading {
        Heading::None => SharedString::default(),
        Heading::Key(key) => upper(&catalog.tr(key)).into(),
        Heading::Plugin(title) => upper(title).into(),
    }
}

#[cfg(test)]
#[path = "content_caps_tests.rs"]
mod caps_tests;

pub fn row_label(catalog: &I18n, entry: &RailEntry) -> SharedString {
    if entry.label_key.is_empty() {
        entry.label.clone().into()
    } else {
        catalog.tr(entry.label_key).into()
    }
}

fn row(catalog: &I18n, theme: &TagTheme, entry: &RailEntry) -> DrawnRow {
    // A plugin row keeps its glyph and shows its state in the trailing slot, as a share row does.
    let plugin = entry.kind == flea::ui_logic_rail_places::rail::Kind::PluginRow;
    let mark = match (entry.tag, entry.square) {
        (Some(name), _) => RowMark::Tag(tag_mark(theme, name, Ground::Surface)),
        (None, Square::None) => RowMark::None,
        (None, _) if plugin => RowMark::None,
        (None, square) => RowMark::Square(square),
    };
    let mounted = match entry.square {
        Square::On if plugin => Some(true),
        Square::Off if plugin => Some(false),
        _ => entry.mounted,
    };
    let error = entry.error.as_ref().map(|error| match error {
        flea::ui_logic_rail_places::rail::FavouriteError::Record(record) => {
            catalog.tr(record.key()).into()
        }
        flea::ui_logic_rail_places::rail::FavouriteError::Status(line) => line.clone().into(),
    });
    DrawnRow {
        label: row_label(catalog, entry),
        glyph: entry.glyph,
        mark,
        group: entry.group,
        kind: entry.kind,
        mounted,
        releasable: entry.releasable,
        error,
        size: entry
            .size
            .filter(|_| entry.group == Group::Device)
            .map(|bytes| catalog.locale().format_size(bytes).into())
            .unwrap_or_default(),
    }
}

pub fn build(catalog: &I18n, sections: &[RailSection], theme: &TagTheme) -> Vec<DrawnSection> {
    sections
        .iter()
        .map(|s| DrawnSection {
            heading: heading_text(catalog, &s.heading),
            caseless: crate::g2::status_feedback::caption::caseless(catalog.language()),
            rows: s.rows.iter().map(|e| row(catalog, theme, e)).collect(),
        })
        .collect()
}

/// Rebuilds only when its inputs change, so the rail repaints once per real change and never per frame.
#[derive(Debug, Default)]
pub struct Provider {
    sections: Vec<RailSection>,
    theme: TagTheme,
    pub drawn: Vec<DrawnSection>,
    pub revision: u64,
}

impl Provider {
    pub fn update(&mut self, catalog: &I18n, sections: Vec<RailSection>, theme: TagTheme) -> bool {
        if self.revision > 0 && sections == self.sections && theme == self.theme {
            return false;
        }
        self.drawn = build(catalog, &sections, &theme);
        self.sections = sections;
        self.theme = theme;
        self.revision += 1;
        true
    }

    /// A language change re-resolves every string over the same inputs.
    pub fn relabel(&mut self, catalog: &I18n) {
        self.drawn = build(catalog, &self.sections, &self.theme);
        self.revision += 1;
    }
}
