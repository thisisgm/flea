//! Mounted regression cases sharing the parent fixture.
use super::*;

#[path = "rename_views_gesture_cases.rs"]
mod gesture_cases;
use gesture_cases::{band_drag_survives_a_refusal, row_press_survives_a_refusal};

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
const CLASH_SEND: &str =
    r#"{"c":"rename","path":"/files/notes.txt","to":"beta.md","menuId":0,"source":"1:2:32768"}"#;

fn clash_refusal() -> BackendEvent {
    BackendEvent::Refused {
        operation: "rename".into(),
        path: "/files/notes.txt".into(),
        message: "file exists".into(),
        mode: 0,
    }
}

// Four rows, F2 on notes.txt, and beta.md refused: the editor stays open under the refusal.
fn refused_clash(
    cx: &mut TestAppContext,
    view: View,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    super::super::rename_tests::Lines,
) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let rows = vec![
        super::super::test_shell::row("alpha", true),
        super::super::test_shell::row("beta", true),
        super::super::test_shell::row("notes.txt", false),
        super::super::test_shell::row("gamma.txt", false),
    ];
    super::super::test_shell::land(&shell, cx, "/files", rows, 2);
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
        (
            said,
            shell
                .locale
                .catalog
                .tr_args("jobs.rename.exists", &["beta.md"]),
        )
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
    assert_eq!(
        frame_ink,
        Some(error),
        "{view:?} draws the frame in the error ink"
    );
    assert_eq!(
        error_ink,
        Some(error),
        "{view:?} draws the refusal in the error ink"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .line
            .error_ground),
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
    let end = state[at..]
        .find(',')
        .map_or(state.len(), |comma| at + comma);
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
    let state = flea::jsondoc::parse(&read(&shell, cx, "renameState", &[])).unwrap();
    let height = |key| {
        state
            .get(key)
            .and_then(flea::jsondoc::Json::as_f64)
            .unwrap()
    };
    assert!(height("rowHeight") > height("normalRowHeight"), "{state:?}");
    assert_eq!(height("currentRowHeight"), height("rowHeight"));
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
    assert_eq!(
        ground,
        Some(area),
        "Grid paints the refusal on the window ground"
    );
    close(&shell, cx);
}
