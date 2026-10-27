//! The CSV table and the office view as real GPUI entities in test windows: a stand-in code engine and office backend
//! answer as the backend's wire does, and every case reads what the frame lays out and draws.
use flea::i18n::I18n;
use flea::json::{escape, field_str, field_usize};
use flea::ui_logic_pv2::csv::{self, records};
use flea::ui_logic_pv2::office::State;
use flea_gui::g2::pv2::csv::{CodeHost, CsvView, TableFace, TablePalette};
use flea_gui::g2::pv2::office::{OfficeHost, OfficeView};
use flea_gui::g2::pv2::{Metrics, Palette};
use gpui::{font, hsla, px, size, TestAppContext, WindowHandle};
use std::{cell::RefCell, path::PathBuf, process::Command, rc::Rc};

const METRICS: Metrics = Metrics {
    gap: 9.,
    margin: 28.,
    row_height: 28.,
};

fn face() -> TableFace {
    TableFace {
        font: font("JetBrains Mono"),
        caption: px(12.),
        line: 16.,
        row_padding_x: 14.,
        hairline: 1.,
    }
}

fn table_palette() -> TablePalette {
    TablePalette {
        foreground: hsla(0.1, 0.3, 0.68, 1.),
        bright: hsla(0.1, 0.3, 0.79, 1.),
        muted: hsla(0.05, 0.1, 0.5, 1.),
    }
}

fn palette() -> Palette {
    Palette {
        background: hsla(0.06, 0.37, 0.08, 1.),
    }
}

/// The lines every request sent, shared with the test.
type Sent = Rc<RefCell<Vec<String>>>;

struct Engine(Sent);

impl CodeHost for Engine {
    fn code(&self, line: &dyn Fn(u64) -> String) -> u64 {
        let token = self.0.borrow().len() as u64 + 1;
        self.0.borrow_mut().push(line(token));
        token
    }
}

struct Backend(Sent);

impl OfficeHost for Backend {
    fn office(&self, path: &str, sheets: bool) -> u64 {
        let id = self.0.borrow().len() as u64 + 1;
        self.0
            .borrow_mut()
            .push(flea::ui_logic_pv2::office::request_line(id, path, sheets));
        id
    }
}

/// The code engine's raw table reply to one ask over `text`, aligned to whole records as src/backend/csvrange.rs does.
fn answer(ask: &str, text: &str) -> String {
    let token = field_usize(ask, "token").unwrap();
    let record = field_usize(ask, "record").unwrap();
    let count = field_usize(ask, "count").unwrap();
    let delim = field_str(ask, "csv").and_then(|d| d.chars().next());
    let lines = csv::plain_lines(text);
    let recs = records(&lines, delim);
    let mut starts = Vec::new();
    let mut at = 1;
    for rec in &recs {
        starts.push((at, rec.span));
        at += rec.span;
    }
    let (first, held) = match starts.get(record.saturating_sub(1)) {
        Some(&(first, _)) => {
            let end = first + count - 1;
            let last = starts[record - 1..]
                .iter()
                .map(|&(s, n)| s + n - 1)
                .find(|&l| l >= end)
                .unwrap_or(lines.len());
            (first, lines[first - 1..last.min(lines.len())].to_vec())
        }
        None => (1, Vec::new()),
    };
    let body: Vec<String> = held
        .iter()
        .map(|l| {
            if l.is_empty() {
                "[]".into()
            } else {
                format!(r#"[0,"{}"]"#, escape(l))
            }
        })
        .collect();
    format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"plain","first":{first},"total":{},"lines":[{}],"records":{},"firstRecord":{record}}}"#,
        lines.len(),
        body.join(","),
        recs.len()
    )
}

fn budget() -> String {
    let mut text = String::from("Item,Planned,Actual,Difference\n");
    for (item, planned, actual) in [
        ("Rent", 950., 950.),
        ("Electricity", 70., 64.2),
        ("Water", 32., 32.),
        ("Internet", 40., 40.),
        ("Phone", 25., 25.),
        ("Groceries", 420., 451.35),
        ("Transport", 90., 76.8),
        ("Insurance", 48., 48.),
        ("Eating out", 80., 112.6),
        ("Clothes", 50., 18.),
        ("Gifts", 30., 45.),
        ("Savings", 200., 200.),
        ("Total", 2035., 2062.95),
    ] {
        text.push_str(&format!(
            "{item},{planned:.2},{actual:.2},{:.2}\n",
            planned - actual
        ));
    }
    text
}

fn table(
    cx: &mut TestAppContext,
    look: bool,
    width: f32,
    height: f32,
) -> (WindowHandle<CsvView>, Sent) {
    let sent: Sent = Rc::default();
    let host = Engine(Rc::clone(&sent));
    let view = cx.open_window(size(px(width), px(height)), move |_, _| {
        let mut view = CsvView::new(look, face(), table_palette());
        view.set_host(Box::new(host));
        view
    });
    (view, sent)
}

fn draw<V: 'static>(cx: &mut TestAppContext, view: WindowHandle<V>) {
    view.update(cx, |_, window, _| window.refresh()).unwrap();
    cx.run_until_parked();
}

// Draws, answers every ask the frame sent from `text`, and draws again until the frame asks nothing more.
fn serve(cx: &mut TestAppContext, view: WindowHandle<CsvView>, sent: &Sent, text: &str) {
    let mut answered = 0;
    for _ in 0..200 {
        draw(cx, view);
        let pending: Vec<String> = sent.borrow()[answered..].to_vec();
        if pending.is_empty() {
            return;
        }
        for ask in pending {
            answered += 1;
            let reply = answer(&ask, text);
            view.update(cx, |v, _, cx| v.answered(&reply, cx)).unwrap();
            cx.run_until_parked();
        }
    }
    panic!("the asks did not end");
}

fn read<R>(
    cx: &mut TestAppContext,
    view: WindowHandle<CsvView>,
    f: impl FnOnce(&CsvView) -> R,
) -> R {
    view.update(cx, |v, _, _| f(v)).unwrap()
}

// ql-office-sheet: the grid sits where the board draws it, row numbers muted, the header bold over its rule.
#[gpui::test]
fn pv2_csv_quick_look_draws_the_boards_grid(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 902., 625.);
    view.update(cx, |v, _, cx| v.set_path("/d/budget.csv", cx))
        .unwrap();
    draw(cx, view);
    assert_eq!(
        read(cx, view, |v| v.head.len()),
        0,
        "nothing draws before the first answer"
    );
    serve(cx, view, &sent, &budget());
    assert!(
        sent.borrow()
            .iter()
            .all(|a| a.contains(r#""raw":true"#) && a.contains(r#""csv":",""#)),
        "every ask is raw"
    );
    assert_eq!(sent.borrow().len(), 1, "one ask fills the first screen");
    let (head, rows) = read(cx, view, |v| {
        (
            v.head
                .iter()
                .map(|c| (c.x, c.text.clone(), c.right))
                .collect::<Vec<_>>(),
            v.rows
                .iter()
                .map(|r| {
                    (
                        r.top,
                        r.number.as_ref().map(|n| (n.x, n.text.clone(), n.right)),
                    )
                })
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(
        head[0],
        (56., "Item".into(), false),
        "Item starts at 56 as the board's first column"
    );
    assert!(
        head[1..].iter().all(|h| h.2),
        "Planned, Actual and Difference are right-aligned numbers"
    );
    assert_eq!(rows.len(), 13, "13 rows on the board's card");
    assert_eq!(
        rows[0].0, 33.,
        "the first row starts under the 33 px header"
    );
    assert_eq!(rows[1].0 - rows[0].0, 28., "rows are 28 apart");
    assert_eq!(
        rows[0].1,
        Some((0., "1".into(), true)),
        "row 1's number is right-aligned in the gutter"
    );
    let i18n = I18n::default();
    assert_eq!(
        read(cx, view, |v| v.count_text(&i18n)),
        Some("13 rows".into()),
        "the bar counts 13 rows"
    );
    let (shaped, painted) = read(cx, view, |v| (v.cells.shaped, v.painted.get()));
    assert_eq!(
        shaped,
        4 + 13 * 5,
        "each drawn cell, number and header shaped once"
    );
    draw(cx, view);
    assert_eq!(
        read(cx, view, |v| v.cells.shaped),
        shaped,
        "a second frame shapes nothing"
    );
    assert!(
        read(cx, view, |v| v.painted.get()) > painted,
        "and paints again"
    );
}

#[gpui::test]
fn pv2_csv_column_fits_its_frame_and_elides(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, false, 400., 600.);
    let mut text = String::from("name,size\n");
    text.push_str(&format!("{},1\n", "x".repeat(200)));
    view.update(cx, |v, _, cx| v.set_path("/d/long.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    let cells = read(cx, view, |v| {
        v.rows[0]
            .cells
            .iter()
            .map(|c| (c.x, c.w, c.cut))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        read(cx, view, |v| v.head[0].x),
        12.,
        "the header has only the board's 12 px inset"
    );
    assert!(cells[0].2, "the long first cell is elided");
    assert!(
        cells.iter().all(|c| c.0 + c.1 <= 400. - 12.),
        "the drawn columns end inside the frame"
    );
    assert!(
        read(cx, view, |v| v.rows[0].number.is_none()),
        "the column draws no row numbers"
    );
}

#[gpui::test]
fn pv2_csv_wide_grid_scrolls_sideways_under_a_fixed_header(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 460., 600.);
    let mut text = (1..=12)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    text.push('\n');
    text.push_str(
        &(1..=12)
            .map(|c| format!("value-{c}"))
            .collect::<Vec<_>>()
            .join(","),
    );
    text.push('\n');
    view.update(cx, |v, _, cx| v.set_path("/d/wide.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    let at = |v: &CsvView, name: &str| v.head.iter().find(|c| &*c.text == name).map(|c| c.x);
    let before = read(cx, view, |v| at(v, "c3")).unwrap();
    assert_eq!(
        read(cx, view, |v| at(v, "c12")),
        None,
        "a column out of view is not laid out"
    );
    view.update(cx, |v, _, _| v.scroll.0 = 200.).unwrap();
    draw(cx, view);
    assert_eq!(
        read(cx, view, |v| at(v, "c3")),
        Some(before - 200.),
        "the header moves with the grid"
    );
    assert_eq!(
        read(cx, view, |v| v.rows[0].top),
        33.,
        "and stays at the top"
    );
}

// A frame between the answer and its parse, or for a binary file, draws no table at all.
#[gpui::test]
fn pv2_csv_never_draws_a_partial_or_refused_table(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 460., 600.);
    view.update(cx, |v, _, cx| v.set_path("/d/nul.csv", cx))
        .unwrap();
    draw(cx, view);
    let ask = sent.borrow()[0].clone();
    let token = field_usize(&ask, "token").unwrap();
    let binary = format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"binary","first":1,"total":0,"lines":[]}}"#
    );
    view.update(cx, |v, _, cx| v.answered(&binary, cx)).unwrap();
    draw(cx, view);
    assert!(
        read(cx, view, |v| v.head.is_empty() && !v.first_screen()),
        "the binary file draws no table"
    );
    draw(cx, view);
    assert_eq!(sent.borrow().len(), 1, "and asks nothing more");
    view.update(cx, |v, _, cx| v.set_text("plain.csv", "a,b\n1,2\n", cx))
        .unwrap();
    assert!(
        read(cx, view, |v| v.head.is_empty()),
        "a parse not yet landed draws nothing"
    );
    draw(cx, view);
    assert_eq!(
        read(cx, view, |v| (v.feed.body_rows(), v.rows.len())),
        (1, 1),
        "the caller's text draws one row"
    );
}

// A scroll past the first screen asks only what it lands on and keeps the first screen's widths.
#[gpui::test]
fn pv2_csv_scroll_keeps_the_widths_and_asks_near_the_view(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 460., 600.);
    let mut text = String::from("id,name\n");
    for i in 1..5000 {
        text.push_str(&format!(
            "{i},{}\n",
            if i == 60 {
                "n".repeat(120)
            } else {
                format!("f{i:05}")
            }
        ));
    }
    view.update(cx, |v, _, cx| v.set_path("/d/big.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    let widths = read(cx, view, |v| v.feed.widths.clone());
    let asked = sent.borrow().len();
    view.update(cx, |v, _, cx| v.scroll_to_row(4990, cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    assert_eq!(
        sent.borrow().len() - asked,
        1,
        "one request draws the last screen"
    );
    assert_eq!(
        read(cx, view, |v| v.feed.widths.clone()),
        widths,
        "a far jump preserves the widths"
    );
    assert!(
        read(cx, view, |v| v.drawn_texts().any(|t| t == "f04999")),
        "the last row draws"
    );
    let shaped = read(cx, view, |v| v.cells.len());
    assert!(
        shaped <= 3 * 22 * 3,
        "the shaped cells stay bounded: {shaped}"
    );
}

fn scratch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-pv2-office")
}

/// A marked scratch root that removes itself when its test passes and stays behind for a failure.
struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        let parent = scratch().canonicalize().unwrap();
        let ours = self.0.is_absolute()
            && self.0.join(".flea-test-sandbox").is_file()
            && self.0.parent().and_then(|p| p.canonicalize().ok()) == Some(parent);
        if ours && !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn root() -> Root {
    std::fs::create_dir_all(scratch()).unwrap();
    let out = Command::new("mktemp")
        .arg("-d")
        .arg(scratch().join("test.XXXXXXXX"))
        .output()
        .unwrap();
    let root = PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g2-pv2-office\n").unwrap();
    Root(root)
}

// The PDF view's render thread is real, so its wakes come from outside the deterministic scheduler.
fn office(cx: &mut TestAppContext, look: bool) -> (WindowHandle<OfficeView>, Sent, Sent) {
    cx.executor().allow_parking();
    let (asked, code): (Sent, Sent) = (Rc::default(), Rc::default());
    let (backend, engine) = (Backend(Rc::clone(&asked)), Engine(Rc::clone(&code)));
    let view = cx.open_window(size(px(902.), px(625.)), move |_, cx| {
        let mut view = OfficeView::new(look, palette(), METRICS, face(), table_palette(), cx);
        view.set_host(Box::new(backend), Box::new(engine), cx);
        view
    });
    (view, asked, code)
}

fn open(cx: &mut TestAppContext, view: WindowHandle<OfficeView>, path: &str) {
    view.update(cx, |v, window, cx| v.set_path(path, window, cx))
        .unwrap();
    draw(cx, view);
}

fn reply(cx: &mut TestAppContext, view: WindowHandle<OfficeView>, line: &str) -> bool {
    let taken = view
        .update(cx, |v, window, cx| v.reply(line, window, cx))
        .unwrap();
    draw(cx, view);
    taken
}

fn state(cx: &mut TestAppContext, view: WindowHandle<OfficeView>) -> State {
    view.update(cx, |v, _, _| v.office.state.clone()).unwrap()
}

// ql-office-converting then ql-office-doc: no pager and dimmed zoom, the cached PDF opens, a stale line drops.
#[gpui::test]
fn pv2_office_document_converts_then_opens_in_the_pdf_viewer(cx: &mut TestAppContext) {
    let (view, asked, _) = office(cx, true);
    open(cx, view, "/d/talk.odp");
    assert_eq!(
        asked.borrow()[0],
        r#"{"c":"officepdf","id":1,"path":"/d/talk.odp"}"#,
        "a presentation asks for its PDF"
    );
    let i18n = I18n::default();
    let bar = view.update(cx, |v, _, cx| v.bar(&i18n, cx)).unwrap();
    assert_eq!(
        bar.glyph, "presentation",
        "the bar leads with the presentation mark"
    );
    let strip = bar.strip.unwrap();
    assert_eq!(
        strip.counter, None,
        "the pager waits until there are pages to count"
    );
    assert!(
        strip
            .items
            .iter()
            .filter(|(i, _, _)| matches!(
                i.control,
                flea::ui_logic_pv2::viewer::Control::ZoomIn
                    | flea::ui_logic_pv2::viewer::Control::ZoomOut
            ))
            .all(|(i, _, _)| !i.enabled),
        "the zoom marks rest dimmed"
    );
    assert!(
        strip
            .items
            .iter()
            .filter(|(i, _, _)| matches!(
                i.control,
                flea::ui_logic_pv2::viewer::Control::Previous
                    | flea::ui_logic_pv2::viewer::Control::Next
            ))
            .all(|(i, _, _)| !i.visible),
        "no pager"
    );
    assert_eq!(
        view.update(cx, |v, _, _| v.failure(&i18n)).unwrap(),
        None,
        "converting says nothing"
    );
    assert!(
        view.update(cx, |v, _, cx| v.shown(cx)).unwrap(),
        "the swap may show the converting card"
    );
    let thumb = {
        let buffer = image::RgbaImage::from_raw(16, 9, vec![255; 16 * 9 * 4]).unwrap();
        std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(buffer)]))
    };
    view.update(cx, |v, _, cx| v.set_thumbnail("/d/talk.odp", thumb, cx))
        .unwrap();
    let rect = view
        .update(cx, |v, _, _| v.thumb_rect((902., 625.)))
        .unwrap()
        .unwrap();
    assert_eq!(
        rect.2,
        902. - 56.,
        "a 16 by 9 slide takes the card's width inside the page margin"
    );
    assert!(
        (rect.1 - (625. - rect.3) / 2.).abs() < 1e-6,
        "and sits centred in the height left over"
    );
    assert!(reply(
        cx,
        view,
        r#"{"t":"officepdf","id":1,"state":"converting"}"#
    ));
    open(cx, view, "/d/garden-report.docx");
    assert!(
        !reply(
            cx,
            view,
            r#"{"t":"officepdf","id":1,"state":"ready","path":"/c/talk.pdf"}"#
        ),
        "the left subject's late line is dropped"
    );
    assert!(
        view.update(cx, |v, _, _| v.thumb.is_none()).unwrap(),
        "the old thumbnail went with its subject"
    );
    assert!(reply(
        cx,
        view,
        r#"{"t":"officepdf","id":2,"state":"ready","path":"/c/garden.pdf"}"#
    ));
    assert_eq!(state(cx, view), State::Ready);
    assert!(
        !view.update(cx, |v, _, cx| v.shown(cx)).unwrap(),
        "the swap waits for the first page"
    );
    assert_eq!(
        view.update(cx, |v, _, cx| v.bar(&i18n, cx).glyph).unwrap(),
        "file-text",
        "a document leads with the lined page"
    );
}

// ql-office-locked: a failed conversion of an encrypted package reads its header and says it is password protected.
#[gpui::test]
fn pv2_office_failures_project_the_approved_sentences(cx: &mut TestAppContext) {
    let root = root();
    let lease = root.0.join("lease.docx");
    let mut cfb = vec![0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    cfb.resize(4096, 0);
    std::fs::write(&lease, cfb).unwrap();
    let report = root.0.join("report.docx");
    // A real container, since a fake one falls to the unsupported card before any conversion runs.
    std::fs::write(&report, container::REAL_DOCX).unwrap();
    let i18n = I18n::default();
    let (view, _, _) = office(cx, true);
    let cases = [
        (
            &lease,
            "soffice: general input/output error",
            false,
            "This document is password protected.",
            "lock",
        ),
        (
            &report,
            "the converter wrote no PDF",
            false,
            "This document could not be previewed.",
            "alert",
        ),
        (
            &report,
            "the conversion took longer than 60 seconds",
            false,
            "This document could not be previewed.",
            "alert",
        ),
        (
            &report,
            "LibreOffice is not installed on this box.",
            true,
            "This file cannot be previewed.",
            "file",
        ),
    ];
    for (n, (path, err, unavailable, sentence, glyph)) in cases.into_iter().enumerate() {
        open(cx, view, path.to_str().unwrap());
        let flag = if unavailable {
            r#","unavailable":true"#
        } else {
            ""
        };
        reply(
            cx,
            view,
            &format!(
                r#"{{"t":"officepdf","id":{},"state":"failed","err":"{}"{flag}}}"#,
                n + 1,
                escape(err)
            ),
        );
        for _ in 0..200 {
            if view
                .update(cx, |v, _, _| {
                    v.failure(&i18n).map(|f| f.glyph) == Some(glyph)
                })
                .unwrap()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            draw(cx, view);
        }
        let failure = view
            .update(cx, |v, _, _| v.failure(&i18n))
            .unwrap()
            .unwrap();
        assert_eq!(
            (failure.sentence.as_str(), failure.glyph),
            (sentence, glyph),
            "{err}"
        );
        assert_eq!(
            failure.open_with.as_deref(),
            Some("Open With\u{2026}"),
            "{err}: Open With is the way in"
        );
        assert_eq!(
            view.update(cx, |v, _, _| v.office.diagnostic.clone())
                .unwrap(),
            err,
            "the backend text stays a diagnostic"
        );
    }
    let read = view.update(cx, |v, _, _| v.sniffed_bytes).unwrap();
    assert!(
        read <= 8 + 65_557 + 302,
        "the sniff read a bounded head and tail: {read}"
    );
    let bar = view.update(cx, |v, _, cx| v.bar(&i18n, cx)).unwrap();
    assert_eq!(
        bar.strip.unwrap().counter,
        None,
        "the failure keeps the viewer's bar with no pager"
    );
    open(cx, view, "/d/talk.odp");
    view.update(cx, |v, window, cx| v.leave(window, cx))
        .unwrap();
    assert_eq!(
        state(cx, view),
        State::Cancelled,
        "leaving mid conversion is a state"
    );
    assert_eq!(
        view.update(cx, |v, _, _| v.failure(&i18n)).unwrap(),
        None,
        "and never a toast or a sentence"
    );
}

// ql-office-sheet: a workbook opens its first sheet's cached CSV in Quick Look's table, [ and ] switch sheets.
#[gpui::test]
fn pv2_office_sheets_open_in_the_table_and_switch(cx: &mut TestAppContext) {
    let (view, asked, code) = office(cx, true);
    open(cx, view, "/d/budget.xlsx");
    assert!(
        asked.borrow()[0].starts_with(r#"{"c":"officesheets","id":1"#),
        "a workbook asks for its sheets"
    );
    assert!(reply(
        cx,
        view,
        r#"{"t":"officesheets","id":1,"state":"ready","names":["Monthly","Savings"],"paths":["/c/m.csv","/c/s.csv"]}"#
    ));
    let table = view.update(cx, |v, _, _| v.table.clone()).unwrap();
    let mut answered = 0;
    for _ in 0..20 {
        draw(cx, view);
        let pending: Vec<String> = code.borrow()[answered..].to_vec();
        if pending.is_empty() {
            break;
        }
        for ask in pending {
            answered += 1;
            assert!(
                ask.contains(r#""path":"/c/m.csv""#),
                "the first sheet's CSV is read"
            );
            let line = answer(&ask, &budget());
            cx.update(|cx| table.update(cx, |t, cx| t.answered(&line, cx)));
            cx.run_until_parked();
        }
    }
    let i18n = I18n::default();
    assert!(
        view.update(cx, |v, _, cx| v.shown(cx)).unwrap(),
        "the sheet's first screen is whole"
    );
    let bar = view.update(cx, |v, _, cx| v.bar(&i18n, cx)).unwrap();
    assert_eq!(bar.glyph, "table", "the bar leads with the table mark");
    assert_eq!(
        bar.count.as_deref(),
        Some("13 rows"),
        "and counts the sheet's rows"
    );
    assert_eq!(
        bar.sheets,
        vec![("Monthly".into(), true), ("Savings".into(), false)],
        "Monthly in foreground, Savings muted"
    );
    assert!(bar.strip.is_none(), "the text card's bar has no pager");
    let facts = view.update(cx, |v, _, cx| v.facts(&i18n, cx)).unwrap();
    assert_eq!(
        facts[0],
        ("Sheets".into(), "2".into()),
        "the facts count the sheets"
    );
    assert!(view
        .update(cx, |v, _, cx| v.action("nextSheet", cx))
        .unwrap());
    draw(cx, view);
    assert!(
        code.borrow()
            .last()
            .unwrap()
            .contains(r#""path":"/c/s.csv""#),
        "] reads the next sheet"
    );
    assert!(
        view.update(cx, |v, _, cx| v.bar(&i18n, cx).sheets[1].1)
            .unwrap(),
        "Savings is current"
    );
    assert!(view
        .update(cx, |v, _, cx| v.action("nextSheet", cx))
        .unwrap());
    draw(cx, view);
    assert!(
        code.borrow()
            .last()
            .unwrap()
            .contains(r#""path":"/c/m.csv""#),
        "] on the last sheet wraps to the first, as the 3MF plate steps wrap"
    );
    assert!(
        view.update(cx, |v, _, cx| v.bar(&i18n, cx).sheets[0].1)
            .unwrap(),
        "Monthly is current again"
    );
    view.update(cx, |v, _, cx| {
        v.set_meta("/d/budget.xlsx", Some("Budget".into()), None, cx)
    })
    .unwrap();
    let facts = view.update(cx, |v, _, cx| v.facts(&i18n, cx)).unwrap();
    assert_eq!(
        facts[1],
        ("Title".into(), "Budget".into()),
        "the document's Title follows the count"
    );
}

#[path = "g2_pv2_office/container.rs"]
mod container;
#[path = "g2_pv2_office/fix_qlsheet.rs"]
mod fix_qlsheet;
#[path = "g2_pv2_office/review.rs"]
mod review;
#[path = "g2_pv2_office/review_r2.rs"]
mod review_r2;
#[path = "g2_pv2_office/review_r3.rs"]
mod review_r3;
