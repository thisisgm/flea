//! Chrome gestures run through a mounted Root; only the unsupported test compositor handoff is observed.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use gpui::{
    point, px, size, App, AppContext, Context, Global, Modifiers, MouseButton, Pixels, Point,
    TestAppContext, VisualTestContext, Window, WindowHandle,
};
use std::cell::Cell;

#[derive(Default)]
struct Moves(Cell<usize>);
impl Global for Moves {}

pub(super) fn record(cx: &App) {
    if let Some(moves) = cx.try_global::<Moves>() {
        moves.0.set(moves.0.get() + 1);
    }
}

type Handle = WindowHandle<Root>;

fn open(cx: &mut TestAppContext) -> Handle {
    cx.update(|cx| cx.set_global(Moves::default()));
    let (backend, _) = Backend::test_channel();
    let handle = cx.open_window(size(px(1100.), px(700.)), |window, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.panes[0].as_mut().unwrap().backend = None;
            shell.view = View::List;
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    frame(cx, handle);
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

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn count(vcx: &mut VisualTestContext) -> usize {
    vcx.update(|_, cx| cx.global::<Moves>().0.get())
}

fn travel(vcx: &mut VisualTestContext, at: Point<Pixels>, dx: f32, dy: f32) {
    let end = at + point(px(dx), px(dy));
    vcx.simulate_mouse_move(at, None, Modifiers::default());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
}

/// A point on the chrome row, level with its buttons, away from every control.
fn chrome_at(cx: &mut TestAppContext, handle: Handle) -> Point<Pixels> {
    point(px(500.), control(cx, handle, 0).y)
}

fn control(cx: &mut TestAppContext, handle: Handle, index: usize) -> Point<Pixels> {
    edit(cx, handle, |shell, _, cx| {
        shell
            .chrome
            .view
            .as_ref()
            .unwrap()
            .read(cx)
            .frame
            .borrow()
            .controls[index]
            .bounds
            .unwrap()
            .center()
    })
}

#[test]
fn pb_drag4_chrome_starts_one_system_move_after_four_pixels() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    let at = chrome_at(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    travel(&mut vcx, at, 4., 0.);
    assert_eq!(count(&mut vcx), 0, "a click stays a click at the threshold");
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    for dx in [5., 8., 100.] {
        vcx.simulate_mouse_move(
            at + point(px(dx), px(0.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
    }
    assert_eq!(count(&mut vcx), 1, "one compositor request per gesture");
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    travel(&mut vcx, at, 0., 5.);
    assert_eq!(count(&mut vcx), 2, "vertical travel also moves the window");
}

#[test]
fn pb_drag4_button_click_survives_slop_but_move_cancels_its_action() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    let grid = control(&mut cx, handle, 7);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    travel(&mut vcx, grid, 5., 0.);
    assert_eq!(count(&mut vcx), 1);
    assert_eq!(edit(&mut cx, handle, |shell, _, _| shell.view), View::List);
    travel(&mut vcx, grid, 3., 0.);
    assert_eq!(count(&mut vcx), 1);
    assert_eq!(edit(&mut cx, handle, |shell, _, _| shell.view), View::Grid);
}

#[test]
fn pb_drag4_path_editor_and_covered_chrome_disable_system_move() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    let at = chrome_at(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_keystrokes("ctrl-l");
    assert!(edit(&mut cx, handle, |shell, _, cx| {
        let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
        chrome.path.read(cx).editing()
    }));
    travel(&mut vcx, at, 40., 0.);
    assert_eq!(count(&mut vcx), 0, "the editor selects text");
    vcx.simulate_keystrokes("escape");
    frame(&mut cx, handle);
    travel(&mut vcx, at, 40., 0.);
    assert_eq!(
        count(&mut vcx),
        1,
        "a closed editor leaves the chrome movable"
    );
    let settings = control(&mut cx, handle, 9);
    vcx.simulate_mouse_down(settings, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_up(settings, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(edit(&mut cx, handle, |shell, _, cx| shell
        .card_holds_keyboard(cx)));
    travel(&mut vcx, at, 40., 0.);
    assert_eq!(count(&mut vcx), 1, "covered chrome cannot move the window");
}

#[test]
fn pb_drag4_only_a_left_press_starting_on_chrome_can_move() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    let chrome = chrome_at(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_down(chrome, MouseButton::Right, Modifiers::default());
    vcx.simulate_mouse_move(
        chrome + point(px(40.), px(0.)),
        Some(MouseButton::Right),
        Modifiers::default(),
    );
    vcx.simulate_mouse_up(chrome, MouseButton::Right, Modifiers::default());
    travel(&mut vcx, point(px(500.), px(100.)), 40., -88.);
    assert_eq!(
        count(&mut vcx),
        0,
        "entering chrome with a held press is not a title drag"
    );
    travel(&mut vcx, chrome, 5., 0.);
    assert_eq!(count(&mut vcx), 1, "a new eligible press still works");
}
