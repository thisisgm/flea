//! Text-input ownership at the mounted root: platform composition reaches the focused field, holds the window's keys while it lasts, and ends when focus leaves.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::{root::Root, text_input::Owner},
};
use gpui::{
    AppContext, Context, PlatformInputHandler, PlatformWindow, TestAppContext, Window, WindowHandle,
};

#[path = "i50_ime_commit_tests.rs"]
mod commit_tests;

type Handle = WindowHandle<Root>;

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    let out = handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    out
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn mounted(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, read) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    frame(cx, handle);
    (handle, read)
}

#[derive(Clone, Copy, Debug)]
enum Entry {
    Filter,
    Find,
    Name,
}

impl Entry {
    fn owner(self) -> Owner {
        match self {
            Entry::Filter => Owner::Filter(0),
            Entry::Find => Owner::Find(0),
            Entry::Name => Owner::Name,
        }
    }
}

// Opens the entry the way its key does and draws the frame that mounts its platform handler.
fn open(cx: &mut TestAppContext, handle: Handle, entry: Entry) {
    edit(cx, handle, |shell, window, cx| {
        let opened = match entry {
            Entry::Filter => shell.filter_action("filter", window, cx),
            Entry::Find => shell.search_action("search", window, cx),
            Entry::Name => shell.rename_open(0, 0, window, cx),
        };
        assert!(opened, "{entry:?} must open");
    });
    frame(cx, handle);
}

// The handler the platform holds after the last frame, as the fork's text-input events reach it.
fn platform(cx: &mut TestAppContext, handle: Handle) -> PlatformInputHandler {
    cx.test_window(handle.into())
        .take_input_handler()
        .expect("the focused field must register platform input")
}

fn give_back(cx: &mut TestAppContext, handle: Handle, input: PlatformInputHandler) {
    cx.test_window(handle.into()).set_input_handler(input);
}

fn cursor(cx: &mut TestAppContext, handle: Handle) -> usize {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().map_or(0, |host| host.pane.cursor)
    })
}

fn owner(cx: &mut TestAppContext, handle: Handle) -> Option<Owner> {
    edit(cx, handle, |shell, _, _| shell.keyboard.text_owner())
}

type Line = (
    String,
    std::ops::Range<usize>,
    bool,
    Option<std::ops::Range<usize>>,
);

// The line as an edit would change it: text, caret and selection, and the marked range.
fn line(cx: &mut TestAppContext, handle: Handle, entry: Entry) -> Line {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let read = |e: &crate::path::entry::Entry| {
            (
                e.text.clone(),
                e.selected.clone(),
                e.reversed,
                e.marked.clone(),
            )
        };
        match entry {
            Entry::Find => read(&host.search.owner.entry.borrow()),
            Entry::Filter => read(&host.filter.owner.entry.borrow()),
            Entry::Name => read(&shell.rename.edit.as_ref().unwrap().entry),
        }
    })
}

fn compose(cx: &mut TestAppContext, handle: Handle, text: &str) {
    let mut input = platform(cx, handle);
    let end = text.encode_utf16().count();
    input.replace_and_mark_text_in_range(None, text, Some(end..end));
    give_back(cx, handle, input);
    frame(cx, handle);
}

fn commit(cx: &mut TestAppContext, handle: Handle, text: &str) {
    let mut input = platform(cx, handle);
    input.replace_text_in_range(None, text);
    give_back(cx, handle, input);
    frame(cx, handle);
}

const ENTRIES: [Entry; 3] = [Entry::Filter, Entry::Find, Entry::Name];

#[gpui::test]
fn i50_ime_root_entry_matrix(cx: &mut TestAppContext) {
    for entry in ENTRIES {
        let (handle, _read) = mounted(cx);
        open(cx, handle, entry);
        assert_eq!(owner(cx, handle), None, "{entry:?}: nothing marked yet");
        compose(cx, handle, "に");
        assert_eq!(
            owner(cx, handle),
            Some(entry.owner()),
            "{entry:?}: the marked field owns the root"
        );
        commit(cx, handle, "日本");
        assert_eq!(
            owner(cx, handle),
            None,
            "{entry:?}: a commit ends ownership"
        );
    }
}

#[gpui::test]
fn i50_ime_root_marked_suppresses_action(cx: &mut TestAppContext) {
    for entry in [Entry::Filter, Entry::Find] {
        let (handle, mut read) = mounted(cx);
        open(cx, handle, entry);
        // A committed letter sits before the mark, so a leaked edit or caret move shows in the line.
        commit(cx, handle, "a");
        while read().is_some() {}
        let (before, total) = edit(cx, handle, |shell, _, _| {
            let pane = &shell.panes[0].as_ref().unwrap().pane;
            assert!(!pane.busy(), "{entry:?}: the listing can move");
            (pane.cursor, pane.held.total)
        });
        assert!(
            before + 2 < total,
            "{entry:?}: rows remain below the cursor, so a leaked Down or PageDown would move it"
        );
        // Leave text on both sides of the caret, so either arrow and forward Delete can change it.
        let mut input = platform(cx, handle);
        input.replace_and_mark_text_in_range(None, "にほ", Some(1..1));
        give_back(cx, handle, input);
        frame(cx, handle);
        let shown = line(cx, handle, entry);
        assert_eq!(shown, ("aにほ".into(), 4..4, false, Some(1..7)));
        // Find's own Tab commits the candidate and changes scope, so only the filter holds it.
        let tab = matches!(entry, Entry::Filter).then_some("tab");
        for key in [
            "down",
            "pagedown",
            "delete",
            "ctrl-a",
            "f2",
            "a",
            "ctrl-l",
            "enter",
            "backspace",
            "left",
            "right",
            "shift-left",
            "shift-right",
        ]
        .into_iter()
        .chain(tab)
        {
            cx.simulate_keystrokes(handle.into(), key);
            frame(cx, handle);
            assert_eq!(
                cursor(cx, handle),
                before,
                "{entry:?} {key}: the list never moves under a composition"
            );
            assert_eq!(
                line(cx, handle, entry),
                shown,
                "{entry:?} {key}: text, caret and mark never change under a composition"
            );
            assert_eq!(
                read(),
                None,
                "{entry:?} {key}: no backend request while composing"
            );
            assert_eq!(
                owner(cx, handle),
                Some(entry.owner()),
                "{entry:?} {key}: the composition still owns the keys"
            );
        }
        assert!(
            edit(cx, handle, |shell, _, _| shell.rename.edit.is_none()),
            "{entry:?}: F2 under a composition opens no name editor"
        );
        // The line's own Escape cancels the candidate and closes nothing else.
        cx.simulate_keystrokes(handle.into(), "escape");
        frame(cx, handle);
        assert_eq!(
            owner(cx, handle),
            None,
            "{entry:?}: Escape ends the composition"
        );
        assert_eq!(cursor(cx, handle), before, "{entry:?}: Escape moves no row");
        assert_eq!(read(), None, "{entry:?}: Escape sends nothing");
        let typing = edit(cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            match entry {
                Entry::Find => host.search.owner.typing(),
                _ => host.filter.owner.typing(),
            }
        });
        assert!(
            typing,
            "{entry:?}: the line stays open after the first Escape"
        );
    }
}

#[gpui::test]
fn i50_ime_root_blur_detaches(cx: &mut TestAppContext) {
    for entry in ENTRIES {
        let (handle, mut read) = mounted(cx);
        open(cx, handle, entry);
        while read().is_some() {}
        compose(cx, handle, "日本");
        assert_eq!(owner(cx, handle), Some(entry.owner()));
        // Focus leaves the shell for another surface without a key, as a card taking focus does.
        let other = edit(cx, handle, |_, window, cx| {
            let other = cx.focus_handle();
            window.focus(&other, cx);
            other
        });
        frame(cx, handle);
        assert_eq!(
            owner(cx, handle),
            None,
            "{entry:?}: a blurred field owns nothing"
        );
        let marked = edit(cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let line =
                |entry: &crate::path::entry::Entry| (entry.marked.clone(), entry.text.clone());
            match entry {
                Entry::Find => line(&host.search.owner.entry.borrow()),
                Entry::Filter => line(&host.filter.owner.entry.borrow()),
                Entry::Name => line(&shell.rename.edit.as_ref().unwrap().entry),
            }
        });
        assert_eq!(
            marked,
            (None, "日本".into()),
            "{entry:?}: the mark is committed as shown"
        );
        // A query sends its committed text once; a name waits for its own Enter.
        let sends = usize::from(!matches!(entry, Entry::Name));
        let sent = std::iter::from_fn(&mut read).collect::<Vec<_>>();
        assert_eq!(
            sent.iter().filter(|line| line.contains("日本")).count(),
            sends,
            "{entry:?}: the committed text is sent {sends} times: {sent:?}"
        );
        // Focus returns: the platform reaches the field again with no stale mark.
        edit(cx, handle, |shell, window, cx| {
            window.focus(&shell.focus, cx)
        });
        frame(cx, handle);
        drop(other);
        assert_eq!(owner(cx, handle), None);
        let mut input = platform(cx, handle);
        assert_eq!(input.marked_text_range(), None, "{entry:?}");
        give_back(cx, handle, input);
    }
}

#[gpui::test]
fn i50_ime_root_multiwindow_owner(cx: &mut TestAppContext) {
    let (first, _read_first) = mounted(cx);
    let (second, mut read_second) = mounted(cx);
    open(cx, first, Entry::Filter);
    compose(cx, first, "に");
    frame(cx, second);
    assert_eq!(owner(cx, first), Some(Owner::Filter(0)));
    assert_eq!(
        owner(cx, second),
        None,
        "another window's composition owns nothing here"
    );
    while read_second().is_some() {}
    let before = cursor(cx, second);
    cx.simulate_keystrokes(second.into(), "down");
    frame(cx, second);
    assert_eq!(
        cursor(cx, second),
        before + 1,
        "the other window's list still moves"
    );
    assert_eq!(
        owner(cx, first),
        Some(Owner::Filter(0)),
        "and the first keeps its composition"
    );
    commit(cx, first, "日本");
    assert_eq!(owner(cx, first), None);
}

#[gpui::test]
fn i50_ime_root_listing_keys(cx: &mut TestAppContext) {
    let (handle, _read) = mounted(cx);
    let before = cursor(cx, handle);
    cx.simulate_keystrokes(handle.into(), "j");
    frame(cx, handle);
    assert_eq!(
        cursor(cx, handle),
        before + 1,
        "a single letter still moves the list"
    );
    cx.simulate_keystrokes(handle.into(), "k");
    frame(cx, handle);
    assert_eq!(cursor(cx, handle), before);
    // A filter composes and commits; the list's letters work again once it closes.
    open(cx, handle, Entry::Filter);
    compose(cx, handle, "に");
    commit(cx, handle, "に");
    cx.simulate_keystrokes(handle.into(), "escape");
    frame(cx, handle);
    cx.simulate_keystrokes(handle.into(), "escape");
    frame(cx, handle);
    assert_eq!(owner(cx, handle), None);
    let before = cursor(cx, handle);
    cx.simulate_keystrokes(handle.into(), "j");
    frame(cx, handle);
    assert_eq!(
        cursor(cx, handle),
        before + 1,
        "letters navigate after the composition"
    );
}

#[gpui::test]
fn i50_ime_root_blur_detaches_closed_dialog(cx: &mut TestAppContext) {
    let (handle, _read) = mounted(cx);
    // A dialog field composed, then its card closed with no key: the shared entry keeps the mark.
    edit(cx, handle, |shell, _, _| {
        let entry = &mut shell.rename.ime.entry;
        entry.begin("に");
        entry.marked = Some(0..entry.text.len());
        shell.keyboard.own_text(Some(Owner::Name));
    });
    frame(cx, handle);
    assert_eq!(owner(cx, handle), None, "a closed dialog owns nothing");
    let marked = edit(cx, handle, |shell, _, _| {
        shell.rename.ime.entry.marked.clone()
    });
    assert_eq!(marked, None, "the closed dialog's composition is let go");
}
