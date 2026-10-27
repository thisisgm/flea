//! The inline rename field in Columns and Grid: F2 draws the framed field over the cursor's name with its stem selected, and typing and Enter rename.
use super::{
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

fn frame(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    shell.read_with(cx, |shell, _| shell.rename.edit.as_ref()?.line.frame)
}

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
        [r#"{"c":"rename","path":"/files/notes.txt","to":"draft.txt","menuId":0}"#]
    );
}

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
fn new_folder_off_screen(cx: &mut TestAppContext, view: View) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let mut rows: Vec<_> = (0..120)
        .map(|at| super::test_shell::row(&format!("d{at:03}"), true))
        .collect();
    rows.insert(110, super::test_shell::row("New Folder", true));
    switch(&shell, cx, view);
    sent(&mut lines);
    cx.simulate_keystrokes("ctrl-shift-n");
    assert_eq!(sent(&mut lines), [r#"{"c":"mkdir","path":"/files"}"#]);
    super::fileops_tests::reply(
        &shell,
        cx,
        r#"{"t":"made","ok":true,"path":"/files/New Folder"}"#,
    );
    super::test_shell::land(&shell, cx, "/files", rows, 2);
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

#[gpui::test]
fn a_new_folder_off_screen_keeps_its_field_in_list(cx: &mut TestAppContext) {
    new_folder_off_screen(cx, View::List);
}

#[gpui::test]
fn a_new_folder_off_screen_keeps_its_field_in_columns(cx: &mut TestAppContext) {
    new_folder_off_screen(cx, View::Columns);
}

#[gpui::test]
fn a_new_folder_off_screen_keeps_its_field_in_grid(cx: &mut TestAppContext) {
    new_folder_off_screen(cx, View::Grid);
}

// A Grid edit stays while any of its tile shows, even with the field itself scrolled away, and drops uncommitted once the tile has gone.
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

#[gpui::test]
fn a_list_edit_drops_once_its_row_leaves_the_list(cx: &mut TestAppContext) {
    drops_once_row_leaves(cx, View::List);
}

#[gpui::test]
fn a_columns_edit_drops_once_its_row_leaves_the_track(cx: &mut TestAppContext) {
    drops_once_row_leaves(cx, View::Columns);
}

// A clashing rename keeps its editor open under the refusal: the frame and the refusal take the error ink, and the edited row grows so the row below clears the refusal.
// The clashing draft's own send, and the refusal the backend answers with.
const CLASH_SEND: &str = r#"{"c":"rename","path":"/files/notes.txt","to":"beta.md","menuId":0}"#;

fn clash_refusal() -> BackendEvent {
    BackendEvent::Refused {
        operation: "rename".into(),
        path: "/files/notes.txt".into(),
        message: "file exists".into(),
        mode: 0,
    }
}

// Four rows, F2 on notes.txt, and beta.md refused: the editor stays open under the refusal.
fn refused_clash<'a>(
    cx: &'a mut TestAppContext,
    view: View,
) -> (
    Entity<Shell>,
    &'a mut VisualTestContext,
    super::rename_tests::Lines,
) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let rows = vec![
        super::test_shell::row("alpha", true),
        super::test_shell::row("beta", true),
        super::test_shell::row("notes.txt", false),
        super::test_shell::row("gamma.txt", false),
    ];
    super::test_shell::land(&shell, cx, "/files", rows, 2);
    switch(&shell, cx, view);
    sent(&mut lines);
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("beta.md");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut lines), [CLASH_SEND]);
    receive(&shell, cx, clash_refusal());
    draw(cx);
    (shell, cx, lines)
}

fn clash_grows_the_row(cx: &mut TestAppContext, view: View) {
    let (shell, cx, _lines) = refused_clash(cx, view);
    assert_eq!(
        editing(&shell, cx).map(|(row, text, _)| (row, text)),
        Some((2, "beta.md".into())),
        "{view:?} keeps the editor open on the clashing draft"
    );
    let (said, exists) = shell.read_with(cx, |shell, _| {
        let said = shell.rename.edit.as_ref().unwrap().error.clone();
        (said, shell.locale.catalog.tr_args("jobs.rename.exists", &["beta.md"]))
    });
    assert_eq!(said, exists, "{view:?} says the name is taken");
    let error = cx.update(|window, cx| {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        crate::list::Palette::inherited(&window.text_style(), theme).error
    });
    let (area, frame_ink, error_ink) = shell.read_with(cx, |shell, _| {
        let line = &shell.rename.edit.as_ref().unwrap().line;
        (line.error_area, line.frame_ink, line.error_ink)
    });
    let frame = frame(&shell, cx).expect("the field drew over the row");
    let area = area.expect("the refusal drew under the field");
    let row = match view {
        View::Columns => crate::list::pane::ROW_HEIGHT,
        _ => shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().list.borrow().drawn.metrics.row
        }),
    };
    let bounds = clip(&shell, cx, view);
    let scroll = scrolled(&shell, cx, view);
    let grow = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match view {
            View::Columns => {
                let columns = host.columns.as_ref().unwrap();
                let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                paint.drawn.as_ref().unwrap().grow
            }
            _ => host.list.borrow().drawn.grow,
        }
    });
    let top = |at: usize| bounds.origin.y + px(grow.top(at, row) - scroll);
    assert_eq!(frame_ink, Some(error), "{view:?} draws the frame in the error ink");
    assert_eq!(error_ink, Some(error), "{view:?} draws the refusal in the error ink");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.rename.edit.as_ref().unwrap().line.error_ground),
        None,
        "{view:?} grew its row, so the grown plate covers the refusal"
    );
    assert!(
        area.top() >= frame.bottom(),
        "{view:?} draws the refusal under the field: {area:?} against {frame:?}"
    );
    assert!(
        frame.top() >= top(2) && frame.bottom() <= top(2) + px(row),
        "{view:?} keeps the field inside its own row"
    );
    assert!(
        top(3) - top(2) > px(row),
        "{view:?} grows the edited row by the refusal: the row below moved to {:?}",
        top(3)
    );
    assert!(
        area.bottom() <= top(3),
        "{view:?} keeps the refusal clear of the row below: {area:?} against {:?}",
        top(3)
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_clashing_list_rename_grows_its_row_and_keeps_the_editor(cx: &mut TestAppContext) {
    clash_grows_the_row(cx, View::List);
}

#[gpui::test]
fn a_clashing_columns_rename_grows_its_row_and_keeps_the_editor(cx: &mut TestAppContext) {
    clash_grows_the_row(cx, View::Columns);
}

// The List's live gesture: whether a band is up, the row a press holds, and the selection.
fn gesture(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (bool, Option<usize>, Vec<usize>) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (
            host.input.band.is_some(),
            host.input.press.as_ref().map(|press| press.row),
            host.selected.clone(),
        )
    })
}

// The rows' height in the view: the List's own metrics or the Columns track's row.
fn row_height(shell: &Entity<Shell>, cx: &mut VisualTestContext, view: View) -> f32 {
    match view {
        View::Columns => crate::list::pane::ROW_HEIGHT,
        _ => shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().list.borrow().drawn.metrics.row
        }),
    }
}

// A press below the rows commits the refused draft and re-sends it, so the band is down until the second refusal.
// A second refusal that lands mid-drag raises the band again: the drag must keep its band and its selection.
fn band_drag_survives_a_refusal(cx: &mut TestAppContext, view: View) {
    let (shell, cx, mut lines) = refused_clash(cx, view);
    let row = row_height(&shell, cx, view);
    let bounds = clip(&shell, cx, view);
    // Below all four rows, with or without the band's extra height.
    let start = point(bounds.origin.x + px(40.), bounds.origin.y + px(4. * row + 40.));
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(sent(&mut lines), [CLASH_SEND], "{view:?}: the press commits the draft");
    assert!(gesture(&shell, cx).0, "{view:?}: the press starts a band");
    let mut at = start;
    for step in 1..=3 {
        at = start + point(px(20.), px(-40. * step as f32));
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::default());
        draw(cx);
    }
    receive(&shell, cx, clash_refusal());
    draw(cx);
    assert!(
        gesture(&shell, cx).0,
        "{view:?}: the refusal's band must not cancel the band drag"
    );
    for step in 4..=5 {
        at = start + point(px(20.), px(-40. * step as f32));
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::default());
        draw(cx);
    }
    let (band, _, selected) = gesture(&shell, cx);
    assert!(band, "{view:?}: the band survives every frame of the drag");
    assert!(
        selected.contains(&2) && selected.contains(&3),
        "{view:?}: the band selects the rows it crosses: {selected:?}"
    );
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    draw(cx);
    close(&shell, cx);
}

// A press on a row, while the commit's send is pending, keeps its press when the refusal raises the band.
fn row_press_survives_a_refusal(cx: &mut TestAppContext, view: View) {
    let (shell, cx, _lines) = refused_clash(cx, view);
    let row = row_height(&shell, cx, view);
    let bounds = clip(&shell, cx, view);
    // The first row, above the band's extra height.
    let press = point(bounds.origin.x + px(40.), bounds.origin.y + px(row / 2.));
    cx.simulate_mouse_move(press, None, Modifiers::default());
    cx.simulate_mouse_down(press, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(gesture(&shell, cx).1, Some(0), "{view:?}: the press holds alpha");
    receive(&shell, cx, clash_refusal());
    draw(cx);
    draw(cx);
    assert_eq!(
        gesture(&shell, cx).1,
        Some(0),
        "{view:?}: the refusal's band must not cancel the row press"
    );
    cx.simulate_mouse_up(press, MouseButton::Left, Modifiers::default());
    draw(cx);
    close(&shell, cx);
}

#[gpui::test]
fn a_refusal_landing_mid_band_drag_keeps_the_band_in_list(cx: &mut TestAppContext) {
    band_drag_survives_a_refusal(cx, View::List);
}

#[gpui::test]
fn a_refusal_landing_mid_band_drag_keeps_the_band_in_columns(cx: &mut TestAppContext) {
    band_drag_survives_a_refusal(cx, View::Columns);
}

#[gpui::test]
fn a_refusal_landing_mid_row_press_keeps_the_press_in_list(cx: &mut TestAppContext) {
    row_press_survives_a_refusal(cx, View::List);
}

#[gpui::test]
fn a_refusal_landing_mid_row_press_keeps_the_press_in_columns(cx: &mut TestAppContext) {
    row_press_survives_a_refusal(cx, View::Columns);
}

// errorHeight as renameState serves it to the readers.
fn error_height(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> f32 {
    let state = read(shell, cx, "renameState", &[]);
    let key = "\"errorHeight\":";
    let at = state.find(key).expect("renameState reports errorHeight") + key.len();
    let end = state[at..].find(',').map_or(state.len(), |comma| at + comma);
    state[at..end]
        .parse()
        .unwrap_or_else(|error| panic!("errorHeight in {state}: {error}"))
}

// The band the List or Columns row drew for the refusal: its grow.
fn drawn_band(shell: &Entity<Shell>, cx: &mut VisualTestContext, view: View) -> f32 {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match view {
            View::Columns => {
                let columns = host.columns.as_ref().unwrap();
                let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                paint.drawn.as_ref().unwrap().grow.band
            }
            _ => host.list.borrow().drawn.grow.band,
        }
    })
}

// A grown view reports the band it drew as errorHeight, so the readers agree with the paint.
fn error_height_is_the_drawn_band(cx: &mut TestAppContext, view: View) {
    let (shell, cx, _lines) = refused_clash(cx, view);
    let drawn = drawn_band(&shell, cx, view);
    assert!(drawn > 0., "{view:?} grew its row for the refusal");
    let reported = error_height(&shell, cx);
    assert!(
        (reported - drawn).abs() < 0.01,
        "{view:?} reports the band it drew: {reported} against {drawn}"
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_clashing_list_rename_reports_its_drawn_band_as_errorheight(cx: &mut TestAppContext) {
    error_height_is_the_drawn_band(cx, View::List);
}

#[gpui::test]
fn a_clashing_columns_rename_reports_its_drawn_band_as_errorheight(cx: &mut TestAppContext) {
    error_height_is_the_drawn_band(cx, View::Columns);
}

// Grid draws the refusal under its tile without growing the tile, and reports the caption line and gap v0.3.12 reports.
#[gpui::test]
fn a_clashing_grid_rename_reports_the_caption_band_as_errorheight(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = refused_clash(cx, View::Grid);
    let (area, gap) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let typeface = host.list.borrow().typeface.clone();
        (
            shell
                .rename
                .edit
                .as_ref()
                .unwrap()
                .line
                .error_area
                .expect("the refusal drew under the field"),
            crate::list::Metrics::for_typeface(&typeface).gap,
        )
    });
    let reported = error_height(&shell, cx);
    let caption = f32::from(area.size.height) + gap;
    assert!(
        (reported - caption).abs() < 0.5,
        "Grid reports the caption line and gap: {reported} against {caption}"
    );
    close(&shell, cx);
}

// Grid never grows its row, so the refusal paints the window ground under itself; a grown row keeps its own plate instead.
#[gpui::test]
fn a_clashing_grid_rename_paints_its_refusal_on_the_window_ground(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = refused_clash(cx, View::Grid);
    let (area, ground) = shell.read_with(cx, |shell, _| {
        let line = &shell.rename.edit.as_ref().unwrap().line;
        (line.error_area, line.error_ground)
    });
    let area = area.expect("the refusal drew under the field");
    assert_eq!(ground, Some(area), "Grid paints the refusal on the window ground");
    close(&shell, cx);
}

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

fn list_scroll(shell: &Entity<Shell>, cx: &mut VisualTestContext, to: f32) {
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().pane.scroll_to(to)
    });
    draw(cx);
}
