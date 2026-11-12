// Multi-image, PDF-page and office export jobs, published beside the source without replacement.
use super::{
    aliases::Aliases,
    archivework::Work,
    menu_actions::Selected,
    mime::Db,
    opsreq::OpMsg,
    pagerange::Pages,
    printconvert::{self, Kind},
    printtool,
    undo::{ItemIdentity, Step},
};
use flea::bytename::ExactText as _;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc,
};

pub const PDF_DPI: u32 = 150;
const MAX_INPUT_EXTENSION_BYTES: usize = 10;
const PROGRESS_TOTAL: usize = 100;
const CONVERSION_PROGRESS: usize = PROGRESS_TOTAL / 2;

#[path = "convertbatch_commands.rs"]
mod commands;
use commands::image_outputs;
pub(crate) use commands::{finished, progress};
pub use commands::{image_argv, pdf_argv, Ask};

// A staged result, the name it publishes under and the file it was made from.
type Staged = (PathBuf, PathBuf, PathBuf);

// A taken name is stepped past at publish, so only a name that cannot be read refuses before a tool runs.
fn taken(path: &Path) -> Result<bool, String> {
    match path.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(crate::error::io_message(&e)),
        Ok(_) => Ok(true),
    }
}

// Combine names its own output, so a taken name refuses there.
pub(crate) fn vacant(path: &Path) -> Result<(), String> {
    match taken(path)? {
        true => Err(format!(
            "{} already exists",
            path.file_name().unwrap_or_default().to_string_lossy()
        )),
        false => Ok(()),
    }
}

fn destination(source: &Path, name: String, collision_suffix: bool) -> Result<PathBuf, String> {
    let parent = source.parent().ok_or("the source has no parent folder")?;
    let mut output = parent.join(name);
    if collision_suffix && taken(&output)? {
        output = parent.join(printconvert::page_fallback(&output));
    }
    taken(&output)?;
    Ok(output)
}

// A taken output lands under Duplicate's next free copy name; the no-replace rename passes over one taken meanwhile too.
fn land(made: &Path, dest: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    for n in 0..=printconvert::COPY_TRIES {
        let target = printconvert::copy_path(dest, n)
            .ok_or("that name is taken and no copy name can be made from it")?;
        if taken(&target)? {
            continue;
        }
        match printtool::publish(made, &target, cancel) {
            Err(msg) if msg == "already exists" => {}
            outcome => return outcome.map(|()| target),
        }
    }
    Err(format!(
        "every copy name for {} is already taken",
        dest.file_name().unwrap_or_default().to_string_lossy()
    ))
}

// Undo checks only the output's identity, taken as it lands, so a source moved or deleted meanwhile still journals it.
fn step(from: &Path, to: &Path) -> Option<Step> {
    let created = ItemIdentity::inspect(to).ok()?;
    let source = ItemIdentity::inspect(from).unwrap_or_else(|_| created.clone());
    Some(super::undo::copied_as(from, to, source, created))
}

pub(crate) fn staged_inputs(paths: &[PathBuf], work: &Work) -> Result<Vec<PathBuf>, String> {
    paths
        .iter()
        .enumerate()
        .map(|(n, path)| {
            let ext = printconvert::extension(path);
            let ext = if ext.len() <= MAX_INPUT_EXTENSION_BYTES
                && ext.bytes().all(|b| b.is_ascii_alphanumeric())
            {
                ext.as_str()
            } else {
                "image"
            };
            let staged = work.dir.join(format!("input-{n}.{ext}"));
            printtool::stage(path, &staged)?;
            Ok(staged)
        })
        .collect()
}

pub(crate) fn image_results(
    output: &Path,
    dest: &Path,
    source: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let parent = output
        .parent()
        .ok_or("the staged output has no parent folder")?;
    let prefix = format!(
        "{}-",
        output.file_stem().unwrap_or_default().to_string_lossy()
    );
    let format = printconvert::extension(output);
    let suffix = format!(".{format}");
    let mut pages = Vec::new();
    for entry in std::fs::read_dir(parent).map_err(|e| crate::error::io_message(&e))? {
        let entry = entry.map_err(|e| crate::error::io_message(&e))?;
        let name = entry.file_name();
        // ImageMagick numbered output sample: output-0-2.png
        let Some(scene) = name
            .to_str()
            .and_then(|n| n.strip_prefix(&prefix))
            .and_then(|n| n.strip_suffix(&suffix))
        else {
            continue;
        };
        if scene.is_empty() || !scene.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let scene: usize = scene
            .parse()
            .map_err(|_| "the converter returned an invalid page number")?;
        pages.push((scene, entry.path()));
        if pages.len() > super::pagerange::MAX_PAGES as usize {
            return Err("The converter returned too many pages.".into());
        }
    }
    if output.symlink_metadata().is_ok() {
        if !pages.is_empty() {
            return Err("the converter returned conflicting output files".into());
        }
        return Ok(vec![(output.to_path_buf(), dest.to_path_buf())]);
    }
    if pages.is_empty() {
        return Err("the converter wrote nothing".into());
    }
    pages.sort_unstable_by_key(|(scene, _)| *scene);
    if pages.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("the converter returned a repeated page number".into());
    }
    pages
        .into_iter()
        .enumerate()
        .map(|(n, (_, staged))| {
            let name = printconvert::page_name(dest, (n + 1) as u32, &format);
            Ok((staged, destination(source, name, false)?))
        })
        .collect()
}

fn convert_images(
    ask: &Ask,
    work: &mut Work,
    cancel: &AtomicBool,
    id: usize,
    tx: &Sender<OpMsg>,
) -> Result<Vec<Staged>, String> {
    let outputs = image_outputs(ask)?;
    let count = outputs.len();
    let inputs = staged_inputs(&ask.paths, work)?;
    let mut staged = Vec::new();
    for (n, dest) in outputs.into_iter().enumerate() {
        let output = work.dir.join(format!("output-{n}.{}", ask.format));
        let sources = if ask.combine && ask.format == "pdf" {
            &inputs[..]
        } else {
            &inputs[n..n + 1]
        };
        progress(
            tx,
            "convert",
            id,
            ask.request_id,
            n * CONVERSION_PROGRESS / count,
            PROGRESS_TOTAL,
            &dest,
        );
        printtool::boxed(
            image_argv(sources, &output, ask.strip && ask.format != "pdf"),
            work,
            cancel,
        )?;
        staged.extend(
            image_results(&output, &dest, &ask.paths[n])?
                .into_iter()
                .map(|(made, dest)| (made, dest, ask.paths[n].clone())),
        );
    }
    Ok(staged)
}

fn convert_pdf(
    ask: &Ask,
    work: &mut Work,
    cancel: &AtomicBool,
    id: usize,
    tx: &Sender<OpMsg>,
) -> Result<Vec<Staged>, String> {
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
        } else {
            rendered
        };
        staged.push((output, dest, ask.paths[0].clone()));
    }
    Ok(staged)
}

fn convert_office(ask: &Ask, work: &Work, cancel: Arc<AtomicBool>) -> Result<Vec<Staged>, String> {
    let dest = destination(
        &ask.paths[0],
        printconvert::converted_name(&ask.paths[0], "pdf"),
        false,
    )?;
    let cached = super::officepdf::cached_pdf(&ask.paths[0], cancel)?;
    let output = work.dir.join("output.pdf");
    printtool::stage(&cached, &output)?;
    Ok(vec![(output, dest, ask.paths[0].clone())])
}

pub fn run(
    ask: Ask,
    selection: Result<Option<Vec<Selected>>, String>,
    id: usize,
    mime: &Db,
    aliases: &Aliases,
    cancel: Arc<AtomicBool>,
    tx: &Sender<OpMsg>,
) -> (String, Vec<Step>) {
    let mut published = Vec::new();
    let mut journalled = Vec::new();
    let outcome = (|| {
        let selected = selection?;
        super::menu_actions::validate_sources(selected.as_deref(), &ask.paths)?;
        printtool::inputs(&ask.paths)?;
        let kinds: Vec<_> = ask
            .paths
            .iter()
            .map(|p| printconvert::classify(&p.to_string_lossy(), mime, aliases))
            .collect();
        if !printconvert::can_convert(&kinds) {
            return Err("Select images, one PDF or one office document to convert.".into());
        }
        if !kinds[0].formats().contains(&ask.format.as_str()) {
            return Err("Choose a supported output format.".into());
        }
        Pages::parse(&ask.pages)?;
        let parent = ask.paths[0]
            .parent()
            .ok_or("the source has no parent folder")?;
        if ask.paths.iter().any(|p| p.parent() != Some(parent)) {
            return Err("The selected files must be in one folder.".into());
        }
        let leaf = ask.paths[0]
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // The inputs are copied into the stage beside the outputs, so it holds them twice over.
        let mut work =
            super::archivework::workhome::work_for(parent, "convert", "convert", &leaf, || {
                super::archivework::workhome::files_bytes(&ask.paths).saturating_mul(2)
            })
            .map_err(|e| e.msg)?;
        let staged = match kinds[0] {
            Kind::Image => convert_images(&ask, &mut work, &cancel, id, tx)?,
            Kind::Pdf => convert_pdf(&ask, &mut work, &cancel, id, tx)?,
            Kind::Office => convert_office(&ask, &work, Arc::clone(&cancel))?,
            _ => unreachable!(),
        };
        let mut destinations = std::collections::HashSet::new();
        for (made, dest, _) in &staged {
            let meta = made
                .symlink_metadata()
                .map_err(|_| "the converter wrote nothing")?;
            if !meta.is_file() || meta.len() == 0 {
                return Err("the converter wrote no regular output file".into());
            }
            if !destinations.insert(dest) {
                return Err("The selected files would have the same output name.".into());
            }
        }
        let count = staged.len();
        progress(
            tx,
            "convert",
            id,
            ask.request_id,
            CONVERSION_PROGRESS,
            PROGRESS_TOTAL,
            &ask.paths[0],
        );
        for (made, dest, from) in staged {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let landed = land(&made, &dest, &cancel)?;
            journalled.extend(step(&from, &landed));
            published.push(landed);
            let done = CONVERSION_PROGRESS
                + published.len() * (PROGRESS_TOTAL - CONVERSION_PROGRESS) / count;
            progress(
                tx,
                "convert",
                id,
                ask.request_id,
                done,
                PROGRESS_TOTAL,
                published.last().unwrap(),
            );
        }
        Ok(())
    })();
    let line = finished(
        "convert",
        id,
        ask.request_id,
        &ask.paths
            .first()
            .map_or(String::new(), |p| p.exact_text().into_owned()),
        &published,
        outcome,
    );
    // What landed stays journalled when a later output fails or a cancel stops the rest.
    (line, journalled)
}

#[cfg(test)]
#[path = "convertbatch_tests.rs"]
mod tests;
