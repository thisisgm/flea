//! EY-505: a live search re-answer shows the new hit and keeps the cursor on the file it stood on, as PaneSwap.qml does.
use crate::pane::Pane;
use flea::{
    protocol::{Reply, RowFacts},
    ui_logic_sr1::{Mode, Session},
};

const HITS: [&str; 3] = ["image-bmp.bmp", "image-gif.gif", "image-ico.ico"];
const MORE: [&str; 4] = [
    "image-000.txt",
    "image-bmp.bmp",
    "image-gif.gif",
    "image-ico.ico",
];

fn rows(names: &[&str]) -> Vec<RowFacts> {
    names
        .iter()
        .map(|name| RowFacts {
            name: (*name).into(),
            ..RowFacts::default()
        })
        .collect()
}

fn land(pane: &mut Pane, names: &[&str], listing: usize, now: u64) {
    let listed = Reply::Listed {
        path: "/k".into(),
        total: names.len(),
    };
    assert!(!pane.receive_session(listed, 2, now).unwrap());
    let rows = Reply::Rows {
        start: 0,
        rows: rows(names),
        listing,
    };
    assert!(pane.receive_session(rows, 2, now + 1).unwrap());
}

// The folder /k lists, then the search for the query answers on it and the cursor goes two rows down, to image-ico.ico.
fn searched(query: &str) -> Pane {
    let mut pane = Pane::new("/k".into(), 620.);
    let folder = ["a", "b", "c", "d"];
    pane.receive(Reply::Listed {
        path: "/k".into(),
        total: folder.len(),
    })
    .unwrap();
    assert!(pane
        .receive(Reply::Rows {
            start: 0,
            rows: rows(&folder),
            listing: 1
        })
        .unwrap());
    pane.swap.session = super::Search {
        session: Session {
            mode: Mode::Typing,
            gen: 2,
            query: query.into(),
            ..Session::default()
        },
        ..super::Search::default()
    }
    .pane_session();
    land(&mut pane, &HITS, 2, 10);
    assert_eq!(pane.cursor, 0, "a first answer starts at the window");
    pane.cursor = 2;
    pane
}

#[test]
fn fixlogic_ey505_a_reanswer_keeps_the_cursor_on_its_file() {
    let mut pane = searched("image");
    land(&mut pane, &MORE, 3, 20);
    assert_eq!(pane.held.total, 4, "the new hit is shown");
    assert_eq!(pane.cursor, 3);
    assert_eq!(
        pane.held.row(pane.cursor).map(|row| row.name.as_str()),
        Some("image-ico.ico")
    );
}

#[test]
fn fixlogic_ey505_a_reanswer_without_the_file_or_for_a_new_query_starts_at_the_window() {
    let mut pane = searched("image");
    land(&mut pane, &MORE[..3], 3, 20);
    assert_eq!(
        pane.cursor, 0,
        "the file is gone, so the cursor takes the window start"
    );
    let mut pane = searched("image");
    pane.swap.session.query = "imag".into();
    land(&mut pane, &MORE, 3, 20);
    assert_eq!(pane.cursor, 0, "another query is a new answer");
}
