//! Escape through the window's key path closes the open card through op1; a cardless entity drops while the stroke still reaches the window.
use crate::{
    backend::Backend,
    g2::op1::press::Press,
    state::{tests::shell_with, Shell},
    window::{input::ActionEvent, root::Root},
};
use gpui::{
    AppContext, Bounds, Pixels, Quad, TestAppContext, VisualTestContext, Window, WindowHandle,
};
use std::{cell::RefCell, rc::Rc};

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

fn escape(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    let focus = shell(cx, handle, |shell, _, _| shell.focus.clone());
    cx.update_window(handle.into(), |_, window, cx| {
        window.focus(&focus, cx);
    })
    .expect("the window is open");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.simulate_keystrokes("escape");
}

#[gpui::test]
fn escape_closes_the_copy_card_through_its_close(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    let own = frame(cx, handle);
    open_copy_to(cx, handle);
    frame(cx, handle);
    assert!(
        marks(cx, handle, "op1-scrim").is_some(),
        "the card is open before Escape"
    );
    // Clone the entity before the window takes it, so the close line stays readable after the drop.
    let overlay = shell(cx, handle, |shell, _, _| {
        shell.overlay.clone().expect("an open overlay")
    });
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
    let (outbox, open) = shell(cx, handle, |_, _, cx| {
        overlay.read_with(cx, |card, _| (card.model.outbox.clone(), card.model.open()))
    });
    assert!(!open, "Escape closes the copy card");
    assert!(
        outbox
            .iter()
            .any(|line| line.contains("\"menuaction\"") && line.contains("\"op\":\"close\"")),
        "Escape queues the card's menu_close, not a bare drop: {outbox:?}"
    );
}

#[gpui::test]
fn escape_after_a_mouse_cancel_leaves_no_overlay(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    frame(cx, handle);
    let entity = handle
        .update(cx, |root, _, _| root.shell.clone())
        .expect("the window is open");
    let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let log = seen.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&entity, move |_, event: &ActionEvent, _| {
            log.borrow_mut().push(event.action)
        })
    });
    escape(cx, handle);
    let plain = seen
        .borrow()
        .iter()
        .filter(|action| **action == "escape")
        .count();
    assert!(plain > 0, "a plain Escape reaches the window's action");
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
    escape(cx, handle);
    frame(cx, handle);
    assert!(
        !shell(cx, handle, |shell, _, _| shell.overlay.is_some()),
        "Escape drops the cardless entity instead of swallowing the key"
    );
    let total = seen
        .borrow()
        .iter()
        .filter(|action| **action == "escape")
        .count();
    assert_eq!(
        total - plain,
        plain,
        "an Escape after a mouse Cancel reaches the window as it does with no card"
    );
}

#[gpui::test]
fn escape_queues_the_cards_menu_close(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    frame(cx, handle);
    open_copy_to(cx, handle);
    frame(cx, handle);
    // Escape goes through the window branch; the clone keeps the card readable after the slot drops.
    let overlay = shell(cx, handle, |shell, _, _| {
        shell.overlay.clone().expect("an open overlay")
    });
    escape(cx, handle);
    frame(cx, handle);
    assert_eq!(
        marks(cx, handle, "op1-scrim"),
        None,
        "Escape closes the card"
    );
    let (outbox, open) = shell(cx, handle, |_, _, cx| {
        overlay.read_with(cx, |card, _| (card.model.outbox.clone(), card.model.open()))
    });
    assert!(!open, "Escape closes the copy card");
    assert!(
        outbox
            .iter()
            .any(|line| line.contains("\"menuaction\"") && line.contains("\"op\":\"close\"")),
        "Escape queues the card's menu_close, not a bare drop: {outbox:?}"
    );
}
