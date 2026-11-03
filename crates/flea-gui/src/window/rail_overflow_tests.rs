//! Every frame of a rail toggle through the real Root, in every view: no pane paint over the rail, no frame whose pane origin disagrees with the rail.
use super::*;
use crate::{
    backend::Event as BackendEvent,
    rail::Mode,
    window::{
        pane::PaneHost,
        root::Root,
        startup::{Settings, View},
    },
};
use gpui::{size, App, AppContext, Bounds, Context, Pixels, Window, WindowHandle};
use std::time::Duration;

type Handle = WindowHandle<Root>;

const VIEWS: [View; 4] = [View::List, View::Grid, View::Columns, View::Dual];

// ui.sh case_sidebar's window: a list of long names in a 1200 px float, the rail as the state document says.
fn launch(view: View, state: &str, cx: &mut TestAppContext) -> Handle {
    let (backend, receiver) = Backend::test_channel();
    let state = state.to_owned();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            cx.on_release(move |_, _| drop(receiver)).detach();
            shell.launch.settings = Settings::from_json(flea::uistate::from_file(&state));
            shell.panes[0].as_mut().unwrap().backend = None;
            if view == View::Dual {
                let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
                right.backend = None;
                shell.panes[1] = Some(right);
                shell.lifecycle.attach(1);
            }
            shell.view = view;
            shell.mount_rail(cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    edit(cx, handle, |shell, window, cx| {
        for (side, path) in [(0, "/files"), (1, "/right")] {
            if shell.panes[side].is_none() {
                continue;
            }
            if side == 0 {
                let pane = &mut shell.panes[side].as_mut().unwrap().pane;
                assert!(pane.begin(path.into(), Default::default(), pane.now()));
            }
            for reply in [
                flea::protocol::Reply::Listed {
                    path: path.into(),
                    total: 24,
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: (0..24)
                        .map(|at| flea::protocol::RowFacts {
                            name: format!("a-rather-long-file-name-number-{at:02}.txt"),
                            size: 5,
                            ..Default::default()
                        })
                        .collect(),
                },
            ] {
                shell.receive(side, BackendEvent::Reply(reply), window, cx);
            }
        }
        let producer = shell.rail.as_ref().unwrap().places.clone();
        producer.update(cx, |producer, cx| {
            producer.replace_sections(
                vec![RailSection {
                    heading: Heading::Key("places.heading.places"),
                    rows: vec![row("Home", "/home/test"), row("Trash", "trash:///")],
                }],
                cx,
            )
        });
    });
    resize(cx, handle, 1200.);
    handle
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

fn patch(cx: &mut TestAppContext, handle: Handle, places: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.change_settings(jsondoc::parse(places).unwrap(), window, cx)
    });
}

fn keys(cx: &mut TestAppContext, handle: Handle, keys: &str) {
    gpui::VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(keys);
}

fn pointer(cx: &mut TestAppContext, handle: Handle, x: f32) {
    gpui::VisualTestContext::from_window(handle.into(), cx).simulate_mouse_move(
        point(px(x), px(300.)),
        None,
        Modifiers::none(),
    );
}

fn resize(cx: &mut TestAppContext, handle: Handle, width: f32) {
    gpui::VisualTestContext::from_window(handle.into(), cx)
        .simulate_resize(size(px(width), px(700.)));
}

/// What a sweep saw: every fault with its frame, and whether a loading mark ever stood under the drawn rail.
#[derive(Default)]
struct Watch {
    faults: Vec<String>,
    mark_under_rail: bool,
}

impl Watch {
    // One native frame: the platform's request runs next-frame callbacks, then draws only if dirty.
    fn frame(&mut self, cx: &mut TestAppContext, handle: Handle, label: &str) {
        cx.simulate_frame_request(
            handle.into(),
            gpui::RequestFrameOptions {
                require_presentation: true,
                ..Default::default()
            },
        );
        handle
            .update(cx, |root, window, cx| self.check(root, label, window, cx))
            .unwrap();
    }

    // The mode or key frame first, then each 16 ms animation step after it.
    fn frames(&mut self, cx: &mut TestAppContext, handle: Handle, label: &str, count: usize) {
        self.frame(cx, handle, label);
        for step in 0..count {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.run_until_parked();
            self.frame(cx, handle, &format!("{label}+{step}"));
        }
    }

    fn check(&mut self, root: &Root, label: &str, window: &mut Window, cx: &mut App) {
        let shell = root.shell.read(cx);
        let geometry = shell.geometry(window);
        let host = shell.rail.as_ref().unwrap().host.read(cx);
        let at = cx.background_executor().now();
        let (mode, slide, drawn, width) = (
            host.rail.mode(),
            host.rail.slide(at),
            host.rail.drawn(at),
            host.width(),
        );
        let left = if mode == Mode::Overlay {
            geometry.rail.x + (slide - 1.) * width
        } else {
            geometry.rail.x
        };
        let body = Bounds::from_corners(
            point(px(left), px(geometry.rail.y)),
            point(px(left + width), px(geometry.rail.y + geometry.rail.height)),
        );
        let seen = body.intersect(&Bounds::new(point(px(0.), px(0.)), window.viewport_size()));
        let mut fault = |text: String| self.faults.push(format!("{label}: {text}"));
        let docked = if mode == Mode::Docked {
            geometry.rail.width
        } else {
            0.
        };
        if geometry.panes[0].x != docked || (mode == Mode::Docked && docked != width) {
            fault(format!(
                "{mode:?} pane origin {} against rail {docked} and host {width}",
                geometry.panes[0].x
            ));
        }
        for side in 0..2 {
            let Some(pane) = shell.panes[side].as_ref().filter(|pane| pane.drawn_list) else {
                continue;
            };
            let mask = pane.list.borrow().drawn.mask;
            if f32::from(mask.left()) + 0.01 < geometry.panes[side].x {
                fault(format!(
                    "list {side} clips at {mask:?}, left of its pane {:?}",
                    geometry.panes[side]
                ));
            }
        }
        if !drawn || f32::from(seen.size.width) < 0.5 {
            return;
        }
        let scale = window.scale_factor();
        let logical = |bounds: Bounds<gpui::ScaledPixels>| {
            Bounds::from_corners(
                point(px(bounds.origin.x.0 / scale), px(bounds.origin.y.0 / scale)),
                point(
                    px(bounds.bottom_right().x.0 / scale),
                    px(bounds.bottom_right().y.0 / scale),
                ),
            )
        };
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let near = |a: Pixels, b: Pixels| (f32::from(a) - f32::from(b)).abs() <= 1.;
        let quads: Vec<_> = window
            .painted_quads()
            .into_iter()
            .map(|quad| {
                (
                    quad.order,
                    logical(quad.bounds),
                    logical(quad.content_mask.bounds),
                    quad.background,
                )
            })
            .collect();
        let Some(ground) = quads.iter().find(|(_, bounds, _, background)| {
            near(bounds.left(), body.left())
                && near(bounds.right(), body.right())
                && near(bounds.top(), body.top())
                && *background == gpui::Background::from(theme.surface)
        }) else {
            fault(format!("the drawn rail at {body:?} painted no ground"));
            return;
        };
        for (order, bounds, mask, _) in &quads {
            let over = bounds.intersect(mask).intersect(&seen);
            let own =
                bounds.left() >= body.left() - px(1.) && bounds.right() <= body.right() + px(1.);
            if *order >= ground.0
                && !own
                && f32::from(over.size.width) * f32::from(over.size.height) > 0.5
            {
                fault(format!(
                    "pane paint over the rail: order {order} above ground {} at {bounds:?} in {mask:?}",
                    ground.0
                ));
            }
        }
        let marks = shell
            .panes
            .iter()
            .flatten()
            .map(|host| host.pane.swap.mark.clone())
            .chain(shell.column_loading_marks(window, cx).into_iter().flatten());
        for mark in marks {
            if mark.shown() && mark.slot().intersects(&seen) {
                self.mark_under_rail = true;
            }
        }
    }

    fn assert_clean(&self, sweep: &str) {
        assert!(
            self.faults.is_empty(),
            "{sweep}: {} faulty frames, first {:#?}",
            self.faults.len(),
            &self.faults[..self.faults.len().min(8)]
        );
    }
}

fn motion(reduced: bool, cx: &mut TestAppContext) -> &'static str {
    cx.update(|cx| cx.set_reduce_motion(reduced));
    if reduced {
        "reduced"
    } else {
        "normal"
    }
}

#[gpui::test]
fn ctrl_b_and_the_places_row_never_leave_pane_paint_over_the_docked_rail(cx: &mut TestAppContext) {
    for view in VIEWS {
        for reduced in [false, true] {
            let sweep = format!("{view:?} {}", motion(reduced, cx));
            let mut watch = Watch::default();
            let handle = launch(view, "{}", cx);
            watch.frames(cx, handle, "launch", 2);
            keys(cx, handle, "ctrl-b");
            watch.frames(cx, handle, "ctrl-b hide", 3);
            keys(cx, handle, "ctrl-b");
            watch.frames(cx, handle, "ctrl-b show", 3);
            patch(cx, handle, r#"{"places":{"rail":"hidden"}}"#);
            watch.frames(cx, handle, "places row hide", 2);
            patch(cx, handle, r#"{"places":{"rail":"shown"}}"#);
            watch.frames(cx, handle, "places row show", 2);
            resize(cx, handle, 520.);
            watch.frames(cx, handle, "narrow", 1);
            keys(cx, handle, "ctrl-b");
            resize(cx, handle, 1200.);
            watch.frames(cx, handle, "hide with a resize", 2);
            keys(cx, handle, "ctrl-b");
            watch.frames(cx, handle, "show", 2);
            edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
            // A choice that outlives the window: launched hidden, then shown.
            let handle = launch(view, r#"{"places":{"rail":"hidden"}}"#, cx);
            watch.frames(cx, handle, "relaunch hidden", 1);
            keys(cx, handle, "ctrl-b");
            watch.frames(cx, handle, "relaunch show", 3);
            edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
            watch.assert_clean(&sweep);
        }
    }
}

// ui.sh case_sidebar's auto-hide half: the switch, the edge reveal and its withdraw, Tab in, a resize mid-slide, Tab out.
fn overlay_sweep(watch: &mut Watch, handle: Handle, cx: &mut TestAppContext) {
    patch(cx, handle, r#"{"places":{"autoHide":true}}"#);
    watch.frames(cx, handle, "auto-hide on", 3);
    pointer(cx, handle, 1.);
    watch.frames(cx, handle, "edge", 18);
    pointer(cx, handle, 1000.);
    watch.frames(cx, handle, "away", 30);
    keys(cx, handle, "tab");
    watch.frames(cx, handle, "tab in", 4);
    resize(cx, handle, 520.);
    watch.frames(cx, handle, "resize mid-slide", 10);
    keys(cx, handle, "tab");
    watch.frames(cx, handle, "tab out", 12);
    patch(cx, handle, r#"{"places":{"autoHide":false}}"#);
    watch.frames(cx, handle, "auto-hide off", 2);
}

#[gpui::test]
fn the_overlay_reveal_and_withdraw_never_leave_pane_paint_over_the_rail(cx: &mut TestAppContext) {
    for view in VIEWS {
        for reduced in [false, true] {
            let sweep = format!("{view:?} {}", motion(reduced, cx));
            let mut watch = Watch::default();
            let handle = launch(view, "{}", cx);
            overlay_sweep(&mut watch, handle, cx);
            edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
            watch.assert_clean(&sweep);
        }
    }
}

#[gpui::test]
fn an_overlay_rail_stays_over_the_loading_mark_of_the_pane_under_it(cx: &mut TestAppContext) {
    for view in [View::Dual, View::Columns] {
        for reduced in [false, true] {
            let sweep = format!("{view:?} {}", motion(reduced, cx));
            let mut watch = Watch::default();
            let handle = launch(view, "{}", cx);
            resize(cx, handle, 520.);
            // A folder that never answers, so the left pane falls back to its loading mark.
            edit(cx, handle, |shell, window, cx| {
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                assert!(pane.begin("/slow".into(), Default::default(), pane.now()));
                shell.arm_transition(0, window, cx);
            });
            let mut waited = 0;
            while !edit(cx, handle, |shell, window, cx| {
                shell.mark_drawn(0)
                    || shell
                        .column_loading_marks(window, cx)
                        .iter()
                        .any(Option::is_some)
            }) {
                assert!(
                    waited < 200,
                    "{sweep}: the left pane never drew its loading mark"
                );
                waited += 1;
                cx.executor().advance_clock(Duration::from_millis(16));
                cx.run_until_parked();
                watch.frame(cx, handle, "loading");
            }
            patch(cx, handle, r#"{"places":{"autoHide":true}}"#);
            watch.frames(cx, handle, "auto-hide on", 2);
            keys(cx, handle, "tab");
            watch.frames(cx, handle, "tab in", 14);
            keys(cx, handle, "tab");
            watch.frames(cx, handle, "tab out", 12);
            edit(cx, handle, |shell, _, _| shell.stop_for_app_quit());
            assert!(
                watch.mark_under_rail,
                "{sweep}: no loading mark stood under the drawn rail, so the sweep proves nothing"
            );
            watch.assert_clean(&sweep);
        }
    }
}
