//! The production Shell the dispatch tests drive: the native constructor with only the backend process replaced.
use crate::{
    backend::{Backend, Event as BackendEvent},
    navigation::folder_state::Writer,
    state::Shell,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::{
    keys,
    protocol::{Reply, RowFacts},
};
use gpui::{
    px, size, AppContext, Entity, KeyDownEvent, Keystroke, Modifiers, TestAppContext,
    VisualTestContext,
};
use std::{sync::Arc, time::Duration};

pub(super) fn open<'a>(
    cx: &'a mut TestAppContext,
    keys: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    build(
        cx,
        &format!(r#""keys":"{keys}","view":"list""#),
        None,
        false,
        "",
    )
}

// The same launch with its own settings fields and a --select path.
pub(super) fn open_with<'a>(
    cx: &'a mut TestAppContext,
    settings: &str,
    select: Option<&str>,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    build(cx, settings, select, false, "")
}

// The native window's own root, which mounts the menu layer and the deferred cards and takes the right click before the panes.
pub(crate) fn open_root<'a>(
    cx: &'a mut TestAppContext,
    keys: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    build(
        cx,
        &format!(r#""keys":"{keys}","view":"list""#),
        None,
        true,
        "",
    )
}

// The native root with its own settings fields and a --select path.
pub(crate) fn open_root_with<'a>(
    cx: &'a mut TestAppContext,
    settings: &str,
    select: Option<&str>,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    build(cx, settings, select, true, "")
}

// The same launch in a named view.
pub(super) fn open_in<'a>(
    cx: &'a mut TestAppContext,
    keys: &str,
    view: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    open_places(cx, keys, view, "")
}

// The same launch with more places members, written as `,"name":value`.
pub(super) fn open_places<'a>(
    cx: &'a mut TestAppContext,
    keys: &str,
    view: &str,
    places: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    build(
        cx,
        &format!(r#""keys":"{keys}","view":"{view}""#),
        None,
        false,
        places,
    )
}

fn build<'a>(
    cx: &'a mut TestAppContext,
    settings: &str,
    select: Option<&str>,
    root: bool,
    places: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    build_at(cx, settings, select, root, places, "/files")
}

pub(super) fn open_root_at<'a>(
    cx: &'a mut TestAppContext,
    path: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    build_at(
        cx,
        r#""keys":"default","view":"list""#,
        None,
        true,
        "",
        path,
    )
}

fn build_at<'a>(
    cx: &'a mut TestAppContext,
    settings: &str,
    select: Option<&str>,
    root: bool,
    places: &str,
    path: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let document = flea::jsondoc::parse(&format!(
        r#"{{{settings},"places":{{"rail":"shown","showHome":true,"showTrash":true{places}}}}}"#
    ))
    .unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let launch = Launch::resolve(
        Settings::from_json(document),
        Arguments {
            path: Some(path.into()),
            select: select.map(str::to_owned),
        },
        &home,
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    SENT.with(|sent| sent.borrow_mut().clear());
    // Only the backend process is replaced; the rail, Trash, watch and devices start as the native launch starts them.
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, _| {
                let (backend, lines) = Backend::test_lines();
                SENT.with(|sent| sent.borrow_mut().push(Box::new(lines)));
                Ok(backend)
            },
            monitor: None,
        })
    });
    let (shell, cx) = if root {
        let (root, cx) = cx.add_window_view(|window, cx| crate::window::root::Root {
            shell: cx.new(|cx| Shell::new(launch, locale, events, window, cx)),
        });
        let shell = root.read_with(cx, |root, _| root.shell.clone());
        cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
        (shell, cx)
    } else {
        cx.add_window_view(|window, cx| Shell::new(launch, locale, events, window, cx))
    };
    // The folder memory writer only counts here, so no step reaches the real state file.
    shell.update(cx, |shell, _| {
        let writer = &shell.panes[0].as_ref().unwrap().nav.writer;
        assert_eq!(
            writer.borrow().sent,
            0,
            "nothing was written before the swap"
        );
        *writer.borrow_mut() = Writer::default();
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    (shell, cx)
}

// One backend's request lines, read in order.
type Lines = Box<dyn FnMut() -> Option<String>>;

thread_local! {
    // Every request line the test window's backends were handed, read in order by sent.
    static SENT: std::cell::RefCell<Vec<Lines>> = Default::default();
}

/// The request lines sent since the last call, from every backend the window started.
pub(super) fn sent() -> Vec<String> {
    SENT.with(|sent| {
        let mut out = Vec::new();
        for lines in sent.borrow_mut().iter_mut() {
            while let Some(line) = lines() {
                out.push(line);
            }
        }
        out
    })
}

pub(super) fn row(name: &str, dir: bool) -> RowFacts {
    RowFacts {
        source: Some(format!("1:2:{}", if dir { 0o040000 } else { 0o100000 })),
        name: name.into(),
        dir,
        ..RowFacts::default()
    }
}

// The backend's answer to the listing out, then the frames and time a swap needs to commit it.
pub(crate) fn land(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    path: &str,
    rows: Vec<RowFacts>,
    listing: usize,
) {
    let total = rows.len();
    land_on(shell, cx, 0, path, rows, total, listing);
}

// One side's answer, whose total may name rows the window does not hold yet.
pub(super) fn land_on(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    path: &str,
    rows: Vec<RowFacts>,
    total: usize,
    listing: usize,
) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for reply in [
                Reply::Listed {
                    path: path.into(),
                    total,
                },
                Reply::Rows {
                    start: 0,
                    rows,
                    listing,
                },
            ] {
                shell.receive(side, BackendEvent::Reply(reply), window, cx);
            }
        })
    });
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(400));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
    let drawn = shell.read_with(cx, |shell, _| {
        shell.panes[side].as_ref().unwrap().path.clone()
    });
    assert_eq!(drawn, path, "the landed listing is drawn");
}

pub(super) fn asked(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    asked_on(shell, cx, 0)
}

pub(super) fn asked_on(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) -> String {
    shell.read_with(cx, |shell, _| {
        shell.panes[side].as_ref().unwrap().pane.place().to_owned()
    })
}

pub(super) fn lists(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.counters.lists
    })
}

pub(super) fn status(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().status.to_string()
    })
}

pub(super) fn set_cursor(shell: &Entity<Shell>, cx: &mut VisualTestContext, row: usize) {
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = row
        })
    });
}

pub(super) fn keypad_enter(cx: &mut VisualTestContext) {
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke {
            key: "enter".into(),
            key_char: None,
            modifiers: Modifiers::none(),
            is_keypad: true,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    });
    cx.run_until_parked();
}

// A German (de) press as fork 0c60e44 reports it: `us` is the US key at its position, and AltGr makes it text input.
pub(crate) fn german_press(key: &str, typed: &str, us: Option<&str>, altgr: bool) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: Some(typed.into()),
            modifiers: Modifiers::none(),
            is_keypad: false,
            us_key: us.map(Into::into),
        },
        is_held: false,
        prefer_character_input: altgr,
    }
}

// The keystroke the platform delivers for a table row, Shift on a cased letter as Linux sends it.
pub(super) fn keystroke(binding: &keys::Binding) -> KeyDownEvent {
    let text = binding.mods == "text";
    let key = match binding.key {
        _ if text => binding.key.to_lowercase(),
        "Return" | "Enter" => "enter".into(),
        "Backtab" => "tab".into(),
        "BracketLeft" => "[".into(),
        "BracketRight" => "]".into(),
        "Comma" => ",".into(),
        "Period" => ".".into(),
        "Equal" => "=".into(),
        "Plus" => "+".into(),
        "Minus" => "-".into(),
        "Underscore" => "_".into(),
        "Greater" => ">".into(),
        key => key.to_lowercase(),
    };
    let mask = binding.mask;
    // A main-block key on a US layout reports itself as its US position, as the fork does.
    let us_key =
        (key.len() == 1 && key.bytes().all(|byte| byte.is_ascii_graphic())).then(|| key.clone());
    KeyDownEvent {
        keystroke: Keystroke {
            key,
            key_char: text.then(|| binding.key.to_owned()),
            modifiers: Modifiers {
                control: mask & keys::CONTROL != 0,
                alt: mask & keys::ALT != 0,
                shift: mask & keys::SHIFT != 0
                    || (text && binding.key.chars().any(char::is_uppercase)),
                platform: mask & keys::SUPER != 0,
                function: false,
            },
            is_keypad: binding.key == "Enter",
            us_key,
        },
        is_held: false,
        prefer_character_input: false,
    }
}
