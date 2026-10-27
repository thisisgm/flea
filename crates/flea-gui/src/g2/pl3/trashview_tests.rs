//! Trash rail activation through Root, including draw-time registration and the visible view owners.
use super::join_tests::{answer, draw, root, shell};
use crate::{list::bench::real_cx, window::root::Root};
use flea::{
    protocol::{Reply, RowFacts},
    ui_logic_pl4::{order::RailSection, rail::Kind},
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
                    heading: flea::ui_logic_pl4::order::Heading::Key("places.heading.places"),
                    rows: vec![flea::ui_logic_pl4::order::trash_entry()],
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
