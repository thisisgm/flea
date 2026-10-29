use super::Step;
use crate::{
    backend::{Backend, Event as BackendEvent},
    navigation::{history::Stack, watch::Gate},
    state::tests::shell_with,
};
use flea::protocol::{Reply, RowFacts};
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn shell_routes_steps_watch_notices_and_landings_through_one_navigator(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            shell.receive(
                0,
                BackendEvent::Changed {
                    path: "/elsewhere".into(),
                },
                window,
                cx,
            );
            assert!(shell.panes[0].as_ref().unwrap().nav.watch.due.is_none());
            shell.receive(
                0,
                BackendEvent::Changed {
                    path: "/files".into(),
                },
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.nav.watch.due.is_some() && host.watch_task.is_some());
            assert_eq!(shell.step(0, Step::Back, window, cx), Ok(false));
            assert_eq!(
                shell.step(0, Step::Open("/files/x".into()), window, cx),
                Ok(true)
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                (host.pane.place(), host.pane.path()),
                ("/files/x", "/files")
            );
            assert_eq!(host.nav.history.paths(Stack::Back), ["/files"]);
            assert_eq!(host.nav.counters.lists, 1);
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Listed {
                    path: "/files/x".into(),
                    total: 2,
                }),
                window,
                cx,
            );
            let rows = vec![
                RowFacts {
                    name: "a".into(),
                    ..RowFacts::default()
                };
                2
            ];
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Rows {
                    start: 0,
                    rows,
                    listing: 3,
                }),
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                (host.path.as_str(), host.nav.watch.stale),
                ("/files/x", false),
                "the old folder's debt is dropped"
            );
            assert_eq!(shell.path_labels[0].as_ref(), "/files/x");
            shell.set_gate(
                0,
                Gate {
                    editing: true,
                    ..Gate::default()
                },
                window,
                cx,
            );
            assert_eq!(shell.step(0, Step::SortNext, window, cx), Ok(false));
            shell.set_gate(0, Gate::default(), window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().nav.sort_mark(), "size:asc");
            assert_eq!(shell.step(0, Step::Back, window, cx), Ok(true));
            assert_eq!(
                shell.step(0, Step::Refresh(None), window, cx),
                Err(crate::navigation::Refusal::Busy)
            );
            assert_eq!(
                shell.panes[0].as_ref().unwrap().status.as_ref(),
                "A directory is already loading."
            );
        })
    });
}

#[gpui::test]
fn a_resolved_anchor_pays_a_held_watch_debt(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            let mut anchor =
                crate::navigation::anchor::Anchor::capture(&host.pane, &[], false, false);
            anchor.locate = Some(7);
            host.nav.anchor = Some(anchor);
            let now = host.pane.now();
            assert!(host.nav.changed(&host.pane, "/files", now));
            assert!(
                !host.nav.fire_watch(&mut host.pane, &[], now + 400),
                "the anchor holds the re-read"
            );
            assert!(host.nav.watch.stale && host.nav.watch.due.is_none());
            let located = crate::backend::Located {
                directory: "/files".into(),
                path: String::new(),
                index: -1,
                id: Some(7),
                transfer: 0,
                ok: true,
                matches: Vec::new(),
            };
            shell.receive(0, BackendEvent::Located(located), window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.nav.anchor.is_none(),
                "the batch reply resolved the anchor"
            );
            assert!(
                host.nav.watch.due.is_some(),
                "the held debt must be re-armed once nothing holds the rows"
            );
        })
    });
}

#[gpui::test]
fn a_returning_window_rereads_a_fuse_folder_and_leaves_a_local_one_to_inotify(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let facts = |name: &str| crate::backend::status::Filesystem {
                class: String::new(),
                tagable: None,
                path: "/files".into(),
                name: name.into(),
                free: 0,
            };
            shell.surfaces[0].filesystem.facts = Some(facts("btrfs"));
            shell.window_returned(window, cx);
            assert!(shell.panes[0].as_ref().unwrap().nav.watch.due.is_none());
            shell.surfaces[0].filesystem.facts = Some(facts("fuse"));
            shell.window_returned(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.nav.watch.stale,
                "the return owes the folder a re-read (#324)"
            );
            assert!(host.nav.watch.due.is_some() && host.watch_task.is_some());
        })
    });
}
