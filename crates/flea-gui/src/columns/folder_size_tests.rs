//! Columns folder sizes: the active track walks its painted folders and prints the List's size text, and a closed column paints no late answer.
use crate::list::sizes::Reply::Sized;
use crate::{
    backend::{Backend, Event},
    columns::geometry::Slot,
    state::{tests::shell_with, Shell},
    window::startup::View,
};
use flea::{
    jsondoc::{self, Json},
    protocol::{Reply, RowFacts},
};
use gpui::{px, size, TestAppContext, TextRun, WindowHandle};
use std::{
    process::Command,
    time::{Duration, Instant},
};

// The window/columns_test_host.rs helpers this module needs; a second #[path] load of that file is clippy's duplicate_mod.
#[path = "folder_size_test_host.rs"]
mod host;

const PATH: &str = "/fx/work";
// The fx/work folders and the walk each answers: the seven sizes v0.3.12 prints, then one partial walk.
const FOLDERS: [(&str, u64, bool, &str); 8] = [
    ("A", 243, false, "243 B"),
    ("Code", 170, false, "170 B"),
    ("Docs", 22_000, false, "22.0 kB"),
    ("Empty", 0, false, "0 B"),
    ("Locked", 32, false, "32 B"),
    ("Many", 0, false, "0 B"),
    ("Media", 5_100_000, false, "5.1 MB"),
    ("Part", 4_096, true, ">4.1 kB"),
];
const NO_SIZE_YET: &str = "\u{b7}";

fn rows(prefix: &str) -> Vec<RowFacts> {
    let mut rows: Vec<RowFacts> = FOLDERS
        .iter()
        .map(|(name, ..)| host::row(&format!("{prefix}{name}"), true))
        .collect();
    rows.push(RowFacts {
        size: 1_000,
        ..host::row("notes.txt", false)
    });
    rows
}

// A backend that answers fsinfo and dirsize from the table and receipts every walk command it reads.
fn backend(cx: &mut TestAppContext) -> Backend {
    let script = r#"import json,sys
table=json.loads(sys.argv[2])
for line in sys.stdin:
 request=json.loads(line)
 c=request.get('c')
 if c=='quit': break
 if c in ('fsinfo','dirsize','dirsizecancel'):
  print(json.dumps(dict(t='paths',paths=[line])),flush=True)
 if c=='fsinfo':
  print(json.dumps(dict(t='fsinfo',path=sys.argv[1],**{'class':''})),flush=True)
 if c=='dirsize':
  for row in request['rows']:
   if row<len(table):
    print(json.dumps(dict(t='dirsized',row=row,bytes=table[row][0],partial=table[row][1])),flush=True)
"#;
    let table: Vec<String> = FOLDERS
        .iter()
        .map(|(_, bytes, partial, _)| format!("[{bytes},{partial}]"))
        .collect();
    let mut command = Command::new("python3");
    command.args(["-u", "-c", script, PATH, &format!("[{}]", table.join(","))]);
    let backend = Backend::start_with(command, r#"{"c":"noop"}"#.into()).unwrap();
    let _ = backend.test_own(cx);
    backend
}

fn mount(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (channel, _requests) = Backend::test_channel();
    let backend = backend(cx);
    let handle = cx.open_window(size(px(1400.), px(700.)), |window, cx| {
        let mut shell = shell_with(channel, cx);
        shell.launch.settings.columns = 3;
        let host = shell.panes[0].as_mut().unwrap();
        host.pane = crate::pane::Pane::new(PATH.into(), 646.);
        let rows = rows("");
        host.pane
            .receive(Reply::Listed {
                path: PATH.into(),
                total: rows.len(),
            })
            .unwrap();
        host.pane
            .receive(Reply::Rows {
                start: 0,
                listing: 3,
                rows,
            })
            .unwrap();
        host.changed(&shell.locale);
        host.nav
            .after_reply(&mut host.pane, PATH, &mut host.selected);
        host.backend = Some(backend);
        shell.path_labels[0] = PATH.to_owned().into();
        shell.set_view(View::Columns, window, cx);
        shell
    });
    host::change(cx, handle, host::settle);
    handle
}

fn advance(cx: &mut TestAppContext, ms: u64) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

/// Reads the backend for up to `wait`: walk commands by name and walk answers held back undelivered; fsinfo and every other event is delivered at once.
fn collect(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    wait: Duration,
    enough: impl Fn(&[String], &[Event]) -> bool,
) -> (Vec<String>, Vec<Event>) {
    let replies = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            host.backend.as_ref().unwrap().replies.clone()
        })
        .unwrap();
    let (mut commands, mut sized) = (Vec::new(), Vec::new());
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline && !enough(&commands, &sized) {
        match replies.try_recv() {
            Ok(Event::Paths(lines)) => {
                let command = jsondoc::parse(&lines[0]).unwrap();
                let name = command.get("c").and_then(Json::as_str).unwrap_or_default();
                if name != "fsinfo" {
                    commands.push(name.to_owned());
                }
            }
            Ok(event @ Event::ListDetails(Sized { .. })) => sized.push(event),
            Ok(event) => deliver(cx, handle, vec![event]),
            Err(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    (commands, sized)
}

fn deliver(cx: &mut TestAppContext, handle: WindowHandle<Shell>, events: Vec<Event>) {
    handle
        .update(cx, |shell, window, cx| {
            for event in events {
                shell.receive(0, event, window, cx);
            }
        })
        .unwrap();
}

/// Classifies the pane and lets each settle pass until the painted folders' walk answers, returned undelivered.
fn walk(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> Vec<Event> {
    let (mut commands, mut sized) = (Vec::new(), Vec::new());
    for _ in 0..10 {
        host::frame(cx, handle);
        advance(cx, 70);
        let (more, answers) = collect(cx, handle, Duration::from_millis(200), |_, events| {
            events.len() >= FOLDERS.len()
        });
        commands.extend(more);
        sized.extend(answers);
        if sized.len() >= FOLDERS.len() {
            break;
        }
    }
    assert_eq!(
        commands,
        ["dirsize"],
        "Columns never asked for its painted folders"
    );
    assert_eq!(
        sized.len(),
        FOLDERS.len(),
        "the walk asked beyond the folders"
    );
    sized
}

/// What one track painted in each row's size cell: None for no cell, else (bytes, width) of the shaped run.
fn painted(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    slot: Slot,
) -> Vec<Option<(usize, f32)>> {
    handle
        .update(cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let painter = columns.painters.get(slot).unwrap().borrow();
            let indices: Vec<usize> = painter.visible().collect();
            indices
                .into_iter()
                .map(|index| {
                    painter.prepared(index).unwrap().size.as_ref().map(|run| {
                        let line = run.line();
                        (line.len, f32::from(line.width))
                    })
                })
                .collect()
        })
        .unwrap()
}

/// The same measure for a text the size cell should hold, shaped in the active track's own face.
fn shaped(cx: &mut TestAppContext, handle: WindowHandle<Shell>, text: &str) -> (usize, f32) {
    handle
        .update(cx, |shell, window, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let typeface = columns
                .painters
                .get(Slot::Active)
                .unwrap()
                .borrow()
                .typeface
                .clone();
            let run = TextRun {
                len: text.len(),
                font: typeface.font.clone(),
                color: gpui::Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window
                .text_system()
                .layout_line(text, typeface.caption, &[run], None);
            (line.len, f32::from(line.width))
        })
        .unwrap()
}

fn misses(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> u64 {
    handle
        .update(cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let misses = columns
                .painters
                .get(Slot::Active)
                .unwrap()
                .borrow()
                .cache
                .misses;
            misses
        })
        .unwrap()
}

fn expect_dots(cx: &mut TestAppContext, handle: WindowHandle<Shell>, why: &str) {
    let dot = shaped(cx, handle, NO_SIZE_YET);
    let active = painted(cx, handle, Slot::Active);
    assert_eq!(active.len(), FOLDERS.len() + 1);
    for (at, cell) in active.iter().take(FOLDERS.len()).enumerate() {
        assert_eq!(*cell, Some(dot), "row {at}: {why}");
    }
}

#[test]
fn g1rows28_active_column_prints_walked_folder_sizes_and_parent_and_child_print_none() {
    let mut cx = host::context();
    let handle = mount(&mut cx);
    host::frame(&mut cx, handle);
    expect_dots(
        &mut cx,
        handle,
        "a folder printed a size before its walk answered",
    );
    let sized = walk(&mut cx, handle);
    let before = misses(&mut cx, handle);
    deliver(&mut cx, handle, sized);
    host::frame(&mut cx, handle);
    let active = painted(&mut cx, handle, Slot::Active);
    for (at, (name, _, _, text)) in FOLDERS.iter().enumerate() {
        let expected = shaped(&mut cx, handle, text);
        assert_eq!(active[at], Some(expected), "{name} must print {text}");
    }
    let file = shaped(&mut cx, handle, "1.0 kB");
    assert_eq!(
        active[FOLDERS.len()],
        Some(file),
        "a file prints its own bytes"
    );
    assert_eq!(
        misses(&mut cx, handle) - before,
        FOLDERS.len() as u64,
        "an answer reshaped more than its own row"
    );
    // v0.3.12 ColumnPane.showsSize: only the column with a pane draws a size cell.
    for slot in [Slot::Parent, Slot::Child] {
        let cells = painted(&mut cx, handle, slot);
        assert!(!cells.is_empty(), "{slot:?} painted no rows");
        assert!(
            cells.iter().all(Option::is_none),
            "{slot:?} drew a size cell"
        );
    }
    advance(&mut cx, 120);
    let (commands, _) = collect(&mut cx, handle, Duration::from_millis(60), |c, _| {
        !c.is_empty()
    });
    assert!(
        commands.is_empty(),
        "answered folders were walked again: {commands:?}"
    );
}

#[test]
fn g1rows28_a_column_closed_mid_walk_cancels_and_paints_no_late_answer() {
    let mut cx = host::context();
    let handle = mount(&mut cx);
    let stale = walk(&mut cx, handle);
    // The active column is replaced by another listing while its walk is still out.
    host::change(&mut cx, handle, |shell, window, cx| {
        let rows = rows("new-");
        shell.receive(
            0,
            Event::Reply(Reply::Listed {
                path: PATH.into(),
                total: rows.len(),
            }),
            window,
            cx,
        );
        shell.receive(
            0,
            Event::Reply(Reply::Rows {
                start: 0,
                listing: 4,
                rows,
            }),
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, handle);
    let (commands, _) = collect(&mut cx, handle, Duration::from_secs(1), |c, _| {
        !c.is_empty()
    });
    assert_eq!(
        commands,
        ["dirsizecancel"],
        "the closed column kept walking"
    );
    let revision = |cx: &mut TestAppContext| {
        handle
            .update(cx, |shell, _, _| {
                let list = shell.panes[0].as_ref().unwrap().list.borrow();
                list.view.mark_revision(4).1
            })
            .unwrap()
    };
    let walked = revision(&mut cx);
    deliver(&mut cx, handle, stale);
    host::frame(&mut cx, handle);
    assert_eq!(revision(&mut cx), walked, "a late answer reached the rows");
    expect_dots(&mut cx, handle, "a late answer painted into the new column");
    // The view leaving Columns closes the column the same way.
    let stale = walk(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::Grid, window, cx)
    });
    host::frame(&mut cx, handle);
    let (commands, _) = collect(&mut cx, handle, Duration::from_secs(1), |c, _| {
        !c.is_empty()
    });
    assert_eq!(
        commands,
        ["dirsizecancel"],
        "a hidden Columns view kept walking"
    );
    let walked = revision(&mut cx);
    deliver(&mut cx, handle, stale);
    host::frame(&mut cx, handle);
    assert_eq!(
        revision(&mut cx),
        walked,
        "a late answer reached a hidden column"
    );
}
