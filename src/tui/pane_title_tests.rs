use super::*;
use crate::jsondoc::parse;
use crate::tui::model::Row;
use crate::tui::render::frame_tests::{entry, model, plain};
use crate::tui::wire::echo_wire;

const LISTED: &str = r#"{"t":"trashbrowse","id":1,"op":"list","ok":true,"total":1,"start":0,"rows":[{"uri":"trash:///old-draft.txt","original":"/fx/v/work/old-draft.txt","size":13,"deleted":"2025-06-14T09:30:00","directory":false,"icon":"text-plain"}]}"#;

// The title row as preview() writes it, between the base colour and the padding around it.
fn title_row(m: &Model, columns: usize) -> String {
    let theme = Theme::from_text("");
    let row = preview(m, 0, columns, 6, &theme);
    let base = theme.base();
    let pad = " ".repeat(pane_padding(columns));
    let body = row.strip_prefix(&format!("{base}{pad}")).unwrap();
    body.strip_suffix(&format!("{base}{pad}{base}")).unwrap().to_string()
}
fn file_title(name: &str, marks: bool, columns: usize) -> String {
    let mut m = model();
    m.font_marks = marks;
    let cursor = m.cursor;
    m.rows.insert(cursor, entry(name, false, 99));
    m.preview_path = m.current_path().unwrap();
    title_row(&m, columns)
}
fn linked_row(name: &str, link: &str) -> Row {
    let mut row = entry(name, false, 99);
    row.link = link.into();
    row
}
fn linked_title(name: &str, link: &str, columns: usize) -> String {
    let mut m = model();
    let cursor = m.cursor;
    m.rows.insert(cursor, linked_row(name, link));
    m.preview_path = m.current_path().unwrap();
    title_row(&m, columns)
}
fn selection_title(count: usize, marks: bool, columns: usize) -> String {
    let mut m = model();
    m.font_marks = marks;
    m.selected.extend(0..count);
    title_row(&m, columns)
}
// The bold name between the file title's foreground and meta inks, then the plain cells after it.
fn named(theme: &Theme, mark: &str, name: &str, tail: &str) -> String {
    format!("{}{mark}\x1b[1m{name}\x1b[22m{}{tail}", theme.foreground, theme.meta)
}
// The exact sequence, its plain cells, and plain text equal to ellipsis(mark + name, width).
fn check(title: &str, mark: &str, name: &str, width: usize, expected: &str) {
    let at = format!("{name:?} mark={mark:?} width={width}");
    assert_eq!(title, expected, "{at}");
    assert_eq!(text_width(&plain(title)), width, "{at}");
    assert_eq!(plain(title), ellipsis(&format!("{mark}{name}"), width), "{at}");
}

#[test]
fn file_title_bolds_only_the_name_cells_at_every_width() {
    let theme = Theme::from_text("");
    let icon = icons::row(&entry("main.rs", false, 99), true);
    let mark = format!("{icon} ");
    let cases = vec![
        ("main.rs", true, 30, named(&theme, &mark, "main.rs", &" ".repeat(19))),
        ("main.rs", false, 30, named(&theme, "", "main.rs", &" ".repeat(21))),
        ("main.rs", true, 11, named(&theme, &mark, "main.rs", "")),
        ("a-very-long-file-name.rs", true, 16, named(&theme, &mark, "a-very-long\u{2026}", "")),
        ("a-very-long-file-name.rs", false, 16, named(&theme, "", "a-very-long-f\u{2026}", "")),
        ("界界.rs", true, 11, named(&theme, &mark, "界界.rs", "")),
        ("界界.rs", true, 7, named(&theme, &mark, "界\u{2026}", "")),
        ("a界b", false, 3, format!("{}\x1b[1ma\x1b[22m \x1b[1m\u{2026}\x1b[22m{}", theme.foreground, theme.meta)),
        ("\u{1f3b5} song", false, 5, named(&theme, "", "\u{1f3b5} s\u{2026}", "")),
        ("main.rs", true, 3, format!("{}{icon} \u{2026}{}", theme.foreground, theme.meta)),
        ("main.rs", true, 2, format!("{}{icon}\u{2026}{}", theme.foreground, theme.meta)),
        ("main.rs", true, 1, format!("{}\u{2026}{}", theme.foreground, theme.meta)),
        ("main.rs", false, 2, named(&theme, "", "m\u{2026}", "")),
        ("main.rs", true, 0, format!("{}{}", theme.foreground, theme.meta)),
        ("main.rs", false, 0, format!("{}{}", theme.foreground, theme.meta)),
    ];
    for (name, marks, columns, expected) in cases {
        let width = columns - 2 * pane_padding(columns);
        let mark = if marks { mark.as_str() } else { "" };
        check(&file_title(name, marks, columns), mark, name, width, &expected);
    }
}

#[test]
fn file_title_at_the_smoke_width_bolds_wide_names_in_cells_81_to_128() {
    // The T smoke frame is 130 wide: the title spans cells 81 to 128, which is 48 cells.
    assert_eq!(preview_area(130, false), (81, 48));
    let theme = Theme::from_text("");
    let icon = icons::row(&entry("x.rs", false, 99), true);
    let mark = format!("{icon} ");
    let long = format!("{}.rs", "界".repeat(30));
    let cases = vec![
        (
            long,
            format!("{}{mark}\x1b[1m{}\x1b[22m \x1b[1m\u{2026}\x1b[22m{}", theme.foreground, "界".repeat(22), theme.meta),
        ),
        ("界界.rs".to_string(), named(&theme, &mark, "界界.rs", &" ".repeat(39))),
    ];
    for (name, expected) in cases {
        check(&file_title(&name, true, 50), &mark, &name, 48, &expected);
    }
}

#[test]
fn selection_title_bolds_only_the_count_text_at_every_width() {
    let icon = icons::FILES;
    let mark = format!("{icon} ");
    let cases = vec![
        (2, true, 30, format!("{mark}\x1b[1m2 items selected\x1b[22m{}", " ".repeat(10))),
        (2, false, 30, format!("\x1b[1m2 items selected\x1b[22m{}", " ".repeat(12))),
        (2, true, 50, format!("{mark}\x1b[1m2 items selected\x1b[22m{}", " ".repeat(30))),
        (120, true, 50, format!("{mark}\x1b[1m120 items selected\x1b[22m{}", " ".repeat(28))),
        (2, true, 12, format!("{mark}\x1b[1m2 items\u{2026}\x1b[22m")),
        (2, true, 3, format!("{mark}\u{2026}")),
    ];
    for (count, marks, columns, expected) in cases {
        let width = columns - 2 * pane_padding(columns);
        let name = format!("{} selected", crate::tui::render::items(count));
        let mark = if marks { mark.as_str() } else { "" };
        check(&selection_title(count, marks, columns), mark, &name, width, &expected);
    }
}

#[test]
fn symlink_target_stays_plain_when_it_fits() {
    let theme = Theme::from_text("");
    let row = linked_row("notes.txt", "../shared/notes.txt");
    let mark = format!("{} ", icons::row(&row, true));
    let target = " → ../shared/notes.txt";
    let title = {
        let mut m = model();
        m.font_marks = true;
        let cursor = m.cursor;
        m.rows.insert(cursor, row);
        m.preview_path = m.current_path().unwrap();
        title_row(&m, 60)
    };
    let width = 60 - 2 * pane_padding(60);
    let tail = " ".repeat(width - text_width(&format!("{mark}notes.txt")) - text_width(target));
    let expected = format!("{}{mark}\x1b[1mnotes.txt\x1b[22m{}{target}{tail}", theme.foreground, theme.meta);
    assert_eq!(title, expected);
    assert_eq!(text_width(&plain(&title)), width);
}

#[test]
fn symlink_target_is_cut_plain_with_an_ellipsis() {
    let theme = Theme::from_text("");
    let mark = format!("{} ", icons::row(&linked_row("notes.txt", "x"), true));
    let title = linked_title("notes.txt", "../a/very/long/path/to/the/target/notes.txt", 40);
    let expected = format!("{}{mark}\x1b[1mnotes.txt\x1b[22m{} → ../a/very/long/path/to/\u{2026}", theme.foreground, theme.meta);
    assert_eq!(title, expected);
    assert_eq!(text_width(&plain(&title)), 38);
}

#[test]
fn name_cut_before_the_symlink_target_drops_the_target() {
    let theme = Theme::from_text("");
    let mark = format!("{} ", icons::row(&linked_row("a-very-long-file-name.rs", "x"), true));
    let title = linked_title("a-very-long-file-name.rs", "../shared/x", 22);
    let expected = format!("{}{mark}\x1b[1ma-very-long-file-\u{2026}\x1b[22m{}", theme.foreground, theme.meta);
    assert_eq!(title, expected);
    assert_eq!(text_width(&plain(&title)), 20);
}

#[test]
fn trash_title_bolds_only_the_name_cells() {
    let (mut w, _reader) = echo_wire();
    let mut m = model();
    m.font_marks = true;
    (m.cursor, m.top) = (7, 2);
    crate::tui::trashview::open(&mut m, &mut w).unwrap();
    m.receive(parse(LISTED).unwrap(), &mut w).unwrap();
    m.cursor = 0;
    let row = m.rows.get(&0).unwrap();
    assert_eq!(row.name, "old-draft.txt");
    let mark = format!("{} ", icons::row(row, true));
    let whole = title_row(&m, 50);
    assert_eq!(whole, format!("{mark}\x1b[1mold-draft.txt\x1b[22m{}", " ".repeat(48 - text_width(&mark) - 13)));
    let cut = title_row(&m, 14);
    assert_eq!(cut, format!("{mark}\x1b[1mold-draft\u{2026}\x1b[22m"));
    assert_eq!(text_width(&plain(&cut)), 12);
    m.font_marks = false;
    let plain_name = title_row(&m, 50);
    assert_eq!(plain_name, format!("\x1b[1mold-draft.txt\x1b[22m{}", " ".repeat(48 - 13)));
}
