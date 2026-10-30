//! The status line's selection summary after a launch select (EY-39) and over a marked folder (EY-40).
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::Shell,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::protocol::{Reply, RowFacts};
use gpui::{TestAppContext, WindowHandle};
use std::sync::Arc;

fn open(cx: &mut TestAppContext, select: Option<&str>, json: &str) -> WindowHandle<Shell> {
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
    let (_send, events) = async_channel::bounded(4);
    let select = select.map(str::to_owned);
    cx.add_window(|window, cx| {
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        let settings = Settings::from_json(flea::jsondoc::parse(json).unwrap());
        let launch = Launch::resolve(
            settings,
            Arguments {
                path: Some("/work".into()),
                select,
            },
            "/home/test",
        );
        Shell::new(launch, locale, events, window, cx)
    })
}

fn row(name: &str, dir: bool, size: u64) -> RowFacts {
    RowFacts {
        name: name.into(),
        dir,
        size,
        ..RowFacts::default()
    }
}

fn rows() -> Vec<RowFacts> {
    vec![
        row("A", true, 4096),
        row("Code", true, 4096),
        row("notes.txt", false, 99),
        row("todo.md", false, 48),
    ]
}

fn list(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Listed {
                    path: "/work".into(),
                    total: 4,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Rows {
                    start: 0,
                    rows: rows(),
                    listing: 1,
                }),
                window,
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
}

fn counts(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> String {
    handle
        .update(cx, |shell, _, cx| {
            shell.sync_footer(1100., cx);
            let footer = shell.surfaces[0].footer.clone().unwrap();
            footer.read(cx).counts.to_string()
        })
        .unwrap()
}

#[gpui::test]
fn a_launch_select_lands_as_a_lone_selection_with_its_size(cx: &mut TestAppContext) {
    let handle = open(cx, Some("/work/notes.txt"), "{}");
    cx.run_until_parked();
    list(cx, handle);
    let (cursor, selected) = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            (host.pane.cursor, host.selected.clone())
        })
        .unwrap();
    assert_eq!(cursor, 2);
    assert_eq!(
        selected,
        [2],
        "the asked row is selected, not only under the cursor"
    );
    assert_eq!(counts(cx, handle), "1 of 4 selected · 99 B");
}

#[gpui::test]
fn a_launch_select_under_the_sweep_settings_stays_selected(cx: &mut TestAppContext) {
    let handle = open(
        cx,
        Some("/work/notes.txt"),
        r#"{"view":"list","columns":["name","size","date","kind"],"columnWidths":{"size":480,"kind":295,"date":480,"mode":480},"keyHints":true,"wrapAtEnds":true}"#,
    );
    cx.run_until_parked();
    list(cx, handle);
    let (cursor, selected) = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            (host.pane.cursor, host.selected.clone())
        })
        .unwrap();
    assert_eq!((cursor, selected), (2, vec![2]));
    assert_eq!(counts(cx, handle), "1 of 4 selected · 99 B");
}

fn marked_folder(cx: &mut TestAppContext, walk: Option<crate::navigation::sort::Size>) -> String {
    marked_folder_walked_at(cx, walk, 0)
}

// `behind` names how many listing generations the walk's held listing trails the pane's.
fn marked_folder_walked_at(
    cx: &mut TestAppContext,
    walk: Option<crate::navigation::sort::Size>,
    behind: usize,
) -> String {
    let handle = open(cx, None, "{}");
    cx.run_until_parked();
    list(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.selected = vec![0];
            if let Some(size) = walk {
                let held = std::rc::Rc::new(crate::list::pane::Held {
                    listing: host.pane.held.listing - behind,
                    total: host.pane.held.total,
                    start: host.pane.held.start,
                    rows: host.pane.held.rows.clone(),
                });
                host.sizes = crate::list::sizes::State::walked_fixture(held, 0, size);
            }
        })
        .unwrap();
    counts(cx, handle)
}

#[gpui::test]
fn one_marked_folder_states_its_finished_walk(cx: &mut TestAppContext) {
    let walk = crate::navigation::sort::Size {
        bytes: 243,
        partial: false,
    };
    assert_eq!(marked_folder(cx, Some(walk)), "1 of 4 selected · 243 B");
}

#[gpui::test]
fn a_folder_without_a_finished_walk_states_no_size(cx: &mut TestAppContext) {
    assert_eq!(marked_folder(cx, None), "1 of 4 selected");
    let partial = crate::navigation::sort::Size {
        bytes: 20,
        partial: true,
    };
    assert_eq!(marked_folder(cx, Some(partial)), "1 of 4 selected");
}

#[gpui::test]
fn a_walk_from_an_older_listing_states_no_size(cx: &mut TestAppContext) {
    let walk = crate::navigation::sort::Size {
        bytes: 243,
        partial: false,
    };
    assert_eq!(
        marked_folder_walked_at(cx, Some(walk), 1),
        "1 of 4 selected"
    );
}
