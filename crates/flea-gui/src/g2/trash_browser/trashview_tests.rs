//! Trash rail activation through Root, including draw-time registration and the visible view owners.
use super::join_tests::{answer, call, draw, root, shell};
use crate::{list::bench::real_cx, window::root::Root};
use flea::{
    protocol::{Reply, RowFacts},
    ui_logic_rail_places::{order::RailSection, rail::Kind},
};
use gpui::{App, TestAppContext, VisualTestContext, Window, WindowHandle};
use std::cell::Cell;

#[derive(Default)]
struct Registrations {
    total: Cell<usize>,
    during_draw: Cell<usize>,
}
impl gpui::Global for Registrations {}

pub(super) fn record_registration(window: &Window, cx: &App) {
    if let Some(counts) = cx.try_global::<Registrations>() {
        counts.total.set(counts.total.get() + 1);
        if window.current_draw_id().is_some() {
            counts.during_draw.set(counts.during_draw.get() + 1);
        }
    }
}

fn mounted(cx: &mut TestAppContext) -> WindowHandle<Root> {
    cx.update(|cx| cx.set_global(Registrations::default()));
    let window = root(cx);
    shell(cx, window, |shell, window, cx| {
        cx.set_reduce_motion(true);
        shell.mount_rail(cx);
        shell.mount_trash(window, cx);
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, _| {
            pane.monitor_start = None;
        });
        let producer = shell.rail.as_ref().unwrap().places.clone();
        producer.update(cx, |producer, cx| {
            producer.replace_sections(
                vec![RailSection {
                    heading: flea::ui_logic_rail_places::order::Heading::Key(
                        "places.heading.places",
                    ),
                    rows: vec![flea::ui_logic_rail_places::order::trash_entry()],
                }],
                cx,
            );
            producer.key_action("cursorFirst", cx);
        });
        let host = shell.panes[0].as_mut().unwrap();
        host.path = "/Docs".into();
        host.pane = crate::list::pane::Pane::at_row_height("/Docs".into(), 620., 31.);
        host.pane.use_executor(cx.background_executor().clone());
        host.pane
            .receive(Reply::Listed {
                path: "/Docs".into(),
                total: 3,
            })
            .unwrap();
        host.pane
            .receive(Reply::Rows {
                start: 0,
                listing: 3,
                rows: (0..3)
                    .map(|index| RowFacts {
                        name: format!("file-{index}.txt"),
                        ..Default::default()
                    })
                    .collect(),
            })
            .unwrap();
        window.focus(&shell.focus, cx);
        cx.notify();
    });
    draw(cx, window);
    window
}

fn enter(cx: &mut TestAppContext, window: WindowHandle<Root>) {
    cx.simulate_keystrokes(window.into(), "tab");
    shell(cx, window, |shell, window, cx| {
        assert!(shell.dual_focus.rail.contains_focused(window, cx));
    });
    cx.simulate_keystrokes(window.into(), "enter");
    draw(cx, window);
    cx.run_until_parked();
    shell(cx, window, |shell, _, cx| {
        assert!(
            shell.trash_opened(cx),
            "Return must activate the rail's Trash row"
        );
    });
}

fn settle(cx: &mut TestAppContext, window: WindowHandle<Root>, total: usize) {
    let rows = (0..total).map(|index| format!(
        r#"{{"uri":"trash:///item-{index}","original":"/Docs/file-{index}.txt","identity":"id-{index}","size":6}}"#,
    )).collect::<Vec<_>>().join(",");
    answer(
        cx,
        window,
        "list",
        &format!(r#", "total":{total},"start":0,"rows":[{rows}]"#),
    );
    answer(
        cx,
        window,
        "summary",
        &format!(r#", "bytes":{},"partial":false"#, 6 * total),
    );
    draw(cx, window);
}

#[test]
fn trashview_select_all_delete_tab_return_registers_outside_frame_dispatch() {
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    cx.simulate_keystrokes(window.into(), "ctrl-a");
    shell(&mut cx, window, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, 1, 2]);
    });
    cx.simulate_keystrokes(window.into(), "delete");
    enter(&mut cx, window);
    settle(&mut cx, window, 3);
    shell(&mut cx, window, |shell, _, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.back_pressed(cx));
    });
    draw(&mut cx, window);
    enter(&mut cx, window);
    settle(&mut cx, window, 3);
    cx.update(|cx| {
        let counts = cx.global::<Registrations>();
        assert_eq!(
            counts.total.get(),
            1,
            "presentation feedback must still be installed"
        );
        assert_eq!(
            counts.during_draw.get(),
            0,
            "Wayland holds its callbacks borrow across request_frame draw"
        );
    });
}

#[test]
fn trashview_footer_uses_trash_count_and_restores_folder_on_close() {
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    enter(&mut cx, window);
    settle(&mut cx, window, 0);
    shell(&mut cx, window, |shell, _, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        assert_eq!(footer.total, 0);
        assert_eq!(
            footer.counts.as_ref(),
            shell.locale.catalog.trn("items.count", 0)
        );
    });
    shell(&mut cx, window, |shell, _, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.back_pressed(cx));
    });
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        assert_eq!(shell.surfaces[0].footer.as_ref().unwrap().read(cx).total, 3);
        assert_eq!(
            shell
                .rail
                .as_ref()
                .unwrap()
                .places
                .read(cx)
                .model
                .borrow()
                .lit,
            None
        );
    });
}

#[test]
fn trashview_footer_holds_folder_then_tracks_trash_selection_and_disk_facts() {
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    let disk = shell(&mut cx, window, |shell, _, cx| {
        shell.surfaces[0]
            .footer
            .as_ref()
            .unwrap()
            .read(cx)
            .filesystem
            .clone()
    });
    enter(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        assert_eq!(
            footer.total, 3,
            "the held folder owns the footer before Trash lands"
        );
        assert_eq!(footer.path, "/Docs");
    });
    settle(&mut cx, window, 2);
    for (keys, selected) in [("", 1), ("ctrl-a", 2)] {
        if !keys.is_empty() {
            cx.simulate_keystrokes(window.into(), keys);
            answer(
                &mut cx,
                window,
                "select",
                r#", "selectionToken":9,"selectionCount":2"#,
            );
            draw(&mut cx, window);
        }
        shell(&mut cx, window, |shell, _, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            assert_eq!((footer.total, footer.selected), (2, selected));
            assert_eq!(footer.path, shell.locale.catalog.tr("trash.title"));
            assert_eq!(
                footer.filesystem, disk,
                "Trash keeps the covered folder's disk facts"
            );
            assert_eq!(
                footer.counts.as_ref(),
                crate::strips::status_host::selection_head(
                    &shell.locale.catalog,
                    selected,
                    2,
                    -1,
                    "",
                )
            );
        });
    }
}

#[test]
fn trashview_rail_lights_trash_after_return() {
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    enter(&mut cx, window);
    settle(&mut cx, window, 0);
    shell(&mut cx, window, |shell, _, cx| {
        let places = shell.rail.as_ref().unwrap().places.read(cx);
        let lit = places
            .model
            .borrow()
            .lit
            .expect("the Trash rail row must be lit");
        assert_eq!(places.entries()[lit].kind, Kind::Trash);
        assert!(
            !places.model.borrow().focused,
            "the rail uses its unfocused selected fill after Return"
        );
    });
}

#[test]
fn trashview_empty_catalog_line_is_visible_and_centered() {
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    enter(&mut cx, window);
    settle(&mut cx, window, 0);
    shell(&mut cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        assert_eq!(
            pane.empty_line(),
            Some((shell.locale.catalog.tr("trash.is_empty"), false))
        );
    });
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    let area = visual.debug_bounds("trash-view").unwrap();
    let line = visual.debug_bounds("trash-empty-line").unwrap();
    assert!(line.size.width > gpui::px(0.) && line.size.height > gpui::px(0.));
    assert_eq!(
        line.center(),
        area.center(),
        "TrashView.qml centres its empty line in the whole view"
    );
}

#[test]
fn trashview_comma_then_escape_keeps_trash_and_m_targets_the_trash_row() {
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    cx.simulate_keystrokes(window.into(), "delete");
    draw(&mut cx, window);
    enter(&mut cx, window);
    settle(&mut cx, window, 2);
    cx.simulate_keystrokes(window.into(), ",");
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "settingsOpen", &[]), "true");
    shell(&mut cx, window, |shell, _, cx| {
        assert!(shell.trash_opened(cx), "comma must leave Trash open");
    });
    cx.simulate_keystrokes(window.into(), "escape");
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "settingsOpen", &[]), "false");
    shell(&mut cx, window, |shell, _, cx| {
        assert!(
            shell.trash_opened(cx),
            "Escape closing Settings must keep Trash open"
        );
        let places = shell.rail.as_ref().unwrap().places.read(cx);
        let lit = places.model.borrow().lit.expect("Trash row lit");
        assert_eq!(places.entries()[lit].kind, Kind::Trash);
    });
    cx.simulate_keystrokes(window.into(), "m");
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        let pane_focused = host.pane.read(cx).focus.contains_focused(window, cx);
        assert!(
            host.menu.borrow().opened(),
            "m must open the owned Trash row menu (trash pane focused: {pane_focused})"
        );
    });
}

#[test]
fn ey262_focus_view_names_the_list_stop_while_trash_holds_the_keyboard() {
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    cx.simulate_keystrokes(window.into(), "delete");
    draw(&mut cx, window);
    enter(&mut cx, window);
    settle(&mut cx, window, 2);
    assert_eq!(call(&mut cx, window, "trashFocusEmpty", &[]), "true");
    assert_eq!(call(&mut cx, window, "focusView", &[]), "list");
    assert_eq!(call(&mut cx, window, "trashFocusListing", &[]), "true");
    assert_eq!(call(&mut cx, window, "focusView", &[]), "list");
}

fn menu_inks(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
) -> Vec<(String, bool, crate::g2::context_menus::lines::Ink)> {
    cx.simulate_keystrokes(window.into(), "m");
    draw(cx, window);
    shell(cx, window, |shell, _, _| {
        let menu = shell.trash.as_ref().unwrap().menu.borrow();
        let card = menu.card.as_ref().expect("m opens the Trash menu");
        card.built
            .lines
            .iter()
            .filter(|line| !line.separator)
            .map(|line| (line.id.clone(), line.disabled, line.ink))
            .collect()
    })
}

#[test]
fn ey198_an_unavailable_empty_trash_row_drops_the_destructive_ink() {
    use crate::g2::context_menus::lines::Ink;
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    enter(&mut cx, window);
    settle(&mut cx, window, 0);
    let empty = menu_inks(&mut cx, window);
    let row = empty
        .iter()
        .find(|(id, ..)| id == "emptyTrash")
        .expect("Empty Trash… row");
    assert!(row.1, "an empty Trash disables Empty Trash…");
    assert_eq!(
        row.2,
        Ink::Plain,
        "MenuRow.qml draws an unavailable row muted, never red"
    );
}

#[test]
fn ey198_an_available_empty_trash_row_keeps_the_destructive_ink() {
    use crate::g2::context_menus::lines::Ink;
    let mut cx = real_cx();
    let window = mounted(&mut cx);
    enter(&mut cx, window);
    settle(&mut cx, window, 2);
    shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        pane.update(cx, |pane, cx| {
            pane.action("toggleSelect", false, cx);
            pane.right_click(None, 40., 200., cx);
        });
        shell.flush_trash(window, cx);
    });
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, _| {
        let menu = shell.trash.as_ref().unwrap().menu.borrow();
        let card = menu
            .card
            .as_ref()
            .expect("a right click on the blank area opens the Trash menu");
        let row = card
            .built
            .lines
            .iter()
            .find(|line| line.id == "emptyTrash")
            .expect("Empty Trash… row");
        assert!(
            !row.disabled && row.ink == Ink::Danger,
            "Empty Trash… keeps the destructive ink while it can act"
        );
    });
}
