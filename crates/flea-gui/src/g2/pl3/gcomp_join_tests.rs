//! The Trash message producer shares the real footer slot without taking the ordinary pane's queue.
use super::join_tests::{call, draw, opened, root, shell};
use flea::ui_logic_fb1::status::Hint;

#[test]
fn feedback_remainder_ey21_trash_text_size_chords_paint_each_notice() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, window, cx| {
        let (theme, parsed) = crate::palette::Theme::from_text_observed("");
        shell.apply_presentation(
            crate::presentation::Snapshot {
                theme,
                parsed,
                fonts: crate::presentation::Fonts::from_sources(
                    "[font]\nbase-size=14",
                    "",
                    "monospace".into(),
                ),
            },
            window,
            cx,
        );
        shell.status_note("ordinary retained message", cx);
    });
    opened(&mut cx, window);
    for (stroke, expected) in [
        ("ctrl-shift-+", "Text size 16px."),
        ("ctrl-shift-+", "Text size 20px."),
        ("ctrl-shift--", "Text size 16px."),
        ("ctrl-shift-0", "Text size follows Omarchy, 14px"),
    ] {
        cx.simulate_keystrokes(window.into(), stroke);
        draw(&mut cx, window);
        let message = call(&mut cx, window, "lastMessage", &[]);
        eprintln!("EY21 Trash {stroke}: {message:?}");
        assert!(message.starts_with(expected), "{stroke}: {message:?}");
        shell(&mut cx, window, |shell, _, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            let displayed = footer.message_line();
            let line = displayed.read(cx);
            let painted = line
                .painted
                .get("primary")
                .expect("Trash paints the notice");
            assert_eq!(painted.text.as_ref(), message);
            assert!(painted.bounds.size.width > gpui::px(0.));
            assert_eq!(
                footer.line.read(cx).drawn().primary.as_ref(),
                "ordinary retained message"
            );
        });
    }
    cx.simulate_keystrokes(window.into(), "backspace");
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "lastMessage", &[]),
        "ordinary retained message"
    );
}

#[test]
fn feedback_remainder_ey21_trash_shows_a_global_recent_refusal() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, _, cx| {
        shell.status_note("ordinary retained message", cx);
    });
    opened(&mut cx, window);
    let expected = shell(&mut cx, window, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().recent.on = true;
        assert!(shell.settings_action("toggleHidden", window, cx));
        shell.locale.catalog.tr("places.recent.hidden_refused")
    });
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "lastMessage", &[]), expected);
    shell(&mut cx, window, |shell, _, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        let displayed = footer.message_line();
        assert_eq!(
            displayed
                .read(cx)
                .painted
                .get("primary")
                .unwrap()
                .text
                .as_ref(),
            expected
        );
        assert_eq!(
            footer.line.read(cx).drawn().primary.as_ref(),
            "ordinary retained message"
        );
    });
}

#[test]
fn feedback_remainder_ey21_a_note_during_trash_close_uses_the_new_footer_owner() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        shell.drop_trash(0, window, cx);
        assert!(!shell.trash_opened(cx));
        shell.status_note("ordinary note after closing Trash", cx);
    });
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "lastMessage", &[]),
        "ordinary note after closing Trash"
    );
    shell(&mut cx, window, |shell, _, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        assert_eq!(footer.message_line(), footer.line);
    });
}

#[test]
fn gcomp_batch_u32_arm_paints_in_the_real_footer_and_close_restores_its_prior_owner() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.say("ordinary retained message", false, "", Hint::None, now)
        });
    });
    draw(&mut cx, window);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "d");
    draw(&mut cx, window);
    let prompt = call(&mut cx, window, "lastMessage", &[]);
    assert!(prompt.starts_with("Press d again"));
    shell(&mut cx, window, |shell, _, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        let displayed = footer.message_line();
        assert_eq!(displayed, shell.trash.as_ref().unwrap().messages);
        assert_ne!(
            displayed, footer.line,
            "the ordinary queue must keep its own owner"
        );
        let line = displayed.read(cx);
        let painted = line
            .painted
            .get("primary")
            .expect("the actual armed text painted in slots.status");
        assert_eq!(painted.text.as_ref(), prompt);
        assert!(painted.bounds.size.width > gpui::px(0.));
        assert_eq!(
            footer.line.read(cx).drawn().primary.as_ref(),
            "ordinary retained message"
        );
    });
    cx.simulate_keystrokes(window.into(), "backspace");
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "lastMessage", &[]),
        "ordinary retained message"
    );
    shell(&mut cx, window, |shell, _, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        assert_eq!(footer.message_line(), footer.line);
        assert!(footer.line.read(cx).painted.get("primary").is_some());
        assert!(shell.trash_message().is_none());
    });
}

#[test]
fn g70_footer_owner_switch_sizes_the_first_frame_from_the_operation_detail() {
    use crate::window::status::paint_tests::FooterPaints;
    use flea::ui_logic_op1::{confirm::Source, trash};
    use gpui::px;

    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, _, cx| {
        shell.panes[1] = Some(crate::window::pane::PaneHost::new(
            "/other".into(),
            None,
            620.,
            &shell.locale,
        ));
        shell.view = crate::window::startup::View::Dual;
        cx.notify();
    });
    draw(&mut cx, window);
    let plain_y = shell(&mut cx, window, |shell, window, _| {
        shell.geometry(window).status.y
    });
    shell(&mut cx, window, |shell, window, cx| {
        shell.confirm_operation(
            0,
            trash::Review {
                count: Some(2),
                token: "1".into(),
                ..Default::default()
            },
            trash::Scope::Items,
            Source::Menu(6),
            window,
            cx,
        );
        shell.operation_view(0).unwrap().update(cx, |overlay, cx| {
            overlay.model.key(&overlay.catalog, "tab", false);
            overlay.model.key(&overlay.catalog, "enter", false);
            cx.notify();
        });
        shell.flush_operations(0, cx);
        shell.receive(
            0,
            crate::backend::Event::Operations(
                r#"{"t":"menuaction","id":6,"op":"delete","ok":true,"deleted":1,"failed":1,"cancelled":false,"error":"First filesystem failure.\nSecond filesystem failure.","remaining":["/files/a"]}"#.into(),
            ),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "statusDetail", &[]),
        "First filesystem failure.\nSecond filesystem failure."
    );
    let detail_height = shell(&mut cx, window, |shell, _, cx| {
        let height = shell.surfaces[0].detail_height;
        assert!(height > 0.);
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        assert_eq!(
            footer.painted.get("frame").unwrap().bounds.origin.y,
            px(plain_y - height)
        );
        height
    });
    cx.update(|cx| cx.set_global(FooterPaints::default()));
    opened(&mut cx, window);
    let trash_owner = shell(&mut cx, window, |shell, _, _| {
        shell.trash.as_ref().unwrap().messages.entity_id()
    });
    cx.read(|cx| {
        let paints = cx.global::<FooterPaints>().0.borrow();
        let first = paints
            .iter()
            .find(|paint| paint.owner == trash_owner)
            .expect("opening Trash must paint its actual footer");
        assert!(first.detail.is_empty());
        assert_eq!(
            first.bounds.origin.y,
            px(plain_y),
            "first actual Trash footer paint must reserve only its own detail: {paints:?}"
        );
    });
    for side in [1, 0, 1] {
        let owner = shell(&mut cx, window, |shell, _, cx| {
            cx.global::<FooterPaints>().0.borrow_mut().clear();
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            let owner = if side == 0 {
                trash_owner
            } else {
                footer.line.entity_id()
            };
            shell.focus_pane(side, cx);
            owner
        });
        draw(&mut cx, window);
        cx.read(|cx| {
            let paints = cx.global::<FooterPaints>().0.borrow();
            let first = paints.first().expect("focus change must paint the footer");
            assert_eq!(first.owner, owner);
            let expected_detail = if side == 0 {
                ""
            } else {
                "First filesystem failure.\nSecond filesystem failure."
            };
            assert_eq!(first.detail.as_ref(), expected_detail);
            assert_eq!(
                first.bounds.origin.y,
                px(plain_y - if side == 0 { 0. } else { detail_height }),
                "first actual footer paint after focus must use current owner geometry: {paints:?}"
            );
        });
        shell(&mut cx, window, |shell, _, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            assert_eq!(
                footer.line.read(cx).drawn().detail.as_ref(),
                "First filesystem failure.\nSecond filesystem failure.",
                "switching owners must retain the ordinary error"
            );
        });
    }
}
