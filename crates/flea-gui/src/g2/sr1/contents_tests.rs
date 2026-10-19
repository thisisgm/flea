// Contents blocks: fixed 55 px blocks from the board, only the visible ones shaped, and no allocation on a still frame.
use super::contents::Palette;
use super::contents::{visible, Block, ContentsElement, ContentsState, Held, BLOCK_H};
use crate::list::bench::{allocations, bench_typeface, fixture, real_cx};
use crate::list::Palette as ListPalette;
use crate::palette::Theme;
use gpui::{
    div, prelude::*, px, size, Context, IntoElement, Render, TestAppContext, Window, WindowHandle,
};
use std::{cell::RefCell, rc::Rc};

#[test]
fn a_block_is_the_row_its_line_and_the_air_under_it() {
    assert_eq!(BLOCK_H, 55.);
    assert_eq!(visible(0., 550., 100), 0..10);
    assert_eq!(visible(30., 550., 100), 0..11);
    assert_eq!(visible(0., 550., 4), 0..4);
    assert_eq!(visible(5500., 550., 100), 100..100);
    assert_eq!(visible(-20., 110., 100), 0..2);
}

fn held(count: usize) -> Rc<Held> {
    let blocks: Vec<Block> = fixture("uniform")
        .into_iter()
        .take(count)
        .enumerate()
        .map(|(at, row)| Block {
            row,
            location: "src".into(),
            line_no: at as u64 + 1,
            line: format!("let report_{at} = load(\"report\");"),
        })
        .collect();
    Rc::new(Held {
        listing: 1,
        total: blocks.len(),
        start: 0,
        blocks,
    })
}

struct View {
    held: Rc<Held>,
    scroll_y: f32,
    state: Rc<RefCell<ContentsState>>,
}

impl Render for View {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let list = ListPalette::inherited(&window.text_style(), Theme::default());
        let palette = Palette::from_list(&list);
        div().size_full().child(ContentsElement::new(
            Rc::clone(&self.held),
            self.scroll_y,
            0,
            palette,
            Rc::clone(&self.state),
        ))
    }
}

fn open(
    cx: &mut TestAppContext,
    held: Rc<Held>,
    blocks: f32,
) -> (WindowHandle<View>, Rc<RefCell<ContentsState>>) {
    let mut state = ContentsState::new(bench_typeface());
    state.query = "report".into();
    let state = Rc::new(RefCell::new(state));
    let kept = Rc::clone(&state);
    let window = cx.open_window(size(px(900.), px(blocks * BLOCK_H)), |_, _| View {
        held,
        scroll_y: 0.,
        state: kept,
    });
    (window, state)
}

fn frame(cx: &mut TestAppContext, window: WindowHandle<View>, scroll_y: f32) {
    window
        .update(cx, |view, _, _| view.scroll_y = scroll_y)
        .expect("the contents window is open");
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("the contents window is open");
}

#[test]
fn only_visible_blocks_are_shaped_and_a_still_frame_allocates_alike_at_any_count() {
    let still = |blocks: f32| {
        let mut cx = real_cx();
        let (window, state) = open(&mut cx, held(400), blocks);
        for _ in 0..3 {
            frame(&mut cx, window, 0.);
        }
        assert_eq!(state.borrow().visible.len(), blocks as usize);
        assert_eq!(state.borrow().misses, blocks as u64);
        let (heap, made) = (allocations(), state.borrow().misses);
        for _ in 0..10 {
            frame(&mut cx, window, 0.);
        }
        assert_eq!(state.borrow().misses, made, "a still frame shaped a block");
        let spent = allocations() - heap;
        // One block of scroll shapes exactly the block that enters.
        frame(&mut cx, window, BLOCK_H);
        assert_eq!(state.borrow().misses, made + 1);
        spent
    };
    assert_eq!(
        still(4.),
        still(12.),
        "a still frame allocated per visible block"
    );
}

#[test]
fn the_cursor_block_reshapes_its_run_bold_and_unwashed() {
    let mut cx = real_cx();
    let (window, state) = open(&mut cx, held(10), 4.);
    frame(&mut cx, window, 0.);
    let state = state.borrow();
    let (cursor, rest) = (&state.visible[0].1, &state.visible[1].1);
    assert!(
        cursor.wash.is_none(),
        "the cursor's run is bold, not washed"
    );
    let (start, end) = rest.wash.expect("a resting block washes its run");
    assert!(end > start);
}
