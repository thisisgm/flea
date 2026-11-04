//! Escape through the window's key path closes the open card through op1; a cardless entity drops while the stroke still reaches the window.
use crate::{
    backend::Backend,
    g2::op1::press::Press,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{
    AppContext, Bounds, Pixels, Quad, TestAppContext, VisualTestContext, Window, WindowHandle,
};
use std::rc::Rc;

fn open(cx: &mut TestAppContext, backend: Backend) -> WindowHandle<Root> {
    cx.add_window(|_, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        Root { shell }
    })
}

fn shell<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    update: impl FnOnce(&mut Shell, &mut Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .clone()
                .update(cx, |shell, cx| update(shell, window, cx))
        })
        .expect("the window is open")
}

// The window's painted quads for one whole frame, after the root has drawn it.
fn frame(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> Vec<Quad> {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.painted_quads()
    })
    .expect("the window is open")
}

fn same_frame(before: &[Quad], after: &[Quad]) -> bool {
    format!("{before:?}") == format!("{after:?}")
}

// The laid-out bounds of one tagged part, read from the frame the window drew.
fn marks(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    selector: &'static str,
) -> Option<Bounds<Pixels>> {
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.debug_bounds(selector)
}

fn open_copy_to(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    shell(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/fx/v/work", window, cx);
    });
}

fn focus_shell(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    let focus = shell(cx, handle, |shell, _, _| shell.focus.clone());
    cx.update_window(handle.into(), |_, window, cx| {
        window.focus(&focus, cx);
    })
    .expect("the window is open");
}

fn escape(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    focus_shell(cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.simulate_keystrokes("escape");
}

#[gpui::test]
fn escape_closes_the_copy_card_through_its_close(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    // The list draws its focused cursor once the window has focus, so the own frame is taken focused.
    focus_shell(cx, handle);
    let own = frame(cx, handle);
    open_copy_to(cx, handle);
    frame(cx, handle);
    assert!(
        marks(cx, handle, "op1-scrim").is_some(),
        "the card is open before Escape"
    );
    // Retain the model to check closure, and observe the command on its owning backend.
    let overlay = shell(cx, handle, |shell, _, _| {
        shell.overlay.clone().expect("an open overlay")
    });
    while requests().is_some() {}
    escape(cx, handle);
    let back = frame(cx, handle);
    assert_eq!(
        marks(cx, handle, "op1-scrim"),
        None,
        "Escape closes the card"
    );
    assert!(
        same_frame(&back, &own),
        "the window draws its own frame after Escape"
    );
    let open = shell(cx, handle, |_, _, cx| overlay.read(cx).model.open());
    let outbox: Vec<_> = std::iter::from_fn(&mut requests).collect();
    assert!(!open, "Escape closes the copy card");
    assert!(
        outbox
            .iter()
            .any(|line| line.contains("\"menuaction\"") && line.contains("\"op\":\"close\"")),
        "Escape queues the card's menu_close, not a bare drop: {outbox:?}"
    );
    assert_eq!(outbox, [r#"{"c":"menuaction","op":"close","id":0}"#]);
}

#[gpui::test]
fn escape_after_a_mouse_cancel_leaves_no_overlay(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    frame(cx, handle);
    // The listing family runs a plain Escape: with no mark it clears the notice.
    let say = |cx: &mut TestAppContext| {
        shell(cx, handle, |shell, _, cx| {
            shell.status_messages(cx, |messages, now| {
                messages.say(
                    "Copied 1 item",
                    false,
                    "",
                    flea::ui_logic_fb1::status::Hint::None,
                    now,
                )
            })
        })
    };
    let notice = |cx: &mut TestAppContext| {
        shell(cx, handle, |shell, _, cx| {
            shell.status_messages(cx, |messages, _| messages.notice().to_owned())
        })
    };
    say(cx);
    escape(cx, handle);
    assert_eq!(notice(cx), "", "a plain Escape reaches the window's action");
    open_copy_to(cx, handle);
    frame(cx, handle);
    // A pointer press on Cancel is what a mouse Cancel does; the entity lingers with no card.
    shell(cx, handle, |shell, _, cx| {
        let overlay = shell.overlay.clone().expect("an open overlay");
        let catalog = overlay.read_with(cx, |card, _| card.catalog.clone());
        overlay.update(cx, |card, _| card.model.press(&catalog, Press::CopyCancel));
        cx.notify();
    });
    frame(cx, handle);
    assert_eq!(
        marks(cx, handle, "op1-scrim"),
        None,
        "no scrim draws with no card"
    );
    assert!(
        shell(cx, handle, |shell, _, _| shell.overlay.is_some()),
        "the entity lingers after a mouse Cancel"
    );
    say(cx);
    escape(cx, handle);
    frame(cx, handle);
    assert!(
        !shell(cx, handle, |shell, _, _| shell.overlay.is_some()),
        "Escape drops the cardless entity instead of swallowing the key"
    );
    assert_eq!(
        notice(cx),
        "",
        "an Escape after a mouse Cancel reaches the window as it does with no card"
    );
}

#[gpui::test]
fn escape_queues_the_cards_menu_close(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    frame(cx, handle);
    open_copy_to(cx, handle);
    frame(cx, handle);
    // Retain the model while Escape delivers the close through the owning backend.
    let overlay = shell(cx, handle, |shell, _, _| {
        shell.overlay.clone().expect("an open overlay")
    });
    while requests().is_some() {}
    escape(cx, handle);
    frame(cx, handle);
    assert_eq!(
        marks(cx, handle, "op1-scrim"),
        None,
        "Escape closes the card"
    );
    let open = shell(cx, handle, |_, _, cx| overlay.read(cx).model.open());
    let outbox: Vec<_> = std::iter::from_fn(&mut requests).collect();
    assert!(!open, "Escape closes the copy card");
    assert!(
        outbox
            .iter()
            .any(|line| line.contains("\"menuaction\"") && line.contains("\"op\":\"close\"")),
        "Escape queues the card's menu_close, not a bare drop: {outbox:?}"
    );
    assert_eq!(outbox, [r#"{"c":"menuaction","op":"close","id":0}"#]);
}

// Exercise the actual menu producer, which obtains its pane-bound operation session.
fn chosen_copy_to(cx: &mut TestAppContext, handle: WindowHandle<Root>, side: usize, id: u64) {
    use crate::g2::mn1::join::{Captured, Chosen};
    shell(cx, handle, |shell, _, cx| {
        shell.focus_pane(side, cx);
        let host = shell.panes[side].as_ref().unwrap();
        cx.emit(Chosen {
            captured: Captured {
                side,
                listing: host.pane.held.listing,
                rows: vec![0],
                cursor: 0,
                path: host.path.clone(),
                identity: String::new(),
                id,
                ready: true,
                kinds: vec![0],
                background: false,
                target: None,
            },
            action: "copyTo".into(),
            leaf: String::new(),
            paths: vec![format!("{}/a.txt", host.path)],
            claimed: Rc::new(std::cell::Cell::new(false)),
        });
    });
    cx.run_until_parked();
    frame(cx, handle);
}

#[gpui::test]
fn g70_menu_copy_keeps_its_owner_after_other_backend_retires(cx: &mut TestAppContext) {
    use crate::{
        backend::{Event, Failure},
        g2::op1::model::Dialog,
        window::{pane::PaneHost, startup::View},
    };
    for dead in [0, 1] {
        for failed in [false, true] {
            let (backend, mut first_lines) = Backend::test_lines();
            let (other_backend, mut second_lines) = Backend::test_lines();
            let handle = open(cx, backend);
            shell(cx, handle, |shell, window, cx| {
                let mut other = PaneHost::new("/other".into(), None, 620., &shell.locale);
                other.backend = Some(other_backend);
                other.pane.use_executor(cx.background_executor().clone());
                other
                    .pane
                    .receive(flea::protocol::Reply::Listed {
                        path: "/other".into(),
                        total: 1,
                    })
                    .unwrap();
                other
                    .pane
                    .receive(flea::protocol::Reply::Rows {
                        start: 0,
                        rows: vec![Default::default()],
                        listing: 3,
                    })
                    .unwrap();
                shell.panes[1] = Some(other);
                shell.lifecycle.attach(1);
                shell.set_view(View::Dual, window, cx);
            });
            focus_shell(cx, handle);
            frame(cx, handle);
            chosen_copy_to(cx, handle, dead, 71);
            let old = shell(cx, handle, |shell, _, _| {
                shell.operation_view(dead).unwrap()
            });
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let cancel = visual.debug_bounds("op1-button-0").unwrap().center();
            visual.simulate_click(cancel, gpui::Modifiers::none());
            visual.run_until_parked();
            frame(cx, handle);
            shell(cx, handle, |shell, _, cx| {
                assert!(!old.read(cx).model.open());
                assert_eq!(
                    shell.overlay.as_ref(),
                    Some(&old),
                    "mouse Cancel retains the mount"
                );
            });
            let survivor = 1 - dead;
            chosen_copy_to(cx, handle, survivor, 72);
            let live = shell(cx, handle, |shell, _, cx| {
                let live = shell.operation_view(survivor).unwrap();
                assert_ne!(old, live, "menu sessions have distinct entities");
                assert_eq!(old.read(cx).owner, Some(dead));
                assert_eq!(live.read(cx).owner, Some(survivor));
                assert_eq!(shell.overlay.as_ref(), Some(&live));
                live
            });
            // A real mounted submission leaves a validation pending on the surviving backend.
            while first_lines().is_some() {}
            while second_lines().is_some() {}
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            visual.simulate_input("/destination");
            visual.simulate_keystrokes("enter");
            visual.run_until_parked();
            frame(cx, handle);
            let first: Vec<_> = std::iter::from_fn(&mut first_lines).collect();
            let second: Vec<_> = std::iter::from_fn(&mut second_lines).collect();
            let validate =
                |line: &String| line.contains("\"op\":\"validate\"") && line.contains("\"id\":72");
            assert_eq!(first.iter().any(validate), survivor == 0);
            assert_eq!(second.iter().any(validate), survivor == 1);
            let pending = shell(cx, handle, |_, _, cx| live.read(cx).model.dialog.clone());
            assert!(
                matches!(&pending, Some(Dialog::CopyTo { menu: 72, path, busy: true, .. })
                if path == "/destination")
            );
            shell(cx, handle, |shell, window, cx| {
                let event = if failed {
                    Event::Failed(Failure {
                        key: "window.backend_eof",
                        detail: String::new(),
                    })
                } else {
                    Event::Drained
                };
                shell.receive(dead, event, window, cx);
                if failed {
                    shell.receive(dead, Event::Drained, window, cx);
                }
            });
            frame(cx, handle);
            assert!(marks(cx, handle, "op1-scrim").is_some());
            shell(cx, handle, |shell, window, cx| {
                assert_eq!(shell.overlay.as_ref(), Some(&live));
                assert_eq!(shell.operation_view(dead).as_ref(), Some(&old));
                assert_eq!(live.read(cx).model.dialog, pending);
                assert_eq!(live.read(cx).owner, Some(survivor));
                assert!(live.read(cx).focus_handle().unwrap().is_focused(window));
                assert!(!old.read(cx).model.open());
            });
        }
    }
}

#[path = "overlay_owner_tests.rs"]
mod owners;
