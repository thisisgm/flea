//! The join review's unbound-key and bulk-selection regressions through the live window.
use super::join_tests::{answer, call, draw, opened, root, shell, state};
use crate::list::bench::real_cx;
use flea::jsondoc::Json;

#[test]
fn u32_an_unbound_modified_key_ends_the_delete_arm() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "d");
    assert!(!call(&mut cx, window, "lastMessage", &[]).is_empty());
    cx.simulate_keystrokes(window.into(), "ctrl-alt-9");
    shell(&mut cx, window, |shell, _, cx| {
        assert!(!shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .read(cx)
            .view()
            .arm
            .armed());
    });
    assert!(call(&mut cx, window, "lastMessage", &[]).is_empty());
    cx.simulate_keystrokes(window.into(), "d");
    assert_eq!(
        state(&mut cx, window)
            .get("confirmation")
            .unwrap()
            .get("opened"),
        Some(&Json::Bool(false))
    );
}

#[test]
fn u32_restore_from_a_bulk_selection_menu_keeps_its_provider_token() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "ctrl-a");
    answer(
        &mut cx,
        window,
        "select",
        r#", "selectionToken":12,"selectionCount":2"#,
    );
    answer(
        &mut cx,
        window,
        "window",
        r#", "total":2,"start":0,"selectionToken":12,"selectionCount":2,"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity","bulkSelected":true},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity","bulkSelected":true}]"#,
    );
    cx.simulate_keystrokes(window.into(), "m enter");
    shell(&mut cx, window, |shell, _, cx| {
        let view = shell.trash.as_ref().unwrap().pane.read(cx).view();
        assert_eq!(
            view.pending_op, "restore",
            "the menu accepts the unchanged bulk selection"
        );
        assert_eq!(view.selection.token, 12);
        assert_eq!(view.selected_count(), 2);
    });
}

fn right_pane(shell: &mut crate::state::Shell) {
    use crate::window::{pane::PaneHost, startup::View};
    use flea::protocol::{Reply, RowFacts};
    let mut host = PaneHost::new("/right".into(), None, 500., &shell.locale);
    host.pane
        .receive(Reply::Listed {
            path: "/right".into(),
            total: 100,
        })
        .unwrap();
    host.pane
        .receive(Reply::Rows {
            start: 0,
            rows: vec![RowFacts::default(); 100],
            listing: 3,
        })
        .unwrap();
    shell.panes[1] = Some(host);
    shell.view = View::Dual;
    shell.lifecycle.attach(1);
}

#[test]
fn u32_the_other_pane_keeps_keys_and_a_trash_row_click_takes_focus_back() {
    use gpui::{point, px, Modifiers, VisualTestContext};
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        right_pane(shell);
        shell.focus_pane(1, cx);
        window.focus(&shell.focus, cx);
        assert!(shell.focus.is_focused(window));
    });
    cx.simulate_keystrokes(window.into(), "j");
    shell(&mut cx, window, |shell, _, cx| {
        assert_eq!(shell.panes[1].as_ref().unwrap().pane.cursor, 1);
        assert_eq!(shell.trash.as_ref().unwrap().pane.read(cx).view().cursor, 0);
        let message = shell.trash_message();
        let adapter = super::readers::TrashAdapter {
            host: shell.trash.as_ref(),
            message: message.as_deref(),
            overlay: shell.overlay.as_ref(),
            cx,
        };
        assert!(
            crate::ipc::Adapter::call(&adapter, "lastMessage", &[]).is_none(),
            "the ordinary focused pane owns its displayed message"
        );
    });
    super::join_tests::draw(&mut cx, window);
    let centre = call(&mut cx, window, "trashRowCentre", &["0"]);
    let coords: Vec<f32> = centre
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    let before = shell(&mut cx, window, |shell, _, _| {
        shell.panes[1].as_ref().unwrap().pane.scroll_y
    });
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(coords[0]), px(coords[1])),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-31.))),
        modifiers: Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    shell(&mut cx, window, |shell, _, _| {
        assert_eq!(
            shell.panes[1].as_ref().unwrap().pane.scroll_y,
            before,
            "a wheel over Trash does not scroll the other pane"
        )
    });
    let centre = call(&mut cx, window, "trashRowCentre", &["0"]);
    let coords: Vec<f32> = centre
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.simulate_click(point(px(coords[0]), px(coords[1])), Modifiers::none());
    cx.simulate_keystrokes(window.into(), "j");
    shell(&mut cx, window, |shell, _, cx| {
        assert_eq!(shell.focus_side, 0);
        assert_eq!(shell.trash.as_ref().unwrap().pane.read(cx).view().cursor, 1);
        assert_eq!(shell.panes[1].as_ref().unwrap().pane.cursor, 1);
    });
}

#[test]
fn u32_reopening_on_another_pane_keeps_an_active_operations_producer() {
    use crate::backend::Event;
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        right_pane(shell);
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        pane.update(cx, |pane, cx| {
            pane.menu_action(Some("restoreTrashSelection"), cx);
            pane.close(cx);
        });
        shell.flush_trash(window, cx);
        shell.focus_pane(1, cx);
        shell.trash_action("openTrash", window, cx);
        let host = shell.trash.as_ref().unwrap();
        assert_eq!((host.side, host.backend_side), (1, 0));
        let id = host.pane.read(cx).view().request_id;
        shell.receive(0, Event::TrashLine(format!(r#"{{"t":"trashbrowse","id":{id},"op":"restore","ok":true,"done":1,"failed":0,"failures":[]}}"#)), window, cx);
        shell.flush_trash(window, cx);
        let view = shell.trash.as_ref().unwrap().pane.read(cx).view();
        assert!(
            !view.operation_active,
            "the original producer's completion is taken"
        );
        assert_eq!(
            view.pending_op, "list",
            "the reopened browser refreshes after completion"
        );
    });
}

#[test]
fn u32_external_confirmation_takeover_ends_the_previous_cards_session() {
    use crate::g2::file_ops::model::{Dialog, Operations};
    use flea::ui_logic_file_ops::{
        confirm::Source,
        trash::{Button, Review, Scope},
    };
    let review = |token: &str, count| Review {
        count: Some(count),
        token: token.into(),
        ..Default::default()
    };
    let mut operations = Operations::new("default");
    operations.confirm_delete(review("7", 1), Scope::Items, Source::Menu(17), 0);
    operations.tick(&crate::test_catalogs::english(), 2500);
    operations.present_external_delete(review("8", 2), Scope::TrashItems, 8);
    assert!(
        operations.confirm.is_none(),
        "the old review can no longer replace the external card"
    );
    assert_eq!(
        operations.due(),
        None,
        "only the Trash producer checks the new review"
    );
    assert!(operations
        .outbox
        .iter()
        .any(|line| line.contains(r#""op":"close""#)));
    assert!(
        matches!(&operations.dialog, Some(Dialog::Delete { review, .. }) if review.count == Some(2) && review.token == "8")
    );
    operations.press_delete(Button::Delete);
    assert_eq!(operations.external_answers, [(8, Some(8))]);
    operations.present_external_delete(review("9", 3), Scope::TrashItems, 9);
    operations.copy_to(25, false, "/private");
    assert_eq!(operations.external_answers, [(8, Some(8)), (9, None)]);
}

#[test]
fn u32_closing_trash_reveals_the_listings_message_without_leaking_its_arm_prompt() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().status = "listing status".into();
    });
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "d");
    shell(&mut cx, window, |shell, _, _| {
        assert!(shell.trash_message().unwrap().starts_with("Press d again"));
        assert_eq!(
            shell.panes[0].as_ref().unwrap().status.as_ref(),
            "listing status"
        );
        shell.panes[0].as_mut().unwrap().status = "new listing status".into();
    });
    cx.simulate_keystrokes(window.into(), "backspace");
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        assert!(!shell.trash_opened(cx));
        assert!(shell.trash_message().is_none());
        assert_eq!(
            shell.panes[0].as_ref().unwrap().status.as_ref(),
            "new listing status"
        );
        assert!(
            shell.trash.as_ref().unwrap().drawn_message.is_empty(),
            "the closed arm's message cannot leak into a later loading state"
        );
    });
    shell(&mut cx, window, |shell, window, cx| {
        shell.trash_action("openTrash", window, cx);
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(66));
    cx.run_until_parked();
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, _| {
        assert_eq!(
            shell.trash.as_ref().unwrap().presentation.stage,
            super::presentation::Stage::Loading
        );
        assert!(
            shell.trash_message().unwrap().is_empty(),
            "slow reopen never replays the prior arm prompt"
        );
    });
}

#[test]
fn u32_the_confirmation_state_carries_what_the_card_draws() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "d d");
    answer(
        &mut cx,
        window,
        "prepare",
        r#", "token":71,"count":1,"bytes":6,"all":false"#,
    );
    draw(&mut cx, window);
    let card = state(&mut cx, window).get("confirmation").unwrap().clone();
    assert_eq!(card.get("destructiveFocus"), Some(&Json::Bool(false)));
    assert_eq!(card.get("count").and_then(Json::as_f64), Some(1.));
    let title = card
        .get("title")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let body = card.get("body").and_then(Json::as_str).unwrap().to_string();
    assert!(!title.is_empty(), "the drawn card title is reported");
    assert!(body.contains("cannot be undone"), "the drawn body: {body}");
    cx.simulate_keystrokes(window.into(), "tab");
    let moved = state(&mut cx, window).get("confirmation").unwrap().clone();
    assert_eq!(
        moved.get("destructiveFocus"),
        Some(&Json::Bool(true)),
        "focus on Delete is reported"
    );
}
