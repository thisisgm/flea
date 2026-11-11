//! The inline rename field in Columns and Grid: F2 draws the framed field over the cursor's name with its stem selected, and typing and Enter rename.
use super::{
    fileops_tests::create_card,
    rename_tests::{close, draw, editing, notice, ready, receive, sent},
    test_shell::set_cursor,
};
use crate::{
    backend::Event as BackendEvent,
    columns::geometry::{Slot, CHEVRON, GAP, LANE, NAME_X, PAD_X, SIZE_W},
    state::Shell,
    window::{startup::View, u07_tests::read},
};
use gpui::{
    point, px, Bounds, Entity, Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext,
};
use std::time::Duration;

/// The bounds the rename field drew last
fn frame(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    shell.read_with(cx, |shell, _| shell.rename.edit.as_ref()?.line.frame)
}

/// Switches views and waits for the new element to mount and draw
fn switch(shell: &Entity<Shell>, cx: &mut VisualTestContext, view: View) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(view, window, cx)));
    draw(cx);
    set_cursor(shell, cx, 2);
    // The view's own element mounts once the swap that follows the cursor has had its time.
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(400));
        draw(cx);
    }
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().drawn_view),
        Some(view),
        "{view:?} drew the pane"
    );
}

// Typing replaces the selected stem and Enter sends the rename, the same in every view.
/// Drives typing and Enter through the field and checks the rename line
fn types_and_sends(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    lines: &mut super::rename_tests::Lines,
) {
    // The view switch's own requests went out first.
    sent(lines);
    cx.simulate_input("draft");
    assert_eq!(editing(shell, cx).unwrap().1, "draft.txt");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(lines),
        [
            r#"{"c":"rename","path":"/files/notes.txt","to":"draft.txt","menuId":0,"source":"1:2:32768"}"#
        ]
    );
}

/// F2 in Columns draws the field over the active track's name, inside the cursor row
#[gpui::test]
fn f2_in_columns_draws_the_field_over_the_active_tracks_name(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    switch(&shell, cx, View::Columns);
    cx.simulate_keystrokes("f2");
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        Some((2, "notes.txt".into(), "notes".into())),
        "the editor opens on the cursor row with its stem selected"
    );
    let frame = frame(&shell, cx).expect("Columns drew the rename field");
    let (track, scroll) = shell.read_with(cx, |shell, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(Slot::Active).unwrap().borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        (drawn.bounds, drawn.scroll)
    });
    let row = crate::list::pane::ROW_HEIGHT;
    let top = track.origin.y + px(2. * row - scroll.max(0.));
    assert_eq!(
        frame.origin.x,
        track.origin.x + px(NAME_X),
        "the frame sits on the name column"
    );
    assert!(
        frame.top() >= top && frame.bottom() <= top + px(row),
        "the frame sits inside the cursor row: {frame:?} against {top:?}"
    );
    let size_right = f32::from(track.size.width) - LANE - PAD_X - GAP;
    assert_eq!(
        frame.right(),
        track.origin.x + px(size_right - SIZE_W - GAP),
        "the file's field stops before its size cell"
    );
    types_and_sends(&shell, cx, &mut lines);
    close(&shell, cx);
}

/// F2 in Grid draws the field in place of the tile's caption
#[gpui::test]
fn f2_in_grid_draws_the_field_in_place_of_the_tiles_caption(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    switch(&shell, cx, View::Grid);
    cx.simulate_keystrokes("f2");
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        Some((2, "notes.txt".into(), "notes".into())),
        "the editor opens on the cursor tile with its stem selected: {}",
        notice(&shell, cx)
    );
    let frame = frame(&shell, cx).expect("Grid drew the rename field");
    let (cell, caption, origin, hidden) = shell.read_with(cx, |shell, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let tile = state.drawn_row(2).expect("the cursor tile was drawn");
        (tile.cell, tile.caption, state.bounds.origin, tile.editing)
    });
    assert!(hidden, "the tile under the field drops its own caption");
    assert_eq!(
        frame.origin.x,
        origin.x + px(caption.x),
        "the frame starts at the caption's left"
    );
    assert_eq!(
        frame.origin.y,
        origin.y + px(caption.y),
        "the frame takes the caption's first line"
    );
    assert!(
        frame.right() <= origin.x + px(cell.x + cell.width),
        "the frame stays inside its tile"
    );
    types_and_sends(&shell, cx, &mut lines);
    close(&shell, cx);
}

// PaneWire.qml openRenameOnArrival: New Folder's arrival opens the same field in Columns and Grid, its whole name selected.
/// A New Folder arrival opens the field in Columns and Grid with its whole name selected
#[gpui::test]
fn a_new_folder_arrival_opens_the_field_in_columns_and_grid(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready(cx, "default");
    for view in [View::Columns, View::Grid] {
        switch(&shell, cx, view);
        set_cursor(&shell, cx, 0);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.operations.fileops.arrival[0] = Some("/files/beta".into());
                shell.fileops_arrival(0, window, cx);
            })
        });
        draw(cx);
        assert_eq!(
            editing(&shell, cx),
            Some((1, "beta".into(), "beta".into())),
            "{view:?} opens the editor on the new folder with its name selected"
        );
        let frame = frame(&shell, cx).expect("the field drew over the new folder");
        let clip = clip(&shell, cx, view);
        assert!(
            frame.top() >= clip.top() && frame.bottom() <= clip.bottom(),
            "{view:?} drew the field inside its view"
        );
        cx.simulate_keystrokes("escape");
        assert_eq!(editing(&shell, cx), None);
    }
    close(&shell, cx);
}

// A New Folder that sorts past the first screen still opens its field: the landing reveals it before the field asks where it stands.
/// New Folder past the first screen still opens its field, the landing having scrolled it into view
fn new_folder_off_screen(cx: &mut TestAppContext, view: View) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let mut rows: Vec<_> = (0..120)
        .map(|at| super::test_shell::row(&format!("d{at:03}"), true))
        .collect();
    rows.insert(110, super::test_shell::row("New Folder", true));
    switch(&shell, cx, view);
    sent(&mut lines);
cx.simulate_keystrokes("ctrl-shift-n");
    // Ctrl+Shift+N opens the create card suggesting a folder; Create takes that suggestion.
    create_card(&shell, cx);
    let sent_out = sent(&mut lines);
    assert_eq!(
        sent_out.len(),
        1,
        "the card put its mkdir on the wire: {sent_out:?}"
    );
    // The card owns the receipt, so the reply names its request and the card closes on it.
    let id = flea::json::field_usize(&sent_out[0], "request").unwrap();
    super::fileops_tests::reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"creation","id":{id},"kind":"made","ok":true,"path":"/files/New Folder"}}"#
        ),
    );
    super::fileops_tests::refocus(&shell, cx);
    super::test_shell::land(&shell, cx, "/files", rows, 2);
    draw(cx);
    // The card asked for the name, so the field the user sees is one they asked for: r opens Rename on the arrival.
    cx.simulate_keystrokes("r");
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        Some((110, "New Folder".into(), "New Folder".into())),
        "{view:?} keeps the editor on the off-screen arrival"
    );
    assert!(
        scrolled(&shell, cx, view) > 0.,
        "{view:?} scrolled to reveal the arrival"
    );
    let frame = frame(&shell, cx).expect("the field drew over the arrival");
    let clip = clip(&shell, cx, view);
    assert!(
        frame.top() >= clip.top() && frame.bottom() <= clip.bottom(),
        "{view:?} drew the field inside the scrolled view: {frame:?} in {clip:?}"
    );
    if view == View::Columns {
        // A folder cursor row draws its chevron, so the name cell ends a chevron and a gap earlier than a file's.
        let content = f32::from(clip.size.width) - LANE;
        let size_right = content - PAD_X - CHEVRON - GAP;
        assert_eq!(
            frame.right(),
            clip.origin.x + px(size_right - SIZE_W - GAP),
            "the folder's field stops before its size cell"
        );
    }
    close(&shell, cx);
}

// The scroll offset the side's view drew last.
/// The offset the current view scrolled to in its last draw
fn scrolled(shell: &Entity<Shell>, cx: &mut VisualTestContext, view: View) -> f32 {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match view {
            View::Columns => {
                let columns = host.columns.as_ref().unwrap();
                let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                paint.drawn.as_ref().unwrap().scroll
            }
            View::Grid => host.grid.as_ref().unwrap().state.borrow().scroll,
            _ => host.list.borrow().drawn.scroll_y,
        }
    })
}

// The bounds the side's view drew last: the active track, the grid or the list.
/// Where the active track, the grid or the list last laid itself out
fn clip(shell: &Entity<Shell>, cx: &mut VisualTestContext, view: View) -> Bounds<Pixels> {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match view {
            View::Columns => {
                let columns = host.columns.as_ref().unwrap();
                let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                paint.drawn.as_ref().unwrap().bounds
            }
            View::Grid => host.grid.as_ref().unwrap().state.borrow().bounds,
            _ => host.list.borrow().drawn.bounds,
        }
    })
}

/// List keeps the off-screen arrival's field drawn inside it
#[gpui::test]
fn a_new_folder_off_screen_keeps_its_field_in_list(cx: &mut TestAppContext) {
    new_folder_off_screen(cx, View::List);
}

/// Columns keeps the off-screen arrival's field drawn inside it
#[gpui::test]
fn a_new_folder_off_screen_keeps_its_field_in_columns(cx: &mut TestAppContext) {
    new_folder_off_screen(cx, View::Columns);
}

/// Grid keeps the off-screen arrival's field drawn inside it
#[gpui::test]
fn a_new_folder_off_screen_keeps_its_field_in_grid(cx: &mut TestAppContext) {
    new_folder_off_screen(cx, View::Grid);
}

// A Grid edit stays while any of its tile shows, even with the field itself scrolled away, and drops uncommitted once the tile has gone.
/// A Grid edit survives while any of its tile shows and drops uncommitted once the tile has gone
#[gpui::test]
fn a_grid_edit_drops_only_once_its_whole_tile_leaves_the_view(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let rows: Vec<_> = (0..120)
        .map(|at| super::test_shell::row(&format!("d{at:03}"), true))
        .collect();
    super::test_shell::land(&shell, cx, "/files", rows, 2);
    switch(&shell, cx, View::Grid);
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("f2");
    draw(cx);
    assert!(
        editing(&shell, cx).is_some(),
        "F2 opens the editor on the first tile"
    );
    let (cell, field) = shell.read_with(cx, |shell, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let tile = state.drawn_row(0).expect("the first tile was drawn");
        let field = shell.rename.edit.as_ref().unwrap().line.frame.unwrap();
        (tile.cell, f32::from(field.bottom() - state.bounds.origin.y))
    });
    cx.simulate_input("draft");
    // The field's band is above the view, the tile's lower edge still shows.
    grid_scroll(&shell, cx, field + 1.);
    assert!(
        field + 1. < cell.y + cell.height,
        "the tile still reaches into the view"
    );
    assert!(
        editing(&shell, cx).is_some(),
        "a partly shown tile keeps its edit"
    );
    sent(&mut lines);
    grid_scroll(&shell, cx, cell.y + cell.height + 1.);
    assert_eq!(
        editing(&shell, cx),
        None,
        "a tile wholly out drops its edit"
    );
    assert_eq!(sent(&mut lines), Vec::<String>::new(), "with no commit");
    close(&shell, cx);
}

// A List or Columns edit outlives its field band while the row still shows, and drops uncommitted, typed draft and all, once the whole row has gone.
/// A List or Columns edit survives while its row shows and drops uncommitted once the row has gone
fn drops_once_row_leaves(cx: &mut TestAppContext, view: View) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let rows: Vec<_> = (0..120)
        .map(|at| super::test_shell::row(&format!("f{at:03}.txt"), false))
        .collect();
    super::test_shell::land(&shell, cx, "/files", rows, 2);
    switch(&shell, cx, view);
    cx.simulate_keystrokes("f2");
    draw(cx);
    assert!(
        editing(&shell, cx).is_some(),
        "F2 opens the editor on row 2"
    );
    cx.simulate_input("draft");
    let row = match view {
        View::Columns => crate::list::pane::ROW_HEIGHT,
        _ => shell.read_with(cx, |shell, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .drawn
                .metrics
                .row
        }),
    };
    // Row 2's last pixel still shows.
    list_scroll(&shell, cx, 3. * row - 1.);
    assert!(
        editing(&shell, cx).is_some(),
        "{view:?}: a partly shown row keeps its edit"
    );
    let field = frame(&shell, cx).expect("the field still stands over its row");
    assert!(
        field.bottom() <= clip(&shell, cx, view).top(),
        "{view:?}: the field band itself is above the view"
    );
    sent(&mut lines);
    list_scroll(&shell, cx, 3. * row + 1.);
    assert_eq!(
        editing(&shell, cx),
        None,
        "{view:?}: a row wholly out drops its edit"
    );
    assert_eq!(sent(&mut lines), Vec::<String>::new(), "with no commit");
    close(&shell, cx);
}

/// Scrolls the grid to an offset and draws
fn grid_scroll(shell: &Entity<Shell>, cx: &mut VisualTestContext, to: f32) {
    shell.update(cx, |shell, _| {
        shell.panes[0]
            .as_mut()
            .unwrap()
            .grid
            .as_mut()
            .unwrap()
            .view
            .scroll = to
    });
    draw(cx);
}

/// Scrolls the pane's list to an offset and draws
fn list_scroll(shell: &Entity<Shell>, cx: &mut VisualTestContext, to: f32) {
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().pane.scroll_to(to)
    });
    draw(cx);
}

#[path = "rename_views_clash_tests.rs"]
mod extracted;
