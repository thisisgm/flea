//! The actual sheet result must reveal the same search row as the direct o key.
use super::*;
use gpui::VisualTestContext;

#[gpui::test]
fn pvset_repro_sheet_reveal_routes_the_live_search_result(cx: &mut TestAppContext) {
    let (handle, _read) = contents_needle(cx);
    snapshot(cx, handle, 6, 16, "A/alpha.txt", true);
    edit(cx, handle, |shell, window, cx| shell.open_sheet(window, cx));
    frame(cx, handle);
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("r e v e a l");
    frame(cx, handle);
    assert!(super::super::tests::call(cx, handle, "keymapSheetRows")
        .stdout
        .contains("Reveal"));
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("enter");
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.settings.sheet.is_none(), "Return closes the sheet");
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.select.as_deref(), Some("/files/A/alpha.txt"));
    });
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            BackendEvent::Reply(flea::protocol::Reply::Listed {
                path: "/files/A".into(),
                total: 3,
            }),
            window,
            cx,
        );
        shell.receive(
            0,
            BackendEvent::Reply(flea::protocol::Reply::Rows {
                start: 0,
                listing: 17,
                rows: [("empty", true), ("full", true), ("alpha.txt", false)]
                    .into_iter()
                    .map(|(name, dir)| RowFacts {
                        name: name.into(),
                        dir,
                        icon: if dir { "folder" } else { "text-plain" }.into(),
                        ..Default::default()
                    })
                    .collect(),
            }),
            window,
            cx,
        );
    });
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        frame(cx, handle);
    }
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            !host.search.owner.active(),
            "the folder replaces the search"
        );
        assert_eq!(host.path, "/files/A");
        assert_eq!(host.pane.cursor, 2);
        assert_eq!(host.pane.held.row(2).unwrap().name, "alpha.txt");
    });
}
