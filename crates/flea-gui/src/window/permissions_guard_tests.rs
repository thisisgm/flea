//! Captures that no longer identify a live selected item never issue a Permissions inspect.
use super::fixture::{edit, setup};
use crate::{
    g2::context_menus::join::{Captured, Chosen},
    window::dispatch::Order,
};
use gpui::TestAppContext;
use std::{cell::Cell, rc::Rc};

fn chosen() -> Chosen {
    Chosen {
        captured: Captured {
            side: 0,
            listing: 2,
            rows: vec![0],
            cursor: 0,
            path: "/files".into(),
            identity: String::new(),
            id: 8,
            ready: true,
            kinds: vec![0o100644],
            background: false,
            target: None,
        },
        action: "permissions".into(),
        leaf: String::new(),
        paths: vec!["/files/notes.txt".into()],
        claimed: Rc::new(Cell::new(false)),
    }
}

#[gpui::test]
fn m9_permissions_rejects_unavailable_captures_without_sending_inspect(cx: &mut TestAppContext) {
    for held in [false, true] {
        for case in 0..8 {
            let (handle, mut requests) = setup(cx);
            edit(cx, handle, |shell, window, cx| {
                let mut choice = chosen();
                match case {
                    0 => choice.captured.ready = false,
                    1 => choice.captured.background = true,
                    2 => choice.paths.clear(),
                    3 => choice.captured.listing += 1,
                    4 => choice.captured.path = "/another-folder".into(),
                    5 => shell.panes[0].as_mut().unwrap().stopped = true,
                    6 => shell.panes[0].as_mut().unwrap().backend = None,
                    7 => shell.settings.opened = true,
                    _ => unreachable!(),
                }
                shell.menu.captured = held.then(|| choice.captured.clone());
                shell.dispatch(Order::Menu(choice), window, cx);
                assert!(
                    shell.permissions.view.is_none(),
                    "capture case {case}, held={held}"
                );
                assert!(
                    shell.menu.captured.is_none(),
                    "capture case {case}, held={held} retained its menu"
                );
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().status.to_string(),
                    shell.locale.catalog.tr("menu.refusal.unavailable"),
                    "capture case {case}, held={held} says why it could not open"
                );
            });
            let requests: Vec<_> = std::iter::from_fn(&mut requests).collect();
            assert!(
                !requests
                    .iter()
                    .any(|line| line.contains(r#""c":"permissions""#)),
                "capture case {case}, held={held} issued a request"
            );
            // Background menus never snapshot; a missing backend has already dropped its snapshots.
            if case != 1 && case != 6 {
                assert_eq!(
                    requests
                        .iter()
                        .filter(|line| line.contains(r#""op":"close","id":8"#))
                        .count(),
                    1,
                    "capture case {case}, held={held} releases its reviewed paths once"
                );
            }
        }
    }
}
