//! Word chords reach the mounted main path and inline rename before any listing action.
use super::*;
use crate::path::{
    entry::navigation_cases::{check, snapshot, COMPOSING},
    view::PathView,
};
use gpui::Entity;

fn mounted(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (handle, read) = super::mounted(cx);
    edit(cx, handle, |shell, _, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        let listing = host.pane.held.listing + 1;
        host.pane
            .receive(flea::protocol::Reply::Listed {
                path: "/files".into(),
                total: 1,
            })
            .unwrap();
        host.pane
            .receive(flea::protocol::Reply::Rows {
                start: 0,
                rows: vec![flea::protocol::RowFacts {
                    name: "folder".into(),
                    dir: true,
                    source: Some("1:2:16384".into()),
                    ..Default::default()
                }],
                listing,
            })
            .unwrap();
        assert!(
            host.pane.held.row(0).unwrap().dir,
            "the listing positive control needs a real folder row"
        );
        cx.notify();
    });
    frame(cx, handle);
    (handle, read)
}

fn unchanged(cx: &mut TestAppContext, handle: Handle) {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.path, "/files");
        assert_eq!(host.pane.path(), "/files");
        assert_eq!(host.pane.cursor, 0);
        assert!(!host.outline_used && host.outline_request.is_none());
        assert!(!host.pane.held.row(0).unwrap().open);
    });
}

#[gpui::test]
fn fx466_listing_control_right_positive_control(cx: &mut TestAppContext) {
    let (handle, mut read) = mounted(cx);
    while read().is_some() {}
    cx.simulate_keystrokes(handle.into(), "ctrl-right");
    frame(cx, handle);
    let sent = std::iter::from_fn(&mut read).collect::<Vec<_>>();
    assert!(
        sent.iter().any(|line| line.contains("\"c\":\"outline\"")),
        "listing must be able to open this folder: {sent:?}"
    );
}

fn path(cx: &mut TestAppContext, handle: Handle) -> Entity<PathView> {
    edit(cx, handle, |shell, _, cx| {
        shell.chrome.view.as_ref().unwrap().read(cx).path.clone()
    })
}

#[gpui::test]
fn fx466_main_path_word_navigation(cx: &mut TestAppContext) {
    let (handle, mut read) = mounted(cx);
    let view = path(cx, handle);
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| view.open(window, cx))
        })
        .unwrap();
    check(|text, keys| {
        view.update(cx, |view, cx| {
            view.entry.begin(text);
            view.entry.move_to(0);
            cx.notify();
        });
        frame(cx, handle);
        while read().is_some() {}
        cx.simulate_keystrokes(handle.into(), keys);
        frame(cx, handle);
        assert_eq!(
            read(),
            None,
            "movement cannot send listing or path requests"
        );
        unchanged(cx, handle);
        view.read_with(cx, |view, _| snapshot(&view.entry))
    });
    compose(cx, handle, "にほん");
    let before = view.read_with(cx, |view, _| snapshot(&view.entry));
    assert!(
        before.4.is_some(),
        "the composition must be live: {before:?}"
    );
    while read().is_some() {}
    for keys in COMPOSING {
        cx.simulate_keystrokes(handle.into(), keys);
        frame(cx, handle);
        assert_eq!(read(), None, "{keys}: composition keeps the listing still");
        unchanged(cx, handle);
        let after = view.read_with(cx, |view, _| snapshot(&view.entry));
        assert_eq!(after, before, "{keys}");
    }
}

#[gpui::test]
fn fx466_inline_rename_word_navigation(cx: &mut TestAppContext) {
    let (handle, mut read) = mounted(cx);
    open(cx, handle, Entry::Name);
    check(|text, keys| {
        edit(cx, handle, |shell, _, cx| {
            let entry = &mut shell.rename.edit.as_mut().unwrap().entry;
            entry.begin(text);
            entry.move_to(0);
            cx.notify();
        });
        frame(cx, handle);
        while read().is_some() {}
        cx.simulate_keystrokes(handle.into(), keys);
        frame(cx, handle);
        assert_eq!(read(), None, "movement cannot open or close a listing");
        unchanged(cx, handle);
        edit(cx, handle, |shell, _, _| {
            snapshot(&shell.rename.edit.as_ref().unwrap().entry)
        })
    });
    compose(cx, handle, "にほん");
    let before = line(cx, handle, Entry::Name);
    assert!(
        before.3.is_some(),
        "the composition must be live: {before:?}"
    );
    while read().is_some() {}
    for keys in COMPOSING {
        cx.simulate_keystrokes(handle.into(), keys);
        frame(cx, handle);
        assert_eq!(read(), None, "{keys}: composition keeps the listing still");
        unchanged(cx, handle);
        let after = line(cx, handle, Entry::Name);
        assert_eq!(after, before, "{keys}: IME owns its candidate navigation");
    }
}
