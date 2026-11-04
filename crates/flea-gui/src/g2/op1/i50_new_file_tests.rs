//! New File's name composes through the card's mounted entry: candidate Enter never creates, and the committed name is sent once.
use super::{
    atomic_tests::open_with,
    host::Overlay,
    model::{CopyFocus, Dialog},
};
use crate::{backend::Backend, window::root::Root};
use gpui::{
    AppContext, Context, Entity, EntityInputHandler, TestAppContext, VisualTestContext, Window,
    WindowHandle,
};

// New File on the window's operations overlay, opened and painted the way the folder menu's choice does it.
fn opened(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<Root>,
    Entity<Overlay>,
    impl FnMut() -> Option<String>,
) {
    let (backend, received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    let overlay = handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let view = shell.operation_overlay(0, window, cx).expect("a pane");
                view.update(cx, |overlay, cx| overlay.new_file(81, "/files", cx));
                shell.show_operation(&view, cx);
                view
            })
        })
        .unwrap();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    (handle, overlay, received)
}

fn input<R>(
    visual: &mut VisualTestContext,
    view: &Entity<Overlay>,
    update: impl FnOnce(&mut Overlay, &mut Window, &mut Context<Overlay>) -> R,
) -> R {
    let result = visual.update(|window, cx| view.update(cx, |view, cx| update(view, window, cx)));
    visual.run_until_parked();
    result
}

fn draw(visual: &mut VisualTestContext) {
    visual.update(|window, cx| window.draw(cx).clear(cx));
}

// The card's name, focus and busy flag, or None once the card has closed.
fn named(view: &Entity<Overlay>, cx: &VisualTestContext) -> Option<(String, CopyFocus, bool)> {
    view.read_with(cx, |view, _| match &view.model.dialog {
        Some(Dialog::NewFile {
            name, focus, busy, ..
        }) => Some((name.clone(), *focus, *busy)),
        _ => None,
    })
}

// Sample input: {"c":"newfile","op":"newFile","id":81,"path":"/files","name":"notes.txt"}
fn created(received: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    std::iter::from_fn(received)
        .filter(|line| line.contains(r#""c":"newfile""#))
        .map(|line| {
            let doc = flea::jsondoc::parse(&line).unwrap();
            doc.get("name")
                .and_then(flea::jsondoc::Json::as_str)
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[gpui::test]
fn i50_new_file_name_composes_and_creates_the_committed_name_once(cx: &mut TestAppContext) {
    let (handle, view, mut received) = opened(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    input(&mut visual, &view, |view, window, cx| {
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            0..8,
            "the default name opens selected"
        );
        view.replace_and_mark_text_in_range(None, "\u{3081}\u{3082}", Some(2..2), window, cx);
        assert_eq!(view.marked_text_range(window, cx), Some(0..2));
    });
    visual.simulate_keystrokes("enter");
    assert_eq!(
        named(&view, &visual),
        Some(("\u{3081}\u{3082}".into(), CopyFocus::Field, false)),
        "candidate Enter creates nothing"
    );
    visual.simulate_keystrokes("escape");
    assert_eq!(
        named(&view, &visual).map(|(name, ..)| name),
        Some(String::new()),
        "first Escape drops only the composition"
    );
    draw(&mut visual);
    input(&mut visual, &view, |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "\u{30e1}\u{30e2}", None, window, cx);
        view.replace_text_in_range(None, "\u{30e1}\u{30e2}", window, cx);
        view.replace_text_in_range(None, "\u{7}", window, cx);
        assert_eq!(view.marked_text_range(window, cx), None);
    });
    visual.simulate_input(".txt");
    assert!(created(&mut received).is_empty());
    visual.simulate_keystrokes("enter enter");
    assert_eq!(
        created(&mut received),
        ["\u{30e1}\u{30e2}.txt"],
        "Create sends the committed name once"
    );
}

#[gpui::test]
fn i50_new_file_late_callback_misses_a_closed_or_reopened_card(cx: &mut TestAppContext) {
    let (handle, view, mut received) = opened(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    input(&mut visual, &view, |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "\u{3081}", None, window, cx)
    });
    visual.simulate_keystrokes("escape escape");
    assert_eq!(named(&view, &visual), None, "second Escape closes the card");
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(None, "late", window, cx);
        assert!(view.selected_text_range(false, window, cx).is_none());
    });
    handle
        .update(&mut visual, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                view.update(cx, |overlay, cx| overlay.new_file(82, "/files", cx));
                shell.show_operation(&view, cx);
            });
            // Keep reopening and callbacks in one update so no frame registers the new session between them.
            view.update(cx, |view, cx| {
                view.sync_focus(window, cx);
                view.replace_text_in_range(None, "late", window, cx);
                view.replace_and_mark_text_in_range(None, "late", None, window, cx);
                assert!(view.selected_text_range(false, window, cx).is_none());
            });
        })
        .unwrap();
    assert_eq!(
        named(&view, &visual).map(|(name, ..)| name),
        Some("New File".into()),
        "the reopened card takes nothing before its field is painted"
    );
    draw(&mut visual);
    visual.simulate_input("a");
    assert_eq!(
        named(&view, &visual).map(|(name, ..)| name),
        Some("a".into())
    );
    assert!(created(&mut received).is_empty());
}
