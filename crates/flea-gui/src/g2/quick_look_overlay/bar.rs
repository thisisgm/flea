//! Quick Look's 27 px bar and Compare's 26 px refusal bar: the kind mark, the name, its tail and the chrome controls.
use super::{
    glyph::Glyph,
    look::{self, Look},
    Hit, QuickLook,
};
use gpui::{
    canvas, div, prelude::*, px, AnyElement, ClickEvent, Context, Hsla, MouseButton,
    MouseDownEvent, Role, SharedString,
};

/// What the bar names after the mark.
pub struct Name<'a> {
    pub text: &'a str,
    /// The muted count ("4 lines") or the link's metadata-ink target ("→ photo.png").
    pub tail: Option<(&'a str, Hsla)>,
}

pub fn mark(glyph: Glyph, size: f32, ink: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| glyph.paint(bounds, ink, window, cx),
    )
    .size(px(size))
    .flex_none()
}

/// The bar's leading mark, tagged with the glyph it draws so a test reads the painted mark, not the model.
pub(super) fn bar_mark(
    glyph: Glyph,
    look: &Look,
    observations: &super::observations::Shared,
) -> impl IntoElement {
    let key = glyph.svg().0;
    let captured = observations.clone();
    let ink = look.foreground;
    div()
        .flex_none()
        .debug_selector(move || format!("pv1-bar-mark:{key}"))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    glyph.paint(bounds, ink, window, cx);
                    captured.borrow_mut().bar_mark = Some((glyph.svg().0, ink, bounds));
                },
            )
            .size(px(look::CHROME_MARK))
            .flex_none(),
        )
}

pub(super) fn name_parts(
    name: &Name,
    look: &Look,
    observations: &super::observations::Shared,
) -> Vec<AnyElement> {
    let mut parts = vec![div()
        .relative()
        .child(super::observations::bounds(
            observations,
            |observed, bounds| observed.title = Some(bounds),
        ))
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_middle()
        .text_color(look.foreground)
        .child(SharedString::from(
            flea::bytename::display(name.text).into_owned(),
        ))
        .into_any_element()];
    if let Some((tail, ink)) = name.tail {
        parts.push(
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_color(ink)
                .child(SharedString::from(
                    flea::bytename::display(tail).into_owned(),
                ))
                .into_any_element(),
        );
    }
    parts
}

/// The 27 px bar: 14 px padding, 9 px gaps, a 12 px caption, the hairline under it; filled where the pane under it is bare.
pub fn bar(
    glyph: Glyph,
    name: Name,
    copy: Option<&str>,
    filled: bool,
    look: &Look,
    state: &QuickLook,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    let mut row = div()
        .id("pv1-bar")
        .debug_selector(|| "pv1-bar".into())
        .h(px(look::BAR))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(look::GAP))
        .px(px(look::PAD))
        .border_b_1()
        .border_color(look.hair())
        .text_size(px(look::CAPTION))
        .whitespace_nowrap()
        .child(bar_mark(glyph, look, &state.observations))
        .children(name_parts(&name, look, &state.observations))
        .child(div().flex_1());
    if filled {
        row = row.bg(look.surface);
    }
    let controls = state
        .subject
        .as_ref()
        .map(|s| s.body.controls().to_vec())
        .unwrap_or_default();
    let mut group = div().flex().flex_none().items_center().gap(px(look::GAP));
    if let Some(copy) = copy {
        group = group.child(word(copy, look, state, cx));
    }
    for (index, control) in controls.iter().enumerate() {
        let hit = control.enabled.then_some(Hit::Control(index));
        group = group.child(slot(control.glyph, hit, look::SLOT_H, look, state, cx));
    }
    group = group.child(slot(
        Glyph::Close,
        Some(Hit::Close),
        look::SLOT_H,
        look,
        state,
        cx,
    ));
    row.child(group).into_any_element()
}

/// Compare's refusal bar: 26 px tall, 13 px padding, the title 9 px after the mark, and a 24 by 25 close slot.
pub fn refusal_bar(
    glyph: Glyph,
    title: &str,
    look: &Look,
    state: &QuickLook,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    div()
        .id("pv1-bar")
        .debug_selector(|| "pv1-bar".into())
        .h(px(look::REFUSAL_BAR))
        .flex_none()
        .flex()
        .items_center()
        .px(px(look::REFUSAL_PAD))
        .border_b_1()
        .border_color(look.hair())
        .text_size(px(look::CAPTION))
        .child(bar_mark(glyph, look, &state.observations))
        .child(
            div()
                .relative()
                .child(super::observations::bounds(
                    &state.observations,
                    |observed, bounds| observed.title = Some(bounds),
                ))
                .ml(px(look::GAP))
                .text_color(look.foreground)
                .child(SharedString::from(title.to_owned())),
        )
        .child(div().flex_1())
        .child(slot(
            Glyph::Close,
            Some(Hit::Close),
            look::REFUSAL_SLOT_H,
            look,
            state,
            cx,
        ))
        .into_any_element()
}

pub(super) fn press(
    hit: Hit,
) -> impl Fn(&mut QuickLook, &MouseDownEvent, &mut gpui::Window, &mut Context<QuickLook>) {
    move |look, _, _, cx| {
        look.chrome_armed = look.is_revealed();
        if !look.is_revealed() {
            return;
        }
        look.pressed = Some(hit);
        cx.notify();
    }
}

pub(super) fn click(
    hit: Hit,
) -> impl Fn(&mut QuickLook, &ClickEvent, &mut gpui::Window, &mut Context<QuickLook>) {
    move |look, _, _, cx| {
        if !std::mem::take(&mut look.chrome_armed) || !look.is_revealed() {
            return;
        }
        look.pressed = None;
        look.activate(hit, cx);
    }
}

/// ChromeButton: a foreground mark, a 20 px foreground ring when focused, 0.96 of its size pressed, muted frame at 0.55 disabled.
fn slot(
    glyph: Glyph,
    hit: Option<Hit>,
    height: f32,
    look: &Look,
    state: &QuickLook,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    let ring = hit.is_some() && state.focused == hit;
    ring_slot(glyph, hit, height, ring, look, state, cx)
}

/// A chrome slot whose focus ring the caller decides, for a strip whose view keeps its own focus.
pub(super) fn ring_slot(
    glyph: Glyph,
    hit: Option<Hit>,
    height: f32,
    ring: bool,
    look: &Look,
    state: &QuickLook,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    let id = match hit {
        Some(Hit::Close) => "pv1-close".to_owned(),
        Some(Hit::Control(i)) => format!("pv1-control-{i}"),
        Some(Hit::Strip(i)) => format!("pv1-strip-{i}"),
        _ => format!("pv1-disabled-{}", glyph.svg().0),
    };
    let selector = id.clone();
    let pressed = hit.is_some() && state.pressed == hit;
    let size = if pressed {
        look::CHROME_MARK * look::PRESS_SCALE
    } else {
        look::CHROME_MARK
    };
    let mut slot = div()
        .id(SharedString::from(id))
        .role(Role::Button)
        .debug_selector(move || selector.clone())
        .relative()
        .w(px(look::SLOT_W))
        .h(px(height))
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    if let Some(key) = glyph.access_key() {
        slot = slot.aria_label(state.i18n.tr(key));
    }
    match hit {
        Some(hit) => {
            if ring {
                slot = slot.child(
                    div()
                        .debug_selector(|| "pv1-ring".into())
                        .absolute()
                        .left(px((look::SLOT_W - look::RING) / 2.))
                        .top(px(((height - look::RING) / 2.).ceil()))
                        .size(px(look::RING))
                        .border_1()
                        .border_color(look.foreground)
                        .rounded(look.control_corner / 2.),
                );
            }
            slot = slot
                .cursor_pointer()
                .child(mark(glyph, size, look.foreground))
                .on_mouse_down(MouseButton::Left, cx.listener(press(hit)))
                .on_click(cx.listener(click(hit)));
        }
        None => {
            slot = slot.child(div().opacity(look::DISABLED).child(mark(
                glyph,
                size,
                look.muted_frame,
            )));
        }
    }
    if hit == Some(Hit::Close) {
        super::observations::close_slot(slot, &state.observations).into_any_element()
    } else {
        slot.into_any_element()
    }
}

/// Copy Patch: a bare caption word in the foreground, as the bar's marks are bare (CHANGES compare).
fn word(text: &str, look: &Look, state: &QuickLook, cx: &mut Context<QuickLook>) -> AnyElement {
    let focused = state.focused == Some(Hit::CopyPatch);
    let observed = state.observations.clone();
    let mut word = div()
        .id("pv1-copy-patch")
        .debug_selector(|| "pv1-copy-patch".into())
        .relative()
        .flex_none()
        .whitespace_nowrap()
        .text_size(px(look::CAPTION))
        .text_color(look.foreground)
        .cursor_pointer()
        .on_hover(move |hovered, _, _| observed.borrow_mut().copy_hovered = *hovered)
        .child(SharedString::from(text.to_owned()))
        .child(super::observations::bounds(
            &state.observations,
            |observed, bounds| observed.copy = Some(bounds),
        ))
        .on_mouse_down(MouseButton::Left, cx.listener(press(Hit::CopyPatch)))
        .on_click(cx.listener(click(Hit::CopyPatch)));
    if focused {
        word = word.child(
            div()
                .absolute()
                .inset_0()
                .border_1()
                .border_color(look.foreground)
                .rounded(look.control_corner / 2.),
        );
    }
    word.into_any_element()
}
