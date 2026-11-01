//! Row readers retire their painted snapshot with the old surface.
use super::*;

#[test]
fn uish_misc_empty_directory_background_readers_follow_each_painted_view() {
    for view in [View::List, View::Grid, View::Columns] {
        let mut cx = context();
        let (window, _requests) = open(&mut cx, view);
        ready(&mut cx, window, "empty", 0);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(401));
        cx.run_until_parked();
        draw(&mut cx, window);
        let area = call(&mut cx, window, "listAreaRect", &[]);
        let point = call(&mut cx, window, "listingBackgroundCentre", &[]);
        assert_eq!(point.code, 0, "{view:?}: {point:?}");
        let area: Vec<f32> = area
            .stdout
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        let point: Vec<f32> = point
            .stdout
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(area.len(), 4, "{view:?}");
        assert_eq!(point.len(), 2, "{view:?}");
        assert!(area[2] > 0. && area[3] > 0., "{view:?}: {area:?}");
        assert!((point[0] - (area[0] + area[2] / 2.)).abs() <= 1.);
        assert!((point[1] - (area[1] + area[3] / 2.)).abs() <= 1.);
        window
            .update(&mut cx, |shell, _, _| shell.stop_for_app_quit())
            .unwrap();
    }
}

fn surface_after_painted_rows(view: View, kind: Kind) {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, view);
    ready(&mut cx, window, "current", 1);
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "rowAt", &["0"]),
        Reply::value("current-0|file|18.0 kB")
    );
    window
        .update(&mut cx, |shell, window, cx| {
            let path = if kind == Kind::Empty { "/" } else { "/missing" };
            if kind != Kind::Empty {
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                let now = pane.now();
                assert!(pane.begin(path.into(), crate::pane::swap::Options::default(), now));
            }
            match kind {
                Kind::Loading => {
                    let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                    assert!(pane.tick(pane.now() + crate::pane::swap::CAP_MS));
                    assert!(pane.swap.fell_back, "the real cap expired");
                }
                Kind::Empty => {
                    // Another program deleted the last file: the watch notice, its debounce, then the anchored re-read.
                    shell.receive(0, BackendEvent::Changed { path: path.into() }, window, cx);
                    let host = shell.panes[0].as_mut().unwrap();
                    let due = host.nav.watch.due.expect("the notice armed the watch");
                    assert!(host.nav.fire_watch(&mut host.pane, &[], due));
                    assert_eq!(host.nav.watch.rereads, 1);
                    assert!(
                        host.nav
                            .anchor
                            .as_ref()
                            .is_some_and(|anchor| anchor.path == path),
                        "the re-read rides the cursor anchor"
                    );
                    assert!(host.pane.busy(), "the watched re-read is in flight");
                    shell.run_navigation(0, window, cx);
                    for reply in [
                        BackendReply::Listed {
                            path: path.into(),
                            total: 0,
                        },
                        BackendReply::Rows {
                            start: 0,
                            listing: 8,
                            rows: vec![],
                        },
                    ] {
                        shell.receive(0, BackendEvent::Reply(reply), window, cx);
                    }
                }
                Kind::Error => shell.receive(
                    0,
                    BackendEvent::Reply(BackendReply::Error {
                        operation: "scan".into(),
                        path: path.into(),
                        message: "I/O error".into(),
                        mode: 0,
                    }),
                    window,
                    cx,
                ),
                _ => unreachable!(),
            }
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.state.kind, kind);
        })
        .unwrap();
    draw(&mut cx, window);
    assert_rows(&mut cx, window, ["loading", "", "0"]);
    window
        .update(&mut cx, |shell, _, _| {
            assert_eq!(shell.mark_drawn(0), kind == Kind::Loading);
            assert_eq!(shell.slots.states[0].is_some(), kind != Kind::Loading);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.drawn_view, Some(view));
            assert_eq!(
                host.pane.held.total, 0,
                "the transition cleared the old listing"
            );
            match view {
                View::Columns => {
                    let columns = host.columns.as_ref().unwrap();
                    let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                    let drawn = paint.drawn.as_ref().unwrap();
                    let held = drawn.held.as_ref().unwrap();
                    assert_eq!(held.total, 0);
                    assert!(paint.prepared(0).is_none());
                }
                View::Grid => {
                    let state = host.grid.as_ref().unwrap().state.borrow();
                    assert!(state.frame_held.is_none());
                    assert!(state.drawn_row(0).is_none());
                }
                _ => unreachable!(),
            }
        })
        .unwrap();
    println!("SURFACE {view:?} {kind:?}: rowAt=loading visibleRowName=empty drawnCount=0; old facts cleared");
}

#[test]
fn columns_rows_disappear_after_loading_cap() {
    surface_after_painted_rows(View::Columns, Kind::Loading);
}

#[test]
fn grid_rows_disappear_after_loading_cap() {
    surface_after_painted_rows(View::Grid, Kind::Loading);
}

#[test]
fn columns_rows_disappear_after_last_file_deleted() {
    surface_after_painted_rows(View::Columns, Kind::Empty);
}

#[test]
fn grid_rows_disappear_after_last_file_deleted() {
    surface_after_painted_rows(View::Grid, Kind::Empty);
}

#[test]
fn columns_rows_disappear_after_failed_navigation() {
    surface_after_painted_rows(View::Columns, Kind::Error);
}

#[test]
fn grid_rows_disappear_after_failed_navigation() {
    surface_after_painted_rows(View::Grid, Kind::Error);
}
