//! The folder of pictures the Grid delete tests list, and the backend answers they feed.
use super::super::super::{
    rename_tests::{draw, receive},
    test_shell::row,
};
use crate::{
    backend::{Backend, Event},
    grid::element::pictures::Policy,
    state::Shell,
};
use flea::protocol::{Reply, RowFacts};
use gpui::{Entity, VisualTestContext};

use super::{IMAGES, NEXT};

const PICTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/images/transfer.png"
);
const WINDOW_REPLIES: usize = 8;

pub(super) fn images() -> Vec<RowFacts> {
    (0..IMAGES)
        .map(|n| RowFacts {
            icon: "image-x-generic".into(),
            mode: 0o100644,
            ..row(&format!("image{n:04}.png"), false)
        })
        .collect()
}

pub(super) fn windows(shell: &Entity<Shell>, cx: &mut VisualTestContext, rows: &[RowFacts]) {
    for answered in 0..=WINDOW_REPLIES {
        draw(cx);
        let request = shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().pane.requests.outstanding
        });
        let Some(request) = request else {
            return;
        };
        assert!(answered < WINDOW_REPLIES, "window requests did not settle");
        receive(
            shell,
            cx,
            Event::Reply(Reply::Rows {
                start: request.start,
                rows: rows[request.start..(request.start + request.count).min(rows.len())].to_vec(),
                listing: request.listing,
            }),
        );
    }
}

// The pending swap's first row count and the listing number its rows answer.
pub(super) fn pending(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, usize) {
    shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        (
            pane.swap.pending.as_ref().unwrap().first,
            pane.held.listing + 1,
        )
    })
}

pub(super) fn thumbnail(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    receive(
        shell,
        cx,
        Backend::test_reply_line(&format!(
            r#"{{"t":"thumbed","row":{NEXT},"file":"{PICTURE}"}}"#
        )),
    );
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let policy = Policy::read(&shell.launch.settings.data);
        let files = host
            .list
            .borrow()
            .pictures
            .policy_files(&host.pane.held, &host.path, policy);
        assert_eq!(
            files.get(&NEXT),
            Some(&Some(PICTURE.into())),
            "the thumbnail answer was accepted"
        );
    });
}
