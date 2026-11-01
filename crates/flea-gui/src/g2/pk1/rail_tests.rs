//! #205: the live chooser mounts picker-open's places rail, one flat list with no headings, and its rows open their places.
use super::process::Line;
use super::rail::{entries, WIDTH};
use super::tests::fixture::{fixture, Fixture};
use flea::jsondoc;
use flea::ui_logic_pl4::{
    order::{Heading, RailSection},
    rail::{Group, Kind, RailEntry},
};
use gpui::{Modifiers, TestAppContext};

fn drive(label: &str, kind: Kind, mounted: bool) -> RailEntry {
    RailEntry {
        group: Group::Device,
        kind,
        label: label.into(),
        path: format!("/run/media/{label}"),
        device: format!("/dev/{label}"),
        mounted: Some(mounted),
        glyph: "drive",
        ..RailEntry::default()
    }
}

#[test]
fn the_rail_is_recent_then_favourites_then_home_then_drives_and_phones() {
    let favourites = jsondoc::parse(
        r#"[{"label":"Work","path":"~/Work"},{"label":"NAS","path":"smb://nas/share"},{"label":"Notes","path":"file:///srv/notes"}]"#,
    )
    .unwrap();
    let devices = [RailSection {
        heading: Heading::Key("settings.places.devices"),
        rows: vec![
            drive("usb", Kind::Volume, true),
            drive("phone", Kind::Phone, true),
        ],
    }];
    let rows = entries(true, favourites.as_array().unwrap(), "/h", "", &devices);
    let paths: Vec<&str> = rows.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "flea:recent",
            "/h/Work",
            "smb://nas/share",
            "file:///srv/notes",
            "/h",
            "/run/media/usb",
            "/run/media/phone"
        ],
        "one list in PickerPlaces.qml's order, including remote favourites and phones"
    );
    let saving = entries(false, &[], "/h", "", &[]);
    assert_eq!(
        saving.iter().map(|row| row.kind).collect::<Vec<_>>(),
        [Kind::Home],
        "a save has no Recent to write into"
    );
}

fn listed(f: &mut Fixture, path: &str) {
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":1,"path":"{path}"}}"#
    )));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}]}"#.into()));
    f.cx.run_until_parked();
}

#[gpui::test]
fn the_open_chooser_draws_one_flat_rail_beside_the_listing(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    listed(&mut f, "/d");
    assert_eq!(
        f.read(|picker| picker
            .rail
            .labels
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()),
        ["Recent", "Home"],
        "picker-open's rows: Recent, then Home"
    );
    let rail =
        f.cx.debug_bounds("picker-rail")
            .expect("the rail is mounted");
    let body =
        f.cx.debug_bounds("picker-grid-view")
            .expect("the path strip is drawn");
    assert_eq!(
        f32::from(rail.size.width),
        WIDTH + 1.,
        "Theme.space(172) and its rule"
    );
    assert!(
        rail.top() > body.bottom() - gpui::px(1.),
        "the rail stands under the path strip"
    );
    let first = f.cx.debug_bounds("picker-rail-row-0").unwrap();
    let second = f.cx.debug_bounds("picker-rail-row-1").unwrap();
    assert_eq!(
        f32::from(first.top() - rail.top()),
        7.,
        "rows start 7 px under the strip"
    );
    assert_eq!(f32::from(first.size.height), 29.);
    assert_eq!(
        second.top(),
        first.bottom(),
        "no heading or gap between the rows: one flat list"
    );
    assert_eq!(first.left(), rail.left());
}

#[gpui::test]
fn a_save_chooser_offers_no_recent(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save","name":"a.txt","folder":"/d"}"#, None);
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker
            .rail
            .labels
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()),
        ["Home"]
    );
}

#[gpui::test]
fn a_press_on_home_lists_home_and_leaves_the_keyboard_on_the_files(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    listed(&mut f, "/d");
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    let home = f.cx.debug_bounds("picker-rail-row-1").unwrap().center();
    f.cx.simulate_click(home, Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/home/test"
    );
    assert!(
        f.listing
            .sent()
            .last()
            .unwrap()
            .contains(r#""path":"/home/test""#),
        "the listing child is asked for Home"
    );
    let picker = f.picker.clone();
    assert!(
        f.cx.update(|window, cx| {
            let picker = picker.read(cx);
            picker.focus.is_focused(window) && !picker.rail.focus.is_focused(window)
        }),
        "a press leaves the keyboard on the files, as picker-recent-list draws"
    );
}

#[gpui::test]
fn recent_lists_the_history_paths_under_the_base_and_opens_a_folder_row(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    listed(&mut f, "/d");
    let history = f.root.0.join("recently-used.xbel");
    std::fs::write(
        &history,
        r#"<?xml version="1.0"?><xbel version="1.0"><bookmark href="file:///srv/a.png" added="2026-10-01T10:00:00Z" modified="2026-10-01T10:00:00Z" visited="2026-10-01T10:00:00Z"/><bookmark href="file:///srv/docs" added="2026-10-02T10:00:00Z" modified="2026-10-02T10:00:00Z" visited="2026-10-02T10:00:00Z"/></xbel>"#,
    )
    .unwrap();
    f.update(|picker, cx| {
        picker.rail.history = history.to_string_lossy().into_owned();
        picker.rail_choose(0, cx);
    });
    f.settle(|f| {
        f.listing
            .sent()
            .last()
            .is_some_and(|line| line.contains("listpaths"))
    });
    let line = f.listing.sent().last().cloned().unwrap();
    assert!(
        line.starts_with(r#"{"c":"listpaths","paths":["#)
            && line.contains(r#""/srv/a.png""#)
            && line.contains(r#""/srv/docs""#),
        "{line}"
    );
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":1,"path":"/"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"srv/docs","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| (picker.listing.state, picker.listing.names.clone())),
        ("ready", vec!["srv/docs".to_owned()]),
        "the listing under the base \"/\" is Recent's own answer"
    );
    f.update(|picker, cx| picker.activate_row(0, cx));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/srv/docs",
        "a Recent folder row opens its own path"
    );
}
