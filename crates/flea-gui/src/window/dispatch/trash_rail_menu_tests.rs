use super::{click, open, row, settle, unchanged};
use crate::{backend::Event, state::Shell, window::u07_tests::read};
use flea::ui_logic_pl4::rail::Kind;
use gpui::{
    point, px, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput,
    TestAppContext, VisualTestContext,
};

#[gpui::test]
fn a_stationary_right_click_paints_the_trash_sidebar_menu(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (shell, cx) = open(cx, true);
    let (_, at) = row(&shell, cx, Kind::Trash);
    let handle = cx.update(|window, _| window.window_handle());
    let native = cx.cx.test_window(handle);
    // Keep the click and its first draw in one update so the test executor
    // cannot run the foreground task before we inspect the first draw.
    let mut seen = cx.update(|window, cx| {
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position: at,
                button: MouseButton::Right,
                modifiers: Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                position: at,
                button: MouseButton::Right,
                modifiers: Modifiers::none(),
                click_count: 1,
            }),
            cx,
        );
        let seen = native.frame_wake_count();
        window.draw(cx).clear(cx);
        assert!(
            !shell.read(cx).menu_opened(),
            "the menu must open after the draw"
        );
        seen
    });
    cx.run_until_parked();
    // Deliver subsequent frames only if the platform was asked to draw them.
    for _ in 0..4 {
        let wakes = native.frame_wake_count();
        if wakes == seen {
            break;
        }
        seen = wakes;
        cx.simulate_frame_request(gpui::RequestFrameOptions::default());
        cx.run_until_parked();
    }
    let frame = shell.read_with(cx, |shell, _| {
        shell
            .menu
            .menu
            .borrow()
            .card
            .as_ref()
            .expect("Trash menu")
            .frame
    });
    let painted = cx.update(|window, _| {
        let scale = window.scale_factor();
        window.painted_quads().iter().any(|quad| {
            let bounds = quad.bounds;
            (bounds.origin.x.0 / scale - frame.x).abs() <= 1.
                && (bounds.origin.y.0 / scale - frame.y).abs() <= 1.
                && (bounds.size.width.0 / scale - frame.w).abs() <= 1.
                && (bounds.size.height.0 / scale - frame.h).abs() <= 1.
        })
    });
    assert!(
        painted,
        "the first click must schedule a frame that paints the menu"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn opened_menu(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open(cx, true);
    shell.update_in(cx, |shell, window, cx| {
        shell.mount_trash(window, cx);
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, _| pane.monitor_start = None);
        shell.rail_trash_count(1, cx);
    });
    settle(cx);
    let (_, at) = row(&shell, cx, Kind::Trash);
    click(cx, at, MouseButton::Right);
    (shell, cx)
}

fn click_empty(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let at = shell.read_with(cx, |shell, _| {
        let menu = shell.menu.menu.borrow();
        let card = menu.card.as_ref().expect("Trash rail menu");
        let row = card
            .entries
            .iter()
            .position(|entry| entry.action == "emptyTrash")
            .unwrap();
        point(
            px(card.frame.x + card.frame.w / 2.),
            px(card.row_screen_top(row) + 8.),
        )
    });
    click(cx, at, MouseButton::Left);
}

fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, op: &str, fields: &str) {
    shell.update_in(cx, |shell, window, cx| {
        let view = shell.trash.as_ref().unwrap().pane.read(cx).view();
        assert_eq!(view.pending_op, op);
        let id = view.request_id;
        shell.receive(
            0,
            Event::TrashLine(format!(
                r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true,{fields}}}"#
            )),
            window,
            cx,
        );
    });
    settle(cx);
}

#[gpui::test]
fn emptying_trash_from_the_sidebar_keeps_the_folder_visible(cx: &mut TestAppContext) {
    let (shell, cx) = opened_menu(cx);
    super::super::test_shell::sent();
    click_empty(&shell, cx);
    assert!(!shell.read_with(cx, |shell, cx| shell.trash_opened(cx)));
    unchanged(&shell, cx);
    let lines = super::super::test_shell::sent();
    assert!(lines
        .iter()
        .any(|line| line.contains(r#""op":"prepare""#) && line.contains(r#""emptyTrash":true"#)));
    assert!(!lines.iter().any(|line| line.contains(r#""op":"delete""#)));
    reply(
        &shell,
        cx,
        "prepare",
        r#""all":true,"token":5,"count":1,"bytes":13"#,
    );
    assert!(!shell.read_with(cx, |shell, cx| shell.trash_opened(cx)));
    click_card(&shell, cx, "danger");
    assert!(super::super::test_shell::sent()
        .iter()
        .any(|line| line.contains(r#""op":"delete""#) && line.contains(r#""token":5"#)));
    reply(&shell, cx, "delete", r#""done":1,"failed":0"#);
    assert!(!shell.read_with(cx, |shell, cx| shell.trash_opened(cx)));
    unchanged(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn click_card(shell: &Entity<Shell>, cx: &mut VisualTestContext, button: &str) {
    let centre = read(shell, cx, "trashControlCentre", &[button]);
    let mut position = centre
        .split_whitespace()
        .map(|part| part.parse::<f32>().unwrap());
    let at = point(
        px(position.next().expect("confirmation x")),
        px(position.next().expect("confirmation y")),
    );
    click(cx, at, MouseButton::Left);
}

#[gpui::test]
fn cancelling_sidebar_empty_trash_releases_the_review_without_deleting(cx: &mut TestAppContext) {
    let (shell, cx) = opened_menu(cx);
    click_empty(&shell, cx);
    reply(
        &shell,
        cx,
        "prepare",
        r#""all":true,"token":5,"count":1,"bytes":13"#,
    );
    super::super::test_shell::sent();
    click_card(&shell, cx, "cancel");
    let lines = super::super::test_shell::sent();
    assert!(lines.iter().any(|line| line.contains(r#""op":"cancel""#)));
    assert!(!lines.iter().any(|line| line.contains(r#""op":"delete""#)));
    reply(&shell, cx, "cancel", r#""cancelled":true"#);
    assert!(!shell.read_with(cx, |shell, cx| shell.trash_opened(cx)));
    unchanged(&shell, cx);
    let (_, at) = row(&shell, cx, Kind::Trash);
    click(cx, at, MouseButton::Right);
    click_empty(&shell, cx);
    reply(
        &shell,
        cx,
        "prepare",
        r#""all":true,"token":6,"count":1,"bytes":13"#,
    );
    assert!(!read(&shell, cx, "trashControlCentre", &["danger"]).is_empty());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
