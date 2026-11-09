//! U-DRAG-DROP: Recent's rail row, main-window columns and Used sort route.
use super::mounted_tests::{drain, frame, open, Root};
use super::rail_drop_tests::rail;
use super::u_drag_drop_tests::{open_listed, FIRST_LISTING};
use crate::list::bench;
use gpui::{px, TestAppContext};

#[test]
fn ey267_the_recent_rail_row_stays_lit_while_recent_is_open() {
    use flea::ui_logic_rail_places::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
        recent::Visit,
    };
    let root = Root::new("ey267");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    rail(&mut cx, handle, &root.src());
    let entry = |label: &str, path: &str, group, kind| RailEntry {
        group,
        kind,
        label: label.into(),
        path: path.into(),
        glyph: "house",
        ..RailEntry::default()
    };
    let src = root.src();
    handle
        .update(&mut cx, |shell, _, cx| {
            let places = shell.rail.as_ref().unwrap().places.clone();
            places.update(cx, |producer, cx| {
                producer.replace_sections(
                    vec![RailSection {
                        heading: Heading::Key("places.heading.places"),
                        rows: vec![
                            entry("Home", &src, Group::Home, Kind::Home),
                            entry("Recent", "", Group::Home, Kind::Recent),
                        ],
                    }],
                    cx,
                )
            });
        })
        .unwrap();
    frame(&mut cx, handle);
    let lit = |cx: &mut TestAppContext| {
        handle
            .update(cx, |shell, _, cx| {
                let producer = shell.rail.as_ref().unwrap().places.read(cx);
                let lit = producer.model.borrow().lit;
                lit
            })
            .unwrap()
    };
    assert_eq!(lit(&mut cx), Some(0), "the folder lights its place");
    let visit = Visit {
        path: format!("{src}/b.txt"),
        stamp: "2026-10-31T10:00:00Z".into(),
    };
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.recent_open(0, vec![visit], window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(
        lit(&mut cx),
        Some(1),
        "Recent lights its own row while it stands"
    );
}

fn recent_main_fixture(
    cx: &mut TestAppContext,
    root: &Root,
) -> (
    gpui::WindowHandle<crate::state::Shell>,
    impl FnMut() -> Option<String>,
) {
    use flea::ui_logic_rail_places::recent::Visit;
    let (handle, lines) = open_listed(cx, root.src());
    let visits = vec![
        Visit {
            path: format!("{}/b.txt", root.src()),
            stamp: "2026-11-01T10:00:00Z".into(),
        },
        Visit {
            path: format!("{}/alpha/a.txt", root.src()),
            stamp: "2026-10-30T10:00:00Z".into(),
        },
    ];
    handle
        .update(cx, |shell, window, cx| {
            // A non-default folder order, so leaving Recent must restore it rather than reset it.
            shell.panes[0].as_mut().unwrap().nav.order = crate::navigation::folder_state::Order {
                key: "size",
                desc: true,
            };
            shell.recent_open(0, visits.clone(), window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            let rows = visits
                .iter()
                .enumerate()
                .map(|(index, visit)| {
                    let mut row = bench::fixture("uniform").remove(0);
                    row.name = visit.path.trim_start_matches('/').into();
                    row.dir = false;
                    row.mtime = if index == 0 { 1 } else { 2_000_000_000 };
                    row
                })
                .collect();
            host.pane
                .receive(flea::protocol::Reply::Listed {
                    path: "/".into(),
                    total: 2,
                })
                .unwrap();
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    // Recent's history listing is the one after the folder's first.
                    listing: FIRST_LISTING + 1,
                    rows,
                })
                .unwrap();
            host.changed(&shell.locale);
            host.nav
                .after_reply(&mut host.pane, &root.src(), &mut host.selected);
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    (handle, lines)
}

#[test]
fn ey267_recent_main_columns_show_leaf_location_and_used_time() {
    let root = Root::new("ey267columns");
    let mut cx = bench::real_cx();
    let (handle, _lines) = recent_main_fixture(&mut cx, &root);
    handle
        .update(&mut cx, |shell, window, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            assert_eq!(list.drawn.layout.names(), "name,location,size,date");
            assert!(
                list.header.painted_titles.contains("Location"),
                "{}",
                list.header.painted_titles
            );
            assert!(
                list.header.painted_titles.contains("Used"),
                "{}",
                list.header.painted_titles
            );
            let first = list
                .visible()
                .iter()
                .find(|shown| shown.index == 0)
                .unwrap()
                .prepared
                .as_ref()
                .unwrap();
            assert_eq!(
                first.label, "b.txt",
                "Recent prints the leaf, not the absolute path"
            );
            let used =
                flea::ui_logic_rail_places::recent::stamp_seconds("2026-11-01T10:00:00Z").unwrap();
            assert_eq!(
                first.date_text,
                crate::row::format::date_text(used, &list.view.words)
            );
            assert!(
                first.mode.line().width > px(80.),
                "the fixed Location slot contains the parent path"
            );
            let ellipsis = window.text_system().layout_line(
                "\u{2026}",
                first.mode.line().font_size,
                &[gpui::TextRun {
                    len: 3,
                    font: list.typeface.font.clone(),
                    color: Default::default(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            assert_eq!(
                first.mode.line().runs[0].glyphs[0].id,
                ellipsis.runs[0].glyphs[0].id,
                "Recent Location keeps the path tail, as Row.qml's ElideLeft"
            );
            use crate::list::{columns::Column, header};
            assert_eq!(
                header::cell_text(Column::Date, &host.pane.held, 0, &list.view),
                Some(first.date_text.clone())
            );
            assert_eq!(
                header::fitted_width(Column::Mode, &host.pane.held, &list.view, 150., |_| 999.),
                None,
                "fixed Location never writes a normal Mode width"
            );
            let layout = list.header.drawn.layout;
            assert!(layout.recent_location, "the header keeps Recent's layout");
            assert!(
                header::handles(&layout, &list.drawn.metrics, &list.view)
                    .all(|handle| handle.column != Column::Mode),
                "fixed Location has no resize handle"
            );
        })
        .unwrap();
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.leave_recent();
            assert!(
                host.list.borrow().view.place_recent.is_none(),
                "leaving Recent hands back the folder columns"
            );
        })
        .unwrap();
}

#[test]
fn ey267_used_header_preserves_the_oracle_sort_route_and_hands_back_the_folder_order() {
    let root = Root::new("ey267sort");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = recent_main_fixture(&mut cx, &root);
    drain(&mut lines);
    handle
        .update(&mut cx, |shell, window, cx| {
            // The step a Used header tap sends, list/join.rs.
            shell
                .step(
                    0,
                    crate::window::navigate::Step::SortColumn("mtime".into()),
                    window,
                    cx,
                )
                .unwrap();
        })
        .unwrap();
    let sent = drain(&mut lines);
    assert!(
        sent.iter()
            .any(|line| line.contains(r#""c":"sort""#) && line.contains(r#""by":"mtime""#)),
        "Used retains v0.3.12's backend sort route: {sent:?}"
    );
    assert!(
        sent.iter().all(|line| !line.contains(r#""c":"listpaths""#)),
        "a sort does not re-list the history: {sent:?}"
    );
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.leave_recent();
            assert_eq!(host.nav.order.key, "size");
            assert!(host.nav.order.desc);
        })
        .unwrap();
}

#[test]
fn ey267_used_header_deferred_by_rename_keeps_the_oracle_sort_route() {
    let root = Root::new("ey267gate");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = recent_main_fixture(&mut cx, &root);
    drain(&mut lines);
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().nav.gate.editing = true;
            assert_eq!(
                shell.step(
                    0,
                    crate::window::navigate::Step::SortColumn("mtime".into()),
                    window,
                    cx
                ),
                Ok(false)
            );
        })
        .unwrap();
    assert!(
        drain(&mut lines).is_empty(),
        "rename defers the history sort"
    );
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.set_gate(0, crate::navigation::watch::Gate::default(), window, cx);
        })
        .unwrap();
    let sent = drain(&mut lines);
    assert!(
        sent.iter()
            .any(|line| line.contains(r#""c":"sort""#) && line.contains(r#""by":"mtime""#)),
        "{sent:?}"
    );
    assert!(
        sent.iter().all(|line| !line.contains(r#""c":"listpaths""#)),
        "Used follows the oracle sort route after rename: {sent:?}"
    );
}
