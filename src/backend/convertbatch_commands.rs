// Conversion requests, command arguments, destination planning and protocol replies.
use super::{destination, PDF_DPI};
use crate::backend::{opsreq::OpMsg, printconvert};
use crate::json::{escape, field_bool, field_str, field_usize};
use flea::bytename::ExactText as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

pub struct Ask {
    pub request_id: usize,
    pub menu_id: usize,
    pub paths: Vec<PathBuf>,
    pub format: String,
    pub strip: bool,
    pub combine: bool,
    pub pages: String,
}

impl Ask {
    pub fn parse(line: &str) -> Self {
        let combine = crate::jsondoc::parse(line)
            .ok()
            .and_then(|j| j.get("combine").and_then(|v| v.as_bool()))
            .unwrap_or(true);
        Self {
            request_id: field_usize(line, "requestId").unwrap_or(0),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
            paths: crate::backend::printreq::paths(line),
            format: field_str(line, "format")
                .unwrap_or_default()
                .to_ascii_lowercase(),
            strip: field_bool(line, "strip"),
            combine,
            pages: field_str(line, "pages").unwrap_or_else(|| "All".into()),
        }
    }
}

pub fn image_argv(inputs: &[PathBuf], output: &Path, strip: bool) -> Vec<String> {
    let mut args = vec!["magick".into()];
    args.extend(printconvert::MAGICK_LITERAL.map(String::from));
    let pdf = printconvert::extension(output) == "pdf";
    for input in inputs {
        // The page suffix joins the raw name's bytes, so the argument still decodes to that file.
        let mut source = input.as_os_str().to_owned();
        if pdf {
            source.push("[0]");
        }
        args.extend([
            "-read".into(),
            printconvert::magick_input(Path::new(&source)),
        ]);
    }
    if strip {
        args.push("-strip".into());
    }
    // A suffix can silently fall back to the input codec when its encoder is missing.
    args.push(format!(
        "{}:{}",
        printconvert::extension(output),
        printconvert::magick_output(output)
    ));
    args
}

pub fn pdf_argv(input: &Path, prefix: &Path, page: u32, format: &str) -> Vec<String> {
    let codec = match format {
        "jpg" => "-jpeg",
        "tiff" => "-tiff",
        _ => "-png",
    };
    [
        "pdftoppm",
        "-r",
        &PDF_DPI.to_string(),
        "-f",
        &page.to_string(),
        "-l",
        &page.to_string(),
        "-singlefile",
        codec,
        "--",
        &input.exact_text(),
        &prefix.exact_text(),
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect()
}

pub(crate) fn progress(
    tx: &Sender<OpMsg>,
    event: &str,
    id: usize,
    request_id: usize,
    done: usize,
    total: usize,
    path: &Path,
) {
    let _ = tx.send(OpMsg::Meta { line: format!(r#"{{"t":"{event}progress","id":{id},"requestId":{request_id},"done":{done},"total":{total},"name":"{}"}}"#,
        escape(&path.file_name().unwrap_or_default().to_string_lossy())) });
}

pub(crate) fn finished(
    event: &str,
    id: usize,
    request_id: usize,
    source: &str,
    paths: &[PathBuf],
    outcome: Result<(), String>,
) -> String {
    let err = outcome.err().unwrap_or_default();
    let outputs: Vec<_> = paths
        .iter()
        .map(|p| format!(r#""{}""#, escape(&p.exact_text())))
        .collect();
    format!(
        r#"{{"t":"{event}done","id":{id},"requestId":{request_id},"source":"{}","ok":{},"path":"{}","paths":[{}],"err":"{}","collision":{}}}"#,
        escape(source),
        err.is_empty(),
        escape(
            &paths
                .first()
                .map_or(String::new(), |p| p.exact_text().into_owned())
        ),
        outputs.join(","),
        escape(&err),
        err.contains("already exists")
    )
}

pub(super) fn image_outputs(ask: &Ask) -> Result<Vec<PathBuf>, String> {
    let paths = if ask.combine && ask.format == "pdf" {
        &ask.paths[..1]
    } else {
        &ask.paths
    };
    let mut seen = std::collections::HashSet::new();
    paths
        .iter()
        .map(|source| {
            let dest = destination(
                source,
                printconvert::converted_name(source, &ask.format),
                false,
            )?;
            if !seen.insert(dest.clone()) {
                return Err("The selected files would have the same output name.".into());
            }
            Ok(dest)
        })
        .collect()
}
