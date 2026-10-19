// The strip's geometry from the board, and a real frame: one shaping per change, nothing allocated on a still frame.
use super::strip::{
    layout, Palette, StripElement, StripState, Words, CARET_GAP, CARET_W, GAP, MARK, PAD_X,
};
use crate::list::bench::{allocations, bench_typeface, real_cx};
use crate::list::Palette as ListPalette;
use crate::palette::Theme;
use gpui::{
    div, prelude::*, px, size, Context, IntoElement, Render, TestAppContext, Window, WindowHandle,
};
use std::{cell::RefCell, rc::Rc};

#[test]
fn the_strip_lays_out_left_to_right_and_the_scope_gives_way_first() {
    let wide = layout(800., 40., 120., 60., 70.);
    assert_eq!((wide.mark_x, wide.query_x), (PAD_X, PAD_X + MARK + GAP));
    assert_eq!(wide.caret_x, wide.query_x + 40. + CARET_GAP);
    assert_eq!(wide.scope_x, wide.caret_x + CARET_W + GAP);
    assert_eq!(
        (wide.scope_w, wide.note_x),
        (120., wide.scope_x + 120. + GAP)
    );
    assert_eq!(wide.way_x, 800. - PAD_X - 70.);
    // A narrow strip cuts the scope down to what is left beside the count and the way out, never below zero.
    let narrow = layout(300., 40., 120., 60., 70.);
    assert_eq!(
        narrow.scope_w,
        (narrow.way_x - narrow.scope_x - 60. - 2. * GAP).max(0.)
    );
    assert!(narrow.scope_w < 120.);
    assert_eq!(layout(120., 40., 120., 60., 70.).scope_w, 0.);
}

struct View {
    words: Rc<Words>,
    state: Rc<RefCell<StripState>>,
}

impl Render for View {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let list = ListPalette::inherited(&window.text_style(), Theme::default());
        let palette = Palette::from_list(&list);
        div().size_full().child(StripElement::new(
            Rc::clone(&self.words),
            palette,
            Rc::clone(&self.state),
        ))
    }
}

fn open(
    cx: &mut TestAppContext,
    words: Words,
    width: f32,
) -> (WindowHandle<View>, Rc<RefCell<StripState>>) {
    let state = Rc::new(RefCell::new(StripState::new(bench_typeface())));
    let held = Rc::clone(&state);
    let window = cx.open_window(size(px(width), px(27.)), |_, _| View {
        words: Rc::new(words),
        state: held,
    });
    (window, state)
}

fn frame(cx: &mut TestAppContext, window: WindowHandle<View>, words: Option<Words>) {
    if let Some(words) = words {
        window
            .update(cx, |view, _, _| view.words = Rc::new(words))
            .expect("the strip window is open");
    }
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("the strip window is open");
}

fn words(query: &str, scope: &str) -> Words {
    Words {
        query: query.into(),
        scope: scope.into(),
        note: "9 found".into(),
        way_out: "esc returns".into(),
        typing: true,
    }
}

#[test]
fn a_still_strip_shapes_once_and_a_key_reshapes_once() {
    let mut cx = real_cx();
    let (window, state) = open(&mut cx, words("rep", "in ~/Work"), 900.);
    for _ in 0..3 {
        frame(&mut cx, window, None);
    }
    assert_eq!(state.borrow().shapings, 1);
    let heap = allocations();
    for _ in 0..10 {
        frame(&mut cx, window, None);
    }
    let still = allocations() - heap;
    assert_eq!(
        state.borrow().shapings,
        1,
        "a still frame reshaped the strip"
    );
    frame(&mut cx, window, Some(words("repo", "in ~/Work")));
    assert_eq!(state.borrow().shapings, 2);
    let heap = allocations();
    for _ in 0..10 {
        frame(&mut cx, window, None);
    }
    assert_eq!(
        allocations() - heap,
        still,
        "the strip's own paint allocated"
    );
}

#[test]
fn a_narrow_strip_keeps_the_scopes_tail_behind_an_ellipsis() {
    let mut cx = real_cx();
    let scope = "in ~/Work/clients/northwind/reports/quarterly";
    let (window, state) = open(&mut cx, words("q", scope), 330.);
    frame(&mut cx, window, None);
    let state = state.borrow();
    let prepared = state.prepared.as_ref().expect("the strip shaped");
    assert!(
        prepared.scope_text.starts_with('\u{2026}'),
        "{}",
        prepared.scope_text
    );
    assert!(scope.ends_with(&prepared.scope_text['\u{2026}'.len_utf8()..]));
    assert!(f32::from(prepared.scope.line().width) <= prepared.boxes.scope_w + 0.5);
    let mut wide_cx = real_cx();
    let (window, state) = open(&mut wide_cx, words("q", scope), 1200.);
    frame(&mut wide_cx, window, None);
    assert_eq!(
        state
            .borrow()
            .prepared
            .as_ref()
            .expect("the strip shaped")
            .scope_text,
        scope
    );
}
