use super::*;
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
};
use gpui::{point, px, Entity, Modifiers, TestAppContext, VisualTestContext};
use std::rc::Rc;

#[test]
fn u03_double_click_requires_the_first_taps_file_identity() {
    let mut input = super::super::PaneInput::default();
    let press = |name: &str, count| Press {
        row: 2,
        listing: 1,
        name: Some(name.into()),
        event: MouseDownEvent {
            click_count: count,
            ..Default::default()
        },
        dragged: false,
    };
    assert_eq!(input.tap_count(&press("old", 1)), 1);
    assert_eq!(input.tap_count(&press("other", 2)), 0);
    assert_eq!(input.tap_count(&press("other", 3)), 1);
    assert_eq!(input.tap_count(&press("other", 4)), 2);
}

// Pane.qml:74: the pane's singleClick is ViewState's openMode word, "single" or "double".
fn mounted_open_mode<'a>(
    cx: &'a mut TestAppContext,
    settings: &str,
) -> (
    Entity<Shell>,
    &'a mut VisualTestContext,
    impl FnMut() -> Option<(usize, usize)>,
) {
    let (backend, requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.launch.settings.data = flea::jsondoc::parse(settings).unwrap();
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    (shell, cx, requests)
}

// Row 2 becomes the folder "sub", and one lone left tap lands on its row.
fn tap_the_folder(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        let mut rows = host.pane.held.rows.clone();
        rows[2].dir = true;
        rows[2].name = "sub".into();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing,
            total: host.pane.held.total,
            start: host.pane.held.start,
            rows,
        });
        cx.notify();
    });
    cx.run_until_parked();
    let position = shell.read_with(cx, |shell, _| {
        let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
        point(
            frame.bounds.left() + px(60.),
            frame.bounds.top() + px(frame.row * 2.5),
        )
    });
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn open_mode_single_opens_a_folder_on_one_left_tap(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mounted_open_mode(cx, r#"{"openMode":"single"}"#);
    tap_the_folder(&shell, cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.place(),
            "/files/sub",
            "single click opens the folder"
        );
        assert!(
            host.selected.is_empty(),
            "the opened folder's own listing has no selection"
        );
    });
}

#[gpui::test]
fn open_mode_double_keeps_one_left_tap_a_selection(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mounted_open_mode(cx, r#"{"openMode":"double"}"#);
    tap_the_folder(&shell, cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.place(), "/files", "double click never opens once");
        assert_eq!(host.selected, [2]);
        assert_eq!(host.pane.cursor, 2);
    });
}

#[gpui::test]
fn a_missing_open_mode_key_defaults_to_double_and_only_selects(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mounted_open_mode(cx, "{}");
    tap_the_folder(&shell, cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.place(), "/files", "double click is the default");
        assert_eq!(host.selected, [2]);
        assert_eq!(host.pane.cursor, 2);
    });
}
