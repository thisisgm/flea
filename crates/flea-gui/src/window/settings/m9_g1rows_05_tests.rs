use super::store::Update;
use crate::{
    backend::{Backend, Event},
    navigation::folder_state::{Folders, Order, Writer},
    window::navigate::Step,
};
use flea::{
    jsondoc::Json,
    protocol::{Reply, RowFacts},
};
use gpui::{AppContext, TestAppContext};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn reread(document: &str) -> Update {
    Update::Loaded {
        document: flea::uistate::from_file(document),
        through: 0,
        saved: false,
    }
}

#[gpui::test]
fn m9_g1rows_05_another_window_never_replaces_this_window_global_sort(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.apply_settings(
                flea::uistate::from_file(r#"{"sort":{"key":"size","reverse":true}}"#),
                window,
                cx,
            );
            shell.receive_settings(
                reread(r#"{"sort":{"key":"name","reverse":false},"hidden":true}"#),
                window,
                cx,
            );
            let size = Order {
                key: "size",
                desc: true,
            };
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.nav.folders.borrow().global, size);
            assert!(host.nav.folders.borrow().hidden, "shared keys still arrive");
            assert_eq!(
                shell.launch.settings.data.get("sort"),
                flea::uistate::from_file(r#"{"sort":{"key":"size","reverse":true}}"#).get("sort")
            );
        })
    });
}

#[gpui::test]
fn m9_g1rows_05_unfinished_folder_sort_survives_a_stale_reread(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            *host.nav.writer.borrow_mut() = Writer::held(Arc::new(AtomicUsize::new(0)));
            *host.nav.folders.borrow_mut() =
                Folders::from_settings(&flea::uistate::from_file("{}"), Vec::new());
            assert_eq!(shell.step(0, Step::Open("/B".into()), window, cx), Ok(true));
            for reply in [
                Reply::Listed {
                    path: "/B".into(),
                    total: 1,
                },
                Reply::Rows {
                    start: 0,
                    rows: vec![RowFacts::default()],
                    listing: 3,
                },
            ] {
                shell.receive(0, Event::Reply(reply), window, cx);
            }
            assert_eq!(
                shell.step(0, Step::SortColumn("size".into()), window, cx),
                Ok(true)
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.nav.writer.borrow().sort_writes(), 1);
            let chosen = host.nav.folders.borrow().get("/B");
            assert!(chosen.is_some(), "the explicit choice is remembered");
            shell.receive_settings(reread("{}"), window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.nav.folders.borrow().get("/B"), chosen);
            let saved = shell
                .launch
                .settings
                .data
                .get("folderSorts")
                .and_then(|map| map.get("/B"))
                .and_then(|entry| entry.get("key"))
                .and_then(Json::as_str);
            assert_eq!(saved, Some("size"));
            // Every write so far finishes, the sort write with the path write before it.
            let writer = &shell.panes[0].as_ref().unwrap().nav.writer;
            let sent = writer.borrow().sent;
            writer.borrow().done().store(sent, Ordering::Release);
            shell.receive_settings(reread("{}"), window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.nav.folders.borrow().get("/B"),
                None,
                "a finished write leaves the file's outcome"
            );
        })
    });
}
