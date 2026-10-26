//! Typing inside the open Add a Network Share card reaches its fields through the whole window, not the card alone.
use super::*;

fn card_of(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<NetworkView> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .card
            .clone()
    })
}

fn opened_in(
    cx: &mut TestAppContext,
) -> (Entity<Shell>, Entity<NetworkView>, &mut VisualTestContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    let card = card_of(&shell, cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    frame(cx);
    assert!(cx.update(|_, cx| card.read(cx).model.opened));
    (shell, card, cx)
}

fn opened(cx: &mut TestAppContext) -> (Entity<NetworkView>, &mut VisualTestContext) {
    let (_, card, cx) = opened_in(cx);
    (card, cx)
}

fn printable(key: &str) -> gpui::KeyDownEvent {
    gpui::KeyDownEvent {
        keystroke: gpui::Keystroke {
            key: key.into(),
            key_char: Some(key.into()),
            ..Default::default()
        },
        is_held: false,
        prefer_character_input: false,
    }
}

#[gpui::test]
fn fix_nettype_a_key_is_card_text_only_while_the_card_holds_focus(cx: &mut TestAppContext) {
    let (shell, card, cx) = opened_in(cx);
    let key = printable("x");
    assert!(
        cx.update(|window, cx| card.read(cx).holds_focus(window, cx)),
        "the opened card takes focus"
    );
    assert!(cx.update(|window, cx| shell.read(cx).card_takes_text(&key, window, cx)));
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    frame(cx);
    assert!(cx.update(|_, cx| card.read(cx).model.opened));
    assert!(
        !cx.update(|window, cx| shell.read(cx).card_takes_text(&key, window, cx)),
        "with focus outside the card a typed key keeps the window's own handling"
    );
    cx.simulate_keystrokes("x");
    cx.simulate_input("y");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert!(view.model.opened);
        assert_eq!(
            &*view.model.form.host, "",
            "a key typed outside the card never lands in it"
        );
    });
}

#[gpui::test]
fn fix_nettype_a_closed_card_that_keeps_focus_takes_no_text(cx: &mut TestAppContext) {
    let (shell, card, cx) = opened_in(cx);
    let key = printable("x");
    // No frame after the close, so the last drawn tree still puts the window's focus inside the card.
    cx.update(|_, cx| card.update(cx, |card, _| drop(card.model.close())));
    assert!(!cx.update(|_, cx| card.read(cx).model.opened));
    assert!(
        cx.update(|window, cx| card.read(cx).holds_focus(window, cx)),
        "the closed card still holds focus"
    );
    assert!(
        !cx.update(|window, cx| shell.read(cx).card_takes_text(&key, window, cx)),
        "a closed card takes no typed text even while it holds focus"
    );
}

#[gpui::test]
fn fix_nettype_typed_text_lands_in_host_share_and_username(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    assert_eq!(
        cx.update(|_, cx| card.read(cx).model.focus),
        dialog::Focus::Host
    );
    for step in [
        "n", "a", "s", "tab", "tab", "m", "e", "d", "i", "a", "tab", "tab", "g", "m",
    ] {
        cx.simulate_keystrokes(step);
        frame(cx);
    }
    cx.update(|_, cx| {
        let form = &card.read(cx).model.form;
        assert_eq!(
            (&*form.host, &*form.path, &*form.user),
            ("nas", "media", "gm"),
            "typed text reaches HOST, SHARE and USERNAME"
        );
        assert_eq!(card.read(cx).model.focus, dialog::Focus::User);
    });
}

#[gpui::test]
fn fix_nettype_mounts_as_line_reads_the_typed_share(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    cx.simulate_input("nas");
    cx.simulate_keystrokes("tab tab");
    cx.simulate_input("media");
    cx.simulate_keystrokes("tab tab");
    cx.simulate_input("gm");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(view.model.form.uri().as_deref(), Some("smb://gm@nas/media"));
        let drawn = view.drawn.borrow();
        let line: Vec<_> = drawn
            .text
            .iter()
            .filter(|text| text.field.is_none() && text.rect.x == 17. && text.rect.y == 523.)
            .map(|text| text.source.to_string())
            .collect();
        assert_eq!(
            line,
            ["Mounts as smb://gm@nas/media"],
            "the Mounts as line under the fields carries the typed share"
        );
    });
}

#[gpui::test]
fn fix_nettype_space_lands_in_a_text_field(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    cx.simulate_keystrokes("tab tab");
    cx.simulate_input("my");
    cx.simulate_keystrokes("space");
    cx.simulate_input("files");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(view.model.focus, dialog::Focus::Path);
        assert_eq!(&*view.model.form.path, "my files");
    });
}

#[gpui::test]
fn fix_nettype_found_hosts_never_open_a_list_by_themselves(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    cx.update(|_, cx| {
        card.update(cx, |view, cx| {
            let id = view.model.discovery.id.unwrap();
            let rows = r#"{"name":"nas","source":"tailscale","uri":"smb://nas/media"}"#;
            let line = format!(
                r#"{{"t":"nearby","id":{id},"state":"found","rows":[{rows}],"errors":[]}}"#
            );
            assert!(view.line(&line, cx));
        })
    });
    frame(cx);
    assert!(
        !cx.update(|_, cx| card.read(cx).suggestions_open()),
        "no board draws a host list under HOST on open"
    );
    cx.simulate_input("na");
    frame(cx);
    assert!(
        !cx.update(|_, cx| card.read(cx).suggestions_open()),
        "typing a matching host opens no list"
    );
    cx.simulate_keystrokes("down");
    frame(cx);
    assert!(
        cx.update(|_, cx| card.read(cx).suggestions_open()),
        "Down in HOST asks for the found hosts"
    );
    cx.simulate_keystrokes("escape");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert!(!view.suggestions_open(), "Escape closes the list first");
        assert!(view.model.opened, "and leaves the card open");
    });
}

#[gpui::test]
fn fix_nettype_space_lands_in_the_password(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    for _ in 0..5 {
        if cx.update(|_, cx| card.read(cx).model.focus) == dialog::Focus::Password {
            break;
        }
        cx.simulate_keystrokes("tab");
        frame(cx);
    }
    assert_eq!(
        cx.update(|_, cx| card.read(cx).model.focus),
        dialog::Focus::Password
    );
    cx.simulate_input("a");
    cx.simulate_keystrokes("space");
    cx.simulate_input("b");
    frame(cx);
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert_eq!(view.model.focus, dialog::Focus::Password);
        assert_eq!(view.model.password.expose(), b"a b");
    });
}

#[gpui::test]
fn fix_nettype_space_on_a_button_presses_it(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    for _ in 0..16 {
        if cx.update(|_, cx| card.read(cx).model.focus) == dialog::Focus::Cancel {
            break;
        }
        cx.simulate_keystrokes("tab");
        frame(cx);
    }
    assert_eq!(
        cx.update(|_, cx| card.read(cx).model.focus),
        dialog::Focus::Cancel
    );
    cx.simulate_keystrokes("space");
    frame(cx);
    assert!(
        !cx.update(|_, cx| card.read(cx).model.opened),
        "space on Cancel closes the card"
    );
}
