use super::*;
use gpui::{Entity, KeyDownEvent, Keystroke, Modifiers, VisualTestContext};

fn typed(key: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: Some(key.into()),
            modifiers: Modifiers::default(),
            is_keypad: false,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    }
}

fn call(
    shell: &mut Shell,
    target: &str,
    function: &str,
    arguments: &[&str],
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> String {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        Event::Call {
            request: flea::ipc::wire::Request::Call {
                target: target.into(),
                function: function.into(),
                arguments: arguments
                    .iter()
                    .map(|argument| argument.to_string())
                    .collect(),
            },
            answer,
        },
        window,
        cx,
    );
    receive.try_recv().unwrap().reply.stdout
}

fn sent(shell: &Shell) -> usize {
    shell.panes[0].as_ref().unwrap().nav.writer.borrow().sent
}

#[gpui::test]
fn a_tab_key_opens_the_strip_its_row_and_its_readers_and_saves_the_strip_once(
    cx: &mut TestAppContext,
) {
    let (backend, _) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let shell = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let lone = shell.geometry(window);
            assert_eq!(lone.tabs.height, 0., "one tab draws no strip");
            assert_eq!(
                call(shell, "flea", "tabBarVisible", &[], window, cx),
                "false\n"
            );
            shell.key(&typed("t"), window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 2);
            let two = shell.geometry(window);
            assert_eq!(two.tabs.height, 27., "a second tab gives the strip its row");
            assert_eq!(
                two.rows_height(false, 0.),
                lone.rows_height(false, 0.) - 27.,
                "taken from the rows"
            );
            assert_eq!(
                sent(shell),
                1,
                "the change saves lastPath and lastTabs in one patch"
            );
            let _ = shell.render(window, cx);
            let _ = shell.render(window, cx);
            assert_eq!(sent(shell), 1, "and a frame saves nothing");
            assert_eq!(call(shell, "flea", "tabCount", &[], window, cx), "2\n");
            assert_eq!(call(shell, "flea", "tabIndex", &[], window, cx), "1\n");
            assert_eq!(
                call(shell, "flea", "tabLabels", &[], window, cx),
                "files|files\n"
            );
            assert_eq!(
                call(shell, "flea", "tabBarVisible", &[], window, cx),
                "true\n"
            );
            let centre = call(shell, "flea", "tabCentre", &["1"], window, cx);
            let (x, y) = centre
                .trim()
                .split_once(' ')
                .expect("a drawn tab has a centre");
            assert!(
                x.parse::<i64>().unwrap() > 0 && y == "41",
                "the strip sits under the chrome: {centre}"
            );
            assert_eq!(
                call(shell, "fleatab", "taken", &["no-such-lift"], window, cx),
                "false\n",
                "an ack for no lift closes nothing"
            );
            assert_eq!(shell.strip.tabs[0].count(), 2);
            shell.key(&typed("w"), window, cx);
            assert_eq!(
                shell.strip.tabs[0].count(),
                1,
                "the close key closes the current tab"
            );
            assert_eq!(
                shell.geometry(window).tabs.height,
                0.,
                "and the strip gives its row back"
            );
            assert_eq!(sent(shell), 2);
        })
    });
}

#[gpui::test]
fn the_drawn_strip_brightens_the_mark_under_the_pointer_and_a_secondary_press_asks_the_menu(
    cx: &mut TestAppContext,
) {
    use crate::tabs::bar::{MenuAsk, HEIGHT, MENU_GAP, PAD_X};
    use gpui::{point, px, MouseButton};
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    let middle = px(hover_first_mark(&shell, cx));
    let width = cx.update(|_, cx| shell.read(cx).strip.bar.tab_width(2));
    cx.simulate_mouse_move(point(px(PAD_X + 1.), middle), None, Modifiers::default());
    let hover = cx.update(|_, cx| shell.read(cx).strip.bar.close_hover);
    assert_eq!(hover, None, "the tab body is not the mark");
    let at = PAD_X + width + 40.;
    cx.simulate_mouse_down(
        point(px(at), middle),
        MouseButton::Right,
        Modifiers::default(),
    );
    let menu = cx.update(|_, cx| shell.read(cx).strip.menu);
    assert_eq!(
        menu,
        Some(MenuAsk {
            index: 1,
            x: at,
            y: HEIGHT + MENU_GAP,
        }),
        "a secondary press on the second tab asks its menu 3 under the strip"
    );
    let count = cx.update(|_, cx| shell.read(cx).strip.tabs[0].count());
    assert_eq!(count, 2, "and the press itself closes and selects nothing");
}

// Draws a two-tab strip and parks the pointer on the first tab's close mark; answers the strip's middle y.
fn hover_first_mark(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> f32 {
    use crate::tabs::bar::{HEIGHT, HIT_MIN, PAD_X};
    use gpui::{point, px};
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.key(&typed("t"), window, cx)));
    cx.run_until_parked();
    let (width, top) = cx.update(|window, cx| {
        window.refresh();
        shell.update(cx, |shell, _| {
            (shell.strip.bar.tab_width(2), shell.geometry(window).tabs.y)
        })
    });
    cx.run_until_parked();
    let middle = top + HEIGHT / 2.;
    let mark = PAD_X + width - HIT_MIN / 2.;
    cx.simulate_mouse_move(point(px(mark), px(middle)), None, Modifiers::default());
    let hover = cx.update(|_, cx| shell.read(cx).strip.bar.close_hover);
    assert_eq!(
        hover,
        Some(0),
        "the first tab's mark under the pointer draws bright"
    );
    middle
}

#[gpui::test]
fn a_pointer_leaving_the_strip_from_the_mark_leaves_no_mark_bright(cx: &mut TestAppContext) {
    use crate::tabs::bar::{HIT_MIN, PAD_X};
    use gpui::{point, px};
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    let middle = hover_first_mark(&shell, cx);
    let width = cx.update(|_, cx| shell.read(cx).strip.bar.tab_width(2));
    let x = PAD_X + width - HIT_MIN / 2.;
    cx.simulate_mouse_move(point(px(x), px(middle + 120.)), None, Modifiers::default());
    let hover = cx.update(|_, cx| shell.read(cx).strip.bar.close_hover);
    assert_eq!(hover, None, "a pointer in the list leaves no mark bright");
    let frame_hover = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.render(window, cx);
            shell.strip.bar.close_hover
        })
    });
    assert_eq!(frame_hover, None, "and the next frame draws none");
}

#[gpui::test]
fn a_closed_tab_or_a_hidden_strip_hands_its_bright_mark_to_no_other_tab(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    hover_first_mark(&shell, cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.key(&typed("t"), window, cx)));
    cx.run_until_parked();
    let after_open = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.render(window, cx);
            (shell.strip.tabs[0].count(), shell.strip.bar.close_hover)
        })
    });
    assert_eq!(
        after_open,
        (3, None),
        "a tab count change drops the bright mark"
    );
    let hidden = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.strip.bar.close_hover = Some(1);
            shell.key(&typed("w"), window, cx);
            shell.key(&typed("w"), window, cx);
            let _ = shell.render(window, cx);
            (shell.strip.tabs[0].count(), shell.strip.bar.close_hover)
        })
    });
    assert_eq!(hidden, (1, None), "a strip that hides keeps no bright mark");
}
