//! Keyboard input traverses mounted native regions while each pane retains its own session.
use crate::{
    backend::Backend,
    rail::{host::Host, Mode},
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, startup::View},
};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    div, prelude::*, AppContext, Context, KeyDownEvent, Keystroke, Render, TestAppContext, Window,
};
use std::rc::Rc;

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
    host.selected = vec![2, 5];
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

#[gpui::test]
fn u26_dual_tab_walks_mounted_regions_and_reveals_overlay_without_relayout(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        shell.view = View::Dual;
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
        shell.slots.chrome = Some(cx.new(|_| Label).into());
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let before = shell.geometry(window).panes;
            let right = Rc::clone(&shell.panes[1].as_ref().unwrap().pane.held);
            for (stroke, side, stop) in [
                ("tab", 1, "list"),
                ("tab", 0, "rail"),
                ("tab", 0, "chrome"),
                ("tab", 0, "list"),
                ("shift-tab", 0, "chrome"),
                ("shift-tab", 0, "rail"),
                ("shift-tab", 1, "list"),
                ("shift-tab", 0, "list"),
            ] {
                key(shell, stroke, window, cx);
                assert_eq!(
                    (shell.focus_side, shell.focus_view(window, cx)),
                    (side, stop),
                    "{stroke}"
                );
                assert_eq!(
                    shell.geometry(window).panes,
                    before,
                    "rail focus must never resize listings"
                );
                if stop != "list" {
                    let cursors = std::array::from_fn::<_, 2, _>(|side| {
                        shell.panes[side].as_ref().unwrap().pane.cursor
                    });
                    key(shell, "down", window, cx);
                    assert_eq!(
                        std::array::from_fn::<_, 2, _>(|side| shell.panes[side]
                            .as_ref()
                            .unwrap()
                            .pane
                            .cursor),
                        cursors
                    );
                }
                let rail = shell
                    .slots
                    .rail
                    .clone()
                    .unwrap()
                    .downcast::<Host>()
                    .unwrap();
                assert_eq!(rail.read(cx).rail.wants().focus, stop == "rail");
                assert_eq!(shell.panes[1].as_ref().unwrap().selected, vec![2, 5]);
                assert!(Rc::ptr_eq(
                    &right,
                    &shell.panes[1].as_ref().unwrap().pane.held
                ));
            }
        })
    });
}

#[gpui::test]
fn u26_hidden_pane_keeps_marks_and_focus_cannot_reach_it(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.panes[1] = Some(right(shell));
            shell.view = View::Dual;
            window.focus(&shell.focus, cx);
            shell.focus_pane(1, cx);
            let held = Rc::clone(&shell.panes[1].as_ref().unwrap().pane.held);
            shell.set_view(View::List, window, cx);
            shell.focus_pane(1, cx);
            assert_eq!(shell.focus_side, 0);
            assert_eq!(shell.focus_view(window, cx), "list");
            let _ = shell.render(window, cx);
            assert_eq!(
                shell.panes[1].as_ref().unwrap().list.borrow().rows.misses,
                0
            );
            assert!(
                !shell.dual_adapter(window, cx).panes[1]
                    .as_ref()
                    .unwrap()
                    .focused
            );
            shell.set_view(View::Dual, window, cx);
            assert_eq!(shell.focus_side, 1);
            let host = shell.panes[1].as_ref().unwrap();
            assert_eq!(
                (host.pane.cursor, host.selected.as_slice()),
                (5, &[2, 5][..])
            );
            assert!(Rc::ptr_eq(&held, &host.pane.held));
        });
    });
}

#[gpui::test]
fn u26_slow_pane_never_traps_tab_or_changes_the_other_session(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.panes[1] = Some(right(shell));
            shell.view = View::Dual;
            window.focus(&shell.focus, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.pane
                .begin("/slow".into(), Default::default(), host.pane.now());
            let held = Rc::clone(&host.pane.held);
            key(shell, "tab", window, cx);
            assert_eq!(shell.focus_side, 1);
            key(shell, "down", window, cx);
            assert_eq!(shell.panes[1].as_ref().unwrap().pane.cursor, 6);
            assert!(shell.panes[0].as_ref().unwrap().pane.busy());
            assert!(Rc::ptr_eq(
                &held,
                &shell.panes[0].as_ref().unwrap().pane.held
            ));
            assert_eq!(shell.panes[1].as_ref().unwrap().selected, vec![2, 5]);
        });
    });
}

#[gpui::test]
fn u26_dual_clamp_uses_the_current_typeface_without_changing_the_stored_split(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, _| {
            shell.view = View::Dual;
            shell.launch.settings.split = 0.99;
            let before = shell.geometry(window).panes[1].width;
            {
                let mut list = shell.panes[0].as_ref().unwrap().list.borrow_mut();
                list.typeface.body = gpui::px(20.);
                list.typeface.body_small = gpui::px(18.);
            }
            let geometry = shell.geometry(window);
            let room: f32 = geometry.panes.iter().map(|pane| pane.width).sum();
            assert!(geometry.panes[1].width > before);
            assert!((geometry.panes[1].width - shell.dual_min().min(room / 2.)).abs() < 0.01);
            assert_eq!(shell.launch.settings.split, 0.99);
        });
    });
}

/// ViewState.owe: a focus save is written only when focus or paths changed.
#[gpui::test]
fn u26_a_pane_click_saves_focus_once_and_a_repeat_saves_nothing(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.panes[1] = Some(right(shell));
            shell.view = View::Dual;
            let sent = |shell: &Shell| -> usize {
                shell
                    .panes
                    .iter()
                    .flatten()
                    .map(|host| host.nav.writer.borrow().sent)
                    .sum()
            };
            shell.focus_pane(1, cx);
            let first = sent(shell);
            assert_eq!(first, 1, "a changed focus is saved");
            shell.focus_pane(1, cx);
            assert_eq!(sent(shell), first, "the same focus and paths save nothing");
            shell.focus_pane(0, cx);
            assert_eq!(sent(shell), first + 1, "the other pane is saved again");
            let _ = window;
        });
    });
}
