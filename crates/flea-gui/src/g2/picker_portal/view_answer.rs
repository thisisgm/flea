//! The existing picker answer button recipe.
use super::super::drawn;
use super::*;

// DialogButton's recipe: every button framed, the default an accent frame on the wash; a refused default loses both.
pub(super) fn answer(
    id: &'static str,
    label: SharedString,
    default: bool,
    available: bool,
    (palette, size): (&Palette, Size),
    drawn: &drawn::Drawn,
    slot: Slot,
) -> Stateful<Div> {
    let lit = default && available;
    let hover = drawn.clone();
    let down = drawn.clone();
    let up = drawn.clone();
    let outside = drawn.clone();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .relative()
        .children(drawn.probe(slot, [1.; 4]))
        .on_hover(move |hovered, _, _| hover.pointer(slot, Some(*hovered), None))
        .on_mouse_down(MouseButton::Left, move |_, _, _| {
            down.pointer(slot, None, Some(true))
        })
        .on_mouse_up(MouseButton::Left, move |_, _, _| {
            up.pointer(slot, None, Some(false))
        })
        .on_mouse_up_out(MouseButton::Left, move |_, _, _| {
            outside.pointer(slot, None, Some(false))
        })
        .flex_none()
        .flex()
        .items_center()
        .h(size.px(BUTTON_HEIGHT))
        .px(size.px(BUTTON_SIDE))
        .text_size(px(size.body))
        .text_color(if available {
            palette.foreground
        } else {
            palette.muted
        })
        .border_1()
        .border_color(if lit {
            palette.accent_frame
        } else {
            crate::presentation::frames::Frames::from_palette(palette, 0.).ink
        })
        .when(lit, |button| button.bg(palette.accent.opacity(WASH)))
        .when(!available, |button| button.opacity(REFUSED))
        .child(label)
}
