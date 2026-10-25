//! The production constructor, real Tab order and native key dispatcher own these checks.
use super::*;
use crate::window::input::ActionEvent;
use std::{cell::RefCell, rc::Rc};

fn tab_to_rail(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    for _ in 0..3 {
        frame(cx);
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            window.focus(&shell.focus, cx);
            shell.keyboard.context = flea::keys::Context::Listing;
        })
    });
    cx.simulate_keystrokes("tab");
    frame(cx);
    cx.update(|window, cx| {
        shell.read_with(cx, |shell, cx| {
            assert!(
                shell.dual_focus.rail.contains_focused(window, cx),
                "Tab did not reach the actual mounted rail"
            );
            assert_eq!(shell.keyboard.context, flea::keys::Context::Rail);
        })
    });
}

#[gpui::test]
fn gcomp36_real_constructor_tab_rail_ctrl_l_focuses_actual_path_field(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, "list", cx);
    tab_to_rail(&shell, cx);
    cx.simulate_keystrokes("ctrl-l");
    frame(cx);
    cx.update(|window, cx| {
        shell.read_with(cx, |shell, cx| {
            assert!(
                shell.path_holds_keyboard(window, cx),
                "focused rail swallowed the actual pathBar handler"
            );
            assert!(!shell.dual_focus.rail.contains_focused(window, cx));
        })
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn gcomp36_focused_rail_passes_window_handlers_and_swallows_listing_actions(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(false, "list", cx);
    tab_to_rail(&shell, cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            sink.borrow_mut().push((event.action, event.context));
        })
    });
    // Plain F5 must cross the non-Dual seam.key branch and reach the window action owner.
    for key in ["f5", "ctrl-r", "ctrl-t", "t"] {
        cx.simulate_keystrokes(key);
        frame(cx);
    }
    // The window runs reload itself now, so only the actions with no owner yet reach observers; dispatch/navigation_tests.rs proves the rail reload lists.
    assert_eq!(
        seen.borrow().as_slice(),
        &[("openTerminal", flea::keys::Context::Rail),],
        "focused rail swallowed a real window action handler"
    );
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.strip.tabs[0].count(),
            2,
            "T reached the mounted tab owner"
        );
        assert_eq!(shell.strip.tabs[0].current(), 1);
    });
    let before = seen.borrow().clone();
    for key in ["ctrl-c", "delete", "ctrl-x", "y"] {
        cx.simulate_keystrokes(key);
        frame(cx);
    }
    assert_eq!(
        *seen.borrow(),
        before,
        "rail leaked a listing copy/delete action"
    );
    assert!(shell.read_with(cx, |shell, _| shell.overlay.is_none()));
    cx.simulate_keystrokes("ctrl-,");
    frame(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.settings.opened),
        "focused rail swallowed the real settings handler"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn rail_rename_f1_printable_t_stays_in_actual_editor(cx: &mut TestAppContext) {
    use flea::ui_logic_pl4::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
    };

    let (shell, cx) = open(false, "list", cx);
    let producer = shell.read_with(cx, |shell, _| {
        shell
            .rail
            .as_ref()
            .expect("the real constructor mounted rail")
            .places
            .clone()
    });
    // The owner seam supplies the supported Network/Share rename fixture to the mounted producer.
    producer.update(cx, |producer, cx| {
        producer.set_tail(
            vec![RailSection {
                heading: Heading::Key("places.heading.network"),
                rows: vec![RailEntry {
                    group: Group::Network,
                    kind: Kind::Share,
                    label: "Rename bookmark".into(),
                    uri: "smb://rename-fixture/share".into(),
                    glyph: "server",
                    ..RailEntry::default()
                }],
            }],
            cx,
        );
    });
    tab_to_rail(&shell, cx);
    let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let sink = seen.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            sink.borrow_mut().push(event.action);
        })
    });
    cx.simulate_keystrokes("end");
    frame(cx);
    let index = producer.read_with(cx, |producer, _| {
        let index = producer.cursor().expect("End selected the real bookmark");
        let entry = &producer.entries()[index];
        assert_eq!((entry.group, entry.kind), (Group::Network, Kind::Share));
        assert_eq!(entry.uri, "smb://rename-fixture/share");
        index
    });
    cx.simulate_keystrokes("f2");
    frame(cx);
    producer.read_with(cx, |producer, _| {
        assert_eq!(
            producer.rename_index(),
            Some(index),
            "F2 must start the real rename"
        );
        assert_eq!(producer.rename_text(), "Rename bookmark");
        assert!(
            producer.field_shown(),
            "the real rename field must be painted"
        );
        let model = producer.model.borrow();
        assert_eq!(model.editor.selected, 0.."Rename bookmark".len());
    });
    assert!(
        seen.borrow().is_empty(),
        "rename setup leaked a window action"
    );
    cx.simulate_keystrokes("t");
    frame(cx);
    assert_eq!(
        (
            producer.read_with(cx, |producer, _| producer.rename_text()),
            seen.borrow().clone(),
        ),
        ("t".to_owned(), Vec::<&'static str>::new()),
        "active rail rename must own printable t without dispatching a window/tab action",
    );
    assert_eq!(
        producer.read_with(cx, |producer, _| producer.rename_index()),
        Some(index)
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn gcomp45_path_escape_from_rail_restores_listing_context_and_ctrl3_grid(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, "list", cx);
    tab_to_rail(&shell, cx);
    cx.simulate_keystrokes("ctrl-l");
    frame(cx);
    let path = cx.update(|window, cx| {
        shell.read_with(cx, |shell, cx| {
            let chrome = shell.chrome.view.as_ref().expect("real mounted chrome");
            let path = chrome.read_with(cx, |chrome, _| chrome.path.clone());
            assert!(
                path.read(cx).editing(),
                "Ctrl+L did not open the real editor"
            );
            assert!(shell.path_holds_keyboard(window, cx));
            assert!(!shell.dual_focus.rail.contains_focused(window, cx));
            path
        })
    });
    // Escape must reach the focused PathView, whose Closed event is subscribed by Shell.
    cx.simulate_keystrokes("escape");
    frame(cx);
    let closed_context = cx.update(|window, cx| {
        shell.read_with(cx, |shell, cx| {
            assert!(
                !path.read(cx).editing(),
                "Escape did not close the real editor"
            );
            assert!(!shell.path_holds_keyboard(window, cx));
            assert!(
                shell.focus.is_focused(window),
                "list focus was not restored"
            );
            assert_eq!(shell.focus_view(window, cx), "list");
            assert!(!shell.dual_focus.rail.contains_focused(window, cx));
            assert_eq!(shell.view, crate::window::startup::View::List);
            shell.keyboard.context
        })
    });
    // Keep the actual post-close key in the red path, even if logical context is stale.
    cx.simulate_keystrokes("ctrl-3");
    frame(cx);
    let actual_view = shell.read_with(cx, |shell, _| shell.view);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    assert_eq!(
        (closed_context, actual_view),
        (
            flea::keys::Context::Listing,
            crate::window::startup::View::Grid
        ),
        "path close must restore Listing key ownership and let Ctrl+3 reach the real Grid handler",
    );
}
