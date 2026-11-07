//! Additional entry-cap and dispatch-order controls over the retained independent review probes.
use super::*;
use crate::{
    columns::TrackRows,
    list::pane::Move,
    window::columns_tests::host::{advance, row},
};

#[test]
fn entry_retargets_without_restarting_its_original_cap() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    frame(&mut cx, window);
    let mut original = None;
    change(&mut cx, window, |shell, window, cx| {
        original = shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .area
            .landing
            .cap_deadline();
        assert!(original.is_some());
        for movement in [Move::End, Move::Home, Move::End] {
            assert!(shell.panes[0].as_mut().unwrap().pane.move_cursor(movement));
            shell.sync_columns(window, cx);
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            assert!(columns.entering);
            assert_eq!(columns.area.landing.cap_deadline(), original);
            assert_eq!(columns.counters.holds, 1);
        }
        assert!(shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .area
            .landing
            .waiting("/srv/a/b/c/d02", "/srv/a/b/c"));
        assert!(!shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .area
            .landing
            .waiting("/srv/a/b/c/d01", "/srv/a/b/c"));
    });
    advance(
        &mut cx,
        window,
        Duration::from_millis(crate::pane::swap::CAP_MS + 20),
    );
    change(&mut cx, window, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let columns = host.columns.as_ref().unwrap();
        assert!(host.pane.now() >= original.unwrap());
        assert!(
            host.pane.now() < columns.area.host.fallback_at.unwrap(),
            "proof invalid: the separate child fallback already elapsed"
        );
        tick(shell);
    });
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        assert_eq!(shell.shown_view(), View::Columns);
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let child = columns
            .area
            .tracks()
            .iter()
            .find(|v| v.slot == Slot::Child)
            .unwrap();
        assert_eq!(
            (child.path.as_str(), child.state),
            ("/srv/a/b/c/d02", TrackState::Loading)
        );
        assert!(matches!(child.rows, TrackRows::Peek(ref rows) if rows.is_empty()));
        assert!(columns.loading_since[Slot::Child.index()].get().is_some());
        let size = columns.loading[Slot::Child.index()]
            .as_ref()
            .unwrap()
            .slot()
            .size;
        assert!(size.width > gpui::px(0.) && size.height > gpui::px(0.));
        assert_eq!(columns.counters.blank_frames, 0);
        assert_eq!(columns.counters.fallbacks, 1);
        assert_eq!(columns.area.landing.cap_deadline(), None);
    });
}

#[test]
fn entry_cap_preserves_file_and_absent_child_slots() {
    for absent in [false, true] {
        let mut cx = context();
        let (window, _requests) = open(&mut cx, "/srv/a/b/c");
        frame(&mut cx, window);
        change(&mut cx, window, |shell, window, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.columns.as_mut().unwrap().area.settings.column = true;
            let rows = if absent {
                vec![]
            } else {
                vec![row("file.txt", false)]
            };
            host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
                listing: host.pane.held.listing + 1,
                total: rows.len(),
                start: 0,
                rows,
            });
            host.changed(&shell.locale);
            shell.sync_columns(window, cx);
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            assert!(columns.entering);
            assert!(!columns.area.landing.waiting("/srv/a/b/c/d01", "/srv/a/b/c"));
            assert!(!columns
                .area
                .drawn
                .contains(&"/srv/a/b/c/file.txt".to_owned()));
        });
        advance(
            &mut cx,
            window,
            Duration::from_millis(crate::pane::swap::CAP_MS + 20),
        );
        change(&mut cx, window, |shell, _, _| tick(shell));
        frame(&mut cx, window);
        change(&mut cx, window, |shell, _, _| {
            assert_eq!(shell.shown_view(), View::Columns);
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let child = columns
                .area
                .tracks()
                .iter()
                .find(|v| v.slot == Slot::Child)
                .unwrap();
            assert!(child.path.is_empty());
            assert_eq!(child.state, TrackState::Rows);
            assert_eq!(
                child.rows,
                if absent {
                    TrackRows::Blank
                } else {
                    TrackRows::Preview
                }
            );
            assert!(columns.loading_since[Slot::Child.index()].get().is_none());
            assert_eq!(columns.counters.blank_frames, 0);
        });
    }
}

#[test]
fn minimum_release_rebuilds_for_both_dispatch_orders_without_another_reply() {
    for timer_first in [true, false] {
        let mut cx = context();
        let (window, _requests) = open(&mut cx, "/srv/a/b/c");
        let mut asked_first = 0;
        change(&mut cx, window, |shell, _, _| {
            asked_first = shell.panes[0].as_ref().unwrap().pane.first_count();
        });
        frame(&mut cx, window);
        advance(
            &mut cx,
            window,
            Duration::from_millis(crate::pane::swap::CAP_MS + 20),
        );
        change(&mut cx, window, |shell, _, _| tick(shell));
        frame(&mut cx, window);
        let mut minimum = None;
        change(&mut cx, window, |shell, window, cx| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let painted = columns
                .loading_since
                .iter()
                .filter_map(|since| since.get())
                .max()
                .unwrap();
            minimum = Some(painted + crate::pane::swap::MIN_SHOW_MS);
            assert_eq!(columns.area.landing.minimum_deadline(), minimum);
            assert_eq!(columns.timer.as_ref().map(|(due, _)| *due), minimum);
            let mut reply = document(shell, "/srv/a/b", &[("c", true)], false);
            if let flea::jsondoc::Json::Obj(fields) = &mut reply {
                fields.iter_mut().find(|(key, _)| key == "first").unwrap().1 =
                    flea::jsondoc::Json::Num(asked_first.to_string());
            }
            shell.receive(0, crate::backend::Event::Peeked(reply), window, cx);
        });
        frame(&mut cx, window);
        let mut requests = 0;
        change(&mut cx, window, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let columns = host.columns.as_ref().unwrap();
            assert!(
                host.pane.now() < minimum.unwrap(),
                "proof invalid: minimum elapsed before control"
            );
            assert_eq!(columns.area.landing.minimum_deadline(), minimum);
            assert_eq!(columns.timer.as_ref().map(|(due, _)| *due), minimum);
            assert_eq!(
                columns
                    .area
                    .peeks
                    .names(&crate::columns::peeks::peek_key("/srv/a/b", false, false)),
                "c"
            );
            assert_eq!(
                columns
                    .area
                    .tracks()
                    .iter()
                    .find(|v| v.slot == Slot::Parent)
                    .unwrap()
                    .state,
                TrackState::Loading
            );
            requests = columns.area.peek_requests;
        });
        advance(
            &mut cx,
            window,
            Duration::from_millis(crate::pane::swap::MIN_SHOW_MS + 20),
        );
        if timer_first {
            change(&mut cx, window, |shell, window, cx| {
                tick(shell);
                shell.arm_columns(window, cx);
                assert_eq!(
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .columns
                        .as_ref()
                        .unwrap()
                        .area
                        .tracks()
                        .iter()
                        .find(|v| v.slot == Slot::Parent)
                        .unwrap()
                        .state,
                    TrackState::Rows
                );
            });
        }
        frame(&mut cx, window);
        change(&mut cx, window, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            assert_eq!(
                columns
                    .area
                    .tracks()
                    .iter()
                    .find(|v| v.slot == Slot::Parent)
                    .unwrap()
                    .state,
                TrackState::Rows
            );
            assert_eq!(
                columns
                    .area
                    .peeks
                    .names(&crate::columns::peeks::peek_key("/srv/a/b", false, false)),
                "c"
            );
            assert_eq!(columns.area.peek_requests, requests);
            assert_eq!(columns.area.landing.minimum_deadline(), None);
            assert!(columns.timer.is_none());
            assert_eq!(columns.counters.blank_frames, 0);
            eprintln!("release control timer_first={timer_first}: Rows, minimum=None, timer=None, requests={requests}");
        });
    }
}
