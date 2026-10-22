// Review round 1 probes, kept as regression tests; each names the v0.3.12 or reference behaviour it compares.
use super::favourites::{Failure, Favourites};
use super::order::{rail_sections, Heading, Shown};
use super::places::{home_entries, record_error};
use super::placeview::{open_entry, Open};
use super::plugin::{clamp_refresh, parse_section};
use super::rail::RecordError;
use super::recent::{bookmarks, entries};
use super::testing::record;

#[test]
fn review_a_plugin_row_with_a_uri_opens_that_uri() {
    let rows = parse_section(
        "p",
        r#"{"op":"section","rows":[{"id":"nas","label":"nas","opens":"sftp://nas/"}]}"#,
    )
    .unwrap();
    assert_eq!(
        open_entry(&rows[0]),
        Open::Network {
            uri: "sftp://nas/".into(),
            label: "nas".into()
        }
    );
}

#[test]
fn a_plugin_row_with_a_file_uri_opens_its_decoded_path() {
    let rows = parse_section(
        "p",
        r#"{"op":"section","rows":[{"id":"a","label":"a","opens":"file:///srv/a%20b"}]}"#,
    )
    .unwrap();
    assert_eq!(open_entry(&rows[0]), Open::Path("/srv/a b".into()));
}

#[test]
fn review_a_bookmark_inside_an_xml_comment_is_not_read() {
    let x = r#"<?xml version="1.0"?><xbel version="1.0"><!-- <bookmark href="file:///ghost" visited="2099-01-01T00:00:00Z"/> --><bookmark href="file:///real" visited="2026-01-01T00:00:00Z"/></xbel>"#;
    let marks = bookmarks(x);
    let read = entries(marks.iter().map(|(h, s)| (h.as_str(), s.as_str())));
    assert_eq!(
        read.iter().map(|v| v.path.as_str()).collect::<Vec<_>>(),
        ["/real"]
    );
}

#[test]
fn review_tab_separated_attributes_still_read() {
    let x = "<xbel version=\"1.0\"><bookmark\thref=\"file:///a\"\tvisited=\"2026-01-01T00:00:00Z\"/></xbel>";
    let marks = bookmarks(x);
    assert_eq!(
        entries(marks.iter().map(|(h, s)| (h.as_str(), s.as_str()))).len(),
        1,
        "{marks:?}"
    );
}

#[test]
fn only_top_level_bookmarks_read_and_space_may_surround_the_equals() {
    let x = "<xbel><![CDATA[<bookmark href=\"file:///cdata\"/>]]><folder><bookmark href=\"file:///nested\" visited=\"2026-01-02T00:00:00Z\"/></folder>\n<bookmark\n href = 'file:///top'\n visited\t=\t\"2026-01-01T00:00:00Z\" title=\"a>b\"/></xbel>";
    assert_eq!(
        bookmarks(x),
        [("file:///top".to_owned(), "2026-01-01T00:00:00Z".to_owned())],
        "only /xbel/bookmark is read"
    );
}

#[test]
fn review_a_byte_order_mark_label_is_blank_as_js_trim_reads_it() {
    assert_eq!(
        record_error(&record("\u{feff}", "/x")),
        Some(RecordError::LabelBlank)
    );
}

#[test]
fn a_next_line_label_is_visible_as_js_trim_reads_it() {
    assert_eq!(
        record_error(&record("\u{85}", "/x")),
        None,
        "String.prototype.trim keeps U+0085"
    );
}

#[test]
fn review_a_plugin_label_is_bounded() {
    let big = "x".repeat(1 << 20);
    let line = format!(r#"{{"op":"section","rows":[{{"id":"a","label":"{big}","opens":"/a"}}]}}"#);
    let rows = parse_section("p", &line).unwrap();
    assert!(
        rows[0].label.len() <= 4096,
        "label kept at {} bytes",
        rows[0].label.len()
    );
}

#[test]
fn a_plugin_row_with_a_long_id_or_path_is_dropped() {
    let id = "i".repeat(257);
    let opens = format!("/{}", "o".repeat(4096));
    let line = format!(
        r#"{{"op":"section","rows":[{{"id":"{id}","label":"a","opens":"/a"}},{{"id":"b","label":"b","opens":"{opens}"}}]}}"#
    );
    assert!(parse_section("p", &line).unwrap().is_empty());
}

#[test]
fn review_a_plugin_path_carries_no_dot_segments() {
    let rows = parse_section(
        "p",
        r#"{"op":"section","rows":[{"id":"a","label":"a","opens":"/srv/../etc"}]}"#,
    )
    .unwrap();
    assert!(
        rows.is_empty() || !rows[0].path.split('/').any(|seg| seg == ".." || seg == "."),
        "kept {:?}",
        rows[0].path
    );
}

#[test]
fn review_a_plugin_label_refuses_bidi_and_c1_controls() {
    let rows = parse_section("p", "{\"op\":\"section\",\"rows\":[{\"id\":\"a\",\"label\":\"\u{202e}fdp.exe\",\"opens\":\"/a\"},{\"id\":\"b\",\"label\":\"x\u{85}y\",\"opens\":\"/b\"}]}").unwrap();
    assert!(
        rows.is_empty(),
        "kept {:?}",
        rows.iter().map(|r| r.label.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn a_manifest_without_refresh_polls_at_the_stated_default() {
    assert_eq!(
        clamp_refresh(None),
        30,
        "the reference's own example period, not the 5 s floor"
    );
    assert_eq!(
        (clamp_refresh(Some(1.0)), clamp_refresh(Some(9e9))),
        (5, 3600)
    );
}

#[test]
fn places_draws_no_heading_over_a_lone_recent_row() {
    let shown = Shown {
        home: false,
        recent: true,
        trash: false,
        tags: false,
    };
    let s = rail_sections(
        &home_entries("/h", "", |_| "folder"),
        shown,
        Vec::new(),
        &[false; 7],
        Vec::new(),
        &[],
    );
    assert_eq!(s[0].rows.len(), 1, "Recent still draws");
    assert!(
        !matches!(s[0].heading, Heading::Key("places.heading.places")),
        "v0.3.12 Sidebar.qml:499 shows PLACES only over Home or Trash rows"
    );
}

#[test]
fn a_partly_malformed_inspection_lands_nothing() {
    let mut favs = Favourites::default();
    favs.records = vec![record("A", "/a"), record("B", "/b")];
    favs.inspect(&[0, 1]);
    let answer = r#"{"statuses":[{"index":0,"record":{"label":"A","path":"/a"},"error":"gone"},{"index":"x"}]}"#;
    assert_eq!(
        favs.inspected(Some(0), answer),
        Some(Failure::InspectUnread)
    );
    assert!(
        favs.statuses.is_empty(),
        "v0.3.12 assigns the statuses only after the whole answer reads"
    );
}

#[test]
fn the_saved_places_keys_read_as_strictly_as_v0312() {
    use super::order::saved_places;
    let doc = crate::jsondoc::parse(r#"{"places":{"showHome":false,"showRecent":true,"showTrash":"no","showTags":1,"favourites":[{"label":"A","path":"/a"}],"sidebarWidth":231}}"#).unwrap();
    let read = saved_places(&doc);
    assert_eq!(
        read.shown,
        Shown {
            home: false,
            recent: true,
            trash: true,
            tags: false
        },
        "only a literal false hides, only a literal true shows"
    );
    assert_eq!(
        (read.favourites, read.width),
        (vec![record("A", "/a")], 224)
    );
    let odd = crate::jsondoc::parse(r#"{"places":{"showRecent":"true","showTags":true,"showTrash":false,"favourites":{"0":1},"sidebarWidth":"231"}}"#).unwrap();
    let read = saved_places(&odd);
    assert_eq!(
        read.shown,
        Shown {
            home: true,
            recent: false,
            trash: false,
            tags: true
        }
    );
    assert_eq!((read.favourites.len(), read.width), (0, 192));
    let absent = saved_places(&crate::jsondoc::Json::Obj(Vec::new()));
    assert_eq!(
        (absent.shown, absent.favourites.len(), absent.width),
        (Shown::default(), 0, 192)
    );
}

#[test]
fn a_committed_write_adopts_the_records_the_cli_printed() {
    use super::favourites::{Finished, Operation};
    let mut favs = Favourites::default();
    favs.records = vec![record("A", "/a"), record("B", "/b")];
    favs.apply(&Operation::Remove { index: 0 }).unwrap();
    let done = favs.finish(
        Some(0),
        r#"{"places":{"favourites":[{"label":"B","path":"/b"}]}}"#,
        "",
        None,
    );
    assert_eq!(
        done,
        Finished {
            failure: None,
            committed: true,
            external_change: false
        }
    );
    assert_eq!(
        favs.records,
        [record("B", "/b")],
        "the next edit is decided against what committed"
    );
    let wire = favs.apply(&Operation::Remove { index: 0 }).unwrap();
    assert!(
        wire.contains(r#""expected":[{"label":"B","path":"/b"}]"#),
        "{wire}"
    );
    let other = favs.finish(
        Some(0),
        r#"{"places":{"favourites":[{"label":"C","path":"/c"}]}}"#,
        "",
        None,
    );
    assert!(
        other.external_change && favs.records == [record("C", "/c")],
        "another window's change is adopted and surfaced"
    );
    favs.apply(&Operation::Remove { index: 0 }).unwrap();
    let failed = favs.finish(
        Some(1),
        "",
        "flea: Favorites changed in another window; try again.",
        Some(vec![record("D", "/d")]),
    );
    assert!(
        !failed.committed && failed.external_change && favs.records == [record("D", "/d")],
        "a refused write re-reads the state first"
    );
}
