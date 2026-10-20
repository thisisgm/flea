//! The unlock card as the board draws it: title and esc, a hairline, the drive line, the field, one note line and two buttons.
use super::ink::CardInk;
use super::unlock::{Card, Part, Phase};
use super::words::say;
use flea::i18n::I18n;
use flea::ui_logic_pl1::unlock::{subtitle, title};
use gpui::{
    div, prelude::*, px, App, Bounds, ClickEvent, Element, ElementId, FontWeight, GlobalElementId, Hsla, InspectorElementId,
    LayoutId, Pixels, SharedString, Style, TransformationMatrix, Window,
};

/// A Lucide path drawn in the cut: square caps, mitred joins.
macro_rules! mark_svg {
    ($d:literal) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
            r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d=""#,
            $d,
            r#""/></svg>"#
        )
        .as_bytes()
    };
}

/// The card's own width on the board, narrowed only by a narrow window.
pub const CARD_WIDTH: f32 = 448.;
const PAD: f32 = 16.;
const GAP: f32 = 12.;
const FIELD_H: f32 = 30.;
const NOTE_H: f32 = 16.;
/// The board's --r corner.
pub const RADIUS: f32 = 8.;
/// The oracle dialog button's pointer washes in the label's ink, 8% hover and 14% press (v0.3.12 Buttons.js).
pub const WASH_HOVER: f32 = 0.08;
pub const WASH_PRESS: f32 = 0.14;
/// Unlock unavailable draws at this opacity, the board's disabled button.
pub const DISABLED_OPACITY: f32 = 0.55;

/// The words the card shows, built once per card change or language change, never in paint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardText {
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
        let size = if card.size > 0 { catalog.locale().format_size(card.size) } else { String::new() };
        Self {
            title: say(catalog, &title(&card.label)),
            esc: "esc".into(),
            subtitle: say(catalog, &subtitle(&card.label, &size, &card.crypto)),
            label: catalog.tr("device.unlock.passphrase").to_uppercase(),
            note: card.note.as_ref().map(|(n, _)| say(catalog, n)).unwrap_or_default(),
            cancel: catalog.tr("device.unlock.cancel"),
            unlock: catalog.tr("device.unlock.button"),
        }
    }
}

/// The card element for a card state and its words.
pub fn card(card: &Card, text: &CardText, ink: &CardInk, window_width: f32, press: Press) -> impl IntoElement {
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
    div()
        .id("g2-pl1-unlock-card")
        .flex()
        .flex_col()
        .flex_none()
        .w(px(CARD_WIDTH.min((window_width - 2. * GAP).max(0.))))
        .p(px(PAD))
        .gap(px(GAP))
        .bg(ink.surface)
        .border_1()
        .border_color(ink.frame)
        .rounded(px(RADIUS))
        .text_color(ink.foreground)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(9.))
                .font_weight(FontWeight::BOLD)
                .text_size(px(16.))
                .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text.title.clone()))
                .child(div().ml_auto().font_weight(FontWeight::NORMAL).text_size(px(12.)).text_color(ink.muted).child(text.esc.clone())),
        )
        .child(div().h(px(1.)).bg(ink.rule))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(9.))
                .text_size(px(14.))
                .child(Mark::new(DRIVE, 19., ink.foreground))
                .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text.subtitle.clone())),
        )
        .child(
            div()
                .mt(px(4.))
                .opacity(if unlocking { DISABLED_OPACITY } else { 1. })
                .child(div().text_size(px(12.)).line_height(px(14.)).mb(px(5.)).text_color(ink.muted).child(text.label.clone()))
                .child(
                    div()
                        .h(px(FIELD_H))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(9.))
                        .text_size(px(14.))
                        .bg(ink.ground)
                        .border_1()
                        .border_color(field_frame)
                        .rounded(px(RADIUS))
                        .when(!dots.is_empty(), |d| d.child(dots))
                        .when(card.focus == Some(Part::Field) && !unlocking, |d| {
                            let tuck = if card.field.dots() > 0 { -7. } else { 0. };
                            d.child(div().w(px(1.)).h(px(16.)).ml(px(tuck)).bg(ink.foreground))
                        })
                        .child(div().flex_1())
                        .child(Mark::new(EYE, 13., ink.frame)),
                ),
        )
        .child(
            div()
                .mt(px(-2.))
                .h(px(NOTE_H))
                .line_height(px(NOTE_H))
                .text_size(px(12.))
                .overflow_hidden()
                .text_color(if note_error { ink.error } else { ink.muted })
                .child(text.note.clone()),
        )
        .child(
            div()
                .mt(px(10.))
                .flex()
                .justify_end()
                .items_center()
                .gap(px(9.))
                .child(button("g2-pl1-cancel", &text.cancel, false, true, card.focus == Some(Part::Cancel), ink, press.cancel))
                .child(button("g2-pl1-unlock", &text.unlock, true, available, card.focus == Some(Part::Unlock), ink, press.unlock)),
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

/// The board's framed button: accent frame over a 14% wash when primary, foreground frame on focus, 8% hover, 14% press.
fn button(id: &'static str, label: &str, primary: bool, enabled: bool, focused: bool, ink: &CardInk, click: Click) -> impl IntoElement {
    let (frame, ring) = button_frame(primary, enabled, focused, ink);
    let wash = button_wash(primary, enabled, ink);
    // Pointer wash inside the frame: hover while over the button, press only while held inside it (pressShown).
    let pointer_wash = div()
        .id(SharedString::from(format!("{id}-wash")))
        .absolute()
        .inset_0()
        .rounded(px(RADIUS - if ring { RING } else { 1. }))
        .opacity(0.)
        .when_some(wash.hover, |d, c| d.group_hover(id, move |s| s.opacity(1.).bg(c)))
        .when_some(wash.press, |d, c| d.group_active(id, move |s| s.bg(c)));
    let pointer = div().id(id).group(id).absolute().inset_0().child(pointer_wash);
    div()
        .relative()
        .h(px(FIELD_H))
        .px(px(9.))
        .flex()
        .items_center()
        .text_size(px(14.))
        .text_color(ink.foreground)
        .whitespace_nowrap()
        .map(|d| if ring { d.border_2() } else { d.border_1() })
        .border_color(frame)
        .rounded(px(RADIUS))
        .when_some(wash.rest, |d, c| d.bg(c))
        .opacity(if enabled { 1. } else { DISABLED_OPACITY })
        .child(if enabled { pointer.on_click(move |event, window, cx| click(event, window, cx)) } else { pointer })
        .child(label.to_string())
}

const DRIVE: (&str, &[u8]) = ("flea-mark:drive", mark_svg!("M6 4h12l4 8v8H2v-8z M2 12h20 M6 16L6.01 16 M10 16L10.01 16"));
const EYE: (&str, &[u8]) = ("flea-mark:eye", mark_svg!("M12 5 22 12 12 19 2 12z M10 10h4v4h-4z"));

/// One cut mark at a fixed size, painted from its path with no allocation in paint.
pub struct Mark {
    key: &'static str,
    data: &'static [u8],
    size: f32,
    ink: Hsla,
}

impl Mark {
    pub fn new(mark: (&'static str, &'static [u8]), size: f32, ink: Hsla) -> Self {
        Self { key: mark.0, data: mark.1, size, ink }
    }
}

impl IntoElement for Mark {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Mark {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = px(self.size).into();
        style.size.height = px(self.size).into();
        style.flex_shrink = 0.;
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut Window, _: &mut App) {}
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let key = SharedString::new_static(self.key);
        let _ = window.paint_svg(bounds, key, Some(self.data), TransformationMatrix::unit(), self.ink, cx);
    }
}
