//! The small drawing pieces every Settings and sheet row shares: a mark, a spaced capital heading, a washed run.
use super::glyphs;
use gpui::{canvas, div, prelude::*, px, App, ClickEvent, Div, FontWeight, Hsla, MouseButton, SharedString, TransformationMatrix, Window};
use std::rc::Rc;

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
                let _ = window.paint_svg(bounds, key, Some(data), TransformationMatrix::unit(), ink, cx);
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
    letter.is_ascii() || matches!(letter as u32, 0x00C0..=0x024F | 0x0370..=0x03FF | 0x0400..=0x04FF | 0x1E00..=0x1FFF)
}

/// A label with its typed run (in characters) washed in accent, or bold with no wash inside the highlighted row.
pub fn washed(label: &str, run: Option<(usize, usize)>, lifted: bool, wash: Hsla, spacing: f32) -> Div {
    let (start, end) = run.map_or((0, 0), |(start, length)| (start, start + length));
    let dress = move |piece: Div, inside: bool| match (inside, lifted) {
        (false, _) => piece,
        (true, true) => piece.font_weight(FontWeight::BOLD),
        (true, false) => piece.bg(wash),
    };
    if spacing > 0. && label.chars().all(spaceable) {
        return div().flex().children(label.chars().enumerate().map(|(index, letter)| {
            dress(div().mr(px(spacing)).child(SharedString::from(letter.to_string())), index >= start && index < end)
        }));
    }
    if run.is_none() {
        return div().child(SharedString::from(label.to_owned()));
    }
    let at = |chars: usize| label.char_indices().nth(chars).map_or(label.len(), |(byte, _)| byte);
    let (from, to) = (at(start), at(end));
    div()
        .flex()
        .whitespace_nowrap()
        .child(SharedString::from(label[..from].to_owned()))
        .child(dress(div().child(SharedString::from(label[from..to].to_owned())), true))
        .child(SharedString::from(label[to..].to_owned()))
}

/// An overlay's ground: sinks hover, presses and the wheel, and a left or right pointer click closes; a keyboard click never does.
pub fn ground<E: StatefulInteractiveElement>(element: E, close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> E {
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
