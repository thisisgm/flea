use super::harness::{english, held, metrics, open, real_cx, row, uniform, Pane};
use flea_gui::list::{
    columns::{Column, Switched},
    header::fitted_width,
    pane::Held,
    ListView, Metrics, RowStates,
};
use flea_gui::row::{format::Words, marks::ClipMark};
use gpui::{px, SharedString, TestAppContext};
use std::rc::Rc;

fn english_pane(cx: &mut TestAppContext, rows: Vec<flea::protocol::RowFacts>, width: f32) -> Pane {
    let pane = open(cx, held(rows, 1), width, 10.);
    pane.list
        .borrow_mut()
        .set_words(Words::from_catalog(&english()));
    pane
}

fn drop_on(cx: &mut TestAppContext, pane: &Pane, index: usize) {
    pane.update(cx, |view| {
        view.states = RowStates {
            listing: 1,
            drop_target: Some(index),
            drop_label: Some(SharedString::from("move here")),
            ..RowStates::default()
        }
    });
    pane.draw(cx);
}

#[test]
fn a_row_mark_is_v0312_mark_size_under_its_slot_at_every_text_size() {
    // ui/Theme.qml markSize is bodySmall x 1.45, and ui/Glyph.qml caps it at the slot.
    for (text, small) in [
        (9., 8.),
        (10., 9.),
        (11., 10.),
        (12., 11.),
        (14., 13.),
        (16., 15.),
        (20., 18.),
    ] {
        let at = Metrics::at(px(text));
        let want = f32::min(small * 1.45, at.slot);
        assert!(
            (at.mark - want).abs() < 1e-3,
            "text {text}: mark {} where v0.3.12 draws {want}",
            at.mark
        );
    }
}

#[test]
fn a_cut_drop_target_keeps_its_scissors_clear_of_the_caption() {
    let mut cx = real_cx();
    let long = "n".repeat(120);
    let pane = english_pane(
        &mut cx,
        vec![
            row(&long, "text-plain", 26, false),
            row(&long, "text-plain", 26, false),
        ],
        260.,
    );
    {
        let mut state = pane.list.borrow_mut();
        state.view.clip.scope("/w");
        state.view.clip.set(&[format!("/w/{long}")], true);
    }
    drop_on(&mut cx, &pane, 1);
    let m = metrics();
    let state = pane.list.borrow();
    let shown = &state.visible()[1];
    assert_eq!(shown.clip, Some(ClipMark::Cut));
    let (_, caption) = state.drawn.drop.expect("a drop target drew no caption");
    let layout = state.drawn.layout;
    let name = f32::from(shown.prepared.as_ref().unwrap().name.line().width).min(layout.name.width);
    let clip_right = f32::from(state.drawn.bounds.left()) + layout.name.x + name + m.gap + m.clip;
    assert!(
        clip_right + m.gap <= f32::from(caption.left()),
        "the scissors overlap the caption: clip right {clip_right}, caption left {:?}",
        caption.left()
    );
}

#[test]
fn a_drop_target_paints_none_of_its_cells() {
    let mut cx = real_cx();
    let pane = english_pane(&mut cx, uniform(20), 900.);
    pane.frame(&mut cx, 0.);
    let columns = {
        let state = pane.list.borrow();
        Column::ALL
            .iter()
            .filter(|column| state.drawn.layout.shown(**column))
            .count()
    };
    assert!(columns > 0, "the default layout draws no cells");
    assert_eq!(
        pane.list.borrow().painted_cells.get(),
        10 * columns,
        "a plain frame did not count its cells"
    );
    drop_on(&mut cx, &pane, 3);
    assert_eq!(
        pane.list.borrow().painted_cells.get(),
        9 * columns,
        "the drop target painted its cells under the caption"
    );
}

#[test]
fn an_unfetched_row_paints_the_file_mark_beside_its_loading_bar() {
    let mut cx = real_cx();
    let partial = Rc::new(Held {
        listing: 1,
        total: 100,
        start: 0,
        rows: uniform(4),
    });
    let pane = open(&mut cx, partial, 900., 10.);
    pane.frame(&mut cx, 0.);
    let state = pane.list.borrow();
    assert_eq!(state.drawn.loading, 6);
    assert_eq!(
        state.painted_marks.get(),
        10,
        "an unfetched row painted no mark"
    );
}

#[test]
fn a_date_or_link_fit_before_the_catalog_words_stores_nothing() {
    let mut view = ListView::default();
    view.switched = Switched::default().with(Column::Date, true);
    let holding = held(uniform(5), 1);
    assert_eq!(
        fitted_width(Column::Date, &holding, &view, 134., |text| text.len()
            as f32
            * 7.),
        None
    );
    let mut link = row("to-elsewhere", "text-plain", 26, false);
    link.link = Some("elsewhere.txt".into());
    link.mode = 0o120777;
    assert_eq!(
        fitted_width(
            Column::Size,
            &held(vec![link], 1),
            &view,
            70.,
            |text| text.len() as f32 * 7.
        ),
        None
    );
}
