// Typed checks for the plugin rail-section answer, from the plugin-rail-section board and the plugin reference.
use super::plugin::*;
use super::rail::{Group, Identity, Kind, RailEntry, Square};

fn row_ids(rows: &[RailEntry]) -> Vec<&str> {
    rows.iter().map(|r| r.id.as_str()).collect()
}

#[test]
fn a_section_answer_reads_rows_and_drops_bad_ones_alone() {
    let line = r#"{"op":"section","rows":[
        {"id":"nas","label":" nas ","glyph":"server","state":"on","opens":"sftp://nas/"},
        {"id":"box","label":"box","glyph":"rocket","state":"off","opens":"/srv/box"},
        {"id":"nas","label":"again","opens":"/x"},
        {"id":"","label":"blank id","opens":"/x"},
        {"id":"ws","label":"  ","opens":"/x"},
        {"id":"rel","label":"relative","opens":"srv/x"},
        {"id":"ctl","label":"a\u0007b","opens":"/x"},
        {"id":"bad","label":"bad scheme","opens":"1x://y"},
        {"id":"plain","label":"plain","opens":"smb://host/share"}
    ]}"#;
    let rows = parse_section("tailnet", line).expect("a section answer");
    assert_eq!(
        row_ids(&rows),
        ["nas", "box", "plain"],
        "duplicate, blank, relative, control and bad scheme rows drop"
    );
    assert_eq!(
        (rows[0].label.as_str(), rows[0].glyph, rows[0].square),
        ("nas", "server", Square::On)
    );
    assert_eq!(
        (rows[0].uri.as_str(), rows[0].path.as_str()),
        ("sftp://nas/", ""),
        "a remote opens is a uri"
    );
    assert_eq!(
        (rows[1].glyph, rows[1].square, rows[1].path.as_str()),
        ("folder", Square::Off, "/srv/box"),
        "an unknown glyph draws the folder"
    );
    assert_eq!(rows[2].square, Square::None, "no state draws no square");
    assert!(rows
        .iter()
        .all(|r| r.group == Group::Plugin && r.kind == Kind::PluginRow && r.section == "tailnet"));
    assert_eq!(
        rows[0].identity(),
        Identity::Place(Group::Plugin, Kind::PluginRow, "tailnet\nnas".into()),
        "a row is keyed by section and id"
    );
}

#[test]
fn a_malformed_answer_keeps_the_last_rows() {
    for line in [
        "",
        "not json",
        r#"{"op":"other","rows":[]}"#,
        r#"{"op":"section"}"#,
        r#"{"op":"section","rows":{}}"#,
    ] {
        assert_eq!(parse_section("p", line), None, "{line:?} is no answer");
    }
    assert_eq!(
        parse_section("p", r#"{"op":"section","rows":[]}"#),
        Some(Vec::new()),
        "an empty list is an answer"
    );
    let many: Vec<String> = (0..ROWS_MAX + 5)
        .map(|i| format!(r#"{{"id":"r{i}","label":"r{i}","opens":"/r{i}"}}"#))
        .collect();
    let line = format!(r#"{{"op":"section","rows":[{}]}}"#, many.join(","));
    assert_eq!(
        parse_section("p", &line).map(|r| r.len()),
        Some(ROWS_MAX),
        "a long answer keeps its first rows"
    );
}

#[test]
fn refresh_is_clamped() {
    assert_eq!(clamp_refresh(Some(1.0)), REFRESH_MIN);
    assert_eq!(clamp_refresh(Some(30.4)), 30);
    assert_eq!(clamp_refresh(Some(1e9)), REFRESH_MAX);
    assert_eq!(
        clamp_refresh(None),
        REFRESH_DEFAULT,
        "an absent refreshSec takes the stated default, not the floor"
    );
    assert_eq!(
        clamp_refresh(Some(f64::NAN)),
        REFRESH_DEFAULT,
        "so does one that is not a number"
    );
}

fn descriptor(plugin: &str, title: &str, enabled: bool, failed: bool) -> Descriptor {
    Descriptor {
        plugin: plugin.into(),
        title: title.into(),
        refresh_sec: 30,
        enabled,
        failed,
    }
}

#[test]
fn plugin_sections_follow_every_built_in_row() {
    let one = parse_section(
        "a",
        r#"{"op":"section","rows":[{"id":"x","label":"x","opens":"/x"}]}"#,
    )
    .unwrap();
    let descriptors = [
        descriptor("a", "Tailnet", true, false),
        descriptor("b", "Off", false, false),
        descriptor("c", "Broken", true, true),
        descriptor("d", "Empty", true, false),
        descriptor("e", "Silent", true, false),
    ];
    let answers = vec![
        ("a".to_owned(), one.clone()),
        ("b".into(), one.clone()),
        ("c".into(), one.clone()),
        ("d".into(), Vec::new()),
    ];
    let drawn = sections(&descriptors, &answers);
    assert_eq!(
        drawn.len(),
        1,
        "disabled, failed, empty and silent plugins draw no heading"
    );
    assert_eq!(
        drawn[0].heading, "Tailnet",
        "the title stays as resolved; the rail sets every heading in capitals"
    );
    let home = RailEntry {
        group: Group::Home,
        kind: Kind::Home,
        label: "Home".into(),
        ..RailEntry::default()
    };
    let rail = with_plugins(std::slice::from_ref(&home), &drawn);
    assert_eq!(rail.len(), 2);
    assert_eq!(
        (rail[0].group, rail[1].group),
        (Group::Home, Group::Plugin),
        "built-ins never move"
    );
}
