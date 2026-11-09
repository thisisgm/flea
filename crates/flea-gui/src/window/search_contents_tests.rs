//! Mounted contents search: Ctrl+G, the producer's match lines joined to their rows, and the match line kept.
use super::tests::{control_key, edit, frame, key, open, reply, results, Handle};
use crate::backend::Event as BackendEvent;
use flea::{
    protocol::RowFacts,
    ui_logic_file_search::{ContentsMatch, ContentsMatches, Kind},
};
use gpui::TestAppContext;

#[path = "pvset_reveal_tests.rs"]
mod pvset_reveal;
#[path = "search_contents_quick_look_tests.rs"]
mod quick_look_match;
#[path = "search_contents_real_paint_tests.rs"]
mod real_paint;
#[path = "search_contents_regression_tests.rs"]
mod regressions;

fn paths(path: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|at| {
            if at == 0 {
                path.to_owned()
            } else {
                format!("src/f{at}.rs")
            }
        })
        .collect()
}

fn matches(gen: u64, listing: u64, path: &str, operable: bool) -> String {
    matches_n(gen, listing, &paths(path, 1), operable)
}

fn matches_n(gen: u64, listing: u64, paths: &[String], operable: bool) -> String {
    ContentsMatches {
        gen,
        listing,
        start: 0,
        matches: paths
            .iter()
            .enumerate()
            .map(|(row, path)| ContentsMatch {
                row,
                path: path.clone(),
                path_bytes: path.as_bytes().to_vec(),
                line_no: 3,
                line: "let needle = load();".into(),
                operable,
            })
            .collect(),
    }
    .line()
}

// The producer's order for one snapshot: match lines, then listed, rows and the terminal searched.
fn snapshot(
    cx: &mut TestAppContext,
    handle: Handle,
    gen: usize,
    listing: usize,
    path: &str,
    operable: bool,
) {
    snapshot_n(cx, handle, gen, listing, &paths(path, 1), operable);
}

fn snapshot_n(
    cx: &mut TestAppContext,
    handle: Handle,
    gen: usize,
    listing: usize,
    paths: &[String],
    operable: bool,
) {
    let count = paths.len();
    reply(
        cx,
        handle,
        0,
        &matches_n(gen as u64, listing as u64, paths, operable),
    );
    reply(
        cx,
        handle,
        0,
        &format!(r#"{{"t":"listed","path":"/files","n":{count},"gen":{gen}}}"#),
    );
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            BackendEvent::Session {
                generation: gen,
                reply: flea::protocol::Reply::Rows {
                    start: 0,
                    listing,
                    rows: paths
                        .iter()
                        .map(|path| RowFacts {
                            name: path.clone(),
                            icon: "text-plain".into(),
                            ..Default::default()
                        })
                        .collect(),
                },
            },
            window,
            cx,
        )
    });
    reply(
        cx,
        handle,
        0,
        &format!(
            r#"{{"t":"searched","n":{count},"scanned":4,"ms":1,"cancelled":false,"gen":{gen},"walking":false}}"#
        ),
    );
    frame(cx, handle);
}

// Ctrl+G in the listing, then the query typed key by key: gen 6 is "needle".
fn contents_needle(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    while read().is_some() {}
    control_key(cx, handle, "g");
    for letter in ["n", "e", "e", "d", "l", "e"] {
        key(cx, handle, letter, Some(letter));
    }
    (handle, read)
}

#[gpui::test]
fn m9_g2rows_12_ctrl_g_dispatches_shared_contents_wire(cx: &mut TestAppContext) {
    let sheet = flea::keys::BASE_SHEET
        .iter()
        .find(|row| row.action == "searchContents")
        .map(|row| row.keys);
    assert_eq!(sheet, Some("^g"), "the sheet names Ctrl+G Search contents");
    let find = flea::keys::SHEET_GROUPS
        .iter()
        .find(|group| group.id == "find")
        .unwrap();
    assert_eq!(&find.actions[..3], ["filter", "search", "searchContents"]);

    let (handle, mut read) = open(cx);
    frame(cx, handle);
    while read().is_some() {}
    // Negative control: without the shared producer Ctrl+G opens nothing and sends nothing.
    edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_mut()
            .unwrap()
            .search
            .owner
            .session
            .wire
            .contents = false;
    });
    control_key(cx, handle, "g");
    assert_eq!(read(), None, "no producer, no request");
    edit(cx, handle, |shell, _, _| {
        let search = &mut shell.panes[0].as_mut().unwrap().search.owner;
        assert!(!search.active());
        search.session.wire.contents = true;
    });

    control_key(cx, handle, "g");
    let opened = read().expect("Ctrl+G opens the backend search session");
    assert_eq!(
        flea::json::field_str(&opened, "c").as_deref(),
        Some("searchopen")
    );
    let path = edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.search.owner.session.kind,
            Kind::Contents,
            "the strip opens already in contents"
        );
        assert!(
            host.search.owner.typing(),
            "the query line takes the next key"
        );
        host.path.clone()
    });
    key(cx, handle, "n", Some("n"));
    let sent = read().expect("a typed key sends one contents query");
    assert_eq!(
        flea::json::field_str(&sent, "c").as_deref(),
        Some("searchcontents"),
        "{sent}"
    );
    assert_eq!(
        flea::json::field_str(&sent, "path").as_deref(),
        Some(path.as_str()),
        "{sent}"
    );
    assert_eq!(
        flea::json::field_str(&sent, "query").as_deref(),
        Some("n"),
        "{sent}"
    );
    assert_eq!(read(), None, "one request per key, no second producer");
}

#[gpui::test]
fn m9_g2rows_12_match_blocks_publish_as_one_generation(cx: &mut TestAppContext) {
    let (handle, _read) = contents_needle(cx);
    // Lines for a gen the strip already replaced are refused whole.
    reply(cx, handle, 0, &matches(5, 15, "old.rs", true));
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.search.owner.session.gen, 6);
        assert_eq!(
            host.search.owner.matches.line(15, 0),
            None,
            "a stale gen never joins"
        );
        assert!(
            shell.contents_blocks(0).is_none(),
            "the folder's rows have no lines"
        );
    });
    // The lines arrive first; until their rows present, the drawn snapshot is not a mix of the two.
    reply(cx, handle, 0, &matches(6, 16, "src/main.rs", true));
    edit(cx, handle, |shell, _, _| {
        assert!(
            shell.contents_blocks(0).is_none(),
            "lines alone never present"
        );
    });
    snapshot(cx, handle, 6, 16, "src/main.rs", true);
    edit(cx, handle, |shell, _, _| {
        let held = shell
            .contents_blocks(0)
            .expect("rows and lines of gen 6 present together");
        assert_eq!((held.listing, held.total, held.start), (16, 1, 0));
        let block = &held.blocks[0];
        assert_eq!(block.row.name, "main.rs");
        assert_eq!(block.location, "src");
        assert_eq!(
            (block.line_no, block.line.as_str()),
            (3, "let needle = load();")
        );
    });
    // Rows of another listing never borrow these lines, so nothing draws without its own line.
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let place = crate::g2::file_search::matches::Place {
            root: "/files",
            home: "",
            scope: flea::ui_logic_file_search::Scope::Folder,
        };
        let rows = host.pane.held.rows.clone();
        assert!(host
            .search
            .owner
            .matches
            .blocks(17, 1, 0, &rows, &place)
            .is_none());
        assert!(host
            .search
            .owner
            .matches
            .blocks(16, 1, 0, &rows, &place)
            .is_some());
    });
}

#[gpui::test]
fn m9_g2rows_12_open_reveal_and_quicklook_keep_match_line(cx: &mut TestAppContext) {
    let (handle, mut read) = contents_needle(cx);
    snapshot(cx, handle, 6, 16, "src/main.rs", true);
    results(cx, handle);
    while read().is_some() {}
    edit(cx, handle, |shell, _, _| {
        // Quick Look's seam (G2ROWS-27): the cursor row's match line from the search owner.
        let host = shell.panes[0].as_ref().unwrap();
        let line = host
            .search
            .owner
            .match_line(host.pane.held.listing, host.pane.cursor)
            .expect("the cursor's block has its line");
        assert_eq!(
            (line.number, line.text.as_str()),
            (3, "let needle = load();")
        );
        assert_eq!(
            line.run,
            Some((4, 10)),
            "Quick Look gets the typed run, not only the line"
        );
    });
    let path = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().path.clone()
    });
    key(cx, handle, "o", Some("o"));
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.select.as_deref(),
            Some(format!("{path}/src/main.rs").as_str()),
            "o reveals the full path"
        );
    });
}

#[gpui::test]
fn m9_g2rows_12_raw_filename_is_never_revealed_from_its_display(cx: &mut TestAppContext) {
    let (handle, mut read) = contents_needle(cx);
    snapshot(cx, handle, 6, 16, "src/main.rs", false);
    results(cx, handle);
    while read().is_some() {}
    key(cx, handle, "o", Some("o"));
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.select, None, "an inoperable row takes no reveal");
        assert!(host.search.owner.active(), "the search stays where it was");
    });
    assert_eq!(read(), None, "no listing request for an inoperable row");
}

#[gpui::test]
fn m9_g2rows_12_hit_test_and_page_keys_land_on_blocks(cx: &mut TestAppContext) {
    use crate::g2::file_search::contents::BLOCK_H;
    let (handle, mut read) = contents_needle(cx);
    snapshot_n(cx, handle, 6, 16, &paths("src/main.rs", 60), true);
    key(cx, handle, "home", None);
    frame(cx, handle);
    while read().is_some() {}
    let (frame_row, viewport) = edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.list.borrow().view.contents,
            "a contents snapshot draws blocks"
        );
        assert_eq!(host.list.borrow().file_metrics().row, BLOCK_H);
        let frame = host
            .input
            .frame
            .expect("the list frame of the drawn blocks");
        // The third block's matching line, below its row: a list-height metric would name a later row.
        let (x, y) = (frame.bounds.origin.x, frame.bounds.origin.y);
        let point = gpui::point(
            x + gpui::px(40.),
            y + gpui::px(frame.cell_origin.1 + 2. * BLOCK_H + 40.),
        );
        assert_eq!(
            host.input.hit(point),
            Some(2),
            "a point on block 2's line hits row 2"
        );
        (frame.row, frame.viewport())
    });
    assert_eq!(frame_row, BLOCK_H);
    control_key(cx, handle, "d");
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let cursor = host.pane.cursor;
        assert!(cursor > 0, "the page key moved");
        // One page is the blocks one viewport shows, never the rows of the list's own height.
        assert!(
            cursor as f32 * BLOCK_H <= viewport + BLOCK_H,
            "cursor {cursor} paged by blocks of {BLOCK_H} in {viewport}"
        );
        let top = cursor as f32 * BLOCK_H - host.pane.scroll_y;
        assert!(
            top >= 0. && top + BLOCK_H <= viewport + 0.5,
            "the paged cursor's block is in view: top {top}"
        );
    });
}
