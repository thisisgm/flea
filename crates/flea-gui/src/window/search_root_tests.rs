//! Fresh search opens use the focused folder, independently of prior scopes and panes.
use super::tests::{control_key, edit, frame, key, open, reply, Handle};
use crate::{
    backend::{Backend, Event},
    window::{navigate::Step, pane::PaneHost, startup::View},
};
use flea::{
    json,
    protocol::{Reply, RowFacts},
    ui_logic_file_search::Scope,
};
use gpui::TestAppContext;
use std::time::Duration;

fn land_folder(cx: &mut TestAppContext, handle: Handle, side: usize, path: &str) {
    edit(cx, handle, |shell, window, cx| {
        let listing = shell.panes[side].as_ref().unwrap().pane.held.listing + 1;
        for answer in [
            Reply::Listed {
                path: path.into(),
                total: 1,
            },
            Reply::Rows {
                start: 0,
                rows: vec![RowFacts {
                    name: "x.txt".into(),
                    ..Default::default()
                }],
                listing,
            },
        ] {
            shell.receive(
                side,
                Event::Session {
                    generation: 0,
                    reply: answer,
                },
                window,
                cx,
            );
        }
    });
    frame(cx, handle);
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[side].as_ref().unwrap().path, path);
    });
}

fn navigate(cx: &mut TestAppContext, handle: Handle, side: usize, path: &str) {
    edit(cx, handle, |shell, window, cx| {
        assert_eq!(
            shell.step(side, Step::Open(path.into()), window, cx),
            Ok(true)
        );
    });
    land_folder(cx, handle, side, path);
}

fn read_request(read: &mut dyn FnMut() -> Option<String>) -> String {
    loop {
        let line = read().expect("search sends a backend request");
        if json::field_str(&line, "c").is_some_and(|command| command.starts_with("search")) {
            return line;
        }
    }
}

fn search(
    cx: &mut TestAppContext,
    handle: Handle,
    read: &mut dyn FnMut() -> Option<String>,
    path: &str,
    contents: bool,
) {
    while read().is_some() {}
    if contents {
        control_key(cx, handle, "g");
    } else {
        key(cx, handle, "f", Some("f"));
    }
    let opened = read_request(read);
    assert_eq!(json::field_str(&opened, "c").as_deref(), Some("searchopen"));
    assert_eq!(json::field_str(&opened, "path").as_deref(), Some(path));
    assert_eq!(json::field_str(&opened, "folder").as_deref(), Some(path));
    key(cx, handle, "x", Some("x"));
    let query = read_request(read);
    assert_eq!(
        json::field_str(&query, "c").as_deref(),
        Some(if contents {
            "searchcontents"
        } else {
            "searchquery"
        })
    );
    if contents {
        assert_eq!(json::field_str(&query, "path").as_deref(), Some(path));
    }
    let side = edit(cx, handle, |shell, _, _| shell.focus_side);
    let generation = json::field_usize(&query, "gen").unwrap();
    reply(
        cx,
        handle,
        side,
        &format!(
            r#"{{"t":"searched","n":0,"scanned":0,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#
        ),
    );
    key(cx, handle, "escape", None);
    land_folder(cx, handle, side, path);
    edit(cx, handle, |shell, _, _| {
        assert!(!shell.panes[side].as_ref().unwrap().search.owner.active());
    });
}

#[gpui::test]
fn searchroot_first_open_and_reporter_sequence_follow_navigation_in_every_view(
    cx: &mut TestAppContext,
) {
    let home = std::env::var("HOME").unwrap();
    for view in [View::List, View::Grid, View::Columns] {
        let (handle, mut read) = open(cx);
        edit(cx, handle, |shell, window, cx| {
            shell.set_view(view, window, cx)
        });
        let first = format!("{home}/Notes");
        navigate(cx, handle, 0, &first);
        for contents in [false, true, false] {
            search(cx, handle, &mut read, &first, contents);
        }
        let next = format!("{home}/Projects");
        navigate(cx, handle, 0, &next);
        search(cx, handle, &mut read, &next, false);
        search(cx, handle, &mut read, &next, true);
    }
}

#[gpui::test]
fn searchroot_dual_panes_keep_roots_and_explicit_home_scope_independent(cx: &mut TestAppContext) {
    let home = std::env::var("HOME").unwrap();
    let paths = [format!("{home}/Left"), format!("{home}/Right")];
    let (handle, left_read) = open(cx);
    let (right_backend, right_read) = Backend::test_lines();
    edit(cx, handle, |shell, _, cx| {
        let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
        right.pane.use_executor(cx.background_executor().clone());
        right.backend = Some(right_backend);
        shell.panes[1] = Some(right);
        shell.lifecycle.attach(1);
        shell.view = View::Dual;
    });
    let mut reads: [Box<dyn FnMut() -> Option<String>>; 2] =
        [Box::new(left_read), Box::new(right_read)];
    for (side, path) in paths.iter().enumerate() {
        navigate(cx, handle, side, path);
    }
    for side in [0, 1, 0, 1] {
        edit(cx, handle, |shell, _, _| shell.focus_side = side);
        for contents in [false, true, false] {
            search(cx, handle, &mut reads[side], &paths[side], contents);
        }
    }
    while reads[0]().is_some() {}
    edit(cx, handle, |shell, _, _| shell.focus_side = 0);
    key(cx, handle, "f", Some("f"));
    read_request(&mut reads[0]);
    key(cx, handle, "tab", None);
    let widened = read_request(&mut reads[0]);
    assert_eq!(
        json::field_str(&widened, "path").as_deref(),
        Some(home.as_str())
    );
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.scope,
            Scope::Home
        );
        shell.focus_side = 1;
    });
    search(cx, handle, &mut reads[1], &paths[1], false);
    edit(cx, handle, |shell, _, _| shell.focus_side = 0);
    key(cx, handle, "escape", None);
    search(cx, handle, &mut reads[0], &paths[0], false);
}
