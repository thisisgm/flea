//! Menu sorts retain Search and do not remember the results' order as the folder's own.
use super::*;
use crate::g2::context_menus::menu::Outcome;
use flea::{jsondoc::Json, ui_logic_context_menus::rows::SORT_ORDERS};

#[gpui::test]
fn gridchrome_search_menu_sorts_results_without_changing_folder_preferences(
    cx: &mut TestAppContext,
) {
    for kind in [Kind::Names, Kind::Contents] {
        for mode in [Mode::Typing, Mode::Results] {
            for (leaf, _) in SORT_ORDERS {
                let (backend, mut requests) = Backend::test_lines();
                let (shell, cx) = open(backend, cx);
                shell.update(cx, |shell, _| {
                    let host = shell.panes[0].as_mut().unwrap();
                    host.search.owner.session.kind = kind;
                    host.search.owner.session.mode = mode;
                    host.search.owner.session.query = "notes".into();
                    host.pane.state.search = true;
                    host.nav.folders.borrow_mut().remember_sort = true;
                });
                while requests().is_some() {}
                let before = shell.read_with(cx, |shell, _| {
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .nav
                        .writer
                        .borrow()
                        .patches
                        .clone()
                });
                cx.update(|window, cx| {
                    shell.update(cx, |shell, cx| {
                        shell.open_menu(0, None, (500., 300.), window, cx);
                        let offered = shell.menu_entries();
                        let sort = offered.iter().find(|entry| entry.action == "sort").unwrap();
                        assert!(sort
                            .submenu
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|entry| entry.id == leaf));
                        shell.menu_outcome(
                            Outcome::Fire {
                                action: "sort".into(),
                                leaf: leaf.into(),
                            },
                            window,
                            cx,
                        );
                    })
                });
                let sent: Vec<_> = std::iter::from_fn(&mut requests)
                    .map(|line| flea::jsondoc::parse(&line).unwrap())
                    .collect();
                let sorts: Vec<_> = sent
                    .iter()
                    .filter(|line| line.get("c").and_then(Json::as_str) == Some("sort"))
                    .collect();
                assert_eq!(sorts.len(), 1, "{kind:?} {mode:?} {leaf}: {sent:?}");
                assert_eq!(sorts[0].get("by").and_then(Json::as_str), Some(leaf));
                assert_eq!(
                    sorts[0].get("desc").and_then(Json::as_bool),
                    Some(leaf == "name")
                );
                shell.read_with(cx, |shell, _| {
                    let host = shell.panes[0].as_ref().unwrap();
                    assert_eq!(host.search.owner.session.kind, kind);
                    assert_eq!(host.search.owner.session.mode, mode);
                    assert_eq!(host.search.owner.session.query, "notes");
                    assert!(host.pane.state.search);
                    assert_eq!(host.nav.writer.borrow().patches, before);
                    assert!(host.nav.folders.borrow().get(&host.path).is_none());
                    assert!(host.nav.pending_sort.is_none());
                });
                shell.update(cx, |shell, _| shell.stop_for_app_quit());
            }
        }
    }
}
