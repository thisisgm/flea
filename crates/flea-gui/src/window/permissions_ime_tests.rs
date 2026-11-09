//! The octal field keeps the input method off and takes digits only, and late callbacks miss.
use super::fixture::{choose, click, edit, inspect, model, next, setup};
use crate::{
    g2::open_with_properties::permissions::{host::View, Stop},
    window::root::Root,
};
use gpui::{Context, EntityInputHandler, TestAppContext, VisualTestContext, Window, WindowHandle};

fn ime<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    run: impl FnOnce(&mut View, &mut Window, &mut Context<View>) -> R,
) -> R {
    let view = edit(cx, handle, |shell, _, _| {
        shell.permissions.view.clone().expect("an open card")
    });
    let out = handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| run(view, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    out
}

fn draw(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    VisualTestContext::from_window(handle.into(), cx).update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn keys(cx: &mut TestAppContext, handle: WindowHandle<Root>, keys: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(keys);
}

fn typed(cx: &mut TestAppContext, handle: WindowHandle<Root>, text: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_input(text);
}

// One readable item with the octal field focused and painted.
fn octal(cx: &mut TestAppContext) -> (WindowHandle<Root>, impl FnMut() -> Option<String>) {
    let (handle, mut requests) = setup(cx);
    choose(cx, handle, &mut requests);
    next(&mut requests, "permissions", "inspect");
    inspect(cx, handle, 1, "notes.txt", "0644", "");
    click(cx, handle, "op3-field");
    draw(cx, handle);
    (handle, requests)
}

#[gpui::test]
fn i50_permissions_octal_keeps_the_input_method_off_and_takes_digits_only(cx: &mut TestAppContext) {
    let (handle, mut requests) = octal(cx);
    keys(cx, handle, "ctrl-a");
    typed(cx, handle, "07");
    assert_eq!(
        model(cx, handle).text,
        "07",
        "the field takes platform text"
    );
    ime(cx, handle, |view, window, cx| {
        assert!(
            !view.accepts_text_input(window, cx),
            "a number keeps the input method off"
        );
        view.replace_and_mark_text_in_range(None, "5", Some(1..1), window, cx);
        assert_eq!(view.marked_text_range(window, cx), None);
        view.replace_text_in_range(None, "日本語", window, cx);
        view.replace_text_in_range(None, "\u{7}", window, cx);
        assert_eq!(view.marked_text_range(window, cx), None);
    });
    assert_eq!(
        model(cx, handle).text,
        "07",
        "no composition and no CJK text"
    );
    typed(cx, handle, "55");
    assert_eq!(model(cx, handle).text, "0755");
    assert!(!std::iter::from_fn(&mut requests).any(|line| line.contains(r#""op":"apply""#)));
    keys(cx, handle, "enter enter");
    assert!(next(&mut requests, "permissions", "apply").contains(r#""mode":"0755""#));
}

#[gpui::test]
fn i50_permissions_octal_late_callbacks_miss_once_the_field_or_card_goes(cx: &mut TestAppContext) {
    let (handle, _requests) = octal(cx);
    let up = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("up").unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    ime(cx, handle, |view, window, cx| {
        view.model.text = "0700".into();
        view.key(&up, window, cx);
        view.replace_text_in_range(None, "1", window, cx);
    });
    assert_eq!(
        model(cx, handle).text,
        "0700",
        "a session restarted on a changed mode takes nothing before its field is painted"
    );
    draw(cx, handle);
    ime(cx, handle, |view, window, cx| {
        view.replace_text_in_range(None, "7", window, cx)
    });
    keys(cx, handle, "tab");
    assert_eq!(model(cx, handle).focus, Stop::Cancel);
    ime(cx, handle, |view, window, cx| {
        view.replace_text_in_range(None, "1", window, cx);
        assert!(view.selected_text_range(false, window, cx).is_none());
    });
    assert_eq!(
        model(cx, handle).text,
        "7",
        "Tab moved on and a late callback misses"
    );
    ime(cx, handle, |view, window, cx| {
        let backtab = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("shift-tab").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        // Refocus and start the new session before any frame can register its input handler.
        view.key(&backtab, window, cx);
        view.key(&up, window, cx);
        assert_eq!(view.model.focus, Stop::Octal);
        view.replace_text_in_range(None, "1", window, cx);
        assert!(view.selected_text_range(false, window, cx).is_none());
    });
    assert_eq!(
        model(cx, handle).text,
        "7",
        "a new session takes nothing before its field is painted"
    );
    draw(cx, handle);
    typed(cx, handle, "0755");
    assert_eq!(model(cx, handle).text, "0755");
    keys(cx, handle, "escape");
    assert!(!edit(cx, handle, |shell, _, cx| shell.card_holds_keyboard(cx)));
    ime(cx, handle, |view, window, cx| {
        view.replace_text_in_range(None, "1", window, cx);
        assert!(view.marked_text_range(window, cx).is_none());
    });
    assert_eq!(model(cx, handle).text, "0755");
}
