//! The original reader name is answered by the adapter set mounted under a normal window root.
use crate::{backend::Backend, state::tests::shell_with, window::root::Root};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn pb_scroll_preview_reader_reads_rendered_and_source_positions(cx: &mut TestAppContext) {
    use crate::g2::{
        doc_preview::markdown,
        quick_look_overlay::{doc::View, Body},
    };
    use std::sync::Arc;
    cx.executor().allow_parking();
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            let mut rows = pane.held.rows.clone();
            let row = &mut rows[0];
            row.name = "readme.md".into();
            row.icon = "text-markdown".into();
            pane.held = std::rc::Rc::new(crate::list::pane::Held {
                listing: pane.held.listing,
                total: pane.held.total,
                start: pane.held.start,
                rows,
            });
            assert!(shell.open_quick_look(0, window, cx));
        });
        Root { shell: entity }
    });
    let shell = shell.read_with(cx, |root, _| root.shell.clone());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let look = shell.quick_look.as_ref().unwrap().view.clone();
            let doc = match &look.read(cx).subject().unwrap().body {
                Body::Doc(doc) => doc.clone(),
                _ => panic!("Markdown has its own mounted document"),
            };
            let View::Markdown(markdown) = doc.view else {
                panic!("Markdown view")
            };
            markdown.update(cx, |view, cx| {
                view.set_path(None, cx);
                view.doc.status = markdown::Status::Ready;
                view.doc.height = 4000.;
                view.frame = (800., 200.);
                view.scroll_by(27.5, cx);
            });
            let read = |shell: &mut crate::state::Shell,
                        window: &mut gpui::Window,
                        cx: &mut gpui::Context<crate::state::Shell>| {
                let (answer, receive) = std::sync::mpsc::sync_channel(1);
                shell.ipc(
                    crate::ipc::server::Event::Call {
                        request: Request::Call {
                            target: "flea".into(),
                            function: "previewScrollY".into(),
                            arguments: vec![],
                        },
                        answer,
                    },
                    window,
                    cx,
                );
                receive.try_recv().unwrap().reply
            };
            // Rendered answers PreviewMarkdown.qml's contentY, which rests at minus the body's top inset.
            let inset = markdown.read(cx).tokens.inset_y();
            assert!(inset > 0.);
            assert_eq!(
                read(shell, window, cx),
                Reply::value(((27.5 - inset + 0.5_f32).floor()).to_string())
            );
            markdown.update(cx, |view, cx| {
                let text: Arc<str> = "line\n".repeat(100).into();
                let starts = Arc::new(markdown::line_starts(&text));
                view.source.set_text(text, starts);
                view.source.answer = markdown::Answer::Plain;
                view.source.total = 100;
                view.source.frame.line = 20.;
                view.source.frame.height = 100.;
                view.source.frame.wrap = false;
                view.set_view(markdown::View::Source, cx);
                view.scroll_by(42.5, cx);
            });
            assert_eq!(read(shell, window, cx), Reply::value("43"));
            look.update(cx, |look, cx| look.close(cx));
            assert_eq!(read(shell, window, cx), Reply::value("-1"));
        })
    });
}

#[gpui::test]
fn pb_scroll_preview_reader_is_mounted_and_closed_returns_minus_one(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let mut shell = None;
    let (_, cx) = cx.add_window_view(|_, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        shell = Some(entity.clone());
        Root { shell: entity }
    });
    let shell = shell.unwrap();
    cx.run_until_parked();
    let reply = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "previewScrollY".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply
        })
    });
    assert_eq!(reply, Reply::value("-1"));
}

#[gpui::test]
fn ey260_preview_table_answers_an_object_from_quick_look_never_the_column_facts(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            let mut rows = pane.held.rows.clone();
            rows[0].name = "readme.md".into();
            rows[0].icon = "text-markdown".into();
            pane.held = std::rc::Rc::new(crate::list::pane::Held {
                listing: pane.held.listing,
                total: pane.held.total,
                start: pane.held.start,
                rows,
            });
            assert!(shell.open_quick_look(0, window, cx));
        });
        Root { shell: entity }
    });
    let shell = shell.read_with(cx, |root, _| root.shell.clone());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let read = |shell: &mut crate::state::Shell,
                        window: &mut gpui::Window,
                        cx: &mut gpui::Context<crate::state::Shell>| {
                let (answer, receive) = std::sync::mpsc::sync_channel(1);
                shell.ipc(
                    crate::ipc::server::Event::Call {
                        request: Request::Call {
                            target: "flea".into(),
                            function: "previewTable".into(),
                            arguments: vec![],
                        },
                        answer,
                    },
                    window,
                    cx,
                );
                receive.try_recv().unwrap().reply
            };
            // No table overflows the card's document yet, so the contract's empty object answers.
            assert_eq!(read(shell, window, cx), Reply::value("{}"));
            let look = shell.quick_look.as_ref().unwrap().view.clone();
            look.update(cx, |look, cx| look.close(cx));
            assert_eq!(read(shell, window, cx), Reply::value("{}"));
        })
    });
}
