//! Pointer actions propagate without implicitly moving the listing's keyboard focus.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, startup::View},
};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    div, point, prelude::*, px, AppContext, Context, KeyDownEvent, Keystroke, Modifiers,
    MouseButton, Render, TestAppContext, Window,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Actions {
    pressed: Vec<MouseButton>,
    clicks: usize,
}

struct Control(Rc<RefCell<Actions>>);

impl Render for Control {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let presses = Rc::clone(&self.0);
        let clicks = Rc::clone(&self.0);
        div()
            .id("pointer-action")
            .debug_selector(|| "pointer-action".into())
            .size_full()
            .on_any_mouse_down(move |event, _, _| {
                presses.borrow_mut().pressed.push(event.button);
            })
            .on_click(move |_, _, _| clicks.borrow_mut().clicks += 1)
            .child("Action")
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
    host
}

fn pointer_controls(cx: &mut TestAppContext, strip: bool) {
    let (backend, _requests) = Backend::test_channel();
    let actions = Rc::new(RefCell::new(Actions::default()));
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        if strip {
            shell.slots.tabs = Some(cx.new(|_| Control(Rc::clone(&actions))).into());
        } else {
            shell.view = View::Dual;
        }
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    if !strip {
        cx.update(|_, cx| {
            entity.update(cx, |shell, cx| {
                assert!(shell.chrome.view.is_some());
                shell.slots.chrome = Some(cx.new(|_| Control(Rc::clone(&actions))).into());
                cx.notify();
            })
        });
        cx.run_until_parked();
    }
    assert!(cx.debug_bounds("pointer-action").is_some());
    let at = point(px(300.), px(if strip { 40. } else { 10. }));
    let sides: &[usize] = if strip { &[0] } else { &[0, 1] };
    let buttons = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];
    for &side in sides {
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                shell.focus_pane(side, cx);
                window.focus(&shell.focus, cx);
            })
        });
        cx.run_until_parked();
        for button in buttons {
            let before = cx.update(|_, cx| {
                entity.update(cx, |shell, _| {
                    std::array::from_fn::<_, 2, _>(|side| {
                        let host = shell.panes[side].as_ref().unwrap();
                        (
                            host.pane.cursor,
                            host.selected.clone(),
                            Rc::clone(&host.pane.held),
                        )
                    })
                })
            });
            cx.simulate_mouse_move(at, None, Modifiers::none());
            cx.simulate_mouse_down(at, button, Modifiers::none());
            cx.simulate_mouse_up(at, button, Modifiers::none());
            cx.run_until_parked();
            cx.update(|window, cx| {
                entity.update(cx, |shell, cx| {
                    assert_eq!(
                        shell.focus_view(window, cx),
                        "list",
                        "{button:?}, strip={strip}"
                    );
                    assert_eq!(shell.focus_side, side);
                    for (index, (cursor, selected, held)) in before.iter().enumerate() {
                        let host = shell.panes[index].as_ref().unwrap();
                        assert_eq!((host.pane.cursor, &host.selected), (*cursor, selected));
                        assert!(Rc::ptr_eq(&host.pane.held, held));
                    }
                    shell.key(
                        &KeyDownEvent {
                            keystroke: Keystroke::parse("down").unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    );
                    assert_eq!(
                        shell.panes[side].as_ref().unwrap().pane.cursor,
                        before[side].0 + 1
                    );
                    assert_eq!(
                        shell.panes[1 - side].as_ref().unwrap().pane.cursor,
                        before[1 - side].0
                    );
                })
            });
        }
    }
    assert_eq!(actions.borrow().pressed, buttons.repeat(sides.len()));
    assert_eq!(actions.borrow().clicks, sides.len());
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let handle = if strip {
                &shell.dual_focus.strip
            } else {
                &shell.dual_focus.chrome
            };
            window.focus(handle, cx);
            assert_eq!(
                shell.focus_view(window, cx),
                if strip { "strip" } else { "chrome" }
            );
        })
    });
}

#[gpui::test]
fn pointer_chrome_actions_keep_both_dual_panes_and_the_listing_keys(cx: &mut TestAppContext) {
    pointer_controls(cx, false);
}

#[gpui::test]
fn pointer_strip_actions_keep_the_listing_keys_and_deliberate_focus(cx: &mut TestAppContext) {
    pointer_controls(cx, true);
}
