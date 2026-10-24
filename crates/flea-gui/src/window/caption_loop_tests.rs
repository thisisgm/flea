//! Caption timers follow the visibility of the pane that owns the hero.
use crate::{
    backend::Backend,
    pane::states::Kind,
    state::{tests::shell_with, Shell},
};
use gpui::{AppContext, TestAppContext, WindowHandle};
use std::time::Duration;

fn tick(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.simulate_next_frame(cx)
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn fb1_a_hidden_dual_pane_stops_its_caption_and_reappears_on_the_first_phrase(
    cx: &mut TestAppContext,
) {
    use crate::window::startup::View;
    use flea::ui_logic_fb1::empty::ROTATE_MS;

    let (backend, _requests) = Backend::test_channel();
    let (spare, _spare_requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = shell_with(spare, cx).panes[0].take();
        shell.view = View::Dual;
        shell.panes[1].as_mut().unwrap().pane.state.kind = Kind::Empty;
        shell
    });
    cx.run_until_parked();
    for _ in 0..(ROTATE_MS + 2400) / 16 {
        tick(cx, handle);
        cx.run_until_parked();
    }
    let hero = handle
        .update(cx, |shell, _, _| shell.surfaces[1].hero.clone().unwrap())
        .unwrap();
    assert_eq!(hero.read_with(cx, |hero, _| hero.rotation().index()), 1);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    tick(cx, handle);
    let before = hero.read_with(cx, |hero, _| {
        assert!(!hero.rotation().visible());
        assert_eq!(hero.rotation().next_deadline(), None);
        (hero.counters.renders.get(), hero.counters.timers.get())
    });
    for _ in 0..3 * 7 * ROTATE_MS / 16 {
        tick(cx, handle);
        cx.run_until_parked();
    }
    assert_eq!(
        hero.read_with(cx, |hero, _| (
            hero.counters.renders.get(),
            hero.counters.timers.get()
        )),
        before
    );
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Dual, window, cx)
        })
        .unwrap();
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
