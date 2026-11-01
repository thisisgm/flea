//! Pointer gestures exercise the existing Markdown renderer in both preview slots.
use super::super::{doc, worker, Faces, MarkdownView, Palette, Status, Tokens, View};
use flea::ui_logic_pv2::markdown::{markdown_layout as layout, Block};
use gpui::{
    div, point, prelude::*, px, size, AppContext, Context, Entity, FocusHandle, Modifiers,
    MouseButton, Render, TestAppContext, TestDispatcher, VisualTestContext, Window,
};
use std::sync::Arc;

struct Host {
    view: Entity<MarkdownView>,
    focus: FocusHandle,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(|event, window, cx| {
                crate::g2::pv2::selection::key(event, window, cx);
            })
            .child(self.view.clone())
    }
}

fn tokens(column: bool) -> Tokens {
    Tokens {
        body: 14.,
        theme_body: 14.,
        caption: 12.,
        gap: 9.,
        hairline: 1.,
        row_pad_x: 14.,
        row_pad_y: 7.,
        mark: 16.,
        state_mark: 40.,
        source_line: None,
        column,
    }
}
fn palette() -> Palette {
    let t = crate::palette::Theme::default();
    Palette {
        background: t.background,
        code: t.surface,
        surface: t.surface,
        foreground: t.foreground,
        bright: t.foreground,
        muted: t.foreground,
        figure_muted: t.foreground,
        muted_frame: t.muted_frame,
        accent_frame: t.accent_frame,
        inks: t.code_inks,
    }
}

/// A laid view of `raw` and its blocks in a 600 by 400 window, painted once so its runs are hit targets.
fn mount(
    column: bool,
    source: bool,
    raw: &str,
    blocks: Vec<Block>,
) -> (
    TestAppContext,
    gpui::WindowHandle<Host>,
    Entity<MarkdownView>,
) {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    let handle = cx.open_window(size(px(600.), px(400.)), |window, cx| {
        let view = cx.new(|cx| {
            let tokens = tokens(column);
            let faces = Faces::omarchy_default();
            let mut view = MarkdownView::new(
                palette(),
                tokens,
                faces.clone(),
                if column { "column" } else { "look" },
                &crate::test_catalogs::english(),
                cx,
            );
            let raw: Arc<str> = raw.into();
            view.doc.text = Some(raw.clone());
            view.doc.starts = Arc::new(worker::line_starts(&raw));
            view.doc.lines = view.doc.starts.len();
            view.doc.status = Status::Ready;
            view.doc.shown = true;
            view.doc.take(worker::Parsed::of(blocks), true);
            let mut measure = worker::TextMeasure::new(cx.text_system().clone(), faces);
            let blocks = view.doc.parsed.blocks.clone();
            for (i, block) in blocks.iter().enumerate() {
                let geometry = layout::layout(
                    block,
                    560.,
                    &mut layout::Ctx {
                        metrics: tokens.metrics(),
                        measure: &mut measure,
                        assets: &*view.doc.assets,
                        counter: view.doc.counter(i),
                        remote: "",
                    },
                );
                view.doc.laid.push(Some(doc::Laid {
                    geometry: Arc::new(geometry),
                    generation: 0,
                    version: 0,
                }));
            }
            let n = view.doc.blocks().len();
            view.doc.laid.drain(..n);
            view.doc.retop(&tokens.metrics());
            view.source.set_text(raw, view.doc.starts.clone());
            view.source.answer = super::super::source::Answer::Plain;
            view.source.total = view.doc.lines;
            if source {
                view.view = View::Source;
            } else {
                view.source.answer = super::super::source::Answer::Binary;
            }
            view
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Host { view, focus }
    });
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let view = handle
        .update(&mut cx, |host, _, _| host.view.clone())
        .unwrap();
    (cx, handle, view)
}

#[test]
fn gm_prevsel_markdown_rendered_and_source_drag_copy_in_columns_and_quick_look() {
    for column in [true, false] {
        for source in [false, true] {
            let (cx, handle, view) = mount(
                column,
                source,
                "# Héllo **world**\r\n\r\nSecond\tparagraph\r\n",
                vec![
                    Block::Heading {
                        level: 1,
                        text: "Héllo <strong>world</strong>".into(),
                        align: None,
                    },
                    Block::run("Second paragraph"),
                ],
            );
            let (start, end) = view.read_with(&cx, |view, _| {
                let first = view.selection.spans.first().expect("first selectable run");
                let last = view.selection.spans.last().expect("last selectable run");
                (
                    point(first.bounds.left(), first.bounds.center().y),
                    point(last.bounds.right(), last.bounds.center().y),
                )
            });
            let mut visual = VisualTestContext::from_window(handle.into(), &cx);
            visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
            visual.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
            visual.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
            visual.simulate_keystrokes("ctrl-c");
            let copied = visual.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
            assert_eq!(
                copied.as_deref(),
                Some(if source {
                    "# Héllo **world**\r\n\r\nSecond\tparagraph"
                } else {
                    "Héllo world\nSecond paragraph"
                }),
                "column={column} source={source}"
            );
            visual.simulate_keystrokes("escape");
            view.read_with(&visual, |view, _| {
                assert!(view.selection.range().is_none());
                assert_eq!(view.scroll, 0.);
                assert_eq!(view.source.top, 0);
            });
        }
    }
}

#[test]
fn gm_prevsel_rendered_copy_preserves_hard_breaks_and_soft_wrap_text() {
    use flea::ui_logic_pv2::markdown::{
        markdown_flow::{self as flow, Face, Ink, Measure},
        markdown_rich::{Piece, Style},
    };
    struct Mono;
    impl Measure for Mono {
        fn width(&mut self, text: &str, _: Face, _: f32) -> f32 {
            text.chars().count() as f32
        }
        fn extent(&mut self, _: Face, _: f32) -> (f32, f32) {
            (1., 0.)
        }
    }
    let base = tokens(false)
        .metrics()
        .base(14., Face::Regular, Ink::Foreground, None);
    let laid = flow::flow(
        &[Piece::Text("a long along".into(), Style::default())],
        &base,
        4.,
        &mut Mono,
        &layout::NoSizes,
    );
    assert_eq!(super::text(&laid), "a long along");
    let laid = flow::flow(
        &[
            Piece::Text("first".into(), Style::default()),
            Piece::Break,
            Piece::Text("second".into(), Style::default()),
        ],
        &base,
        40.,
        &mut Mono,
        &layout::NoSizes,
    );
    assert_eq!(super::text(&laid), "first\nsecond");
}

#[test]
fn gm_prevsel_a_press_builds_only_the_text_it_can_reach() {
    let raw: String = (0..3000).map(|n| format!("line {n}\n")).collect();
    for source in [false, true] {
        let blocks = (0..300)
            .map(|n| Block::run(format!("paragraph {n}")))
            .collect();
        let (cx, handle, view) = mount(true, source, &raw, blocks);
        let (start, end) = view.read_with(&cx, |view, _| {
            let spans = &view.selection.spans;
            assert!(spans.len() > 2 && spans.len() < 100, "{}", spans.len());
            (
                point(spans[0].bounds.left(), spans[0].bounds.center().y),
                point(spans[1].bounds.right(), spans[1].bounds.center().y),
            )
        });
        let mut visual = VisualTestContext::from_window(handle.into(), &cx);
        visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
        visual.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        view.read_with(&visual, |view, _| {
            assert!(view.selection.range().is_some(), "source={source}");
            assert!(
                view.selection.chunks.len() < 100,
                "source={source}: {} chunks cut for a two-run range",
                view.selection.chunks.len()
            );
        });
    }
}

#[test]
fn gm_prevsel_a_long_drag_cuts_only_the_shown_text_per_move() {
    let raw: String = (0..300).map(|n| format!("paragraph {n}\n\n")).collect();
    let blocks = (0..300)
        .map(|n| Block::run(format!("paragraph {n}")))
        .collect();
    let (cx, handle, view) = mount(true, false, &raw, blocks);
    let start = view.read_with(&cx, |view, _| {
        let first = &view.selection.spans[0];
        point(first.bounds.left(), first.bounds.center().y)
    });
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    view.update(&mut visual, |view, cx| view.scroll_rows(100_000, cx));
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let (far, near) = view.read_with(&visual, |view, _| {
        let last = view.selection.spans.last().expect("a run at the bottom");
        let y = last.bounds.center().y;
        (
            point(last.bounds.right(), y),
            point(last.bounds.left() + px(2.), y),
        )
    });
    visual.simulate_mouse_move(far, Some(MouseButton::Left), Modifiers::none());
    view.update(&mut visual, |view, _| {
        assert!(
            view.doc.laid.iter().all(Option::is_some),
            "a range-wide cut would reach every block"
        );
        view.selection.chunks.clear()
    });
    visual.simulate_mouse_move(near, Some(MouseButton::Left), Modifiers::none());
    view.read_with(&visual, |view, _| {
        assert!(view.selection.range().is_some());
        assert!(
            view.selection.chunks.len() < 100,
            "{} chunks cut for one move of a 300-block range",
            view.selection.chunks.len()
        );
    });
    visual.simulate_mouse_up(near, MouseButton::Left, Modifiers::none());
    visual.simulate_keystrokes("ctrl-c");
    let copied = visual
        .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap_or_default();
    assert!(
        copied.starts_with("paragraph 0\nparagraph 1\n") && copied.contains("\nparagraph 150\n"),
        "the release still copies the whole range: {copied:.60}"
    );
}

#[test]
fn ey260_preview_table_reads_the_sideways_table_view_row_and_shift() {
    let cell = "abcdefghijklmnopqrstuvwxyz";
    let line = |c: &str| format!("|{}|\n", [c; 12].join("|"));
    let raw = format!("{}{}{}{}", line("h"), line("---"), line(cell), line(cell));
    let words = flea::ui_logic_pv2::markdown::Words::english();
    let blocks = flea::ui_logic_pv2::markdown::blocks(
        &raw,
        &flea::ui_logic_pv2::markdown::Ctx {
            dir: "/md-tables",
            chrome: "#181825",
            ink: "#c0caf5",
            words: &words,
        },
    );
    assert!(matches!(blocks.as_slice(), [Block::Table(_)]), "{blocks:?}");
    let (mut cx, handle, view) = mount(false, false, &raw, blocks);
    let read = |cx: &TestAppContext| {
        let state = view.read_with(cx, |view, _| view.table_state());
        let json = flea::jsondoc::parse(&state).unwrap();
        let rect = |key: &str| -> Vec<f32> {
            json.get(key)
                .and_then(flea::jsondoc::Json::as_str)
                .unwrap_or_else(|| panic!("{key} in {state}"))
                .split(' ')
                .map(|n| n.parse().unwrap())
                .collect()
        };
        let shift = json.get("scrollX").and_then(flea::jsondoc::Json::as_f64);
        (shift, rect("view"), rect("row"))
    };
    let (shift, table, row) = read(&cx);
    assert_eq!(shift, Some(0.));
    // The row starts under the header, runs the whole content width and is one line high.
    assert_eq!(row[0], table[0]);
    assert!(
        row[1] > table[1] && row[1] + row[3] < table[1] + table[3],
        "{table:?} {row:?}"
    );
    assert!(row[2] > table[2] && row[3] > 0., "{table:?} {row:?}");
    view.update(&mut cx, |view, _| {
        view.tables
            .insert(super::super::paint::TableId::Block(0, 0), 40.);
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let (shift, moved, after) = read(&cx);
    assert_eq!(shift, Some(40.));
    assert_eq!((moved, after[0], after[1]), (table, row[0] - 40., row[1]));
    view.update(&mut cx, |view, cx| view.set_view(View::Source, cx));
    assert_eq!(view.read_with(&cx, |view, _| view.table_state()), "{}");
}
#[path = "selection_tests/rendered.rs"]
mod rendered;
