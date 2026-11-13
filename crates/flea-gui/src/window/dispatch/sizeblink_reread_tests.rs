//! A same-folder re-read after a write: one read per change, and walked folder sizes stay on the rows the view keeps.
use super::super::{lists, receive, serve, DONE, WATCH};
use crate::{
    backend::Event as BackendEvent, list::sizes::Reply as SizeReply, navigation::sort::Size,
    state::Shell, window::dispatch::test_shell::open_in, window::navigate::Step,
};
use flea::protocol::RowFacts;
use gpui::{Entity, TestAppContext, VisualTestContext};

const TOTAL: usize = 400;

fn named(index: usize) -> String {
    format!("d{index:04}")
}

// Even rows are folders with their own identity, odd rows are files.
fn entries(gone: Option<usize>) -> Vec<RowFacts> {
    (0..TOTAL)
        .filter(|index| Some(*index) != gone)
        .map(|index| RowFacts {
            source: Some(format!(
                "1:{index}:{}",
                if index.is_multiple_of(2) {
                    0o040755
                } else {
                    0o100644
                }
            )),
            name: named(index),
            dir: index.is_multiple_of(2),
            ..RowFacts::default()
        })
        .collect()
}

fn bytes(name: &str) -> u64 {
    1000 + name[1..].parse::<u64>().unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Write {
    Transfer,
    Refresh,
}

// The write's own re-read, with the folder's change notice before its request or after it.
fn reads_for(cx: &mut TestAppContext, write: Write, gone: usize, notice_first: bool) -> usize {
    let (shell, cx) = open_in(cx, "default", "list");
    serve(&shell, cx, 0, "/files", &entries(None));
    let before = lists(&shell, cx, 0);
    let changed = || BackendEvent::Changed {
        path: "/files".into(),
    };
    if notice_first {
        receive(&shell, cx, 0, changed());
    }
    match write {
        Write::Transfer => receive(&shell, cx, 0, BackendEvent::Operations(DONE.into())),
        Write::Refresh => {
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    let _ = shell.step(0, Step::Refresh(None), window, cx);
                })
            });
            cx.run_until_parked();
        }
    }
    if !notice_first {
        receive(&shell, cx, 0, changed());
    }
    serve(&shell, cx, 0, "/files", &entries(Some(gone)));
    cx.executor().advance_clock(WATCH * 3);
    cx.run_until_parked();
    serve(&shell, cx, 0, "/files", &entries(Some(gone)));
    let reads = lists(&shell, cx, 0) - before;
    shell.read_with(cx, |shell, _| {
        let nav = &shell.panes[0].as_ref().unwrap().nav;
        eprintln!(
            "FX-SIZEBLINK reader write={write:?} moved={} notice_first={notice_first}: listRequests={reads} watchNotices={} stale={} due={:?}",
            if gone.is_multiple_of(2) { "folder" } else { "file" },
            nav.watch.notices,
            nav.watch.stale,
            nav.watch.due
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    reads
}

#[gpui::test]
fn fxsizeblink_a_move_reads_once_when_its_notice_came_first_and_twice_when_it_came_after(
    cx: &mut TestAppContext,
) {
    for write in [Write::Transfer, Write::Refresh] {
        for gone in [10, 11] {
            assert_eq!(
                reads_for(cx, write, gone, true),
                1,
                "{write:?} {gone}: the read covers the notice before it"
            );
            assert_eq!(
                reads_for(cx, write, gone, false),
                2,
                "{write:?} {gone}: a notice after the request is read again"
            );
        }
    }
}

fn shown(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<(String, Option<Size>, bool)> {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let first = (host.pane.scroll_y / host.pane.row_height()) as usize;
        let listing = host.pane.held.listing;
        (first..first + host.pane.visible_rows())
            .filter_map(|index| {
                let row = host.pane.held.row(index)?;
                row.dir.then(|| {
                    let walked = host.list.borrow().view.walked_for(listing, index).is_some();
                    (
                        row.name.clone(),
                        host.sizes.dir_sizes().size_for(index),
                        walked,
                    )
                })
            })
            .collect()
    })
}

// No walk answers after the move, so every size drawn on the kept deep place is a carried one.
#[gpui::test]
fn fxsizeblink_a_deep_list_keeps_its_folder_sizes_through_the_transfer_reread(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_in(cx, "default", "list");
    serve(&shell, cx, 0, "/files", &entries(None));
    cx.simulate_keystrokes("end");
    serve(&shell, cx, 0, "/files", &entries(None));
    cx.simulate_keystrokes("pageup pageup pageup pageup");
    serve(&shell, cx, 0, "/files", &entries(None));
    // The filesystem class is what lets the pane walk its folders at all.
    let local = SizeReply::Filesystem {
        path: "/files".into(),
        class: "local".into(),
    };
    receive(&shell, cx, 0, BackendEvent::ListDetails(local));
    serve(&shell, cx, 0, "/files", &entries(None));
    let answers = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.pane.scroll_y > 1000., "the list is scrolled deep");
        let listing = host.pane.held.listing;
        let first = (host.pane.scroll_y / host.pane.row_height()) as usize;
        (first..first + host.pane.visible_rows())
            .filter_map(|index| {
                let row = host.pane.held.row(index).filter(|row| row.dir)?;
                Some(SizeReply::Sized {
                    listing: Some(listing),
                    row: index,
                    size: Size {
                        bytes: bytes(&row.name),
                        partial: false,
                    },
                })
            })
            .collect::<Vec<_>>()
    });
    assert!(
        !answers.is_empty(),
        "the drawn folders asked for their sizes"
    );
    for answer in answers {
        receive(&shell, cx, 0, BackendEvent::ListDetails(answer));
    }
    let before = shown(&shell, cx);
    assert!(
        before
            .iter()
            .all(|(_, size, walked)| size.is_some() && *walked),
        "{before:?}"
    );
    // A folder far above the view moves away: every drawn row's number changes by one.
    receive(&shell, cx, 0, BackendEvent::Operations(DONE.into()));
    serve(&shell, cx, 0, "/files", &entries(Some(10)));
    let after = shown(&shell, cx);
    assert_eq!(
        after.iter().map(|(name, ..)| name).collect::<Vec<_>>(),
        before.iter().map(|(name, ..)| name).collect::<Vec<_>>(),
        "the re-read kept the drawn place"
    );
    for (name, size, walked) in &after {
        assert_eq!(
            size.map(|size| size.bytes),
            Some(bytes(name)),
            "{name} keeps its own size"
        );
        assert!(walked, "{name} paints its size, not the pending mark");
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.nav.refresh(&mut host.pane, None).unwrap());
            assert!(host.pane.busy());
            shell.settle_list_sizes(window, cx);
        });
    });
    assert_eq!(
        shown(&shell, cx),
        after,
        "the deep busy read keeps every painted size"
    );
    serve(&shell, cx, 0, "/files", &entries(Some(10)));
    assert_eq!(
        shown(&shell, cx),
        after,
        "the next deep landing keeps every painted size"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
