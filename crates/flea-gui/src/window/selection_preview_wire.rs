//! The backend lines the column's mounted views ask through, and the replies they read back.
use super::*;
use crate::g2::doc_preview::{
    self,
    markdown::{CodeAnswer, CodeSource, CodeState},
};
use flea::{
    json::escape,
    ui_logic_doc_preview::code_view::{self, Answer, Source},
};
pub(crate) const SLOT: &str = "column";

/// The backend lines every mounted view asks through: one token each, written to the pane's outbox.
#[derive(Clone)]
pub(crate) struct Wire(pub Outbox);
impl Wire {
    fn send(&self, line: String) {
        self.0.borrow_mut().push(line);
    }
}
impl doc_preview::code::CodeHost for Wire {
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
impl doc_preview::markdown::CodeHost for Wire {
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
impl doc_preview::csv::CodeHost for Wire {
    fn code(&self, line: &dyn Fn(u64) -> String) -> u64 {
        let id = token();
        self.send(line(id));
        id
    }
}
// The archive's names ride the column's own meta ask, which asks with "archive":true.
impl doc_preview::archive::ArchiveHost for Wire {
    fn ask_meta(&self, _: &str) {}
}
impl doc_preview::html::HtmlHost for Wire {
    fn ask_html(&self, slot: &str, path: &str, width: u32, height: u32) -> u64 {
        let id = token();
        let line = flea::ui_logic_doc_preview::html::ask_line(id, slot, path, width, height);
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
impl doc_preview::office::OfficeHost for Wire {
    fn office(&self, path: &str, sheets: bool) -> u64 {
        let id = token();
        self.send(flea::ui_logic_doc_preview::office::request_line(
            id, path, sheets,
        ));
        id
    }
}

fn number(doc: &Json, key: &str) -> usize {
    doc.get(key).and_then(Json::as_f64).unwrap_or(0.).max(0.) as usize
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

/// Sample input: {"t":"code","token":4,"slot":"column","state":"styled","first":1,"total":2,"lines":[[5,"fn"],[]]}.
pub(crate) fn code_reply(doc: &Json) -> code_view::Reply {
    code_view::Reply {
        token: number(doc, "token") as u64,
        state: Answer::parse(doc.get("state").and_then(Json::as_str).unwrap_or("")),
        first: doc.get("first").map_or(1, |_| number(doc, "first")),
        total: number(doc, "total"),
        lines: runs(doc),
    }
}

/// Sample input: {"t":"code","token":4,"slot":"column","state":"plain","first":1,"total":2,"lines":[[0,"# Title"],[]]}.
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

/// Sample input: {"t":"html","slot":"column","token":3,"state":"ready","file":"/cache/x.png","total":61,"width":984,"title":"Page"}.
pub(crate) fn html_reply(doc: &Json) -> flea::ui_logic_doc_preview::html::Reply {
    let text = |key| doc.get(key).and_then(Json::as_str).unwrap_or("").to_owned();
    flea::ui_logic_doc_preview::html::Reply {
        token: number(doc, "token") as u64,
        slot: text("slot"),
        state: text("state"),
        file: text("file"),
        total: number(doc, "total"),
        width: number(doc, "width") as u32,
        title: text("title"),
    }
}
