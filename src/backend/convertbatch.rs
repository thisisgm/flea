// Multi-image, PDF-page and office export jobs, published beside the source without replacement.
use flea::bytename::ExactText as _;
use super::{aliases::Aliases, archivework::Work, menu_actions::Selected, mime::Db, opsreq::OpMsg, pagerange::Pages, printconvert::{self, Kind}, printtool};
use crate::json::{escape, field_bool, field_str, field_usize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::Sender};

pub const PDF_DPI: u32 = 150;
const MAX_INPUT_EXTENSION_BYTES: usize = 10;
const PROGRESS_TOTAL: usize = 100;
const CONVERSION_PROGRESS: usize = PROGRESS_TOTAL / 2;

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
        let combine = crate::jsondoc::parse(line).ok().and_then(|j| j.get("combine").and_then(|v| v.as_bool())).unwrap_or(true);
        Self { request_id: field_usize(line, "requestId").unwrap_or(0), menu_id: field_usize(line, "menuId").unwrap_or(0), paths: super::printreq::paths(line),
            format: field_str(line, "format").unwrap_or_default().to_ascii_lowercase(), strip: field_bool(line, "strip"), combine,
            pages: field_str(line, "pages").unwrap_or_else(|| "All".into()) }
    }
}

pub fn image_argv(inputs: &[PathBuf], output: &Path, strip: bool) -> Vec<String> {
    let mut args = vec!["magick".into()];
    let pdf = printconvert::extension(output) == "pdf";
    for input in inputs {
        // The page suffix joins the raw name's bytes, so the argument still decodes to that file.
        let mut source = input.as_os_str().to_owned();
        if pdf {
            source.push("[0]");
        }
        args.extend([
            "--".into(),
            flea::bytename::path_text(Path::new(&source)).into_owned(),
        ]);
    }
    if strip {
        args.push("-strip".into());
    }
    // A suffix can silently fall back to the input codec when its encoder is missing.
    args.push(format!(
        "{}:{}",
        printconvert::extension(output),
        output.exact_text()
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

pub(crate) fn vacant(path: &Path) -> Result<(), String> {
    match path.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(crate::error::io_message(&e)),
        Ok(_) => Err(format!("{} already exists", path.file_name().unwrap_or_default().to_string_lossy())),
    }
}

fn destination(source: &Path, name: String, collision_suffix: bool) -> Result<PathBuf, String> {
    let parent = source.parent().ok_or("the source has no parent folder")?;
    let mut output = parent.join(name);
    if collision_suffix && vacant(&output).is_err() {
        output = parent.join(printconvert::page_fallback(&output));
    }
    vacant(&output)?;
    Ok(output)
}

pub(crate) fn staged_inputs(paths: &[PathBuf], work: &Work) -> Result<Vec<PathBuf>, String> {
    paths.iter().enumerate().map(|(n, path)| {
        let ext = printconvert::extension(path);
        let ext = if ext.len() <= MAX_INPUT_EXTENSION_BYTES && ext.bytes().all(|b| b.is_ascii_alphanumeric()) { ext.as_str() } else { "image" };
        let staged = work.dir.join(format!("input-{n}.{ext}"));
        printtool::stage(path, &staged)?;
        Ok(staged)
    }).collect()
}

fn image_outputs(ask: &Ask) -> Result<Vec<PathBuf>, String> {
    let paths = if ask.combine && ask.format == "pdf" { &ask.paths[..1] } else { &ask.paths };
    let mut seen = std::collections::HashSet::new();
    paths.iter().map(|source| {
        let dest = destination(source, printconvert::converted_name(source, &ask.format), false)?;
        if !seen.insert(dest.clone()) { return Err("The selected files would have the same output name.".into()); }
        Ok(dest)
    }).collect()
}

pub(crate) fn image_results(output: &Path, dest: &Path, source: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let parent = output.parent().ok_or("the staged output has no parent folder")?;
    let prefix = format!("{}-", output.file_stem().unwrap_or_default().to_string_lossy());
    let format = printconvert::extension(output);
    let suffix = format!(".{format}");
    let mut pages = Vec::new();
    for entry in std::fs::read_dir(parent).map_err(|e| crate::error::io_message(&e))? {
        let entry = entry.map_err(|e| crate::error::io_message(&e))?;
        let name = entry.file_name();
        // ImageMagick numbered output sample: output-0-2.png
        let Some(scene) = name.to_str().and_then(|n| n.strip_prefix(&prefix)).and_then(|n| n.strip_suffix(&suffix)) else { continue; };
        if scene.is_empty() || !scene.bytes().all(|b| b.is_ascii_digit()) { continue; }
        let scene: usize = scene.parse().map_err(|_| "the converter returned an invalid page number")?;
        pages.push((scene, entry.path()));
        if pages.len() > super::pagerange::MAX_PAGES as usize { return Err("The converter returned too many pages.".into()); }
    }
    if output.symlink_metadata().is_ok() {
        if !pages.is_empty() { return Err("the converter returned conflicting output files".into()); }
        return Ok(vec![(output.to_path_buf(), dest.to_path_buf())]);
    }
    if pages.is_empty() { return Err("the converter wrote nothing".into()); }
    pages.sort_unstable_by_key(|(scene, _)| *scene);
    if pages.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err("the converter returned a repeated page number".into()); }
    pages.into_iter().enumerate().map(|(n, (_, staged))| {
        let name = printconvert::page_name(dest, (n + 1) as u32, &format);
        Ok((staged, destination(source, name, false)?))
    }).collect()
}

fn convert_images(ask: &Ask, work: &mut Work, cancel: &AtomicBool, id: usize, tx: &Sender<OpMsg>) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let outputs = image_outputs(ask)?;
    let count = outputs.len();
    let inputs = staged_inputs(&ask.paths, work)?;
    let mut staged = Vec::new();
    for (n, dest) in outputs.into_iter().enumerate() {
        let output = work.dir.join(format!("output-{n}.{}", ask.format));
        let sources = if ask.combine && ask.format == "pdf" { &inputs[..] } else { &inputs[n..n + 1] };
        progress(tx, "convert", id, ask.request_id, n * CONVERSION_PROGRESS / count, PROGRESS_TOTAL, &dest);
        printtool::boxed(image_argv(sources, &output, ask.strip && ask.format != "pdf"), work, cancel)?;
        staged.extend(image_results(&output, &dest, &ask.paths[n])?);
    }
    Ok(staged)
}

fn convert_pdf(
    ask: &Ask,
    work: &mut Work,
    cancel: &AtomicBool,
    id: usize,
    tx: &Sender<OpMsg>,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let input = work.dir.join("input.pdf");
    printtool::stage(&ask.paths[0], &input)?;
    let info = printtool::boxed(
        vec![
            "pdfinfo".into(),
            "--".into(),
            input.exact_text().into_owned(),
        ],
        work,
        cancel,
    )?;
    // pdfinfo sample: Pages:           3
    let count = info
        .lines()
        .find_map(|l| l.strip_prefix("Pages:").and_then(|s| s.trim().parse().ok()))
        .ok_or("the PDF page count could not be read")?;
    let pages = Pages::parse(&ask.pages)?.selected(count)?;
    let outputs = pages
        .iter()
        .map(|page| {
            destination(
                &ask.paths[0],
                printconvert::page_name(&ask.paths[0], *page, &ask.format),
                true,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut staged = Vec::new();
    for (n, (page, dest)) in pages.iter().zip(outputs).enumerate() {
        progress(
            tx,
            "convert",
            id,
            ask.request_id,
            n * CONVERSION_PROGRESS / pages.len(),
            PROGRESS_TOTAL,
            &dest,
        );
        let prefix = work.dir.join(format!("page-{page}"));
        printtool::boxed(pdf_argv(&input, &prefix, *page, &ask.format), work, cancel)?;
        let rendered = prefix.with_extension(match ask.format.as_str() {
            "webp" => "png",
            "tiff" => "tif",
            format => format,
        });
        let output = if ask.format == "webp" {
            let webp = prefix.with_extension("webp");
            printtool::boxed(image_argv(&[rendered], &webp, false), work, cancel)?;
            webp
        } else { rendered };
        staged.push((output, dest));
    }
    Ok(staged)
}

fn convert_office(ask: &Ask, work: &Work, cancel: Arc<AtomicBool>) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let dest = destination(&ask.paths[0], printconvert::converted_name(&ask.paths[0], "pdf"), false)?;
    let cached = super::officepdf::cached_pdf(&ask.paths[0], cancel)?;
    let output = work.dir.join("output.pdf");
    printtool::stage(&cached, &output)?;
    Ok(vec![(output, dest)])
}

pub fn run(ask: Ask, selection: Result<Option<Vec<Selected>>, String>, id: usize, mime: &Db, aliases: &Aliases, cancel: Arc<AtomicBool>, tx: &Sender<OpMsg>) -> String {
    let mut published = Vec::new();
    let outcome = (|| {
        let selected = selection?;
        super::menu_actions::validate_sources(selected.as_deref(), &ask.paths)?;
        printtool::inputs(&ask.paths)?;
        let kinds: Vec<_> = ask.paths.iter().map(|p| printconvert::classify(&p.to_string_lossy(), mime, aliases)).collect();
        if !printconvert::can_convert(&kinds) { return Err("Select images, one PDF or one office document to convert.".into()); }
        if !kinds[0].formats().contains(&ask.format.as_str()) { return Err("Choose a supported output format.".into()); }
        Pages::parse(&ask.pages)?;
        let parent = ask.paths[0].parent().ok_or("the source has no parent folder")?;
        if ask.paths.iter().any(|p| p.parent() != Some(parent)) { return Err("The selected files must be in one folder.".into()); }
        let mut work = Work::new(parent, "convert").map_err(|e| e.msg)?;
        let staged = match kinds[0] {
            Kind::Image => convert_images(&ask, &mut work, &cancel, id, tx)?,
            Kind::Pdf => convert_pdf(&ask, &mut work, &cancel, id, tx)?,
            Kind::Office => convert_office(&ask, &work, Arc::clone(&cancel))?,
            _ => unreachable!(),
        };
        let mut destinations = std::collections::HashSet::new();
        for (source, dest) in &staged {
            let meta = source.symlink_metadata().map_err(|_| "the converter wrote nothing")?;
            if !meta.is_file() || meta.len() == 0 { return Err("the converter wrote no regular output file".into()); }
            if !destinations.insert(dest) { return Err("The selected files would have the same output name.".into()); }
            vacant(dest)?;
        }
        let count = staged.len();
        progress(tx, "convert", id, ask.request_id, CONVERSION_PROGRESS, PROGRESS_TOTAL, &ask.paths[0]);
        for (source, dest) in staged {
            if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
            printtool::publish(&source, &dest)?;
            published.push(dest);
            let done = CONVERSION_PROGRESS + published.len() * (PROGRESS_TOTAL - CONVERSION_PROGRESS) / count;
            progress(tx, "convert", id, ask.request_id, done, PROGRESS_TOTAL, published.last().unwrap());
        }
        Ok(())
    })();
    finished(
        "convert",
        id,
        ask.request_id,
        &ask.paths
            .first()
            .map_or(String::new(), |p| p.exact_text().into_owned()),
        &published,
        outcome,
    )
}

#[cfg(test)]
#[path = "convertbatch_tests.rs"]
mod tests;
