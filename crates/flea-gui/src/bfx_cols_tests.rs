//! BFX-cols reads the exact text and SVG bounds submitted by the production painters.
use crate::{
    backend::Backend,
    list::{bench, columns::Column},
    state::{tests::shell_with, Shell},
    window::startup::View,
};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    point, px, size, AppContext, Bounds, GlyphRun, Modifiers, MouseButton, Pixels, TestAppContext,
    VisualTestContext, WindowHandle,
};
use std::cell::RefCell;

/// The fixture's rows; every view draws all of them.
const ROWS: usize = 4;
/// The pane height the shell fixtures open at.
const PANE_HEIGHT: f32 = 646.;
/// Any listing id; the pane only matches its rows to the listing it last took.
const LISTING: usize = 3;
const COPY_MARK: &str = "flea-mark:copy";
const CUT_MARK: &str = "flea-mark:scissors";
/// ClipMark::bounds lifts every mark one pixel above its line's centre.
const MARK_LIFT: f32 = 1.;
/// How far the Kind header's left divider is dragged left, which widens Kind.
const DRAG: f32 = 35.;

#[derive(Clone, Debug)]
pub(crate) struct Painted {
    pub(crate) run: usize,
    pub(crate) mark: String,
    pub(crate) bounds: Bounds<Pixels>,
}

thread_local! {
    static PAINTED: RefCell<Option<Vec<Painted>>> = const { RefCell::new(None) };
}

pub(crate) fn record_line(run: &GlyphRun, x: Pixels, baseline: Pixels) {
    PAINTED.with_borrow_mut(|painted| {
        if let Some(painted) = painted {
            let line = run.line();
            painted.push(Painted {
                run: run as *const GlyphRun as usize,
                mark: String::new(),
                bounds: Bounds::new(
                    point(x, baseline - line.ascent),
                    size(line.width, line.ascent + line.descent),
                ),
            });
        }
    });
}

pub(crate) fn record_mark(mark: &str, bounds: Bounds<Pixels>) {
    PAINTED.with_borrow_mut(|painted| {
        if let Some(painted) = painted {
            painted.push(Painted {
                run: 0,
                mark: mark.into(),
                bounds,
            });
        }
    });
}

pub(crate) fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> Vec<Painted> {
    PAINTED.with_borrow_mut(|painted| *painted = Some(Vec::new()));
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    PAINTED.with_borrow_mut(|painted| painted.take().unwrap())
}

fn open(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (backend, _requests) = Backend::test_channel();
    cx.open_window(size(px(1100.), px(700.)), |window, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        let long = "very_long_".repeat(20);
        let names: [&str; ROWS] = ["a", "a_much_longer_folder", "short.txt", long.as_str()];
        let rows = names
            .iter()
            .enumerate()
            .map(|(index, name)| RowFacts {
                name: (*name).into(),
                dir: index < 2,
                mode: if index < 2 { 0o040755 } else { 0o100644 },
                kind: if index < 2 { "Folder" } else { "Text document" }.into(),
                icon: if index < 2 { "folder" } else { "text-plain" }.into(),
                ..RowFacts::default()
            })
            .collect::<Vec<_>>();
        host.pane = crate::pane::Pane::new("/bfx-cols".into(), PANE_HEIGHT);
        host.pane
            .receive(Reply::Listed {
                path: "/bfx-cols".into(),
                total: rows.len(),
            })
            .unwrap();
        host.pane
            .receive(Reply::Rows {
                start: 0,
                listing: LISTING,
                rows,
            })
            .unwrap();
        host.changed(&shell.locale);
        host.nav
            .after_reply(&mut host.pane, "/bfx-cols", &mut host.selected);
        host.backend = None;
        shell.path_labels[0] = "/bfx-cols".into();
        window.focus(&shell.focus, cx);
        shell
    })
}

fn clipboard(cx: &mut TestAppContext, handle: WindowHandle<Shell>, op: &str) {
    handle.update(cx, |shell, window, cx| {
        let paths = shell.panes[0].as_ref().unwrap().pane.held.rows.iter()
            .map(|row| flea::jsondoc::Json::Str(format!("/bfx-cols/{}", row.name)))
            .collect();
        let paths = flea::jsondoc::render(&flea::jsondoc::Json::Arr(paths));
        let line = format!(r#"{{"t":"clip","op":"changed","clip":"{op}","paths":{paths},"token":"bfx","skipped":0}}"#);
        shell.receive(0, Backend::test_reply_line(&line), window, cx);
    }).unwrap();
}

fn aligned_marks(view: View) {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.set_view(view, window, cx);
            if view == View::Columns {
                crate::window::columns_tests::host::settle(shell, window, cx);
            }
        })
        .unwrap();
    frame(&mut cx, handle);
    // The second copy proves a cut's scissors leave when the clipboard turns back to copy.
    for op in ["copy", "cut", "copy"] {
        clipboard(&mut cx, handle, op);
        let painted = frame(&mut cx, handle);
        let (key, stale) = if op == "cut" {
            (CUT_MARK, COPY_MARK)
        } else {
            (COPY_MARK, CUT_MARK)
        };
        let marks: Vec<_> = painted
            .iter()
            .filter(|p| p.mark == key)
            .map(|p| p.bounds)
            .collect();
        assert_eq!(
            marks.len(),
            ROWS,
            "every copied or cut row must actually paint a mark"
        );
        assert!(
            painted.iter().all(|p| p.mark != stale),
            "{op}: no row may still paint {stale}"
        );
        handle
            .update(&mut cx, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                match view {
                    View::List => {
                        let list = host.list.borrow();
                        assert_eq!(list.drawn.metrics.row, 31.);
                        assert_eq!(list.drawn.layout.name.x, 46.);
                        // EY-349: the mark sits one gap after the drawn name, inside the name column's reserved slot.
                        let name = list.drawn.bounds.left() + px(list.drawn.layout.name.x);
                        let x = list.drawn.bounds.left()
                            + px(list.drawn.layout.name.right() - list.drawn.metrics.clip);
                        for mark in &marks {
                            assert!(
                                mark.left() > name + px(list.drawn.metrics.gap) && mark.left() <= x,
                                "List clipboard marks follow the name: {mark:?}"
                            );
                        }
                    }
                    View::Columns => {
                        let tracks = host.columns.as_ref().unwrap();
                        use crate::columns::geometry::Slot;
                        let slots = [
                            Slot::GreatGrandparent,
                            Slot::Grandparent,
                            Slot::Parent,
                            Slot::Active,
                            Slot::Child,
                        ];
                        let paint = slots
                            .into_iter()
                            .filter_map(|slot| tracks.painters.get(slot))
                            .map(|paint| paint.borrow())
                            .find(|paint| {
                                paint.drawn.as_ref().is_some_and(|drawn| {
                                    marks.iter().all(|mark| drawn.bounds.contains(&mark.origin))
                                })
                            })
                            .expect("one Columns track painted every clipboard mark");
                        let drawn = paint.drawn.as_ref().unwrap();
                        let width = f32::from(drawn.bounds.size.width);
                        let cell =
                            crate::columns::geometry::cells_in(width, drawn.lane, true, true);
                        let x =
                            drawn.bounds.left() + px(cell.name_right - crate::columns::paint::CLIP);
                        for mark in &marks {
                            assert_eq!(
                                mark.left(),
                                x,
                                "Columns clipboard slot must ignore name length and chevron state"
                            );
                        }
                    }
                    View::Grid => {
                        let state = host.grid.as_ref().unwrap().state.borrow();
                        let layout = state.layout.unwrap();
                        let mut relative = Vec::new();
                        assert_eq!(state.drawn().len(), marks.len(), "one mark per drawn tile");
                        for (drawn, mark) in state.drawn().iter().zip(&marks) {
                            relative.push(mark.left() - px(drawn.cell.x));
                            assert_eq!(
                                mark.left(),
                                state.bounds.left()
                                    + px(drawn.caption.x
                                        + drawn.caption.width
                                        + layout.metrics.gap)
                            );
                            let y = state.bounds.top()
                                + px(drawn.caption.y
                                    + drawn.lines.saturating_sub(1) as f32
                                        * layout.metrics.caption_line
                                    + (layout.metrics.caption_line - layout.metrics.clip_mark)
                                        / 2.
                                    - MARK_LIFT);
                            assert_eq!(
                                mark.top(),
                                y,
                                "Grid mark keeps its shipped caption baseline"
                            );
                        }
                        assert!(relative.windows(2).all(|pair| pair[0] == pair[1]));
                    }
                    _ => unreachable!(),
                }
            })
            .unwrap();
        println!("BFX-cols {view:?} {op}: {marks:?}");
    }
}

#[test]
fn bfx_cols_clipboard_list_painted_slot() {
    aligned_marks(View::List);
}

#[test]
fn bfx_cols_clipboard_columns_painted_slot() {
    aligned_marks(View::Columns);
}

#[test]
fn bfx_cols_clipboard_grid_painted_slot() {
    aligned_marks(View::Grid);
}

fn kind_right(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> (Pixels, Vec<Pixels>) {
    let painted = frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let header = list.header.drawn.bounds;
            let cell = list.header.drawn.layout.cell(Column::Kind).unwrap();
            let title = painted
                .iter()
                .find(|p| {
                    p.run != 0
                        && p.bounds.top() < header.bottom()
                        && p.bounds.left() >= header.left() + px(cell.x)
                })
                .expect("Kind title must be painted");
            let expected = header.left() + px(cell.right());
            assert_eq!(
                title.bounds.right(),
                expected,
                "Kind title must be right pinned"
            );
            let mut rows = Vec::new();
            for shown in list.visible() {
                let run = &shown.prepared.as_ref().unwrap().kind;
                let p = painted
                    .iter()
                    .find(|p| p.run == run as *const GlyphRun as usize)
                    .expect("Kind cell must be painted");
                let right = list.drawn.bounds.left()
                    + px(list.drawn.layout.cell(Column::Kind).unwrap().right());
                assert_eq!(p.bounds.right(), right, "Kind cell must be right pinned");
                rows.push(right);
            }
            assert_eq!(rows.len(), ROWS, "every row must paint its Kind cell");
            println!("BFX-cols Kind: title={:?}, rows={rows:?}", title.bounds);
            (expected, rows)
        })
        .unwrap()
}

#[test]
fn bfx_cols_kind_left_divider_pins_painted_header_and_cells() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    handle
        .update(&mut cx, |shell, _, _| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"columns":["mode","size","date","kind"]}"#).unwrap();
        })
        .unwrap();
    frame(&mut cx, handle);
    let before = kind_right(&mut cx, handle);
    let (start, width) = handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let header = list.header.drawn.bounds;
            let cell = list.header.drawn.layout.cell(Column::Kind).unwrap();
            (
                point(header.left() + px(cell.x), header.center().y),
                cell.width,
            )
        })
        .unwrap();
    let sent = |cx: &mut VisualTestContext| {
        handle
            .update(cx, |shell, _, _| {
                shell.panes[0].as_ref().unwrap().nav.writer.borrow().sent
            })
            .unwrap()
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let sent_before = sent(&mut vcx);
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    let finish = start + point(px(-DRAG), px(0.));
    vcx.simulate_mouse_move(finish, Some(MouseButton::Left), Modifiers::default());
    assert_eq!(kind_right(&mut vcx, handle), before);
    assert_eq!(
        sent(&mut vcx),
        sent_before,
        "a drag commits nothing before release"
    );
    handle
        .update(&mut vcx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            assert_eq!(
                list.header.drawn.layout.cell(Column::Kind).unwrap().width,
                width + DRAG
            );
            assert_eq!(list.drawn.layout.cell(Column::Kind).unwrap().width, width);
        })
        .unwrap();
    vcx.simulate_mouse_up(finish, MouseButton::Left, Modifiers::default());
    assert_eq!(kind_right(&mut vcx, handle), before);
    handle
        .update(&mut vcx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.list
                    .borrow()
                    .drawn
                    .layout
                    .cell(Column::Kind)
                    .unwrap()
                    .width,
                width + DRAG
            );
            assert_eq!(
                host.nav.writer.borrow().sent,
                sent_before + 1,
                "drag must commit one width"
            );
        })
        .unwrap();
}

#[path = "bfx_cols_fixlist_tests.rs"]
mod fixlist;
