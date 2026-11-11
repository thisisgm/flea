//! The production constructor, real Tab order and native key dispatcher own these checks.
use super::*;
use crate::window::input::ActionEvent;
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static TERMINALS: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

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
    TERMINALS.with(|calls| calls.borrow_mut().clear());
    let folder = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.path().to_owned()
    });
    shell.update(cx, |shell, _| {
        shell.opener[0].launch = |argv| {
            TERMINALS.with(|calls| calls.borrow_mut().push(argv.to_vec()));
            (Some(0), String::new())
        };
    });
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
    // Reload and Terminal run in the window; dispatch/navigation_tests.rs proves that the rail reload lists.
    assert_eq!(
        seen.borrow().as_slice(),
        &[],
        "focused rail swallowed a real window action handler"
    );
    let terminals = TERMINALS.with(|calls| calls.take());
    assert_eq!(terminals.len(), 1, "Ctrl+T must reach the terminal owner");
    assert_eq!(&terminals[0][1..], ["--terminal", folder.as_str()]);
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
    use flea::ui_logic_rail_places::{
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

// The Settings panel's section, the side that holds the keys, and the id of the cursor row.
fn settings_panel(
    shell: &gpui::Entity<Shell>,
    cx: &mut VisualTestContext,
) -> (&'static str, crate::g2::settings_pages::Side, String) {
    shell.read_with(cx, |shell, cx| {
        let view = shell.settings.panel.as_ref().unwrap().read(cx);
        let panel = &view.panel;
        (
            panel.section,
            panel.side,
            panel.rows[panel.cursor].id.clone(),
        )
    })
}

// The stored text size mode: absent while following Omarchy, a stop once overridden.
fn display_size_mode(shell: &gpui::Entity<Shell>, cx: &mut VisualTestContext) -> Option<f64> {
    shell.read_with(cx, |shell, _| {
        flea::ui_logic_settings_pages::pages::at(
            &shell.launch.settings.data,
            &["display", "textSize", "mode"],
        )
        .and_then(flea::jsondoc::Json::as_f64)
    })
}

// The Display ruler's stop at the stored size, and the Effective fact as the pane draws it.
fn display_ruler_stop(shell: &gpui::Entity<Shell>, cx: &mut VisualTestContext) -> Option<u32> {
    shell.read_with(cx, |shell, cx| {
        let view = shell.settings.panel.as_ref()?.read(cx);
        let ruler = view.panel.rows.iter().find(|row| row.id == "textStop")?;
        Some(ruler.stops[ruler.index])
    })
}

fn effective_text(shell: &gpui::Entity<Shell>, cx: &mut VisualTestContext) -> Option<String> {
    shell.read_with(cx, |shell, cx| {
        let view = shell.settings.panel.as_ref()?.read(cx);
        let fact = view
            .panel
            .rows
            .iter()
            .find(|row| row.id == "textEffective")?;
        Some(fact.value.resolve(&view.panel.catalog))
    })
}

#[gpui::test]
fn ey220_display_takes_down_and_right_after_tab_from_the_sidebar(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, "list", cx);
    // Seeded at text size 14 on Override, the EY-162 scene.
    shell.update(cx, |shell, _| {
        shell.launch.settings.data =
            flea::uistate::from_file(r#"{"display":{"textSize":{"mode":14}}}"#);
    });
    tab_to_rail(&shell, cx);
    cx.simulate_keystrokes("ctrl-,");
    frame(cx);
    assert!(shell.read_with(cx, |shell, _| shell.settings.opened));
    assert_eq!(display_size_mode(&shell, cx), Some(14.));
    cx.simulate_keystrokes("tab");
    frame(cx);
    assert_eq!(
        settings_panel(&shell, cx).1,
        crate::g2::settings_pages::Side::Rail
    );
    for _ in 0..5 {
        cx.simulate_keystrokes("down");
        frame(cx);
    }
    assert_eq!(settings_panel(&shell, cx).0, "display");
    cx.simulate_keystrokes("tab");
    frame(cx);
    assert_eq!(
        settings_panel(&shell, cx).1,
        crate::g2::settings_pages::Side::Pane,
        "Tab hands the keys from the sidebar to the page, so the sidebar Display row is no longer held"
    );
    cx.simulate_keystrokes("down");
    frame(cx);
    assert_eq!(
        settings_panel(&shell, cx),
        (
            "display",
            crate::g2::settings_pages::Side::Pane,
            "textStop".to_owned()
        ),
        "Down lights the Text size ruler on the Display page"
    );
    cx.simulate_keystrokes("right");
    frame(cx);
    assert_eq!(
        display_size_mode(&shell, cx),
        Some(16.),
        "Right steps the size 14 to 16"
    );
    assert_eq!(display_ruler_stop(&shell, cx), Some(16));
    assert_eq!(effective_text(&shell, cx).as_deref(), Some("16 px"));
    cx.simulate_keystrokes("left");
    frame(cx);
    assert_eq!(
        display_size_mode(&shell, cx),
        Some(14.),
        "Left steps the size back to 14"
    );
    assert_eq!(display_ruler_stop(&shell, cx), Some(14));
    assert_eq!(effective_text(&shell, cx).as_deref(), Some("14 px"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// German types [ with AltGr at the US 8 key, which is text, and ü at the US [ key, which is the tabPrevious chord.
#[gpui::test]
fn altgr_text_in_the_focused_rail_passes_no_window_chord(cx: &mut TestAppContext) {
    use crate::window::dispatch::test_shell::german_press;
    let (shell, cx) = open(false, "list", cx);
    tab_to_rail(&shell, cx);
    let altgr = german_press("[", "[", Some("8"), true);
    let plain = german_press("[", "ü", Some("["), false);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.rail_key(&altgr, window, cx),
                Some(true),
                "the focused rail keeps AltGr+8 as text"
            );
            assert_eq!(
                shell.rail_key(&plain, window, cx),
                None,
                "ü passes tabPrevious on to the window"
            );
        })
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
