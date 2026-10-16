use super::sort_tests::{drain, press};
use super::*;
use crate::tui::{input::Key, model::{Model, Row}};
use std::path::PathBuf;

fn mixed_selection(filter: &str) -> Model {
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    model.total = 7;
    for (index, name) in ["keep-amber", "bronze", "keep-charlie", "keep-delta", "echo"].into_iter().enumerate() {
        model.rows.insert(index, Row::parse(&Json::Obj(vec![("n".into(), word(name))]), &[]));
    }
    model.cursor = 3;
    model.top = 1;
    model.filter = filter.into();
    model.search = "recursive needle".into();
    model.search_query = "recursive needle".into();
    // Include a remembered mark outside the held rows and a mark the filter does not draw.
    model.selected = [0, 1, 3, 5].into_iter().collect();
    model.remember_selection();
    model.selected_rows.insert(5, Row::parse(&Json::Obj(vec![("n".into(), word("unloaded"))]), &[]));
    model
}

#[test]
fn deselect_all_clears_only_marks_with_or_without_a_filter() {
    for filter in ["", "keep", "absent"] {
        let (mut wire, reader) = echo_wire();
        let mut model = mixed_selection(filter);
        press(&mut model, &mut wire, &Key::named("A", "ctrlshift"));
        assert!(model.selected.is_empty(), "deselect leaves no marks under {filter:?}");
        assert!(model.selected_rows.is_empty(), "remembered marks clear too");
        assert_eq!((model.filter.as_str(), model.search.as_str(), model.search_query.as_str(), model.cursor, model.top),
            (filter, "recursive needle", "recursive needle", 3, 1));
        assert_eq!(model.total, 7);
        assert_eq!(model.rows.len(), 5);
        assert!(drain(&mut wire).is_empty(), "deselect sends no backend request");
        finish(wire, reader);
    }
}

#[test]
fn invert_selection_complements_only_drawn_rows_without_moving_the_view() {
    for (filter, expected, remembered) in [
        ("", vec![2, 4, 6], vec![2, 4]),
        ("keep", vec![2], vec![2]),
        ("absent", vec![], vec![]),
    ] {
        let (mut wire, reader) = echo_wire();
        let mut model = mixed_selection(filter);
        press(&mut model, &mut wire, &Key::character('V', ""));
        assert_eq!(model.selected.iter().copied().collect::<Vec<_>>(), expected, "invert under {filter:?}");
        assert_eq!(model.selected_rows.keys().copied().collect::<Vec<_>>(), remembered, "only held new marks are remembered");
        for (&index, row) in &model.selected_rows {
            assert_eq!(row.name, model.rows[&index].name);
        }
        assert_eq!((model.filter.as_str(), model.search.as_str(), model.search_query.as_str(), model.cursor, model.top),
            (filter, "recursive needle", "recursive needle", 3, 1));
        assert_eq!(model.total, 7);
        assert_eq!(model.rows.len(), 5);
        assert!(drain(&mut wire).is_empty(), "invert sends no backend request");
        finish(wire, reader);
    }
}
