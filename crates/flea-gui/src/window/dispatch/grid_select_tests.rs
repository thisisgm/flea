//! EY-445: Grid tile presses keep the List's selection model, the margin band crosses whole cells, and Home reveals the top.
use super::test_shell::{land, open_root_with, row};
use crate::state::Shell;
use crate::window::startup::View;
use flea::keys::{Context, Match};
use gpui::{
    point, px, Entity, KeyDownEvent, Keystroke, Modifiers, MouseButton, Pixels, Point,
    TestAppContext, VisualTestContext,
};

fn open(cx: &mut TestAppContext, count: usize) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"grid""#, None);
    land(
        &shell,
        cx,
        "/files",
        (0..count)
            .map(|i| row(&format!("item-{i:02}.txt"), false))
            .collect(),
        1,
    );
    (shell, cx)
}

// A drawn tile's cell: its window top left, width and height.
fn cell_point(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    index: usize,
) -> (Point<Pixels>, f32, f32) {
    shell.read_with(cx, |shell, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let tile = state
            .drawn()
            .iter()
            .find(|tile| tile.listing == index)
            .expect("the tile is drawn");
        (
            state.bounds.origin + point(px(tile.cell.x), px(tile.cell.y)),
            tile.cell.width,
            tile.cell.height,
        )
    })
}

fn click(shell: &Entity<Shell>, cx: &mut VisualTestContext, index: usize, modifiers: Modifiers) {
    let (origin, width, height) = cell_point(shell, cx, index);
    let at = origin + point(px(width / 2.), px(height / 2.));
    cx.simulate_click(at, modifiers);
    cx.run_until_parked();
}

fn marks(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Vec<usize>, usize) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.selected.clone(), host.pane.cursor)
    })
}

#[gpui::test]
fn ey445_grid_shift_click_adds_its_range_to_a_ctrl_selection(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, 4);
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    click(&shell, cx, 1, Modifiers::none());
    click(&shell, cx, 3, ctrl);
    assert_eq!(marks(&shell, cx), (vec![1, 3], 3));
    // Issue 209: a shift range adds to a ctrl selection, so from 1,3 it keeps 1 and adds 2,3.
    click(&shell, cx, 2, shift);
    assert_eq!(marks(&shell, cx), (vec![1, 2, 3], 2));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(marks(&shell, cx), (vec![], 2));
    // PR 106: a ctrl+click with nothing marked adds to the cursor row rather than replacing it.
    click(&shell, cx, 3, ctrl);
    assert_eq!(marks(&shell, cx), (vec![2, 3], 3));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey445_grid_click_mark_is_lone_and_follows_down(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, 61);
    click(&shell, cx, 0, Modifiers::none());
    assert_eq!(marks(&shell, cx), (vec![0], 0));
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let (selected, cursor) = marks(&shell, cx);
    assert!(cursor > 1, "Down walks one tile row, at {cursor}");
    assert_eq!(
        selected,
        [cursor],
        "the clicked lone mark follows the cursor"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey445_grid_margin_band_marks_the_four_tiles_it_crosses(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, 24);
    let columns = shell.read_with(cx, |shell, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        grid.state.borrow().layout.unwrap().columns
    });
    assert!(columns >= 4, "a first row of four or more, got {columns}");
    let (origin, width, height) = cell_point(&shell, cx, 1);
    // tests/ui-marquee.sh marquee_grid_margin: three pixels inside tile 1's right edge is cell margin.
    let start = origin + point(px(width - 3.), px(height / 2.));
    let (far, width, height) = cell_point(&shell, cx, columns + 2);
    let end = far + point(px(width / 2.), px(height / 2.));
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    let selected = marks(&shell, cx).0;
    assert_eq!(selected, [1, 2, columns + 1, columns + 2]);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(marks(&shell, cx).0, [1, 2, columns + 1, columns + 2]);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey445_grid_home_on_the_first_tile_brings_the_view_back_to_the_top(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, 200);
    cx.simulate_keystrokes("home");
    cx.run_until_parked();
    shell.update(cx, |shell, cx| {
        shell.grid_scroll(400., cx);
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    let scrolled = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.cursor, host.grid.as_ref().unwrap().view.scroll)
    });
    assert!(scrolled.0 == 0 && scrolled.1 > 100., "{scrolled:?}");
    cx.simulate_keystrokes("home");
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    let scroll = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .unwrap()
            .view
            .scroll
    });
    assert_eq!(scroll, 0., "Home on the first tile reveals the top");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn uishlist_review_blank_grid_drag_keeps_band_control(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, 4);
    let (start, end) = shell.read_with(cx, |shell, _| {
        let pane = shell.panes[0].as_ref().unwrap();
        let grid = pane.grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let start = state.bounds.origin + point(px(30.), state.bounds.size.height - px(60.));
        assert!(state
            .tile_at(
                f32::from(start.x - state.bounds.left()),
                f32::from(start.y - state.bounds.top())
            )
            .is_none());
        (start, start + point(px(100.), px(30.)))
    });
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .input
        .press
        .is_none()));
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .input
        .band
        .as_ref()
        .is_some_and(|band| band.active)));
    assert!(
        !cx.update(|_, cx| cx.has_active_drag()),
        "blank-ground band must not become a Seed drag"
    );
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn uishlist_review_grid_menu_mark_drops_landing(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, 4);
    shell.update(cx, |shell, _| {
        let pane = shell.panes[0].as_mut().unwrap();
        pane.input.selection.only(&mut pane.selected, 0);
        pane.nav.landed = Some((pane.pane.held.listing, 0));
    });
    let (origin, width, height) = cell_point(&shell, cx, 1);
    let at = origin + point(px(width / 2.), px(height / 2.));
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        marks(&shell, cx),
        (vec![], 1),
        "an unmarked menu row clears the previous marks"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().nav.landed),
        None
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey445_grid_keyboard_range_shrinks_and_stays_deliberate(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, 61);
    click(&shell, cx, 0, Modifiers::none());
    cx.simulate_keystrokes("shift-down");
    cx.run_until_parked();
    let (selected, end) = marks(&shell, cx);
    assert!(end > 1);
    assert_eq!(selected, (0..=end).collect::<Vec<_>>());
    cx.simulate_keystrokes("shift-up");
    cx.run_until_parked();
    assert_eq!(marks(&shell, cx), (vec![0], 0));
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(marks(&shell, cx), (vec![0], end));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn issue_433_grid_shift_horizontal_uses_keymap_wrap_policy_and_keeps_list_unchanged(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, 61);
    let columns = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        host.grid
            .as_ref()
            .unwrap()
            .view
            .layout(crate::grid::Shown::All(host.pane.held.total))
            .columns
    });
    assert!(columns > 1);
    click(&shell, cx, columns - 1, Modifiers::none());
    assert_eq!(marks(&shell, cx), (vec![columns - 1], columns - 1));

    cx.simulate_keystrokes("shift-right");
    cx.run_until_parked();
    assert_eq!(
        marks(&shell, cx),
        (vec![columns - 1], columns - 1),
        "the default row-edge policy blocks Shift+Right"
    );

    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.change_settings(
                flea::jsondoc::parse(r#"{"gridWrapRows":true}"#).unwrap(),
                window,
                cx,
            );
        })
    });
    assert!(shell.read_with(cx, |shell, _| {
        flea::ui_logic_st1::pages::is_true(&shell.launch.settings.data, &["gridWrapRows"])
    }));
    let event = KeyDownEvent {
        keystroke: Keystroke::parse("shift-right").unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    assert_eq!(
        shell.read_with(cx, |shell, _| {
            shell.keyboard.lookup(
                "default",
                Context::Listing,
                &event,
                event.keystroke.modifiers,
                false,
            )
        }),
        Match::Action("extendRight"),
        "the generated native key table resolves Shift+Right"
    );
    shell.update(cx, |shell, cx| assert!(shell.grid_key("extendRight", cx)));
    assert_eq!(
        marks(&shell, cx),
        (vec![columns - 1, columns], columns),
        "native Grid input wraps and extends through the generated action"
    );
    shell.update(cx, |shell, cx| assert!(shell.grid_key("extendLeft", cx)));
    assert_eq!(marks(&shell, cx), (vec![columns - 1], columns - 1));
    shell.update(cx, |shell, cx| assert!(shell.grid_key("extendLeft", cx)));
    assert_eq!(
        marks(&shell, cx),
        (vec![columns - 2, columns - 1], columns - 2),
        "reversing past the anchor extends left across the row"
    );

    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::List, window, cx)));
    assert!(!shell.update(cx, |shell, cx| shell.grid_key("extendRight", cx)));
    let before = marks(&shell, cx);
    cx.simulate_keystrokes("shift-right");
    cx.run_until_parked();
    assert_eq!(
        marks(&shell, cx),
        before,
        "horizontal extension remains Grid-only"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// 1605: tile clicks mark the tiles the Grid paints, and selectionCount and selectedIndices read those marks, not the cursor.
#[gpui::test]
fn fixreaders_grid_selection_count_reads_the_painted_tile(cx: &mut TestAppContext) {
    use crate::window::u07_tests::read;
    let (shell, cx) = open(cx, 4);
    let painted = |shell: &Entity<Shell>, cx: &mut VisualTestContext| -> Vec<usize> {
        shell.read_with(cx, |shell, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let state = grid.state.borrow();
            state
                .drawn()
                .iter()
                .filter(|tile| tile.selected)
                .map(|tile| tile.listing)
                .collect()
        })
    };
    click(&shell, cx, 1, Modifiers::none());
    assert_eq!(painted(&shell, cx), vec![1]);
    assert_eq!(read(&shell, cx, "selectionCount", &[]), "1");
    assert_eq!(read(&shell, cx, "selectedIndices", &[]), "1");
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    click(&shell, cx, 3, ctrl);
    assert_eq!(painted(&shell, cx), vec![1, 3]);
    assert_eq!(read(&shell, cx, "selectionCount", &[]), "2");
    assert_eq!(read(&shell, cx, "selectedIndices", &[]), "1,3");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
