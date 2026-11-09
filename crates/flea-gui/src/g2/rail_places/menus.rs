//! The rows pl4 contributes to menus the menu unit draws: a favourite's rail menu and the Tags flyout.
use super::content::{tag_mark, Ground, TagMark};
use flea::i18n::I18n;
use flea::ui_logic_rail_places::tags::{flyout, FlyoutRow, State, CLEAR_LABEL_KEY};
use flea::ui_logic_rail_places::tagtheme::TagTheme;
use gpui::SharedString;

/// What a menu row does when chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    RemoveFavourite { index: usize, path: String },
    Tag(&'static str),
    ClearTags,
}

/// The tag's state in the flyout: a checkmark for one in effect on every target; mixed keeps v0.3.12's bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    None,
    Applied,
    Mixed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuRow {
    Item {
        label: SharedString,
        glyph: &'static str,
        hint: &'static str,
        tag: Option<TagMark>,
        check: Check,
        act: Act,
    },
    Separator,
}

/// rail-favorites: a favourite's menu is Remove from Favorites with the minus mark.
pub fn favourite_menu(catalog: &I18n, index: usize, path: &str) -> Vec<MenuRow> {
    vec![MenuRow::Item {
        label: catalog.tr("places.favorite.remove").into(),
        glyph: "minus",
        hint: "",
        tag: None,
        check: Check::None,
        act: Act::RemoveFavourite {
            index,
            path: path.to_owned(),
        },
    }]
}

/// tags-menu: each tag leads with its swatch from the menu colours and carries its digit hint; Remove All Tags only while a tag is set.
pub fn tags_menu(catalog: &I18n, theme: &TagTheme, states: &[State; 7]) -> Vec<MenuRow> {
    flyout(states, theme.fallback)
        .into_iter()
        .map(|row| match row {
            FlyoutRow::Tag(tag) => MenuRow::Item {
                label: catalog.tr(tag.label_key).into(),
                glyph: "",
                hint: tag.digit,
                tag: Some(tag_mark(theme, tag.name, Ground::Surface)),
                check: match tag.state {
                    State::Applied => Check::Applied,
                    State::Mixed => Check::Mixed,
                    State::Absent => Check::None,
                },
                act: Act::Tag(tag.name),
            },
            FlyoutRow::Separator => MenuRow::Separator,
            FlyoutRow::Clear => MenuRow::Item {
                label: catalog.tr(CLEAR_LABEL_KEY).into(),
                glyph: "x",
                hint: "",
                tag: None,
                check: Check::None,
                act: Act::ClearTags,
            },
        })
        .collect()
}
