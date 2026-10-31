// Contents blocks: fixed 55 px blocks from the board, only the visible ones shaped, and no allocation on a still frame.
use super::contents::Palette;
use super::contents::{visible, Block, ContentsElement, ContentsState, Held, BLOCK_H};
use crate::list::bench::{allocations, bench_typeface, english, fixture, real_cx};
use crate::list::Palette as ListPalette;
use crate::palette::Theme;
use crate::row::format::{size_text, Words};
use gpui::{
    div, prelude::*, px, size, AppContext, Context, GlyphRun, Hsla, IntoElement, Render, ShapedRun,
    TestAppContext, TextRun, Window, WindowHandle,
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

#[test]
fn codeicons_language_contents_uses_the_shared_file_mark() {
    let mut cx = real_cx();
    let blocks = Rc::new(Held {
        listing: 9,
        total: 1,
        start: 0,
        blocks: vec![Block {
            row: flea::protocol::RowFacts {
                name: "main.rs".into(),
                icon: "text-plain".into(),
                mode: 0o100644,
                ..Default::default()
            },
            location: "src".into(),
            line_no: 1,
            line: "fn main() {}".into(),
        }],
    });
    let (window, state) = open(&mut cx, blocks, 1.);
    frame(&mut cx, window, 0.);
    assert_eq!(
        state.borrow().visible[0].1.mark,
        crate::row::marks::Mark::Language(
            crate::row::marks::Language::for_file("main.rs").unwrap()
        )
    );
    let misses = state.borrow().misses;
    state
        .borrow_mut()
        .apply_settings(&flea::uistate::from_file(r#"{"langMarks":false}"#));
    frame(&mut cx, window, 0.);
    assert_eq!(
        state.borrow().visible[0].1.mark,
        crate::row::marks::Mark::FileText
    );
    assert_eq!(
        state.borrow().misses,
        misses + 1,
        "setting invalidates the cached mark"
    );
    frame(&mut cx, window, 0.);
    assert_eq!(
        state.borrow().misses,
        misses + 1,
        "still frame reuses preparation"
    );
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
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::from_list(&distinct_palette());
        div().size_full().child(ContentsElement::new(
            Rc::clone(&self.held),
            self.scroll_y,
            0,
            palette,
            Rc::clone(&self.state),
        ))
    }
}

/// Each role blends the theme foreground at its own opacity, so a role drawn in another's place changes the ink.
fn distinct_palette() -> ListPalette {
    let theme = Theme::default();
    let foreground = theme.foreground;
    ListPalette {
        background: theme.background,
        foreground,
        muted: foreground.opacity(0.65),
        error: foreground.opacity(0.95),
        error_on_surface: foreground.opacity(0.96),
        metadata: foreground.opacity(0.62),
        metadata_frame: foreground.opacity(0.5),
        accent: foreground.opacity(0.9),
        accent_frame: foreground.opacity(0.85),
        symlink: foreground.opacity(0.8),
        executable: foreground.opacity(0.75),
        marked: foreground.opacity(0.35),
        muted_frame: foreground.opacity(0.45),
        tags: theme.tag_menu_inks,
    }
}

#[test]
fn the_contents_fixture_gives_each_role_its_own_ink() {
    let palette = distinct_palette();
    let roles = [
        palette.background,
        palette.foreground,
        palette.metadata,
        palette.metadata_frame,
        palette.accent,
        palette.accent_frame,
        palette.symlink,
        palette.executable,
        palette.marked,
        palette.muted_frame,
    ];
    for (at, role) in roles.iter().enumerate() {
        assert!(
            roles[at + 1..].iter().all(|other| other != role),
            "role {at} shares its ink with a later role"
        );
    }
}

// The contents module's EDGE_W, private there: the lifted block's edge is 2 wide.
const EDGE_W: f32 = 2.;

#[test]
fn the_lifted_block_edge_paints_in_the_accent_frame() {
    let mut cx = real_cx();
    let (window, _) = open(&mut cx, held(4), 4.);
    frame(&mut cx, window, 0.);
    let palette = Palette::from_list(&distinct_palette());
    let (bounds, colour) = cx
        .update_window(window.into(), |_, window, _| {
            let scale = window.scale_factor();
            let mut edges: Vec<_> = window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    quad.bounds.size.width.as_f32() == EDGE_W * scale
                        && quad.bounds.size.height.as_f32() == BLOCK_H * scale
                })
                .map(|quad| (quad.bounds, quad.background.as_solid()))
                .collect();
            assert_eq!(edges.len(), 1, "the lifted block paints one edge quad");
            edges.pop().unwrap()
        })
        .expect("the contents window is open");
    assert_eq!(
        (bounds.origin.x.as_f32(), bounds.origin.y.as_f32()),
        (0., 0.),
        "the edge quad sits on the lifted block's left"
    );
    assert_eq!(
        colour,
        Some(palette.accent_frame),
        "the lifted block's edge paints in the accent frame"
    );
    assert_ne!(colour, Some(palette.accent), "the edge is not the accent");
    assert_ne!(
        colour,
        Some(palette.foreground),
        "the edge is not the foreground"
    );
}

fn open(
    cx: &mut TestAppContext,
    held: Rc<Held>,
    blocks: f32,
) -> (WindowHandle<View>, Rc<RefCell<ContentsState>>) {
    let mut state = ContentsState::new(bench_typeface(), Words::from_catalog(&english()));
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

// The text a shaped run prints, judged by its glyphs as the track tests judge theirs.
fn prints(cx: &mut TestAppContext, window: WindowHandle<View>, run: &GlyphRun, text: &str) -> bool {
    cx.update_window(window.into(), |_, window, _| {
        let style = TextRun {
            len: text.len(),
            font: bench_typeface().font,
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let want = window
            .text_system()
            .layout_line(text, bench_typeface().caption, &[style], None);
        let glyphs = |runs: &[ShapedRun]| {
            runs.iter()
                .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id))
                .collect::<Vec<_>>()
        };
        run.line().len == want.len && glyphs(&run.line().runs) == glyphs(&want.runs)
    })
    .expect("the contents window is open")
}

#[test]
fn the_size_cell_prints_the_list_text_under_localized_words() {
    let mut cx = real_cx();
    let words = crate::list::size_tests::german_words();
    let file = fixture("uniform").into_iter().next().expect("a row");
    let mut link = file.clone();
    link.mode = 0o120777;
    for row in [file, link] {
        let want = size_text(&row, None, &words);
        let block = Block {
            row,
            location: "src".into(),
            line_no: 1,
            line: "let report = load();".into(),
        };
        let held = Rc::new(Held {
            listing: 1,
            total: 1,
            start: 0,
            blocks: vec![block],
        });
        let mut state = ContentsState::new(bench_typeface(), words.clone());
        state.query = "report".into();
        let state = Rc::new(RefCell::new(state));
        let window = cx.open_window(size(px(900.), px(BLOCK_H)), |_, _| View {
            held,
            scroll_y: 0.,
            state: Rc::clone(&state),
        });
        frame(&mut cx, window, 0.);
        let shown = Rc::clone(&state.borrow().visible[0].1);
        assert!(
            prints(&mut cx, window, &shown.size, &want),
            "the sr1 size cell differs from the list's {want:?}"
        );
    }
}
