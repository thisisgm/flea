use super::*;
use gpui::{div, prelude::*, FocusHandle, ScrollDelta, ScrollWheelEvent};

struct StopView(FocusHandle);
impl gpui::Render for StopView {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div().size_full().track_focus(&self.0)
    }
}

#[gpui::test]
fn u03_focus_ring_uses_mounted_handles_and_consecutive_dual_lists(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        let rail = cx.focus_handle();
        shell.slots.rail = Some(cx.new(|_| StopView(rail.clone())).into());
        shell.input.register(crate::input::focus::Stop {
            name: "rail",
            handle: rail,
            context: flea::keys::Context::Rail,
        });
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                call(shell, "focusView", window, cx),
                IpcReply::value("rail")
            );
            key(shell, "escape", window, cx);
            assert_eq!(
                call(shell, "focusView", window, cx),
                IpcReply::value("list")
            );
            shell.view = View::Dual;
            shell.panes[1] = Some(PaneHost::new("/second".into(), None, 620., &shell.locale));
            cx.notify();
        })
    });
    cx.simulate_keystrokes("tab");
    shell.read_with(cx, |shell, _| assert_eq!(shell.focus_side, 1));
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.focus_side, 0);
            assert_eq!(
                call(shell, "focusView", window, cx),
                IpcReply::value("rail")
            );
            shell.slots.rail = None;
            shell.input.remove("rail");
            window.focus(&shell.focus, cx);
            key(shell, "tab", window, cx);
            assert_eq!(shell.focus_side, 1);
            assert_eq!(
                call(shell, "focusView", window, cx),
                IpcReply::value("list")
            );
        })
    });
}

#[gpui::test]
fn u03_cut_row_reveals_only_the_cut_and_wheel_exposes_footer(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let index = (frame.viewport() / frame.row).floor() as usize;
    let cut = (index + 1) as f32 * frame.row - frame.viewport();
    assert!(cut > 0.);
    let point = point(
        frame.bounds.left() + px(60.),
        frame.bounds.bottom() - px(2.),
    );
    cx.simulate_mouse_move(point, None, Modifiers::none());
    cx.simulate_mouse_down(point, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert!((shell.panes[0].as_ref().unwrap().pane.scroll_y - cut).abs() < 0.01)
    });
    cx.update(|window, cx| shell.update(cx, |shell, cx| key(shell, "end", window, cx)));
    cx.run_until_parked();
    let flush = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.scroll_y
    });
    assert!((flush - (frame.total as f32 * frame.row - frame.viewport())).abs() < 0.01);
    cx.simulate_event(ScrollWheelEvent {
        position: point,
        delta: ScrollDelta::Lines(gpui::point(0., -1.)),
        ..Default::default()
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.pane.scroll_y > flush);
        assert!((host.pane.scroll_y - host.input.frame.unwrap().max_scroll()).abs() < 0.01);
    });
}

#[test]
fn u03_narrowed_selection_skips_hidden_indices_and_shrinks_its_own_range() {
    let mut selection = crate::input::selection::Selection::default();
    let mut rows = vec![2];
    selection.extend(&mut rows, 2, 9, None, Some(&[2, 4, 9]));
    assert_eq!(rows, [2, 4, 9]);
    selection.extend(&mut rows, 9, 4, None, Some(&[2, 4, 9]));
    assert_eq!(rows, [2, 4]);
    selection.all(&mut rows, 100, Some(&[2, 4, 9]), true);
    assert_eq!(rows, [9]);
}

#[gpui::test]
fn u03_release_never_retargets_a_reordered_row_in_the_same_listing(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let at = point(
        frame.bounds.left() + px(60.),
        frame.bounds.top() + px(frame.row * 2.5),
    );
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        let mut rows = host.pane.held.rows.clone();
        rows[2].name = "different-row".into();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing,
            total: host.pane.held.total,
            start: 0,
            rows,
        });
    });
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.selected.is_empty());
        assert_eq!(host.pane.cursor, 0);
    });
}

thread_local! {
    static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[gpui::test]
fn u03_middle_file_is_inert_and_first_tap_policies_open_once(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.launch.settings.data = flea::jsondoc::parse(r#"{"openMode":"single"}"#).unwrap();
        shell.opener[0].launch = |argv| {
            OPENED.with(|opened| opened.borrow_mut().push(argv[2..].join(" ")));
            (Some(0), String::new())
        };
        // The fixture's rows are unnamed, and the opener rightly refuses a row with no name to open.
        let host = shell.panes[0].as_mut().unwrap();
        let mut rows = host.pane.held.rows.clone();
        rows[2].name = "notes.txt".into();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing,
            total: host.pane.held.total,
            start: host.pane.held.start,
            rows,
        });
        window.focus(&shell.focus, cx);
        shell
    });
    let observed = Rc::new(std::cell::RefCell::new(Vec::new()));
    let collected = observed.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(
            &shell,
            move |_, event: &crate::window::input::ActionEvent, _| {
                collected.borrow_mut().push(event.action);
            },
        )
    });
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let position = point(
        frame.bounds.left() + px(60.),
        frame.bounds.top() + px(frame.row * 2.5),
    );
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_mouse_down(position, MouseButton::Middle, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Middle, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.selected.is_empty());
        assert_eq!(host.pane.cursor, 0);
    });
    for click_count in 1..=2 {
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count,
            ..Default::default()
        });
        cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
    }
    // The file row's one open now runs the opener instead of reaching an observer.
    assert_eq!(*observed.borrow(), Vec::<&str>::new());
    assert_eq!(
        OPENED.with(|opened| opened.take()).len(),
        1,
        "the file opens once"
    );
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [2])
    });
    shell.update(cx, |shell, cx| {
        shell.view = View::Columns;
        shell.launch.settings.data = flea::jsondoc::parse("{}").unwrap();
        let host = shell.panes[0].as_mut().unwrap();
        let mut rows = host.pane.held.rows.clone();
        rows[3].dir = true;
        rows[3].name = "sub".into();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing,
            total: host.pane.held.total,
            start: host.pane.held.start,
            rows,
        });
        cx.notify();
    });
    cx.run_until_parked();
    let lists = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.counters.lists
    });
    let position = point(position.x, frame.bounds.top() + px(frame.row * 3.5));
    cx.simulate_mouse_move(position, None, Modifiers::none());
    for click_count in 1..=2 {
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count,
            ..Default::default()
        });
        cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
    }
    // The column's folder open is run by the window, once, so it is no longer handed to observers.
    assert_eq!(*observed.borrow(), Vec::<&str>::new());
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .counters
            .lists),
        lists + 1
    );
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.place(), "/files/sub")
    });
}
