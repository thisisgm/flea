use crate::state::tests::shell_with;
use crate::{
    backend::{Backend, Event as BackendEvent, Failure},
    window::locale::Snapshot,
};
use flea::protocol::Reply;
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn navigation_commits_host_path_selection_and_count_only_with_rows(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.changed(&shell.locale);
            host.selected = vec![2, 4];
            host.pane.cursor = 4;
            assert!(shell.navigate(
                0,
                "/next".into(),
                None,
                crate::pane::swap::Options::default(),
                window,
                cx
            ));
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!((host.path.as_str(), host.pane.held.total), ("/files", 1000));
            assert_eq!(host.selected, vec![2, 4]);
            assert_eq!(host.pane.cursor, 4);
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Listed {
                    path: "/next".into(),
                    total: 2,
                }),
                window,
                cx,
            );
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.held.total, 1000);
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default(); 2],
                    listing: 3,
                }),
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!((host.path.as_str(), host.pane.held.total), ("/next", 2));
            assert!(host.selected.is_empty());
            assert_eq!(shell.path_labels[0].as_ref(), "/next");
            assert_eq!(host.status.as_ref(), "2 items");
        });
    });
}

#[gpui::test]
fn terminal_failure_cancels_a_hold_and_locale_refresh_keeps_the_failure(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            assert!(shell.navigate(
                0,
                "/next".into(),
                None,
                crate::pane::swap::Options::default(),
                window,
                cx
            ));
            shell.receive(
                0,
                BackendEvent::Failed(Failure {
                    key: "window.backend_eof",
                    detail: String::new(),
                }),
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert!(!host.pane.busy());
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Error);
            assert_eq!(host.pane.held.total, 0);
            assert!(host.transition.is_none());
            assert_eq!(
                host.error.as_deref(),
                Some("The backend closed its reply stream")
            );
            let locale = Snapshot {
                revision: 2,
                catalog: shell.locale.catalog.clone(),
            };
            shell.apply_locale(locale, cx);
            assert_eq!(
                shell.panes[0].as_ref().unwrap().error.as_deref(),
                Some("The backend closed its reply stream")
            );
        });
    });
}
