//! Output requests remain argv-free JSON lines; opaque tool diagnostics are data.
use super::{
    model::{Card, Model},
    seam::Ticket,
};
use flea::{json::escape, jsondoc::Json};

pub fn text<'a>(j: &'a Json, key: &str) -> Option<&'a str> {
    j.get(key)?.as_str()
}
pub fn boolean(j: &Json, key: &str) -> Option<bool> {
    j.get(key)?.as_bool()
}
pub fn number(j: &Json, key: &str) -> Option<u64> {
    let Json::Num(n) = j.get(key)? else {
        return None;
    };
    n.parse().ok()
}
pub fn strings(j: &Json, key: &str) -> Option<Vec<String>> {
    let Json::Arr(items) = j.get(key)? else {
        return None;
    };
    if items.len() > 10_000 {
        return None;
    }
    items
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect()
}
pub fn quoted(text: &str) -> String {
    format!("\"{}\"", escape(text))
}
pub fn printer_query(id: u64, printer: &str) -> String {
    if printer.is_empty() {
        format!(r#"{{"c":"printoptions","id":{id}}}"#)
    } else {
        format!(
            r#"{{"c":"printoptions","id":{id},"printer":{}}}"#,
            quoted(printer)
        )
    }
}

pub fn cancel(id: u64) -> String {
    format!(r#"{{"c":"transfercancel","id":{id}}}"#)
}

pub fn request(model: &Model, ticket: Ticket) -> Option<String> {
    let selection = model.selection.as_ref()?;
    let paths = selection
        .files
        .iter()
        .map(|f| quoted(&f.path))
        .collect::<Vec<_>>()
        .join(",");
    let fields = format!(
        r#""requestId":{},"menuId":{},"paths":[{}]"#,
        ticket.request, selection.menu, paths
    );
    match model.card.as_ref()? {
        Card::Convert(c) => Some(format!(
            r#"{{"c":"convertbatch",{fields},"format":{},"strip":{},"combine":{},"pages":{}}}"#,
            quoted(&c.format),
            c.strip && c.format != "pdf",
            c.combine,
            quoted(&c.pages)
        )),
        Card::Combine(c) => Some(format!(
            r#"{{"c":"combine",{fields},"layout":{},"spacing":{},"background":{},"format":{},"name":{},"columns":{}}}"#,
            quoted(&format!("{:?}", c.settings.layout)),
            quoted(&format!("{:?}", c.settings.spacing)),
            quoted(&format!("{:?}", c.settings.background)),
            quoted(&c.format),
            quoted(&c.name),
            c.settings.columns
        )),
        Card::Print(p) => Some(format!(
            r#"{{"c":"print",{fields},"printer":{},"printerName":{},"copies":{},"pages":{},"twoSided":{},"grayscale":{}}}"#,
            quoted(&p.printer),
            quoted(&p.printer_name),
            p.copies.parse::<usize>().ok()?,
            quoted(&p.pages),
            p.two_sided,
            p.grayscale
        )),
    }
}
