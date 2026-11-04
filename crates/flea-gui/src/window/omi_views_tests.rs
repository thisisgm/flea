//! A held listing draws Omi in List, Grid and both Dual panes (ruling 145); past its wait the pane yields it to the alert sentence.
use crate::{
    backend::Backend,
    ipc::{pane::PaneAdapter, Adapter},
    pane::swap::{Options, CAP_MS},
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use flea::jsondoc::Json;
use gpui::{AppContext, TestAppContext, VisualTestContext, WindowHandle};
use std::time::Duration;

type Keep = Vec<Box<dyn FnMut() -> Option<(usize, usize)>>>;

fn open(cx: &mut TestAppContext, view: View) -> (WindowHandle<Root>, Keep) {
    let (backend, first) = Backend::test_channel();
    let (spare, second) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            if view == View::Dual {
                shell.panes[1] = shell_with(spare, cx).panes[0].take();
                shell.view = View::Dual;
            }
            shell
        }),
    });
    if view == View::Grid {
        shell(cx, handle, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        });
    }
    (handle, vec![Box::new(first), Box::new(second)])
}

fn shell<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    update: impl FnOnce(&mut Shell, &mut gpui::Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .clone()
                .update(cx, |shell, cx| update(shell, window, cx))
        })
        .unwrap()
}

// Starts a listing on `side` and spends its cap, so the pane falls back to its loading state.
fn hold(cx: &mut TestAppContext, handle: WindowHandle<Root>, side: usize) {
    shell(cx, handle, |shell, window, cx| {
        assert!(shell.navigate(side, "/slow".into(), None, Options::default(), window, cx));
        let host = shell.panes[side].as_mut().unwrap();
        assert!(host.pane.tick(host.pane.now() + CAP_MS));
        host.changed(&shell.locale);
        cx.notify();
    });
}

fn frames(cx: &mut TestAppContext, handle: WindowHandle<Root>, count: usize) {
    for _ in 0..count {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
}

// The pane's swapState describe, the reader a native run polls for its loading mark.
fn mark(cx: &mut TestAppContext, handle: WindowHandle<Root>, side: usize) -> Json {
    let reply = shell(cx, handle, |shell, _, _| {
        PaneAdapter {
            pane: &shell.panes[side].as_ref().unwrap().pane,
            catalog: &shell.locale.catalog,
        }
        .call("swapState", &[])
        .unwrap()
    });
    flea::jsondoc::parse(reply.stdout.trim())
        .unwrap()
        .get("mark")
        .unwrap()
        .clone()
}

fn assert_omi_paints(cx: &mut TestAppContext, handle: WindowHandle<Root>, side: usize, view: &str) {
    let described = mark(cx, handle, side);
    let field = |key: &str| described.get(key).cloned();
    assert_eq!(
        field("mode").as_ref().and_then(Json::as_str),
        Some("thinking"),
        "{view} side {side}: ruling 145 requires Omi while a listing loads (EY-354)"
    );
    assert_eq!(
        field("shown").as_ref().and_then(Json::as_bool),
        Some(true),
        "{view} side {side}"
    );
    let painted = field("frames")
        .as_ref()
        .and_then(Json::as_f64)
        .unwrap_or(0.);
    assert!(
        painted >= 8.,
        "{view} side {side}: Omi painted {painted} frames"
    );
    let slot = shell(cx, handle, |shell, _, _| {
        shell.panes[side].as_ref().unwrap().pane.swap.mark.slot()
    });
    assert!(
        f32::from(slot.size.width) > 0. && f32::from(slot.size.height) > 0.,
        "{view} side {side}: the mark slot was never placed"
    );
}

#[gpui::test]
fn list_grid_and_dual_held_listings_paint_omi(cx: &mut TestAppContext) {
    for (view, sides) in [(View::List, 1), (View::Grid, 1), (View::Dual, 2)] {
        let (handle, _keep) = open(cx, view);
        for side in 0..sides {
            hold(cx, handle, side);
        }
        frames(cx, handle, 10);
        for side in 0..sides {
            assert_omi_paints(cx, handle, side, &format!("{view:?}"));
        }
    }
}

#[gpui::test]
fn past_its_wait_the_pane_hides_omi_for_the_alert_sentence(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, View::List);
    hold(cx, handle, 0);
    frames(cx, handle, 10);
    assert_omi_paints(cx, handle, 0, "List");
    cx.executor().advance_clock(Duration::from_millis(
        crate::window::startup::LISTING_WAIT_MS,
    ));
    cx.run_until_parked();
    frames(cx, handle, 2);
    assert_eq!(
        mark(cx, handle, 0).get("shown").and_then(Json::as_bool),
        Some(false)
    );
    let kind = shell(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.state.kind
    });
    assert_eq!(kind, crate::pane::states::Kind::Waiting);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    assert!(
        visual.debug_bounds("fb1-state-mark").is_some(),
        "the waiting sentence stands on its alert mark"
    );
}
