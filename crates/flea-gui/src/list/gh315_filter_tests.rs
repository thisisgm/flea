//! Issues #315 and PR #318 on the mounted List: a match past the held rows draws, and one Escape refills around it.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::protocol::{Reply, RowFacts};
use gpui::{AppContext, Context, KeyDownEvent, Keystroke, TestAppContext, Window, WindowHandle};

type Handle = WindowHandle<Root>;

const TOTAL: usize = 1200;

fn name(ix: usize) -> String {
    format!("item-{ix:04}.txt")
}

fn facts(range: std::ops::Range<usize>) -> Vec<RowFacts> {
    range
        .map(|ix| RowFacts {
            name: name(ix),
            ..RowFacts::default()
        })
        .collect()
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn press(cx: &mut TestAppContext, handle: Handle, key: &str, text: Option<&str>) {
    edit(cx, handle, |shell, window, cx| {
        let keystroke = Keystroke {
            key: key.into(),
            key_char: text.map(Into::into),
            ..Default::default()
        };
        shell.key(
            &KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            },
            window,
            cx,
        )
    });
}

fn line(cx: &mut TestAppContext, handle: Handle, text: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(text), window, cx)
    });
}

fn reply(cx: &mut TestAppContext, handle: Handle, reply: Reply) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, crate::backend::Event::Reply(reply), window, cx)
    });
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

// The rows List drew, as (view position, listing row), with the name each painted.
fn drawn(cx: &mut TestAppContext, handle: Handle) -> Vec<(usize, usize, Option<String>)> {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let list = host.list.borrow();
        list.visible()
            .iter()
            .map(|row| {
                (
                    row.view,
                    row.index,
                    list.drawn_rows
                        .row(row.index)
                        .map(|facts| facts.name.clone()),
                )
            })
            .collect()
    })
}

// Every drawn row is held, or inside the window the pane is asking for.
fn covered(cx: &mut TestAppContext, handle: Handle) {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let held = &host.pane.held;
        let owed = host.pane.requests.outstanding;
        for row in host.list.borrow().visible() {
            let has = row.index >= held.start && row.index < held.start + held.rows.len();
            let asked = owed.is_some_and(|w| row.index >= w.start && row.index < w.start + w.count);
            assert!(
                has || asked,
                "row {} drew with nothing held or asked: held {}..{}, owed {owed:?}",
                row.index,
                held.start,
                held.start + held.rows.len()
            );
        }
    });
}

// Answers the pane's outstanding window from the 1200-name listing, as the backend would.
fn serve(cx: &mut TestAppContext, handle: Handle) {
    let owed = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.requests.outstanding
    });
    if let Some(w) = owed {
        let end = (w.start + w.count).min(TOTAL);
        reply(
            cx,
            handle,
            Reply::Rows {
                start: w.start,
                rows: facts(w.start..end),
                listing: 2,
            },
        );
    }
    frame(cx, handle);
}

#[gpui::test]
fn gh315_318_a_match_past_the_held_rows_draws_and_one_escape_refills_around_the_cursor(
    cx: &mut TestAppContext,
) {
    let (backend, mut read) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            // A 1200-row listing whose first 320 rows are held, the rest only counted.
            shell.panes[0].as_mut().unwrap().pane.held =
                std::rc::Rc::new(crate::list::pane::Held {
                    listing: 2,
                    total: TOTAL,
                    start: 0,
                    rows: facts(0..320),
                });
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    frame(cx, handle);
    while read().is_some() {}
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!((host.pane.held.total, host.pane.held.listing), (TOTAL, 2));
        assert!(
            host.pane.held.start + host.pane.held.rows.len() <= 320,
            "the fixture holds only the first rows"
        );
    });
    assert_eq!(drawn(cx, handle).first().map(|row| row.1), Some(0));

    press(cx, handle, "/", Some("/"));
    for (key, query) in [("1", "1"), ("1", "11"), ("5", "115")] {
        press(cx, handle, key, Some(key));
        let asked = read().expect("each key asks the backend");
        assert!(asked.contains(&format!(r#""query":"{query}""#)), "{asked}");
    }
    // The backend's whole-listing answer: every match sits past the held rows.
    let id = 3;
    let matches: Vec<usize> = (1150..1160).collect();
    let rows = matches
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",");
    line(
        cx,
        handle,
        &format!(
            r#"{{"t":"filtered","id":{id},"query":"115","n":{TOTAL},"rows":[{rows}],"ms":0.010,"listing":2}}"#
        ),
    );
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.filter.shown.as_deref(),
            Some(&matches[..]),
            "the offscreen matches are the drawn set"
        );
        assert_eq!(
            host.pane.cursor, 1150,
            "a hidden cursor takes the first match"
        );
        assert_eq!(host.status.as_ref(), "10 of 1,200 shown");
    });
    frame(cx, handle);
    covered(cx, handle);
    serve(cx, handle);
    let narrowed = drawn(cx, handle);
    assert_eq!(
        narrowed
            .iter()
            .map(|row| (row.0, row.1))
            .collect::<Vec<_>>(),
        matches
            .iter()
            .enumerate()
            .map(|(view, row)| (view, *row))
            .collect::<Vec<_>>(),
        "List draws exactly the matches at their view positions"
    );
    for (_, row, painted) in &narrowed {
        assert_eq!(
            painted.as_deref(),
            Some(name(*row).as_str()),
            "row {row} drew another row's name"
        );
    }

    // Every request the pane sent before the clear, so only the clear's own lines are judged after it.
    while let Some(sent) = read() {
        eprintln!("GH315 before clear sent {sent}");
    }
    // One Escape clears the filter and refills the unfiltered viewport around the cursor; no second key is needed.
    press(cx, handle, "escape", None);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.filter.shown, None, "one Escape cleared the filter");
        assert!(shell.slots.strips[0].is_none(), "and unmounted the strip");
        assert_eq!(host.pane.cursor, 1150, "the cursor stays on its row");
    });
    frame(cx, handle);
    covered(cx, handle);
    serve(cx, handle);
    let refilled = drawn(cx, handle);
    assert!(
        refilled.len() > matches.len(),
        "the unfiltered viewport refilled: {} rows",
        refilled.len()
    );
    assert!(
        refilled.iter().any(|row| row.1 == 1150),
        "the cursor row is drawn after the clear"
    );
    for pair in refilled.windows(2) {
        assert_eq!(
            pair[1].1,
            pair[0].1 + 1,
            "the refill is the contiguous listing"
        );
        assert_eq!(
            pair[0].1, pair[0].0,
            "an unfiltered view position is its listing row"
        );
    }
    for (_, row, painted) in &refilled {
        assert_eq!(
            painted.as_deref(),
            Some(name(*row).as_str()),
            "row {row} drew a stale or empty name after the clear"
        );
    }
    // The clear asked nothing new of the filter: no second request, and the next Escape is the list's.
    let mut after = Vec::new();
    while let Some(sent) = read() {
        eprintln!("GH315 after clear sent {sent}");
        after.push(sent);
    }
    assert!(
        after.iter().all(|sent| !sent.contains(r#""c":"filter""#)),
        "the clear sent another filter request: {after:?}"
    );
}
