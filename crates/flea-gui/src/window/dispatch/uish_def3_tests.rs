//! Native battery regressions for listing traces, Quick Look moves and pending rename ownership.
use super::super::{preview_tests, rename_tests, test_shell};
use crate::{
    pane::{
        states::{Kind, State},
        swap::{Options, Swap, CAP_MS},
    },
    window::{startup::View, u07_tests::read},
};
use gpui::TestAppContext;

#[test]
fn fx_uish_def3_expired_hold_keeps_its_trace_after_the_rows_land() {
    let mut swap = Swap::default();
    let state = State {
        kind: Kind::Ready,
        ..Default::default()
    };
    assert!(swap.begin("/slow".into(), 40, &state, Options::default(), 100));
    assert!(swap.expire(100 + CAP_MS));
    swap.end(2600, "landed");
    assert_eq!(
        (swap.counters.last_end, swap.counters.last_ms),
        ("expired", CAP_MS)
    );
    assert!(!swap.fell_back && !swap.holding);
    assert!(swap.begin("/fast".into(), 40, &state, Options::default(), 3000));
    swap.end(3010, "landed");
    assert_eq!(
        (swap.counters.last_end, swap.counters.last_ms),
        ("landed", 10)
    );
}

#[gpui::test]
fn fx_uish_def3_quick_look_moves_one_file_in_grid(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = preview_tests::opened(cx);
    let rows = (0..30)
        .map(|i| test_shell::row(&format!("picture-{i:02}.jpg"), false))
        .collect();
    test_shell::land(&shell, cx, "/files", rows, 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Grid, window, cx)));
    preview_tests::draw(cx);
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .input
        .frame
        .unwrap()
        .columns
        > 1));
    test_shell::set_cursor(&shell, cx, 20);
    preview_tests::press(cx, "space");
    for (key, expected) in [("k", "19"), ("k", "18"), ("j", "19")] {
        preview_tests::press(cx, key);
        assert_eq!(read(&shell, cx, "cursor", &[]), expected);
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    }
    preview_tests::press(cx, "escape");
    let columns = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .input
            .frame
            .unwrap()
            .columns
    });
    preview_tests::press(cx, "k");
    assert_eq!(read(&shell, cx, "cursor", &[]), (19 - columns).to_string());
    preview_tests::close(&shell, cx);
}

#[gpui::test]
fn fx_uish_def3_return_and_escape_keep_an_unresolved_rename(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = rename_tests::ready(cx, "default");
    cx.simulate_keystrokes("f2");
    rename_tests::draw(cx);
    cx.simulate_input("draft");
    for key in ["enter", "enter", "escape"] {
        cx.simulate_keystrokes(key);
        rename_tests::draw(cx);
        assert_eq!(rename_tests::editing(&shell, cx).unwrap().1, "draft.txt");
        assert!(shell.read_with(cx, |shell, _| shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .pending
            .is_some()));
    }
    assert_eq!(rename_tests::sent(&mut lines).len(), 1);
    rename_tests::receive(
        &shell,
        cx,
        crate::backend::Event::Failed(crate::backend::Failure {
            key: "window.backend_eof",
            detail: String::new(),
        }),
    );
    assert!(rename_tests::editing(&shell, cx).is_none());
    rename_tests::close(&shell, cx);
}

fn code_window(ask: &flea::jsondoc::Json, wrapped_tail: bool) -> String {
    use flea::jsondoc::Json;
    let first = ask.get("first").and_then(Json::as_f64).unwrap() as usize;
    let count = ask.get("count").and_then(Json::as_f64).unwrap() as usize;
    let rows = (first..(first + count).min(8001))
        .map(|line| {
            let tail = if wrapped_tail && line == 8000 {
                "x".repeat(4000)
            } else {
                String::new()
            };
            format!("[0,\"Log row {:05}{tail}\"]", line - 1)
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"t":"code","slot":"look","token":{},"state":"plain","first":{first},"total":8000,"lines":[{rows}]}}"#,
        preview_tests::token(ask)
    )
}

#[gpui::test]
fn fx_uish_def3_code_end_requests_the_last_file_window(cx: &mut TestAppContext) {
    use flea::jsondoc::Json;
    let (shell, cx, mut requests) = preview_tests::opened(cx);
    test_shell::land(
        &shell,
        cx,
        "/files",
        vec![
            preview_tests::file("large.txt", "text-plain"),
            preview_tests::file("later.txt", "text-plain"),
        ],
        1,
    );
    for key in ["end", "ctrl-end"] {
        preview_tests::press(cx, "space");
        // Answer each range the real view asks, including its first measured viewport.
        for _ in 0..4 {
            let asks = preview_tests::sent(&mut requests, "code");
            if asks.is_empty() {
                break;
            }
            for ask in asks {
                preview_tests::answer(&shell, cx, code_window(&ask, false));
            }
            preview_tests::draw(cx);
        }
        assert!(read(&shell, cx, "previewText", &[]).contains("Log row 00000"));
        let Some(crate::g2::quick_look_overlay::Body::Doc(doc)) = preview_tests::body(&shell, cx)
        else {
            panic!("Code document")
        };
        let crate::g2::quick_look_overlay::doc::View::Code(code) = doc.view else {
            panic!("Code view")
        };
        code.update(cx, |code, cx| code.set_wrap(key == "ctrl-end", cx));
        preview_tests::press(cx, key);
        let asks = preview_tests::sent(&mut requests, "code");
        let ask = asks.last().expect("End asks for the final paged window");
        let first = ask.get("first").and_then(Json::as_f64).unwrap() as usize;
        let count = ask.get("count").and_then(Json::as_f64).unwrap() as usize;
        assert!(first > 7500 && count <= 400, "{key}: {ask:?}");
        preview_tests::answer(&shell, cx, code_window(ask, key == "ctrl-end"));
        preview_tests::draw(cx);
        assert!(read(&shell, cx, "previewText", &[]).contains("Log row 07999"));
        if key == "ctrl-end" {
            code.read_with(cx, |code, _| {
                assert_eq!(
                    code.top, 7999,
                    "the newly read wrapped final line fills the viewport"
                );
                assert!(code.offset > 0., "End exposes its final wrapped row");
            });
        }
        assert_eq!(
            read(&shell, cx, "cursor", &[]),
            "0",
            "the listing stays stationary"
        );
        preview_tests::press(cx, "escape");
    }
    preview_tests::close(&shell, cx);
}
