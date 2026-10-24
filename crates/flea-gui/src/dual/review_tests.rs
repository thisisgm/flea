//! Review probes for uish-u26 round 1; each asserts the v0.3.12 QML behaviour it cites.
use crate::{
    backend::Backend,
    rail::{host::Host, Mode},
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, startup::View},
};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    div, point, prelude::*, px, AppContext, Context, KeyDownEvent, Keystroke, Modifiers,
    MouseButton, Render, TestAppContext, Window,
};

struct Label;
impl Render for Label {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child("Native control")
    }
}

fn right(shell: &Shell) -> PaneHost {
    let mut host = PaneHost::new("/right".into(), None, 620., &shell.locale);
    host.pane
        .receive(Reply::Listed {
            path: "/right".into(),
            total: 8,
        })
        .unwrap();
    host.pane
        .receive(Reply::Rows {
            start: 0,
            rows: vec![RowFacts::default(); 8],
            listing: 2,
        })
        .unwrap();
    host.pane.cursor = 5;
    host
}

fn key(shell: &mut Shell, text: &str, window: &mut Window, cx: &mut Context<Shell>) {
    shell.key(
        &KeyDownEvent {
            keystroke: Keystroke::parse(text).unwrap(),
            is_held: false,
            prefer_character_input: false,
        },
        window,
        cx,
    );
}

/// ChromeBar.qml, ChromeButton.qml and ChromeModes.qml never take focus on a click, so the focused Dual list keeps the keys.
#[gpui::test]
fn review_a_chrome_click_keeps_the_dual_list_keys(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        shell.view = View::Dual;
        shell.focus_side = 1;
        shell.dual_focus.last_side = 1;
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    let at = point(px(300.), px(10.));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let stop = shell.focus_view(window, cx);
            key(shell, "down", window, cx);
            let cursor = shell.panes[1].as_ref().unwrap().pane.cursor;
            assert_eq!(
                (stop, cursor),
                ("list", 6),
                "after a click on the chrome bar the keyboard sat on {stop:?} and Down left the right cursor at {cursor}"
            );
        })
    });
}

fn nonleft_chrome(cx: &mut TestAppContext, button: MouseButton) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        shell.panes[1].as_mut().unwrap().pane.cursor = 0;
        shell.view = View::Dual;
        shell.focus_side = 1;
        shell.dual_focus.last_side = 1;
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    let at = point(px(300.), px(10.));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, button, Modifiers::none());
    cx.simulate_mouse_up(at, button, Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let stop = shell.focus_view(window, cx);
            key(shell, "down", window, cx);
            assert_eq!(
                (stop, shell.panes[1].as_ref().unwrap().pane.cursor),
                ("list", 1),
                "{button:?} chrome press must leave the listing keyboard intact"
            );
        })
    });
}

#[gpui::test]
fn round2_right_chrome_press_keeps_the_dual_list_keys(cx: &mut TestAppContext) {
    nonleft_chrome(cx, MouseButton::Right);
}

#[gpui::test]
fn round2_middle_chrome_press_keeps_the_dual_list_keys(cx: &mut TestAppContext) {
    nonleft_chrome(cx, MouseButton::Middle);
}

/// WindowBody.qml:134-137: entering Dual calls focusPane(focusSide), whose forceActiveFocus hands the keys to that list.
#[gpui::test]
fn review_dual_activation_hands_the_keys_to_the_focused_list(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    let at = point(px(300.), px(10.));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let before = shell.focus_view(window, cx);
            shell.set_view(View::Dual, window, cx);
            assert_eq!(shell.view, View::Dual, "the listed secondary activates at once");
            let after = shell.focus_view(window, cx);
            // tests/ui.sh case_dual: click_chrome dual, then Tab reaches the right listing.
            key(shell, "tab", window, cx);
            let tab = (shell.focus_side, shell.focus_view(window, cx));
            assert_eq!(
                (after, tab),
                ("list", (1, "list")),
                "focus before Dual was {before:?}; after activation it was {after:?}, and Tab then reached {tab:?}"
            );
        })
    });
}

/// PaneRail.qml's overlay owns only its edge strip and its body; a press on a pane's path row or header leaves the list focused.
#[gpui::test]
fn review_an_auto_hidden_rail_never_takes_a_pane_header_press(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        shell.view = View::Dual;
        shell.focus_side = 1;
        shell.dual_focus.last_side = 1;
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"places":{"autoHide":true}}"#).unwrap();
        let rail = cx.new(|cx| {
            Host::new(
                Mode::Overlay,
                192.,
                Box::new(|_, cx| cx.new(|_| Label).into()),
                cx,
            )
        });
        shell.slots.rail = Some(rail.into());
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    let (x, y) = cx.update(|window, cx| {
        entity.update(cx, |shell, _| {
            let pane = shell.geometry(window).panes[1];
            // The right pane's own path row, the first 27 px of a Dual pane.
            (pane.x + 60., pane.y + 13.)
        })
    });
    let at = point(px(x), px(y));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let stop = shell.focus_view(window, cx);
            let revealed = shell
                .slots
                .rail
                .clone()
                .unwrap()
                .downcast::<Host>()
                .unwrap()
                .read(cx)
                .rail
                .wants()
                .focus;
            key(shell, "down", window, cx);
            let cursor = shell.panes[1].as_ref().unwrap().pane.cursor;
            assert_eq!(
                (stop, revealed, cursor),
                ("list", false, 6),
                "a press on the right path row put the keyboard on {stop:?} (rail asked to reveal: {revealed}) and Down left the cursor at {cursor}"
            );
        })
    });
}

/// Focus.js:52-66 with ChromeKeys.js:37-40: past a hidden rail, Tab from the right list enters the chrome as the right pane, so focusSide stays 1.
#[gpui::test]
fn review_tab_past_a_hidden_rail_enters_the_chrome_on_the_right_side(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        shell.view = View::Dual;
        shell.focus_side = 1;
        shell.dual_focus.last_side = 1;
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"places":{"rail":"hidden"}}"#).unwrap();
        let rail = cx.new(|cx| {
            Host::new(
                Mode::Hidden,
                192.,
                Box::new(|_, cx| cx.new(|_| Label).into()),
                cx,
            )
        });
        shell.slots.rail = Some(rail.into());
        shell.slots.chrome = Some(cx.new(|_| Label).into());
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            key(shell, "tab", window, cx);
            let stop = (shell.focus_side, shell.focus_view(window, cx));
            assert_eq!(
                stop,
                (1, "chrome"),
                "Tab from the right list reached {stop:?}"
            );
        })
    });
}

/// Ipc.qml:152 reports a pane focused while anything inside it holds focus, and the rail is the left pane's.
#[gpui::test]
fn review_dual_state_reports_the_left_pane_focused_while_its_rail_has_the_keys(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        shell.view = View::Dual;
        let rail = cx.new(|cx| {
            Host::new(
                Mode::Docked,
                192.,
                Box::new(|_, cx| cx.new(|_| Label).into()),
                cx,
            )
        });
        shell.slots.rail = Some(rail.into());
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            key(shell, "tab", window, cx);
            key(shell, "tab", window, cx);
            let stop = (shell.focus_side, shell.focus_view(window, cx));
            let focused = shell.dual_adapter(window, cx).panes[0]
                .as_ref()
                .unwrap()
                .focused;
            assert_eq!(
                (stop, focused),
                ((0, "rail"), true),
                "on {stop:?} dualState reported panes[0].focused = {focused}"
            );
        })
    });
}
