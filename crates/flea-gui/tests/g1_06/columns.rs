use flea::jsondoc::parse;
use flea_gui::list::columns::{
    autofit_width, clamp_width, resolve, Column, Layout, StoredWidths, Switched, Tokens, MAX_WIDTH,
    MIN_WIDTH,
};
use flea_gui::list::ListView;

// The tokens at Omarchy's default face: padding 14, mark slot 23, gap 9, and the small body's 7.8 px advance at 13 px.
fn tokens() -> Tokens {
    Tokens::measure(14., 23., 9., 7.8, 13.)
}

// The intake report's ui.json: name, size, date and kind on, every stored width oversized, mode's included though it is off.
const REPORTED: &str = r#"{"columns":["name","size","date","kind"],"columnWidths":{"size":480,"kind":295,"date":480,"mode":480}}"#;

fn reported() -> (Switched, StoredWidths) {
    let data = parse(REPORTED).unwrap();
    (
        Switched::from_settings(&data),
        StoredWidths::from_settings(&data),
    )
}

fn widths(layout: &Layout) -> [Option<f32>; 4] {
    Column::ALL.map(|column| layout.cell(column).map(|cell| cell.width))
}

#[test]
fn tokens_follow_the_theme_counts_at_the_default_face() {
    let t = tokens();
    assert_eq!((t.name_min, t.measured), (156., [70., 70., 125., 130.]));
}

#[test]
fn issue_333_oversized_stored_widths_shrink_before_any_switched_on_column_drops() {
    let (switched, stored) = reported();
    // A 1580 px window with a 192 px sidebar leaves a 1374 px row once the 14 px scroll lane is taken.
    let row = 1580. - 192. - 14.;
    let layout = resolve(row, &tokens(), switched, stored);
    assert_eq!(
        layout.names(),
        "name,size,date,kind",
        "a switched-on column was dropped"
    );
    assert_eq!(widths(&layout), [None, Some(426.), Some(433.), Some(272.)]);
    assert!(
        layout.name.width >= tokens().name_min,
        "the name fell below its floor: {}",
        layout.name.width
    );
    assert_eq!(layout.name.width, 156.);
    // The chain closes on the row: padding, mark, gap, name, then each column and its gap, then the padding.
    let kind = layout.cell(Column::Kind).unwrap();
    assert_eq!(kind.right(), row - 14.);
    assert_eq!(layout.name.x, 14. + 23. + 9.);
    // The shrink is layout only: the stored widths read back exactly as written.
    assert_eq!(
        StoredWidths::from_settings(&parse(REPORTED).unwrap()),
        stored
    );
    assert_eq!(
        (
            stored.get(Column::Size),
            stored.get(Column::Date),
            stored.get(Column::Kind)
        ),
        (Some(480), Some(480), Some(295))
    );
}

#[test]
fn the_same_widths_on_a_row_too_narrow_for_every_minimum_drop_kind_then_date_then_size() {
    let (switched, stored) = reported();
    let t = tokens();
    for (row, names) in [
        (568., "name,size,date,kind"),
        (567., "name,size,date"),
        (500., "name,size,date"),
        (429., "name,size,date"),
        (428., "name,size"),
        (400., "name,size"),
        (295., "name,size"),
        (294., "name"),
        (250., "name"),
    ] {
        let layout = resolve(row, &t, switched, stored);
        assert_eq!(layout.names(), names, "at a {row} px row");
        assert!(
            layout.name.width >= t.name_min,
            "the name fell below its floor at {row} px"
        );
        for column in Column::ALL {
            if let Some(cell) = layout.cell(column) {
                assert!(
                    cell.width >= t.measured[column.slot()].min(t.wanted(column, stored)),
                    "{column:?} shrank under its minimum at {row} px"
                );
            }
        }
    }
}

#[test]
fn a_wider_row_gets_every_stored_width_back() {
    let (switched, stored) = reported();
    let layout = resolve(3000., &tokens(), switched, stored);
    assert_eq!(widths(&layout), [None, Some(480.), Some(480.), Some(295.)]);
}

#[test]
fn a_hidden_column_takes_no_room_whatever_its_stored_width() {
    let data =
        parse(r#"{"columns":["name","size"],"columnWidths":{"mode":480,"date":480,"kind":480}}"#)
            .unwrap();
    let layout = resolve(
        330.,
        &tokens(),
        Switched::from_settings(&data),
        StoredWidths::from_settings(&data),
    );
    assert_eq!(layout.names(), "name,size");
    assert_eq!(layout.cell(Column::Size).unwrap().width, 70.);
}

#[test]
fn a_fresh_install_draws_name_size_and_date() {
    let wide = resolve(
        1200.,
        &tokens(),
        Switched::default(),
        StoredWidths::default(),
    );
    assert_eq!(wide.names(), "name,size,date");
    assert_eq!(widths(&wide), [None, Some(70.), Some(125.), None]);
    let mut view = ListView::default();
    view.apply_settings(&parse("{}").unwrap(), 0);
    assert_eq!(
        (view.switched, view.stored),
        (Switched::default(), StoredWidths::default())
    );
    assert!(
        view.lang_marks,
        "language marks are on when the setting is absent"
    );
    assert_eq!(
        view.today, None,
        "highlighting today's dates is off when the setting is absent"
    );
}

#[test]
fn stored_widths_read_like_the_view_state_and_clamp_to_the_rails() {
    let data =
        parse(r#"{"columnWidths":{"size":10,"date":"wide","kind":-5,"mode":100000,"size":200.6}}"#)
            .unwrap();
    let stored = StoredWidths::from_settings(&data);
    assert_eq!(
        stored.get(Column::Size),
        Some(201),
        "the last duplicate key wins, rounded"
    );
    assert_eq!(stored.get(Column::Date), None, "a string is not a width");
    assert_eq!(
        stored.get(Column::Kind),
        None,
        "a negative width is not a width"
    );
    assert_eq!(stored.get(Column::Mode), Some(MAX_WIDTH));
    assert_eq!(
        StoredWidths::from_settings(&parse(r#"{"columnWidths":{"size":3}}"#).unwrap())
            .get(Column::Size),
        Some(MIN_WIDTH)
    );
    assert_eq!(
        StoredWidths::from_settings(&parse(r#"{"columnWidths":[]}"#).unwrap()),
        StoredWidths::default()
    );
    let switched =
        Switched::from_settings(&parse(r#"{"columns":["kind","bogus","mode"]}"#).unwrap());
    assert!(
        switched.on(Column::Kind)
            && switched.on(Column::Mode)
            && !switched.on(Column::Size)
            && !switched.on(Column::Date)
    );
    assert_eq!(
        Switched::from_settings(&parse(r#"{"columns":"size"}"#).unwrap()),
        Switched::default()
    );
}

#[test]
fn clamp_and_autofit_follow_columns_js() {
    assert_eq!(
        (
            clamp_width(47.4),
            clamp_width(48.),
            clamp_width(100.5),
            clamp_width(480.4),
            clamp_width(9e9)
        ),
        (48, 48, 101, 480, 480)
    );
    assert_eq!((clamp_width(f64::NAN), clamp_width(-3.)), (48, 48));
    assert_eq!(
        autofit_width(&[], 130.),
        130,
        "an empty window keeps the width it has"
    );
    assert_eq!(autofit_width(&[30.], 130.), 48);
    assert_eq!(autofit_width(&[100.4, 200.6, 12.], 130.), 201);
    assert_eq!(autofit_width(&[900.], 130.), 480);
}

#[test]
fn a_preview_moves_the_dragged_edge_and_the_chain_left_of_it_only() {
    let t = tokens();
    let row = 1000.;
    let layout = resolve(row, &t, Switched::default(), StoredWidths::default());
    let preview = layout.previewing(Column::Size, 150., &t, row);
    assert_eq!(
        preview.cell(Column::Date),
        layout.cell(Column::Date),
        "the column right of the edge moved"
    );
    assert_eq!(preview.cell(Column::Size).unwrap().width, 150.);
    assert_eq!(
        preview.cell(Column::Size).unwrap().right(),
        layout.cell(Column::Size).unwrap().right()
    );
    assert_eq!(preview.name.width, layout.name.width - 80.);
    assert_eq!(
        layout.previewing(Column::Kind, 150., &t, row),
        layout,
        "a column that is not drawn previews nothing"
    );
}
