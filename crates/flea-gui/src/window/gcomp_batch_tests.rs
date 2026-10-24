//! Shared-reader and Omi ownership checks over the actual mounted Columns.
use super::*;
use crate::{columns::geometry::Slot, feedback::spinner::Mode};
use std::{rc::Rc, time::Duration};

pub(super) fn columns(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<(usize, usize)>) {
    let handle = open(cx);
    let (backend, requests) = Backend::test_channel();
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx);
        shell.launch.settings.columns = 3;
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = Some(backend);
        assert!(host
            .pane
            .begin("/srv/a/b/c".into(), Default::default(), host.pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/srv/a/b/c".into(),
                total: 2,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing: 50,
                rows: ["d01", "d02"]
                    .map(|name| flea::protocol::RowFacts {
                        name: name.into(),
                        dir: true,
                        ..Default::default()
                    })
                    .into(),
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
    });
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(View::Columns, window, cx)
    });
    (handle, requests)
}

#[test]
fn gcomp_batch_u02_drawn_mode_follows_the_actual_u09_columns_not_the_selected_view() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests) = columns(&mut cx);
    frame(&mut cx, handle);
    assert_eq!(call(&mut cx, handle, "viewMode"), Reply::value("list"));
    let list_frames = edit(&mut cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().list.borrow().drawn.frames
    });
    edit(&mut cx, handle, |shell, window, cx| {
        crate::window::columns_tests::host::settle(shell, window, cx);
    });
    frame(&mut cx, handle);
    assert_eq!(call(&mut cx, handle, "viewMode"), Reply::value("columns"));
    edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.list.borrow().drawn.frames,
            list_frames,
            "a List stand-in repainted under the Columns label"
        );
        let columns = host.columns.as_ref().unwrap();
        let painter = columns.painters.get(Slot::Active).unwrap().borrow();
        let drawn = painter
            .drawn
            .as_ref()
            .expect("the actual Columns Element painted");
        assert!(drawn.bounds.size.width > gpui::px(0.));
        assert!(painter.visible().count() > 0);
        assert_eq!(host.drawn_view, Some(View::Columns));
        assert!(!host.drawn_list);
    });
    edit(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::Grid, window, cx)
    });
    frame(&mut cx, handle);
    assert_eq!(
        call(&mut cx, handle, "viewMode"),
        Reply::value("grid"),
        "the actual U10 Grid must own DRAWN after leaving Columns"
    );
    let name = edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.drawn_view, Some(View::Grid));
        assert!(!host.drawn_list);
        let grid = host.grid.as_ref().expect("actual Grid mount");
        let state = grid.state.borrow();
        assert!(state.frames > 0);
        assert!(state.bounds.size.height > gpui::px(0.));
        let tile = state.drawn().first().expect("an actual painted tile");
        assert_eq!(tile.listing, 0);
        tile.name().expect("painted tile name").to_owned()
    });
    edit(&mut cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: "visibleRowName".into(),
                    arguments: vec!["0".into()],
                },
                answer,
            },
            window,
            cx,
        );
        assert_eq!(receive.try_recv().unwrap().reply, Reply::value(name));
    });
}

#[test]
fn gcomp_batch_u09_loading_uses_omi_preparation_and_paint_then_stops_when_hidden() {
    let mut cx = crate::list::bench::real_cx();
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, _requests) = columns(&mut cx);
    frame(&mut cx, handle);
    cx.run_until_parked();
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::CAP_MS));
    cx.run_until_parked();
    frame(&mut cx, handle);
    assert_eq!(call(&mut cx, handle, "viewMode"), Reply::value("columns"));
    let mark = edit(&mut cx, handle, |shell, _, _| {
        Rc::clone(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .loading[Slot::Child.index()]
            .as_ref()
            .unwrap(),
        )
    });
    assert_eq!(mark.mode(), Mode::Thinking);
    assert!(mark.slot().size.width > gpui::px(0.));
    assert!(
        mark.counters.frames.get() > 0,
        "the mounted loading tile painted nothing"
    );
    assert_eq!(
        mark.counters.writes.get(),
        mark.counters.frames.get(),
        "the root must prepare each painted Omi sample exactly once"
    );
    assert_eq!(
        mark.counters.requests.get(),
        0,
        "reduced motion must not schedule"
    );
    assert_eq!(mark.ink(), crate::palette::Theme::default().accent_frame);
    let before = (mark.counters.writes.get(), mark.counters.frames.get());
    edit(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx)
    });
    frame(&mut cx, handle);
    assert_eq!(
        (mark.counters.writes.get(), mark.counters.frames.get()),
        before
    );
}
