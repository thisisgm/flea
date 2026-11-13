//! The unlock card as the board draws it: title and esc, a hairline, the drive line, the field, one note line and two buttons.
use super::ink::CardInk;
use super::mark::{Mark, DRIVE, EYE};
use super::probes::{observe, ButtonProbe, Probes};
use super::unlock::{Card, Part, Phase};
use super::words::say;
use crate::g2::file_ops::glyph::Tracked;
use crate::g2::status_feedback::caption::{heading_for, tracking_for};
use flea::i18n::I18n;
use flea::ui_logic_device_rail::unlock::{subtitle, title};
use gpui::{div, prelude::*, px, App, ClickEvent, FontWeight, Hsla, SharedString, Window};

/// The card's own width on the board, narrowed only by a narrow window.
pub const CARD_WIDTH: f32 = 448.;
const PAD: f32 = 16.;
const GAP: f32 = 12.;
const FIELD_H: f32 = 30.;
const NOTE_H: f32 = 16.;
/// The oracle dialog button's pointer washes in the label's ink, 8% hover and 14% press (v0.3.12 Buttons.js).
pub const WASH_HOVER: f32 = 0.08;
pub const WASH_PRESS: f32 = 0.14;
/// Unlock unavailable draws at this opacity, the board's disabled button.
pub const DISABLED_OPACITY: f32 = 0.55;

/// The words the card shows, built once per card change or language change, never in paint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardText {
    pub language: String,
    pub title: String,
    pub esc: String,
    pub subtitle: String,
    pub label: String,
    pub note: String,
    pub cancel: String,
    pub unlock: String,
}

impl CardText {
    pub fn of(card: &Card, catalog: &I18n) -> Self {
        let size = if card.size > 0 {
            catalog.locale().format_size(card.size)
        } else {
            String::new()
        };
        // BitLocker names its secrets as Windows does: a password, or the 48-digit recovery key.
        let bitlocker = card.crypto == "BitLocker";
        let label = if bitlocker {
            catalog.tr("device.unlock.recovery_key")
        } else {
            catalog.tr("device.unlock.passphrase")
        };
        Self {
            language: catalog.language().into(),
            title: say(catalog, &title(&card.label)),
            esc: "esc".into(),
            subtitle: say(catalog, &subtitle(&card.label, &size, &card.crypto)),
            label: heading_for(&label, catalog.language()),
            note: card
                .note
                .as_ref()
                .map(|(n, _)| match n.key {
                    "device.unlock.wrong" if bitlocker => {
                        catalog.tr("device.unlock.wrong_bitlocker")
                    }
                    _ => say(catalog, n),
                })
                .unwrap_or_default(),
            cancel: catalog.tr("device.unlock.cancel"),
            unlock: catalog.tr("device.unlock.button"),
        }
    }
}

/// Every size above is the board's at its 14 px body; the card multiplies by the live body over 14, as the other dialog cards do (#310).
pub const BOARD_BODY: f32 = 14.;

/// The card's width: the board width grown with the text size, narrowed only by a narrow window.
pub fn width(scale: f32, window_width: f32) -> f32 {
    (CARD_WIDTH * scale)
        .ceil()
        .min((window_width - 2. * GAP).max(0.))
}

/// The card element for a card state and its words; scale is the live body over BOARD_BODY.
pub fn card(
    card: &Card,
    text: &CardText,
    ink: &CardInk,
    window_width: f32,
    scale: f32,
    press: Press,
) -> impl IntoElement {
    card_with_input(card, text, ink, window_width, scale, press, None)
}

/// What only the mounted view supplies: the text input while typing, and the probes unlockState reads.
pub(super) struct Mounted<'a> {
    pub input: Option<(gpui::Entity<super::view::UnlockView>, crate::list::Typeface)>,
    pub probes: &'a Probes,
}

pub(super) fn card_with_input(
    card: &Card,
    text: &CardText,
    ink: &CardInk,
    window_width: f32,
    scale: f32,
    press: Press,
    mounted: Option<Mounted<'_>>,
) -> impl IntoElement {
    let (input, probes) = match mounted {
        Some(Mounted { input, probes }) => (input, Some(probes)),
        None => (None, None),
    };
    let k = move |v: f32| px(v * scale);
    let note_error = card.note.as_ref().is_some_and(|(_, e)| *e);
    let field_frame = if card.field.error {
        ink.error
    } else if card.focus == Some(Part::Field) {
        ink.accent_frame
    } else {
        ink.frame
    };
    let unlocking = card.phase == Phase::Unlocking;
    let available = card.unlock_available();
    let dots: String = "\u{2022}".repeat(card.field.dots());
    let probe = |pick: fn(&Probes) -> ButtonProbe| probes.map(pick);
    observe(div(), probes.map(|p| p.card.clone()))
        .id("g2-pl1-unlock-card")
        .flex()
        .flex_col()
        .flex_none()
        .w(px(width(scale, window_width)))
        .p(k(PAD))
        .gap(k(GAP))
        .bg(ink.surface)
        .border_1()
        .border_color(ink.frame)
        .rounded(px(ink.radius))
        .text_color(ink.foreground)
        .child(
            div()
                .flex()
                .items_center()
                .gap(k(9.))
                .font_weight(FontWeight::BOLD)
                .text_size(k(16.))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(text.title.clone()),
                )
                .child(
                    div()
                        .ml_auto()
                        .font_weight(FontWeight::NORMAL)
                        .text_size(k(12.))
                        .text_color(ink.muted)
                        .child(text.esc.clone()),
                ),
        )
        .child(div().h(px(1.)).bg(ink.rule))
        .child(
            div()
                .flex()
                .items_center()
                .gap(k(9.))
                .text_size(k(14.))
                .child(Mark::new(DRIVE, 19. * scale, ink.foreground))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(text.subtitle.clone()),
                ),
        )
        .child(
            div()
                .mt(k(4.))
                .opacity(if unlocking { DISABLED_OPACITY } else { 1. })
                .child(
                    div()
                        .text_size(k(12.))
                        .line_height(k(14.))
                        .mb(k(5.))
                        .text_color(ink.muted)
                        .child(Tracked {
                            text: text.label.clone().into(),
                            size: 12. * scale,
                            line: 14. * scale,
                            track: tracking_for(&text.language) * scale,
                            ink: ink.muted,
                        }),
                )
                .child(
                    div()
                        .h(k(FIELD_H))
                        .flex()
                        .items_center()
                        .gap(k(8.))
                        .px(k(9.))
                        .text_size(k(14.))
                        .bg(ink.ground)
                        .border_1()
                        .border_color(field_frame)
                        .rounded(px(ink.radius))
                        .when(input.is_none() && !dots.is_empty(), |d| d.child(dots))
                        .when(
                            input.is_none() && card.focus == Some(Part::Field) && !unlocking,
                            |d| {
                                let tuck = if card.field.dots() > 0 { -7. } else { 0. };
                                d.child(div().w(px(1.)).h(k(16.)).ml(k(tuck)).bg(ink.foreground))
                            },
                        )
                        .children(input.map(|(view, face)| {
                            crate::g2::file_ops::field::mount(
                                view,
                                div().flex_1().min_w_0().h_full(),
                                &face,
                            )
                        }))
                        .child(div().flex_1().when(!unlocking, |d| d.w(px(0.)).flex_none()))
                        .child(observe(
                            div()
                                .flex_none()
                                .child(Mark::new(EYE, 13. * scale, ink.frame)),
                            probes.map(|p| p.eye.clone()),
                        )),
                ),
        )
        .child(
            div()
                .mt(k(-2.))
                .h(k(NOTE_H))
                .line_height(k(NOTE_H))
                .text_size(k(12.))
                .overflow_hidden()
                .text_color(if note_error { ink.error } else { ink.muted })
                .child(text.note.clone()),
        )
        .child(
            div()
                .mt(k(10.))
                .flex()
                .justify_end()
                .items_center()
                .gap(k(9.))
                .child(button(
                    "g2-pl1-cancel",
                    &text.cancel,
                    false,
                    true,
                    card.focus == Some(Part::Cancel),
                    Look { ink, scale },
                    Act {
                        click: press.cancel,
                        probe: probe(|p| p.cancel.clone()),
                    },
                ))
                .child(button(
                    "g2-pl1-unlock",
                    &text.unlock,
                    true,
                    available,
                    card.focus == Some(Part::Unlock),
                    Look { ink, scale },
                    Act {
                        click: press.unlock,
                        probe: probe(|p| p.unlock.clone()),
                    },
                )),
        )
}

/// What a click on each button does, handed in by the view that owns the card.
pub struct Press {
    pub cancel: Click,
    pub unlock: Click,
}

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A button's two fills: the frame's resting wash, and the pointer wash laid inside the frame over it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonWash {
    pub rest: Option<Hsla>,
    pub hover: Option<Hsla>,
    pub press: Option<Hsla>,
}

/// The oracle recipe: a primary rests on the accent wash, and hover or press adds ink over it, never in its place.
pub fn button_wash(primary: bool, enabled: bool, ink: &CardInk) -> ButtonWash {
    ButtonWash {
        rest: (primary && enabled).then_some(ink.wash),
        hover: enabled.then(|| ink.foreground.opacity(WASH_HOVER)),
        press: enabled.then(|| ink.foreground.opacity(WASH_PRESS)),
    }
}

/// A focused primary whose resting frame is already the foreground draws its frame this wide, inward (Buttons.js RING).
pub const RING: f32 = 2.;

/// The frame's ink, and whether it draws RING wide: foreground on focus, thick where a hairline would match the rest (DialogButton focusFrame).
pub fn button_frame(primary: bool, enabled: bool, focused: bool, ink: &CardInk) -> (Hsla, bool) {
    match (focused && enabled, primary && enabled) {
        (true, true) if ink.accent_frame == ink.foreground => (ink.foreground, true),
        (true, _) => (ink.foreground, false),
        (false, true) => (ink.accent_frame, false),
        (false, false) => (ink.frame, false),
    }
}

/// The card's ink and its text-size scale, which every button shares.
#[derive(Clone, Copy)]
struct Look<'a> {
    ink: &'a CardInk,
    scale: f32,
}

/// A button's click and, in the mounted view, where it paints and what the pointer holds on it.
struct Act {
    click: Click,
    probe: Option<ButtonProbe>,
}

/// The board's framed button: accent frame over a 14% wash when primary, foreground frame on focus, 8% hover, 14% press.
fn button(
    id: &'static str,
    label: &str,
    primary: bool,
    enabled: bool,
    focused: bool,
    look: Look,
    act: Act,
) -> impl IntoElement {
    let Look { ink, scale } = look;
    let Act { click, probe } = act;
    let k = move |v: f32| px(v * scale);
    let (frame, ring) = button_frame(primary, enabled, focused, ink);
    let wash = button_wash(primary, enabled, ink);
    // Pointer wash inside the frame: hover while over the button, press only while held inside it (pressShown).
    let pointer_wash = div()
        .id(SharedString::from(format!("{id}-wash")))
        .absolute()
        .inset_0()
        .rounded(px((ink.radius - if ring { RING } else { 1. }).max(0.)))
        .opacity(0.)
        .when_some(wash.hover, |d, c| {
            d.group_hover(id, move |s| s.opacity(1.).bg(c))
        })
        .when_some(wash.press, |d, c| d.group_active(id, move |s| s.bg(c)));
    let mut pointer = div()
        .id(id)
        .group(id)
        .absolute()
        .inset_0()
        .child(pointer_wash);
    let painted = probe.as_ref().map(|p| p.painted.clone());
    if let Some(probe) = probe {
        let (hovered, down, up, out) = (
            probe.hovered,
            probe.pressed.clone(),
            probe.pressed.clone(),
            probe.pressed,
        );
        pointer = pointer
            .on_hover(move |over, _, _| hovered.set(*over))
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, _| down.set(true))
            .on_mouse_up(gpui::MouseButton::Left, move |_, _, _| up.set(false))
            .on_mouse_up_out(gpui::MouseButton::Left, move |_, _, _| out.set(false));
    }
    observe(div(), painted)
        .h(k(FIELD_H))
        .px(k(9.))
        .flex()
        .items_center()
        .text_size(k(14.))
        .text_color(ink.foreground)
        .whitespace_nowrap()
        .map(|d| if ring { d.border_2() } else { d.border_1() })
        .border_color(frame)
        .rounded(px(ink.radius))
        .when_some(wash.rest, |d, c| d.bg(c))
        .opacity(if enabled { 1. } else { DISABLED_OPACITY })
        .child(if enabled {
            pointer.on_click(move |event, window, cx| click(event, window, cx))
        } else {
            pointer
        })
        .child(label.to_string())
}
