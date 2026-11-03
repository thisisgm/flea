//! The backend lines the card's mounted views ask through, and the replies they read back.
use super::token;
use crate::g2::{pv2, pv4};
use flea::{
    json::escape,
    jsondoc::Json,
    ui_logic_pv2::code_view::{self, Answer, Source},
};
use pv2::markdown::{CodeAnswer, CodeSource, CodeState};
use std::{cell::RefCell, rc::Rc};

/// Quick Look's ask slot, as the backend's samples name it.
pub(crate) const SLOT: &str = "look";
/// The slot a slow PDF's copy is asked on (src/backend/pdfcopy.rs).
pub(crate) const COPY_SLOT: &str = "quicklook";

/// The card's outbox: every view writes its asks here, and each write wakes the Shell's drain, layout-time asks included.
#[derive(Clone)]
pub(crate) struct Wire {
    lines: Rc<RefCell<Vec<String>>>,
    wake: async_channel::Sender<()>,
}
impl Wire {
    pub(crate) fn new() -> (Self, async_channel::Receiver<()>) {
        let (wake, woken) = async_channel::unbounded();
        let lines = Rc::default();
        (Self { lines, wake }, woken)
    }
    pub(crate) fn send(&self, line: String) {
        self.lines.borrow_mut().push(line);
        let _ = self.wake.try_send(());
    }
    pub(crate) fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.lines.borrow_mut())
    }
}
impl pv2::code::CodeHost for Wire {
    fn ask_code(&self, slot: &str, ask: &code_view::Ask) -> u64 {
        let id = token();
        let source = match &ask.source {
            Source::Path(path) => format!(r#""path":"{}""#, escape(path)),
            Source::Text { text, name } => format!(
                r#""text":"{}","name":"{}","lang":"""#,
                escape(text),
                escape(name)
            ),
        };
        self.send(format!(
            r#"{{"c":"code","token":{id},"slot":"{}",{source},"first":{},"count":{}}}"#,
            escape(slot),
            ask.first,
            ask.count
        ));
        id
    }
}
impl pv2::markdown::CodeHost for Wire {
    fn ask_code(&self, slot: &str, source: CodeSource, first: usize, count: usize) -> u64 {
        let id = token();
        let source = match source {
            CodeSource::Path(path) => format!(r#""path":"{}""#, escape(&path)),
            CodeSource::Text { text, name, lang } => format!(
                r#""text":"{}","name":"{}","lang":"{}""#,
                escape(&text),
                escape(&name),
                escape(&lang)
            ),
        };
        self.send(format!(
            r#"{{"c":"code","token":{id},"slot":"{}",{source},"first":{first},"count":{count}}}"#,
            escape(slot)
        ));
        id
    }
}
impl pv2::csv::CodeHost for Wire {
    fn code(&self, line: &dyn Fn(u64) -> String) -> u64 {
        let id = token();
        self.send(line(id));
        id
    }
}
// The archive's names ride the card's own meta ask, sent with "archive":true when it mounts.
impl pv2::archive::ArchiveHost for Wire {
    fn ask_meta(&self, _: &str) {}
}
impl pv2::html::HtmlHost for Wire {
    fn ask_html(&self, slot: &str, path: &str, width: u32, height: u32) -> u64 {
        let id = token();
        let line = flea::ui_logic_pv2::html::ask_line(id, slot, path, width, height);
        self.send(line);
        id
    }
    fn cancel_html(&self, slot: &str, width: u32) {
        self.send(format!(
            r#"{{"c":"html","token":{},"slot":"{}","path":"","width":{width},"height":0,"cancel":true}}"#,
            token(),
            escape(slot)
        ));
    }
}
impl pv2::office::OfficeHost for Wire {
    fn office(&self, path: &str, sheets: bool) -> u64 {
        let id = token();
        self.send(flea::ui_logic_pv2::office::request_line(id, path, sheets));
        id
    }
}
impl pv2::font::FontHost for Wire {
    fn font_look(&self, ask: &flea::ui_logic_pv2::font::Ask) {
        let sizes: Vec<String> = ask.sizes.iter().map(u32::to_string).collect();
        self.send(format!(
            r#"{{"c":"fontlook","id":{},"path":"{}","fg":"{}","bg":"{}","sizes":[{}],"text":"{}"}}"#,
            ask.id,
            escape(&ask.path),
            escape(&ask.fg),
            escape(&ask.bg),
            sizes.join(","),
            escape(ask.text)
        ));
    }
}
impl pv2::view::CopyHost for Wire {
    fn pdf_copy(&self, path: &str, slot: &str) -> u64 {
        let id = token();
        self.send(format!(
            r#"{{"c":"pdfcopy","id":{id},"slot":"{slot}","path":"{}"}}"#,
            escape(path)
        ));
        id
    }
}
impl pv4::seam::Wire for Wire {
    fn send(&self, line: String) -> bool {
        Wire::send(self, line);
        true
    }
}

fn number(doc: &Json, key: &str) -> usize {
    doc.get(key).and_then(Json::as_f64).unwrap_or(0.).max(0.) as usize
}

fn text(doc: &Json, key: &str) -> String {
    doc.get(key).and_then(Json::as_str).unwrap_or("").to_owned()
}

/// A `code` reply's lines: each a flat [role, text, role, text, ...] array.
fn runs(doc: &Json) -> Vec<Vec<(i64, String)>> {
    let lines = doc.get("lines").and_then(Json::as_array).unwrap_or(&[]);
    lines
        .iter()
        .map(|line| {
            line.as_array()
                .unwrap_or(&[])
                .chunks(2)
                .filter_map(|pair| match pair {
                    [role, text] => Some((role.as_f64()? as i64, text.as_str()?.to_owned())),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

/// Sample input: {"t":"code","token":4,"slot":"look","state":"styled","first":1,"total":2,"lines":[[5,"fn"],[]]}.
pub(crate) fn code_reply(doc: &Json) -> code_view::Reply {
    code_view::Reply {
        token: number(doc, "token") as u64,
        state: Answer::parse(doc.get("state").and_then(Json::as_str).unwrap_or("")),
        first: doc.get("first").map_or(1, |_| number(doc, "first")),
        total: number(doc, "total"),
        lines: runs(doc),
    }
}

pub(crate) fn markdown_answer(doc: &Json) -> CodeAnswer {
    let state = match doc.get("state").and_then(Json::as_str).unwrap_or("") {
        "styled" => CodeState::Styled,
        "plain" => CodeState::Plain,
        "binary" => CodeState::Binary,
        "large" => CodeState::Large,
        _ => CodeState::Failed,
    };
    CodeAnswer {
        token: number(doc, "token") as u64,
        state,
        total: number(doc, "total"),
        first: number(doc, "first"),
        lines: runs(doc),
    }
}

pub(crate) fn html_reply(doc: &Json) -> flea::ui_logic_pv2::html::Reply {
    flea::ui_logic_pv2::html::Reply {
        token: number(doc, "token") as u64,
        slot: text(doc, "slot"),
        state: text(doc, "state"),
        file: text(doc, "file"),
        total: number(doc, "total"),
        width: number(doc, "width") as u32,
        title: text(doc, "title"),
    }
}

/// Sample input: {"t":"fontlooked","id":4,"family":"Inter","style":"Bold","glyphs":2548,"files":["/c/a-0.png"]}.
pub(crate) fn font_reply(doc: &Json) -> flea::ui_logic_pv2::font::Reply {
    flea::ui_logic_pv2::font::Reply {
        id: number(doc, "id") as u64,
        err: doc.get("err").and_then(Json::as_str).map(str::to_owned),
        family: text(doc, "family"),
        style: text(doc, "style"),
        glyphs: number(doc, "glyphs") as u64,
        files: doc
            .get("files")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_owned)
            .collect(),
    }
}
