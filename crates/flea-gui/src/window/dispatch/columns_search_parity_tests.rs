//! Search input publishes basename and location to the active Columns row painter.
use super::{previewtypes_tests as host, search_open_tests};
use crate::columns::geometry::Slot;
use gpui::{TestAppContext, TextRun};

#[gpui::test]
fn pb_columns2_ey173_search_rows_report_the_drawn_basename_and_location(cx: &mut TestAppContext) {
    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file("notes.txt", "text-plain", "Text", 0)],
        1,
    );
    cx.simulate_keystrokes("f n");
    search_open_tests::answer(
        &shell,
        cx,
        1,
        vec![
            host::file("notes.txt", "text-plain", "Text", 0),
            host::file("Docs/notes.txt", "text-plain", "Text", 0),
        ],
    );
    cx.simulate_keystrokes("home");
    host::pump(cx);
    cx.update(|window, cx| {
        shell.read_with(cx, |shell, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns
                .painters
                .get(Slot::Active)
                .expect("active column")
                .borrow();
            let row = paint.prepared(1).expect("search row is painted");
            assert_eq!(
                (row.label.as_str(), row.location_label.as_str()),
                ("notes.txt", "Docs"),
                "EY-173: read the exact name and location bytes used for paint"
            );
            let root_row = paint.prepared(0).unwrap();
            assert_eq!(
                (root_row.label.as_str(), root_row.location_label.as_str()),
                ("notes.txt", "")
            );
            assert!(
                row.location
                    .as_ref()
                    .is_some_and(|location| location.line().width > gpui::px(0.)),
                "folder location is prepared alongside the basename"
            );
            let face = &shell.panes[0].as_ref().unwrap().list.borrow().typeface;
            let text = "notes.txt";
            let run = TextRun {
                len: text.len(),
                font: face.font.clone(),
                color: gpui::Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let expected = window
                .text_system()
                .layout_line(text, face.body, &[run], None)
                .width;
            assert_eq!(
                row.plain.line().width,
                expected,
                "the name cell must show just the basename"
            );
        })
    });
}

// A search row's typed run is found in the drawn basename, so its wash starts on the name, not past the folder.
#[gpui::test]
fn search_rows_wash_the_typed_run_inside_the_drawn_basename(cx: &mut TestAppContext) {
    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file("notes.txt", "text-plain", "Text", 0)],
        1,
    );
    cx.simulate_keystrokes("f n");
    search_open_tests::answer(
        &shell,
        cx,
        1,
        vec![
            host::file("notes.txt", "text-plain", "Text", 0),
            host::file("Docs/notes.txt", "text-plain", "Text", 0),
        ],
    );
    cx.simulate_keystrokes("home");
    host::pump(cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.state.query, "n",
            "the search query reaches the painter"
        );
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .expect("active column")
            .borrow();
        let row = paint.prepared(1).expect("search row is painted");
        assert_eq!(
            (row.label.as_str(), row.location_label.as_str()),
            ("notes.txt", "Docs")
        );
        let (from, to) = row
            .wash_span(false, false)
            .expect("the typed run is washed");
        assert_eq!(
            from,
            gpui::px(0.),
            "the run starts at the basename's first letter"
        );
        assert!(to > from);
    });
}
