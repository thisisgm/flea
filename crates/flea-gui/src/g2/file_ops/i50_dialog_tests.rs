//! Mounted operation inputs keep composition, dispatch and lifecycle ownership on their real hosts.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{
    AppContext, Context, Entity, EntityInputHandler, TestAppContext, VisualTestContext, Window,
};

type Lines = Box<dyn FnMut() -> Option<String>>;
fn ready(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Lines) {
    let (backend, lines) = Backend::test_operation_channel();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        let focus = shell.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root { shell }
    });
    let shell = root.read_with(cx, |r, _| r.shell.clone());
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.run_until_parked();
    (shell, cx, Box::new(lines))
}
fn input<R>(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    cx.update(|window, cx| shell.update(cx, |s, cx| f(s, window, cx)))
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
fn bulk(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    input(cx, shell, |s, w, cx| {
        s.bulk_rename_open(
            0,
            vec!["/files/猫.txt".into(), "/files/犬.txt".into()],
            w,
            cx,
        );
        w.focus(&s.focus, cx);
    });
    draw(cx);
}
fn mark(shell: &Entity<Shell>, cx: &mut VisualTestContext, text: &str) {
    input(cx, shell, |s, w, cx| {
        s.replace_and_mark_text_in_range(
            None,
            text,
            Some(text.encode_utf16().count()..text.encode_utf16().count()),
            w,
            cx,
        )
    });
    draw(cx);
}

#[gpui::test]
fn i50_dialog_field_composition_matrix(cx: &mut TestAppContext) {
    use crate::g2::archive_flash::{
        compress::{Compress, Stop},
        extract,
    };
    let (shell, cx, _lines) = ready(cx);
    for index in 0..4 {
        input(cx, &shell, |s, _, _| {
            s.rename.ime.close();
            if index < 3 {
                let (mut card, _) =
                    Compress::open(vec!["/files/a.txt".into()], "/files", "files", 3, &["7z"]);
                if index == 0 {
                    card.key(
                        &s.locale.catalog,
                        crate::g2::archive_flash::keys::Key::Backtab,
                    );
                }
                if index == 2 {
                    card.key(&s.locale.catalog, crate::g2::archive_flash::keys::Key::Tab);
                }
                s.operations.archive.compress = Some(card);
            } else {
                s.operations.archive.compress = None;
                s.operations
                    .archive
                    .extract
                    .open("/files/a.7z", "/files", "", &s.locale.catalog);
            }
        });
        draw(cx);
        // Every field but the archive name is a password, which keeps the input method off.
        let secret = index > 0;
        input(cx, &shell, |s, w, cx| {
            assert_eq!(s.accepts_text_input(w, cx), !secret)
        });
        mark(&shell, cx, "かな");
        if secret {
            input(cx, &shell, |s, w, cx| {
                assert_eq!(s.marked_text_range(w, cx), None);
                assert!(s.dialog_value().is_some_and(|(_, value)| value.is_empty()));
            });
        } else {
            input(cx, &shell, |s, w, cx| {
                assert_eq!(s.marked_text_range(w, cx), Some(0..2));
                let bounds = s.rename.ime.drawn.borrow().bounds.expect("field painted");
                let rect = s.bounds_for_range(0..2, bounds, w, cx).unwrap();
                assert!(rect.size.width > gpui::px(0.));
            });
            cx.simulate_keystrokes("enter");
            assert!(
                shell.read_with(cx, |s, _| s.operations.archive.card_open()),
                "candidate Enter closed the operation"
            );
        }
        input(cx, &shell, |s, w, cx| {
            s.replace_text_in_range(None, "名前", w, cx);
            assert_eq!(s.marked_text_range(w, cx), None);
            assert!(s
                .dialog_value()
                .is_some_and(|(_, value)| value.ends_with("名前")));
            if index == 3 {
                s.operations
                    .archive
                    .extract
                    .press(&s.locale.catalog, extract::Stop::Cancel);
            } else {
                s.operations
                    .archive
                    .compress
                    .as_mut()
                    .unwrap()
                    .press(&s.locale.catalog, Stop::Cancel);
            }
            s.sync_dialog_editor();
        });
        draw(cx);
    }
}

#[gpui::test]
fn i50_dialog_batch_pattern_unicode(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx);
    bulk(&shell, cx);
    mark(&shell, cx, "猫");
    cx.simulate_keystrokes("enter");
    assert!(
        !std::iter::from_fn(&mut lines).any(|line| line.contains("\"c\":\"bulkrename\"")),
        "candidate Enter dispatched an operation"
    );
    input(cx, &shell, |s, w, cx| {
        s.replace_text_in_range(None, "猫", w, cx)
    });
    cx.simulate_keystrokes("tab");
    draw(cx);
    mark(&shell, cx, "新");
    input(cx, &shell, |s, w, cx| {
        s.replace_text_in_range(None, "新", w, cx)
    });
    assert_eq!(
        shell.read_with(cx, |s, _| s
            .rename
            .bulk
            .as_ref()
            .unwrap()
            .card
            .names
            .clone()),
        ["新.txt", "犬.txt"]
    );
    cx.simulate_keystrokes("tab");
    draw(cx);
    mark(&shell, cx, "個別.txt");
    input(cx, &shell, |s, w, cx| {
        s.replace_text_in_range(None, "個別.txt", w, cx)
    });
    assert_eq!(
        shell.read_with(cx, |s, _| s.rename.bulk.as_ref().unwrap().card.names[0]
            .clone()),
        "個別.txt"
    );
}

#[gpui::test]
fn i50_dialog_tab_preedit_owner(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    bulk(&shell, cx);
    mark(&shell, cx, "猫");
    let previous = shell.read_with(cx, |s, _| s.rename.ime.entry.session());
    input(cx, &shell, |s, w, cx| {
        s.bulk_rename_key(
            &gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("tab").unwrap(),
                is_held: false,
                prefer_character_input: false,
            },
            w,
            cx,
        );
        s.replace_text_in_range(None, "late", w, cx);
        assert_ne!(s.rename.ime.entry.session(), previous);
        assert!(s.rename.bulk.as_ref().unwrap().card.replace.is_empty());
        assert!(s.selected_text_range(false, w, cx).is_none());
    });
    draw(cx);
    input(cx, &shell, |s, w, cx| {
        s.replace_text_in_range(None, "犬", w, cx)
    });
    assert_eq!(
        shell.read_with(cx, |s, _| s
            .rename
            .bulk
            .as_ref()
            .unwrap()
            .card
            .replace
            .clone()),
        "犬"
    );
}

#[gpui::test]
fn i50_dialog_closed_callback_refused(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    bulk(&shell, cx);
    mark(&shell, cx, "猫");
    input(cx, &shell, |s, w, cx| {
        s.rename.bulk = None;
        s.sync_dialog_editor();
        s.replace_text_in_range(None, "late", w, cx);
        s.replace_and_mark_text_in_range(None, "late", None, w, cx);
        assert!(s.selected_text_range(false, w, cx).is_none());
        assert!(s.rename.ime.entry.session().is_none());
    });
}

#[gpui::test]
fn i50_dialog_new_name_commit_once(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx);
    input(cx, &shell, |s, w, cx| {
        s.rename_open(0, 0, w, cx);
    });
    draw(cx);
    mark(&shell, cx, "新規");
    cx.simulate_keystrokes("enter");
    assert!(!std::iter::from_fn(&mut lines).any(|line| line.contains("\"c\":\"rename\"")));
    input(cx, &shell, |s, w, cx| {
        s.replace_text_in_range(None, "新規", w, cx)
    });
    cx.simulate_keystrokes("enter enter");
    let sent = std::iter::from_fn(&mut lines)
        .filter(|l| l.contains("\"c\":\"rename\""))
        .count();
    assert_eq!(sent, 1);
}

#[gpui::test]
fn i50_dialog_archive_printable_and_controls(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx);
    input(cx, &shell, |s, _, _| {
        s.operations
            .archive
            .extract
            .open("/files/a.7z", "/files", "", &s.locale.catalog);
    });
    draw(cx);
    cx.simulate_input("pw");
    assert_eq!(
        shell.read_with(cx, |s, _| s.rename.ime.entry.text.chars().count()),
        2,
        "the live platform handler receives printable text"
    );
    cx.simulate_keystrokes("left");
    assert_eq!(
        shell.read_with(cx, |s, _| s.rename.ime.entry.cursor()),
        1,
        "Left moves the caret without reaching listing navigation"
    );
    mark(&shell, cx, "猫");
    assert_eq!(
        shell.read_with(cx, |s, _| s.rename.ime.entry.text.chars().count()),
        2,
        "a password refuses composition"
    );
    assert!(!std::iter::from_fn(&mut lines).any(|line| line.contains("\"c\":\"archive\"")));
    cx.simulate_keystrokes("escape");
    assert!(
        shell.read_with(cx, |s, _| !s.operations.archive.extract.opened()),
        "Escape closes only the card"
    );
}

#[gpui::test]
fn i50_dialog_batch_printable_text(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    bulk(&shell, cx);
    cx.simulate_input("猫");
    cx.simulate_keystrokes("tab");
    draw(cx);
    cx.simulate_input("新");
    assert_eq!(
        shell.read_with(cx, |s, _| s.rename.bulk.as_ref().unwrap().card.names[0]
            .clone()),
        "新.txt"
    );
}

#[gpui::test]
fn i50_dialog_password_reveal_and_retained_mask(cx: &mut TestAppContext) {
    use crate::g2::archive_flash::extract::Stop;
    let (shell, cx, _) = ready(cx);
    input(cx, &shell, |s, _, _| {
        s.operations
            .archive
            .extract
            .open("/files/a.7z", "/files", "", &s.locale.catalog);
    });
    draw(cx);
    cx.simulate_input("pw");
    input(cx, &shell, |s, _, _| {
        s.operations.archive.extract.eye_press()
    });
    draw(cx);
    input(cx, &shell, |s, w, cx| {
        let drawn = s.rename.ime.drawn.borrow();
        assert!(drawn.shaped_text.as_bytes() == b"pw");
        drop(drawn);
        let mut adjusted = None;
        assert!(s.text_for_range(0..2, &mut adjusted, w, cx).is_none());
        s.operations.archive.extract.eye_release();
        s.operations
            .archive
            .extract
            .press(&s.locale.catalog, Stop::Extract);
        s.operations.archive.extract.open(
            "/files/a.7z",
            "/files",
            "wrong-password",
            &s.locale.catalog,
        );
        s.sync_dialog_editor();
    });
    draw(cx);
    input(cx, &shell, |s, w, cx| {
        let drawn = s.rename.ime.drawn.borrow();
        assert_eq!(drawn.shaped_text.chars().count(), 2);
        assert!(drawn.shaped_text.chars().all(|c| c == '\u{2022}'));
        let field = drawn.bounds.unwrap();
        let width = drawn.line.run.as_ref().unwrap().line().width;
        drop(drawn);
        assert_eq!(
            s.bounds_for_range(0..0, field, w, cx).unwrap().left(),
            field.left() + width
        );
    });
    cx.simulate_input("x");
    assert_eq!(shell.read_with(cx, |s, _| s.rename.ime.entry.text.len()), 1);
}

#[path = "i50_owner_tests.rs"]
mod owner_tests;

#[path = "ime_fields_tests.rs"]
mod fields;
