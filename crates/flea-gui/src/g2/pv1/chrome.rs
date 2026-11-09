//! A mounted view's 27 px bar: the row's mark, the name and its tail, the pager and its counter, the view words, the tools and close.
use super::{
    Hit, QuickLook,
    bar::{Name, bar_mark, click, name_parts, press, ring_slot},
    doc::{Chrome, StripItem},
    glyph::Glyph,
    look::{self, Look},
};
use gpui::{AnyElement, Context, MouseButton, SharedString, div, prelude::*, px};

fn strip_slot(
    item: &StripItem,
    look: &Look,
    state: &QuickLook,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    let hit = if item.glyph == Glyph::Close {
        Some(Hit::Close)
    } else {
        item.enabled.then_some(Hit::Strip(item.index))
    };
    let ring = item.focused || (hit.is_some() && state.focused == hit);
    let index = item.index;
    let hover = state.observations.clone();
    div()
        .id(SharedString::from(format!("pv1-strip-observe-{index}")))
        .relative()
        .flex_none()
        .child(ring_slot(
            item.glyph,
            hit,
            look::SLOT_H,
            ring,
            look,
            state,
            cx,
        ))
        .child(super::observations::bounds(
            &state.observations,
            move |observed, bounds| {
                if let Some(slot) = observed.strip.get_mut(index) {
                    *slot = Some(bounds);
                }
            },
        ))
        .on_hover(move |over, _, _| {
            if let Some(slot) = hover.borrow_mut().strip_hovered.get_mut(index) {
                *slot = *over;
            }
        })
        .into_any_element()
}

/// A bare caption word: the view in effect, the sheet shown or the plate chosen in foreground, the others muted.
fn word(
    index: usize,
    text: &str,
    on: bool,
    look: &Look,
    state: &QuickLook,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    let hit = Hit::Word(index);
    let mut word = div()
        .id(SharedString::from(format!("pv1-word-{index}")))
        .debug_selector(move || format!("pv1-word-{index}"))
        .relative()
        .flex_none()
        .whitespace_nowrap()
        .text_size(px(look::CAPTION))
        .text_color(if on { look.foreground } else { look.muted })
        .cursor_pointer()
        .child(SharedString::from(text.to_owned()))
        .on_mouse_down(MouseButton::Left, cx.listener(press(hit)))
        .on_click(cx.listener(click(hit)));
    if state.focused == Some(hit) {
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

/// The bar over a mounted view; a page kind's bar is filled, as the pane under it is bare.
pub fn bar(
    glyph: Glyph,
    name: &str,
    chrome: &Chrome,
    filled: bool,
    look: &Look,
    state: &QuickLook,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    let name = Name {
        text: name,
        tail: chrome.tail.as_deref().map(|tail| (tail, look.muted)),
    };
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
        .child(bar_mark(glyph, look))
        .children(name_parts(&name, look));
    if filled {
        row = row.bg(look.surface);
    }
    let shown: Vec<&StripItem> = chrome.strip.iter().filter(|item| item.visible).collect();
    let pager = |g: Glyph| shown.iter().find(|item| item.glyph == g).copied();
    // ql-pdf3: the pager and its count follow the name; the tools sit at the right end. The pager keeps its order when mirrored.
    let mut pages = div().flex().flex_none().items_center().gap(px(look::GAP));
    if let Some(item) = pager(Glyph::ChevronLeft) {
        pages = pages.child(strip_slot(item, look, state, cx));
    }
    if let Some(counter) = &chrome.counter {
        pages = pages.child(
            div()
                .debug_selector(|| "pv1-counter".into())
                .flex_none()
                .text_color(look.foreground)
                .child(SharedString::from(counter.clone())),
        );
    }
    if let Some(item) = pager(Glyph::ChevronRight) {
        pages = pages.child(strip_slot(item, look, state, cx));
    }
    if chrome.counter.is_some()
        || pager(Glyph::ChevronLeft)
            .or(pager(Glyph::ChevronRight))
            .is_some()
    {
        row = row.child(crate::locale_geometry::unmirrored(pages));
    }
    row = row.child(div().flex_1());
    let mut group = div().flex().flex_none().items_center().gap(px(look::GAP));
    if !chrome.words.is_empty() {
        // The view words keep their own wider gap; the tools and close follow at the bar's gap.
        let mut words = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(look::WORD_GAP));
        for (index, (text, on)) in chrome.words.iter().enumerate() {
            words = words.child(word(index, text, *on, look, state, cx));
        }
        group = group.child(words);
    }
    let tools = shown
        .iter()
        .filter(|item| !matches!(item.glyph, Glyph::ChevronLeft | Glyph::ChevronRight));
    let mut closed = false;
    for item in tools {
        closed |= item.glyph == Glyph::Close;
        group = group.child(strip_slot(item, look, state, cx));
    }
    let pdf = state
        .subject
        .as_ref()
        .and_then(|subject| subject.body.doc())
        .is_some_and(|doc| doc.kind() == "pdf");
    if !closed && !pdf {
        let ring = chrome.close_focused || state.focused == Some(Hit::Close);
        group = group.child(ring_slot(
            Glyph::Close,
            Some(Hit::Close),
            look::SLOT_H,
            ring,
            look,
            state,
            cx,
        ));
    }
    row.child(group).into_any_element()
}
