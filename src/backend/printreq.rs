// Feature requests stay off the listing loop and share its detached-job shutdown registry.
use flea::bytename::ExactText as _;
use super::{aliases::Aliases, mime::Db, opsdispatch::Ops, opsreq::OpMsg, pagerange::Pages, printconvert, printing, printtool};
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

pub enum Request {
    Options { id: usize, printer: String },
    Print(Print),
    Convert(super::convertbatch::Ask),
    Combine(super::combine::Ask),
}

pub struct Print {
    pub request_id: usize,
    pub menu_id: usize,
    pub paths: Vec<PathBuf>,
    pub printer: String,
    pub printer_name: String,
    pub copies: usize,
    pub pages: String,
    pub two_sided: bool,
    pub grayscale: bool,
}

pub fn paths(line: &str) -> Vec<PathBuf> {
    let paths = field_str_array(line, "paths");
    if paths.is_empty() {
        field_str(line, "path")
            .into_iter()
            .map(flea::bytename::exact)
            .collect()
    } else {
        paths.into_iter().map(flea::bytename::exact).collect()
    }
}

impl Request {
    pub fn parse(line: &str) -> Self {
        match field_str(line, "c").as_deref() {
            Some("printoptions") => Self::Options { id: field_usize(line, "id").unwrap_or(0), printer: field_str(line, "printer").unwrap_or_default() },
            Some("print") => Self::Print(Print {
                request_id: field_usize(line, "requestId").unwrap_or(0), menu_id: field_usize(line, "menuId").unwrap_or(0), paths: paths(line),
                printer: field_str(line, "printer").unwrap_or_default(), printer_name: field_str(line, "printerName").unwrap_or_default(),
                copies: crate::jsondoc::parse(line).ok().and_then(|j| match j.get("copies") { None => Some(1), Some(_) => field_usize(line, "copies") }).unwrap_or(0), pages: field_str(line, "pages").unwrap_or_else(|| "All".into()),
                two_sided: field_bool(line, "twoSided"), grayscale: field_bool(line, "grayscale"),
            }),
            Some("combine") => Self::Combine(super::combine::Ask::parse(line)),
            _ => Self::Convert(super::convertbatch::Ask::parse(line)),
        }
    }
}

fn query(args: &[&str], cancel: &AtomicBool) -> Result<String, String> {
    printtool::run(&args.iter().map(|arg| (*arg).into()).collect::<Vec<_>>(), cancel, printtool::QUERY_WAIT).map_err(|e| e.message)
}

// A query whose failure the reply survives; a cancel still stops the reply.
fn optional(result: Result<String, String>, cancel: &AtomicBool) -> Result<String, String> {
    match result { Err(err) if cancel.load(Ordering::Relaxed) => Err(err), result => Ok(result.unwrap_or_default()) }
}

pub fn options_line(id: usize, printer: &str, cancel: &AtomicBool) -> String {
    let result = (|| {
        let list = query(&["lpstat", "-e"], cancel)?;
        // A system with destinations but no default makes lpstat -d exit 1; the list still holds.
        let default = optional(query(&["lpstat", "-d"], cancel), cancel)?;
        let destinations = printing::destinations(&list, &default);
        let printer = if printer.is_empty() { destinations.first().map_or("", |d| d.name.as_str()) } else { printer };
        if !printer.is_empty() && !destinations.iter().any(|d| d.name == printer) { return Err("Choose a printer from the destination list.".into()); }
        let caps = if printer.is_empty() { printing::Capabilities::default() } else { printing::capabilities(&query(&["lpoptions", "-p", printer, "-l"], cancel)?) };
        let details = if printer.is_empty() { String::new() } else { optional(query(&["lpstat", "-l", "-p", printer], cancel), cancel)? };
        let printer_name = printing::printer_name(&details, printer);
        let rows: Vec<_> = destinations.iter().map(|d| format!(r#"{{"name":"{}","default":{}}}"#, escape(&d.name), d.default)).collect();
        let message = if destinations.is_empty() { printing::NO_PRINTER } else { "" };
        Ok(format!(r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{}],"printer":"{}","printerName":"{}","duplex":{},"twoSided":{},"color":{},"grayscale":{},"message":"{message}","setupArgv":{},"err":""}}"#,
            rows.join(","), escape(printer), escape(&printer_name), caps.duplex, caps.two_sided, caps.color, caps.grayscale, crate::json::string_array(&printing::setup_argv())))
    })();
    result.unwrap_or_else(|err: String| format!(r#"{{"t":"printoptions","id":{id},"ok":false,"destinations":[],"err":"{}"}}"#, escape(&err)))
}

// Print's input: an office document becomes the PDF fetch returns, every other kind prints as it is.
pub(crate) fn print_input_with(fetch: impl FnOnce(&std::path::Path, Arc<AtomicBool>) -> Result<PathBuf, String>, path: &std::path::Path,
                               kind: printconvert::Kind, cancel: &Arc<AtomicBool>) -> Result<PathBuf, String> {
    if kind == printconvert::Kind::Office { fetch(path, Arc::clone(cancel)) } else { Ok(path.to_path_buf()) }
}

fn print_file(path: &std::path::Path, kind: printconvert::Kind, options: &printing::Options, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    let file = print_input_with(super::officepdf::cached_pdf, path, kind, cancel)?;
    let argv = printing::argv(&file, kind == printconvert::Kind::Image, kind.paged(), options)?;
    printtool::run(&argv, cancel, printtool::TOOL_WAIT).map_err(|e| e.message)?;
    Ok(())
}

fn print(ask: Print, selection: Result<Option<Vec<super::menu_actions::Selected>>, String>, id: usize, mime: &Db, aliases: &Aliases,
         cancel: Arc<AtomicBool>, tx: &std::sync::mpsc::Sender<OpMsg>) -> String {
    let mut sent = 0;
    let mut message = String::new();
    let outcome: Result<(), String> = (|| {
        let selected = selection?;
        super::menu_actions::validate_sources(selected.as_deref(), &ask.paths)?;
        printtool::inputs(&ask.paths)?;
        let kinds: Vec<_> = ask.paths.iter().map(|p| printconvert::classify(&p.to_string_lossy(), mime, aliases)).collect();
        if !printconvert::can_print(&kinds) { return Err("Select PDF, images, text, code or office documents to print.".into()); }
        let options = printing::Options { destination: ask.printer.clone(), copies: ask.copies, pages: Pages::parse(&ask.pages)?, two_sided: ask.two_sided, grayscale: ask.grayscale };
        printing::argv(&ask.paths[0], false, true, &options)?;
        let list = printing::destinations(&query(&["lpstat", "-e"], &cancel)?, "");
        if !list.iter().any(|d| d.name == ask.printer) { return Err(printing::NO_PRINTER.into()); }
        let caps = printing::capabilities(&query(&["lpoptions", "-p", &ask.printer, "-l"], &cancel)?);
        if ask.two_sided && !caps.duplex { return Err("This printer does not support two-sided printing.".into()); }
        let mut refused = Vec::new();
        for (path, kind) in ask.paths.iter().zip(kinds) {
            if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let display = if ask.printer_name.is_empty() { &ask.printer } else { &ask.printer_name };
            let result = print_file(path, kind, &options, &cancel);
            if result.is_err() && cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
            message = match &result { Ok(()) => printing::receipt(&name, display), Err(reason) => printing::refusal(&name, reason) };
            let _ = tx.send(OpMsg::Meta { line: format!(r#"{{"t":"printed","id":{id},"requestId":{},"path":"{}","ok":{},"message":"{}"}}"#,
                ask.request_id, escape(&path.exact_text()), result.is_ok(), escape(&message)) });
            match result {
                Ok(()) => sent += 1,
                Err(_) => refused.push(message.clone()),
            }
        }
        if refused.is_empty() { Ok(()) } else { Err(refused.join("\n")) }
    })();
    let err = outcome.err().unwrap_or_default();
    if !err.is_empty() && sent == 0 && message.is_empty() && !cancel.load(Ordering::Relaxed) {
        let name = ask.paths.first().and_then(|p| p.file_name()).unwrap_or_default().to_string_lossy();
        message = printing::refusal(&name, &err);
    }
    format!(r#"{{"t":"printdone","id":{id},"requestId":{},"ok":{},"sent":{sent},"message":"{}","err":"{}"}}"#, ask.request_id, err.is_empty(), escape(&message), escape(&err))
}

#[cfg(test)]
#[path = "print_protocol_tests.rs"]
mod tests;

pub fn serve(out: &mut impl Write, ops: &mut Ops, request: Request, mime: Arc<Db>, aliases: Arc<Aliases>) {
    let (paths, menu_id) = match &request {
        Request::Options { .. } => (Vec::new(), 0),
        Request::Print(ask) => (ask.paths.clone(), ask.menu_id),
        Request::Convert(ask) => (ask.paths.clone(), ask.menu_id),
        Request::Combine(ask) => (ask.paths.clone(), ask.menu_id),
    };
    let selection = super::opsdispatch::menu_sources(ops, menu_id);
    if !matches!(&request, Request::Options { .. } | Request::Print(_)) {
        let targets: Vec<_> = paths.first().and_then(|p| p.parent()).map(|p| p.to_path_buf()).into_iter().collect();
        if super::opsdispatch::pending_busy_for(out, ops, "convert", &paths, &targets) { return; }
    }
    let id = ops.claim_id();
    let cancel = Arc::new(AtomicBool::new(false));
    ops.detached.insert(id, &cancel);
    match &request {
        Request::Options { .. } => {}
        Request::Print(ask) => {
            writeln!(
                out,
                r#"{{"t":"printstarted","id":{id},"requestId":{},"title":"{}"}}"#,
                ask.request_id,
                escape(&printconvert::title("Print", &paths))
            )
            .ok();
        }
        Request::Convert(ask) => {
            writeln!(
                out,
                "{}",
                super::archivereq::convertstarted_line(
                    id,
                    ask.request_id,
                    &paths
                        .first()
                        .map_or(String::new(), |p| p.exact_text().into_owned())
                )
            )
            .ok();
        }
        Request::Combine(ask) => {
            writeln!(
                out,
                r#"{{"t":"combinestarted","id":{id},"requestId":{}}}"#,
                ask.request_id
            )
            .ok();
        }
    }
    out.flush().ok();
    let tx = ops.tx.clone();
    std::thread::spawn(move || {
        let line = match request {
            Request::Options { id, printer } => options_line(id, &printer, &cancel),
            Request::Print(ask) => print(ask, selection, id, &mime, &aliases, cancel, &tx),
            Request::Convert(ask) => {
                let (line, steps) =
                    super::convertbatch::run(ask, selection, id, &mime, &aliases, cancel, &tx);
                let _ = tx.send(OpMsg::Converted { id, line, steps });
                return;
            }
            Request::Combine(ask) => super::combine::run(ask, selection, id, &mime, &aliases, cancel, &tx),
        };
        let _ = tx.send(OpMsg::DetachedDone { id, line });
    });
}
