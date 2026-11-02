use super::harness::{english, held, metrics, open, real_cx, row, uniform};
use flea::jsondoc::parse;
use flea_gui::list::{
    columns::{resolve, Column, StoredWidths, Switched, Tokens},
    header::{fitted_width, handle_at, handles, width_patch, Drag},
    ListView,
};
use flea_gui::navigation::folder_state::Order;
use flea_gui::row::format::Words;

fn tokens() -> Tokens {
    Tokens::measure(14., 23., 9., 7.8, 13.)
}

#[test]
fn zones_sit_on_every_drawn_column_edge_and_only_in_a_single_unsearched_pane() {
    let layout = resolve(
        1000.,
        &tokens(),
        Switched::default().with(Column::Mode, true),
        StoredWidths::default(),
    );
    let mut view = ListView::default();
    let zones: Vec<_> = handles(&layout, &metrics(), &view).collect();
    assert_eq!(
        zones.iter().map(|zone| zone.column).collect::<Vec<_>>(),
        [Column::Mode, Column::Size, Column::Date]
    );
    for zone in &zones {
        let cell = layout.cell(zone.column).unwrap();
        assert_eq!(
            (zone.x, zone.width),
            (cell.x - 4., 9.),
            "the {:?} zone is not centred on its edge",
            zone.column
        );
    }
    let size = layout.cell(Column::Size).unwrap();
    assert_eq!(
        handle_at(&layout, &metrics(), &view, size.x),
        Some(Column::Size)
    );
    assert_eq!(
        handle_at(&layout, &metrics(), &view, size.x - 4.),
        Some(Column::Size)
    );
    assert_eq!(handle_at(&layout, &metrics(), &view, size.x + 5.), None);
    view.dual = true;
    assert_eq!(
        handles(&layout, &metrics(), &view).count(),
        0,
        "a dual pane built zones"
    );
    view.dual = false;
    view.searching = true;
    assert_eq!(
        handle_at(&layout, &metrics(), &view, size.x),
        None,
        "a search left the zones live"
    );
}

#[test]
fn a_press_that_never_travels_is_a_click_and_writes_nothing() {
    let mut drag = Drag::default();
    drag.begin(Column::Size, 300., 70.);
    assert!(
        !drag.move_to(Column::Size, 300.6),
        "a sub-pixel move counted as a drag"
    );
    assert!(
        !drag.move_to(Column::Date, 250.),
        "another column's move moved this drag"
    );
    assert_eq!(drag.end(), None);
    assert_eq!(drag.trace(), "300|300|70|70");
}

#[test]
fn a_drag_previews_the_header_alone_then_writes_one_clamped_width_on_release() {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(50), 1), 900., 10.);
    pane.frame(&mut cx, 0.);
    let (rows_before, size) = {
        let state = pane.list.borrow();
        (
            state.drawn.layout,
            state.drawn.layout.cell(Column::Size).unwrap(),
        )
    };
    pane.list
        .borrow_mut()
        .header
        .drag
        .begin(Column::Size, size.x, size.width);
    assert!(pane
        .list
        .borrow_mut()
        .header
        .drag
        .move_to(Column::Size, size.x - 30.));
    pane.frame(&mut cx, 0.);
    {
        let state = pane.list.borrow();
        let header = state.header.drawn.layout.cell(Column::Size).unwrap();
        assert_eq!(
            header.width,
            size.width + 30.,
            "the header did not follow the pointer"
        );
        assert_eq!(
            state.drawn.layout, rows_before,
            "the rows moved before the release"
        );
        assert_eq!(
            state.header.drag.trace(),
            format!(
                "{}|{}|{}|{}",
                size.x,
                size.x - 30.,
                size.width,
                size.width + 30.
            )
        );
    }
    let wanted = (size.width + 30.) as u32;
    let patch = pane.list.borrow_mut().end_drag(true);
    assert_eq!(patch, Some(width_patch(Column::Size, wanted)));
    assert_eq!(
        patch.as_deref(),
        Some(format!("{{\"columnWidths\":{{\"size\":{wanted}}}}}").as_str())
    );
    assert_eq!(
        pane.list.borrow().view.stored.get(Column::Size),
        Some(wanted)
    );
    assert_eq!(
        pane.list.borrow_mut().end_drag(true),
        None,
        "a second release wrote again"
    );
    assert_eq!(
        pane.list.borrow_mut().write_width(Column::Size, wanted),
        None,
        "an unchanged width wrote again"
    );
    // A drag past the rail writes the rail, never a width the view state would refuse.
    pane.list
        .borrow_mut()
        .header
        .drag
        .begin(Column::Size, size.x, size.width);
    pane.list
        .borrow_mut()
        .header
        .drag
        .move_to(Column::Size, size.x - 5_000.);
    assert_eq!(
        pane.list.borrow_mut().end_drag(true),
        Some(width_patch(Column::Size, 480))
    );
    pane.frame(&mut cx, 0.);
    let state = pane.list.borrow();
    assert!(state.header.drag.column.is_none());
    assert!(state.drawn.layout.cell(Column::Size).unwrap().width <= 480.);
    assert_eq!(
        state.header.drawn.layout, state.drawn.layout,
        "the header and rows disagree after the drag"
    );
}

#[test]
fn a_fit_measures_the_held_rows_only_and_an_empty_window_writes_nothing() {
    let mut view = ListView::default();
    view.apply_settings(&parse("{}").unwrap(), 0);
    view.words = std::rc::Rc::new(Words::from_catalog(&english()));
    let rows = vec![
        row("a", "text-plain", 26, false),
        row("b", "text-plain", 12_345_678, false),
    ];
    let holding = held(rows, 1);
    let by_chars = |text: &str| text.chars().count() as f32 * 10.;
    // "12.3 MB" is seven cells: seventy pixels, inside the rails.
    assert_eq!(
        fitted_width(Column::Size, &holding, &view, 70., by_chars),
        Some(70)
    );
    assert_eq!(
        fitted_width(Column::Mode, &holding, &view, 70., by_chars),
        Some(90)
    );
    assert_eq!(
        fitted_width(Column::Size, &held(Vec::new(), 1), &view, 70., by_chars),
        None
    );
    assert_eq!(
        fitted_width(Column::Date, &holding, &view, 125., |_| 9_000.),
        Some(480)
    );
    // A double click through the window ends a drag in flight and writes the fitted width once.
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(30), 1), 900., 10.);
    pane.frame(&mut cx, 0.);
    pane.list
        .borrow_mut()
        .header
        .drag
        .begin(Column::Size, 600., 70.);
    let patch = pane.with_window(&mut cx, |list, held, window| {
        list.autofit(Column::Size, held, window)
    });
    assert!(
        pane.list.borrow().header.drag.column.is_none(),
        "the fit left the drag running"
    );
    let fitted = pane
        .list
        .borrow()
        .view
        .stored
        .get(Column::Size)
        .expect("the fit wrote a width");
    assert_eq!(patch, Some(width_patch(Column::Size, fitted)));
    assert!((48..=480).contains(&fitted));
    let again = pane.with_window(&mut cx, |list, held, window| {
        list.autofit(Column::Size, held, window)
    });
    assert_eq!(again, None, "a second fit to the same width wrote again");
    let kind = pane.with_window(&mut cx, |list, held, window| {
        list.autofit(Column::Kind, held, window)
    });
    assert_eq!(kind, None, "a column that is not drawn was fitted");
}

#[test]
fn titles_shape_once_across_scrolls_and_reshape_for_a_new_sort() {
    let mut cx = real_cx();
    let pane = open(&mut cx, held(uniform(200), 1), 900., 10.);
    pane.frame(&mut cx, 0.);
    let shaped = pane.list.borrow().header.drawn.shaped;
    for step in 1..=20 {
        pane.frame(&mut cx, step as f32 * 31.);
    }
    assert_eq!(
        pane.list.borrow().header.drawn.shaped,
        shaped,
        "a scroll reshaped the titles"
    );
    pane.list.borrow_mut().header.hot = Some(Column::Size);
    pane.frame(&mut cx, 0.);
    assert_eq!(
        pane.list.borrow().header.drawn.shaped,
        shaped,
        "a hovered zone reshaped the titles"
    );
    pane.list.borrow_mut().view.order = Order {
        key: "size",
        desc: true,
    };
    pane.frame(&mut cx, 0.);
    assert_eq!(
        pane.list.borrow().header.drawn.shaped,
        shaped + 1,
        "a new sort did not reshape the titles"
    );
}
