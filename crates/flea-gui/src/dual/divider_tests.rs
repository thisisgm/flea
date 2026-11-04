//! Divider wirings pinned per mutant: Escape, blur, hover (nine pixels), real presses, leaving Dual.
use crate::{
    backend::Backend,
    dual::{split_of, ROOM_MIN, SCALE},
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, startup::View},
};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    point, px, AppContext, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext,
};

fn escape_key() -> gpui::KeyDownEvent {
    gpui::KeyDownEvent {
        keystroke: gpui::Keystroke {
            key: "escape".into(),
            ..Default::default()
        },
        is_held: false,
        prefer_character_input: false,
    }
}

#[gpui::test]
fn escape_cancels_the_drag_with_no_write(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            shell.view = View::Dual;
            shell.dual_begin(600., 1, window, cx);
            shell.dual_move(700., window, cx);
            assert!(
                shell.dual_drag.is_some_and(|drag| drag.live()),
                "the travel is live"
            );
            let before = shell.launch.settings.split;
            shell.key(&escape_key(), window, cx);
            assert!(shell.dual_drag.is_none(), "Escape leaves no drag live");
            shell.dual_end(true, window, cx);
            assert_eq!(
                shell.launch.settings.split, before,
                "an escaped drag owes no leaf"
            );
        })
    });
}

#[gpui::test]
fn blur_cancels_the_drag_with_no_write(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.subscribe_blur(window, cx);
        shell
    });
    cx.update(|window, cx| {
        // Activate first: a fresh test window starts inactive, so deactivating it would no-op.
        window.activate_window();
        entity.update(cx, |shell, cx| {
            window.focus(&shell.focus, cx);
            shell.view = View::Dual;
            shell.dual_begin(600., 1, window, cx);
            shell.dual_move(700., window, cx);
            assert!(
                shell.dual_drag.is_some_and(|drag| drag.live()),
                "the travel is live"
            );
        })
    });
    cx.run_until_parked();
    let before = cx.update(|_, cx| entity.read(cx).launch.settings.split);
    cx.deactivate_window();
    cx.run_until_parked();
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            assert!(
                shell.dual_drag.is_none(),
                "deactivation leaves no drag live"
            );
            shell.dual_end(true, window, cx);
            assert_eq!(
                shell.launch.settings.split, before,
                "a blurred drag owes no leaf"
            );
        })
    });
}

#[gpui::test]
fn hover_lights_only_the_nine_pixel_zone(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|_, cx| {
        entity.update(cx, |shell, cx| {
            shell.view = View::Dual;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let (left, top, height) = cx.update(|window, cx| {
        entity.update(cx, |shell, _| {
            let geometry = shell.geometry(window);
            let x = geometry.rail.width + geometry.panes[0].width;
            (x, geometry.panes[0].y, geometry.panes[0].height)
        })
    });
    let middle = top + height / 2.;
    let hovered = |cx: &mut gpui::VisualTestContext, x: f32| {
        cx.simulate_mouse_move(point(px(x), px(middle)), None, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.update(|_, cx| entity.read(cx).dual_hover)
    };
    assert!(hovered(cx, left + 2.), "two pixels in lights the accent");
    assert!(hovered(cx, left + 8.), "eight pixels in still lights it");
    assert!(
        !hovered(cx, left + 9.),
        "nine pixels out is the first pixel past the zone"
    );
    assert!(
        !hovered(cx, left + 12.),
        "twelve pixels out is the listing's padding"
    );
    assert!(!hovered(cx, left - 50.), "the left listing never lights it");
    assert_eq!(crate::dual::ZONE, 9., "ResizeHandle.qml:24 is nine pixels");
}

/// Dual on a left listing and a right listing of ten rows; blur subscribes the deactivation arm.
fn dual_window(cx: &mut TestAppContext, blur: bool) -> (Entity<Shell>, &mut VisualTestContext) {
    let (backend, _) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        if blur {
            shell.subscribe_blur(window, cx);
        }
        let mut host = PaneHost::new("/right".into(), None, 620., &shell.locale);
        host.pane
            .receive(Reply::Listed {
                path: "/right".into(),
                total: 10,
            })
            .unwrap();
        host.pane
            .receive(Reply::Rows {
                start: 0,
                rows: vec![RowFacts::default(); 10],
                listing: 2,
            })
            .unwrap();
        shell.panes[1] = Some(host);
        shell.view = View::Dual;
        shell
    });
    cx.run_until_parked();
    (entity, cx)
}

/// The divider's pixel and the middle of the listings' height, in window px.
fn divider_at(entity: &Entity<Shell>, cx: &mut VisualTestContext) -> (f32, f32) {
    cx.update(|window, cx| {
        entity.update(cx, |shell, _| {
            let geometry = shell.geometry(window);
            (
                geometry.rail.width + geometry.panes[0].width,
                geometry.panes[0].y + geometry.panes[0].height / 2.,
            )
        })
    })
}

/// The drag room and the left listing's width, as Shell::dual_room reads them.
fn room_and_left(entity: &Entity<Shell>, cx: &mut VisualTestContext) -> (f32, f32) {
    cx.update(|window, cx| {
        let width = f32::from(window.viewport_size().width);
        let rail = entity.read(cx).launch.settings.rail_width.clamp(0., width);
        let left = entity.read(cx).geometry(window).panes[0].width;
        (((width - rail) - 1.).max(0.), left)
    })
}

/// Leaves written so far, summed over both panes' backends.
fn sent(entity: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| {
        entity
            .read(cx)
            .panes
            .iter()
            .flatten()
            .map(|host| host.nav.writer.borrow().sent)
            .sum()
    })
}

/// A press in the zone takes it alone: the right listing never takes focus.
#[gpui::test]
fn divider_press_keeps_the_focused_pane(cx: &mut TestAppContext) {
    let (entity, cx) = dual_window(cx, false);
    let (left, middle) = divider_at(&entity, cx);
    let mut seen = Vec::new();
    for offset in [0., 4., 8.] {
        cx.update(|_, cx| entity.update(cx, |shell, _| shell.focus_side = 0));
        let at = point(px(left + offset), px(middle));
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        let (began, side) = cx.update(|_, cx| {
            let shell = entity.read(cx);
            (shell.dual_drag.is_some(), shell.focus_side)
        });
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        seen.push((offset, began, side));
    }
    for (offset, began, side) in seen {
        assert!(began, "the press at +{offset} did not start the drag");
        assert_eq!(
            side, 0,
            "a divider press at +{offset} moved focus to the right pane"
        );
    }
}

/// A release past either window edge ends the drag at its release x clamped to the window, so a later click writes no split.
#[gpui::test]
fn release_past_the_window_edge_ends_the_drag(cx: &mut TestAppContext) {
    let (entity, cx) = dual_window(cx, false);
    let (width, height) = cx.update(|window, _| {
        let size = window.viewport_size();
        (f32::from(size.width), f32::from(size.height))
    });
    for (label, out_y) in [("right", None), ("below", Some(height + 40.))] {
        cx.update(|_, cx| {
            entity.update(cx, |shell, cx| {
                shell.launch.settings.split = 0.5;
                cx.notify();
            })
        });
        cx.run_until_parked();
        let (left, middle) = divider_at(&entity, cx);
        let (room, left_width) = room_and_left(&entity, cx);
        let out = out_y.map_or_else(
            || point(px(width + 40.), px(middle)),
            |y| point(px(left + 104.), px(y)),
        );
        let at = point(px(left + 4.), px(middle));
        let to = point(px(left + 104.), px(middle));
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(out, MouseButton::Left, Modifiers::none());
        let before = sent(&entity, cx);
        cx.simulate_mouse_up(out, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        let release_writes = sent(&entity, cx) - before;
        let (live, split_at_release) = cx.update(|_, cx| {
            let shell = entity.read(cx);
            (
                shell.dual_drag.is_some_and(|drag| drag.live()),
                shell.launch.settings.split,
            )
        });
        // A click on the already focused pane saves nothing once its focus leaf is stored, so a stray leaf shows as a write.
        cx.update(|_, cx| entity.update(cx, |shell, _| shell.remember_dual()));
        let row = point(px(left - 200.), px(middle));
        let before = sent(&entity, cx);
        cx.simulate_mouse_move(row, None, Modifiers::none());
        cx.simulate_mouse_down(row, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(row, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        let click_writes = sent(&entity, cx) - before;
        let split_after_click = cx.update(|_, cx| entity.read(cx).launch.settings.split);
        // Right: the clamp of the release x; below: the release x is in the window, so its own travel.
        let units = if out_y.is_none() {
            split_of(room * (1. - ROOM_MIN / room), room)
        } else {
            split_of(left_width + 100., room)
        };
        let expected = units as f32 / SCALE as f32;
        assert!(!live, "release past the {label} edge left the drag live");
        assert!(
            (split_at_release - expected).abs() < 1e-4,
            "release past the {label} edge committed {split_at_release}, not the clamp {expected}"
        );
        assert_eq!(
            release_writes, 1,
            "release past the {label} edge wrote {release_writes} leaves"
        );
        assert_eq!(click_writes, 0, "a row click on the focused pane after a release past the {label} edge wrote {click_writes} saves");
        assert_eq!(
            split_after_click, split_at_release,
            "a row click after a release past the {label} edge wrote a split"
        );
    }
}

/// Deactivation cancels a live drag even when the shell root holds no focus.
#[gpui::test]
fn deactivation_without_root_focus_cancels_the_drag(cx: &mut TestAppContext) {
    let (entity, cx) = dual_window(cx, true);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let (left, middle) = divider_at(&entity, cx);
    let at = point(px(left + 4.), px(middle));
    let to = point(px(left + 104.), px(middle));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    let (focused, live, before) = cx.update(|window, cx| {
        let shell = entity.read(cx);
        (
            shell.focus.is_focused(window),
            shell.dual_drag.is_some_and(|drag| drag.live()),
            shell.launch.settings.split,
        )
    });
    assert!(!focused, "the shell root holds no focus in this case");
    assert!(live, "the travel is live before deactivation");
    cx.deactivate_window();
    cx.run_until_parked();
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let (live_after, after) = cx.update(|_, cx| {
        let shell = entity.read(cx);
        (shell.dual_drag.is_some(), shell.launch.settings.split)
    });
    assert!(
        !live_after,
        "deactivation without root focus left the drag live"
    );
    assert_eq!(
        after, before,
        "a drag deactivated without root focus wrote a leaf"
    );
}

/// Activation mid-drag keeps the drag: a press on an inactive window, then focus arrives, still commits.
#[gpui::test]
fn activation_mid_drag_keeps_the_drag(cx: &mut TestAppContext) {
    let (entity, cx) = dual_window(cx, true);
    let active_at_start = cx.update(|window, _| window.is_window_active());
    let (left, middle) = divider_at(&entity, cx);
    let at = point(px(left + 4.), px(middle));
    let to = point(px(left + 104.), px(middle));
    let before = sent(&entity, cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let (active, live) = cx.update(|window, cx| {
        let shell = entity.read(cx);
        (
            window.is_window_active(),
            shell.dual_drag.is_some_and(|drag| drag.live()),
        )
    });
    assert!(
        !active_at_start && active,
        "the window must activate mid-drag"
    );
    assert!(live, "activation cancelled a live drag");
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        sent(&entity, cx) - before,
        1,
        "the release after activation did not write one leaf"
    );
}

/// Leaving Dual mid-drag cancels the drag, so the release writes no split.
#[gpui::test]
fn leaving_dual_mid_drag_writes_no_leaf(cx: &mut TestAppContext) {
    let (entity, cx) = dual_window(cx, false);
    let (left, middle) = divider_at(&entity, cx);
    let at = point(px(left), px(middle));
    let to = point(px(left + 100.), px(middle));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    let (live, before) = cx.update(|_, cx| {
        let shell = entity.read(cx);
        (
            shell.dual_drag.is_some_and(|drag| drag.live()),
            shell.launch.settings.split,
        )
    });
    assert!(live, "the travel is live before the view leaves Dual");
    cx.update(|window, cx| entity.update(cx, |shell, cx| shell.set_view(View::List, window, cx)));
    cx.run_until_parked();
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let after = cx.update(|_, cx| entity.read(cx).launch.settings.split);
    assert_eq!(
        after, before,
        "a drag whose divider left with Dual wrote a leaf"
    );
}

/// EY-258: after a drag, a double click on the moved divider restores the equal split.
#[gpui::test]
fn double_click_on_the_moved_divider_restores_equal_panes(cx: &mut TestAppContext) {
    let (entity, cx) = dual_window(cx, false);
    let (left, middle) = divider_at(&entity, cx);
    let equal = cx.update(|window, cx| entity.read(cx).geometry(window).panes[0].width);
    let at = point(px(left + 4.), px(middle));
    let to = point(px(left + 104.), px(middle));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let widened = cx.update(|window, cx| entity.read(cx).geometry(window).panes[0].width);
    assert!(
        (widened - equal - 100.).abs() < 1.,
        "the drag widened {equal} to {widened}"
    );
    for click_count in [1, 2] {
        cx.simulate_event(gpui::MouseDownEvent {
            button: MouseButton::Left,
            position: to,
            modifiers: Modifiers::none(),
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            button: MouseButton::Left,
            position: to,
            modifiers: Modifiers::none(),
            click_count,
        });
        cx.run_until_parked();
    }
    let (split, reset) = cx.update(|window, cx| {
        let shell = entity.read(cx);
        (
            shell.launch.settings.split,
            shell.geometry(window).panes[0].width,
        )
    });
    assert_eq!(split, 0.5, "the double click left split {split}");
    assert!(
        (reset - equal).abs() < 0.5,
        "the double click left the left pane {reset}, not {equal}"
    );
}
