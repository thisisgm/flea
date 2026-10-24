//! Return, the keypad Enter, a double click and BackSpace navigate the production Shell with no test subscriber.
use crate::{
    backend::{Backend, Event as BackendEvent},
    navigation::folder_state::Writer,
    state::Shell,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
        u07_tests::{press, read},
    },
};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    px, size, Entity, KeyDownEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext,
};
use std::{sync::Arc, time::Duration};

pub(super) fn open<'a>(
    cx: &'a mut TestAppContext,
    keys: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let document = flea::jsondoc::parse(&format!(
        r#"{{"keys":"{keys}","view":"list","places":{{"rail":"shown","showHome":true,"showTrash":true}}}}"#
    ))
    .unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let launch = Launch::resolve(
        Settings::from_json(document, true),
        Arguments {
            path: Some("/files".into()),
            select: None,
        },
        &home,
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(flea::i18n::I18n::default()),
    };
    let (_sender, events) = async_channel::unbounded();
    // Only the backend process is replaced; the rail, Trash, watch and devices start as the native launch starts them.
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
    let (shell, cx) =
        cx.add_window_view(|window, cx| Shell::new(launch, locale, events, window, cx));
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

pub(super) fn row(name: &str, dir: bool) -> RowFacts {
    RowFacts {
        name: name.into(),
        dir,
        ..RowFacts::default()
    }
}

// The backend's answer to the listing out, then the frames and time a swap needs to commit it.
pub(super) fn land(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    path: &str,
    rows: Vec<RowFacts>,
    listing: usize,
) {
    let total = rows.len();
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
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
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
    assert_eq!(drawn(shell, cx), path, "the landed listing is drawn");
}

fn drawn(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().path.clone())
}

fn asked(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    })
}

fn set_cursor(shell: &Entity<Shell>, cx: &mut VisualTestContext, row: usize) {
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = row
        })
    });
}

fn keypad_enter(cx: &mut VisualTestContext) {
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke {
            key: "enter".into(),
            key_char: None,
            modifiers: Modifiers::none(),
            is_keypad: true,
        },
        is_held: false,
        prefer_character_input: false,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn return_keypad_enter_double_click_and_backspace_navigate_the_production_shell(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.rail.is_some() && shell.trash.is_some(),
            "the native launch's producers are live"
        );
    });
    let home = || {
        vec![
            row("alpha", true),
            row("beta", true),
            row("notes.txt", false),
        ]
    };
    land(&shell, cx, "/files", home(), 1);
    // The native symptom: Down moves the cursor, then Return leaves the path where it was.
    cx.simulate_keystrokes("down");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        1
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(
        asked(&shell, cx),
        "/files/beta",
        "Return opens the folder under the cursor"
    );
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        asked(&shell, cx),
        "/files",
        "BackSpace climbs to the parent"
    );
    land(&shell, cx, "/files", home(), 3);
    set_cursor(&shell, cx, 1);
    keypad_enter(cx);
    assert_eq!(
        asked(&shell, cx),
        "/files/beta",
        "the keypad Enter opens the folder"
    );
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 4);
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, "/files", home(), 5);
    let centre = read(&shell, cx, "rowCentre", &["0"]);
    press(cx, &centre, 1);
    assert_eq!(asked(&shell, cx), "/files", "one click only selects");
    press(cx, &centre, 2);
    assert_eq!(
        asked(&shell, cx),
        "/files/alpha",
        "a double click opens the folder"
    );
    land(&shell, cx, "/files/alpha", Vec::new(), 6);
    // A file row is the opener's: Enter there navigates nowhere.
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, "/files", home(), 7);
    set_cursor(&shell, cx, 2);
    let lists = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.counters.lists
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(asked(&shell, cx), "/files");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .counters
            .lists),
        lists
    );
    // Focus.js: a reload answers from the rail as well as the list.
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.simulate_keystrokes("tab");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        flea::keys::Context::Rail
    );
    cx.simulate_keystrokes("f5");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .counters
            .lists),
        lists + 1,
        "F5 from the focused rail re-lists the pane"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
