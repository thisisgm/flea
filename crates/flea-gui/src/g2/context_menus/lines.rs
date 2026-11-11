//! One build of a menu card: every row's words resolved and shaped once, and the card's width read from them once.
use super::glyphs;
use super::keycaps;
use super::wrap::Caption;
use crate::g2::doc_preview::code::shape::{elided, Elide};
use crate::list::Typeface;
use flea::i18n::I18n;
use flea::ui_logic_context_menus::{
    fit::{self, RowParts},
    Brand, Entry,
};
use gpui::{point, px, GlyphRun, Hsla, Pixels, TextRun, Window};
use std::cell::Cell;

// MenuRow.qml's available-update status square and the gap before its version.
pub(super) const STATUS_SQUARE: f32 = 6.;
pub(super) const STATUS_GAP: f32 = 7.;

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
    // The busy row's 22px Omi in its thinking mode, centred on the slot.
    Busy,
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
    pub metrics: fit::Metrics,
    pub separator: bool,
    pub action: String,
    pub id: String,
    pub label: Option<Shaped>,
    // The label cut at its end with an ellipsis where the card cannot hold it whole, as v0.3.12 MenuRow elides (#214).
    pub elided: Option<Shaped>,
    // A state word or the default caption, which keeps the chevron's column.
    pub word: Option<Shaped>,
    pub hint_square: bool,
    // The key the row's action is bound to, while key hints are on.
    pub key: Option<Shaped>,
    pub chevron: bool,
    // The entry's own wrap flag, which the producers' wrapped captions set and the readers report.
    pub hint_wrap: bool,
    pub wrapped: Option<Caption>,
    pub lead: Lead,
    pub tag_state: Option<flea::ui_logic_rail_places::tags::State>,
    pub ink: Ink,
    pub disabled: bool,
    // A muted row that says the menu is still asking; its label takes the muted ink, undimmed.
    pub busy: bool,
    // The width this row needs whole, the parts summed once at build.
    pub wanted: f32,
}

impl Line {
    pub fn wraps_hint(&self) -> bool {
        self.hint_wrap && self.word.is_some()
    }

    pub fn frame_inset(&self) -> f32 {
        if self.wraps_hint() {
            0.
        } else {
            1.
        }
    }

    pub fn has_trailing_column(&self) -> bool {
        self.chevron || (!self.wraps_hint() && self.word.is_some())
    }

    fn wrapped_minimum(label: f32, chevron: bool, metrics: fit::Metrics) -> f32 {
        2. * metrics.pad_x
            + metrics.slot
            + 2. * metrics.gap
            + label
            + if chevron {
                metrics.chevron + metrics.gap
            } else {
                0.
            }
    }

    /// The label run the row paints: the elided one where the card cut it, else the whole label.
    pub fn painted_label(&self) -> Option<&Shaped> {
        self.elided.as_ref().or(self.label.as_ref())
    }

    pub fn word_width(&self) -> f32 {
        self.wrapped.as_ref().map_or_else(
            || self.word.as_ref().map_or(0., Shaped::width),
            |caption| caption.width,
        )
    }

    pub fn status_width(&self) -> f32 {
        if self.hint_square {
            self.metrics.of(STATUS_SQUARE) + self.metrics.of(STATUS_GAP)
        } else {
            0.
        }
    }

    /// The label's clip after the trailing parts the painter actually shapes.
    pub fn label_room(&self, width: f32) -> f32 {
        let metrics = self.metrics;
        if self.wraps_hint() {
            let label = self.label.as_ref().map_or(0., Shaped::width);
            let remaining = width + 2. - Self::wrapped_minimum(label, self.chevron, metrics);
            return (label + (remaining - self.word_width())).max(0.);
        }
        let mut edge = width - metrics.pad_x;
        if self.has_trailing_column() {
            edge -= metrics.chevron + metrics.gap;
        }
        if let Some(key) = &self.key {
            edge -= key.width() + metrics.gap;
        }
        if self.word.is_some() {
            edge -= self.word_width() + self.status_width() + metrics.gap;
        }
        if self.tag_state.is_some() {
            edge -= metrics.of(flea::ui_logic_rail_places::tags::SWATCH_SIZE) + metrics.gap;
        }
        (edge - metrics.pad_x - metrics.slot).max(0.)
    }
}

/// What a build reads besides the rows: the catalog, the key preset and whether key hints are shown.
pub struct Env<'a> {
    pub catalog: &'a I18n,
    pub preset: &'a str,
    pub key_hints: bool,
    pub typeface: &'a Typeface,
    // The vertical inset is Theme.space(4), scaled from Omarchy's nominal base 12.
    pub inset: f32,
}

impl Env<'_> {
    pub fn horizontal_inset(&self) -> f32 {
        (4. * f32::from(self.typeface.body) / 14.).round()
    }
}

pub struct Built {
    pub metrics: fit::Metrics,
    pub lines: Vec<Line>,
    pub widest: f32,
    pub typeface: Typeface,
}

impl Built {
    pub(super) fn wrap(&mut self, width: f32, env: &Env, window: &Window) {
        for line in &mut self.lines {
            if !line.hint_wrap {
                continue;
            }
            if let Some(word) = &line.word {
                let label = line.label.as_ref().map_or(0., Shaped::width);
                let room =
                    (width - Line::wrapped_minimum(label, line.chevron, self.metrics)).max(0.);
                line.wrapped = Some(Caption::new(word, room, env, window));
            }
        }
        // The painter's row is the card less its two frame pixels; a label wider than its room ends in an ellipsis.
        for line in &mut self.lines {
            let Some(label) = &line.label else { continue };
            let room = line.label_room(width - 2.);
            line.elided = (label.width() > room).then(|| {
                let (text, run) = elided(
                    window,
                    &label.text,
                    &env.typeface.font,
                    env.typeface.body,
                    px(room),
                    Elide::End,
                );
                Shaped { text, run }
            });
        }
    }
}

thread_local! {
    // Width computations since start, so a test can pin one per build.
    static BUILDS: Cell<u64> = const { Cell::new(0) };
}

pub fn builds() -> u64 {
    BUILDS.with(Cell::get)
}

pub(super) fn shape(text: String, size: Pixels, typeface: &Typeface, window: &Window) -> Shaped {
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

pub(super) fn lead(entry: &Entry) -> Lead {
    if entry.busy {
        return Lead::Busy;
    }
    // A tag row leads with its colour square even while it carries the mark in effect.
    if let Some(tag) = entry.tag {
        return Lead::Tag(tag);
    }
    if let Some(check) = entry.check {
        return Lead::Check(check);
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

pub fn build(entries: &[Entry], env: &Env, window: &Window, minimum: f32, maximum: f32) -> Built {
    BUILDS.with(|count| count.set(count.get() + 1));
    let body = env.typeface.body;
    let metrics = fit::Metrics::at(f32::from(body));
    let caption = env.typeface.caption;
    // The hint keys read the active group's keycaps once per build, so the card is measured with what it draws.
    let (caps, rtl) = (keycaps::current(), env.catalog.locale().right_to_left);
    let mut widest = 0f32;
    let lines = entries
        .iter()
        .map(|entry| {
            if entry.separator {
                return Line {
                    metrics,
                    separator: true,
                    action: String::new(),
                    id: String::new(),
                    label: None,
                    elided: None,
                    word: None,
                    hint_square: false,
                    key: None,
                    chevron: false,
                    hint_wrap: false,
                    wrapped: None,
                    lead: Lead::None,
                    tag_state: None,
                    ink: Ink::Plain,
                    disabled: false,
                    busy: false,
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
            let wrapping = entry.hint_wrap && word.is_some();
            let hint_square = !wrapping
                && entry.hint_square
                && word.as_ref().is_some_and(|word| !word.text.is_empty());
            let status_width = if hint_square {
                metrics.of(STATUS_SQUARE) + metrics.of(STATUS_GAP)
            } else {
                0.
            };
            let key = (!wrapping)
                .then_some(entry.key_text(env.preset, env.key_hints))
                .filter(|k| !k.is_empty())
                .map(|k| keycaps::chord(k, caps.as_ref(), rtl))
                .map(|k| shape(k, caption, env.typeface, window));
            let parts = RowParts {
                label: label.as_ref().map_or(0., Shaped::width),
                word: if wrapping {
                    0.
                } else {
                    word.as_ref().map_or(0., Shaped::width) + status_width
                },
                key: key.as_ref().map_or(0., Shaped::width),
                chevron: entry.has_submenu(),
            };
            let wanted = if wrapping {
                Line::wrapped_minimum(parts.label, parts.chevron, metrics)
            } else {
                metrics.row_width(parts)
                    + if entry.tag_state.is_some() {
                        metrics.of(flea::ui_logic_rail_places::tags::SWATCH_SIZE) + metrics.gap
                    } else {
                        0.
                    }
            };
            widest = widest.max(wanted);
            let ink = if entry.errored {
                Ink::Errored
            } else if entry.danger {
                Ink::Danger
            } else {
                Ink::Plain
            };
            Line {
                metrics,
                separator: false,
                action: entry.action.to_string(),
                id: entry.id.to_string(),
                label,
                elided: None,
                word,
                hint_square,
                key,
                chevron: entry.has_submenu(),
                hint_wrap: entry.hint_wrap,
                wrapped: None,
                lead: lead(entry),
                tag_state: entry.tag_state,
                ink,
                // A row still reading keeps its resting ink, never grey before it settles red.
                disabled: entry.disabled && !entry.pending,
                busy: entry.busy,
                wanted,
            }
        })
        .collect();
    let mut built = Built {
        metrics,
        lines,
        widest,
        typeface: env.typeface.clone(),
    };
    built.wrap(metrics.flyout_width(minimum, widest, maximum), env, window);
    built
}

pub(super) fn paint_text(
    shaped: &Shaped,
    at: gpui::Point<Pixels>,
    color: Hsla,
    row: i32,
    window: &mut Window,
) {
    let line = shaped.run.line();
    let baseline =
        window.pixel_snap(at.y + (px(row as f32) - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(at.x, baseline), &shaped.run, color);
}
