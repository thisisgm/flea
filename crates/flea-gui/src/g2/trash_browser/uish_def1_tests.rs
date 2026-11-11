//! Window menus over Trash dismiss before the browser reads Escape.
use crate::{backend::Event, window::dispatch::test_shell::open_root};
use gpui::{point, px, MouseButton, TestAppContext};

#[gpui::test]
fn uish_def1_trash_rail_menu_keeps_the_browser_on_escape(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    shell.update_in(cx, |shell, window, cx| {
        shell.mount_trash(window, cx);
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, _| pane.monitor_start = None);
        assert!(shell.trash_action("openTrash", window, cx));
        let id = shell.trash.as_ref().unwrap().pane.read(cx).view().request_id;
        shell.receive(0, Event::TrashLine(format!(r#"{{"t":"trashbrowse","id":{id},"op":"list","ok":true,"total":0,"start":0,"present":[],"rows":[]}}"#)), window, cx);
    });
    for _ in 0..3 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
    let at = shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        let laid = producer.laid.borrow();
        let row = laid.rows.iter().find(|row| row.label == "Trash").unwrap();
        point(
            px(row.label_rect.x + 4.),
            px(row.rect.y + row.rect.height / 2.),
        )
    });
    cx.simulate_mouse_down(at, MouseButton::Right, Default::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Default::default());
    shell.read_with(cx, |shell, cx| {
        assert!(shell.menu_opened());
        assert!(shell.trash_opened(cx));
    });
    cx.simulate_keystrokes("escape");
    shell.read_with(cx, |shell, cx| {
        assert!(!shell.menu_opened(), "Escape dismisses the top menu");
        assert!(
            shell.trash_opened(cx),
            "Escape leaves the Trash browser open"
        );
    });
    cx.simulate_keystrokes("backspace");
    shell.read_with(cx, |shell, cx| {
        assert!(
            !shell.trash_opened(cx),
            "Backspace still leaves the browser"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
