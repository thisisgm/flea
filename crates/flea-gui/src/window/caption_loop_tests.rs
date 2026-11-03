//! Caption timers follow the pane the renderer draws, through the shipped root overlay.
use crate::{
    backend::Backend,
    g2::fb1::hero::EmptyHero,
    pane::states::Kind,
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use flea::ui_logic_fb1::empty::{PHRASES, ROTATE_MS};
use gpui::{AppContext, Context, Entity, TestAppContext, Window, WindowHandle};
use std::time::Duration;

const FRAME: u64 = 16;

/// One platform frame; returns the frame callbacks it ran.
fn tick(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> usize {
    cx.executor().advance_clock(Duration::from_millis(FRAME));
    cx.run_until_parked();
    let asked = handle
        .update(cx, |_, window, cx| window.simulate_next_frame(cx))
        .unwrap();
    cx.run_until_parked();
    asked
}

fn ticks(cx: &mut TestAppContext, handle: WindowHandle<Root>, ms: u64) -> usize {
    (0..ms / FRAME).map(|_| tick(cx, handle)).sum()
}

fn open(cx: &mut TestAppContext, view: View, focus: usize, kinds: [Kind; 2]) -> WindowHandle<Root> {
    let (backend, requests) = Backend::test_channel();
    let (spare, spare_requests) = Backend::test_channel();
    let handle = cx.add_window(|window, cx| {
        window.set_rem_size(gpui::px(14.));
        let shell = cx.new(|cx| {
            cx.on_release(move |_, _| drop((requests, spare_requests)))
                .detach();
            let mut shell = shell_with(backend, cx);
            shell.panes[1] = shell_with(spare, cx).panes[0].take();
            shell.view = view;
            shell.focus_side = focus;
            for (host, kind) in shell.panes.iter_mut().zip(kinds) {
                host.as_mut().unwrap().pane.state.kind = kind;
            }
            shell
        });
        Root { shell }
    });
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.run_until_parked();
    handle
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn hero(cx: &mut TestAppContext, handle: WindowHandle<Root>, side: usize) -> Entity<EmptyHero> {
    edit(cx, handle, |shell, _, _| {
        shell.surfaces[side].hero.clone().unwrap()
    })
}

fn quiet(cx: &mut TestAppContext, hero: &Entity<EmptyHero>) -> (usize, usize, usize) {
    hero.read_with(cx, |hero, _| {
        (
            hero.counters.renders.get(),
            hero.counters.frames.get(),
            hero.counters.timers.get(),
        )
    })
}

#[gpui::test]
fn fb1_a_hidden_dual_pane_stops_its_caption_and_reappears_on_the_first_phrase(
    cx: &mut TestAppContext,
) {
    let handle = open(cx, View::Dual, 0, [Kind::Ready, Kind::Empty]);
    ticks(cx, handle, ROTATE_MS + 2400);
    let hero = hero(cx, handle, 1);
    assert_eq!(hero.read_with(cx, |hero, _| hero.rotation().index()), 1);
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx)
    });
    tick(cx, handle);
    hero.read_with(cx, |hero, _| {
        assert!(!hero.rotation().visible());
        assert_eq!(hero.rotation().next_deadline(), None);
    });
    let before = quiet(cx, &hero);
    ticks(cx, handle, 3 * PHRASES.len() as u64 * ROTATE_MS);
    assert_eq!(quiet(cx, &hero), before);
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(View::Dual, window, cx)
    });
    cx.run_until_parked();
    tick(cx, handle);
    assert_eq!(
        hero.read_with(cx, |hero, _| (
            hero.rotation().visible(),
            hero.rotation().index(),
            hero.rotation().beats()
        )),
        (true, 0, 0)
    );
}

#[gpui::test]
fn fb1_leaving_dual_with_focus_on_pane_one_keeps_only_the_drawn_pane_looping(
    cx: &mut TestAppContext,
) {
    let handle = open(cx, View::Dual, 1, [Kind::Empty, Kind::Empty]);
    ticks(cx, handle, ROTATE_MS + 2400);
    let (drawn, left) = (hero(cx, handle, 0), hero(cx, handle, 1));
    for hero in [&drawn, &left] {
        assert_eq!(hero.read_with(cx, |hero, _| hero.rotation().index()), 1);
    }
    edit(cx, handle, |shell, window, cx| {
        assert_eq!(shell.focus_side, 1);
        shell.set_view(View::List, window, cx);
        assert!(shell.shown_pane(0).is_some() && shell.shown_pane(1).is_none());
    });
    tick(cx, handle);
    assert!(!left.read_with(cx, |hero, _| hero.rotation().visible()));
    let (quiet_left, beats) = (
        quiet(cx, &left),
        drawn.read_with(cx, |hero, _| hero.rotation().beats()),
    );
    ticks(cx, handle, 3 * PHRASES.len() as u64 * ROTATE_MS);
    assert_eq!(quiet(cx, &left), quiet_left, "the undrawn pane stays quiet");
    assert_eq!(
        drawn.read_with(cx, |hero, _| hero.rotation().beats()),
        beats + 3 * PHRASES.len(),
        "the drawn pane keeps looping"
    );
}

/// The pane's caption words never step, and the window asks for no frame, when the pane is not Empty, not drawn or hidden.
#[gpui::test]
fn fb1_pane_words_stop_when_not_empty_not_drawn_or_hidden(cx: &mut TestAppContext) {
    let cases = [
        ("not empty", [Kind::Ready, Kind::Ready], 0, false),
        ("not drawn", [Kind::Ready, Kind::Empty], 1, false),
        ("hidden", [Kind::Empty, Kind::Ready], 0, true),
    ];
    for (case, kinds, side, hide) in cases {
        let handle = open(cx, View::List, 0, kinds);
        ticks(cx, handle, 2400);
        let hero = edit(cx, handle, |shell, _, _| shell.surfaces[side].hero.clone());
        if hide {
            cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Hidden);
            tick(cx, handle);
        }
        let before = hero.as_ref().map(|hero| quiet(cx, hero));
        let mut frames = 0;
        for _ in 0..10_000 / ROTATE_MS {
            frames += ticks(cx, handle, ROTATE_MS);
            let (stepped, beat) = edit(cx, handle, |shell, window, cx| {
                (
                    shell.pane_empty_beat(side, false, window, cx),
                    shell.panes[side].as_ref().unwrap().pane.state.beat,
                )
            });
            assert_eq!((stepped, beat), (false, 0), "{case}");
        }
        frames += ticks(cx, handle, 10_000 % ROTATE_MS);
        assert_eq!(frames, 0, "{case}: no frame requests over 10 s");
        assert_eq!(
            hero.as_ref().map(|hero| quiet(cx, hero)),
            before,
            "{case}: no hero render, frame or timer over 10 s"
        );
        if let Some(hero) = &hero {
            assert_eq!(
                hero.read_with(cx, |hero, _| hero
                    .rotation()
                    .next_deadline()
                    .filter(|_| !hide)),
                None,
                "{case}"
            );
        }
    }
}
