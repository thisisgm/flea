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

#[gpui::test]
fn uish_misc_network_backtab_moves_inside_the_modal_card(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    let first = cx.update(|_, cx| card.read(cx).model.focus);
    let focused = cx.update(|window, cx| window.focused(cx));
    cx.simulate_keystrokes("tab");
    frame(cx);
    assert_ne!(cx.update(|_, cx| card.read(cx).model.focus), first);
    cx.simulate_keystrokes("backtab");
    frame(cx);
    cx.update(|window, cx| {
        let view = card.read(cx);
        assert!(view.model.opened, "Backtab must remain in the open card");
        assert_eq!(view.model.focus, first);
        assert_eq!(window.focused(cx), focused);
    });
}

#[gpui::test]
fn uish_misc_network_overlay_close_restores_origin_key_routing(cx: &mut TestAppContext) {
    for (browser, from_rail) in [(false, false), (false, true), (true, false), (true, true)] {
        let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
        let card = card_of(&shell, cx);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.ensure_network(window, cx);
                shell.rail_focus(from_rail, cx);
                shell.keyboard.context = if from_rail {
                    flea::keys::Context::Rail
                } else {
                    flea::keys::Context::Listing
                };
                window.focus(
                    if from_rail {
                        &shell.dual_focus.rail
                    } else {
                        &shell.focus
                    },
                    cx,
                );
            })
        });
        frame(cx);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                if browser {
                    let origin = shell.network_origin(0);
                    shell.network_service(
                        &Effect::Shares {
                            uri: "smb://stub/".into(),
                            label: "Stub".into(),
                            names: vec!["share".into()],
                            origin,
                        },
                        window,
                        cx,
                    );
                } else {
                    shell.network_add(0, window, cx);
                }
            })
        });
        frame(cx);
        cx.simulate_keystrokes("escape");
        frame(cx);
        shell.read_with(cx, |shell, cx| {
            assert!(!card.read(cx).model.opened);
            assert_eq!(
                shell.keyboard.context,
                if from_rail {
                    flea::keys::Context::Rail
                } else {
                    flea::keys::Context::Listing
                },
                "browser={browser} from_rail={from_rail}"
            );
            let join = shell.rail.as_ref().unwrap();
            assert_eq!(join.places.read(cx).model.borrow().focused, from_rail);
            assert!(!join.network.as_ref().unwrap().shares.read(cx).model.open);
        });
        if from_rail {
            cx.simulate_keystrokes("escape");
            frame(cx);
        }
        cx.update(|window, cx| {
            let shell = shell.read(cx);
            assert_eq!(shell.keyboard.context, flea::keys::Context::Listing);
            assert!(shell.focus.is_focused(window));
        });
    }
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
    assert!(
        cx.update(|window, cx| shell.read(cx).card_takes_text(&key, window, cx)),
        "the opened card holding focus takes the key"
    );
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
fn fix_nettype_no_host_list_opens_on_open_typing_or_tab(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = open(Rc::new(Recorder::default()), cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    frame(cx);
    let card = card_of(&shell, cx);
    assert!(cx.update(|_, cx| card.read(cx).model.opened));
    assert!(
        drain(&mut lines[0])
            .iter()
            .all(|line| !line.contains("nearby")),
        "opening the card starts no host discovery"
    );
    let geometry = cx.update(|_, cx| {
        let view = card.read(cx);
        let drawn = view.drawn.borrow();
        (drawn.origin, drawn.card)
    });
    cx.simulate_input("na");
    frame(cx);
    cx.simulate_keystrokes("down");
    frame(cx);
    let steps = cx.update(|_, cx| card.read(cx).model.ring().len());
    for _ in 0..steps {
        cx.simulate_keystrokes("tab");
        frame(cx);
        cx.update(|_, cx| {
            let view = card.read(cx);
            let drawn = view.drawn.borrow();
            assert_eq!(
                (drawn.origin, drawn.card),
                geometry,
                "every field keeps the card at its one height and place"
            );
        });
    }
    assert!(
        drain(&mut lines[0])
            .iter()
            .all(|line| !line.contains("nearby")),
        "typing, Down and Tab ask for no host discovery"
    );
    cx.update(|_, cx| {
        let view = card.read(cx);
        assert!(view.model.opened, "the card stays open");
        assert_eq!(&*view.model.form.host, "na", "typing lands in HOST alone");
    });
}

#[gpui::test]
fn fix_nettype_error_line_grows_the_card_and_keeps_chips_under_the_title(cx: &mut TestAppContext) {
    let (card, cx) = opened(cx);
    // A whole chip row needs the card's grown height: with Dropbox installed it fits 700 px, without its footer 760 px.
    for (dropbox, window_height, card_height) in [(true, 700., 649.), (false, 760., 728.)] {
        cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(window_height)));
        cx.update(|_, cx| card.update(cx, |view, _| view.model.dropbox_installed = dropbox));
        frame(cx);
        cx.simulate_keystrokes("enter");
        frame(cx);
        cx.update(|_, cx| {
            let view = card.read(cx);
            assert_eq!(view.model.status, Some("network.invalid_form"));
            assert_eq!(view.scroll, 0., "the error line never scrolls the body");
            let drawn = view.drawn.borrow();
            assert_eq!(drawn.card.map(|rect| rect.h), Some(card_height));
            let chips: Vec<_> = drawn
                .hits
                .iter()
                .filter(|(focus, _)| matches!(focus, dialog::Focus::Protocol(_)))
                .collect();
            assert_eq!(chips.len(), 5, "SMB, SFTP, FTPS, WebDAV and NFS");
            for (_, rect) in chips {
                assert_eq!(
                    (rect.y, rect.h),
                    (61., 30.),
                    "a whole chip row sits below the title divider at y 48"
                );
            }
        });
    }
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
