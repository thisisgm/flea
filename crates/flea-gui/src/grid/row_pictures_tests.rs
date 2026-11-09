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

const PICTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/images/transfer.png"
);
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
                let key =
                    crate::columns::peeks::peek_key(path, folders.hidden, folders.hidden_last);
                columns.area.peeks.rows(&key).is_none()
            })
            .cloned()
            .collect();
        (
            out,
            host.pane.first_count(),
            folders.hidden,
            folders.hidden_last,
        )
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
    // Dual's second pane starts its own backend: a test channel, never a flea --backend from PATH.
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    let settings = flea::jsondoc::parse(settings).unwrap();
    let handle = cx.open_window(size(px(1400.), px(700.)), |window, cx| {
        let mut shell = named_shell(backend, cx);
        shell.launch.settings.data = settings;
        shell.launch.settings.columns = 3;
        let host = shell.panes[0].as_mut().unwrap();
        for (at, row) in Rc::get_mut(&mut host.pane.held)
            .unwrap()
            .rows
            .iter_mut()
            .enumerate()
        {
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
        .update(cx, |shell, window, cx| {
            shell.grid_pictures_event(0, &event, window, cx)
        })
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
            rows.split(',')
                .filter_map(|row| row.parse::<usize>().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

// The rows the surface drew last frame, and the picture each drew in its mark slot.
fn drawn(shell: &Shell, view: View) -> Vec<(usize, Option<(u32, u32)>)> {
    let host = shell.panes[0].as_ref().unwrap();
    if view == View::Columns {
        let columns = host.columns.as_ref().expect("the Columns view is mounted");
        let painter = columns
            .painters
            .get(Slot::Active)
            .expect("the active track drew")
            .borrow();
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
        .map(|shown| {
            (
                shown.index,
                shown.picture.as_ref().map(|picture| picture.decoded()),
            )
        })
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

// Thumbnails the surface's last paint drew, not only the ones prepaint lent.
fn painted(shell: &Shell, view: View) -> usize {
    let host = shell.panes[0].as_ref().unwrap();
    if view == View::Columns {
        let columns = host.columns.as_ref().expect("the Columns view is mounted");
        return columns
            .painters
            .get(Slot::Active)
            .expect("the active track drew")
            .borrow()
            .painted_pictures
            .get();
    }
    host.list.borrow().painted_pictures.get()
}

#[path = "row_pictures_tests/asks.rs"]
mod asks;
#[path = "row_pictures_tests/capture.rs"]
mod capture;
#[path = "row_pictures_tests/draws.rs"]
mod draws;
#[path = "row_pictures_tests/fix_preview_2.rs"]
mod fix_preview_2;

#[path = "row_thumbnail_readers_tests.rs"]
mod readers_tests;

#[path = "row_policy_readers_tests.rs"]
mod policy_readers_tests;
