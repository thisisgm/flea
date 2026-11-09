//! The CSV table and the office view as real GPUI entities in test windows: a stand-in code engine and office backend
//! answer as the backend's wire does, and every case reads what the frame lays out and draws.
#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
use flea::json::{escape, field_str, field_usize};
use flea::ui_logic_doc_preview::csv::{self, records};
use flea::ui_logic_doc_preview::csv_feed::WIDEN_CELLS;
use flea::ui_logic_doc_preview::office::State;
use flea_gui::g2::doc_preview::csv::{CodeHost, CsvView, TableFace, TablePalette};
use flea_gui::g2::doc_preview::office::{OfficeHost, OfficeView};
use flea_gui::g2::doc_preview::{Metrics, Palette};
use gpui::{font, px, size, TestAppContext, WindowHandle};
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
        foreground: flea_gui::palette::Theme::default().foreground,
        bright: flea_gui::palette::Theme::default().foreground_bright,
        muted: flea_gui::palette::Theme::default().muted,
    }
}

fn palette() -> Palette {
    Palette {
        background: flea_gui::palette::Theme::default().background,
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
            .push(flea::ui_logic_doc_preview::office::request_line(
                id, path, sheets,
            ));
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

// Draws until no held cell is left to widen by, as the view's own next-frame requests would.
fn widen(cx: &mut TestAppContext, view: WindowHandle<CsvView>) {
    for _ in 0..10_000 {
        if !read(cx, view, |v| v.feed.widening) {
            return;
        }
        draw(cx, view);
    }
    panic!("the widening did not end");
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

#[path = "g2_pv2_office/container.rs"]
mod container;
#[path = "g2_pv2_office/csv_table.rs"]
mod csv_table;
#[path = "g2_pv2_office/fix_qlsheet.rs"]
mod fix_qlsheet;
// The PDF writer the pv2 suite owns; this suite takes only its letter page.
#[allow(dead_code)]
#[path = "g2_pv2/fixture.rs"]
mod fixture;
#[path = "g2_pv2_office/office_cut.rs"]
mod office_cut;
#[path = "g2_pv2_office/office_doc.rs"]
mod office_doc;
#[path = "g2_pv2_office/office_sheets.rs"]
mod office_sheets;
#[path = "g2_pv2_office/review.rs"]
mod review;
#[path = "g2_pv2_office/review_r2.rs"]
mod review_r2;
#[path = "g2_pv2_office/review_r3.rs"]
mod review_r3;
#[path = "g2_pv2_office/s9g8_widen.rs"]
mod s9g8_widen;
