use crate::{
    columns::geometry::Slot,
    g2::pv1::{glyph::Glyph, Body, Subject},
    row::marks::{Language, Mark},
    window::{columns_tests::host, startup::View},
};
use flea::ui_logic_pv1::{kinds::State, rect::Shape};

fn rust_mark() -> Mark {
    Mark::Language(Language::for_file("main.rs").unwrap())
}

#[test]
fn codeicons_language_list_setting_reaches_drawn_rows_and_invalidates_cache() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx);
        let mut row = host::row("main.rs", false);
        row.mode = 0o100644;
        shell.panes[0].as_mut().unwrap().pane.held = crate::list::bench::held(vec![row]);
    });
    for (document, expected) in [
        ("{}", rust_mark()),
        (r#"{"langMarks":false}"#, Mark::FileText),
        ("{}", rust_mark()),
    ] {
        host::change(&mut cx, handle, |shell, window, cx| {
            shell.apply_settings(flea::uistate::from_file(document), window, cx);
        });
        host::frame(&mut cx, handle);
        host::change(&mut cx, handle, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let prepared = list
                .visible()
                .iter()
                .find(|row| row.index == 0)
                .and_then(|row| row.prepared.as_ref())
                .expect("the file row was drawn");
            assert_eq!(prepared.mark, expected, "runtime settings {document}");
        });
    }
}

#[test]
fn codeicons_language_columns_peek_uses_setting_without_relisting() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b",
            &[("c", true), ("main.rs", false)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    for (document, expected) in [
        ("{}", rust_mark()),
        (r#"{"langMarks":false}"#, Mark::File),
        ("{}", rust_mark()),
    ] {
        host::change(&mut cx, handle, |shell, window, cx| {
            shell.apply_settings(flea::uistate::from_file(document), window, cx);
        });
        host::frame(&mut cx, handle);
        host::change(&mut cx, handle, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns
                .painters
                .get(Slot::Parent)
                .expect("parent track")
                .borrow();
            assert_eq!(paint.prepared(1).expect("peek row").mark, expected);
        });
    }
}

#[test]
fn codeicons_language_quick_look_header_keeps_language_and_symlink_identity() {
    let mut subject = Subject {
        name: "main.rs".into(),
        icon: "text-plain".into(),
        mode: 0o100644,
        state: State::Code,
        shape: Shape::unknown(State::Code),
        body: Body::Text {
            lines: Vec::new(),
            count: String::new(),
        },
    };
    assert_eq!(subject.mark(), Glyph::Row(rust_mark()));
    assert_eq!(
        subject.mark_with_languages(false),
        Glyph::Row(Mark::FileText)
    );
    subject.mode = 0o120777;
    assert_eq!(subject.mark(), Glyph::Row(Mark::Symlink));
    assert_eq!(
        subject.mark_with_languages(false),
        Glyph::Row(Mark::Symlink)
    );
}

#[test]
fn codeicons_language_open_quick_look_follows_live_settings() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx);
        let mut row = host::row("main.rs", false);
        row.mode = 0o100644;
        shell.panes[0].as_mut().unwrap().pane.held = crate::list::bench::held(vec![row]);
        assert!(shell.open_quick_look(0, window, cx));
        assert_eq!(
            shell
                .quick_look
                .as_ref()
                .unwrap()
                .view
                .read(cx)
                .header_mark(),
            Some(Glyph::Row(rust_mark()))
        );
        for (document, expected) in [
            (r#"{"langMarks":false}"#, Mark::FileText),
            ("{}", rust_mark()),
        ] {
            shell.apply_settings(flea::uistate::from_file(document), window, cx);
            assert_eq!(
                shell
                    .quick_look
                    .as_ref()
                    .unwrap()
                    .view
                    .read(cx)
                    .header_mark(),
                Some(Glyph::Row(expected))
            );
        }
    });
}
