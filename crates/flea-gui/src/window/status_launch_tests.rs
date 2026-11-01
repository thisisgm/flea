//! Launch selection reaches the real constructor, listing replies and painted footer.
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::{constructor_fixture::Services, Shell},
    window::{
        locale::Snapshot,
        navigate::Step,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::protocol::{Reply, RowFacts};
use gpui::{AppContext, TestAppContext, WindowHandle};
use std::{sync::Arc, time::Duration};

const TARGET: &str = "/files/notes.txt";
const TARGET_INDEX: usize = 4;
const SELECTED: &str = "1 of 9 selected · 99 B";

fn open(
    cx: &mut TestAppContext,
    view: &str,
    focus: usize,
    select: Option<&str>,
) -> WindowHandle<Shell> {
    cx.update(|cx| {
        cx.set_global(Services {
            backend: |listing, cx| {
                if let Some(target) = &listing.select {
                    assert_eq!(target, TARGET);
                    assert_eq!(listing.path, "/files");
                }
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        });
    });
    let args = select.map_or_else(
        || vec!["/files".to_owned()],
        |target| vec!["--select".to_owned(), target.to_owned()],
    );
    let mut args = Arguments::parse(args.into_iter()).unwrap();
    args.absolutize(std::path::Path::new("/"));
    let settings = Settings::from_json(
        flea::jsondoc::parse(&format!(
            r#"{{"view":"{view}","dual":{{"focus":{focus},"paths":["/left","/right"]}}}}"#
        ))
        .unwrap(),
    );
    let launch = Launch::resolve(settings, args, "/home/test");
    assert_eq!(launch.focus, focus);
    assert_eq!(launch.select[focus].as_deref(), select);
    cx.add_window(|window, cx| {
        let (_sender, events) = async_channel::bounded(1);
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        Shell::new(launch, locale, events, window, cx)
    })
}

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn deliver(cx: &mut TestAppContext, handle: WindowHandle<Shell>, side: usize, reply: Reply) {
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(side, BackendEvent::Reply(reply), window, cx);
        })
        .unwrap();
}

fn listed(cx: &mut TestAppContext, handle: WindowHandle<Shell>, side: usize) {
    deliver(
        cx,
        handle,
        side,
        Reply::Listed {
            path: "/files".into(),
            total: 9,
        },
    );
}

fn rows(cx: &mut TestAppContext, handle: WindowHandle<Shell>, side: usize, listing: usize) {
    let rows = [
        "build",
        "docs",
        "image.png",
        "main.rs",
        "notes.txt",
        "plan.md",
        "readme",
        "src",
        "test.rs",
    ]
    .into_iter()
    .enumerate()
    .map(|(at, name)| RowFacts {
        name: name.into(),
        size: if at == TARGET_INDEX { 99 } else { 1234 },
        icon: "text-plain".into(),
        ..Default::default()
    })
    .collect();
    deliver(
        cx,
        handle,
        side,
        Reply::Rows {
            start: 0,
            listing,
            rows,
        },
    );
    frame(cx, handle);
}

fn counts(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> String {
    handle
        .update(cx, |shell, _, cx| {
            let footer = shell.surfaces[0]
                .footer
                .as_ref()
                .expect("the window built its shared footer")
                .read(cx);
            footer
                .painted
                .get("counts")
                .expect("the shared footer painted its focused-pane counts")
                .text
                .to_string()
        })
        .unwrap()
}

fn assert_selected(cx: &mut TestAppContext, handle: WindowHandle<Shell>, side: usize) {
    assert_eq!(counts(cx, handle), SELECTED);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[side].as_ref().unwrap();
            assert_eq!(host.pane.cursor, TARGET_INDEX);
            assert_eq!(host.selected, [TARGET_INDEX]);
            assert!(host.select.is_none());
            if shell.view == crate::window::startup::View::Dual {
                assert!(shell.panes[1 - side].as_ref().unwrap().selected.is_empty());
            }
        })
        .unwrap();
}

fn selected_launch(cx: &mut TestAppContext, view: &str, focus: usize) {
    let handle = open(cx, view, focus, Some(TARGET));
    listed(cx, handle, focus);
    rows(cx, handle, focus, 1);
    assert_selected(cx, handle, focus);
    handle
        .update(cx, |shell, _, cx| {
            shell.panes[focus].as_mut().unwrap().selected.clear();
            cx.notify();
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(
        counts(cx, handle),
        "9 items",
        "launch selection is one-shot"
    );
}

#[gpui::test]
fn launch_select_list(cx: &mut TestAppContext) {
    selected_launch(cx, "list", 0);
}

#[gpui::test]
fn launch_select_columns(cx: &mut TestAppContext) {
    selected_launch(cx, "columns", 0);
}

#[gpui::test]
fn launch_select_grid(cx: &mut TestAppContext) {
    selected_launch(cx, "grid", 0);
}

#[gpui::test]
fn launch_select_dual_left(cx: &mut TestAppContext) {
    selected_launch(cx, "dual", 0);
}

#[gpui::test]
fn launch_select_dual_right(cx: &mut TestAppContext) {
    selected_launch(cx, "dual", 1);
}

#[gpui::test]
fn launch_select_waits_for_initial_rows(cx: &mut TestAppContext) {
    let handle = open(cx, "list", 0, Some(TARGET));
    frame(cx, handle);
    listed(cx, handle, 0);
    cx.executor().advance_clock(Duration::from_millis(500));
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.pane.busy());
            assert_eq!(host.select.as_deref(), Some(TARGET));
            assert!(host.selected.is_empty());
        })
        .unwrap();
    rows(cx, handle, 0, 1);
    assert_selected(cx, handle, 0);
}

#[gpui::test]
fn plain_launch_keeps_item_count_in_every_view(cx: &mut TestAppContext) {
    for (view, side) in [
        ("list", 0),
        ("columns", 0),
        ("grid", 0),
        ("dual", 0),
        ("dual", 1),
    ] {
        let handle = open(cx, view, side, None);
        listed(cx, handle, side);
        rows(cx, handle, side, 1);
        assert_eq!(counts(cx, handle), "9 items", "{view} side {side}");
        handle
            .update(cx, |shell, _, _| {
                assert!(shell.panes[side].as_ref().unwrap().selected.is_empty());
            })
            .unwrap();
    }
}

#[gpui::test]
fn navigation_selection_already_reaches_footer(cx: &mut TestAppContext) {
    let handle = open(cx, "list", 0, None);
    listed(cx, handle, 0);
    rows(cx, handle, 0, 1);
    handle
        .update(cx, |shell, window, cx| {
            assert!(shell
                .step(0, Step::Refresh(Some(TARGET.into())), window, cx)
                .unwrap());
        })
        .unwrap();
    listed(cx, handle, 0);
    rows(cx, handle, 0, 2);
    assert_selected(cx, handle, 0);
}

// SW-18: a deep --select target is revealed on the first drawn frame, as v0.3.12's Show in folder does.
#[gpui::test]
fn sw18_launch_select_reveals_a_deep_row(cx: &mut TestAppContext) {
    const DEEP: usize = 1500;
    const TOTAL: usize = 2000;
    for view in ["list", "columns"] {
        let handle = open(cx, view, 0, Some(TARGET));
        deliver(
            cx,
            handle,
            0,
            Reply::Listed {
                path: "/files".into(),
                total: TOTAL,
            },
        );
        // The backend answers a deep select with the first rows around it (docs/protocol.md list select).
        let first = handle
            .update(cx, |shell, _, _| {
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .pane
                    .swap
                    .pending
                    .as_ref()
                    .unwrap()
                    .first
            })
            .unwrap();
        let start = (DEEP - first / 2).min(TOTAL - first);
        let rows = (start..start + first)
            .map(|at| RowFacts {
                name: if at == DEEP {
                    "notes.txt".into()
                } else {
                    format!("item-{at:04}")
                },
                size: 99,
                icon: "text-plain".into(),
                ..Default::default()
            })
            .collect();
        deliver(
            cx,
            handle,
            0,
            Reply::Rows {
                start,
                listing: 1,
                rows,
            },
        );
        for phase in ["initial", "after Columns entry"] {
            if phase != "initial" {
                cx.executor()
                    .advance_clock(Duration::from_millis(crate::pane::swap::CAP_MS + 1));
            }
            frame(cx, handle);
            handle
                .update(cx, |shell, _, _| {
                    let host = shell.panes[0].as_ref().unwrap();
                    assert_eq!(host.pane.cursor, DEEP, "{view}");
                    assert_eq!(host.selected, [DEEP], "{view}");
                    let (top, row, shown) =
                        if shell.shown_view() == crate::window::startup::View::Columns {
                            let columns = host.columns.as_ref().unwrap();
                            let paint = columns
                                .painters
                                .get(crate::columns::geometry::Slot::Active)
                                .unwrap()
                                .borrow();
                            let drawn = paint.drawn.as_ref().unwrap();
                            (
                                DEEP as f32 * drawn.row_height - drawn.scroll,
                                drawn.row_height,
                                f32::from(drawn.bounds.size.height),
                            )
                        } else {
                            let list = host.list.borrow();
                            let drawn = &list.drawn;
                            (
                                DEEP as f32 * drawn.metrics.row - drawn.scroll_y,
                                drawn.metrics.row,
                                f32::from(drawn.mask.size.height),
                            )
                        };
                    assert!(shown > 0., "{view} {phase}: a viewport was painted");
                    assert!(
                        top >= 0. && top + row <= shown,
                        "{view} {phase}: row top {top} outside {shown}"
                    );
                    if view == "columns" && phase != "initial" {
                        assert_eq!(shell.shown_view(), crate::window::startup::View::Columns);
                    }
                })
                .unwrap();
        }
    }
}
