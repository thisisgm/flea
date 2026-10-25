//! List rows and the active Columns track draw the pane's thumbnails through the Grid's own lender, as v0.3.12's Row.qml and ColumnRow.qml do.
use super::pictures::Pictures;
use crate::{
    backend::{Backend, Event},
    columns::geometry::Slot,
    grid::join_tests::{frame, named_shell},
    state::Shell,
    window::startup::View,
};
use flea::{jsondoc::Json, protocol::RowFacts};
use gpui::{px, size, Context, TestAppContext, Window, WindowHandle};
use std::{rc::Rc, time::Duration};

const PICTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/images/transfer.png");
const LOCAL: &str = r#"{"t":"fsinfo","path":"/files","class":""}"#;

// The Media fixture's first rows, as pictures_tests names them; the rest stay file-N.txt.
fn named(row: &mut RowFacts, at: usize) {
    let (name, icon, dir, mode) = match at {
        0 => ("main.rs", "text-x-generic", false, 0o100644),
        1 => ("photo.png", "image-x-generic", false, 0o100644),
        2 => ("notes.txt", "text-plain", false, 0o100644),
        3 => ("lib.rs", "folder", true, 0o040755),
        4 => ("link.rs", "text-x-generic", false, 0o120777),
        5 => ("clip.mp4", "video-x-generic", false, 0o100644),
        6 => ("Makefile", "text-x-generic", false, 0o100644),
        _ => return,
    };
    row.name = name.into();
    row.icon = icon.into();
    row.dir = dir;
    row.mode = mode;
    row.size = 100 + at as u64;
    row.mtime = 1_700_000_000;
}

// Answers every column the Columns entry still waits on with one folder row, which ends the entry hold on the List.
fn answer_peeks(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    let (out, first, hidden, hidden_last) = {
        let host = shell.panes[0].as_ref().unwrap();
        let folders = host.nav.folders.borrow();
        let Some(columns) = host.columns.as_ref() else {
            return;
        };
        let out: Vec<String> = columns
            .area
            .drawn
            .iter()
            .filter(|path| {
                let key = crate::columns::peeks::peek_key(path, folders.hidden, folders.hidden_last);
                columns.area.peeks.rows(&key).is_none()
            })
            .cloned()
            .collect();
        (out, host.pane.first_count(), folders.hidden, folders.hidden_last)
    };
    for path in out {
        let text = |value: &str| Json::Str(value.into());
        let number = |value: usize| Json::Num(value.to_string());
        let document = Json::Obj(vec![
            ("t".into(), text("peeked")),
            ("path".into(), text(&path)),
            ("first".into(), number(first)),
            ("n".into(), number(1)),
            ("hidden".into(), Json::Bool(hidden)),
            ("hiddenLast".into(), Json::Bool(hidden_last)),
            ("failed".into(), Json::Bool(false)),
            ("mode".into(), number(0)),
            (
                "rows".into(),
                Json::Arr(vec![Json::Obj(vec![
                    ("n".into(), text("x")),
                    ("d".into(), Json::Bool(true)),
                ])]),
            ),
        ]);
        shell.receive(0, Event::Peeked(document), window, cx);
    }
}

fn open(
    cx: &mut TestAppContext,
    view: View,
    settings: &str,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let (backend, lines) = Backend::test_lines();
    let settings = flea::jsondoc::parse(settings).unwrap();
    let handle = cx.open_window(size(px(1400.), px(700.)), |window, cx| {
        let mut shell = named_shell(backend, cx);
        shell.launch.settings.data = settings;
        shell.launch.settings.columns = 3;
        let host = shell.panes[0].as_mut().unwrap();
        for (at, row) in Rc::get_mut(&mut host.pane.held).unwrap().rows.iter_mut().enumerate() {
            named(row, at);
        }
        host.changed(&shell.locale);
        shell.set_view(view, window, cx);
        shell
    });
    for _ in 0..3 {
        frame(cx, handle);
        handle.update(cx, answer_peeks).unwrap();
        cx.run_until_parked();
    }
    (handle, lines)
}

// One frame as production runs it, where render.rs syncs the pictures every frame: settle, sync, draw.
fn step(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| shell.sync_grid_pictures(window, cx))
        .unwrap();
    frame(cx, handle);
}

fn deliver(cx: &mut TestAppContext, handle: WindowHandle<Shell>, line: &str) -> bool {
    let event = Backend::test_reply_line(line);
    handle
        .update(cx, |shell, window, cx| shell.grid_pictures_event(0, &event, window, cx))
        .unwrap()
}

fn settle(cx: &mut TestAppContext, handle: WindowHandle<Shell>, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
    step(cx, handle);
}

fn drained(lines: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    std::iter::from_fn(lines).collect()
}

fn read<R>(cx: &mut TestAppContext, handle: WindowHandle<Shell>, f: impl FnOnce(&Shell) -> R) -> R {
    handle.update(cx, |shell, _, _| f(shell)).unwrap()
}

fn pictures(shell: &Shell) -> Rc<Pictures> {
    Rc::clone(&shell.panes[0].as_ref().unwrap().list.borrow().pictures)
}

fn thumbed(row: usize, file: &str) -> String {
    format!(r#"{{"t":"thumbed","row":{row},"file":"{file}","ms":3}}"#)
}

// The rows of every thumb line, in order.
fn asked(lines: &[String]) -> Vec<usize> {
    lines
        .iter()
        .filter_map(|line| line.strip_prefix(r#"{"c":"thumb","rows":["#))
        .flat_map(|rest| {
            let rows = rest.split(']').next().unwrap_or_default();
            rows.split(',').filter_map(|row| row.parse::<usize>().ok()).collect::<Vec<_>>()
        })
        .collect()
}

// The rows the surface drew last frame, and the picture each drew in its mark slot.
fn drawn(shell: &Shell, view: View) -> Vec<(usize, Option<(u32, u32)>)> {
    let host = shell.panes[0].as_ref().unwrap();
    if view == View::Columns {
        let columns = host.columns.as_ref().expect("the Columns view is mounted");
        let painter = columns.painters.get(Slot::Active).expect("the active track drew").borrow();
        let rows: Vec<usize> = painter.visible().collect();
        return rows
            .into_iter()
            .map(|row| (row, painter.picture(row).map(|picture| picture.decoded())))
            .collect();
    }
    let list = host.list.borrow();
    list.visible()
        .iter()
        .filter(|shown| shown.prepared.is_some())
        .map(|shown| (shown.index, shown.picture.as_ref().map(|picture| picture.decoded())))
        .collect()
}

fn picture_at(shell: &Shell, view: View, row: usize) -> Option<(u32, u32)> {
    drawn(shell, view)
        .into_iter()
        .find(|(index, _)| *index == row)
        .expect("the row is drawn")
        .1
}

// The first settle in a listing, after fsinfo names an ordinary local class.
fn first_settle(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    lines: &mut impl FnMut() -> Option<String>,
) -> Vec<String> {
    step(cx, handle);
    drained(lines);
    deliver(cx, handle, LOCAL);
    step(cx, handle);
    settle(cx, handle, 70);
    drained(lines)
}

fn draws_the_answer_in_place_of_the_kind_mark(cx: &mut TestAppContext, view: View) {
    let (handle, mut lines) = open(cx, view, "{}");
    let first = first_settle(cx, handle, &mut lines);
    let rows = asked(&first);
    assert!(rows.contains(&1) && rows.contains(&5), "{view:?}: the photo and the video are asked: {first:?}");
    assert!(!rows.contains(&3), "{view:?}: a folder is never asked");
    assert!(deliver(cx, handle, &thumbed(1, PICTURE)), "a thumbed line is the pictures'");
    assert!(deliver(cx, handle, &thumbed(5, PICTURE)));
    step(cx, handle);
    assert_eq!(read(cx, handle, |shell| picture_at(shell, view, 1)), None, "{view:?}: the UI thread never decodes");
    cx.run_until_parked();
    step(cx, handle);
    for row in [1, 5] {
        assert_eq!(
            read(cx, handle, |shell| picture_at(shell, view, row)),
            Some((1280, 64)),
            "{view:?}: row {row} draws its thumbnail answer, not its kind glyph"
        );
    }
    for row in [0, 2, 3] {
        assert_eq!(read(cx, handle, |shell| picture_at(shell, view, row)), None, "{view:?}: row {row} keeps its mark");
    }
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!((counters.answered, counters.decoded), (2, 2), "{view:?}");
    assert_eq!(read(cx, handle, |shell| painted(shell, view)), 2, "{view:?}: paint drew both pictures in their mark slots");
}

// Thumbnails the surface's last paint drew, not only the ones prepaint lent.
fn painted(shell: &Shell, view: View) -> usize {
    let host = shell.panes[0].as_ref().unwrap();
    if view == View::Columns {
        let columns = host.columns.as_ref().expect("the Columns view is mounted");
        return columns.painters.get(Slot::Active).expect("the active track drew").borrow().painted_pictures.get();
    }
    host.list.borrow().painted_pictures.get()
}

#[gpui::test]
fn a_list_row_draws_its_thumbnail_answer_in_place_of_the_kind_mark(cx: &mut TestAppContext) {
    draws_the_answer_in_place_of_the_kind_mark(cx, View::List);
}

#[gpui::test]
fn a_columns_row_draws_its_thumbnail_answer_in_place_of_the_kind_mark(cx: &mut TestAppContext) {
    draws_the_answer_in_place_of_the_kind_mark(cx, View::Columns);
}

#[gpui::test]
fn a_dual_pane_row_draws_its_thumbnail_answer(cx: &mut TestAppContext) {
    draws_the_answer_in_place_of_the_kind_mark(cx, View::Dual);
}

fn offscreen_rows_make_no_request(cx: &mut TestAppContext, view: View) {
    let (handle, mut lines) = open(cx, view, "{}");
    let first = first_settle(cx, handle, &mut lines);
    let shown: Vec<usize> = read(cx, handle, |shell| drawn(shell, view)).into_iter().map(|(row, _)| row).collect();
    let total = read(cx, handle, |shell| shell.panes[0].as_ref().unwrap().pane.held.total);
    assert!(shown.len() > 6 && shown.len() < total, "{view:?}: the viewport holds a screenful: {shown:?}");
    let files: Vec<usize> = shown.iter().copied().filter(|&row| row != 3).collect();
    assert_eq!(asked(&first), files, "{view:?}: exactly the drawn files are asked");
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(counters.asked, files.len(), "{view:?}: no offscreen row is asked");
    settle(cx, handle, 200);
    assert!(asked(&drained(&mut lines)).is_empty(), "{view:?}: a still frame asks nothing again");
    handle
        .update(cx, |shell, _, _| shell.panes[0].as_mut().unwrap().pane.scroll_y = 3_100.)
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let moved = drained(&mut lines);
    let now: Vec<usize> = read(cx, handle, |shell| drawn(shell, view)).into_iter().map(|(row, _)| row).collect();
    assert!(now[0] >= 90, "{view:?}: the scroll moved the viewport: {now:?}");
    assert_eq!(asked(&moved), now, "{view:?}: only the rows now in view are asked");
    let cancel = moved
        .iter()
        .find(|line| line.starts_with(r#"{"c":"thumbcancel","#))
        .expect("the rows left behind are cancelled");
    assert!(cancel.contains(&format!("[{}", files[0])), "{cancel}");
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(counters.asked, files.len() + now.len(), "{view:?}: the counter gate holds after a scroll");
    assert_eq!(counters.cancelled, files.len(), "{view:?}");
}

#[gpui::test]
fn offscreen_list_rows_make_no_request(cx: &mut TestAppContext) {
    offscreen_rows_make_no_request(cx, View::List);
}

#[gpui::test]
fn offscreen_columns_rows_make_no_request(cx: &mut TestAppContext) {
    offscreen_rows_make_no_request(cx, View::Columns);
}

#[gpui::test]
fn show_thumbnails_off_and_images_follow_the_mode_in_list_and_columns(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns] {
        let (handle, mut lines) = open(cx, view, r#"{"preview":{"thumbnails":"off"}}"#);
        assert!(asked(&first_settle(cx, handle, &mut lines)).is_empty(), "{view:?}: Off asks nothing");
        let (handle, mut lines) = open(cx, view, r#"{"preview":{"thumbnails":"images"}}"#);
        assert_eq!(asked(&first_settle(cx, handle, &mut lines)), vec![1], "{view:?}: Images asks the image only");
    }
}

#[gpui::test]
fn a_view_switch_keeps_the_one_lender_and_asks_nothing_again(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    first_settle(cx, handle, &mut lines);
    deliver(cx, handle, &thumbed(1, PICTURE));
    cx.run_until_parked();
    step(cx, handle);
    assert!(read(cx, handle, |shell| picture_at(shell, View::List, 1)).is_some());
    handle
        .update(cx, |shell, window, cx| shell.set_view(View::Grid, window, cx))
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let again = asked(&drained(&mut lines));
    assert!(!again.contains(&1), "the Grid borrows the List's decoded picture: {again:?}");
    let lent = read(cx, handle, |shell| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let shared = grid.decor.pictures().expect("the Grid lends the pane's pictures") as *const Pictures;
        (shared == Rc::as_ptr(&pictures(shell)), grid.state.borrow().drawn_row(1).and_then(|tile| tile.decoded))
    });
    assert_eq!(lent, (true, Some((1280, 64))), "one lender per pane, drawn by the Grid tile");
}

#[gpui::test]
fn a_filter_that_hides_every_row_cancels_the_asks_still_waiting(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    let first = asked(&first_settle(cx, handle, &mut lines));
    assert!(!first.is_empty());
    handle
        .update(cx, |shell, _, _| shell.panes[0].as_mut().unwrap().filter.shown = Some(Rc::from(Vec::<usize>::new())))
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let after = drained(&mut lines);
    let cancel = after
        .iter()
        .find(|line| line.starts_with(r#"{"c":"thumbcancel","#))
        .expect("an empty filter cancels every waiting ask");
    assert!(cancel.contains(&format!("[{}", first[0])), "{cancel}");
    assert!(asked(&after).is_empty(), "nothing hidden is asked");
    assert_eq!(read(cx, handle, |shell| pictures(shell).waiting()), Vec::<usize>::new());
}

#[test]
fn a_cut_row_draws_its_picture_faded_once_and_reuses_the_copy() {
    use crate::grid::element::{Identity, PictureKind, Thumbnail};
    let texels = vec![10, 20, 30, 255, 40, 50, 60, 100];
    let buffer = image::RgbaImage::from_raw(2, 1, texels).unwrap();
    let picture = std::sync::Arc::new(Thumbnail {
        identity: Identity {
            name: "photo.png".into(),
            size: 101,
            mtime: 1_700_000_000,
        },
        kind: PictureKind::Image,
        image: std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(buffer)])),
    });
    let pictures = Pictures::default();
    let faded = pictures.faded(std::sync::Arc::clone(&picture), 0.5);
    assert_eq!(faded.image.as_bytes(0).unwrap(), &[10, 20, 30, 128, 40, 50, 60, 50], "only the straight alpha is scaled");
    assert_eq!(picture.image.as_bytes(0).unwrap()[3], 255, "the lent picture is untouched");
    let again = pictures.faded(std::sync::Arc::clone(&picture), 0.5);
    assert!(std::sync::Arc::ptr_eq(&faded, &again), "a still frame reuses the faded copy");
    assert_eq!(faded.identity, picture.identity, "the faded copy keeps the source facts");
}

#[path = "row_thumbnail_readers_tests.rs"]
mod readers_tests;
