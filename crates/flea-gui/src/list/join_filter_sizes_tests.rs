//! EY-589: a query cancels held folder walks after the empty strip has already settled.
use super::*;
use gpui::{KeyDownEvent, Keystroke};

fn press(cx: &mut TestAppContext, handle: WindowHandle<crate::state::Shell>, text: &str) {
    handle
        .update(cx, |shell, window, cx| {
            shell.key(
                &KeyDownEvent {
                    keystroke: Keystroke {
                        key: text.into(),
                        key_char: Some(text.into()),
                        ..Default::default()
                    },
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            )
        })
        .unwrap();
}

fn geometry(cx: &mut TestAppContext, handle: WindowHandle<crate::state::Shell>) -> [f32; 6] {
    handle
        .update(cx, |shell, window, _| {
            let pane = shell.geometry(window).panes[0];
            [
                pane.x,
                pane.y,
                pane.width,
                pane.height,
                shell.slots.strip_height,
                shell.panes[0].as_ref().unwrap().pane.scroll_y,
            ]
        })
        .unwrap()
}

#[test]
fn sol_filter_dirsize_remainder_ey589_real_query_cancels_held_walks_without_geometry_change() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    press(&mut cx, handle, "/");
    classify(&mut cx, handle);
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    assert_eq!(command.get("listing").and_then(Json::as_f64), Some(2.));
    let rows = command.get("rows").and_then(Json::as_array).unwrap();
    assert_eq!(rows.len(), 8, "walks stay inside the painted viewport");
    // The backend received these walks; their replies remain held outside the shell.
    let held: Vec<_> = (0..rows.len()).map(|_| next(&mut cx, handle)).collect();
    let pending = jsondoc::parse(&ask(&mut cx, handle, "dirSizeState", &[])).unwrap();
    for row in 0..8 {
        assert_eq!(
            pending
                .get("file")
                .and_then(|file| file.get(&row.to_string())),
            Some(&Json::Null)
        );
    }
    quiet(&mut cx, handle);
    let before = geometry(&mut cx, handle);
    assert!(before[4] > 0., "the empty strip was already mounted");
    let cancels = ask(&mut cx, handle, "dirSizeCancels", &[])
        .parse::<usize>()
        .unwrap();
    assert_eq!(ask(&mut cx, handle, "dirSizeRequests", &[]), "1");

    press(&mut cx, handle, "p");
    assert_eq!(geometry(&mut cx, handle), before);
    assert_eq!(
        request(&mut cx, handle),
        jsondoc::parse(r#"{"c":"dirsizecancel"}"#).unwrap(),
        "a new no-argument cancellation must follow the real query key"
    );
    assert_eq!(
        ask(&mut cx, handle, "dirSizeCancels", &[]),
        (cancels + 1).to_string()
    );
    let query = "proof-no-match-1101";
    for (at, character) in query.char_indices() {
        let end = at + character.len_utf8();
        if at > 0 {
            press(&mut cx, handle, &character.to_string());
        }
        let filter = request(&mut cx, handle);
        assert_eq!(filter.get("c").and_then(Json::as_str), Some("filter"));
        assert_eq!(
            filter.get("query").and_then(Json::as_str),
            Some(&query[..end])
        );
        if end == query.len() {
            let id = filter.get("id").and_then(Json::as_f64).unwrap() as usize;
            let answer = format!(
                r#"{{"t":"filtered","id":{id},"query":"{query}","n":2000,"rows":[],"ms":0.01,"listing":2}}"#
            );
            receive(&mut cx, handle, Backend::test_reply_line(&answer));
        }
    }
    for reply in held {
        receive(&mut cx, handle, reply);
    }
    frame(&mut cx, handle);
    settle(&mut cx);
    quiet(&mut cx, handle);
    assert_eq!(geometry(&mut cx, handle), before);
    assert_eq!(
        ask(&mut cx, handle, "dirSizeState", &[]),
        r#"{"file":{},"order":[]}"#,
        "cancelled replies cannot restore walked bytes"
    );
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.filter.shown.as_deref(), Some(&[][..]));
            assert!(!host.drawn_list, "the no-match hero replaces the List");
            assert!(shell.no_match_headline(0).is_some());
        })
        .unwrap();
    assert_eq!(
        ask(&mut cx, handle, "dirSizeCancels", &[]),
        (cancels + 1).to_string(),
        "startup and later empty frames cannot supply the query cancellation"
    );
}

#[test]
fn sol_filter_dirsize_remainder_k315_replans_only_the_filtered_viewport() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    press(&mut cx, handle, "/");
    classify(&mut cx, handle);
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    let rows = command.get("rows").and_then(Json::as_array).unwrap();
    assert_eq!(rows.len(), 8);
    let held: Vec<_> = (0..rows.len()).map(|_| next(&mut cx, handle)).collect();
    press(&mut cx, handle, "p");
    assert_eq!(
        request(&mut cx, handle),
        jsondoc::parse(r#"{"c":"dirsizecancel"}"#).unwrap()
    );
    let filter = request(&mut cx, handle);
    assert_eq!(filter.get("c").and_then(Json::as_str), Some("filter"));
    let id = filter.get("id").and_then(Json::as_f64).unwrap() as usize;
    let answer = format!(
        r#"{{"t":"filtered","id":{id},"query":"p","n":2000,"rows":[0,7],"ms":0.01,"listing":2}}"#
    );
    receive(&mut cx, handle, Backend::test_reply_line(&answer));
    for reply in held {
        receive(&mut cx, handle, reply);
    }
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.drawn_list);
            assert_eq!(
                host.list
                    .borrow()
                    .visible()
                    .iter()
                    .map(|row| row.index)
                    .collect::<Vec<_>>(),
                [0, 7]
            );
        })
        .unwrap();
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    assert_eq!(
        command.get("rows"),
        jsondoc::parse("[0,7]").ok().as_ref(),
        "replanning cannot walk the filtered-out folders between visible rows"
    );
    assert_eq!(ask(&mut cx, handle, "dirSizeRequests", &[]), "2");
}
