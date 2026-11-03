//! [ and ] on the production Shell: a workbook of two sheets turns them in Quick Look, any other card keeps the tab step.
use super::preview_tests::{answer, body, close, context, draw, file, opened, sent, Lines};
use super::test_shell::{german_press, land, row, set_cursor};
use crate::{
    g2::pv1::{doc::View, Body},
    state::Shell,
    window::u07_tests::read,
};
use flea::{jsondoc::Json, keys::Context, protocol::RowFacts};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn rows() -> Vec<RowFacts> {
    vec![
        row("alpha", true),
        file("budget.xlsx", "x-office-spreadsheet"),
        file("single.xlsx", "x-office-spreadsheet"),
        file("book.pdf", "application-pdf"),
        file("report.docx", "x-office-document"),
    ]
}

// The middle tab of three, so tabNext (to 2) tells itself apart from tabPrevious (to 0) and from a close.
fn three_tabs(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Lines) {
    cx.executor().allow_parking();
    let (shell, cx, requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    for (action, listing) in [("tabNew", 2), ("tabNew", 3), ("tabPrevious", 4)] {
        cx.update(|window, cx| {
            shell.update(
                cx,
                |shell, cx| assert!(shell.tab_action(action, window, cx)),
            )
        });
        land(&shell, cx, "/files", rows(), listing);
    }
    assert_eq!(strip(&shell, cx), (3, 1));
    (shell, cx, requests)
}

fn strip(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, usize) {
    shell.read_with(cx, |shell, _| {
        (shell.strip.tabs[0].count(), shell.strip.tabs[0].current())
    })
}

// Opens the workbook at `index` and answers its officesheets ask with `names`, one cached CSV per sheet.
fn open_workbook(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    requests: &mut Lines,
    index: usize,
    names: &[&str],
) {
    set_cursor(shell, cx, index);
    cx.simulate_keystrokes("space");
    draw(cx);
    let ask = sent(requests, "officesheets");
    let id = ask
        .last()
        .and_then(|doc| doc.get("id"))
        .and_then(Json::as_f64)
        .expect("the workbook asks for its sheets") as u64;
    let words: Vec<String> = names.iter().map(|n| format!(r#""{n}""#)).collect();
    let paths: Vec<String> = names.iter().map(|n| format!(r#""/c/{n}.csv""#)).collect();
    answer(
        shell,
        cx,
        format!(
            r#"{{"t":"officesheets","id":{id},"state":"ready","names":[{}],"paths":[{}]}}"#,
            words.join(","),
            paths.join(",")
        ),
    );
}

// The mounted workbook's (shown sheet, sheet count); panics unless the office view is mounted.
fn sheet(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, usize) {
    match body(shell, cx) {
        Some(Body::Doc(doc)) => match &doc.view {
            View::Office(office) => cx.read(|cx| {
                let office = &office.read(cx).office;
                (office.sheet, office.sheet_paths.len())
            }),
            _ => panic!("a workbook mounts the office view"),
        },
        other => panic!("no mounted view: {other:?}"),
    }
}

fn open(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    read(shell, cx, "previewOpen", &[])
}

// EY-58: ] on Monthly turns to Savings and keeps the card; three sheets tell [ from ], both ends wrap, no tab moves.
#[gpui::test]
fn brackets_turn_a_workbooks_sheets_in_the_open_card(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = three_tabs(cx);
    open_workbook(
        &shell,
        cx,
        &mut requests,
        1,
        &["Monthly", "Savings", "Notes"],
    );
    assert_eq!(sheet(&shell, cx), (0, 3), "Monthly shows first of three");
    for (key, want) in [("]", 1), ("[", 0), ("[", 2), ("]", 0)] {
        cx.simulate_keystrokes(key);
        draw(cx);
        assert_eq!(open(&shell, cx), "true", "{key} keeps the card open");
        assert_eq!(sheet(&shell, cx), (want, 3), "{key} turns the sheet");
        assert_eq!(strip(&shell, cx), (3, 1), "{key} moves no tab");
    }
    close(&shell, cx);
}

// One sheet has nothing to turn: ] keeps the card and moves no tab, as v0.3.12 keeps tab keys in Quick Look (EY-438).
#[gpui::test]
fn a_single_sheet_keeps_tab_keys_in_the_card(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = three_tabs(cx);
    open_workbook(&shell, cx, &mut requests, 2, &["Only"]);
    assert_eq!(open(&shell, cx), "true", "the workbook opens the card");
    assert_eq!(sheet(&shell, cx), (0, 1), "one sheet shows");
    assert_eq!(
        context(&shell, cx),
        Context::Pdf,
        "a workbook card takes the pdf keys"
    );
    cx.simulate_keystrokes("]");
    draw(cx);
    assert_eq!(open(&shell, cx), "true", "] keeps a one-sheet card");
    assert_eq!(sheet(&shell, cx), (0, 1), "the one sheet stays");
    assert_eq!(strip(&shell, cx), (3, 1), "] moves no tab");
    close(&shell, cx);
}

// A PDF card binds [ and ] in its context too, yet it has no sheets: [ keeps the card and moves no tab.
#[gpui::test]
fn a_pdf_card_keeps_tab_keys_in_the_card(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = three_tabs(cx);
    set_cursor(&shell, cx, 3);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(open(&shell, cx), "true", "the PDF opens the card");
    assert_eq!(context(&shell, cx), Context::Pdf);
    cx.simulate_keystrokes("[");
    draw(cx);
    assert_eq!(open(&shell, cx), "true", "[ keeps the PDF card");
    assert_eq!(context(&shell, cx), Context::Pdf);
    assert_eq!(strip(&shell, cx), (3, 1), "[ moves no tab");
    close(&shell, cx);
}

// EY-46: a workbook's table sits on the outlined card as a CSV table does; a document's pages stay on the bare ground.
#[gpui::test]
fn a_workbook_card_is_framed_and_a_document_card_is_not(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = three_tabs(cx);
    open_workbook(&shell, cx, &mut requests, 2, &["Only"]);
    let framed = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        let body = body(shell, cx).expect("a mounted view");
        assert!(matches!(&body, Body::Doc(doc) if matches!(doc.view, View::Office(_))));
        cx.read(|cx| body.framed(cx))
    };
    assert!(framed(&shell, cx), "the workbook card draws its outline");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(open(&shell, cx), "false", "escape closes the workbook card");
    set_cursor(&shell, cx, 4);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert!(
        !framed(&shell, cx),
        "a document's pages keep the bare ground"
    );
    close(&shell, cx);
}

// Both German AltGr text and the physical bracket binding stay inside Quick Look (EY-438).
#[gpui::test]
fn altgr_and_physical_brackets_keep_tabs_inside_the_card(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = three_tabs(cx);
    set_cursor(&shell, cx, 3);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(open(&shell, cx), "true", "the PDF opens the card");
    cx.simulate_event(german_press("[", "[", Some("8"), true));
    draw(cx);
    assert_eq!(
        open(&shell, cx),
        "true",
        "AltGr+8 types [ and keeps the card"
    );
    assert_eq!(context(&shell, cx), Context::Pdf);
    assert_eq!(strip(&shell, cx), (3, 1), "AltGr+8 moves no tab");
    cx.simulate_event(german_press("[", "ü", Some("["), false));
    draw(cx);
    assert_eq!(open(&shell, cx), "true", "ü keeps the PDF card");
    assert_eq!(context(&shell, cx), Context::Pdf);
    assert_eq!(strip(&shell, cx), (3, 1), "ü moves no tab");
    close(&shell, cx);
}
