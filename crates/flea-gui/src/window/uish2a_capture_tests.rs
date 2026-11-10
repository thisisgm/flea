//! Observation regressions drive the real mounted painter in each listing view.
use super::dispatch::test_shell::{land, open_root_with};
use super::startup::View;
use super::u07_tests::read;
use flea::{
    jsondoc::{self, Json},
    protocol::RowFacts,
};

fn check(cx: &mut gpui::TestAppContext, view: View) {
    let settings = format!(
        r#""keys":"default","view":"{}","langMarks":true,"columnsLimit":3,"preview":{{"column":false,"thumbnails":"off"}}"#,
        crate::visual::view_name(view)
    );
    let (shell, cx) = open_root_with(cx, &settings, None);
    cx.update(|window, _| window.activate_window());
    let row = |name: &str| RowFacts {
        name: name.into(),
        ..Default::default()
    };
    land(
        &shell,
        cx,
        "/files",
        vec![row("main.rs"), row("notes.ini")],
        1,
    );
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let state = jsondoc::parse(&read(&shell, cx, "langmarksState", &[])).unwrap();
    assert_eq!(
        state.get("view").and_then(Json::as_str),
        Some(crate::visual::view_name(view))
    );
    let rows = state.get("rows").and_then(Json::as_array).unwrap();
    assert_eq!(rows.len(), 2, "{state:?}");
    let code = rows
        .iter()
        .find(|r| r.get("name").and_then(Json::as_str) == Some("main.rs"))
        .unwrap();
    assert_eq!(code.get("slug").and_then(Json::as_str), Some("rust"));
    assert_eq!(code.get("built").and_then(Json::as_bool), Some(true));
    let fallback = rows
        .iter()
        .find(|r| r.get("name").and_then(Json::as_str) == Some("notes.ini"))
        .unwrap();
    assert_eq!(fallback.get("built").and_then(Json::as_bool), Some(false));
    if view == View::Columns {
        let parked = cx.update(|_, cx| {
            shell.update(cx, |shell, _| {
                let painter = shell.panes[shell.focus_side]
                    .as_ref()
                    .unwrap()
                    .columns
                    .as_ref()
                    .unwrap()
                    .painters
                    .get(crate::columns::geometry::Slot::Active)
                    .unwrap();
                painter.borrow_mut().park();
                let parked = painter.borrow().language_paint.borrow().len();
                parked
            })
        });
        assert_eq!(parked, 0, "park drops the painter's language record itself");
        let state = jsondoc::parse(&read(&shell, cx, "langmarksState", &[])).unwrap();
        assert!(
            state
                .get("rows")
                .and_then(Json::as_array)
                .unwrap()
                .is_empty(),
            "cached unpainted tracks are absent"
        );
    } else {
        cx.update(|_, cx| {
            shell.update(cx, |shell, _| {
                let host = shell.panes[shell.focus_side].as_ref().unwrap();
                if view == View::Grid {
                    let grid = host.grid.as_ref().unwrap().state.borrow();
                    grid.language_paint.borrow_mut().clear();
                } else {
                    host.list.borrow().language_paint.borrow_mut().clear();
                }
            })
        });
        let state = jsondoc::parse(&read(&shell, cx, "langmarksState", &[])).unwrap();
        assert!(
            state
                .get("rows")
                .and_then(Json::as_array)
                .unwrap()
                .is_empty(),
            "listed but unpainted rows are absent: {state:?}"
        );
    }
}

#[gpui::test]
fn uish2a_language_receipt_reads_only_the_list_painter(cx: &mut gpui::TestAppContext) {
    check(cx, View::List);
}

#[gpui::test]
fn uish2a_language_receipt_reads_only_the_grid_painter(cx: &mut gpui::TestAppContext) {
    check(cx, View::Grid);
}

#[gpui::test]
fn uish2a_language_receipt_excludes_parked_columns(cx: &mut gpui::TestAppContext) {
    check(cx, View::Columns);
}
