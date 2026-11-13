//! Issue #315: leaving a drawn Columns filter must replace its scroll extent before revealing the cursor.
use super::{edit, frame};
use crate::{
    backend::{Backend, Event},
    columns::geometry::Slot,
    state::{tests::shell_with, Shell},
    window::{columns_tests::host, root::Root, startup::View},
};
use flea::protocol::{Reply, RowFacts};
use gpui::{AppContext, Context, KeyDownEvent, Keystroke, Window};
use std::rc::Rc;

const TOTAL: usize = 5000;

fn facts(range: std::ops::Range<usize>) -> Vec<RowFacts> {
    range
        .map(|row| RowFacts {
            name: format!("file-{:04}.txt", row + 1),
            ..Default::default()
        })
        .collect()
}

fn key(
    shell: &mut Shell,
    key: &str,
    text: Option<&str>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    shell.key(
        &KeyDownEvent {
            keystroke: Keystroke {
                key: key.into(),
                key_char: text.map(Into::into),
                ..Default::default()
            },
            is_held: false,
            prefer_character_input: false,
        },
        window,
        cx,
    );
}

#[test]
fn gh315_columns_one_escape_replaces_the_filtered_scroll_extent() {
    let mut cx = host::context();
    let (backend, _requests) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    edit(&mut cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        pane.held = Rc::new(crate::pane::Held {
            listing: 2,
            total: TOTAL,
            start: 1600,
            rows: facts(1600..1920),
        });
        pane.cursor = 1666;
        pane.scroll_extent = None;
        pane.reveal_cursor();
        shell.set_view(View::Columns, window, cx);
        window.focus(&shell.focus, cx);
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(
            crate::pane::swap::CAP_MS + 1,
        ));
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        assert!(shell.panes[0].as_ref().unwrap().pane.scroll_y > 0.);
        assert!(!shell.panes[0].as_ref().unwrap().pane.busy());
        key(shell, "/", Some("/"), window, cx);
        for digit in ["0", "3", "0", "0"] {
            key(shell, digit, Some(digit), window, cx);
        }
        let id = shell.panes[0]
            .as_ref()
            .unwrap()
            .filter
            .owner
            .asked()
            .unwrap()
            .id;
        let answer = format!(
            r#"{{"t":"filtered","id":{id},"query":"0300","n":5000,"rows":[299],"ms":0.1,"listing":2}}"#
        );
        shell.receive(0, Backend::test_reply_line(&answer), window, cx);
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        let request = pane
            .requests
            .outstanding
            .expect("the off-window match asks for rows");
        shell.receive(
            0,
            Event::Reply(Reply::Rows {
                start: request.start,
                listing: 2,
                rows: facts(request.start..(request.start + request.count).min(TOTAL)),
            }),
            window,
            cx,
        );
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        let pane = shell.panes[0].as_ref().unwrap();
        assert_eq!(pane.pane.cursor, 299);
        // One filtered row of 31 px and the active column's ColumnPane.qml footer.
        assert_eq!(
            pane.pane.scroll_extent,
            Some(31. + crate::columns::geometry::FOOTER)
        );
        let paint = pane
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        assert_eq!(paint.visible().collect::<Vec<_>>(), vec![299]);
        assert_eq!(paint.prepared(299).unwrap().label, "file-0300.txt");
        drop(paint);
        key(shell, "escape", None, window, cx);
        let pane = shell.panes[0].as_ref().unwrap();
        assert!(pane.filter.shown.is_none() && shell.slots.strips[0].is_none());
        assert_eq!(pane.pane.cursor, 299);
        assert!(
            pane.pane.scroll_y > 0.,
            "one Escape must reveal the cursor beyond the old one-row extent"
        );
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        let request = pane
            .requests
            .outstanding
            .expect("clearing the filter refills its unfiltered viewport");
        assert!(
            pane.held.row(299).is_some(),
            "the cursor keeps its filtered facts until the refill lands"
        );
        shell.receive(
            0,
            Event::Reply(Reply::Rows {
                start: request.start,
                listing: 2,
                rows: facts(request.start..(request.start + request.count).min(TOTAL)),
            }),
            window,
            cx,
        );
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, _, _| {
        let pane = shell.panes[0].as_ref().unwrap();
        let paint = pane
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        let visible: Vec<_> = paint.visible().collect();
        assert!(
            visible.len() > 1 && visible.contains(&299),
            "the restored viewport paints the cursor: {visible:?}"
        );
        assert!(visible.windows(2).all(|pair| pair[1] == pair[0] + 1));
        assert_eq!(paint.prepared(299).unwrap().label, "file-0300.txt");
        assert_eq!(paint.drawn.as_ref().unwrap().scroll, pane.pane.scroll_y);
    });
}
