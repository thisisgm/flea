//! A selection wider than the held rows still drags: the source backend names its paths by index and the drop moves them all.
use super::mounted_tests::{centre, drag, frame, transfer, Root};
use super::vf3_tests::{crumb_point, open};
use crate::list::bench;
use gpui::VisualTestContext;
use std::rc::Rc;

const WIDE: usize = 2400;

fn dual(
    cx: &mut gpui::TestAppContext,
    source: &Root,
    target: &Root,
) -> (
    gpui::WindowHandle<crate::state::Shell>,
    impl FnMut() -> Option<String>,
    impl FnMut() -> Option<String>,
) {
    let (handle, source_lines) = open(cx, source.src());
    let (backend, target_lines) = crate::backend::Backend::test_lines();
    handle
        .update(cx, |shell, window, cx| {
            let source = shell.panes[0].as_mut().unwrap();
            let rows = source.pane.held.rows.clone();
            source.pane.held = Rc::new(crate::pane::Held {
                listing: 2,
                start: 0,
                rows: rows.clone(),
                total: WIDE,
            });
            source.selected = (0..WIDE).collect();
            let mut right =
                crate::window::pane::PaneHost::new(target.src(), None, 646., &shell.locale);
            right.backend = Some(backend);
            right.pane.use_executor(cx.background_executor().clone());
            right
                .pane
                .receive(flea::protocol::Reply::Listed {
                    path: target.src(),
                    total: rows.len(),
                })
                .unwrap();
            right
                .pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 2,
                    rows,
                })
                .unwrap();
            let held = &right.pane.held;
            right.pane.held = Rc::new(crate::pane::Held {
                listing: held.listing,
                start: held.start,
                rows: held.rows.clone(),
                total: WIDE,
            });
            right.changed(&shell.locale);
            right
                .nav
                .after_reply(&mut right.pane, &target.src(), &mut right.selected);
            right.selected = (0..WIDE).collect();
            right.list.borrow_mut().typeface = bench::bench_typeface();
            shell.panes[1] = Some(right);
            shell.launch.paths[1] = target.src();
            shell.path_labels[1] = target.src().into();
            shell.set_view(crate::window::startup::View::Dual, window, cx);
        })
        .unwrap();
    frame(cx, handle);
    (handle, source_lines, target_lines)
}

fn point_on(
    cx: &mut gpui::TestAppContext,
    handle: gpui::WindowHandle<crate::state::Shell>,
    side: usize,
) -> gpui::Point<gpui::Pixels> {
    handle
        .read_with(cx, |shell, _| {
            let input = &shell.panes[side].as_ref().unwrap().input;
            let bounds = input.frame.unwrap().bounds;
            (0..f32::from(bounds.size.height) as usize)
                .map(|dy| {
                    gpui::point(
                        bounds.left() + gpui::px(60.),
                        bounds.top() + gpui::px(dy as f32),
                    )
                })
                .find(|point| input.hit(*point) == Some(1))
                .unwrap()
                + gpui::point(gpui::px(0.), gpui::px(8.))
        })
        .unwrap()
}

#[test]
fn vf3_wide_selection_drags_by_index_into_the_owned_parent() {
    let root = Root::new("wide");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let held = &host.pane.held;
            host.pane.held = Rc::new(crate::list::pane::Held {
                listing: held.listing,
                total: WIDE,
                start: held.start,
                rows: held.rows.clone(),
            });
            host.selected = (0..WIDE).collect();
            host.input.selection.sync(&host.selected);
        })
        .unwrap();
    frame(&mut cx, handle);
    let from = centre(&mut cx, handle, 1);
    let parent = crumb_point(&mut cx, handle, &root, false);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, from, parent);
    vcx.run_until_parked();
    let sent: Vec<_> = std::iter::from_fn(&mut lines).collect();
    let rows = (0..WIDE).map(|row| row.to_string()).collect::<Vec<_>>();
    let ask = format!(r#"{{"c":"paths","rows":[{}]}}"#, rows.join(","));
    assert!(
        sent.contains(&ask),
        "the lift must ask the backend for every selected row: {:?}",
        sent.iter()
            .map(|line| &line[..line.len().min(80)])
            .collect::<Vec<_>>()
    );
    let paths: Vec<String> = ["alpha".to_owned(), "b.txt".to_owned()]
        .into_iter()
        .chain((2..WIDE).map(|n| format!("f{n:04}.txt")))
        .map(|name| format!("{}/{name}", root.src()))
        .collect();
    handle
        .update(&mut vcx, |shell, window, cx| {
            shell.receive(0, crate::backend::Event::Paths(paths), window, cx)
        })
        .unwrap();
    vcx.run_until_parked();
    let transfer = transfer(&mut vcx, handle, &mut lines);
    let dest = root.src().trim_end_matches("/src").to_owned();
    assert!(transfer.contains(r#""op":"move""#), "same device moves");
    assert!(
        transfer.contains(&format!(r#""dest":"{dest}""#)),
        "into the parent"
    );
    assert_eq!(
        transfer.matches(&format!("\"{}/", root.src())).count(),
        WIDE,
        "every selected row moves"
    );
    handle
        .read_with(&vcx, |shell, _| {
            assert!(
                !shell.panes[0].as_ref().unwrap().nav.gate.clip_pending,
                "the answer hands the paths slot back"
            )
        })
        .unwrap();
}

#[test]
fn vf3_wide_opposite_pane_lift_never_strands_the_first_paths_gate() {
    let source = Root::new("ask-source");
    let target = Root::new("ask-target");
    let mut cx = bench::real_cx();
    let (handle, _left, _right) = dual(&mut cx, &source, &target);
    let from = point_on(&mut cx, handle, 0);
    let other = point_on(&mut cx, handle, 1);
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    for (index, point) in [from, other].into_iter().enumerate() {
        cx.simulate_mouse_move(point, None, gpui::Modifiers::default());
        cx.simulate_mouse_down(point, gpui::MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(
            point + gpui::point(gpui::px(20.), gpui::px(0.)),
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        if index == 0 {
            handle
                .read_with(&cx, |shell, _| {
                    assert!(shell.panes[0].as_ref().unwrap().nav.gate.clip_pending)
                })
                .unwrap();
        } else {
            handle
                .read_with(&cx, |shell, _| {
                    assert!(shell.panes[1].as_ref().unwrap().nav.gate.clip_pending);
                })
                .unwrap();
        }
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_up(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    }
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.receive(0, crate::backend::Event::Paths(Vec::new()), window, cx);
            assert!(
                !shell.panes[0].as_ref().unwrap().nav.gate.clip_pending,
                "the first reply must release its paths gate after an opposite-pane attempt"
            );
            assert!(shell.panes[1].as_ref().unwrap().nav.gate.clip_pending);
            shell.receive(1, crate::backend::Event::Paths(Vec::new()), window, cx);
            assert!(!shell.panes[1].as_ref().unwrap().nav.gate.clip_pending);
        })
        .unwrap();
}

#[test]
fn vf3_wide_release_never_retargets_through_a_changed_ancestor() {
    let source = Root::new("wide-source");
    let target = Root::new("wide-dest-a");
    let changed = Root::new("wide-dest-b");
    let mut cx = bench::real_cx();
    let (handle, mut source_lines, mut target_lines) = dual(&mut cx, &source, &target);
    let from = point_on(&mut cx, handle, 0);
    let parent = handle
        .read_with(&cx, |shell, cx| {
            let view = shell.chrome.view.as_ref().unwrap().read(cx).path.read(cx);
            let paint = view.paint.borrow();
            let strip = &view.strips[2];
            let parent = target.src().trim_end_matches("/src").to_owned();
            let index = strip
                .crumbs()
                .iter()
                .position(|crumb| crumb.path == parent)
                .unwrap();
            let bounds = paint.strips[2].unwrap();
            let pad = crate::path::paint::strip_pad(true, &paint.scale());
            gpui::point(
                bounds.left() + gpui::px(pad + strip.extent(index).unwrap().0 + 2.),
                bounds.top() + bounds.size.height / 2.,
            )
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut cx, from, parent);
    cx.run_until_parked();
    handle
        .update(&mut cx, |_, window, cx| {
            assert!(
                super::join::pending_drop(window, 0, cx),
                "initial wide breadcrumb release must queue a pending drop"
            );
        })
        .unwrap();
    handle
        .update(&mut cx, |shell, window, cx| {
            shell
                .step(
                    1,
                    crate::window::navigate::Step::OpenPlace(changed.src()),
                    window,
                    cx,
                )
                .unwrap();
            shell.receive(
                1,
                crate::backend::Event::Reply(flea::protocol::Reply::Listed {
                    path: changed.src(),
                    total: 0,
                }),
                window,
                cx,
            );
            shell.receive(
                1,
                crate::backend::Event::Reply(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: Vec::new(),
                }),
                window,
                cx,
            );
        })
        .unwrap();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let paths = (0..WIDE)
        .map(|index| format!("{}/f{index:04}.txt", source.src()))
        .collect();
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.receive(0, crate::backend::Event::Paths(paths), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    let sent: Vec<_> = std::iter::from_fn(&mut source_lines)
        .chain(std::iter::from_fn(&mut target_lines))
        .collect();
    handle
        .update(&mut cx, |_, window, cx| {
            assert!(!super::join::pending_drop(window, 0, cx));
        })
        .unwrap();
    assert!(
        !sent
            .iter()
            .any(|line| line.contains("collisions") || line.contains("transfer")),
        "navigation after release must cancel the pending drop rather than retarget it: {sent:?}"
    );
}

#[test]
fn vf3_wide_window_cleanup_forgets_its_pending_paths_ask() {
    let source = Root::new("closing-source");
    let target = Root::new("closing-target");
    let mut cx = bench::real_cx();
    let (handle, _left, _right) = dual(&mut cx, &source, &target);
    let from = point_on(&mut cx, handle, 0);
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        from + gpui::point(gpui::px(20.), gpui::px(0.)),
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    handle
        .update(&mut cx, |shell, window, cx| {
            assert!(shell.panes[0].as_ref().unwrap().nav.gate.clip_pending);
            shell.forget_file_drags(window, cx);
            assert!(
                !shell.drag_paths(0, &[], window, cx),
                "closed-window requests must be gone"
            );
        })
        .unwrap();
}
