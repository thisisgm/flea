// Image layout math and ImageMagick commands are shared by the window and terminal cards.
use super::{aliases::Aliases, archivework::Work, convertbatch, menu_actions::Selected, mime::Db, opsreq::OpMsg, printconvert, printtool};
use crate::json::{field_str, field_usize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::Sender};

pub use flea::ui_logic_print_convert::composition::*;

pub struct Ask {
    pub request_id: usize,
    pub menu_id: usize,
    pub paths: Vec<PathBuf>,
    pub format: String,
    pub name: String,
    pub layout: String,
    pub spacing: String,
    pub background: String,
    pub columns: usize,
}

impl Ask {
    pub fn parse(line: &str) -> Self {
        let paths = super::printreq::paths(line);
        let format = field_str(line, "format").unwrap_or_else(|| printconvert::combined_format(paths.first().map_or(Path::new(""), PathBuf::as_path)).into()).to_ascii_lowercase();
        Self { request_id: field_usize(line, "requestId").unwrap_or(0), menu_id: field_usize(line, "menuId").unwrap_or(0), paths,
            background: field_str(line, "background").unwrap_or_else(|| if format == "jpg" { "White" } else { "Transparent" }.into()), format,
            name: field_str(line, "name").unwrap_or_default(), layout: field_str(line, "layout").unwrap_or_else(|| "Row".into()),
            spacing: field_str(line, "spacing").unwrap_or_else(|| "None".into()), columns: field_usize(line, "columns").unwrap_or(2) }
    }
}

fn make(ask: &Ask, settings: &Settings, work: &mut Work, cancel: &AtomicBool, id: usize, tx: &Sender<OpMsg>) -> Result<PathBuf, String> {
    let inputs = convertbatch::staged_inputs(&ask.paths, work)?;
    let mut identify = vec!["magick".into()];
    for input in &inputs { identify.extend(["--".into(), format!("{}[0]", input.display())]); }
    identify.extend(["-auto-orient".into(), "-format".into(), "%w %h\n".into(), "info:".into()]);
    let info = printtool::boxed(identify, work, cancel)?;
    // magick -auto-orient -format "%w %h\n" info: sample: 800 600
    let sizes = info.lines().map(|line| {
        let mut words = line.split_whitespace();
        let size = words.next().and_then(|w| w.parse().ok()).zip(words.next().and_then(|h| h.parse().ok()));
        size.filter(|_| words.next().is_none()).ok_or_else(|| "The image dimensions could not be used.".to_string())
    }).collect::<Result<Vec<_>, _>>()?;
    if sizes.len() != inputs.len() { return Err("The image dimensions could not be used.".into()); }
    let size = shared_size(settings.layout, &sizes)?;
    let mut scaled = Vec::new();
    for (n, input) in inputs.iter().enumerate() {
        convertbatch::progress(tx, "combine", id, ask.request_id, n, inputs.len() + 1, &ask.paths[n]);
        let output = work.dir.join(format!("scaled-{n}.png"));
        printtool::boxed(scale_argv(input, &output, size, settings, n == 0), work, cancel)?;
        scaled.push(output);
    }
    convertbatch::progress(tx, "combine", id, ask.request_id, inputs.len(), inputs.len() + 1, &ask.paths[0]);
    let output = work.dir.join(format!("output.{}", ask.format));
    for command in join_commands(&scaled, &output, size, settings) { printtool::boxed(command, work, cancel)?; }
    Ok(output)
}

pub fn run(ask: Ask, selection: Result<Option<Vec<Selected>>, String>, id: usize, mime: &Db, aliases: &Aliases, cancel: Arc<AtomicBool>, tx: &Sender<OpMsg>) -> String {
    let mut published = Vec::new();
    let outcome = (|| {
        let selected = selection?;
        super::menu_actions::validate_sources(selected.as_deref(), &ask.paths)?;
        printtool::inputs(&ask.paths)?;
        let kinds: Vec<_> = ask.paths.iter().map(|p| printconvert::classify(&p.to_string_lossy(), mime, aliases)).collect();
        if !printconvert::can_combine(&kinds) { return Err("Select at least two images to combine.".into()); }
        let settings = Settings::parse(&ask.layout, &ask.spacing, &ask.background, &ask.format, ask.columns)?;
        let parent = ask.paths[0].parent().ok_or("the source has no parent folder")?;
        if ask.paths.iter().any(|p| p.parent() != Some(parent)) { return Err("The selected images must be in one folder.".into()); }
        let name = if ask.name.is_empty() { printconvert::combined_name(&ask.paths[0], &ask.format) } else { ask.name.clone() };
        if !printconvert::bare_name(&name) || printconvert::extension(Path::new(&name)) != ask.format { return Err("Enter a file name with the chosen format's extension.".into()); }
        let dest = parent.join(name);
        convertbatch::vacant(&dest)?;
        let mut work = Work::new(parent, "combine").map_err(|e| e.msg)?;
        let staged = make(&ask, &settings, &mut work, &cancel, id, tx)?;
        if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
        printtool::publish(&staged, &dest)?;
        published.push(dest);
        convertbatch::progress(tx, "combine", id, ask.request_id, ask.paths.len() + 1, ask.paths.len() + 1, &published[0]);
        Ok(())
    })();
    convertbatch::finished("combine", id, ask.request_id, &ask.paths.first().map_or(String::new(), |p| p.to_string_lossy().into_owned()), &published, outcome)
}

#[cfg(test)]
#[path = "combine_tests.rs"]
mod tests;
