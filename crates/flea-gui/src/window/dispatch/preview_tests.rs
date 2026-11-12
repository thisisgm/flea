//! Space, l, the cursor keys and Escape open, follow and close Quick Look on the production Shell, read back through the state IPC.
use super::test_shell::{keystroke, land, open_root, row, set_cursor};
use crate::{
    backend::{Backend, Event as BackendEvent},
    g2::quick_look_overlay::Body,
    state::Shell,
    window::u07_tests::read,
};
use flea::{
    jsondoc::{self, Json},
    keys::{self, Context, Frontend, Match},
    protocol::RowFacts,
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

pub(super) type Lines = Box<dyn FnMut() -> Option<String>>;

#[path = "quick_look_inactive_tests.rs"]
mod inactive;

// The window under its root with the pane's backend replaced by one that records each request line.
pub(super) fn opened(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Lines) {
    let (shell, cx) = open_root(cx, "default");
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(
        cx.update(|window, _| window.is_window_active()),
        "Quick Look frame fixture must be active before timed work"
    );
    let (backend, requests) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    (shell, cx, Box::new(requests))
}

pub(super) fn file(name: &str, icon: &str) -> RowFacts {
    RowFacts {
        name: name.into(),
        icon: icon.into(),
        mode: 0o100644,
        size: 64,
        ..RowFacts::default()
    }
}

// Every request the pane sent since the last read.
pub(super) fn drain(requests: &mut Lines) -> Vec<Json> {
    std::iter::from_fn(&mut *requests)
        .filter_map(|line| jsondoc::parse(&line).ok())
        .collect()
}

// Every request the pane sent since the last read whose command is `command`; the others are drained with them.
pub(super) fn sent(requests: &mut Lines, command: &str) -> Vec<Json> {
    drain(requests)
        .into_iter()
        .filter(|doc| doc.get("c").and_then(Json::as_str) == Some(command))
        .collect()
}

/// The backend's thumbed reply to the last tagged thumbnail request naming `row`, echoing its identity.
pub(super) fn thumbed_reply(asked: &[Json], row: usize, file: &str) -> String {
    let names_row = |ask: &&Json| {
        ask.get("rows")
            .and_then(Json::as_array)
            .is_some_and(|rows| rows.iter().any(|r| r.as_f64() == Some(row as f64)))
    };
    let ask = asked
        .iter()
        .filter(|ask| ask.get("thumbToken").is_some())
        .rfind(names_row)
        .expect("a tagged thumbnail request for the row");
    let echo = |key| jsondoc::render_at(ask.get(key).expect("thumbnail identity"), 0);
    format!(
        r#"{{"t":"thumbed","row":{row},"file":"{file}","thumbToken":{},"thumbPath":{},"thumbListing":{}}}"#,
        echo("thumbToken"),
        echo("thumbPath"),
        echo("thumbListing")
    )
}

pub(super) fn token(doc: &Json) -> u64 {
    doc.get("token").and_then(Json::as_f64).expect("a token") as u64
}

pub(super) fn answer(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: String) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, BackendEvent::Producer(line), window, cx)
        })
    });
    draw(cx);
}

pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

pub(super) fn body(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<Body> {
    shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref()?;
        Some(host.view.read(cx).subject()?.body.clone())
    })
}

pub(super) fn context(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Context {
    shell.read_with(cx, |shell, _| shell.keyboard.context)
}

pub(super) fn close(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
}

pub(super) fn rows() -> Vec<RowFacts> {
    vec![
        row("alpha", true),
        file("notes.txt", "text-plain"),
        file("photo.png", "image-png"),
        file("disk.img", "application-octet-stream"),
    ]
}

pub(super) fn code_reply(token: u64, lines: &[&str], total: usize) -> String {
    let lines: Vec<String> = lines.iter().map(|l| format!(r#"[0,"{l}"]"#)).collect();
    format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"plain","first":1,"total":{total},"lines":[{}]}}"#,
        lines.join(",")
    )
}

pub(super) fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

#[gpui::test]
fn space_opens_quick_look_on_the_cursor_file_and_space_or_escape_closes_it(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    // PreviewKeys.open: Space on a folder is quiet.
    press(cx, "space");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    assert!(sent(&mut requests, "code").is_empty());
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "held back until the read lands"
    );
    let asked = sent(&mut requests, "code");
    assert_eq!(asked.len(), 1, "one code ask for the cursor file");
    assert_eq!(
        asked[0].get("path").and_then(Json::as_str),
        Some("/files/notes.txt")
    );
    assert_eq!(asked[0].get("slot").and_then(Json::as_str), Some("look"));
    answer(
        &shell,
        cx,
        code_reply(token(&asked[0]), &["first line", "second line"], 2),
    );
    assert_eq!(read(&shell, cx, "previewKind", &[]), "text");
    assert_eq!(read(&shell, cx, "previewState", &[]), "ready");
    // The card mounts the column's code view, unnumbered, so w wraps plain text too.
    let shown = shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref()?;
        match &host.view.read(cx).subject()?.body {
            Body::Doc(doc) => match &doc.view {
                crate::g2::quick_look_overlay::doc::View::Code(code) => {
                    let code = code.read(cx);
                    Some((code.numbered, code.lines.plain_lines()))
                }
                _ => None,
            },
            _ => None,
        }
    });
    assert_eq!(
        shown,
        Some((
            false,
            vec!["first line".to_owned(), "second line".to_owned()]
        ))
    );
    assert!(
        cx.debug_bounds("pv1-card").is_some(),
        "the card draws over the window"
    );
    assert_eq!(context(&shell, cx), Context::Preview);
    // A second Space closes every kind, and the keyboard returns to the list.
    press(cx, "space");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    assert!(
        cx.debug_bounds("pv1-card").is_none(),
        "a closed card mounts nothing"
    );
    assert_eq!(context(&shell, cx), Context::Listing);
    press(cx, "space");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    press(cx, "escape");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    assert_eq!(
        read(&shell, cx, "path", &[]),
        "/files",
        "Escape closed the card and left the folder alone"
    );
    close(&shell, cx);
}

#[gpui::test]
fn the_cursor_keys_move_under_the_card_and_it_follows_each_file(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let first = sent(&mut requests, "code");
    answer(&shell, cx, code_reply(token(&first[0]), &["text"], 1));
    press(cx, "down");
    assert_eq!(
        read(&shell, cx, "cursor", &[]),
        "2",
        "the cursor moved under the card"
    );
    let meta = sent(&mut requests, "meta");
    assert_eq!(meta.len(), 1, "the card follows onto the picture");
    assert_eq!(meta[0].get("row").and_then(Json::as_f64), Some(2.));
    let reply = format!(
        r#"{{"t":"meta","row":2,"w":800,"h":600,"orient":1,"ms":0,"rate":0,"entries":0,"unpacked":0,"afailed":false,"names":[],"lines":0,"partial":false,"lfailed":false,"frames":0,"photo":null,"target":"","targetdir":false,"raw":null,"owner":"gm","token":{}}}"#,
        token(&meta[0])
    );
    answer(&shell, cx, reply);
    assert_eq!(read(&shell, cx, "previewKind", &[]), "image");
    assert!(matches!(body(&shell, cx), Some(Body::Image { w, h, .. }) if w == 800. && h == 600.));
    // The unsupported row shows the calm can't-show card at once.
    press(cx, "down");
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "This file cannot be previewed."
    );
    // PreviewKeys.follow: a folder row leaves the card on the file it shows.
    set_cursor(&shell, cx, 1);
    press(cx, "up");
    assert_eq!(read(&shell, cx, "cursor", &[]), "0");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "This file cannot be previewed."
    );
    close(&shell, cx);
}

#[gpui::test]
fn page_forward_on_a_file_and_space_with_two_selected_open_the_cursor_file(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    let forward: Vec<_> = keys::binding_rows("default", Frontend::Gui)
        .filter(|binding| binding.action == "pageForward")
        .map(keystroke)
        .filter(|event| {
            keys::lookup(
                "default",
                Context::Listing,
                Frontend::Gui,
                crate::window::input::event_input(event, Modifiers::none()),
            ) == Match::Action("pageForward")
        })
        .collect();
    assert!(!forward.is_empty(), "default binds pageForward");
    for event in forward {
        set_cursor(&shell, cx, 3);
        cx.simulate_event(event.clone());
        draw(cx);
        assert_eq!(
            read(&shell, cx, "previewOpen", &[]),
            "true",
            "{event:?} on a file opens Quick Look"
        );
        assert_eq!(read(&shell, cx, "path", &[]), "/files");
        press(cx, "escape");
    }
    // Focus.js: Space opens the cursor file whatever else is selected.
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 2];
        host.pane.cursor = 2;
    });
    press(cx, "space");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    assert_eq!(
        read(&shell, cx, "selectionCount", &[]),
        "2",
        "the selection stays"
    );
    assert_eq!(
        sent(&mut requests, "meta").len(),
        1,
        "the cursor file, a picture, is asked for"
    );
    close(&shell, cx);
}

#[gpui::test]
fn every_key_that_opens_quick_look_opens_it_in_every_view(cx: &mut TestAppContext) {
    use crate::window::startup::View;
    use gpui::{KeyDownEvent, Keystroke};
    let (shell, cx, _requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    let key = |key: &str, text: &str, shift: bool| KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: Some(text.into()),
            modifiers: Modifiers {
                shift,
                ..Modifiers::none()
            },
            is_keypad: false,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    };
    let space = key("space", " ", false);
    // l and Right are pageForward; the Shift-held l a virtual keyboard delivers keeps its lowercase text, as Focus.js matches it.
    let (l, right, shift_l) = (
        key("l", "l", false),
        key("right", "", false),
        key("l", "l", true),
    );
    for view in [View::List, View::Columns, View::Grid, View::Dual] {
        cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(view, window, cx)));
        draw(cx);
        // Dual lands once its second pane has a listing.
        if view == View::Dual {
            super::test_shell::land_on(&shell, cx, 1, "/files", rows(), 4, 1);
        }
        assert_eq!(
            shell.read_with(cx, |shell, _| (shell.view, shell.focus_side)),
            (view, 0),
            "the view landed"
        );
        // Grid.sideways: the bare arrows and h, l step the grid's cells instead.
        let opening: Vec<&KeyDownEvent> = if view == View::Grid {
            vec![&space, &shift_l]
        } else {
            vec![&space, &l, &right, &shift_l]
        };
        for event in opening {
            set_cursor(&shell, cx, 1);
            cx.simulate_event(event.clone());
            draw(cx);
            assert_eq!(
                read(&shell, cx, "previewOpen", &[]),
                "true",
                "{view:?} {event:?} opens Quick Look"
            );
            assert_eq!(
                read(&shell, cx, "cursor", &[]),
                "1",
                "{view:?} {event:?} keeps the cursor"
            );
            press(cx, "escape");
            assert_eq!(
                read(&shell, cx, "previewOpen", &[]),
                "false",
                "{view:?} Escape closes it"
            );
        }
    }
    close(&shell, cx);
}
