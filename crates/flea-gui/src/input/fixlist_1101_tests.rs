//! FIX-LIST 2026-11-01: List row plates per focus state, hover under a still pointer, and the header after a drag or a fit.
use super::*;
use crate::{
    list::{columns::Column, Palette, GREY_CURSOR, HOVER},
    palette::Theme,
    window::{pane::PaneHost, startup::View},
};
use gpui::{
    point, Entity, Hsla, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, VisualTestContext,
    Window,
};

// catppuccin-latte and sunset as Omarchy ships them, a light and a dark theme.
const LATTE: &str = "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e3e4e8'\nmuted='#acb0be'";
const SUNSET: &str = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'\nmuted='#68544b'";

fn mounted<'a>(
    cx: &'a mut TestAppContext,
    theme: &str,
    dual: bool,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (backend, _) = Backend::test_channel();
    let theme = Theme::from_text(theme);
    let (shell, cx) = cx.add_window_view(|window, cx| {
        cx.set_global(theme);
        let mut shell = shell_with(backend, cx);
        if dual {
            let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
            right
                .pane
                .receive(flea::protocol::Reply::Listed {
                    path: "/right".into(),
                    total: 8,
                })
                .unwrap();
            right
                .pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default(); 8],
                    listing: 2,
                })
                .unwrap();
            shell.panes[1] = Some(right);
            shell.view = View::Dual;
        }
        window.focus(&shell.focus, cx);
        shell
    });
    // The focused pane draws the accent only in the active window; a TestWindow starts inactive.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|_, cx| cx.set_reduce_motion(true));
    cx.simulate_resize(size(px(1100.), px(600.)));
    cx.run_until_parked();
    (shell, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

/// The fills painted over one whole row of a side's list, read from the frame's quads.
fn plates(shell: &Shell, side: usize, row: usize, window: &Window) -> Vec<Hsla> {
    let list = shell.panes[side].as_ref().unwrap().list.borrow();
    let bounds = list.drawn.bounds;
    let height = list.drawn.metrics.row;
    let top = f32::from(bounds.top()) + row as f32 * height - list.drawn.scroll_y;
    let scale = window.scale_factor();
    let near = |actual: f32, expected: f32| (actual / scale - expected).abs() < 0.01;
    window
        .painted_quads()
        .iter()
        .filter(|quad| {
            near(quad.bounds.origin.x.0, f32::from(bounds.left()))
                && near(quad.bounds.origin.y.0, top)
                && near(quad.bounds.size.width.0, f32::from(bounds.size.width))
                && near(quad.bounds.size.height.0, height)
        })
        .filter_map(|quad| quad.background.as_solid())
        .collect()
}

fn palette(shell: &Shell, side: usize) -> Palette {
    shell.panes[side]
        .as_ref()
        .unwrap()
        .list
        .borrow()
        .drawn
        .palette
        .unwrap()
}

fn mark(shell: &mut Shell, side: usize, rows: &[usize], cursor: usize) {
    let host = shell.panes[side].as_mut().unwrap();
    host.selected = rows.to_vec();
    host.input.selection.changed(&host.selected);
    host.pane.cursor = cursor;
}

/// EY-417: the background menu takes the keyboard, the cursor goes grey and a marked row keeps the plate.
#[gpui::test]
fn fixlist_ey417_marked_plate_stays_while_the_menu_holds_the_keyboard(cx: &mut TestAppContext) {
    for (name, theme) in [("catppuccin-latte", LATTE), ("sunset", SUNSET)] {
        let mut app = cx.clone();
        let (shell, cx) = mounted(&mut app, theme, false);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                key(shell, "v", window, cx);
                key(shell, "j", window, cx);
                assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0]);
            })
        });
        draw(cx);
        cx.update(|window, cx| {
            let shell = shell.read(cx);
            let palette = palette(shell, 0);
            assert_eq!(
                plates(shell, 0, 0, window),
                [palette.marked],
                "{name}: focused mark"
            );
        });
        // A right click on the blank ground opens the background menu, which takes the keyboard.
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_menu(0, None, (40., 40.), window, cx)
            })
        });
        cx.run_until_parked();
        draw(cx);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                assert!(shell.menu_opened(), "{name}: the menu opened");
                assert!(!shell.list_states(0, window, cx).focused);
            });
            let shell = shell.read(cx);
            let palette = palette(shell, 0);
            assert_eq!(
                plates(shell, 0, 0, window),
                [palette.marked],
                "{name}: the marked row under the menu"
            );
            assert_eq!(
                plates(shell, 0, 1, window),
                [palette.foreground.opacity(GREY_CURSOR)],
                "{name}: the cursor under the menu"
            );
        });
    }
}

/// CR-CMP-D1: the dual window's other pane draws its marks in v0.3.12's grey selected fill, never weaker than its cursor.
#[gpui::test]
fn fixlist_crcmpd1_other_pane_marks_take_the_grey_selected_fill(cx: &mut TestAppContext) {
    for (name, theme) in [("catppuccin-latte", LATTE), ("sunset", SUNSET)] {
        let mut app = cx.clone();
        let (shell, cx) = mounted(&mut app, theme, true);
        cx.update(|_, cx| {
            shell.update(cx, |shell, _| {
                shell.focus_side = 0;
                mark(shell, 0, &[2], 5);
                mark(shell, 1, &[2], 5);
            })
        });
        draw(cx);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                assert!(shell.list_states(0, window, cx).focused);
                let other = shell.list_states(1, window, cx);
                assert!(!other.focused && other.other_pane);
            });
            let shell = shell.read(cx);
            let grey = palette(shell, 1).foreground.opacity(GREY_CURSOR);
            assert_eq!(
                plates(shell, 0, 2, window),
                [palette(shell, 0).marked],
                "{name}: focused pane mark"
            );
            assert_eq!(
                plates(shell, 1, 2, window),
                [grey],
                "{name}: other pane mark"
            );
            assert_eq!(
                plates(shell, 1, 5, window),
                [grey],
                "{name}: other pane cursor"
            );
        });
    }
}

/// EY-117: a wheel scroll under a still pointer lights the row it brings there, with no new motion event.
#[gpui::test]
fn fixlist_ey117_list_hover_follows_the_rows_a_wheel_brings_under_the_pointer(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = mounted(cx, SUNSET, false);
    draw(cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let at = point(
        frame.bounds.left() + px(200.),
        frame.bounds.top() + px(frame.row * 2.5),
    );
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
    draw(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.list_states(0, window, cx).hovered, Some(2));
        })
    });
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(point(0., -3.)),
        ..Default::default()
    });
    cx.run_until_parked();
    // One frame: the frame the wheel's scroll prepares must itself paint the row now under the pointer.
    draw(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            let scroll = host.pane.scroll_y;
            assert!(scroll > 0., "the wheel scrolled");
            let under =
                ((f32::from(at.y - frame.bounds.top()) + scroll) / frame.row).floor() as usize;
            assert_eq!(
                shell.list_states(0, window, cx).hovered,
                Some(under),
                "the row now under the pointer takes the hover"
            );
            let foreground = palette(shell, 0).foreground;
            assert_eq!(plates(shell, 0, under, window), [foreground.opacity(HOVER)]);
        })
    });
}

fn size_edge(shell: &Shell) -> gpui::Point<gpui::Pixels> {
    let list = shell.panes[0].as_ref().unwrap().list.borrow();
    let header = list.header.drawn.bounds;
    point(
        header.left() + px(list.header.drawn.layout.cell(Column::Size).unwrap().x),
        header.center().y,
    )
}

/// EY-307: the drag writes a width, and neither its release nor F4 leaves the zone's accent line lit from before.
#[gpui::test]
fn fixlist_ey307_header_drag_writes_and_release_or_f4_leaves_no_stale_line(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = mounted(cx, SUNSET, false);
    draw(cx);
    let (start, before) = shell.read_with(cx, |shell, _| {
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        (
            size_edge(shell),
            list.drawn.layout.cell(Column::Size).unwrap().width,
        )
    });
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.run_until_parked();
    let hot = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().list.borrow().header.hot
        })
    };
    assert_eq!(
        hot(cx),
        Some(Column::Size),
        "the zone lights under the pointer"
    );
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    let finish = start + point(px(-60.), px(0.));
    cx.simulate_mouse_move(finish, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(finish, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    draw(cx);
    let stored = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .view
            .stored
            .get(Column::Size)
    });
    assert_eq!(
        stored,
        Some((before + 60.) as u32),
        "begin, move_to and end wrote the width"
    );
    assert_eq!(
        hot(cx),
        Some(Column::Size),
        "the edge followed the pointer, so the still pointer keeps its zone lit"
    );
    let edge = shell.read_with(cx, |shell, _| size_edge(shell));
    assert!(
        (edge.x - finish.x).abs() < px(1.),
        "the moved edge rests under the pointer"
    );
    assert!(
        accent_line(cx, &shell),
        "the lit zone draws its accent line"
    );
    // A drag the rails clamp leaves the edge behind the pointer, so no zone stays lit at a release still over the header.
    let header = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .header
            .drawn
            .bounds
    });
    let far = point(header.right() - px(2.), edge.y);
    assert!(
        header.contains(&far),
        "the clamped release stays over the header"
    );
    cx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(far, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(far, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    draw(cx);
    let clamped = shell.read_with(cx, |shell, _| {
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        list.view.stored.get(Column::Size)
    });
    assert_eq!(
        clamped,
        Some(crate::list::columns::MIN_WIDTH),
        "the rails clamped the width"
    );
    assert_eq!(hot(cx), None, "a clamped edge is not under the pointer");
    cx.simulate_mouse_move(edge, None, Modifiers::none());
    cx.run_until_parked();
    let edge = shell.read_with(cx, |shell, _| size_edge(shell));
    cx.simulate_mouse_move(edge, None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(hot(cx), Some(Column::Size));
    // A press that slides straight down into the rows moves no edge, and its release there lights no zone.
    let straight = edge + point(px(0.), px(3. * crate::list::pane::ROW_HEIGHT));
    cx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(straight, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(straight, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(hot(cx), None, "a release in the rows lights no zone");
    cx.simulate_mouse_move(edge, None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(hot(cx), Some(Column::Size));
    // An edge that followed the pointer down into the rows is not under it, so a release there lights no zone.
    let below = edge + point(px(-20.), px(3. * crate::list::pane::ROW_HEIGHT));
    cx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(below, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(hot(cx), None, "a release off the header lights no zone");
    let edge = shell.read_with(cx, |shell, _| size_edge(shell));
    cx.simulate_mouse_move(edge, None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(hot(cx), Some(Column::Size));
    draw(cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.fit_list_columns(window, cx)));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(hot(cx), None, "F4 moved the edges, so no zone stays lit");
    assert!(
        !accent_line(cx, &shell),
        "no accent line in the header after F4"
    );
}

fn accent_line(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        let header = list.header.drawn.bounds;
        let accent = palette(shell, 0).accent;
        let scale = window.scale_factor();
        window.painted_quads().iter().any(|quad| {
            quad.background.as_solid() == Some(accent)
                && (quad.bounds.origin.y.0 / scale - f32::from(header.top())).abs() < 0.01
        })
    })
}
