//! Lost releases cancel the row hold and let the same watched directory settle.
use super::{frame, open};
use crate::list::bench;
use gpui::{prelude::*, DispatchPhase, MouseUpEvent};
use gpui::{Modifiers, MouseButton, TestAppContext};
use std::{cell::Cell, rc::Rc};

struct ReleaseOverlay {
    shell: gpui::Entity<crate::state::Shell>,
    consumed: Rc<Cell<usize>>,
}

impl gpui::Render for ReleaseOverlay {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let consumed = self.consumed.clone();
        gpui::div().size_full().child(self.shell.clone()).child(
            gpui::canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let consumed = consumed.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            consumed.set(consumed.get() + 1);
                            cx.stop_propagation();
                        }
                    });
                },
            )
            .absolute()
            .size_full(),
        )
    }
}

fn press(
    cx: &mut TestAppContext,
    blur: bool,
) -> (
    gpui::VisualTestContext,
    gpui::WindowHandle<crate::state::Shell>,
) {
    let handle = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            if blur {
                shell.subscribe_blur(window, cx);
            }
            window.activate_window();
        })
        .unwrap();
    frame(cx, handle);
    let at = handle
        .update(cx, |shell, _, _| {
            let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
            gpui::point(
                frame.bounds.left() + gpui::px(40.),
                frame.bounds.top() + gpui::px(frame.row * 1.5),
            )
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.nav.gate.pointer_held);
            let now = host.pane.now();
            assert!(host.nav.changed(&host.pane, "/files", now));
            assert!(!host.nav.fire_watch(&mut host.pane, &[], now + 400));
            assert!(host.nav.watch.stale && host.nav.watch.due.is_none());
        })
        .unwrap();
    (visual, handle)
}

fn released(visual: &mut gpui::VisualTestContext, handle: gpui::WindowHandle<crate::state::Shell>) {
    let entity = handle.entity(visual).unwrap();
    visual.update(|_, cx| {
        entity.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            assert!(
                !host.nav.gate.pointer_held,
                "a lost release left the row hold live"
            );
            assert!(
                host.input.press.is_none()
                    && host.input.grab.is_none()
                    && host.input.band.is_none()
            );
            let due = host
                .nav
                .watch
                .due
                .expect("cancel must rearm the owed reread");
            assert!(host.nav.fire_watch(&mut host.pane, &[], due));
            assert_eq!(host.nav.watch.rereads, 1);
        });
    });
}

#[test]
fn m9_blur_cancels_the_pointer_hold_and_rearms_the_owed_reread() {
    let mut cx = bench::real_cx();
    let (mut visual, handle) = press(&mut cx, true);
    visual.deactivate_window();
    visual.run_until_parked();
    released(&mut visual, handle);
}

#[test]
fn m9_buttonless_motion_recovers_a_swallowed_release() {
    let mut cx = bench::real_cx();
    let (mut visual, handle) = press(&mut cx, false);
    visual.simulate_mouse_move(
        gpui::point(gpui::px(200.), gpui::px(200.)),
        None,
        Modifiers::none(),
    );
    released(&mut visual, handle);
}

#[test]
fn m9_consumed_release_runs_the_capture_fallback_without_pointer_motion() {
    let mut cx = bench::real_cx();
    let (mut visual, handle) = press(&mut cx, false);
    let shell = handle.entity(&visual).unwrap();
    let consumed = Rc::new(Cell::new(0));
    visual.replace_root_view(|_, _| ReleaseOverlay {
        shell: shell.clone(),
        consumed: consumed.clone(),
    });
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    visual.simulate_mouse_up(
        gpui::point(gpui::px(200.), gpui::px(200.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    visual.run_until_parked();
    assert_eq!(consumed.get(), 1, "the overlay must consume Bubble release");
    visual.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            assert!(
                !host.nav.gate.pointer_held,
                "Capture fallback left the hold live"
            );
            assert!(host.input.press.is_none());
            let due = host
                .nav
                .watch
                .due
                .expect("fallback must rearm the owed reread");
            assert!(host.nav.fire_watch(&mut host.pane, &[], due));
            assert_eq!(host.nav.watch.rereads, 1);
        });
    });
}
