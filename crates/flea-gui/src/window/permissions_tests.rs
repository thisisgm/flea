//! Permissions is opened by the mounted menu and served by its captured pane's backend.
#[path = "permissions_test_fixture.rs"]
mod fixture;
#[path = "permissions_guard_tests.rs"]
mod guards;
#[path = "permissions_ime_tests.rs"]
mod ime;
#[path = "permissions_reader_tests.rs"]
mod reader;
use crate::g2::op3::permissions::Stop;
use fixture::*;
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn m9_permissions_menu_opens_on_the_selected_item_and_escape_releases_it(cx: &mut TestAppContext) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    let inspect = next(&mut requests, "permissions", "inspect");
    assert_eq!(model(cx, handle).focus, Stop::Cancel);
    assert!(inspect.contains(r#""path":"/files/notes.txt""#));
    assert!(edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    assert!(!edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
    assert!(next(&mut requests, "permissions", "close").contains(r#""id":1"#));
    assert!(edit(cx, handle, |shell, window, _| shell
        .focus
        .is_focused(window)));
}

#[gpui::test]
fn m9_permissions_each_painted_checkbox_reaches_apply_with_its_own_bit(cx: &mut TestAppContext) {
    for at in 0..9 {
        let (handle, mut requests) = setup(cx);
        choose(cx, handle, &mut requests);
        let line = next(&mut requests, "permissions", "inspect");
        let id = flea::json::field_usize(&line, "id").unwrap() as u64;
        inspect(cx, handle, id, "notes.txt", "0644", "");
        click(
            cx,
            handle,
            [
                "op3-check-0",
                "op3-check-1",
                "op3-check-2",
                "op3-check-3",
                "op3-check-4",
                "op3-check-5",
                "op3-check-6",
                "op3-check-7",
                "op3-check-8",
            ][at],
        );
        click(cx, handle, "op3-button-1");
        let apply = next(&mut requests, "permissions", "apply");
        assert!(
            apply.contains(&format!("\"mode\":\"{:04o}\"", 0o644 ^ (1 << (8 - at)))),
            "{apply}"
        );
        VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
        assert!(
            edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)),
            "a running chmod cannot be dismissed"
        );
        VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("tab");
        assert_eq!(model(cx, handle).focus, Stop::Apply);
        receive(
            cx,
            handle,
            &format!(r#"{{"t":"permissions","id":{id},"op":"apply","ok":true}}"#),
        );
        cx.run_until_parked();
        assert!(!edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
        assert!(next(&mut requests, "permissions", "close").contains(&format!("\"id\":{id}")));
    }
}

#[gpui::test]
fn m9_permissions_octal_validation_and_backend_refusal_stay_visible_until_retry(
    cx: &mut TestAppContext,
) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    click(cx, handle, "op3-field");
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("ctrl-a 8 enter");
    assert!(!std::iter::from_fn(&mut requests).any(|line| line.contains(r#""op":"apply""#)));
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("ctrl-a 0 7 5 5 enter");
    assert!(!std::iter::from_fn(&mut requests).any(|line| line.contains(r#""op":"apply""#)));
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("enter");
    let apply = next(&mut requests, "permissions", "apply");
    assert!(apply.contains(r#""mode":"0755""#), "{apply}");
    receive(
        cx,
        handle,
        r#"{"t":"permissions","id":1,"op":"apply","ok":false,"error":"Selected item changed; reopen Permissions."}"#,
    );
    cx.run_until_parked();
    assert!(VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("op3-note:Selected item changed; reopen Permissions.")
        .is_some());
    assert_eq!(model(cx, handle).focus, Stop::Cancel);
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("tab enter");
    assert!(next(&mut requests, "permissions", "apply").contains(r#""mode":"0755""#));
    receive(
        cx,
        handle,
        r#"{"t":"permissions","id":1,"op":"apply","ok":false,"error":"Permission denied"}"#,
    );
    click(cx, handle, "op3-button-0");
    assert!(!edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
}

#[gpui::test]
fn m9_permissions_mixed_selection_applies_one_batch_and_closes_all_inspects(
    cx: &mut TestAppContext,
) {
    let (handle, mut requests) = setup(cx);
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1]
    });
    choose(cx, handle, &mut requests);
    assert!(next(&mut requests, "permissions", "inspect").contains(r#""id":1000"#));
    assert!(next(&mut requests, "permissions", "inspect").contains(r#""id":1001"#));
    inspect(cx, handle, 1000, "notes.txt", "0644", "");
    inspect(cx, handle, 1001, "other.txt", "0755", "");
    click(cx, handle, "op3-check-2");
    click(cx, handle, "op3-button-1");
    let batch = next(&mut requests, "permissionsBatch", "");
    assert!(batch.contains(r#""modes":["0744","0755"]"#), "{batch}");
    receive(
        cx,
        handle,
        r#"{"t":"permissions","id":1,"op":"applyMany","ok":true}"#,
    );
    cx.run_until_parked();
    assert!(!edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
    for id in [1000, 1001] {
        assert!(next(&mut requests, "permissions", "close").contains(&format!("\"id\":{id}")));
    }
}

#[gpui::test]
fn m9_permissions_inspect_refusal_and_backend_death_keep_cancel_live(cx: &mut TestAppContext) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(
        cx,
        handle,
        1,
        "notes.txt",
        "0644",
        "Read-only: you are not the owner.",
    );
    click(cx, handle, "op3-button-1");
    assert!(!std::iter::from_fn(&mut requests).any(|line| line.contains(r#""op":"apply""#)));
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, crate::backend::Event::Drained, window, cx)
    });
    cx.run_until_parked();
    assert!(VisualTestContext::from_window(handle.into(), cx).debug_bounds("op3-note:Permissions is unavailable because Flea stopped responding. Restart Flea and reopen Permissions.").is_some());
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    assert!(!edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
}

#[gpui::test]
fn m9_permissions_slow_inspect_holds_loading_then_swaps_the_complete_card(cx: &mut TestAppContext) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    assert!(
        visual.debug_bounds("op3-card").is_none(),
        "reading waits for the shared 150 ms hold"
    );
    visual
        .executor()
        .advance_clock(std::time::Duration::from_millis(150));
    visual.run_until_parked();
    assert!(visual.debug_bounds("op3-card").is_some());
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    assert!(
        visual.debug_bounds("op3-field").is_none(),
        "a drawn reading card stands for 100 ms"
    );
    visual
        .executor()
        .advance_clock(std::time::Duration::from_millis(100));
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op3-field").is_some(),
        "the complete grid and field arrive together"
    );
}

#[gpui::test]
fn m9_permissions_dual_routes_only_the_captured_panes_requests_and_replies(
    cx: &mut TestAppContext,
) {
    let (handle, mut left) = setup(cx);
    let (backend, mut right) = crate::backend::Backend::test_lines();
    edit(cx, handle, |shell, _, cx| {
        let mut sibling = crate::state::tests::shell_with(backend, cx);
        let mut host = sibling.panes[0].take().unwrap();
        host.pane.held = shell.panes[0].as_ref().unwrap().pane.held.clone();
        shell.panes[1] = Some(host);
        shell.lifecycle.attach(1);
        shell.view = crate::window::startup::View::Dual;
        shell.focus_side = 1;
    });
    choose_on(cx, handle, &mut right, 1);
    let inspect = next(&mut right, "permissions", "inspect");
    assert!(inspect.contains(r#""path":"/files/notes.txt""#));
    let inspected = r#"{"t":"permissions","id":1,"op":"inspect","ok":true,"path":"/files/notes.txt","mode":"0644","reason":""}"#;
    receive_on(cx, handle, 0, inspected);
    assert!(
        VisualTestContext::from_window(handle.into(), cx)
            .debug_bounds("op3-field")
            .is_none(),
        "a matching id from the other pane cannot complete the card"
    );
    receive_on(cx, handle, 1, inspected);
    click(cx, handle, "op3-check-2");
    click(cx, handle, "op3-button-1");
    assert!(next(&mut right, "permissions", "apply").contains(r#""mode":"0744""#));
    let applied = r#"{"t":"permissions","id":1,"op":"apply","ok":true}"#;
    receive_on(cx, handle, 0, applied);
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, crate::backend::Event::Drained, window, cx)
    });
    assert!(
        edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)),
        "the other pane cannot close the card"
    );
    receive_on(cx, handle, 1, applied);
    cx.run_until_parked();
    assert!(!edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
    assert!(next(&mut right, "permissions", "close").contains(r#""id":1"#));
    assert!(
        !std::iter::from_fn(&mut left).any(|line| line.contains(r#""c":"permissions""#)),
        "no permission request went to pane 0"
    );
}

#[test]
fn m9_permissions_button_space_and_octal_return_keep_the_shipped_control_contract() {
    use crate::g2::op3::{
        keys::Key,
        permissions::{Permissions, Stop},
        Effect,
    };
    fn ready() -> Permissions {
        let (mut card, _) = Permissions::open(4, vec!["/files/notes.txt".into()], "");
        card.reply(&crate::test_catalogs::english(), &flea::jsondoc::parse(
            r#"{"id":4,"op":"inspect","ok":true,"path":"/files/notes.txt","mode":"0644","reason":""}"#,
        ).unwrap());
        card
    }
    assert!(
        ready()
            .key(Key::Space)
            .iter()
            .any(|effect| matches!(effect, Effect::Send(line) if line.contains(r#""op":"apply""#))),
        "Space activates the focused Apply button"
    );
    let mut card = ready();
    card.focus = Stop::Octal;
    assert!(
        card.key(Key::Enter).is_empty(),
        "Octal Return moves focus before any chmod is issued"
    );
    assert_eq!(card.focus, Stop::Apply);
    card.focus = Stop::Box(0);
    assert!(
        card.key(Key::Enter).is_empty(),
        "a checkbox changes with Space, not Return"
    );
    card.focus = Stop::Cancel;
    assert!(
        card.key(Key::Space).contains(&Effect::Close),
        "Space activates Cancel too"
    );
    card.stopped();
    assert!(card.key(Key::Space).contains(&Effect::Close));
}

#[gpui::test]
fn m9_permissions_button_space_and_octal_editing_supports_selection_navigation_and_paste(
    cx: &mut TestAppContext,
) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    click(cx, handle, "op3-field");
    VisualTestContext::from_window(handle.into(), cx)
        .simulate_keystrokes("home right delete 7 end shift-left 5");
    assert_eq!(model(cx, handle).text, "0745");
    edit(cx, handle, |_, _, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("0750".into()));
    });
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("ctrl-a ctrl-v");
    click(cx, handle, "op3-button-1");
    assert!(next(&mut requests, "permissions", "apply").contains(r#""mode":"0750""#));
}

#[gpui::test]
fn m9_permissions_octal_paints_the_caret_and_selection_at_the_edited_position(
    cx: &mut TestAppContext,
) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    click(cx, handle, "op3-field");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("home");
    let start = visual
        .debug_bounds("op1-caret")
        .expect("visible octal caret");
    visual.simulate_keystrokes("end");
    let end = visual.debug_bounds("op1-caret").unwrap();
    assert!(end.origin.x > start.origin.x + gpui::px(20.));
    visual.simulate_keystrokes("shift-left");
    let selected = visual
        .debug_bounds("op1-selection")
        .expect("visible selected digit");
    assert!(selected.size.width > gpui::px(1.));
}

#[gpui::test]
fn m9_permissions_read_only_controls_stay_outside_the_keyboard_focus_ring(cx: &mut TestAppContext) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(
        cx,
        handle,
        1,
        "notes.txt",
        "0644",
        "Read-only: you are not the owner.",
    );
    assert_eq!(model(cx, handle).focus, Stop::Cancel);
    click(cx, handle, "op3-button-1");
    assert_eq!(model(cx, handle).focus, Stop::Cancel);
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("tab");
    assert_eq!(model(cx, handle).focus, Stop::Cancel);
    for key in ["left", "right"] {
        VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(key);
        assert_eq!(model(cx, handle).focus, Stop::Cancel);
    }
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("space");
    assert!(!edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
}

#[gpui::test]
fn m9_permissions_loading_does_not_arm_a_later_select_all(cx: &mut TestAppContext) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    for _ in 0..10 {
        visual.simulate_keystrokes("tab");
    }
    visual.simulate_keystrokes("ctrl-a");
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("5");
    click(cx, handle, "op3-button-1");
    assert!(next(&mut requests, "permissions", "apply").contains(r#""mode":"0644""#));
}
