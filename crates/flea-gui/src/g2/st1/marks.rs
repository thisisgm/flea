//! The small drawing pieces every Settings and sheet row shares: a mark, a spaced capital heading, a washed run.
use super::{
    geometry::{Scale, CHECK},
    glyphs,
    seams::Ink,
};
use flea::ui_logic_st1::rows::Master;
use gpui::{
    canvas, div, prelude::*, px, App, ClickEvent, Div, FontWeight, HighlightStyle, Hsla,
    MouseButton, SharedString, StyledText, TransformationMatrix, Window,
};
use std::{ops::Range, rc::Rc};

#[cfg(test)]
#[path = "rtl_keycap_match_tests.rs"]
mod rtl_keycap_match_tests;

/// A mark from the boards' path data, drawn as an alpha mask in one ink; a name no board draws keeps its slot empty.
pub fn mark(name: &str, size: f32, ink: Hsla) -> Div {
    let slot = div().flex_none().size(px(size));
    let Some((key, data)) = glyphs::svg(name) else {
        return slot;
    };
    slot.child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                #[cfg(test)]
                let observed = key.clone();
                let _painted = window.paint_svg(
                    bounds,
                    key,
                    Some(data),
                    TransformationMatrix::unit(),
                    ink,
                    cx,
                );
                #[cfg(test)]
                if _painted.is_ok() {
                    super::about_logo_gap64_tests::record(&observed, data, bounds, ink);
                }
            },
        )
        .size_full(),
    )
}

/// Letter spacing GPUI's text lacks: each character in its own box with the gap after it; joining scripts stay one run.
pub fn spaced(text: &str, spacing: f32) -> Div {
    washed(text, None, false, Hsla::default(), spacing)
}

fn spaceable(letter: char) -> bool {
    letter.is_ascii()
        || matches!(letter as u32, 0x00C0..=0x024F | 0x0370..=0x03FF | 0x0400..=0x04FF | 0x1E00..=0x1FFF)
}

/// A label with its typed run (in characters) washed in accent, or bold with no wash inside the highlighted row.
pub fn washed(
    label: &str,
    run: Option<(usize, usize)>,
    lifted: bool,
    wash: Hsla,
    spacing: f32,
) -> Div {
    let (start, end) = run.map_or((0, 0), |(start, length)| (start, start + length));
    let dress = move |piece: Div, inside: bool| match (inside, lifted) {
        (false, _) => piece,
        (true, true) => piece.font_weight(FontWeight::BOLD),
        (true, false) => piece.bg(wash),
    };
    if spacing > 0. && label.chars().all(spaceable) {
        return div()
            .flex()
            .children(label.chars().enumerate().map(|(index, letter)| {
                dress(
                    div()
                        .mr(px(spacing))
                        .child(SharedString::from(letter.to_string())),
                    index >= start && index < end,
                )
            }));
    }
    if run.is_none() {
        return div().child(SharedString::from(label.to_owned()));
    }
    // One shaped line, so a mirrored window moves the label whole and never reorders its pieces.
    div().whitespace_nowrap().child(
        StyledText::new(label.to_owned())
            .with_highlights([highlight(label, start, end, lifted, wash)]),
    )
}

/// The typed run's byte range and its style: bold on the highlighted row, else the accent wash.
fn highlight(
    label: &str,
    start: usize,
    end: usize,
    lifted: bool,
    wash: Hsla,
) -> (Range<usize>, HighlightStyle) {
    let at = |chars: usize| {
        label
            .char_indices()
            .nth(chars)
            .map_or(label.len(), |(byte, _)| byte)
    };
    let style = if lifted {
        HighlightStyle {
            font_weight: Some(FontWeight::BOLD),
            ..HighlightStyle::default()
        }
    } else {
        HighlightStyle {
            background_color: Some(wash),
            ..HighlightStyle::default()
        }
    };
    (at(start)..at(end), style)
}

/// An overlay's ground: sinks hover, presses and the wheel, and a left or right pointer click closes; a keyboard click never does.
pub fn ground<E: StatefulInteractiveElement>(
    element: E,
    close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> E {
    let close = Rc::new(close);
    let aux = Rc::clone(&close);
    element
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .on_click(move |event, window, cx| {
            if !event.is_keyboard() {
                close(event, window, cx);
            }
        })
        .on_aux_click(move |event, window, cx| {
            if event.is_right_click() {
                aux(event, window, cx);
            }
        })
}

/// A card on that ground keeps its own presses and wheel, so only the ground around it closes.
pub fn card<E: InteractiveElement>(element: E) -> E {
    element
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

/// The checkbox every surface draws: empty against filled, the tick and the mixed bar cut in the surface.
pub fn check(state: Master, ink: &Ink, corner: f32, s: Scale) -> Div {
    let filled = matches!(state, Master::All | Master::Some);
    div()
        .size(px(s.of(CHECK)))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .border_2()
        .rounded(px(corner / 2.))
        .border_color(if filled {
            ink.foreground
        } else {
            ink.muted_frame
        })
        .when(filled, |box_| box_.bg(ink.foreground))
        .when(state == Master::All, |box_| {
            box_.child(mark("checkmark-bold", s.of(12.), ink.surface))
        })
        .when(state == Master::Some, |box_| {
            box_.child(div().w(px(s.of(8.))).h(px(2.)).bg(ink.surface))
        })
}
