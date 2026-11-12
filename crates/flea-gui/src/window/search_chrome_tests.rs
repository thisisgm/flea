//! Search owns Filter and Sort availability in every view and on the focused side.
use super::super::{input::KeyRun, pane::PaneHost, startup::View, u07_tests::read};
use crate::{backend::Backend, path::chrome::GLYPHS, state::Shell};
use flea::ui_logic_file_search::{Kind, Mode};
use gpui::{point, px, size, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

#[path = "search_menu_sort_tests.rs"]
mod menu_sort_tests;

fn open(backend: Backend, cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| crate::state::tests::shell_with(backend, cx));
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    (shell, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn available(shell: &Entity<Shell>, cx: &mut VisualTestContext, expected: bool) {
    draw(cx);
    shell.read_with(cx, |shell, cx| {
        let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
        assert_eq!(chrome.inputs.filter, expected, "Filter availability");
        assert_eq!(chrome.inputs.sort, expected, "Sort availability");
        if shell.view == View::Grid {
            let frame = chrome.frame.borrow();
            for glyph in ["filter", "sort"] {
                let index = GLYPHS.iter().position(|name| *name == glyph).unwrap();
                assert!(frame.controls[index].visible);
                assert_eq!(frame.controls[index].enabled, expected);
            }
        }
    });
}

#[gpui::test]
fn gridchrome_search_modes_disable_controls_on_only_the_focused_pane(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_lines();
    let (shell, cx) = open(backend, cx);
    let (right_backend, _right_requests) = Backend::test_lines();
    shell.update(cx, |shell, cx| {
        let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
        right.backend = Some(right_backend);
        right.pane.use_executor(cx.background_executor().clone());
        shell.panes[1] = Some(right);
        shell.lifecycle.attach(1);
    });
    for view in [View::Grid, View::List, View::Columns, View::Dual] {
        for side in [0, 1] {
            for kind in [Kind::Names, Kind::Contents] {
                for (mode, published) in [
                    (Mode::Typing, false),
                    (Mode::Typing, true),
                    (Mode::Results, true),
                ] {
                    shell.update(cx, |shell, cx| {
                        shell.view = view;
                        shell.focus_side = side;
                        let host = shell.panes[side].as_mut().unwrap();
                        host.search.owner.session.mode = mode;
                        host.search.owner.session.kind = kind;
                        host.pane.state.search = published;
                        cx.notify();
                    });
                    available(&shell, cx, false);
                    blocked(&shell, cx);
                    shell.update(cx, |shell, cx| {
                        shell.focus_side = 1 - side;
                        cx.notify();
                    });
                    available(&shell, cx, true);
                    shell.update(cx, |shell, cx| {
                        let host = shell.panes[side].as_mut().unwrap();
                        host.search.owner.session.mode = Mode::Off;
                        host.pane.state.search = false;
                        shell.focus_side = side;
                        cx.notify();
                    });
                    available(&shell, cx, true);
                }
            }
        }
    }
}

#[gpui::test]
fn gridchrome_typing_controls_ignore_pointer_hover_press_and_keyboard_actions(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_lines();
    let (shell, cx) = open(backend, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(View::Grid, window, cx);
        })
    });
    draw(cx);
    // The same points hover an enabled Filter and Sort, so the disabled reads in blocked are known to be over them.
    for glyph in ["filter", "sort"] {
        let position = centre(&shell, cx, glyph);
        cx.simulate_mouse_move(position, None, Modifiers::none());
        draw(cx);
        let state = flea::jsondoc::parse(&read(&shell, cx, "chromeButtonState", &[glyph])).unwrap();
        assert_eq!(
            state.get("hovered").and_then(|v| v.as_bool()),
            Some(true),
            "{glyph} hovers"
        );
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.search_action("search", window, cx));
        })
    });
    available(&shell, cx, false);
    blocked(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.search_update(0, None, |search, host| search.leave(host), window, cx);
        })
    });
    available(&shell, cx, true);
}

fn centre(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    glyph: &str,
) -> gpui::Point<gpui::Pixels> {
    // Sample chromeButtonCentre reply: "512 40", window-relative logical pixels.
    let centre = read(shell, cx, "chromeButtonCentre", &[glyph]);
    let mut coordinates = centre.split_whitespace().map(|v| v.parse::<f32>().unwrap());
    point(
        px(coordinates.next().unwrap()),
        px(coordinates.next().unwrap()),
    )
}

fn blocked(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let before = read(shell, cx, "statusActivityState", &[]);
    let (grid, order, pending) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[shell.focus_side].as_ref().unwrap();
        (
            shell.view == View::Grid,
            host.nav.order,
            host.nav.pending_sort.is_some(),
        )
    });
    for glyph in ["filter", "sort"].into_iter().filter(|_| grid) {
        let position = centre(shell, cx, glyph);
        cx.simulate_mouse_move(position, None, Modifiers::none());
        draw(cx);
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            modifiers: Modifiers::none(),
            first_mouse: false,
        });
        draw(cx);
        let state = flea::jsondoc::parse(&read(shell, cx, "chromeButtonState", &[glyph])).unwrap();
        assert_eq!(state.get("enabled").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(state.get("hovered").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(state.get("pressed").and_then(|v| v.as_bool()), Some(false));
        cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(read(shell, cx, "statusActivityState", &[]), before);
    }
    for action in ["filter", "sortNext", "sortReverse"] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.chrome_action(action, window, cx);
                assert_eq!(
                    shell.run_key_action(action, Default::default(), false, false, window, cx),
                    KeyRun::Ran
                );
                let host = shell.panes[shell.focus_side].as_ref().unwrap();
                assert!(!host.filter.owner.up());
                assert!(host.search.owner.active());
                assert_eq!(host.nav.order, order, "{action} keeps the sort");
                assert_eq!(
                    host.nav.pending_sort.is_some(),
                    pending,
                    "{action} asks for no sort"
                );
            })
        });
        assert_eq!(
            read(shell, cx, "statusActivityState", &[]),
            before,
            "{action} stays silent"
        );
    }
}
