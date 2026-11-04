// tests/js/settingsshelf.js, case for case: the Shelf's rows, now its plugin page's, and the pile's pinned reading.
use super::pages::Row;
use super::shelf::{self, Input, Pin};
use crate::i18n::{I18n, Paths};
use crate::jsondoc;
use std::path::PathBuf;

fn english() -> I18n {
    I18n::load(Paths { omarchy: PathBuf::from("/nonexistent"), language_file: PathBuf::from("/nonexistent"), catalogs: Vec::new() })
}

fn rows(state: &str, pins: Vec<Pin>) -> Vec<Row> {
    shelf::rows(&shelf::input(&jsondoc::parse(state).unwrap(), pins), &english())
}

fn kinds(rows: &[Row]) -> String {
    rows.iter().map(|r| match r {
        Row::Group { .. } => "group", Row::Check { .. } => "check", Row::Choice { .. } => "choice", Row::Hint { .. } => "hint",
        Row::Favourite { .. } => "favourite", _ => "other",
    }).collect::<Vec<_>>().join("|")
}

fn find<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter().find(|r| r.id() == id).unwrap_or_else(|| panic!("no row {id}"))
}

fn on(row: &Row) -> bool {
    match row { Row::Check { on, .. } => *on, Row::Group { switch: Some(on), .. } => *on, other => panic!("{other:?}") }
}

fn available(row: &Row) -> bool {
    match row { Row::Check { available, .. } | Row::Choice { available, .. } | Row::Favourite { available, .. } => *available, other => panic!("{other:?}") }
}

fn selected(row: &Row) -> &str {
    match row { Row::Choice { selected, .. } => selected, other => panic!("{other:?}") }
}

const PILE: &str = r#"{"items":[{"path":"/home/gm/Invoices","folder":true,"pinned":true},{"path":"/home/gm/loose.txt","folder":false,"pinned":false},{"path":"/home/gm/Work/","folder":true,"pinned":true,"missing":true},{"folder":true,"pinned":true},{"path":"/home/gm/nope","pinned":"yes"}]}"#;

#[test]
fn st2_settingsshelf_only_pinned_entries_list_and_a_trailing_slash_is_not_the_name() {
    let pins = shelf::pinned(PILE);
    assert_eq!(pins.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("|"), "Invoices|Work");
    assert_eq!(format!("{}|{}|{}", pins[0].folder, pins[0].missing, pins[1].missing), "true|false|true", "a pin says whether it is a folder and whether the file is gone");
    assert_eq!(pins.len(), 2, "an entry with no path, and a pinned flag that is not a boolean, are not pins");
}

#[test]
fn st2_settingsshelf_a_pile_that_cannot_be_read_is_no_pins() {
    assert_eq!(shelf::pinned("{\"items\":[{").len() + shelf::pinned("").len(), 0, "a pile that cannot be parsed is no pins rather than a throw");
    assert_eq!(shelf::pinned(r#"{"items":{"path":"/a"}}"#).len(), 0, "and a pile whose items are not a list is no pins either");
}

#[test]
fn st2_settingsshelf_the_section_is_the_master_the_two_routes_the_captures_and_pinned() {
    let fresh = rows("{}", Vec::new());
    assert_eq!(kinds(&fresh), "group|check|choice|hint|group|check|check|choice|group");
    // The master installs the bar plugin, so it ships off and the rows under it open on what the switch would turn on.
    assert_eq!((on(find(&fresh, "shelf.enabled")), on(find(&fresh, "shelf.bar")), selected(find(&fresh, "shelf.rail"))), (false, true, "off"));
    assert_eq!((on(find(&fresh, "shelf.screenshots")), on(find(&fresh, "shelf.recordings")), selected(find(&fresh, "shelf.recent"))), (true, true, "3"));
    let Row::Choice { labels, .. } = find(&fresh, "shelf.rail") else { panic!() };
    assert_eq!(labels.join("|"), "Off|Left|Right|Bottom", "the rail names its four edges in the board's order");
}

#[test]
fn st2_settingsshelf_the_master_greys_every_row_under_it_and_is_the_way_back_on() {
    let off = rows(r#"{"shelf":{"enabled":false}}"#, Vec::new());
    assert_eq!((on(find(&off, "shelf.enabled")), available(find(&off, "shelf.bar")), available(find(&off, "shelf.recent"))), (false, false, false));
    assert!(find(&off, "shelf.enabled").focusable());
    let live = rows(r#"{"shelf":{"enabled":true}}"#, Vec::new());
    assert!(available(find(&live, "shelf.bar")) && available(find(&live, "shelf.recent")));
}

#[test]
fn st2_settingsshelf_a_pin_is_a_favourite_row_and_a_missing_one_takes_the_error_role() {
    let pins = vec![
        Pin { path: "/home/gm/Invoices".into(), name: "Invoices".into(), folder: true, missing: false },
        Pin { path: "/home/gm/Work/handoff.md".into(), name: "handoff.md".into(), folder: false, missing: true },
    ];
    let pinned = rows("{}", pins);
    assert_eq!(kinds(&pinned), "group|check|choice|hint|group|check|check|choice|group|favourite|favourite");
    let (Row::Favourite { path, glyph, .. }, Row::Favourite { error, glyph: file, .. }) = (find(&pinned, "pin:0"), find(&pinned, "pin:1")) else { panic!() };
    assert_eq!((path.as_str(), glyph.as_str(), error.as_str(), file.as_str()), ("/home/gm/Invoices", "folder", "missing", "file"));
    // CHANGES.md plugin-shelf-page: Pin This Folder takes title case like every button.
    assert!(matches!(find(&pinned, "pinFolder"), Row::Group { action: Some(a), .. } if a == "Pin This Folder"), "the Pinned heading carries the shelf's own Add");
}

#[test]
fn st2_settingsshelf_a_stored_count_off_the_four_falls_back_to_three() {
    assert_eq!(selected(find(&rows(r#"{"shelf":{"recent":4}}"#, Vec::new()), "shelf.recent")), "3");
    assert_eq!(selected(find(&rows(r#"{"shelf":{"rail":"top"}}"#, Vec::new()), "shelf.rail")), "off");
    let input: Input = shelf::input(&jsondoc::parse(r#"{"shelf":{"recent":5,"bar":false}}"#).unwrap(), Vec::new());
    assert_eq!((input.recent, input.bar), (5, false));
    assert_eq!(shelf::value("recent", &jsondoc::parse("4").unwrap()), None, "a write the row does not offer is refused");
    assert_eq!(shelf::value("rail", &jsondoc::parse("\"left\"").unwrap()), Some(super::schema::Value::Choice("left".into())));
    assert_eq!(shelf::value("enabled", &jsondoc::parse("true").unwrap()), None, "the master is the plugin's switch, not a row");
}
