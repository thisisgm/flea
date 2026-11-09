//! Draws the keyboard's caps with the theme's inks: each cap a role at an alpha.
use super::super::SheetView;
use super::{keyboard_height, keyboard_width, CAP_GAP, ROW_GAP};
use crate::g2::st1::seams::Ink;
use crate::g2::st1::sheet::Hover;
use crate::leaf::Painted;
use crate::locale_geometry::{unmirrored, Unmirrored};
use flea::keys::physical::Part;
use flea::ui_logic_st1::sheet_keyboard::{Face, Look};
use gpui::{div, prelude::*, px, Div, Hsla, SharedString, Window};

const RADIUS: f32 = 3.;

struct Inks {
    edge: Hsla,
    fill: Hsla,
    text: Hsla,
    width: f32,
}

/// Fills are blended onto the card so a cap drawn in two pieces never doubles where they meet.
fn inks(look: Look, ink: &Ink) -> Inks {
    let on = |colour: Hsla, alpha: f32| ink.surface.blend(colour.opacity(alpha));
    match look {
        Look::Faint => Inks {
            edge: on(ink.foreground, 0.22),
            fill: on(ink.foreground, 0.02),
            text: on(ink.foreground, 0.5),
            width: 1.,
        },
        Look::Bound => Inks {
            edge: on(ink.foreground, 0.5),
            fill: on(ink.foreground, 0.035),
            text: on(ink.foreground, 0.92),
            width: 1.,
        },
        // A lit legend reads in the foreground, so a light theme's accent wash keeps its contrast.
        Look::Lit => Inks {
            edge: ink.accent,
            fill: on(ink.accent, 0.32),
            text: ink.foreground,
            width: 1.5,
        },
    }
}

/// One cap as its look draws it: the edge and fill of its look, its legend, its shifted legend in the corner.
fn cap(face: &Face, look: Look, unit: f32, ink: &Ink) -> Div {
    let cap = &face.cap;
    let Inks {
        edge,
        fill,
        text,
        width,
    } = inks(look, ink);
    let top = if cap.row == 0 {
        0.
    } else {
        f32::from(cap.row) * unit + ROW_GAP
    };
    let pad = unit * 0.14;
    let chars = face.legend.chars().count() as f32;
    let letter = unit * 0.30;
    // A name shrinks to fit its cap on one line only in a narrow window.
    let room = cap.w * unit - CAP_GAP - 2. * pad;
    let size = if chars <= 1. {
        letter
    } else {
        (unit * 0.24).min(room / (chars * 0.62))
    }
    .max(7.);
    // Every legend shares the letters' baseline, whatever its size: a smaller one hangs lower by its ascent.
    let drop = (letter - size).max(0.) * 0.95;
    let mut key = div()
        .absolute()
        .left(px(cap.x * unit))
        .top(px(top))
        .w(px(cap.w * unit - CAP_GAP))
        .h(px(unit - CAP_GAP))
        .px(px(pad))
        .pt(px(unit * 0.08 + drop))
        .bg(fill)
        .border_color(edge)
        .text_color(text)
        .text_size(px(size))
        .line_height(px(size + 2.))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(SharedString::from(face.legend.clone()))
        .debug_selector({
            let id = cap.id;
            move || format!("sheet-cap-{id}")
        });
    key = match cap.part {
        Part::Whole => key.border(px(width)).rounded(px(RADIUS)),
        // ISO Enter's top runs through the row gap to the foot's row, where the foot carries on below it.
        Part::EnterTop => key
            .h(px(unit))
            .border_t(px(width))
            .border_l(px(width))
            .border_r(px(width))
            .rounded_t(px(RADIUS)),
        Part::EnterFoot => key
            .border_b(px(width))
            .border_l(px(width))
            .border_r(px(width))
            .rounded_b(px(RADIUS)),
    };
    if !face.shifted.is_empty() {
        key = key.child(
            div()
                .absolute()
                .right(px(pad))
                .bottom(px(unit * 0.06))
                .text_size(px((unit * 0.22).max(8.)))
                .child(SharedString::from(face.shifted.clone())),
        );
    }
    key
}

/// The step on ISO Enter's left side, where the wide top meets the narrower foot.
fn step(top: &Face, foot: &Face, look: Look, unit: f32, ink: &Ink) -> Div {
    let Inks { edge, width, .. } = inks(look, ink);
    let y = f32::from(foot.cap.row) * unit + ROW_GAP;
    div()
        .absolute()
        .left(px(top.cap.x * unit))
        .top(px(y))
        .w(px((foot.cap.x - top.cap.x) * unit + width))
        .h(px(width))
        .bg(edge)
}

impl SheetView {
    /// The drawn keyboard, when this window has room for it.
    pub(in crate::g2::st1::sheet_view) fn keyboard_band(
        &self,
        window: &Window,
        ink: &Ink,
    ) -> Option<Painted<Unmirrored>> {
        let unit = self.unit(window)?;
        let keyboard = self.sheet.keyboard.as_ref()?;
        let sheet = &self.sheet;
        // A search lights its highlighted result's keys until a modifier is held over it.
        // An empty search has highlighted nothing yet, so the keyboard keeps its rest look until a word or an arrow.
        let chosen = sheet.chosen();
        let highlighted = sheet
            .ranked
            .get(sheet.cursor)
            .filter(|_| self.searching() && chosen);
        // A row under the pointer lights its keys, as the highlighted result does.
        let resting = sheet.held == 0 && sheet.chord.is_none();
        let looks = match (&sheet.hover, highlighted) {
            // As a rest row does, a held row lights only its own keys.
            (Some(Hover::Binding { mask, cap }), _) if !resting => keyboard
                .looks(*mask, Some(*cap))
                .into_iter()
                .map(|look| if look == Look::Lit { look } else { Look::Faint })
                .collect(),
            (Some(Hover::Action(action)), None) if resting && !self.searching() => {
                keyboard.looks_for(action)
            }
            (_, Some(row)) if sheet.held == 0 => keyboard.looks_for(&row.action),
            _ => keyboard.looks(sheet.held, sheet.chord),
        };
        let faces = &keyboard.faces;
        let mut band = div()
            .flex_none()
            .relative()
            .self_center()
            .w(px(keyboard_width(unit)))
            .h(px(keyboard_height(unit)))
            .debug_selector(|| "sheet-keyboard".into());
        for (face, look) in faces.iter().zip(looks.iter().copied()) {
            band = band.child(cap(face, look, unit, ink));
            if face.cap.part == Part::EnterFoot {
                if let Some(top) = faces.iter().find(|top| top.cap.part == Part::EnterTop) {
                    band = band.child(step(top, face, look, unit, ink));
                }
            }
        }
        // A keyboard is a thing on the desk: a right-to-left window draws it as it lies, Esc on the left.
        Some(unmirrored(band))
    }
}
