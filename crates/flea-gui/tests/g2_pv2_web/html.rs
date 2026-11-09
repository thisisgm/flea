//! The HTML view (html-preview.qml, html-auto-render.qml, html-reload.qml, html-source-refusals.qml): one ask after
//! the settle at the frame's device size, the page decoded off the UI thread, a reload keeping the old page up, Source
//! built once on demand, and every refusal drawing its fixed sentence.
use super::parts::{catalog, draw, look, mark, palette, scale, typeface, with, Calls, METRICS};
use super::{png, root};
use flea::ui_logic_doc_preview::code_view::{
    Answer, Ask, Reply as CodeReply, Source, BYTE_GATE, REMOTE_GATE,
};
use flea::ui_logic_doc_preview::html::{Picture, Reply, View, SETTLE_MS};
use flea::ui_logic_doc_preview::refusal::Refusal;
use flea_gui::g2::doc_preview::code::{shape, CodeHost};
use flea_gui::g2::doc_preview::html::{HtmlHost, HtmlView, Metrics, SourceParts};
use flea_gui::g2::doc_preview::Palette;
use gpui::{TestAppContext, WindowHandle};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Ask(String, String, u32, u32),
    Cancel(String, u32),
}

struct Host(Calls<Call>);

impl HtmlHost for Host {
    fn ask_html(&self, slot: &str, path: &str, width: u32, height: u32) -> u64 {
        self.0
            .borrow_mut()
            .push(Call::Ask(slot.into(), path.into(), width, height));
        self.0.borrow().len() as u64
    }

    fn cancel_html(&self, slot: &str, width: u32) {
        self.0.borrow_mut().push(Call::Cancel(slot.into(), width));
    }
}

struct Code(Calls<Ask>);

impl CodeHost for Code {
    fn ask_code(&self, _: &str, ask: &Ask) -> u64 {
        self.0.borrow_mut().push(ask.clone());
        self.0.borrow().len() as u64
    }
}

const PAGE: Metrics = Metrics {
    row_height: 28.,
    radius: 10.,
    round_top: false,
};

fn open(
    cx: &mut TestAppContext,
    slot: &'static str,
    host: bool,
) -> (WindowHandle<HtmlView>, Calls<Call>, Calls<Ask>) {
    let (calls, code) = (Calls::default(), Calls::default());
    let (html, lines) = (Host(calls.clone()), code.clone());
    let view = cx.add_window(|_, cx| {
        let parts = SourceParts {
            typeface: typeface(),
            palette: palette(),
            metrics: METRICS,
        };
        let ground = Palette {
            background: flea_gui::palette::Theme::default().background,
        };
        let mut view = HtmlView::new(slot, ground, PAGE, parts, mark("html"), cx);
        let host: Option<Box<dyn HtmlHost>> = if host { Some(Box::new(html)) } else { None };
        view.set_hosts(
            host,
            Some(Box::new(move || {
                Box::new(Code(lines.clone())) as Box<dyn CodeHost>
            })),
        );
        view.set_catalog(catalog(), cx);
        view
    });
    draw(cx, view);
    (view, calls, code)
}

fn show(cx: &mut TestAppContext, view: WindowHandle<HtmlView>, path: &str, size: u64) {
    let path = path.to_owned();
    view.update(cx, |v, _, cx| {
        v.set_active(true, cx);
        v.set_path(&path, size, false, cx);
    })
    .unwrap();
    draw(cx, view);
}

fn settle(cx: &mut TestAppContext, view: WindowHandle<HtmlView>) {
    cx.executor()
        .advance_clock(Duration::from_millis(SETTLE_MS + 1));
    draw(cx, view);
}

fn ready(token: u64, file: &str, width: u32) -> Reply {
    Reply {
        token,
        slot: "look".into(),
        state: "ready".into(),
        file: file.into(),
        total: 61,
        width,
        title: "Page".into(),
    }
}

#[path = "html_page_tests.rs"]
mod page;
#[path = "html_remote_tests.rs"]
mod remote;
#[path = "html_source_tests.rs"]
mod source;
