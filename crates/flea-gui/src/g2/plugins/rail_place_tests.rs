use super::{place_model::*, rail_place_fixture::*};
use crate::{state::Shell, window::navigate::Step};
use flea::{
    jsondoc::{self, Json},
    ui_logic_plugin_settings::manifest::Invalid,
    ui_logic_rail_places::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry, Square},
    },
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

fn entries(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<RailEntry> {
    shell.read_with(cx, |s, cx| {
        s.rail.as_ref().unwrap().places.read(cx).entries()
    })
}

#[gpui::test]
fn plugin_runtime_rail_example_paints_after_devices_polls_and_disappears(cx: &mut TestAppContext) {
    let (mut fixture, shell, cx) = Fixture::open("syncthing", cx);
    fixture.pump(cx);
    shell.update(cx, |s, cx| {
        s.rail.as_ref().unwrap().devices.update(cx, |d, _| {
            d.sections = vec![RailSection {
                heading: Heading::Key("settings.places.devices"),
                rows: vec![RailEntry {
                    group: Group::Device,
                    kind: Kind::Disk,
                    label: "Fixture Disk".into(),
                    path: "/fixture".into(),
                    ..Default::default()
                }],
            }]
        });
        s.rail_tail(cx);
    });
    frame(cx);
    let rows = entries(&shell, cx);
    let photos = rows
        .iter()
        .position(|r| r.label == "Photos")
        .expect("repository example contributes rail rows");
    assert!(rows.iter().position(|r| r.label == "Fixture Disk").unwrap() < photos);
    assert_eq!(rows[photos].square, Square::On);
    assert_eq!(rows[photos + 2].square, Square::Off);
    shell.read_with(cx, |s, cx| {
        let producer = s.rail.as_ref().unwrap().places.read(cx);
        let laid = producer.laid.borrow();
        let row = laid
            .row(photos)
            .expect("real rail lays out the contributed row");
        assert_eq!(row.rect.height, 29.);
        assert_eq!(row.label_rect.x, 38.);
        assert!(producer
            .sections
            .iter()
            .any(|s| s.heading == Heading::Plugin("Syncthing".into())));
    });
    let before = fixture.count("section");
    cx.executor().advance_clock(Duration::from_secs(5));
    fixture.pump(cx);
    assert_eq!(fixture.count("section"), before + 1);
    fixture.plugin.enabled = false;
    fixture.publish();
    fixture.pump(cx);
    frame(cx);
    assert!(!entries(&shell, cx).iter().any(|r| r.group == Group::Plugin));
    fixture.plugin.enabled = true;
    fixture.publish();
    fixture.pump(cx);
    frame(cx);
    assert!(entries(&shell, cx).iter().any(|r| r.label == "Photos"));
    fixture.plugin.failure = Some(Invalid::new("start", "fixture stopped"));
    fixture.publish();
    fixture.pump(cx);
    frame(cx);
    assert!(!entries(&shell, cx).iter().any(|r| r.group == Group::Plugin));
}

#[gpui::test]
fn plugin_runtime_place_example_mounts_navigates_renames_and_revokes_rows(cx: &mut TestAppContext) {
    let (mut fixture, shell, cx) = Fixture::open("virtual-place", cx);
    fixture.pump(cx);
    frame(cx);
    let row = entries(&shell, cx)
        .into_iter()
        .find(|r| r.label == "Example Files")
        .expect("place has one rail row");
    cx.update(|window, cx| shell.update(cx, |s, cx| s.plugin_rail_activate(0, &row, window, cx)));
    fixture.pump(cx);
    frame(cx);
    shell.read_with(cx, |s, _| {
        let pane = &s.panes[0].as_ref().unwrap().pane;
        assert_eq!(pane.path(), "plugin://example.virtual-place/");
        assert_eq!(pane.held.total, 2);
        let names: Vec<_> = pane.held.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            ["notes.txt", "Photos"],
            "the normal name order applies to plugin rows"
        );
    });
    shell.update(cx, |s, _| s.panes[0].as_mut().unwrap().pane.cursor = 1);
    navigate(&shell, Step::OpenCursor, cx);
    fixture.pump(cx);
    frame(cx);
    shell.read_with(cx, |s, _| {
        assert_eq!(
            s.panes[0].as_ref().unwrap().pane.held.rows[0].name,
            "lake.png"
        )
    });
    navigate(&shell, Step::Back, cx);
    fixture.pump(cx);
    frame(cx);
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.panes[0].as_mut().unwrap().pane.cursor = 0;
            s.rename_open(0, 0, window, cx);
            let edit = s.rename.edit.as_mut().unwrap();
            edit.entry.text = "draft.txt".into();
            edit.entry.marked = None;
            s.rename_commit(cx);
        })
    });
    fixture.pump(cx);
    frame(cx);
    fixture.pump(cx);
    frame(cx);
    assert!(fixture
        .calls
        .iter()
        .any(|r| text(r, "op") == "rename" && text(r, "path") == "/notes.txt"));
    shell.read_with(cx, |s, _| {
        assert!(s.rename.edit.is_none());
        assert!(s.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .held
            .rows
            .iter()
            .any(|r| r.name == "draft.txt"));
    });
    fixture.plugin.enabled = false;
    fixture.publish();
    fixture.pump(cx);
    frame(cx);
    assert!(!entries(&shell, cx).iter().any(|r| r.group == Group::Plugin));
    shell.read_with(cx, |s, _| {
        assert_eq!(s.panes[0].as_ref().unwrap().pane.held.total, 0)
    });
}

#[gpui::test]
fn plugin_runtime_local_place_opens_the_local_folder_without_list_runtime(cx: &mut TestAppContext) {
    let (mut fixture, shell, cx) = Fixture::open("virtual-place", cx);
    fixture
        .plugin
        .manifest
        .blocks
        .iter_mut()
        .find(|(k, _)| k == "place")
        .unwrap()
        .1 =
        jsondoc::parse(r#"{"label":"Fixture Folder","path":"/tmp/flea-local-place"}"#).unwrap();
    fixture.publish();
    fixture.pump(cx);
    frame(cx);
    let row = entries(&shell, cx)
        .into_iter()
        .find(|r| r.label == "Fixture Folder")
        .unwrap();
    cx.update(|window, cx| shell.update(cx, |s, cx| s.plugin_rail_activate(0, &row, window, cx)));
    fixture.pump(cx);
    shell.read_with(cx, |s, _| {
        assert_eq!(
            s.panes[0].as_ref().unwrap().pane.place(),
            "/tmp/flea-local-place"
        )
    });
    assert_eq!(fixture.count("list"), 0);
}

#[gpui::test]
fn plugin_runtime_place_operation_cards_route_copy_move_and_confirmed_delete(
    cx: &mut TestAppContext,
) {
    use flea::ui_logic_file_ops::collide::{text as encoded, texts, Request};
    let (mut fixture, shell, cx) = Fixture::open("virtual-place", cx);
    navigate(
        &shell,
        Step::OpenPlace("plugin://example.virtual-place/".into()),
        cx,
    );
    fixture.pump(cx);
    frame(cx);
    let source = "plugin://example.virtual-place/notes.txt";
    let dest = "plugin://example.virtual-place/Photos";
    let request = Request::new()
        .with("c", &encoded("transfer"))
        .with("op", &encoded("copy"))
        .with("paths", &texts(&[source]))
        .with("dest", &encoded(dest));
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            assert!(s.ask_operation(0, &request, &[source], false, window, cx))
        })
    });
    fixture.pump(cx);
    frame(cx);
    fixture.pump(cx);
    assert_eq!(
        fixture.count("copy"),
        1,
        "the operation card sends through the shared host"
    );
    navigate(&shell, Step::OpenPlace(dest.into()), cx);
    fixture.pump(cx);
    frame(cx);
    shell.read_with(cx, |s, _| {
        assert!(s.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .held
            .rows
            .iter()
            .any(|r| r.name == "notes.txt"))
    });
    let source = "plugin://example.virtual-place/Photos/lake.png";
    let request = Request::new()
        .with("c", &encoded("transfer"))
        .with("op", &encoded("move"))
        .with("paths", &texts(&[source]))
        .with("dest", &encoded("plugin://example.virtual-place/"));
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            assert!(s.ask_operation(0, &request, &[source], false, window, cx))
        })
    });
    fixture.pump(cx);
    frame(cx);
    fixture.pump(cx);
    assert_eq!(fixture.count("move"), 1);
    navigate(&shell, Step::Up, cx);
    fixture.pump(cx);
    frame(cx);
    shell.update(cx, |s, _| {
        let p = s.panes[0].as_mut().unwrap();
        p.pane.cursor = p
            .pane
            .held
            .rows
            .iter()
            .position(|r| r.name == "lake.png")
            .unwrap();
        p.selected = vec![p.pane.cursor];
    });
    cx.update(|window, cx| {
        shell.update(cx, |s, cx| {
            s.dispatch(
                crate::window::dispatch::Order::Action(crate::window::input::ActionEvent::bare(
                    "deletePermanently",
                    flea::keys::Context::Listing,
                )),
                window,
                cx,
            )
        })
    });
    fixture.pump(cx);
    frame(cx);
    assert!(
        shell.read_with(cx, |s, cx| s.card_holds_keyboard(cx)),
        "the existing Delete permanently card reviews the plugin selection"
    );
    cx.simulate_keystrokes("tab enter");
    fixture.pump(cx);
    frame(cx);
    fixture.pump(cx);
    assert_eq!(fixture.count("delete"), 1);
    assert!(fixture.calls.iter().any(|r| text(r, "op") == "delete"
        && r.get("paths") == Some(&Json::Arr(vec![string("/lake.png")]))));
}
