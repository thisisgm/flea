//! The status line's "z undoes" target runs the undo the z key runs, through the window's own pointer dispatch.
use crate::{backend::Backend, state::tests::shell_with, window::root::Root};
use flea::ui_logic_status_feedback::status::Hint;
use gpui::{AppContext, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle};

// A ready pane with settled footer facts, as the operation routing tests open it.
fn open(cx: &mut TestAppContext) -> (WindowHandle<Root>, impl FnMut() -> Vec<String>) {
    let (backend, mut lines) = Backend::test_operation_channel();
    let handle = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"preview":{"thumbnails":"off"}}"#).unwrap();
            let host = shell.panes[0].as_mut().unwrap();
            host.sizes = crate::list::sizes::State::settled_fixture(
                std::rc::Rc::clone(&host.pane.held),
                host.path.clone(),
                "",
            );
            let facts = crate::backend::status::Filesystem {
                class: String::new(),
                tagable: None,
                path: host.path.clone(),
                name: "testfs".into(),
                free: Some(1_073_741_824),
            };
            shell.surfaces[0].filesystem =
                crate::window::status::Filesystem::settled_fixture(facts, host.pane.held.listing);
            shell
        });
        let focus = shell.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root { shell }
    });
    draw(cx, handle);
    (handle, move || std::iter::from_fn(&mut lines).collect())
}

fn draw(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn say_undoable(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.status_messages(cx, |messages, now| {
                    messages.say("Moved 1 item to Trash", false, "", Hint::Undo, now)
                })
            })
        })
        .unwrap();
    draw(cx, handle);
    draw(cx, handle);
}

#[gpui::test]
fn a_click_on_z_undoes_sends_the_undo_the_z_key_sends(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    lines();
    say_undoable(cx, handle);
    {
        let mut window = VisualTestContext::from_window(handle.into(), cx);
        window.simulate_keystrokes("z");
        window.run_until_parked();
    }
    let keyed = lines();
    assert_eq!(keyed, [r#"{"c":"undo"}"#], "the z key's undo");
    say_undoable(cx, handle);
    {
        let mut window = VisualTestContext::from_window(handle.into(), cx);
        let target = window
            .debug_bounds("fb1-undo")
            .expect("the status line draws z undoes");
        let at = target.center();
        window.simulate_mouse_move(at, None, Modifiers::none());
        window.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        window.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        window.run_until_parked();
    }
    assert_eq!(lines(), keyed, "STATUS_UNDO_CLICK_MUST_RUN_THE_Z_UNDO");
}
