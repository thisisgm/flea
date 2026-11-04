//! The first card frame either has its complete model or the delayed loading state.
use super::super::test_shell::land;
use super::super::{
    menudeliver_tests::{open, select, REG},
    menus_tests::{deliver as reply, home},
};
use gpui::{TestAppContext, VisualTestContext};
use std::time::Duration;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn sol_gap64_openwith_first_frame_waits_for_the_complete_catalogue(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    shell.update(cx, |shell, cx| {
        shell.open_open_with(0, "/files/notes.txt".into(), cx)
    });
    draw(cx);
    assert!(
        cx.debug_bounds("op3-openwith-scrim").is_none(),
        "no partial initial card"
    );
    cx.simulate_keystrokes("z");
    let id = shell.read_with(cx, |shell, _| {
        let open = shell.open_with.as_ref().unwrap();
        assert!(
            open.card.query.is_empty(),
            "unpublished controls cannot consume text"
        );
        open.id
    });
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    draw(cx);
    assert!(cx.debug_bounds("op3-openwith-scrim").is_none());
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"kind":"Plain text document","mime":"text/plain","applications":[{{"id":"a.desktop","label":"A","icon":"","default":true}}],"installed":[]}}"#
        ),
    );
    draw(cx);
    assert!(cx.debug_bounds("op3-openwith-scrim").is_some());
    shell.read_with(cx, |shell, _| {
        assert!(shell
            .open_with
            .as_ref()
            .unwrap()
            .frame
            .part("row0")
            .is_some())
    });
}

#[gpui::test]
fn sol_gap64_properties_initial_loading_obeys_hold_and_drawn_minimum(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    cx.update(|window, cx| {
        shell.update(
            cx,
            |shell, cx| assert!(shell.properties_listing(window, cx)),
        )
    });
    draw(cx);
    assert!(
        cx.debug_bounds("op3-properties-scrim").is_none(),
        "no initial partial properties"
    );
    cx.executor().advance_clock(Duration::from_millis(149));
    draw(cx);
    assert!(cx.debug_bounds("op3-properties-scrim").is_none());
    cx.executor().advance_clock(Duration::from_millis(1));
    draw(cx);
    assert!(cx.debug_bounds("op3-properties-scrim").is_some());
    let height = |cx: &VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            shell
                .properties
                .as_ref()
                .unwrap()
                .probe
                .probe
                .bounds("card")
                .unwrap()
                .size
                .height
        })
    };
    let loading = height(cx);
    let id = shell.read_with(cx, |shell, _| shell.menu.sequence);
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"properties","ok":true,"path":"/files/notes.txt","name":"notes.txt","directory":false,"bytes":12,"mode":"0644"}}"#
        ),
    );
    draw(cx);
    assert_eq!(
        height(cx),
        loading,
        "the drawn loading card keeps its minimum"
    );
    cx.executor().advance_clock(Duration::from_millis(99));
    draw(cx);
    assert!(cx.debug_bounds("op3-properties-scrim").is_some());
    assert_eq!(height(cx), loading);
    cx.executor().advance_clock(Duration::from_millis(1));
    draw(cx);
    assert!(cx.debug_bounds("op3-properties-scrim").is_some());
    assert!(
        height(cx) > loading,
        "the complete model replaces loading at its drawn minimum"
    );
}

#[gpui::test]
fn sol_gap64_drawn_loading_cancel_answers_enter_and_space(cx: &mut TestAppContext) {
    for properties in [false, true] {
        for key in ["enter", "space"] {
            let (shell, cx) = open(cx);
            land(&shell, cx, "/files", home(), 1);
            if properties {
                select(&shell, cx, &[2]);
                cx.update(|window, cx| {
                    shell.update(cx, |shell, cx| {
                        assert!(shell.properties_listing(window, cx));
                    });
                });
            } else {
                shell.update(cx, |shell, cx| {
                    shell.open_open_with(0, "/files/notes.txt".into(), cx)
                });
            }
            cx.executor().advance_clock(Duration::from_millis(150));
            draw(cx);
            let selector = if properties {
                "op3-properties-scrim"
            } else {
                "op3-openwith-scrim"
            };
            assert!(cx.debug_bounds(selector).is_some());
            cx.simulate_keystrokes(key);
            shell.read_with(cx, |shell, _| {
                assert!(shell.properties.is_none());
                assert!(
                    shell.open_with.is_none(),
                    "loading Cancel must answer {key}"
                );
            });
        }
    }
}
