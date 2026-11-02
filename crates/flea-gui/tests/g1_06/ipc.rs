use super::harness::{english, held, metrics, open, real_cx, row, Pane};
use flea_gui::ipc::{
    rows::{colour, rect_of, RowsAdapter},
    Adapter,
};
use flea_gui::list::{columns::Column, RowStates};
use flea_gui::palette::Theme;
use flea_gui::row::format::Words;
use gpui::TestAppContext;
use std::rc::Rc;

fn ask(cx: &mut TestAppContext, pane: &Pane, function: &str, arguments: &[&str]) -> Option<String> {
    let held = pane.held(cx);
    let states = pane.states(cx);
    let list = pane.list.borrow();
    let adapter = RowsAdapter {
        list: &list,
        held: &held,
        states: &states,
    };
    let arguments: Vec<String> = arguments
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect();
    adapter.call(function, &arguments).map(|reply| {
        assert_eq!(reply.code, 0, "{function} failed: {}", reply.stderr);
        reply.stdout.trim_end_matches('\n').to_owned()
    })
}

fn pane(cx: &mut TestAppContext) -> Pane {
    let mut link = row("shell", "folder", 22, false);
    link.mode = 0o120777;
    link.link = Some("/usr/share/omarchy".into());
    let mut tool = row("build.sh", "application-x-executable", 2_048, false);
    tool.mode = 0o100755;
    let rows = vec![
        row("docs", "folder", 4096, true),
        row("notes.txt", "text-plain", 18_000, false),
        link,
        tool,
    ];
    let pane = open(cx, held(rows, 1), 900., 10.);
    pane.list
        .borrow_mut()
        .set_words(Words::from_catalog(&english()));
    pane.frame(cx, 0.);
    pane
}

#[test]
fn row_readers_answer_in_ipc_qml_shapes() {
    let mut cx = real_cx();
    let pane = pane(&mut cx);
    let m = metrics();
    let mut ask = |function: &str, arguments: &[&str]| {
        ask(&mut cx, &pane, function, arguments)
            .unwrap_or_else(|| panic!("{function} is unanswered"))
    };
    assert_eq!(ask("rowAt", &["1"]), "notes.txt|file|18.0 kB");
    assert_eq!(ask("rowAt", &["0"]), "docs|dir|4.1 kB");
    assert_eq!(ask("rowAt", &["99"]), "loading");
    assert_eq!(ask("rowAt", &["-1"]), "loading");
    assert_eq!(ask("rowNamePx", &["1"]), "14");
    assert_eq!(ask("rowNamePx", &["99"]), "-1");
    assert_eq!(ask("rowNameText", &["2"]), "shell -> /usr/share/omarchy");
    assert_eq!(ask("rowSizeText", &["2"]), "link");
    assert_eq!(ask("rowSizeText", &["0"]), "\u{b7}");
    assert_eq!(ask("rowGlyph", &["2"]), "symlink");
    assert_eq!(ask("rowGlyph", &["3"]), "terminal");
    assert_eq!(ask("rowClipMark", &["1"]), "");
    assert_eq!(ask("rowClipMark", &["99"]), "unavailable");
    assert_eq!(ask("rowCellOverflow", &["1"]), "0|0|0|0");
    assert_eq!(ask("columnSet", &["1"]), "name,size,date|name,size,date");
    assert_eq!(ask("rowHovered", &["1"]), "false");
    // Window pixels: the header's own height above the list, rows the list's width less the lane.
    let header = m.header as i64;
    assert_eq!(
        ask("listAreaRect", &[]),
        format!("0 {header} 900 {}", 10 * m.row as i64)
    );
    assert_eq!(
        ask("rowRect", &["1"]),
        rect_of(0., m.header + m.row, 900. - m.lane, m.row)
    );
    assert_eq!(ask("rowLeft", &["1"]), "0");
    assert_eq!(ask("rowCentre", &["99"]), "");
    assert_eq!(ask("listContentY", &[]), "0");
    assert_eq!(ask("headerLeft", &[]), "0");
    assert_eq!(ask("headerTop", &[]), "0");
    let centre: Vec<i64> = ask("rowNameCentre", &["1"])
        .split(' ')
        .map(|part| part.parse().unwrap())
        .collect();
    assert!(
        centre[0] > (m.label_x() as i64) && centre[1] == (m.header + m.row * 1.5).round() as i64,
        "the name centre is off its label: {centre:?}"
    );
}

#[test]
fn header_readers_answer_titles_cells_and_the_drag_trace() {
    let mut cx = real_cx();
    let pane = pane(&mut cx);
    let m = metrics();
    assert_eq!(
        ask(&mut cx, &pane, "headerTitles", &[]).unwrap(),
        "Name|Mode|Size|Modified|Kind"
    );
    let size = pane
        .list
        .borrow()
        .header
        .drawn
        .layout
        .cell(Column::Size)
        .unwrap();
    assert_eq!(
        ask(&mut cx, &pane, "headerCellRect", &["size"]).unwrap(),
        format!("{}|{}", size.x.round(), size.width.round())
    );
    assert_eq!(
        ask(&mut cx, &pane, "headerCellRect", &["name"])
            .unwrap()
            .split('|')
            .next(),
        Some("14")
    );
    // A column that is not drawn collapses at width zero on its right neighbour's edge.
    let kind = ask(&mut cx, &pane, "headerCellRect", &["kind"]).unwrap();
    assert_eq!(kind, format!("{}|0", (900. - m.lane - m.pad).round()));
    assert_eq!(
        ask(&mut cx, &pane, "headerCellRect", &["bogus"]).unwrap(),
        ""
    );
    assert_eq!(
        ask(&mut cx, &pane, "headerDragTrace", &[]).unwrap(),
        "0|0|0|0"
    );
    pane.list
        .borrow_mut()
        .header
        .drag
        .begin(Column::Size, 600., 70.);
    pane.list
        .borrow_mut()
        .header
        .drag
        .move_to(Column::Size, 580.);
    let _ = pane.list.borrow_mut().end_drag(true);
    assert_eq!(
        ask(&mut cx, &pane, "headerDragTrace", &[]).unwrap(),
        "600|580|70|90",
        "the trace did not outlive the release"
    );
}

#[test]
fn colours_follow_row_qml_and_a_cut_row_keeps_its_ink() {
    let mut cx = real_cx();
    let pane = pane(&mut cx);
    let palette = pane
        .list
        .borrow()
        .drawn
        .palette
        .expect("a drawn list has a palette");
    assert_eq!(
        ask(&mut cx, &pane, "rowNameColor", &["3"]).unwrap(),
        colour(palette.executable)
    );
    assert_eq!(
        ask(&mut cx, &pane, "rowNameColor", &["2"]).unwrap(),
        colour(palette.symlink)
    );
    assert_eq!(
        ask(&mut cx, &pane, "rowCellColor", &["1"]).unwrap(),
        colour(palette.metadata)
    );
    pane.update(&mut cx, |view| {
        view.states = RowStates {
            listing: 1,
            cursor: Some(1),
            focused: true,
            ..RowStates::default()
        }
    });
    pane.draw(&mut cx);
    assert_eq!(
        ask(&mut cx, &pane, "rowCellColor", &["1"]).unwrap(),
        colour(palette.foreground),
        "a lifted row kept the metadata ink"
    );
    assert_eq!(
        ask(&mut cx, &pane, "rowNameColor", &["3"]).unwrap(),
        colour(palette.executable)
    );
    {
        let mut list = pane.list.borrow_mut();
        list.view.clip.scope("/w");
        list.view.clip.set(&["/w/build.sh".into()], true);
    }
    pane.draw(&mut cx);
    assert_eq!(
        ask(&mut cx, &pane, "rowClipMark", &["3"]).unwrap(),
        "scissors"
    );
    // drag-cut-move: a cut row ghosts its mark alone, so its cells and name keep their full ink.
    assert_eq!(
        ask(&mut cx, &pane, "rowCellColor", &["3"]).unwrap(),
        colour(palette.metadata),
        "a cut row dimmed its cells"
    );
    assert_eq!(
        ask(&mut cx, &pane, "rowNameColor", &["3"]).unwrap(),
        colour(palette.executable),
        "a cut row dimmed its name"
    );
    pane.list.borrow_mut().view.today = Some(0);
    assert_eq!(
        ask(&mut cx, &pane, "rowDateColor", &["1"]).unwrap(),
        colour(palette.foreground),
        "today's date did not lift"
    );
    assert_eq!(
        ask(&mut cx, &pane, "rowDateColor", &["3"]).unwrap(),
        colour(palette.foreground),
        "a cut row dimmed today's date"
    );
    // The hex mapping at its boundaries: an opaque ink prints bare, a dimmed one leads with its alpha.
    let boundary = |text: &str| Theme::from_text(text).background;
    assert_eq!(colour(boundary("background = \"#ffffff\"")), "#ffffff");
    assert_eq!(
        colour(boundary("background = \"#000000\"").opacity(0.5)),
        "#80000000"
    );
}

#[test]
fn readers_owned_elsewhere_stay_unanswered_and_a_renamed_row_has_no_label() {
    let mut cx = real_cx();
    let pane = pane(&mut cx);
    for function in [
        "rowThumb",
        "rowThumbReady",
        "rowThumbRect",
        "rowIcon",
        "rowIconStatus",
        "scrollbarState",
        "listingDropActive",
        "cursor",
    ] {
        assert_eq!(
            ask(&mut cx, &pane, function, &["1"]),
            None,
            "{function} answered without its producer"
        );
    }
    assert!(!ask(&mut cx, &pane, "rowNameRect", &["1"])
        .unwrap()
        .is_empty());
    let selected: Rc<[usize]> = Rc::from(vec![1]);
    pane.update(&mut cx, |view| {
        view.states = RowStates {
            listing: 1,
            renaming: Some(1),
            selected: Some(selected),
            ..RowStates::default()
        }
    });
    pane.draw(&mut cx);
    assert_eq!(ask(&mut cx, &pane, "rowNameRect", &["1"]).unwrap(), "");
    assert_eq!(ask(&mut cx, &pane, "rowNameCentre", &["1"]).unwrap(), "");
}
