//! Mounted IME, pointer controls and replies use the destination's real entity and transport.
use super::{
    field_tests::{open, state},
    host::Overlay,
    model::{CopyFocus, Dialog},
};
use crate::{backend::Event, window::root::Root};
use gpui::{
    point, px, Context, Entity, EntityInputHandler, Modifiers, TestAppContext, VisualTestContext,
    Window, WindowHandle,
};

fn input<R>(
    visual: &mut VisualTestContext,
    view: &Entity<Overlay>,
    update: impl FnOnce(&mut Overlay, &mut Window, &mut Context<Overlay>) -> R,
) -> R {
    let result = visual.update(|window, cx| view.update(cx, |view, cx| update(view, window, cx)));
    visual.run_until_parked();
    result
}

fn reply(visual: &mut VisualTestContext, handle: WindowHandle<Root>, text: &str) {
    handle
        .update(visual, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.receive(0, Event::Operations(text.into()), window, cx)
            })
        })
        .unwrap();
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(visual: &mut VisualTestContext, selector: &'static str) {
    let bounds = visual
        .debug_bounds(selector)
        .expect("control must be mounted");
    visual.simulate_click(bounds.center(), Modifiers::none());
}

#[gpui::test]
fn u13_fix3_mounted_ime_uses_utf16_selection_and_replaces_marked_text(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    input(&mut visual, &view, |view, window, cx| {
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            0..6
        );
        view.replace_and_mark_text_in_range(None, "/\u{1d11e}n", Some(1..3), window, cx);
        assert_eq!(view.marked_text_range(window, cx), Some(0..4));
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            1..3
        );
        let mut adjusted = None;
        assert_eq!(
            view.text_for_range(1..3, &mut adjusted, window, cx)
                .as_deref(),
            Some("\u{1d11e}")
        );
        assert_eq!(adjusted, Some(1..3));
    });
    assert_eq!(state(&view, &visual).0, "/\u{1d11e}n");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    input(&mut visual, &view, |view, window, cx| {
        let slot = view.editor.drawn.borrow().bounds.unwrap();
        let band = view.bounds_for_range(1..3, slot, window, cx).unwrap();
        assert!(band.size.width > gpui::px(0.) && band.top() == slot.top());
        assert_eq!(
            view.character_index_for_point(band.origin, window, cx),
            Some(1)
        );
        view.replace_and_mark_text_in_range(None, "/\u{4f60}", Some(2..2), window, cx);
        assert_eq!(view.marked_text_range(window, cx), Some(0..2));
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            2..2
        );
    });
    visual.simulate_keystrokes("enter");
    assert!(
        !state(&view, &visual).2,
        "IME Enter must not submit marked text"
    );
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(None, "/\u{4f60}done", window, cx);
        assert_eq!(view.marked_text_range(window, cx), None);
        view.replace_text_in_range(Some(1..2), "x", window, cx);
    });
    assert_eq!(state(&view, &visual).0, "/xdone");
}

#[gpui::test]
fn u13_fix3_pointer_refocus_selection_and_clipboard_use_the_mounted_field(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("tab");
    assert_eq!(state(&view, &visual).1, CopyFocus::Cancel);
    visual.simulate_input("ignored");
    input(&mut visual, &view, |view, window, cx| {
        assert!(view.selected_text_range(false, window, cx).is_none());
        view.replace_text_in_range(None, "ignored", window, cx);
        view.replace_and_mark_text_in_range(None, "ignored", None, window, cx);
    });
    assert_eq!(state(&view, &visual).0, "/files");
    click(&mut visual, "op1-field");
    assert_eq!(state(&view, &visual).1, CopyFocus::Field);
    visual.simulate_keystrokes("ctrl-a ctrl-c end ctrl-v");
    assert_eq!(state(&view, &visual).0, "/files/files");
    visual.simulate_keystrokes("ctrl-a ctrl-x");
    assert_eq!(state(&view, &visual).0, "");
    visual.simulate_keystrokes("ctrl-v");
    assert_eq!(state(&view, &visual).0, "/files/files");
    let field = visual.debug_bounds("op1-destination").unwrap();
    visual.simulate_mouse_down(field.origin, gpui::MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(
        field.bottom_right(),
        Some(gpui::MouseButton::Left),
        Modifiers::none(),
    );
    visual.simulate_mouse_up(
        field.bottom_right(),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    visual.simulate_input("/replaced");
    assert_eq!(state(&view, &visual).0, "/replaced");
}

#[gpui::test]
fn u13_fix3_disabled_and_busy_controls_never_submit_or_edit(cx: &mut TestAppContext) {
    let (handle, view, mut received) = open(cx, "");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "op1-button-1");
    assert!(received().is_none());
    assert_eq!(state(&view, &visual), ("".into(), CopyFocus::Field, false));
    visual.simulate_input("/");
    click(&mut visual, "op1-button-1");
    assert_eq!(
        received().as_deref(),
        Some(r#"{"c":"menuaction","op":"validate","id":71,"action":"copyTo","dest":"/"}"#)
    );
    assert_eq!(state(&view, &visual), ("/".into(), CopyFocus::Cancel, true));
    click(&mut visual, "op1-button-1");
    visual.simulate_keystrokes("tab shift-tab");
    assert_eq!(state(&view, &visual).1, CopyFocus::Cancel);
    visual.simulate_input("changed");
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(None, "changed", window, cx)
    });
    assert_eq!(state(&view, &visual).0, "/");
    assert!(received().is_none());
    reply(
        &mut visual,
        handle,
        r#"{"t":"menuaction","op":"validate","id":72,"ok":false,"error":"foreign"}"#,
    );
    assert!(state(&view, &visual).2);
    reply(
        &mut visual,
        handle,
        r#"{"t":"menuaction","op":"validate","id":71,"ok":false,"error":"Destination refused."}"#,
    );
    assert_eq!(
        state(&view, &visual),
        ("/".into(), CopyFocus::Cancel, false)
    );
    assert!(view.read_with(&visual, |v, _| v.model.said.is_empty()));
    assert!(view.read_with(&visual, |v, _| matches!(
        &v.model.dialog,
        Some(Dialog::CopyTo { error, .. }) if error == "Destination refused."
    )));
    click(&mut visual, "op1-button-0");
    assert!(!view.read_with(&visual, |v, _| v.model.open()));
    assert_eq!(
        received().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":71}"#)
    );
    input(&mut visual, &view, |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "late", None, window, cx)
    });
    assert!(!view.read_with(&visual, |v, _| v.model.open()));
}

#[gpui::test]
fn i50_ime_utf16_surrogate(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(None, "/\u{1d11e}n", window, cx);
        let mut adjusted = None;
        // Half of a surrogate pair widens to its whole character instead of reading nothing.
        assert_eq!(
            view.text_for_range(2..3, &mut adjusted, window, cx)
                .as_deref(),
            Some("\u{1d11e}")
        );
        assert_eq!(adjusted, Some(1..3));
        view.replace_text_in_range(Some(2..3), "x", window, cx);
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            2..2
        );
        view.replace_and_mark_text_in_range(None, "\u{1d11e}", Some(1..1), window, cx);
        assert_eq!(view.marked_text_range(window, cx), Some(2..4));
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            4..4
        );
    });
    assert_eq!(state(&view, &visual).0, "/x\u{1d11e}n");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    input(&mut visual, &view, |view, window, cx| {
        let slot = view.editor.drawn.borrow().bounds.unwrap();
        let whole = view.bounds_for_range(2..4, slot, window, cx).unwrap();
        assert!(whole.size.width > px(0.));
        assert_eq!(view.bounds_for_range(3..4, slot, window, cx), Some(whole));
        let inside = point(whole.left() + px(0.5), whole.center().y);
        assert_eq!(view.character_index_for_point(inside, window, cx), Some(2));
    });
}

#[gpui::test]
fn i50_ime_combining_selection(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let selected = |visual: &mut VisualTestContext| {
        input(visual, &view, |view, window, cx| {
            view.selected_text_range(false, window, cx).unwrap().range
        })
    };
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(None, "/e\u{301}x", window, cx)
    });
    visual.simulate_keystrokes("end left left");
    assert_eq!(
        selected(&mut visual),
        1..1,
        "a base and its mark step together"
    );
    visual.simulate_keystrokes("shift-right");
    assert_eq!(selected(&mut visual), 1..3);
    visual.simulate_keystrokes("left delete");
    assert_eq!(state(&view, &visual).0, "/x");
    // The input method may address one mark on its own, as Vietnamese tone entry does.
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(Some(1..1), "a\u{302}", window, cx);
        view.replace_and_mark_text_in_range(Some(2..3), "\u{301}", Some(1..1), window, cx);
        assert_eq!(view.marked_text_range(window, cx), Some(2..3));
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            3..3
        );
        view.replace_text_in_range(None, "\u{301}", window, cx);
    });
    assert_eq!(state(&view, &visual).0, "/a\u{301}x");
    let mut entry = crate::path::entry::Entry::default();
    entry.begin("\u{d85}\u{dc3}\u{995}\u{9ce}\u{d9a}\u{dca}\u{dc2}");
    let steps = [0, 3, 6, 9].map(|at| entry.next(at));
    assert_eq!(
        steps,
        [3, 6, 9, 12],
        "letters at sign offsets stay their own stops"
    );
    assert_eq!(entry.next(12), 18, "al-lakuna alone ends its cluster");
    visual.simulate_keystrokes("backspace");
    assert_eq!(
        state(&view, &visual).0,
        "/ax",
        "Backspace takes one mark, as GTK does"
    );
}

#[gpui::test]
fn i50_ime_stale_field_generation(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    input(&mut visual, &view, |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "\u{30e1}", None, window, cx)
    });
    // A second card replaces the first, and late callbacks arrive before any frame registers its field.
    let overlay = view.clone();
    handle
        .update(&mut visual, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.copy_to_operation(0, 74, false, "/again", window, cx)
            });
            overlay.update(cx, |view, cx| {
                view.sync_focus(window, cx);
                view.replace_text_in_range(None, "\u{30e1}\u{30e2}", window, cx);
                view.replace_and_mark_text_in_range(None, "late", None, window, cx);
                assert_eq!(view.marked_text_range(window, cx), None);
                assert!(view.selected_text_range(false, window, cx).is_none());
            });
        })
        .unwrap();
    assert_eq!(state(&view, &visual).0, "/again");
    visual.update(|window, cx| window.draw(cx).clear(cx));
    input(&mut visual, &view, |view, window, cx| {
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            0..6
        );
    });
    visual.simulate_input("/new");
    assert_eq!(state(&view, &visual).0, "/new");
}

#[gpui::test]
fn i50_ime_enter_no_submit(cx: &mut TestAppContext) {
    let (handle, view, mut received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(None, "/dir", window, cx);
        view.replace_and_mark_text_in_range(None, "\u{3081}", None, window, cx);
    });
    visual.simulate_keystrokes("enter");
    assert_eq!(
        state(&view, &visual),
        ("/dir\u{3081}".into(), CopyFocus::Field, false)
    );
    assert!(received().is_none());
    // Escape drops the composition first and keeps the card and its committed draft.
    visual.simulate_keystrokes("escape");
    assert!(view.read_with(&visual, |v, _| v.model.open()));
    assert_eq!(state(&view, &visual).0, "/dir");
    input(&mut visual, &view, |view, window, cx| {
        assert_eq!(view.marked_text_range(window, cx), None)
    });
    assert!(received().is_none());
    // Tab finishes the composition as it stands, moves focus, and a late callback finds the field blurred.
    input(&mut visual, &view, |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "\u{3081}", None, window, cx)
    });
    visual.simulate_keystrokes("tab");
    input(&mut visual, &view, |view, window, cx| {
        view.replace_and_mark_text_in_range(None, "late", None, window, cx)
    });
    let committed = ("/dir\u{3081}".into(), CopyFocus::Cancel, false);
    assert_eq!(state(&view, &visual), committed);
    assert!(view.read_with(&visual, |v, _| v.editor.entry.marked.is_none()));
    visual.simulate_keystrokes("shift-tab escape");
    assert!(!view.read_with(&visual, |v, _| v.model.open()));
    assert_eq!(
        received().as_deref(),
        Some(r#"{"c":"menuaction","op":"close","id":71}"#)
    );
}

#[gpui::test]
fn i50_ime_preedit_underline(cx: &mut TestAppContext) {
    let (handle, view, _received) = open(cx, "/files");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(None, "/.txt", window, cx);
        view.replace_text_in_range(Some(1..1), "", window, cx);
        view.replace_and_mark_text_in_range(None, "\u{30e1}\u{30e2}", None, window, cx);
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let painted = |visual: &VisualTestContext| {
        view.read_with(visual, |view, _| {
            let drawn = view.editor.drawn.borrow();
            (drawn.paint.clone().unwrap(), drawn.line.span(&(1..7)))
        })
    };
    let (paint, (from, to)) = painted(&visual);
    let mark = paint.marked.expect("the preedit is underlined");
    assert_eq!(
        (mark.left(), mark.right()),
        (paint.origin + from, paint.origin + to)
    );
    assert_eq!(mark.size.height, px(flea::ui_logic_file_ops::card::RULE));
    assert!(mark.top() > paint.bounds.center().y && mark.bottom() <= paint.bounds.bottom());
    let caret = paint.caret.expect("the caret follows the preedit");
    assert_eq!(
        caret.left(),
        paint.origin + to,
        ".txt stays outside the underline"
    );
    input(&mut visual, &view, |view, window, cx| {
        view.replace_text_in_range(None, "\u{30e1}\u{30e2}", window, cx)
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        painted(&visual).0.marked.is_none(),
        "a committed draft draws no underline"
    );
    assert_eq!(state(&view, &visual).0, "/\u{30e1}\u{30e2}.txt");
}
