use crate::navigation::folder_state::Order;
use crate::{backend::Backend, state::tests::shell_with, window::navigate::Step};
use flea::protocol::{Reply, RowFacts};
use gpui::{AppContext, Render, TestAppContext};

fn deliver(
    shell: &mut crate::state::Shell,
    path: &str,
    listing: usize,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) {
    for reply in [
        Reply::Listed {
            path: path.into(),
            total: 3,
        },
        Reply::Rows {
            start: 0,
            listing,
            rows: vec![RowFacts::default(); 3],
        },
    ] {
        shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
    }
    let _ = shell.render(window, cx);
}

#[gpui::test]
fn m9_g1rows_05_shell_restored_header_waits_for_matching_rows(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let data =
                flea::uistate::from_file(r#"{"folderSorts":{"/B":{"key":"size","reverse":true}}}"#);
            *shell.panes[0].as_ref().unwrap().nav.folders.borrow_mut() =
                super::super::folder_state::Folders::from_settings(&data, Vec::new());
            assert_eq!(shell.step(0, Step::Open("/B".into()), window, cx), Ok(true));
            let _ = shell.render(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.path(), "/files", "old rows remain held");
            assert_eq!(
                host.list.borrow().view.order,
                Order::default(),
                "the header must retain the old order while the old rows are drawn"
            );
            deliver(shell, "/B", 3, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.path(), "/B");
            assert_eq!(host.pane.held.listing, 3);
            assert_eq!(
                host.list.borrow().view.order,
                Order {
                    key: "size",
                    desc: true
                }
            );
        })
    });
}

#[gpui::test]
fn m9_g1rows_05_shell_headers_follow_each_sort_pair_and_a_refused_sort(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, Step::SortColumn("size".into()), window, cx),
                Ok(true)
            );
            assert_eq!(
                shell.step(0, Step::SortColumn("mtime".into()), window, cx),
                Ok(true)
            );
            let _ = shell.render(window, cx);
            assert_eq!(
                shell.panes[0].as_ref().unwrap().list.borrow().view.order,
                Order::default()
            );
            deliver(shell, "/files", 3, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.nav.order.key, "mtime",
                "the second sort remains requested"
            );
            assert_eq!(
                host.list.borrow().view.order,
                Order {
                    key: "size",
                    desc: false
                }
            );
            deliver(shell, "/files", 4, window, cx);
            assert_eq!(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .list
                    .borrow()
                    .view
                    .order
                    .key,
                "mtime"
            );
            assert_eq!(
                shell.step(0, Step::SortColumn("kind".into()), window, cx),
                Ok(true)
            );
            shell.receive(
                0,
                crate::backend::Event::Refused {
                    operation: "sort".into(),
                    path: "/files".into(),
                    message: String::new(),
                    mode: 0,
                },
                window,
                cx,
            );
            let _ = shell.render(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.list.borrow().view.order.key, "mtime");
            assert_eq!(
                host.nav.sort_snapshot(&host.pane).unwrap().order.key,
                "mtime"
            );
        })
    });
}

#[gpui::test]
fn ey476_shell_external_folder_map_updates_memory_not_explicit_order(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            *shell.panes[0].as_ref().unwrap().nav.folders.borrow_mut() =
                super::super::folder_state::Folders::from_settings(
                    &flea::uistate::from_file("{}"),
                    Vec::new(),
                );
            assert_eq!(
                shell.step(0, Step::SortColumn("size".into()), window, cx),
                Ok(true)
            );
            deliver(shell, "/files", 3, window, cx);
            let data = flea::uistate::from_file(
                r#"{"folderSorts":{"/files":{"key":"kind","reverse":true}}}"#,
            );
            shell.apply_settings(data, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.nav.order,
                Order {
                    key: "size",
                    desc: false
                }
            );
            assert_eq!(
                host.nav.folders.borrow().get("/files"),
                Some(Order {
                    key: "kind",
                    desc: true
                })
            );
            deliver(shell, "/files", 4, window, cx);
            assert_eq!(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .list
                    .borrow()
                    .view
                    .order
                    .key,
                "size"
            );
        })
    });
}
