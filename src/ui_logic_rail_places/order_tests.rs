// The rail-favorites, plugin-rail-section and tag-marks boards' section order, as typed checks.
use super::order::*;
use super::places::{home_entries, stored_entries};
use super::plugin::{parse_section, sections, Descriptor};
use super::rail::{FavouriteError, Group, Kind, RailEntry};
use super::testing::record;
use std::collections::HashMap;

fn glyph(_: &str) -> &'static str {
    "folder"
}

fn labels(sections: &[RailSection]) -> Vec<String> {
    sections
        .iter()
        .map(|s| {
            let head = match &s.heading {
                Heading::None => String::new(),
                Heading::Key(k) => (*k).to_owned(),
                Heading::Plugin(t) => t.clone(),
            };
            let rows: Vec<&str> = s
                .rows
                .iter()
                .map(|r| {
                    if r.label.is_empty() {
                        r.label_key
                    } else {
                        r.label.as_str()
                    }
                })
                .collect();
            format!("{head}:{}", rows.join(","))
        })
        .collect()
}

fn devices() -> RailSection {
    let disk = RailEntry {
        group: Group::Device,
        kind: Kind::Volume,
        label: "QEMU HARDDISK".into(),
        ..RailEntry::default()
    };
    RailSection {
        heading: Heading::Key("devices.heading"),
        rows: vec![disk],
    }
}

#[test]
fn rail_favorites_draws_places_then_favorites_then_the_built_in_tail() {
    let home = home_entries("/fx/v", "", glyph);
    let favs = stored_entries(&[record("Docs", "/fx/v/work/Docs")], "/fx/v");
    let drawn = rail_sections(
        &home,
        Shown::default(),
        favs,
        &[false; 7],
        vec![devices()],
        &[],
    );
    assert_eq!(
        labels(&drawn),
        [
            "places.heading.places:places.row.home,places.row.trash",
            "places.heading.favorites:Docs",
            "devices.heading:QEMU HARDDISK"
        ],
        "Recent and Tags ship off, and an empty section draws no heading"
    );
    assert_eq!(
        flat(&drawn).len(),
        4,
        "the cursor walks every drawn row in order"
    );
}

#[test]
fn switches_place_recent_under_home_and_hide_home_or_trash() {
    let home = home_entries("/h", "XDG_MUSIC_DIR=\"$HOME/Music\"\n", glyph);
    let shown = Shown {
        recent: true,
        ..Shown::default()
    };
    let drawn = rail_sections(&home, shown, Vec::new(), &[false; 7], Vec::new(), &[]);
    assert_eq!(
        labels(&drawn),
        ["places.heading.places:places.row.home,places.row.recent,Music,places.row.trash"]
    );
    let bare = rail_sections(
        &home,
        Shown {
            home: false,
            trash: false,
            ..Shown::default()
        },
        Vec::new(),
        &[false; 7],
        Vec::new(),
        &[],
    );
    assert!(bare.is_empty(), "no rows, no heading");
}

#[test]
fn tags_list_only_tags_in_use_when_switched_on() {
    let mut in_use = [false; 7];
    in_use[2] = true;
    in_use[5] = true;
    let off = rail_sections(&[], Shown::default(), Vec::new(), &in_use, Vec::new(), &[]);
    assert!(
        labels(&off)
            .iter()
            .all(|l| !l.starts_with("places.heading.tags")),
        "Tags ships off"
    );
    let on = rail_sections(
        &[],
        Shown {
            tags: true,
            ..Shown::default()
        },
        Vec::new(),
        &in_use,
        Vec::new(),
        &[],
    );
    let tags = on
        .iter()
        .find(|s| s.heading == Heading::Key("places.heading.tags"))
        .expect("a TAGS section");
    assert_eq!(
        tags.rows.iter().map(|r| r.tag).collect::<Vec<_>>(),
        [Some("Yellow"), Some("Purple")]
    );
    assert_eq!(tags.rows[0].label_key, "places.tag.yellow");
}

#[test]
fn plugin_sections_come_after_devices() {
    let rows = parse_section(
        "syncthing",
        r#"{"op":"section","rows":[{"id":"p","label":"Photos","state":"on","opens":"/s/Photos"},{"id":"m","label":"Music","state":"off","opens":"/s/Music"}]}"#,
    )
    .unwrap();
    let plugin = sections(
        &[Descriptor {
            plugin: "syncthing".into(),
            title: "Syncthing".into(),
            refresh_sec: 30,
            enabled: true,
            failed: false,
        }],
        &[("syncthing".into(), rows)],
    );
    let drawn = rail_sections(
        &home_entries("/fx/v", "", glyph),
        Shown::default(),
        Vec::new(),
        &[false; 7],
        vec![devices()],
        &plugin,
    );
    assert_eq!(
        labels(&drawn).last().map(String::as_str),
        Some("Syncthing:Photos,Music")
    );
    assert_eq!(
        drawn[1].heading,
        Heading::Key("devices.heading"),
        "built-ins never move"
    );
}

#[test]
fn an_inspection_status_marks_only_a_favourite_without_its_own_error() {
    let favs = stored_entries(&[record("Docs", "/d"), record("Gone", "/g")], "/h");
    let mut statuses = HashMap::new();
    statuses.insert(1, "No such file or directory".to_owned());
    statuses.insert(0, String::new());
    let marked = with_statuses(favs, &statuses);
    assert_eq!(marked[0].error, None, "an empty status is no error");
    assert_eq!(
        marked[1].error,
        Some(FavouriteError::Status("No such file or directory".into()))
    );
}
