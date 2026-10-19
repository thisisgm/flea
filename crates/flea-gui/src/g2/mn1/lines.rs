//! One build of a menu card: every row's words resolved and shaped once, and the card's width read from them once.
use super::glyphs;
use crate::list::Typeface;
use flea::i18n::I18n;
use flea::ui_logic_mn1::{
    fit::{self, RowParts},
    Brand, Entry,
};
use gpui::{GlyphRun, Hsla, Pixels, TextRun, Window};
use std::cell::Cell;

/// What a row draws in its 18 pixel slot.
#[derive(Clone, Debug, PartialEq)]
pub enum Lead {
    None,
    Cut(&'static str),
    Brand(Brand),
    // A tag's 14 pixel colour square, by its one-based slot.
    Tag(u8),
    // Sort By's checkmark, drawn only on the order in effect.
    Check(bool),
    // An application's own icon name, which the icon theme unit resolves.
    Icon(String),
}

/// How a row's label and mark are inked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Plain,
    // A refused provider or a row that cannot act, dimmed and red.
    Errored,
    // Move to Trash, Delete Permanently and Empty Trash.
    Danger,
}

pub struct Shaped {
    pub text: String,
    pub run: GlyphRun,
}

impl Shaped {
    pub fn width(&self) -> f32 {
        f32::from(self.run.line().width)
    }
}

pub struct Line {
    pub separator: bool,
    pub action: String,
    pub id: String,
    pub label: Option<Shaped>,
    // A state word or the default caption, which keeps the chevron's column.
    pub word: Option<Shaped>,
    // The key the row's action is bound to, while key hints are on.
    pub key: Option<Shaped>,
    pub chevron: bool,
    pub lead: Lead,
    pub ink: Ink,
    pub disabled: bool,
    // The width this row needs whole, the parts summed once at build.
    pub wanted: f32,
}

/// What a build reads besides the rows: the catalog, the key preset and whether key hints are shown.
pub struct Env<'a> {
    pub catalog: &'a I18n,
    pub preset: &'a str,
    pub key_hints: bool,
    pub typeface: &'a Typeface,
    // The work area's inset, Theme.space(4) at the window's text size, which the theme unit resolves.
    pub inset: f32,
}

pub struct Built {
    pub lines: Vec<Line>,
    pub widest: f32,
}

thread_local! {
    // Width computations since start, so a test can pin one per build.
    static BUILDS: Cell<u64> = const { Cell::new(0) };
}

pub fn builds() -> u64 {
    BUILDS.with(Cell::get)
}

fn shape(text: String, size: Pixels, typeface: &Typeface, window: &Window) -> Shaped {
    let run = TextRun {
        len: text.len(),
        font: typeface.font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let run = GlyphRun::new(window.text_system().layout_line(&text, size, &[run], None));
    Shaped { text, run }
}

fn lead(entry: &Entry) -> Lead {
    if let Some(check) = entry.check {
        return Lead::Check(check);
    }
    if let Some(tag) = entry.tag {
        return Lead::Tag(tag);
    }
    if let Some(brand) = entry.brand {
        return Lead::Brand(brand);
    }
    if let Some(icon) = &entry.icon {
        return Lead::Icon(icon.clone());
    }
    entry
        .glyph
        .filter(|name| glyphs::known(name))
        .map_or(Lead::None, Lead::Cut)
}

pub fn build(entries: &[Entry], env: &Env, window: &Window) -> Built {
    BUILDS.with(|count| count.set(count.get() + 1));
    let body = env.typeface.body;
    let caption = env.typeface.caption;
    let mut widest = 0f32;
    let lines = entries
        .iter()
        .map(|entry| {
            if entry.separator {
                return Line {
                    separator: true,
                    action: String::new(),
                    id: String::new(),
                    label: None,
                    word: None,
                    key: None,
                    chevron: false,
                    lead: Lead::None,
                    ink: Ink::Plain,
                    disabled: false,
                    wanted: 0.,
                };
            }
            let label = entry
                .label
                .as_ref()
                .map(|text| shape(text.resolve(env.catalog), body, env.typeface, window));
            let word = entry
                .hint
                .as_ref()
                .map(|text| shape(text.resolve(env.catalog), caption, env.typeface, window));
            let key = Some(entry.key_text(env.preset, env.key_hints))
                .filter(|k| !k.is_empty())
                .map(|k| shape(k.to_owned(), caption, env.typeface, window));
            let parts = RowParts {
                label: label.as_ref().map_or(0., Shaped::width),
                word: word.as_ref().map_or(0., Shaped::width),
                key: key.as_ref().map_or(0., Shaped::width),
                chevron: entry.has_submenu(),
            };
            let wanted = fit::row_width(parts);
            widest = widest.max(wanted);
            let ink = if entry.errored {
                Ink::Errored
            } else if entry.danger {
                Ink::Danger
            } else {
                Ink::Plain
            };
            Line {
                separator: false,
                action: entry.action.to_string(),
                id: entry.id.to_string(),
                label,
                word,
                key,
                chevron: entry.has_submenu(),
                lead: lead(entry),
                ink,
                disabled: entry.disabled,
                wanted,
            }
        })
        .collect();
    Built { lines, widest }
}
