//! Carry in the production Shell from real keystrokes: X lifts the marked rows through the key table and dispatch, the backend names them, the footer says the carry, esc drops it, and a click lands it through op1 as a paste does.
use super::carry::{Carrying, LAYERS, OFFSET};
use super::m9_g2rows_25_rig::{
    answer, carry_key, click, edit, escape_key, footer_status, frame, mark, notice, open,
    pending_frame, press, reply, riding,
};
use crate::backend::Event as BackendEvent;
use gpui::{point, px, TestAppContext};

#[gpui::test]
fn m9_g2rows_25_carry_escape_restores_primary_and_muted_status(cx: &mut TestAppContext) {
    carry_end_restores_status(cx, false);
}

#[gpui::test]
fn m9_g2rows_25_carry_drop_restores_primary_and_muted_status(cx: &mut TestAppContext) {
    carry_end_restores_status(cx, true);
}

fn carry_end_restores_status(cx: &mut TestAppContext, drop: bool) {
    let (handle, mut lines) = open(cx, "/files", &["docs", "a.txt", "b.md", "c.txt"]);
    lines();
    mark(cx, handle, vec![1, 2, 3]);
    frame(cx, handle);
    let before = footer_status(cx, handle);
    assert!(before.0.starts_with("3 of 4 selected"));
    assert!(before.1.primary.is_empty() && before.1.hint.is_empty());
    carry_key(cx, handle);
    lines();
    answer(cx, handle, &["/files/a.txt", "/files/b.md", "/files/c.txt"]);
    let carrying = footer_status(cx, handle).1;
    assert_eq!(carrying.primary.as_ref(), "Carrying 3 items");
    assert_eq!(
        carrying.hint.as_ref(),
        " · click where they go, esc cancels"
    );
    if drop {
        assert!(click(cx, handle, "/files/docs", false, true));
        assert_eq!(lines().len(), 1);
    } else {
        escape_key(cx, handle);
        assert!(lines().is_empty());
    }
    assert!(riding(cx).is_none());
    assert_eq!(
        footer_status(cx, handle),
        before,
        "CARRY_END_RESTORES_STATUS"
    );
    assert_eq!(
        edit(cx, handle, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        vec![1, 2, 3]
    );
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_carry_captures_all_paths_and_escape_cancels(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "/files", &["docs", "a.txt", "b.md", "c.txt"]);
    lines();
    mark(cx, handle, vec![1, 2, 3]);
    carry_key(cx, handle);
    assert_eq!(
        lines(),
        vec![r#"{"c":"paths","rows":[1,2,3]}"#.to_string()],
        "M9_X_ASKS_ALL_MARKED_ROWS"
    );
    answer(cx, handle, &["/files/a.txt", "/files/b.md", "/files/c.txt"]);
    assert_eq!(
        notice(cx, handle),
        "Carrying 3 items",
        "M9_CARRY_SAYS_ITS_COUNT"
    );
    // The cursor and selection move on; the carry is still the lift's.
    mark(cx, handle, vec![0]);
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 0
    });
    frame(cx, handle);
    assert_eq!(
        notice(cx, handle),
        "Carrying 3 items",
        "M9_CARRY_SURVIVES_CURSOR_AND_SELECTION_MOVES"
    );
    assert_eq!(
        riding(cx).unwrap(),
        ["/files/a.txt", "/files/b.md", "/files/c.txt"],
        "M9_CARRY_KEEPS_THE_LIFTED_PATHS"
    );
    let flock = cx.update(|cx| cx.global::<Carrying>().flock.clone());
    assert_eq!(
        (flock.name.as_str(), flock.marks.len()),
        ("a.txt", LAYERS.len()),
        "M9_FLOCK_IS_THE_LIFT_FRONT_FIRST"
    );
    let (offset, layers) = edit(cx, handle, |shell, window, cx| {
        let element = shell
            .carry_flock(window, cx)
            .expect("M9_FLOCK_PAINTS_WHILE_RIDING");
        let layers = element.layers().len();
        (element.origin() - window.mouse_position(), layers)
    });
    assert_eq!(
        (offset, layers),
        (point(px(OFFSET.0), px(OFFSET.1)), 3),
        "M9_FLOCK_SITS_BY_THE_RESTING_POINTER"
    );
    escape_key(cx, handle);
    assert_eq!(
        notice(cx, handle),
        "",
        "M9_ESC_DROPS_THE_CARRY_AND_ITS_NOTICE"
    );
    assert!(lines().is_empty(), "M9_ESC_SENDS_NOTHING");
    assert!(riding(cx).is_none(), "M9_ESC_ENDS_THE_CARRY");
    assert!(
        edit(cx, handle, |shell, window, cx| shell
            .carry_flock(window, cx)
            .is_none()),
        "M9_NO_FLOCK_AFTER_ESC"
    );
    // Unmarked, X carries the cursor row alone.
    mark(cx, handle, Vec::new());
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 2
    });
    carry_key(cx, handle);
    assert_eq!(
        lines(),
        vec![r#"{"c":"paths","rows":[2]}"#.to_string()],
        "M9_UNMARKED_X_CARRIES_THE_CURSOR_ROW"
    );
    let refused = BackendEvent::Refused {
        operation: "paths".into(),
        path: String::new(),
        message: "gone".into(),
        mode: 0,
    };
    edit(cx, handle, move |shell, window, cx| {
        shell.receive(0, refused, window, cx)
    });
    assert!(
        cx.update(|cx| !cx.global::<Carrying>().carry.active()),
        "M9_A_REFUSED_ANSWER_ENDS_THE_LIFT"
    );
    assert!(
        !edit(cx, handle, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .gate
            .clip_pending),
        "M9_THE_SLOT_IS_RELEASED"
    );
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_gesture_payload_ack_and_peek_route_match(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "/files", &["peek", "a.txt", "b.txt", "c.txt"]);
    lines();
    // A cut on the clipboard first: no carry outcome may spend it.
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 3
    });
    press(cx, handle, "x", Some("x"), false);
    lines();
    reply(cx, handle, r#"{"t":"paths","paths":["/files/c.txt"]}"#);
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T1"}"#,
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"cut","paths":["/files/c.txt"],"token":"T1","skipped":0}"#,
    );
    lines();
    mark(cx, handle, vec![1, 2]);
    carry_key(cx, handle);
    assert_eq!(
        lines(),
        vec![r#"{"c":"paths","rows":[1,2]}"#.to_string()],
        "M9_X_ASKS_ITS_ROWS"
    );
    answer(cx, handle, &["/files/a.txt", "/files/b.txt"]);
    assert_eq!(notice(cx, handle), "Carrying 2 items", "M9_CARRY_RIDES");
    // Esc before any click: nothing is asked, and with no carry a click is not the carry's.
    escape_key(cx, handle);
    assert!(lines().is_empty(), "M9_CANCEL_ASKS_NOTHING");
    assert!(
        !click(cx, handle, "/files/peek", false, true),
        "M9_NO_CARRY_TAKES_NO_CLICK"
    );
    assert!(lines().is_empty());
    // Lift again and click the peek column's folder: the paste route's own collision ask, into that folder.
    carry_key(cx, handle);
    lines();
    answer(cx, handle, &["/files/a.txt", "/files/b.txt"]);
    // The drag's own refusal: a click on the folder the files are already in asks nothing and the carry keeps riding.
    assert!(click(cx, handle, "/files", false, true));
    assert!(lines().is_empty(), "M9_A_REFUSED_CLICK_ASKS_NOTHING");
    assert_eq!(
        notice(cx, handle),
        "Already in this folder.",
        "M9_THE_DRAG_REFUSAL_IS_SAID"
    );
    assert!(riding(cx).is_some(), "M9_A_REFUSED_CLICK_KEEPS_THE_CARRY");
    assert!(
        click(cx, handle, "/files/peek", false, true),
        "M9_CLICK_LANDS"
    );
    assert_eq!(
        lines(),
        vec![r#"{"c":"collisions","id":1,"dest":"/files/peek","paths":["/files/a.txt","/files/b.txt"]}"#.to_string()],
        "M9_LANDING_IS_THE_PASTE_ROUTE"
    );
    assert!(riding(cx).is_none(), "M9_AN_ACCEPTED_ASK_ENDS_THE_CARRY");
    assert_ne!(
        notice(cx, handle),
        "Carrying 2 items",
        "M9_A_LANDING_LEAVES_NO_CARRY_NOTICE"
    );
    assert!(
        edit(cx, handle, |shell, _, _| shell.clipboard.captured[0]
            .is_none()),
        "M9_CARRY_CAPTURES_NO_CLIPBOARD_ENTRY"
    );
    // While that ask is open, a second carry's click is refused: it asks nothing and the carry keeps riding.
    carry_key(cx, handle);
    lines();
    answer(cx, handle, &["/files/a.txt", "/files/b.txt"]);
    assert!(click(cx, handle, "/files/peek", true, false));
    let second = lines();
    assert!(
        second.is_empty(),
        "M9_A_SECOND_CLICK_ASKS_NOTHING: {second:?}"
    );
    assert_eq!(
        riding(cx),
        Some(vec!["/files/a.txt".to_string(), "/files/b.txt".to_string()]),
        "M9_A_REFUSED_ACK_KEEPS_THE_CARRY"
    );
    escape_key(cx, handle);
    assert!(riding(cx).is_none(), "M9_ESC_DROPS_THE_REFUSED_CARRY");
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    let sent = lines();
    assert!(
        sent.iter().any(|line| line.contains(r#""c":"transfer""#)
            && line.contains(r#""op":"move""#)
            && line.contains(r#""dest":"/files/peek""#)),
        "M9_MOVE_TRANSFERS_INTO_THE_PEEK_FOLDER: {sent:?}"
    );
    assert!(
        !sent.iter().any(|line| line.contains("clipClear")),
        "M9_CARRY_NEVER_SPENDS_THE_CUT: {sent:?}"
    );
    let current = edit(cx, handle, |shell, _, _| {
        shell.clipboard.board.current().clone()
    });
    assert_eq!(
        (current.paths, current.moving, current.token.as_str()),
        (vec!["/files/c.txt".to_string()], true, "T1"),
        "M9_THE_CUT_STANDS"
    );
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_favorites_and_cross_window_drop_preserve_source(cx: &mut TestAppContext) {
    let (from, mut from_lines) = open(cx, "/src", &["a.txt", "b.txt", "keep.txt"]);
    let (to, mut to_lines) = open(cx, "/dest", &["Projects", "x.txt"]);
    from_lines();
    to_lines();
    mark(cx, from, vec![0, 1]);
    carry_key(cx, from);
    assert_eq!(
        from_lines(),
        vec![r#"{"c":"paths","rows":[0,1]}"#.to_string()],
        "M9_THE_SOURCE_WINDOW_ASKS"
    );
    assert!(to_lines().is_empty(), "M9_THE_OTHER_WINDOW_IS_NOT_ASKED");
    answer(cx, from, &["/src/a.txt", "/src/b.txt"]);
    // The other window's own cursor and selection never reach the carry.
    edit(cx, to, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1];
        host.pane.cursor = 1;
    });
    let rail = edit(cx, to, |shell, _, _| shell.rail.is_some());
    // A Favorites folder in the other window takes the drop through the same route.
    assert!(
        click(cx, to, "/home/fav/Projects", true, false),
        "M9_CROSS_WINDOW_CLICK_LANDS"
    );
    assert_eq!(
        to_lines(),
        vec![r#"{"c":"collisions","id":1,"dest":"/home/fav/Projects","paths":["/src/a.txt","/src/b.txt"]}"#.to_string()],
        "M9_FAVORITE_GETS_THE_SOURCE_PATHS"
    );
    assert!(
        from_lines().is_empty(),
        "M9_THE_SOURCE_BACKEND_SENDS_NOTHING"
    );
    assert_eq!(
        edit(cx, to, |shell, _, _| shell.rail.is_some()),
        rail,
        "M9_NO_RAIL_CHANGE"
    );
    assert_eq!(
        edit(cx, to, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        vec![1],
        "M9_DEST_SELECTION_KEPT"
    );
    assert!(riding(cx).is_none());
    // The landing repaints the source window, whose own frame withdraws its notice.
    pending_frame(cx, from);
    assert_eq!(
        notice(cx, from),
        "",
        "M9_SOURCE_NOTICE_WITHDRAWN_AFTER_A_REMOTE_LANDING"
    );
    // A window that ends takes only its own carry.
    mark(cx, from, vec![2]);
    carry_key(cx, from);
    from_lines();
    answer(cx, from, &["/src/keep.txt"]);
    assert!(
        !edit(cx, to, |shell, window, cx| shell
            .carry_window_ended(window, cx)),
        "M9_ANOTHER_WINDOW_ENDING_KEEPS_IT"
    );
    assert!(riding(cx).is_some());
    assert!(
        edit(cx, from, |shell, window, cx| shell
            .carry_window_ended(window, cx)),
        "M9_THE_SOURCE_WINDOW_ENDING_DROPS_IT"
    );
    assert!(riding(cx).is_none());
    assert!(
        from_lines().is_empty() && to_lines().is_empty(),
        "M9_LIFECYCLE_END_ASKS_NOTHING"
    );
    edit(cx, from, |shell, _, _| shell.stop_for_app_quit());
    edit(cx, to, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m9_g2rows_25_standing_notice_survives_clock_and_withdraws_only_itself(cx: &mut TestAppContext) {
    use flea::ui_logic_status_feedback::status::Hint;
    let (handle, mut lines) = open(cx, "/files", &["a.txt"]);
    lines();
    assert!(edit(cx, handle, |shell, window, cx| shell.carry_lift(window, cx)));
    assert_eq!(lines(), [r#"{"c":"paths","rows":[0]}"#]);
    answer(cx, handle, &["/files/a.txt"]);
    edit(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, _| {
            assert!(
                !messages.tick(u64::MAX / 2).notice_cleared,
                "CARRY_NOTICE_MUST_STAND"
            );
            assert!(!messages.notice().is_empty());
        });
    });
    assert!(edit(cx, handle, |shell, window, cx| shell
        .carry_escape(window, cx)));
    assert_eq!(notice(cx, handle), "");
    edit(cx, handle, |shell, window, cx| shell.carry_lift(window, cx));
    lines();
    answer(cx, handle, &["/files/a.txt"]);
    edit(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.say("newer notice", false, "", Hint::None, now)
        });
    });
    edit(cx, handle, |shell, window, cx| {
        shell.carry_escape(window, cx)
    });
    assert_eq!(
        notice(cx, handle),
        "newer notice",
        "WITHDRAW_MUST_KEEP_NEWER_NOTICE"
    );
    assert!(lines().is_empty());
    edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
}
