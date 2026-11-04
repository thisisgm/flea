use super::harness::{english, held, metrics, open, real_cx, row, uniform, Pane};
use flea::jsondoc::parse;
use flea_gui::list::{
    columns::{Column, Switched},
    element::loading_bar,
    header::fitted_width,
    pane::Held,
    ListView, RowStates,
};
use flea_gui::row::format::{date_text, Words};
use gpui::{point, px, size, Bounds, SharedString, TestAppContext};
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
fn a_drop_target_draws_its_caption_where_the_cells_were() {
    let mut cx = real_cx();
    let pane = english_pane(&mut cx, uniform(20), 900.);
    drop_on(&mut cx, &pane, 3);
    let m = metrics();
    let state = pane.list.borrow();
    let (index, caption) = state.drawn.drop.expect("a drop target drew no caption");
    assert_eq!(index, 3);
    let list = state.drawn.bounds;
    assert_eq!(
        f32::from(caption.right()),
        f32::from(list.left()) + 900. - m.lane - m.pad,
        "the caption is not on the cells' right edge"
    );
    assert_eq!(f32::from(caption.top()), f32::from(list.top()) + 3. * m.row);
    assert!(
        f32::from(caption.size.width) > 0.,
        "the caption has no width"
    );
}

#[test]
fn a_drop_target_narrows_only_a_name_the_caption_would_cover() {
    let mut cx = real_cx();
    let long = "n".repeat(120);
    let pane = english_pane(
        &mut cx,
        vec![
            row(&long, "text-plain", 26, false),
            row(&long, "text-plain", 26, false),
        ],
        300.,
    );
    pane.frame(&mut cx, 0.);
    let plain = pane.list.borrow().visible()[1]
        .prepared
        .as_ref()
        .unwrap()
        .name
        .line()
        .width;
    drop_on(&mut cx, &pane, 1);
    let state = pane.list.borrow();
    let (_, caption) = state.drawn.drop.expect("a drop target drew no caption");
    let name = state.visible()[1]
        .prepared
        .as_ref()
        .unwrap()
        .name
        .line()
        .width;
    let m = metrics();
    let room = caption.left() - state.drawn.bounds.left() - px(m.label_x() + m.gap);
    assert!(
        name <= room && name < plain,
        "the name runs under the caption: {name:?} in {room:?}"
    );
}

#[test]
fn a_held_clipboard_costs_no_lookup_on_a_still_frame_and_one_per_entered_row() {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(40), 1), 900., 10.);
    {
        let mut state = pane.list.borrow_mut();
        state.view.clip.scope("/w");
        state.view.clip.set(&["/w/file-000002.txt".into()], true);
    }
    pane.frame(&mut cx, 0.);
    let after_first = pane.list.borrow().view.clip.lookups.get();
    for _ in 0..10 {
        pane.frame(&mut cx, 0.);
    }
    assert_eq!(
        pane.list.borrow().view.clip.lookups.get(),
        after_first,
        "a still frame looked rows up again"
    );
    pane.frame(&mut cx, metrics().row);
    assert_eq!(
        pane.list.borrow().view.clip.lookups.get(),
        after_first + 1,
        "a one-row scroll looked up more than its new row"
    );
    pane.list
        .borrow_mut()
        .view
        .clip
        .set(&["/w/file-000003.txt".into()], false);
    let before = pane.list.borrow().view.clip.lookups.get();
    pane.frame(&mut cx, metrics().row);
    assert!(
        pane.list.borrow().view.clip.lookups.get() > before,
        "a new clipboard was not looked up"
    );
    assert_eq!(
        pane.list
            .borrow()
            .visible()
            .iter()
            .find(|shown| shown.index == 3)
            .unwrap()
            .clip,
        Some(flea_gui::row::marks::ClipMark::Copy)
    );
}

#[test]
fn a_row_the_pane_does_not_hold_draws_the_loading_bar() {
    let mut cx = real_cx();
    let partial = Rc::new(Held {
        listing: 1,
        total: 100,
        start: 0,
        rows: uniform(4),
    });
    let pane = open(&mut cx, partial, 900., 10.);
    pane.frame(&mut cx, 0.);
    assert_eq!(
        pane.list.borrow().drawn.loading,
        6,
        "six unfetched rows drew no loading bar"
    );
    let m = metrics();
    let bar = loading_bar(px(0.), px(0.), &m, 886., 12.);
    assert_eq!(
        bar,
        Bounds::new(
            point(px(m.label_x()), px((m.row - 12.) / 2.)),
            size(px(886. * 0.3), px(12.))
        )
    );
}

#[test]
fn u04_kind_autofit_measures_empty_held_cells_and_an_empty_window_writes_nothing() {
    let mut view = ListView::default();
    view.apply_settings(&parse("{}").unwrap(), 0);
    let holding = held(vec![row("a", "text-plain", 26, false)], 1);
    assert_eq!(
        fitted_width(Column::Kind, &holding, &view, 120., |text| text.len()
            as f32
            * 10.),
        Some(48)
    );
    assert_eq!(
        fitted_width(
            Column::Kind,
            &held(Vec::new(), 2),
            &view,
            120.,
            |text| text.len() as f32 * 10.
        ),
        None
    );
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(10), 1), 1400., 10.);
    pane.list.borrow_mut().view.switched = Switched::default().with(Column::Kind, true);
    pane.frame(&mut cx, 0.);
    assert!(pane.list.borrow().drawn.layout.shown(Column::Kind));
    assert_eq!(
        pane.list.borrow().visible()[0]
            .prepared
            .as_ref()
            .unwrap()
            .kind
            .line()
            .width,
        px(0.)
    );
    let patch = pane.with_window(&mut cx, |list, held, window| {
        list.autofit(Column::Kind, held, window)
    });
    assert_eq!(
        patch,
        Some(flea_gui::list::header::width_patch(Column::Kind, 48))
    );
    assert_eq!(pane.list.borrow().view.stored.get(Column::Kind), Some(48));
    let again = pane.with_window(&mut cx, |list, held, window| {
        list.autofit(Column::Kind, held, window)
    });
    assert_eq!(again, None, "the fitted empty cells wrote a second time");
}

#[test]
fn the_palette_is_read_once_across_frames_of_the_header_and_the_list() {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(50), 1), 900., 10.);
    for step in 0..10 {
        pane.frame(&mut cx, step as f32 * 31.);
    }
    assert_eq!(
        pane.list.borrow().palette_reads,
        1,
        "the text style was read per frame or per cell"
    );
}

#[test]
fn dates_wait_for_catalog_words_so_the_ui_thread_never_reads_the_zone_first() {
    // Words built without a catalog snapshot would call the zone code on the calling thread.
    assert_eq!(
        date_text(1_760_600_000, &Words::default()),
        "",
        "unwarmed words formatted a date"
    );
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(5), 1), 900., 10.);
    pane.frame(&mut cx, 0.);
    let width = |pane: &Pane| {
        pane.list.borrow().visible()[0]
            .prepared
            .as_ref()
            .unwrap()
            .date
            .line()
            .width
    };
    assert_eq!(
        width(&pane),
        px(0.),
        "a date shaped before the catalog words arrived"
    );
    pane.list
        .borrow_mut()
        .set_words(Words::from_catalog(&english()));
    pane.frame(&mut cx, 0.);
    assert!(width(&pane) > px(0.), "the catalog words drew no date");
}
