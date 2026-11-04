//! Source-only mounted regression proposal for cold join findings B2 and B3.
use super::*;
#[path = "frame_feedback_join_fixture.rs"]
mod fixture;
use crate::window::startup::View;
use fixture::*;
use gpui::TestAppContext;

#[derive(Clone, Copy)]
enum Excluded {
    Grid,
    Menu,
    SettingsModal,
}

fn legacy_minimum(cx: &mut TestAppContext, excluded: Excluded) {
    let (shell, cx, mut requests) = mount(cx, matches!(excluded, Excluded::Menu));
    match excluded {
        Excluded::Grid => edit(&shell, cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        }),
        Excluded::Menu => open_menu(&shell, cx, &mut requests),
        Excluded::SettingsModal => edit(&shell, cx, |shell, window, cx| {
            shell.open_settings(window, cx)
        }),
    }
    paint(cx);
    begin(&shell, cx);
    step(cx, crate::pane::swap::CAP_MS);
    shell.read_with(cx, |shell, _| {
        assert!(shell.pane(0).unwrap().swap.fell_back)
    });
    let loading_receipt = submit(cx);
    let since = shell.read_with(cx, |shell, cx| {
        match excluded {
            Excluded::Grid => assert_eq!(
                shell.panes[0].as_ref().unwrap().drawn_view,
                Some(View::Grid)
            ),
            Excluded::Menu => assert_eq!(shell.menu.captured.as_ref().unwrap().side, 1),
            Excluded::SettingsModal => {
                assert!(shell.settings.opened && shell.card_holds_keyboard(cx))
            }
        }
        let pane = shell.pane(0).unwrap();
        assert!(
            pane.swap.mark.counters.frames.get() > 0,
            "actual Root painted the mark"
        );
        assert_eq!(pane.swap.observed.borrow().frames, 0);
        assert_eq!(pane.swap.observed.borrow().outstanding_loading, 0);
        assert_eq!(
            pane.swap.mark_since,
            Some(pane.now()),
            "B2 legacy minimum starts at real excluded draw"
        );
        pane.swap.mark_since.unwrap()
    });
    let state = readback(&shell, cx, 0);
    let coverage = state.get("presentation").unwrap();
    assert_eq!(
        coverage
            .get("coverage")
            .and_then(flea::jsondoc::Json::as_str),
        Some("unavailable")
    );
    assert_eq!(coverage.get("frames"), Some(&flea::jsondoc::Json::Null));
    assert_eq!(
        coverage.get("exhaustive"),
        Some(&flea::jsondoc::Json::Bool(false))
    );
    terminal(cx, loading_receipt);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.pane(0).unwrap().swap.observed.borrow().frames,
            0,
            "excluded paint is not terminal coverage"
        );
    });
    ready(&shell, cx);
    step(cx, 1);
    paint(cx);
    whole(&shell, cx, false);
    step(cx, 98);
    paint(cx);
    whole(&shell, cx, false);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now()),
        since + 99
    );
    step(cx, 1);
    paint(cx);
    whole(&shell, cx, true);
    close(&shell, cx);
}

#[gpui::test]
fn join_grid_loading_keeps_legacy_minimum(cx: &mut TestAppContext) {
    legacy_minimum(cx, Excluded::Grid);
}

#[gpui::test]
fn join_menu_loading_keeps_legacy_minimum(cx: &mut TestAppContext) {
    legacy_minimum(cx, Excluded::Menu);
}

#[gpui::test]
fn join_settings_modal_loading_keeps_legacy_minimum(cx: &mut TestAppContext) {
    legacy_minimum(cx, Excluded::SettingsModal);
}

#[gpui::test]
fn join_return_to_list_requires_matching_presented(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    edit(&shell, cx, |shell, window, cx| {
        shell.set_view(View::Grid, window, cx)
    });
    begin(&shell, cx);
    step(cx, 151);
    let grid = submit(cx);
    assert!(shell.read_with(cx, |shell, _| shell
        .pane(0)
        .unwrap()
        .swap
        .mark_since
        .is_some()));
    edit(&shell, cx, |shell, window, cx| {
        shell.set_view(View::List, window, cx)
    });
    let list = submit(cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().drawn_view,
            Some(View::List)
        );
        assert_eq!(
            shell.pane(0).unwrap().swap.mark_since,
            None,
            "legacy draw cannot acknowledge eligible List"
        );
        let submitted = shell
            .presentation
            .pending
            .iter()
            .find(|frame| frame.id == list)
            .unwrap();
        assert!(submitted.draw.panes[0].as_ref().unwrap().loading);
    });
    ready(&shell, cx);
    step(cx, 1);
    paint(cx);
    whole(&shell, cx, false);
    terminal(cx, grid);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.pane(0).unwrap().swap.mark_since),
        None,
        "delayed Grid terminal must not acknowledge waiting List loading"
    );
    whole(&shell, cx, false);
    let arrival = shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now());
    terminal(cx, list);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.pane(0).unwrap().swap.mark_since),
        Some(arrival)
    );
    step(cx, 99);
    paint(cx);
    whole(&shell, cx, false);
    step(cx, 1);
    paint(cx);
    whole(&shell, cx, true);
    presented_ready_list(&shell, cx);
    close(&shell, cx);
}

#[gpui::test]
fn join_dual_cover_stages_only_actual_uncovered_pane(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx, true);
    cover_right(&shell, cx);
    begin(&shell, cx);
    step(cx, 151);
    let id = submit(cx);
    let (draw, identity, generation) = shell.read_with(cx, |shell, _| {
        let submitted = shell
            .presentation
            .pending
            .iter()
            .find(|frame| frame.id == id)
            .expect("B3 uncovered neighbor must associate the actual Window receipt");
        assert!(submitted.draw.id > 0);
        assert!(
            submitted.draw.panes[1].is_none(),
            "covered side has no List metadata"
        );
        let frame = submitted.draw.panes[0].as_ref().unwrap();
        let pane = shell.pane(0).unwrap();
        assert_eq!(frame.content.identity, pane.identity());
        assert_eq!(frame.content.generation, pane.list_requests);
        assert!(std::rc::Rc::ptr_eq(&frame.content.held, &pane.held));
        assert!(frame.loading);
        assert_eq!(
            pane.swap.mark_since, None,
            "uncovered List must wait for Presented"
        );
        (
            submitted.draw.id,
            frame.content.identity,
            frame.content.generation,
        )
    });
    let left = readback(&shell, cx, 0);
    let right = readback(&shell, cx, 1);
    assert_eq!(
        left.get("presentation")
            .unwrap()
            .get("coverage")
            .and_then(flea::jsondoc::Json::as_str),
        Some("terminal-stream")
    );
    assert_eq!(
        right
            .get("presentation")
            .unwrap()
            .get("coverage")
            .and_then(flea::jsondoc::Json::as_str),
        Some("unavailable")
    );
    assert_eq!(
        right.get("presentation").unwrap().get("frames"),
        Some(&flea::jsondoc::Json::Null)
    );
    ready(&shell, cx);
    step(cx, 1);
    paint(cx);
    whole(&shell, cx, false);
    let arrival = shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now());
    terminal(cx, id);
    shell.read_with(cx, |shell, _| {
        let pane = shell.pane(0).unwrap();
        let observed = pane.swap.observed.borrow();
        assert_eq!(observed.frames, 1, "one actual matching terminal");
        assert_eq!(observed.last.as_ref().unwrap().identity, identity);
        assert_eq!(observed.last.as_ref().unwrap().generation, generation);
        assert_eq!(pane.swap.mark_since, Some(arrival));
        assert_eq!(shell.pane(1).unwrap().swap.mark_since, None);
        assert!(!shell
            .presentation
            .pending
            .iter()
            .any(|frame| frame.id == id && frame.draw.id == draw));
    });
    terminal(cx, id);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .pane(0)
            .unwrap()
            .swap
            .observed
            .borrow()
            .frames),
        1,
        "duplicate feedback cannot add a frame"
    );
    step(cx, 99);
    paint(cx);
    whole(&shell, cx, false);
    step(cx, 1);
    paint(cx);
    whole(&shell, cx, true);
    presented_ready_list(&shell, cx);
    close(&shell, cx);
}

#[gpui::test]
fn join_hidden_secondary_cannot_reuse_legacy_minimum_on_restore(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx, true);
    edit(&shell, cx, |shell, window, cx| {
        shell.open_settings(window, cx);
        assert!(shell.navigate(
            1,
            "/right-next".into(),
            None,
            Default::default(),
            window,
            cx
        ));
    });
    step(cx, 151);
    paint(cx);
    let (identity, generation, legacy_since) = edit(&shell, cx, |shell, window, cx| {
        assert_eq!(shell.view, View::Dual);
        assert!(shell.settings.opened && shell.card_holds_keyboard(cx));
        assert!(shell.presentation.observer.is_some());
        assert!(shell.slots.covers[1].is_none());
        assert!(shell.mark_drawn(1));
        assert!(!shell.feedback_eligible(1, window, cx));
        assert!(shell.presentation.legacy_loading[1]);
        let pane = shell.pane(1).unwrap();
        assert!(pane.busy() && pane.swap.fell_back);
        assert_eq!(pane.swap.observed.borrow().frames, 0);
        assert!(pane
            .swap
            .mark_since
            .is_some_and(|since| since <= pane.now()));
        (
            pane.identity(),
            pane.list_requests,
            pane.swap.mark_since.unwrap(),
        )
    });
    edit(&shell, cx, |shell, window, cx| {
        shell.set_view(View::List, window, cx);
    });
    paint(cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.view, View::List);
        assert!(shell.shown_pane(1).is_none());
        assert!(
            !shell.mark_drawn(1),
            "leaving Dual actually hides the secondary"
        );
        assert_eq!(shell.panes[1].as_ref().unwrap().drawn_view, None);
        assert_eq!(shell.pane(1).unwrap().identity(), identity);
        assert!(shell.pane(1).unwrap().busy());
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    paint(cx);
    shell.read_with(cx, |shell, cx| {
        assert!(!shell.settings.opened && !shell.card_holds_keyboard(cx));
    });
    step(cx, crate::pane::swap::MIN_SHOW_MS + 1);
    edit(&shell, cx, |shell, window, cx| {
        shell.set_view(View::Dual, window, cx);
    });
    paint(cx);
    edit(&shell, cx, |shell, window, cx| {
        assert_eq!(shell.view, View::Dual);
        assert!(shell.mark_drawn(1));
        assert!(shell.feedback_eligible(1, window, cx));
        let pane = shell.pane(1).unwrap();
        assert_eq!(pane.identity(), identity);
        assert_eq!(pane.list_requests, generation);
        assert!(pane.now() > legacy_since + crate::pane::swap::MIN_SHOW_MS);
        assert_eq!(pane.swap.observed.borrow().frames, 0);
        assert_eq!(
            pane.swap.mark_since, None,
            "restored eligible secondary must await fresh Presented, not reuse legacy minimum"
        );
    });
    let id = submit(cx);
    let arrival = shell.read_with(cx, |shell, _| {
        let submitted = shell
            .presentation
            .pending
            .iter()
            .find(|frame| frame.id == id)
            .unwrap();
        let frame = submitted.draw.panes[1].as_ref().unwrap();
        assert!(frame.loading);
        assert_eq!(frame.content.identity, identity);
        assert_eq!(frame.content.generation, generation);
        let pane = shell.pane(1).unwrap();
        assert_eq!(
            pane.swap.mark_since, None,
            "submission alone cannot acknowledge Loading"
        );
        pane.now()
    });
    terminal(cx, id);
    shell.read_with(cx, |shell, _| {
        let pane = shell.pane(1).unwrap();
        assert_eq!(pane.swap.observed.borrow().frames, 1);
        assert_eq!(pane.swap.mark_since, Some(arrival));
    });
    close(&shell, cx);
}
