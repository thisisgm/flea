//! The live shell's list join, observed through its painted frame and authenticated IPC dispatch.
use crate::{
    backend::Backend,
    ipc::{rows::colour, server::Event},
    list::{bench, columns::Column, pane::Held},
    state::{tests::shell_with, Shell},
};
use flea::ipc::wire::{Reply, Request};
use gpui::{px, size, AppContext, TestAppContext, WindowHandle};
use std::rc::Rc;

#[path = "join_held_tests.rs"]
mod held;

#[path = "join_loading_tests.rs"]
mod loading;

#[path = "join_pointer_tests.rs"]
mod pointer;

#[path = "join_probe_tests.rs"]
mod probe;

#[path = "join_drag_tests.rs"]
mod drag;

#[path = "join_steering_tests.rs"]
mod steering;

#[path = "join_parity_tests.rs"]
mod parity;

fn open(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (backend, _receive) = Backend::test_channel();
    cx.open_window(size(px(1100.), px(700.)), |_, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = None;
        let mut rows = bench::fixture("uniform");
        rows[1].mode = 0o120777;
        rows[1].link = Some("../elsewhere".into());
        host.pane.held = Rc::new(Held {
            listing: 2,
            total: rows.len(),
            start: 0,
            rows,
        });
        host.list.borrow_mut().typeface = bench::bench_typeface();
        shell
    })
}

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn call(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    function: &str,
    args: &[&str],
) -> Reply {
    handle
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: args.iter().map(|v| (*v).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply
        })
        .unwrap()
}

fn ask(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    function: &str,
    args: &[&str],
) -> String {
    let reply = call(cx, handle, function, args);
    assert_eq!(reply.code, 0, "{function}: {}", reply.stderr);
    reply.stdout.trim_end_matches('\n').into()
}

// EY-328: the right click a header case aims at the Name title lands on that title, as Header.qml's title item does.
#[test]
fn ey328_header_cell_centre_sits_on_the_painted_title() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let left: f32 = ask(&mut cx, handle, "headerLeft", &[]).parse().unwrap();
    for (name, from_right) in [("name", false), ("size", true)] {
        let rect = ask(&mut cx, handle, "headerCellRect", &[name]);
        let (x, width) = rect.split_once('|').unwrap();
        let (x, width): (f32, f32) = (x.parse().unwrap(), width.parse().unwrap());
        let centre = ask(&mut cx, handle, "headerCellCentre", &[name]);
        let at = centre.split(' ').next().unwrap().parse::<f32>().unwrap() - left;
        // The title's middle sits within 40 px of the edge it is set on, and never at the cell's middle.
        let edge = if from_right { x + width - at } else { at - x };
        assert!(
            (0. ..=40.).contains(&edge) && edge + 1. < width / 2.,
            "{name}: centre {at} in cell {x}|{width}"
        );
    }
}

#[test]
fn u04_header_and_rows_share_the_live_painted_layout_and_dispatch() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            assert!(
                list.header.drawn.frames > 0,
                "the live window never mounted HeaderElement"
            );
            assert_eq!(list.header.drawn.bounds.bottom(), list.drawn.bounds.top());
            assert_eq!(list.header.drawn.layout, list.drawn.layout);
        })
        .unwrap();
    assert_eq!(
        ask(&mut cx, handle, "headerTitles", &[]),
        "Name|Mode|Size|Modified|Kind"
    );
    for name in ["headerCellCentre", "headerCellRect"] {
        assert!(!ask(&mut cx, handle, name, &["size"]).is_empty());
    }
    for name in [
        "headerDragTrace",
        "headerLeft",
        "headerTop",
        "listAreaRect",
        "listContentY",
        "listingBackgroundCentre",
    ] {
        assert!(!ask(&mut cx, handle, name, &[]).is_empty());
    }
    for name in [
        "rowCellColor",
        "rowCellOverflow",
        "rowCentre",
        "rowGlyph",
        "rowHovered",
        "rowLeft",
        "rowNameCentre",
        "rowNameColor",
        "rowNamePx",
        "rowNameRect",
        "rowNameText",
        "rowRect",
        "rowSizeText",
    ] {
        assert!(!ask(&mut cx, handle, name, &["1"]).is_empty(), "{name}");
    }
    for name in [
        "rowNameText",
        "rowSizeText",
        "rowGlyph",
        "rowRect",
        "rowNameColor",
    ] {
        assert_eq!(
            ask(&mut cx, handle, name, &["999"]),
            "",
            "an offscreen row answered {name}"
        );
        assert_eq!(ask(&mut cx, handle, name, &["-1"]), "");
    }
    assert_eq!(call(&mut cx, handle, "rowThumb", &["1"]), Reply::value(""));
    assert_eq!(
        ask(&mut cx, handle, "visibleRowMode", &["1"]),
        0o120777.to_string(),
        "the drawn row reports its raw mode"
    );
    for index in ["999", "-1"] {
        assert_eq!(ask(&mut cx, handle, "visibleRowMode", &[index]), "-1");
    }
}

#[test]
fn u04_readers_keep_the_painted_name_and_ink_until_the_next_frame() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let old = ask(&mut cx, handle, "rowNameText", &["1"]);
    let ink = ask(&mut cx, handle, "rowNameColor", &["1"]);
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let held = &host.pane.held;
            let mut rows = held.rows.clone();
            rows[1].name = "a-very-long-name-".repeat(40) + ".txt";
            host.pane.held = Rc::new(Held {
                listing: held.listing,
                total: held.total,
                start: held.start,
                rows,
            });
            host.pane.cursor = 1;
        })
        .unwrap();
    assert_eq!(ask(&mut cx, handle, "rowNameText", &["1"]), old);
    assert_eq!(ask(&mut cx, handle, "rowNameColor", &["1"]), ink);
    frame(&mut cx, handle);
    let painted = handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let shown = list
                .visible()
                .iter()
                .find(|shown| shown.index == 1)
                .unwrap();
            (
                shown.prepared.as_ref().unwrap().label.clone(),
                colour(list.drawn.palette.unwrap().foreground),
            )
        })
        .unwrap();
    assert!(
        painted.0.contains('\u{2026}'),
        "the long name was not elided before shaping"
    );
    assert_eq!(
        ask(&mut cx, handle, "rowNameText", &["1"]),
        "a-very-long-name-".repeat(40) + ".txt -> ../elsewhere"
    );
    assert_eq!(ask(&mut cx, handle, "rowNameColor", &["1"]), painted.1);
}

#[test]
fn u04_width_shrink_preserves_stored_widths_and_partial_rows_stay_below_header() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    handle.update(&mut cx, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.scroll_y = 15.;
        shell.launch.settings.data = flea::jsondoc::parse(r#"{"columns":["mode","size","date","kind"],"columnWidths":{"mode":480,"size":480,"date":480,"kind":480}}"#).unwrap();
    }).unwrap();
    for width in [2600., 1100., 600., 2600.] {
        cx.simulate_window_resize(handle.into(), size(px(width), px(700.)));
        frame(&mut cx, handle);
        handle
            .update(&mut cx, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                let list = host.list.borrow();
                assert!(list.header.drawn.frames > 0);
                assert_eq!(list.drawn.mask, list.drawn.bounds);
                assert_eq!(list.header.drawn.bounds.bottom(), list.drawn.bounds.top());
                assert_eq!(list.header.drawn.layout, list.drawn.layout);
                assert_eq!(
                    host.nav.writer.borrow().sent,
                    0,
                    "a viewport resize wrote preferences"
                );
                for column in Column::ALL {
                    assert_eq!(list.view.stored.get(column), Some(480));
                    if width == 2600. {
                        assert_eq!(list.drawn.layout.cell(column).unwrap().width, 480.);
                    } else if width == 1100. {
                        assert!(
                            list.drawn
                                .layout
                                .cell(column)
                                .is_some_and(|cell| cell.width < 480.),
                            "a stored oversized column dropped instead of shrinking: {column:?}"
                        );
                    }
                }
                if width == 600. {
                    assert!(list.drawn.layout.shown(Column::Mode));
                    assert!(!list.drawn.layout.shown(Column::Kind));
                }
                assert!(
                    list.visible()[0].index as f32 * list.drawn.metrics.row - list.drawn.scroll_y
                        < 0.
                );
            })
            .unwrap();
        assert_eq!(ask(&mut cx, handle, "rowCellOverflow", &["0"]), "0|0|0|0");
    }
    let rect = ask(&mut cx, handle, "listAreaRect", &[]);
    assert!(
        rect.starts_with("192 54 "),
        "wrong header/list placement: {rect}"
    );
    let row = ask(&mut cx, handle, "rowCentre", &["1"]);
    assert_ne!(row, format!("{} {}", 0, 0));
}

#[test]
fn u04_an_empty_or_failed_body_has_current_geometry_and_no_old_rows() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    for kind in [
        crate::pane::states::Kind::Empty,
        crate::pane::states::Kind::Ready,
        crate::pane::states::Kind::Error,
    ] {
        handle
            .update(&mut cx, |shell, _, _| {
                let host = shell.panes[0].as_mut().unwrap();
                host.pane.state.kind = kind;
                host.pane.scroll_y = if kind == crate::pane::states::Kind::Ready {
                    31. * 20.
                } else {
                    0.
                };
            })
            .unwrap();
        for width in [1100., 800.] {
            cx.simulate_window_resize(handle.into(), size(px(width), px(700.)));
            frame(&mut cx, handle);
            let rect = ask(&mut cx, handle, "listAreaRect", &[]);
            assert_eq!(rect, format!("192 54 {} 619", width as i32 - 192));
            let centre = ask(&mut cx, handle, "listingBackgroundCentre", &[]);
            assert_eq!(
                centre,
                format!("{} 364", ((192. + width) / 2.).round() as i32)
            );
            if kind != crate::pane::states::Kind::Ready {
                assert_eq!(ask(&mut cx, handle, "listContentY", &[]), "0");
                assert_eq!(ask(&mut cx, handle, "rowNameText", &["1"]), "");
                assert_eq!(ask(&mut cx, handle, "rowCentre", &["1"]), "");
            }
        }
    }
}

#[test]
fn u04_a_larger_text_stop_keeps_header_and_rows_adjacent() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    handle
        .update(&mut cx, |shell, _, _| {
            let mut list = shell.panes[0].as_ref().unwrap().list.borrow_mut();
            list.typeface = crate::list::Typeface {
                base: px(20.),
                body: px(20.),
                body_small: px(18.),
                caption: px(17.),
                ..bench::bench_typeface()
            };
            list.restyle();
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowNamePx", &["1"]), "20");
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            assert_eq!(list.header.drawn.bounds.size.height, px(37.));
            assert_eq!(list.drawn.bounds.top(), list.header.drawn.bounds.bottom());
            assert_eq!(list.drawn.bounds.size.height, px(609.));
            assert_eq!(list.drawn.metrics.row, 42.);
        })
        .unwrap();
}
