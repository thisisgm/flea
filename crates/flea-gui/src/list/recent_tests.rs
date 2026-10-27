use super::{bench, columns::Column, header, recent::Rows};
use crate::row::format;
use flea::protocol::RowFacts;
use gpui::AppContext;
use std::collections::HashMap;

fn history() -> Rows {
    Rows {
        home: "/home/tester".into(),
        visits: HashMap::from([("/home/tester/Docs/note.txt".into(), Some(1_790_000_000))]),
    }
}

#[test]
fn pb_recent_drawn_row_splits_name_location_and_uses_visit_date() {
    let mut cx = bench::real_cx();
    let facts = RowFacts {
        name: "/home/tester/Docs/note.txt".into(),
        mtime: 1_760_600_000,
        mode: 0o100644,
        ..RowFacts::default()
    };
    let held = bench::held(vec![facts]);
    let window = bench::open(&mut cx, held.clone(), 1100., 100.);
    let list = window.update(&mut cx, |view, _, _| view.list.clone()).unwrap();
    list.borrow_mut().view.recent = Some(history());
    bench::frame(&mut cx, window, 0.);
    let state = list.borrow();
    let prepared = state.visible()[0].prepared.as_ref().unwrap();
    assert_eq!(prepared.label, "note.txt");
    assert_eq!(prepared.date_text, format::date_text(1_790_000_000, &state.view.words));
    assert_eq!(header::cell_text(Column::Mode, &held, 0, &state.view).as_deref(), Some("Docs"));
    assert_eq!(held.rows[0].mtime, 1_760_600_000, "filesystem metadata stays intact");
    assert!(state.drawn.layout.shown(Column::Mode));
    assert!(!state.drawn.layout.shown(Column::Kind));
    assert_eq!(header::titles(&state.view), "Name|Location|Size|Used|Kind");
}

#[test]
fn pb_recent_narrow_rows_keep_metadata_before_location_and_restore_plain_rows() {
    let mut cx = bench::real_cx();
    let held = bench::held(vec![RowFacts {
        name: "/home/tester/Docs/note.txt".into(),
        mode: 0o100644,
        ..RowFacts::default()
    }]);
    let window = bench::open(&mut cx, held.clone(), 480., 100.);
    let list = window.update(&mut cx, |view, _, _| view.list.clone()).unwrap();
    list.borrow_mut().view.recent = Some(history());
    bench::frame(&mut cx, window, 0.);
    {
        let state = list.borrow();
        assert!(!state.drawn.layout.shown(Column::Mode));
        assert!(state.drawn.layout.shown(Column::Size));
        assert!(state.drawn.layout.shown(Column::Date));
    }
    list.borrow_mut().view.recent = None;
    bench::frame(&mut cx, window, 0.);
    let state = list.borrow();
    assert_eq!(header::titles(&state.view), "Name|Mode|Size|Modified|Kind");
    assert!(!state.drawn.layout.shown(Column::Mode));
    assert_eq!(held.rows[0].name, "/home/tester/Docs/note.txt");
}
