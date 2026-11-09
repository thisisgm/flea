//! Picker row and tile clicks in List and Grid, on the mounted chooser with platform mouse events.
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use gpui::{point, px, Modifiers, MouseButton, MouseDownEvent, TestAppContext};

fn seed(f: &mut Fixture) {
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":3,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"},{"n":"b.txt","d":false,"s":7,"m":0,"p":33188,"i":"text-x-generic"},{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
}

#[gpui::test]
fn vf6_picker_list_click_moves_cursor(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f);
    let bounds = f.cx.debug_bounds("pk2-listing").unwrap();
    f.cx.simulate_click(bounds.origin + point(px(80.), px(46.5)), Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.cursor), 1);
    assert!(f.checks.sent().is_empty(), "one click never accepts");
}

#[gpui::test]
fn vf6_picker_grid_click_moves_cursor(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f);
    f.cx.simulate_keystrokes("ctrl-3");
    f.cx.run_until_parked();
    let listing = f.read(|picker| picker.browse.listing.clone());
    let at = f.cx.update(|_, cx| {
        let view = listing.read(cx);
        let grid = view.grid_state();
        let grid = grid.borrow();
        let tile = grid.drawn().iter().find(|tile| tile.listing == 1).unwrap();
        grid.bounds.origin
            + point(
                px(tile.slot.x + tile.slot.width / 2.),
                px(tile.slot.y + tile.slot.height / 2.),
            )
    });
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.cursor), 1);
    assert!(f.checks.sent().is_empty());
}

#[gpui::test]
fn vf6_picker_grid_press_below_the_mark_never_marks(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f);
    f.cx.simulate_keystrokes("ctrl-3");
    f.cx.run_until_parked();
    let listing = f.read(|picker| picker.browse.listing.clone());
    // Window points on tile 1: inside its mark, and low in the mark's column; tiled says the tile is drawn there.
    let (inside, below, tiled) = f.cx.update(|_, cx| {
        let view = listing.read(cx);
        let grid = view.grid_state();
        let grid = grid.borrow();
        let layout = grid.layout.unwrap();
        let tile = grid.drawn().iter().find(|tile| tile.listing == 1).unwrap();
        let cell = layout.cell(grid.scroll, tile.view);
        let side =
            crate::list::picker::check_size(layout.metrics.body_small, layout.metrics.hairline);
        let reach = 2. * layout.metrics.gap + side;
        let (x, y) = (cell.x + reach / 2., cell.y + cell.height - 2.);
        assert!(y > cell.y + reach, "inside the cell, below the mark");
        let mark = layout.metrics.gap + side / 2.;
        (
            grid.bounds.origin + point(px(cell.x + mark), px(cell.y + mark)),
            grid.bounds.origin + point(px(x), px(y)),
            grid.tile_at(x, y) == Some(tile.view),
        )
    });
    let mut hit = |at| {
        f.cx.update(|_, cx| {
            listing.update(cx, |view, _| {
                view.pointer_hit(at).map(|hit| (hit.index, hit.checkbox))
            })
        })
    };
    assert_eq!(hit(inside), Some((1, true)), "the mark's own square marks");
    let expected = tiled.then_some((1, false));
    assert_eq!(
        hit(below),
        expected,
        "below the mark the tile decides, never the mark"
    );
    f.cx.simulate_click(below, Modifiers::none());
    f.cx.run_until_parked();
    assert!(
        f.checks
            .sent()
            .iter()
            .all(|line| !line.contains("\"op\":\"mark\"")),
        "only the mark's own square marks"
    );
    let cursor = f.read(|picker| picker.listing.cursor);
    assert!(
        !tiled || cursor == 1,
        "a press on the drawn tile moves the cursor to it"
    );
}

fn double(f: &mut Fixture, row: usize) {
    let bounds = f.cx.debug_bounds("pk2-listing").unwrap();
    let at = bounds.origin + point(px(80.), px(row as f32 * 31. + 15.5));
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    f.cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    f.cx.run_until_parked();
}

#[gpui::test]
fn vf6_picker_double_click_opens_folder(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f);
    double(&mut f, 2);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d/sub");
    assert!(f.checks.sent().is_empty());
}

#[gpui::test]
fn vf6_picker_single_file_double_click_requests_checked_acceptance(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f);
    double(&mut f, 1);
    assert!(f.read(|picker| picker.accept_marks));
    assert!(f
        .checks
        .sent()
        .iter()
        .any(|line| line.contains("/d/b.txt") && line.contains("\"op\":\"mark\"")));
    assert!(
        f.written().is_none(),
        "acceptance waits for the backend's path check"
    );
}

#[gpui::test]
fn vf6_picker_multiple_double_click_marks_without_accepting(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f);
    double(&mut f, 1);
    assert!(f
        .checks
        .sent()
        .iter()
        .any(|line| line.contains("/d/b.txt") && line.contains("\"op\":\"mark\"")));
    assert!(f.written().is_none());
    assert!(!f.read(|picker| picker.accept_marks));
}

#[gpui::test]
fn vf6_picker_checkbox_click_marks_the_hit_row(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f);
    let bounds = f.cx.debug_bounds("pk2-listing").unwrap();
    f.cx.simulate_click(bounds.origin + point(px(20.), px(46.5)), Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.cursor), 1);
    assert!(f
        .checks
        .sent()
        .iter()
        .any(|line| line.contains("/d/b.txt") && line.contains("\"op\":\"mark\"")));
    assert!(f.written().is_none());
}

#[gpui::test]
fn vf6_picker_filtered_click_uses_the_painted_listing_index(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f);
    let listing = f.read(|picker| picker.browse.listing.clone());
    f.cx.update(|_, cx| {
        listing.update(cx, |view, cx| {
            view.set_filtered(Some(std::rc::Rc::from([0, 2])), "", None);
            cx.notify();
        })
    });
    f.cx.run_until_parked();
    let bounds = f.cx.debug_bounds("pk2-listing").unwrap();
    f.cx.simulate_click(bounds.origin + point(px(80.), px(46.5)), Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.cursor), 2);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
}
