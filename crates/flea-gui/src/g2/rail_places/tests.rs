// The rail content, menus and service against the plugin-rail-section, rail-favorites, tag-marks and tags-menu boards.
use super::content::{build, ink, theme_tags, Provider, RowMark, TagMark};
use super::menus::{favourite_menu, tags_menu, Act, Check, MenuRow};
use super::seams::ThemeRoles;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_rail_places::order::{rail_sections, Heading, RailSection, Shown};
use flea::ui_logic_rail_places::places::{home_entries, stored_entries};
use flea::ui_logic_rail_places::plugin::{parse_section, sections, Descriptor};
use flea::ui_logic_rail_places::rail::{Group, Kind, RailEntry, Square};
use flea::ui_logic_rail_places::tags::State;
use std::path::PathBuf;

fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent"),
        language_file: PathBuf::from("/nonexistent"),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

fn bench() -> ThemeRoles {
    let roles = [
        ("red", "#d65a4f"),
        ("orange", "#e19763"),
        ("yellow", "#dc9c36"),
        ("green", "#79ab55"),
        ("blue", "#509467"),
        ("magenta", "#8f7351"),
        ("dark_foreground", "#907e64"),
    ];
    ThemeRoles {
        roles: roles.map(|(r, h)| (r.to_owned(), h.to_owned())).to_vec(),
        background: "#1c120d".into(),
        surface: "#150d0a".into(),
    }
}

fn record(label: &str, path: &str) -> flea::jsondoc::Json {
    use flea::jsondoc::Json;
    Json::Obj(vec![
        ("label".into(), Json::Str(label.into())),
        ("path".into(), Json::Str(path.into())),
    ])
}

fn tail() -> Vec<RailSection> {
    let nas = RailEntry {
        group: Group::Network,
        kind: Kind::Share,
        label: "NAS".into(),
        square: Square::Off,
        ..RailEntry::default()
    };
    let disk = RailEntry {
        group: Group::Device,
        kind: Kind::Volume,
        label: "QEMU HARDDISK".into(),
        square: Square::On,
        ..RailEntry::default()
    };
    vec![
        RailSection {
            heading: Heading::Plugin("Network".into()),
            rows: vec![nas],
        },
        RailSection {
            heading: Heading::Plugin("Devices".into()),
            rows: vec![disk],
        },
    ]
}

fn texts(drawn: &[super::content::DrawnSection]) -> Vec<String> {
    drawn
        .iter()
        .map(|s| {
            format!(
                "{}:{}",
                s.heading,
                s.rows
                    .iter()
                    .map(|r| r.label.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect()
}

#[test]
fn rail_favorites_board_sections_read_in_capitals() {
    let c = english();
    let favs = stored_entries(&[record("Docs", "/fx/v/work/Docs")], "/fx/v");
    let s = rail_sections(
        &home_entries("/fx/v", "", |_| "folder"),
        Shown::default(),
        favs,
        &[false; 7],
        tail(),
        &[],
    );
    let drawn = build(&c, &s, &theme_tags(&bench()));
    assert_eq!(
        texts(&drawn),
        [
            "PLACES:Home,Trash",
            "FAVORITES:Docs",
            "NETWORK:NAS",
            "DEVICES:QEMU HARDDISK"
        ]
    );
    assert_eq!(drawn[3].rows[0].mark, RowMark::Square(Square::On));
}

#[test]
fn plugin_rail_section_board_adds_syncthing_after_devices() {
    let c = english();
    let line = r#"{"op":"section","rows":[{"id":"p","label":"Photos","glyph":"folder","state":"on","opens":"/s/Photos"},{"id":"n","label":"Notes","state":"on","opens":"/s/Notes"},{"id":"m","label":"Music","state":"off","opens":"/s/Music"}]}"#;
    let plugin = sections(
        &[Descriptor {
            plugin: "syncthing".into(),
            title: "Syncthing".into(),
            refresh_sec: 30,
            enabled: true,
            failed: false,
        }],
        &[(
            "syncthing".into(),
            parse_section("syncthing", line).unwrap(),
        )],
    );
    let s = rail_sections(
        &home_entries("/fx/v", "", |_| "folder"),
        Shown::default(),
        Vec::new(),
        &[false; 7],
        tail(),
        &plugin,
    );
    let drawn = build(&c, &s, &theme_tags(&bench()));
    assert_eq!(
        texts(&drawn),
        [
            "PLACES:Home,Trash",
            "NETWORK:NAS",
            "DEVICES:QEMU HARDDISK",
            "SYNCTHING:Photos,Notes,Music"
        ]
    );
    let marks: Vec<(RowMark, Option<bool>)> =
        drawn[3].rows.iter().map(|r| (r.mark, r.mounted)).collect();
    assert_eq!(
        marks,
        [
            (RowMark::None, Some(true)),
            (RowMark::None, Some(true)),
            (RowMark::None, Some(false))
        ],
        "each row keeps its glyph; the trailing square is synced green, paused off"
    );
}

#[test]
fn tag_marks_board_draws_swatches_in_the_lifted_colours() {
    let c = english();
    let mut in_use = [false; 7];
    in_use[2] = true;
    let s = rail_sections(
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
    let drawn = build(&c, &s, &theme_tags(&bench()));
    let tags = drawn
        .iter()
        .find(|d| d.heading.as_ref() == "TAGS")
        .expect("TAGS");
    assert_eq!(tags.rows[0].label.as_ref(), "Yellow");
    assert_eq!(
        tags.rows[0].mark,
        RowMark::Tag(TagMark {
            fill: ink("#dc9c36"),
            index: 2,
            digit: "3",
            fallback: false
        })
    );
    let bare = build(&c, &s, &theme_tags(&ThemeRoles::default()));
    let RowMark::Tag(mark) = bare
        .iter()
        .find(|d| d.heading.as_ref() == "TAGS")
        .unwrap()
        .rows[0]
        .mark
    else {
        panic!("a tag mark")
    };
    assert!(
        mark.fallback && mark.fill.is_none(),
        "no tokens yet draws the framed digit in the foreground"
    );
}

#[test]
fn the_provider_rebuilds_only_on_a_real_change() {
    let c = english();
    let s = rail_sections(
        &home_entries("/h", "", |_| "folder"),
        Shown::default(),
        Vec::new(),
        &[false; 7],
        Vec::new(),
        &[],
    );
    let mut p = Provider::default();
    assert!(p.update(&c, s.clone(), theme_tags(&bench())));
    assert!(
        !p.update(&c, s.clone(), theme_tags(&bench())),
        "the same inputs build nothing"
    );
    assert_eq!(p.revision, 1);
    let more = rail_sections(
        &home_entries("/h", "", |_| "folder"),
        Shown {
            recent: true,
            ..Shown::default()
        },
        Vec::new(),
        &[false; 7],
        Vec::new(),
        &[],
    );
    assert!(p.update(&c, more, theme_tags(&bench())));
    assert_eq!(
        (p.revision, texts(&p.drawn)[0].as_str()),
        (2, "PLACES:Home,Recent,Trash")
    );
}

#[test]
fn rail_favorites_and_tags_menu_rows() {
    let c = english();
    let fav = favourite_menu(&c, 0, "/fx/v/work/Docs");
    let MenuRow::Item {
        label, glyph, act, ..
    } = &fav[0]
    else {
        panic!("an item")
    };
    assert_eq!(
        (label.as_ref(), *glyph, fav.len()),
        ("Remove from Favorites", "minus", 1)
    );
    assert_eq!(
        *act,
        Act::RemoveFavourite {
            index: 0,
            path: "/fx/v/work/Docs".into()
        }
    );
    let theme = theme_tags(&bench());
    let clear = tags_menu(&c, &theme, &[State::Absent; 7]);
    assert_eq!(
        clear.len(),
        7,
        "notes.txt carries no tag, so Remove All Tags hides"
    );
    let MenuRow::Item {
        label,
        hint,
        tag,
        check,
        ..
    } = &clear[0]
    else {
        panic!("a tag row")
    };
    assert_eq!((label.as_ref(), *hint, *check), ("Red", "1", Check::None));
    assert_eq!(
        *tag,
        Some(TagMark {
            fill: ink("#d65a4f"),
            index: 0,
            digit: "1",
            fallback: false
        }),
        "the swatch leads, from the menu colours"
    );
    let mut set = [State::Absent; 7];
    set[0] = State::Applied;
    set[1] = State::Mixed;
    let rows = tags_menu(&c, &theme, &set);
    assert!(matches!(
        &rows[0],
        MenuRow::Item {
            check: Check::Applied,
            ..
        }
    ));
    assert!(matches!(
        &rows[1],
        MenuRow::Item {
            check: Check::Mixed,
            ..
        }
    ));
    assert!(matches!(&rows[7], MenuRow::Separator));
    assert!(
        matches!(&rows[8], MenuRow::Item { label, act: Act::ClearTags, .. } if label.as_ref() == "Remove All Tags")
    );
}
