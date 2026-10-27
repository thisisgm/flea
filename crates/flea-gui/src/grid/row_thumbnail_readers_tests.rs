//! The shipped source URL, raw thumbnail file and Qt decoder states through the real IPC route.
use super::*;
use crate::ipc::server::Event as IpcEvent;
use flea::ipc::wire::{Reply, Request};

fn ipc(cx: &mut TestAppContext, handle: WindowHandle<Shell>, function: &str, row: usize) -> Reply {
    handle
        .update(cx, |shell, window, cx| {
            let (answer, reply) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                IpcEvent::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: vec![row.to_string()],
                    },
                    answer,
                },
                window,
                cx,
            );
            reply.try_recv().unwrap().reply
        })
        .unwrap()
}

#[gpui::test]
fn empty_thumbnail_source_has_null_status_in_every_row_view(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns, View::Dual, View::Grid] {
        let (handle, _lines) = open(cx, view, "{}");
        step(cx, handle);
        assert_eq!(
            ipc(cx, handle, "rowIcon", 0),
            Reply::value(""),
            "{view:?}: no fallback glyph in Image.source"
        );
        assert_eq!(
            ipc(cx, handle, "rowIconStatus", 0),
            Reply::value("0"),
            "{view:?}: Image.Null"
        );
        assert_eq!(ipc(cx, handle, "rowThumb", 0), Reply::value(""));
        assert_eq!(ipc(cx, handle, "rowIconStatus", 99999), Reply::value("-1"));
    }
}

#[gpui::test]
fn thumbnail_readers_keep_raw_file_and_source_url_through_decode(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns, View::Dual, View::Grid] {
        let (handle, mut lines) = open(cx, view, "{}");
        first_settle(cx, handle, &mut lines);
        assert!(deliver(cx, handle, &thumbed(1, PICTURE)));
        assert_eq!(ipc(cx, handle, "thumbFile", 1), Reply::value(PICTURE));
        assert_eq!(ipc(cx, handle, "rowThumb", 1), Reply::value(PICTURE));
        assert_eq!(
            ipc(cx, handle, "rowIcon", 1),
            Reply::value(format!("file://{PICTURE}?m=1700000000"))
        );
        assert_eq!(
            ipc(cx, handle, "rowIconStatus", 1),
            Reply::value("2"),
            "{view:?}: Image.Loading before decoder delivery"
        );
        cx.run_until_parked();
        step(cx, handle);
        assert_eq!(
            ipc(cx, handle, "rowIconStatus", 1),
            Reply::value("1"),
            "{view:?}: Image.Ready"
        );
    }
}

#[gpui::test]
fn failed_thumbnail_decode_keeps_escaped_source_and_error_status(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    first_settle(cx, handle, &mut lines);
    let bad = concat!(env!("CARGO_MANIFEST_DIR"), "/missing thumbnail.png");
    assert!(deliver(cx, handle, &thumbed(1, bad)));
    cx.run_until_parked();
    step(cx, handle);
    assert_eq!(ipc(cx, handle, "rowIconStatus", 1), Reply::value("3"));
    assert_eq!(ipc(cx, handle, "thumbFile", 1), Reply::value(bad));
    assert_eq!(ipc(cx, handle, "rowThumb", 1), Reply::value(bad));
    assert_eq!(
        ipc(cx, handle, "rowIcon", 1),
        Reply::value(format!(
            "file://{}/missing%20thumbnail.png?m=1700000000",
            env!("CARGO_MANIFEST_DIR")
        ))
    );
}

#[gpui::test]
fn thumbnail_readers_follow_policy_and_the_current_row_identity(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    first_settle(cx, handle, &mut lines);
    assert!(deliver(cx, handle, &thumbed(0, PICTURE)));
    cx.run_until_parked();
    step(cx, handle);
    assert_eq!(ipc(cx, handle, "thumbFile", 0), Reply::value(PICTURE));
    handle
        .update(cx, |shell, _, _| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"preview":{"thumbnails":"images"}}"#).unwrap()
        })
        .unwrap();
    step(cx, handle);
    assert_eq!(
        ipc(cx, handle, "thumbFile", 0),
        Reply::value(""),
        "Images hides an old media answer"
    );
    assert_eq!(ipc(cx, handle, "rowIconStatus", 0), Reply::value("0"));
    handle
        .update(cx, |shell, _, _| {
            shell.launch.settings.data = Json::Obj(vec![]);
        })
        .unwrap();
    step(cx, handle);
    assert_eq!(
        ipc(cx, handle, "thumbFile", 0),
        Reply::value(PICTURE),
        "Media exposes the source before its identity changes"
    );
    handle
        .update(cx, |shell, _, _| {
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            let mut rows = held.rows.clone();
            rows[0].mtime += 1;
            *held = Rc::new(crate::list::pane::Held { rows, ..**held });
        })
        .unwrap();
    assert_eq!(
        ipc(cx, handle, "thumbFile", 0),
        Reply::value(""),
        "changed source facts invalidate the reader immediately"
    );
    assert_eq!(ipc(cx, handle, "rowIconStatus", 0), Reply::value("0"));
}

#[gpui::test]
fn cut_list_and_columns_rows_paint_the_faded_copy(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns] {
        let (handle, mut lines) = open(cx, view, "{}");
        first_settle(cx, handle, &mut lines);
        deliver(cx, handle, &thumbed(1, PICTURE));
        cx.run_until_parked();
        step(cx, handle);
        let shown_picture = |shell: &Shell| {
            let host = shell.panes[0].as_ref().unwrap();
            if view == View::Columns {
                let paint = host
                    .columns
                    .as_ref()
                    .unwrap()
                    .painters
                    .get(Slot::Active)
                    .unwrap()
                    .borrow();
                assert_eq!(paint.clip(1), crate::columns::paint::Clip::Cut);
                std::sync::Arc::clone(paint.picture(1).unwrap())
            } else {
                let list = host.list.borrow();
                let shown = list.visible().iter().find(|row| row.index == 1).unwrap();
                assert_eq!(shown.clip, Some(crate::row::marks::ClipMark::Cut));
                std::sync::Arc::clone(shown.picture.as_ref().unwrap())
            }
        };
        let source = read(cx, handle, |shell| {
            pictures(shell)
                .lend(
                    1,
                    shell.panes[0].as_ref().unwrap().pane.held.row(1).unwrap(),
                )
                .unwrap()
        });
        handle.update(cx, |shell, window, cx| {
            shell.receive(0, Backend::test_reply_line(r#"{"t":"clip","op":"changed","clip":"cut","paths":["/files/photo.png"],"token":"cut-row","skipped":0}"#), window, cx);
            shell.sync_clip_marks();
        }).unwrap();
        step(cx, handle);
        let cut = read(cx, handle, shown_picture);
        assert!(
            !std::sync::Arc::ptr_eq(&source, &cut),
            "{view:?}: the actual row holds a faded image"
        );
        let level = if view == View::Columns {
            crate::columns::paint::CUT
        } else {
            crate::list::CUT_DIM
        };
        let original = source.image.as_bytes(0).unwrap();
        let faded = cut.image.as_bytes(0).unwrap();
        assert!(original.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
        for (before, after) in original
            .as_chunks::<4>()
            .0
            .iter()
            .zip(faded.as_chunks::<4>().0)
        {
            assert_eq!(
                &before[..3],
                &after[..3],
                "{view:?}: unchanged colour channels"
            );
            assert_eq!(
                after[3],
                (f32::from(before[3]) * level).round() as u8,
                "{view:?}: row alpha"
            );
        }
        assert_eq!(read(cx, handle, |shell| painted(shell, view)), 1);
        step(cx, handle);
        assert!(
            std::sync::Arc::ptr_eq(&cut, &read(cx, handle, shown_picture)),
            "{view:?}: still frames reuse the copy"
        );
    }
}

// The box a drawn row's thumbnail fills, from the drawn frame: Row.qml's mark slot centred in the row (List), or the track's 23 px slot (Columns).
fn thumb_box(cx: &mut TestAppContext, handle: WindowHandle<Shell>, view: View) -> (f32, f32, f32) {
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            if view == View::Columns {
                let paint = host
                    .columns
                    .as_ref()
                    .unwrap()
                    .painters
                    .get(Slot::Active)
                    .unwrap()
                    .borrow();
                let drawn = paint.drawn.as_ref().unwrap();
                let row = crate::list::pane::ROW_HEIGHT;
                let top = f32::from(drawn.bounds.origin.y) + row - drawn.scroll.max(0.);
                let side = crate::columns::geometry::MARK_SLOT;
                (
                    f32::from(drawn.bounds.origin.x) + crate::columns::geometry::PAD_X,
                    top + (row - side) / 2.,
                    side,
                )
            } else {
                let list = host.list.borrow();
                let drawn = &list.drawn;
                let metrics = &drawn.metrics;
                let top = f32::from(drawn.bounds.origin.y) + metrics.row - drawn.scroll_y;
                (
                    f32::from(drawn.bounds.origin.x) + metrics.pad,
                    top + (metrics.row - metrics.slot) / 2.,
                    metrics.slot,
                )
            }
        })
        .unwrap()
}

#[gpui::test]
fn readers3_row_thumb_ready_and_rect_follow_the_drawn_thumbnail(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns] {
        let (handle, mut lines) = open(cx, view, "{}");
        first_settle(cx, handle, &mut lines);
        let (x, y, side) = thumb_box(cx, handle, view);
        let rect = crate::ipc::rows::rect_of(x, y, side, side);
        assert_eq!(
            ipc(cx, handle, "rowThumbRect", 1),
            Reply::value(rect.clone()),
            "{view:?}: the drawn thumbnail box while the row is drawn"
        );
        assert_eq!(
            ipc(cx, handle, "rowThumbReady", 1),
            Reply::value("false"),
            "{view:?}: no decoded thumbnail yet"
        );
        assert!(deliver(cx, handle, &thumbed(1, PICTURE)));
        cx.run_until_parked();
        step(cx, handle);
        assert_eq!(
            ipc(cx, handle, "rowThumbReady", 1),
            Reply::value("true"),
            "{view:?}: Image.Ready on the drawn row"
        );
        assert_eq!(
            ipc(cx, handle, "rowThumbRect", 1),
            Reply::value(rect),
            "{view:?}: the box is unchanged once decoded"
        );
    }
}
