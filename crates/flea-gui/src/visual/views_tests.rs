//! Capture readers against the painters that drew: List, Columns and Grid.
use super::*;
use crate::{backend::Backend, state::tests::shell_with};
use gpui::{TestAppContext, TestDispatcher};

#[test]
fn visual_capture_reads_the_grid_that_drew_instead_of_a_stale_list() {
    let mut context = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = context.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        for row in &mut std::rc::Rc::get_mut(&mut host.pane.held).unwrap().rows {
            row.name = "alpha.txt".into();
            row.mode = 0o100644;
        }
        host.changed(&shell.locale);
        shell.set_view(View::Grid, window, cx);
        shell
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        entity.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(host.drawn_view, Some(View::Grid));
            host.list.borrow_mut().drawn_cursor = Some(999);
            assert_eq!(shell.visual_drawn_cursor(), Some(0));
            let probe = Probe::default();
            shell.visual_view_geometry(&probe, window);
            let data = probe.json(&flea::jsondoc::Json::Null);
            let roles = data.get("roles").unwrap().as_array().unwrap();
            for role in ["surface", "row", "glyph", "ink", "label"] {
                assert!(
                    roles
                        .iter()
                        .any(|row| row.get("role").and_then(flea::jsondoc::Json::as_str)
                            == Some(role)),
                    "missing {role}"
                );
            }
            shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow_mut()
                .frame_held = None;
            assert_eq!(shell.visual_drawn_cursor(), None);
            shell.panes[0].as_mut().unwrap().drawn_view = None;
            assert_eq!(shell.visual_drawn_cursor(), None);
        });
    });
}

#[gpui::test]
fn visual_pending_includes_only_shown_columns_neighbour_requests(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, window) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.set_view(View::Columns, window, cx);
        shell
    });
    window.update(|_, cx| {
        entity.update(cx, |shell, _| {
            let before = shell.pending_requests();
            shell.panes[0]
                .as_mut()
                .unwrap()
                .columns
                .as_mut()
                .unwrap()
                .area
                .peeks
                .pending
                .track("capture-neighbour");
            assert_eq!(shell.pending_requests(), before + 1);
            shell.panes[0]
                .as_mut()
                .unwrap()
                .columns
                .as_mut()
                .unwrap()
                .area
                .peeks
                .pending
                .drop_one("capture-neighbour");
            assert_eq!(shell.pending_requests(), before);
            fn area(shell: &mut Shell) -> &mut crate::columns::Area {
                &mut shell.panes[0]
                    .as_mut()
                    .unwrap()
                    .columns
                    .as_mut()
                    .unwrap()
                    .area
            }
            // The entry hold arms the landing cap; ticking each due time leaves no Columns deadline armed.
            while let Some(due) = area(shell).landing.next_deadline() {
                area(shell).landing.tick(due);
            }
            assert_eq!(area(shell).next_deadline(), None);
            let idle = shell.pending_requests();
            assert_eq!(idle + 1, before, "the armed landing cap was drawing");
            area(shell).host.fallback_at = Some(1);
            assert_eq!(
                shell.pending_requests(),
                idle + 1,
                "an armed Columns deadline still draws"
            );
            area(shell).host.fallback_at = None;
            assert_eq!(shell.pending_requests(), idle);
            shell.view = View::List;
            let hidden = shell.pending_requests();
            area(shell).host.fallback_at = Some(1);
            assert_eq!(
                shell.pending_requests(),
                hidden,
                "a hidden Columns deadline draws nothing"
            );
            shell.panes[0]
                .as_mut()
                .unwrap()
                .columns
                .as_mut()
                .unwrap()
                .area
                .peeks
                .pending
                .track("hidden-neighbour");
            assert_eq!(shell.pending_requests(), hidden);
        });
    });
}

/// Frames the compositor showed before the scene, so no frame id the scene draws is one of them.
const MAPPED: u64 = 3;

/// Draws a frame, has the compositor number its present, then runs that frame's callbacks.
fn commit_frame(entity: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) -> usize {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        entity.update(cx, |shell, _| {
            shell.visual.compositor.as_mut().unwrap().committed += 1;
        });
        window.simulate_next_frame(cx)
    })
}

/// Has the compositor show every committed frame, then runs what its presented observer runs.
fn present_all(entity: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let frames = shell.visual.compositor.as_mut().unwrap();
            frames.presented = frames.committed;
            let stale = shell.visual_presented(window);
            if stale {
                cx.notify();
            }
            stale
        })
    })
}

#[gpui::test]
fn visual_receipt_waits_for_a_fresh_probe_after_a_newer_cached_submission(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[0].as_mut().unwrap().answered = true;
        shell.visual.compositor = Some(gpui::PresentedFrames {
            committed: MAPPED,
            presented: MAPPED,
            ..Default::default()
        });
        shell
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let spec = super::super::contract::Spec::parse(
                &std::process::id().to_string(),
                "g1-01-list-r0",
                r#"{"path":"/files","width":1100,"height":700}"#,
            )
            .unwrap();
            shell.apply_scene(spec, answer, window, cx);
        })
    });
    let awaited = |cx: &mut gpui::VisualTestContext| {
        entity.update(cx, |shell, _| {
            shell
                .visual
                .awaiting
                .as_ref()
                .map(|awaiting| awaiting.committed)
        })
    };
    assert!(commit_frame(&entity, cx) >= 1);
    assert_eq!(awaited(cx), Some(MAPPED + 1));
    // A newer frame from the compositor's cache is submitted and shown in place of the awaited one.
    entity.update(cx, |shell, _| {
        shell.visual.compositor.as_mut().unwrap().committed += 1;
    });
    assert!(
        present_all(&entity, cx),
        "a cached frame needs a fresh scene probe"
    );
    assert!(receive.try_recv().is_err(), "the old probe is stale");
    assert!(
        commit_frame(&entity, cx) >= 1,
        "the redraw queued scene work"
    );
    assert_eq!(
        awaited(cx),
        Some(MAPPED + 3),
        "the redraw probed the scene again"
    );
    assert!(!present_all(&entity, cx));
    let reply = receive.try_recv().unwrap().reply;
    let receipt = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert_eq!(
        receipt.get("frame").and_then(flea::jsondoc::Json::as_f64),
        Some((MAPPED + 3) as f64)
    );
}
