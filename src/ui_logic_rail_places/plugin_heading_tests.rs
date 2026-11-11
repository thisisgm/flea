// Plugin rail headings from board 0402-plugin-rail-mark: a plugin's own title, merged by title, else one PLUGINS.
use super::order::{Heading, RailSection};
use super::plugin::{plugin_headings, PLUGINS_HEADING};
use super::rail::{Group, Kind, RailEntry};
use crate::ui_logic_plugin_settings::manifest;

fn row(plugin: &str, label: &str) -> RailEntry {
    RailEntry {
        group: Group::Plugin,
        kind: Kind::PluginRow,
        label: label.into(),
        section: plugin.into(),
        id: label.to_lowercase(),
        ..RailEntry::default()
    }
}

fn built_in(key: &'static str, label: &str) -> RailSection {
    RailSection {
        heading: Heading::Key(key),
        rows: vec![RailEntry {
            label: label.into(),
            ..RailEntry::default()
        }],
    }
}

fn plugin(title: &str, rows: &[(&str, &str)]) -> RailSection {
    RailSection {
        heading: Heading::Plugin(title.into()),
        rows: rows.iter().map(|(p, label)| row(p, label)).collect(),
    }
}

// The rail's English headings in capitals, as content::heading_text sets them.
fn english(heading: &Heading) -> String {
    match heading {
        Heading::None => String::new(),
        Heading::Key("places.heading.places") => "PLACES".into(),
        Heading::Key("places.heading.favorites") => "FAVORITES".into(),
        Heading::Key("places.heading.tags") => "TAGS".into(),
        Heading::Key("places.heading.network" | "settings.places.network") => "NETWORK".into(),
        Heading::Key("settings.places.devices") => "DEVICES".into(),
        Heading::Key(PLUGINS_HEADING) => "PLUGINS".into(),
        Heading::Key(other) => panic!("unexpected key {other}"),
        Heading::Plugin(title) => title.to_uppercase(),
    }
}

fn drawn(sections: &[RailSection]) -> Vec<String> {
    sections
        .iter()
        .map(|s| {
            let labels: Vec<_> = s.rows.iter().map(|r| r.label.as_str()).collect();
            format!("{}:{}", english(&s.heading), labels.join(","))
        })
        .collect()
}

#[test]
fn the_board_sets_titled_sections_then_one_plugins_heading_last() {
    let sections = vec![
        built_in("places.heading.places", "Home"),
        built_in("places.heading.network", "NAS"),
        built_in("settings.places.devices", "QEMU HARDDISK"),
        plugin("", &[("io.rober.mega", "MEGA")]),
        plugin(
            "Syncthing",
            &[
                ("example.syncthing", "Photos"),
                ("example.syncthing", "Notes"),
            ],
        ),
        plugin("", &[("example.backups", "Backups")]),
    ];
    assert_eq!(
        drawn(&plugin_headings(sections, english)),
        [
            "PLACES:Home",
            "NETWORK:NAS",
            "DEVICES:QEMU HARDDISK",
            "SYNCTHING:Photos,Notes",
            "PLUGINS:MEGA,Backups",
        ],
        "an untitled place row stands under PLUGINS after every titled section, in plugin order"
    );
}

#[test]
fn one_title_from_two_plugins_shares_one_heading_in_plugin_order() {
    let sections = vec![
        plugin("Cloud", &[("a.one", "Alpha")]),
        plugin("Syncthing", &[("b.two", "Photos")]),
        plugin("cloud", &[("c.three", "Gamma")]),
    ];
    let set = plugin_headings(sections, english);
    assert_eq!(drawn(&set), ["CLOUD:Alpha,Gamma", "SYNCTHING:Photos"]);
    let sections: Vec<_> = set[0].rows.iter().map(|r| r.section.as_str()).collect();
    assert_eq!(
        sections,
        ["a.one", "c.three"],
        "each row keeps its own plugin"
    );
}

#[test]
fn a_title_that_reads_as_a_built_in_heading_or_plugins_goes_under_plugins() {
    let sections = vec![
        built_in("places.heading.favorites", "Docs"),
        built_in("settings.places.devices", "Disk"),
        plugin("Devices", &[("a.one", "Phone")]),
        plugin("favorites", &[("b.two", "Starred")]),
        plugin(" Plugins ", &[("c.three", "Extra")]),
        plugin("Network", &[("d.four", "Share")]),
        plugin("   ", &[("e.five", "Blank")]),
    ];
    assert_eq!(
        drawn(&plugin_headings(sections, english)),
        [
            "FAVORITES:Docs",
            "DEVICES:Disk",
            "PLUGINS:Phone,Starred,Extra,Share,Blank"
        ],
        "no built-in section ever holds a plugin row or moves"
    );
}

#[test]
fn plugins_draws_no_heading_without_a_row() {
    let sections = vec![
        built_in("places.heading.places", "Home"),
        plugin("Syncthing", &[("a.one", "Photos")]),
        plugin("", &[]),
    ];
    assert_eq!(
        drawn(&plugin_headings(sections, english)),
        ["PLACES:Home", "SYNCTHING:Photos"]
    );
}

#[test]
fn a_place_title_is_read_like_a_rail_section_title() {
    let parse = |kind: &str, key: &str, block: &str| {
        manifest::parse(
            &format!(
                r#"{{"schemaVersion":1,"id":"io.rober.mega","name":"MEGA","version":"1","author":"r","description":"d",
                "kinds":["{kind}"],"entryPoints":{{"run":"run"}},"{key}":{block}}}"#
            ),
            false,
        )
    };
    let place = |block: &str| parse("place", "place", block);
    assert!(place(r#"{"label":"MEGA","glyph":"cloud","path":"/m"}"#).is_ok());
    assert!(place(r#"{"label":"MEGA","title":"Cloud"}"#).is_ok());
    for bad in [
        r#"{"label":"MEGA","title":""}"#,
        r#"{"label":"MEGA","title":"  "}"#,
        r#"{"label":"MEGA","title":7}"#,
        r#"{"label":"MEGA","title":"a\u0007b"}"#,
    ] {
        assert_eq!(
            place(bad).unwrap_err(),
            manifest::Invalid::new("block", "title"),
            "{bad}"
        );
    }
    assert!(parse("rail-section", "railSection", "{}").is_ok());
    assert!(parse("rail-section", "railSection", r#"{"title":"Cloud"}"#).is_ok());
    for bad in [r#"{"title":""}"#, r#"{"title":7}"#] {
        assert_eq!(
            parse("rail-section", "railSection", bad).unwrap_err(),
            manifest::Invalid::new("block", "title")
        );
    }
}
