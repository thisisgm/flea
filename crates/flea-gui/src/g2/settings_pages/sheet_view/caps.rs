//! Draws the keyboard's caps with the theme's inks: each cap a role at an alpha.
use super::super::{fit, SheetView};
use super::{keyboard_height, keyboard_width, CAP_GAP, ROW_GAP};
use crate::g2::settings_pages::seams::Ink;
use crate::g2::settings_pages::sheet::Hover;
use crate::leaf::Painted;
use crate::locale_geometry::{unmirrored, Unmirrored};
use flea::keys::physical::{Cap, Part};
use flea::ui_logic_settings_pages::sheet_keyboard::{Face, Look};
use gpui::{div, prelude::*, px, Div, Hsla, SharedString, Window};

/// A cap rounds at half the compositor's corner, as Flea's other small controls do: square where windows are square.
fn radius(corner: f32) -> f32 {
    corner / 2.
}

/// A row's top: the function row at the band's top, the main block below the function row's gap.
fn row_top(row: u8, unit: f32) -> f32 {
    if row == 0 {
        0.
    } else {
        f32::from(row) * unit + ROW_GAP
    }
}

/// A cap's left edge and width, snapped edge by edge: caps that end on the same unit share one pixel edge at any
/// unit, so ISO Enter's two pieces keep one straight right side.
fn span(x: f32, w: f32, unit: f32) -> (f32, f32) {
    let left = (x * unit).round();
    let right = ((x + w) * unit).round() - CAP_GAP;
    (left, right - left)
}

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

/// A legend's size on a cap `width` units wide, and its shifted legend's. Both size with the key unit, which grows with
/// the text size (`scale`), and so do their floors: a name shrinks to fit its cap on one line only in a narrow window.
fn legend_sizes(legend: &str, width: f32, unit: f32, scale: f32) -> (f32, f32) {
    let chars = legend.chars().count() as f32;
    let room = width * unit - CAP_GAP - 2. * unit * 0.14;
    let size = if chars <= 1. {
        unit * 0.30
    } else {
        (unit * 0.24).min(room / (chars * 0.62))
    }
    .max(7. * scale);
    (size, (unit * 0.22).max(8. * scale))
}

/// One cap as its look draws it: the edge and fill of its look, its legend, its shifted legend in the corner.
fn cap(face: &Face, look: Look, unit: f32, scale: f32, corner: f32, ink: &Ink) -> Div {
    let cap = &face.cap;
    let round = px(radius(corner));
    let Inks {
        edge,
        fill,
        text,
        width,
    } = inks(look, ink);
    let top = row_top(cap.row, unit);
    let (left, wide) = span(cap.x, cap.w, unit);
    let pad = unit * 0.14;
    let letter = unit * 0.30;
    let (size, shifted) = legend_sizes(&face.legend, cap.w, unit, scale);
    // Every legend shares the letters' baseline, whatever its size: a smaller one hangs lower by its ascent.
    let drop = (letter - size).max(0.) * 0.95;
    let mut key = div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(wide))
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
        Part::Whole => key.border(px(width)).rounded(round),
        // ISO Enter's top is a whole cap of its row, so its bottom edge is the step, level with the row's other caps.
        Part::EnterTop => key.border(px(width)).rounded_t(round),
        // The foot keeps its row's top and height, so its bottom snaps onto the row's line as every cap's does; `join`
        // carries it up to the step.
        Part::EnterFoot => key
            .border_b(px(width))
            .border_l(px(width))
            .border_r(px(width))
            .rounded_b(round),
    };
    if !face.shifted.is_empty() {
        key = key.child(
            div()
                .absolute()
                .right(px(pad))
                .bottom(px(unit * 0.06))
                .text_size(px(shifted))
                .child(SharedString::from(face.shifted.clone())),
        );
    }
    key
}

/// The rounding of ISO Enter's step, its outer corner on the top's bottom left: the caps' own, as far as the step
/// reaches before the foot.
fn step_radius(top: &Face, foot: &Face, unit: f32, corner: f32) -> f32 {
    let (top_left, _) = span(top.cap.x, top.cap.w, unit);
    let (foot_left, _) = span(foot.cap.x, foot.cap.w, unit);
    radius(corner).min(foot_left - top_left)
}

/// A box in the band: left, top, width, height.
type Area = (f32, f32, f32, f32);

/// Where ISO Enter's foot meets its top: a neck carrying the foot's sides up through the row gap from the step line,
/// and a patch of fill over the top's bottom edge between those sides. Each reaches a pixel above and below its ends
/// into the same ink, so a piece snapped a pixel off leaves no seam.
fn join_areas(foot: &Cap, unit: f32, width: f32) -> (Area, Area) {
    let (left, wide) = span(foot.x, foot.w, unit);
    let step = row_top(foot.row - 1, unit) + unit - CAP_GAP;
    let neck = (left, step - 1., wide, row_top(foot.row, unit) - step + 2.);
    let patch = (
        left + width,
        step - width - 1.,
        wide - 2. * width,
        width + 2.,
    );
    (neck, patch)
}

/// The join drawn in the foot's look, over both pieces.
fn join(foot: &Face, look: Look, unit: f32, ink: &Ink) -> [Div; 2] {
    let Inks {
        edge, fill, width, ..
    } = inks(look, ink);
    let placed = |(x, y, w, h): Area| div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h));
    let (neck, patch) = join_areas(&foot.cap, unit, width);
    let sides = placed(neck).border_color(edge).border_l(px(width));
    [sides.border_r(px(width)).bg(fill), placed(patch).bg(fill)]
}

impl SheetView {
    /// The drawn keyboard, when this window has room for it.
    pub(in crate::g2::settings_pages::sheet_view) fn keyboard_band(
        &self,
        window: &Window,
        ink: &Ink,
    ) -> Option<Painted<Unmirrored>> {
        let unit = self.unit(window)?;
        let scale = fit::text_scale(&self.typeface);
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
        let foot = faces.iter().find(|face| face.cap.part == Part::EnterFoot);
        for (face, look) in faces.iter().zip(looks.iter().copied()) {
            let mut drawn = cap(face, look, unit, scale, self.corner, ink);
            if let (Part::EnterTop, Some(foot)) = (face.cap.part, foot) {
                drawn = drawn.rounded_bl(px(step_radius(face, foot, unit, self.corner)));
            }
            band = band.child(drawn);
        }
        // Drawn after every cap, so it lies over both of Enter's pieces whatever their order.
        if let Some((foot, look)) = faces
            .iter()
            .zip(looks.iter().copied())
            .find(|(face, _)| face.cap.part == Part::EnterFoot)
        {
            band = band.children(join(foot, look, unit, ink));
        }
        // A keyboard is a thing on the desk: a right-to-left window draws it as it lies, Esc on the left.
        Some(unmirrored(band))
    }
}

#[cfg(test)]
#[path = "caps_tests.rs"]
mod tests;
