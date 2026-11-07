//! Cold review probes with production timer state updates and explicitly driven frames.
use crate::window::columns_tests::host::{advance, change, context, document, frame, open};
use crate::{
    columns::{geometry::Slot, TrackState},
    state::Shell,
    window::startup::View,
};
use std::time::Duration;

// Run the timer's production state update without driving GPUI's automatic frame executor.
fn tick(shell: &mut Shell) {
    let host = shell.panes[0].as_mut().unwrap();
    let now = host.pane.now();
    super::with_area(host, |columns, pane, ports| {
        columns.timer = None;
        columns.area.tick(&pane, ports, now);
    });
}

#[test]
fn review_r2_cap_draws_a_loading_tile_in_the_unanswered_child() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    frame(&mut cx, window);
    advance(
        &mut cx,
        window,
        Duration::from_millis(crate::pane::swap::CAP_MS + 20),
    );
    change(&mut cx, window, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let now = host.pane.now();
        let fallback = host
            .columns
            .as_ref()
            .unwrap()
            .area
            .host
            .fallback_at
            .unwrap();
        eprintln!("entry cap tick={now}ms; separate child fallback={fallback}ms");
        assert!(
            now < fallback,
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
        assert_eq!(child.state, TrackState::Loading,
            "the first Columns frame after CAP_MS must draw the child's loading tile; got {child:?}; blank_frames={}", columns.counters.blank_frames);
        assert_eq!(child.path, "/srv/a/b/c/d01");
        let mark = columns.loading[Slot::Child.index()].as_ref().unwrap();
        assert!(
            columns.loading_since[Slot::Child.index()].get().is_some(),
            "the actual child tile did not paint"
        );
        assert!(mark.slot().size.width > gpui::px(0.) && mark.slot().size.height > gpui::px(0.));
        assert_eq!(columns.counters.blank_frames, 0);
    });
    change(&mut cx, window, |shell, _, _| {
        let now = shell.panes[0].as_ref().unwrap().pane.now();
        super::with_area(shell.panes[0].as_mut().unwrap(), |columns, pane, ports| {
            columns.area.host.shown = Default::default();
            columns.area.sync(&pane, ports, now);
        });
    });
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let child = columns
            .area
            .tracks()
            .iter()
            .find(|v| v.slot == Slot::Child)
            .unwrap();
        assert!(matches!(child.rows, crate::columns::TrackRows::Blank));
        assert!(child.path.is_empty());
        assert!(
            columns.counters.blank_frames > 0,
            "an unexpectedly blank requested child went uncounted"
        );
    });
}

#[test]
fn review_r2_loading_minimum_starts_at_the_first_drawn_tile() {
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
    change(&mut cx, window, |shell, _, _| {
        tick(shell);
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(
            columns.painters.built(),
            0,
            "proof requires no Columns frame before the explicit draw"
        );
        assert_eq!(
            columns.area.landing.minimum_deadline(),
            None,
            "model construction impersonated a drawn tile"
        );
    });
    advance(&mut cx, window, Duration::from_millis(80));
    let mut first_draw = 0;
    change(&mut cx, window, |shell, _, _| {
        first_draw = shell.panes[0].as_ref().unwrap().pane.now();
    });
    frame(&mut cx, window);
    change(&mut cx, window, |shell, window, cx| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let shown_at = columns
            .loading_since
            .iter()
            .filter_map(|since| since.get())
            .max()
            .unwrap();
        let deadline = shown_at + crate::pane::swap::MIN_SHOW_MS;
        assert_eq!(columns.area.landing.minimum_deadline(), Some(deadline));
        assert_eq!(
            columns.timer.as_ref().map(|(due, _)| *due),
            Some(deadline),
            "paint must arm the minimum before any reply"
        );
        let mut reply = document(shell, "/srv/a/b", &[("c", true)], false);
        if let flea::jsondoc::Json::Obj(fields) = &mut reply {
            fields.iter_mut().find(|(key, _)| key == "first").unwrap().1 =
                flea::jsondoc::Json::Num(asked_first.to_string());
        }
        shell.receive(0, crate::backend::Event::Peeked(reply), window, cx);
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(
            columns
                .area
                .peeks
                .names(&crate::columns::peeks::peek_key("/srv/a/b", false, false)),
            "c",
            "proof requires the outstanding reply to land"
        );
        let parent = columns
            .area
            .tracks()
            .iter()
            .find(|v| v.slot == Slot::Parent)
            .unwrap();
        assert_eq!(
            parent.state,
            TrackState::Loading,
            "the reply initially obeys the model minimum"
        );
    });
    advance(&mut cx, window, Duration::from_millis(40));
    change(&mut cx, window, |shell, _, _| tick(shell));
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        let elapsed = u128::from(shell.panes[0].as_ref().unwrap().pane.now() - first_draw);
        assert!(
            elapsed < u128::from(crate::pane::swap::MIN_SHOW_MS),
            "proof invalid under this load: {elapsed}ms already elapsed"
        );
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let parent = columns
            .area
            .tracks()
            .iter()
            .find(|v| v.slot == Slot::Parent)
            .unwrap();
        assert_eq!(parent.state, TrackState::Loading,
            "the tile has drawn for only {elapsed}ms, less than MIN_SHOW_MS 100, but already released to {parent:?}");
    });
    advance(
        &mut cx,
        window,
        Duration::from_millis(crate::pane::swap::MIN_SHOW_MS + 10),
    );
    change(&mut cx, window, |shell, _, _| tick(shell));
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let parent = columns
            .area
            .tracks()
            .iter()
            .find(|v| v.slot == Slot::Parent)
            .unwrap();
        assert_eq!(
            parent.state,
            TrackState::Rows,
            "the answered parent did not release after its drawn minimum"
        );
    });
}

#[test]
fn review_r3_render_expiry_does_not_cancel_the_only_release() {
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
    change(&mut cx, window, |shell, window, cx| {
        let mut reply = document(shell, "/srv/a/b", &[("c", true)], false);
        if let flea::jsondoc::Json::Obj(fields) = &mut reply {
            fields.iter_mut().find(|(key, _)| key == "first").unwrap().1 =
                flea::jsondoc::Json::Num(asked_first.to_string());
        }
        shell.receive(0, crate::backend::Event::Peeked(reply), window, cx);
    });
    // Consume the reply's seen invalidation while the real painted minimum is still active.
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let due = columns.area.landing.minimum_deadline().unwrap();
        assert_eq!(columns.timer.as_ref().map(|(due, _)| *due), Some(due));
        assert!(
            shell.panes[0].as_ref().unwrap().pane.now() < due,
            "proof invalid: load already exhausted the first minimum"
        );
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
    });
    advance(
        &mut cx,
        window,
        Duration::from_millis(crate::pane::swap::MIN_SHOW_MS + 20),
    );
    // A real window draw wins the UI dispatch race with the scheduled release callback.
    frame(&mut cx, window);
    let mut observed = TrackState::Rows;
    change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        observed = columns
            .area
            .tracks()
            .iter()
            .find(|v| v.slot == Slot::Parent)
            .unwrap()
            .state;
        eprintln!(
            "after render won expiry: parent={observed:?}, minimum={:?}, timer={:?}",
            columns.area.landing.minimum_deadline(),
            columns.timer.as_ref().map(|(due, _)| *due)
        );
        assert_eq!(
            columns
                .area
                .peeks
                .names(&crate::columns::peeks::peek_key("/srv/a/b", false, false)),
            "c"
        );
    });
    // Control: the existing timer update rebuilds the ready rows without any new reply.
    change(&mut cx, window, |shell, _, _| tick(shell));
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
            TrackState::Rows,
            "control: production timer state update must release the stored reply"
        );
        eprintln!("control: normal timer state update released the stored parent rows");
    });
    assert_eq!(
        observed,
        TrackState::Rows,
        "render expired the minimum and canceled its timer without rebuilding the answered parent"
    );
}

#[test]
fn review_r3_entry_cap_follows_the_current_requested_child() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    frame(&mut cx, window);
    change(&mut cx, window, |shell, window, cx| {
        assert_eq!(shell.shown_view(), View::List);
        assert!(shell.panes[0]
            .as_mut()
            .unwrap()
            .pane
            .move_cursor(crate::list::pane::Move::End));
        shell.sync_columns(window, cx);
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert!(
            columns.entering,
            "proof requires the cursor move inside the entry hold"
        );
        assert!(columns.area.drawn.contains(&"/srv/a/b/c/d02".to_string()));
        assert!(!columns.area.peeks.answered(
            &crate::columns::peeks::peek_key("/srv/a/b/c/d02", false, false),
            "/srv/a/b/c/d02"
        ));
    });
    advance(
        &mut cx,
        window,
        Duration::from_millis(crate::pane::swap::CAP_MS + 20),
    );
    change(&mut cx, window, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let now = host.pane.now();
        let fallback = host
            .columns
            .as_ref()
            .unwrap()
            .area
            .host
            .fallback_at
            .unwrap();
        eprintln!("changed-child entry cap tick={now}ms; separate child fallback={fallback}ms");
        assert!(
            now < fallback,
            "proof invalid: separate 150ms fallback already ran"
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
        eprintln!(
            "changed-child cap result: {child:?}; blank_frames={}",
            columns.counters.blank_frames
        );
        assert_eq!((child.path.as_str(), child.state), ("/srv/a/b/c/d02", TrackState::Loading),
            "the entry cap must publish the cursor's current requested child and its loading tile together");
    });
}

#[path = "columns_review_r3_controls.rs"]
mod controls;
