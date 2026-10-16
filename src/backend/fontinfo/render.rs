// The sample job: one magick run in the archive jail draws three PNGs, cached by file, mtime, colours and sizes.
use crate::backend::aliases::Aliases;
use crate::backend::archivework::{run_boxed_cancellable_capped, Work};
use crate::backend::convert;
use crate::backend::fontinfo::cache::{cached, entry, is_png, key, prune, root as cache_root, WORK_TAG};
use crate::backend::fontinfo::{self, Facts};
use crate::backend::iomount;
use crate::backend::mime::Db;
use crate::backend::opsreq::{op_err, OpMsg};
use crate::error::FleaError;
use crate::json::{escape, field_str, field_usize, field_usize_array};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

const MAGICK: &str = "magick";
// TrueType and OpenType (font/ttf, font/otf) and the collection type, by canonical name after aliases.
const FONT_TYPES: [&str; 3] = ["font/ttf", "font/otf", "font/collection"];
pub const SAMPLES: usize = 3;
const MIN_PX: usize = 6;
const MAX_PX: usize = 400;
const MAX_TEXT_BYTES: usize = 200;
// A cold magick run is well under a second; this bounds a font on a mount that never answers.
const JOB_WAIT: Duration = Duration::from_secs(15);
const WATCH_STEP: Duration = Duration::from_millis(50);

pub struct Ask {
    pub id: usize,
    pub path: PathBuf,
    pub fg: String,
    pub bg: String,
    pub sizes: Vec<usize>,
    pub text: String,
}

impl Ask {
    // Sample input: {"c":"fontlook","id":4,"path":"/f/Inter-Bold.ttf","fg":"#c0caf5","bg":"#1a1b26","sizes":[33,22,18],"text":"The quick brown fox"}
    pub fn parse(line: &str) -> Ask {
        Ask {
            id: field_usize(line, "id").unwrap_or(0),
            path: PathBuf::from(field_str(line, "path").unwrap_or_default()),
            fg: field_str(line, "fg").unwrap_or_default(),
            bg: field_str(line, "bg").unwrap_or_default(),
            sizes: field_usize_array(line, "sizes"),
            text: field_str(line, "text").unwrap_or_default(),
        }
    }
}

// Sample output: {"t":"fontlooked","id":4,"family":"Inter","style":"Bold","glyphs":2548,"files":["/c/a-0.png","/c/a-1.png","/c/a-2.png"]}
pub fn looked_line(id: usize, facts: &Facts, files: &[PathBuf]) -> String {
    let mut line = format!(r#"{{"t":"fontlooked","id":{}"#, id);
    for (key, value) in [("family", &facts.family), ("style", &facts.style)] {
        if let Some(text) = value {
            line.push_str(&format!(r#","{}":"{}""#, key, escape(text)));
        }
    }
    if let Some(n) = facts.glyphs {
        line.push_str(&format!(r#","glyphs":{}"#, n));
    }
    let files: Vec<String> = files.iter().map(|f| format!(r#""{}""#, escape(&f.to_string_lossy()))).collect();
    line.push_str(&format!(r#","files":[{}]}}"#, files.join(",")));
    line
}

// Sample output: {"t":"fontlooked","id":4,"err":"ImageMagick is not installed"}
pub fn failed_line(id: usize, err: &str) -> String {
    format!(r#"{{"t":"fontlooked","id":{},"err":"{}"}}"#, id, escape(err))
}

fn hex_colour(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

// label: reads a file for a leading @, expands backslash escapes, and expands percent escapes, so those three are closed here.
fn label_text(text: &str) -> Result<String, String> {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES || text.starts_with('@') || text.chars().any(|c| c.is_control() || c == '\\') {
        return Err("that sample text cannot be drawn".to_string());
    }
    Ok(text.replace('%', "%%"))
}

fn check(ask: &Ask) -> Result<String, String> {
    if !ask.path.is_absolute() {
        return Err("a font sample takes an absolute path".to_string());
    }
    if !hex_colour(&ask.fg) || !hex_colour(&ask.bg) {
        return Err("a font sample takes #rrggbb colours".to_string());
    }
    if ask.sizes.len() != SAMPLES || ask.sizes.iter().any(|px| !(MIN_PX..=MAX_PX).contains(px)) {
        return Err(format!("a font sample takes {} sizes from {} to {} pixels", SAMPLES, MIN_PX, MAX_PX));
    }
    label_text(&ask.text)
}

// One process, three images: -write then +delete is how magick makes several files from one run.
pub fn argv(program: &str, font: &Path, ask: &Ask, label: &str, out: &[PathBuf]) -> Vec<String> {
    let mut a: Vec<String> = [program, "-background", &ask.bg, "-fill", &ask.fg, "-font"].iter().map(|s| s.to_string()).collect();
    a.push(font.to_string_lossy().to_string());
    for (i, px) in ask.sizes.iter().enumerate() {
        a.push("-pointsize".to_string());
        a.push(px.to_string());
        a.push(format!("label:{}", label));
        if i + 1 < ask.sizes.len() {
            a.push("-write".to_string());
            a.push(out[i].to_string_lossy().to_string());
            a.push("+delete".to_string());
        } else {
            a.push(out[i].to_string_lossy().to_string());
        }
    }
    a
}

// The one runner: the archive jail, the CPU cap, and a wall-clock cancel for a font that never reads.
fn jail(inner: &[String], font: &Path, work: &mut Work) -> Result<(), FleaError> {
    if !convert::available() {
        return Err(op_err("font", MAGICK, "ImageMagick is not installed"));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let watch = {
        let (cancel, finished) = (Arc::clone(&cancel), Arc::clone(&finished));
        std::thread::spawn(move || {
            let until = std::time::Instant::now() + JOB_WAIT;
            while !finished.load(Ordering::Relaxed) {
                if std::time::Instant::now() >= until {
                    cancel.store(true, Ordering::Relaxed);
                    return;
                }
                std::thread::sleep(WATCH_STEP);
            }
        })
    };
    let result = run_boxed_cancellable_capped("font", inner.to_vec(), font, work, &cancel);
    finished.store(true, Ordering::Relaxed);
    let _ = watch.join();
    result
}

type Runner<'a> = dyn FnMut(&[String], &Path, &mut Work) -> Result<(), FleaError> + 'a;

// The reply line for one ask. run draws the three files into the work folder it is given; a hit never calls it.
pub fn answer(ask: &Ask, mime: &Db, aliases: &Aliases, root: &Path, program: &str, run: &mut Runner) -> String {
    match look(ask, mime, aliases, root, program, run) {
        Ok((facts, files)) => looked_line(ask.id, &facts, &files),
        Err(err) => failed_line(ask.id, &err),
    }
}

fn look(ask: &Ask, mime: &Db, aliases: &Aliases, root: &Path, program: &str, run: &mut Runner) -> Result<(Facts, Vec<PathBuf>), String> {
    let label = check(ask)?;
    let name = ask.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let kind = mime.lookup(&name).map(|m| aliases.canonical(m));
    // shared-mime-info assigns *.otf to both OpenType and an OpenDocument formula template
    // at weight 50. The latter can win by file order; only actual font tables admit that case.
    let ambiguous_otf = name.to_ascii_lowercase().ends_with(".otf")
        && kind == Some("application/vnd.oasis.opendocument.formula-template");
    if !ambiguous_otf && !kind.is_some_and(|m| FONT_TYPES.contains(&m)) {
        return Err("only TrueType and OpenType fonts are drawn".to_string());
    }
    // Absolute and canonical, so magick can never read the font as an option and the jail binds the file itself.
    let (font, meta, facts) = iomount::call(&ask.path, &iomount::mount_body(), "font", {
        let path = ask.path.clone();
        move || {
            let font = std::fs::canonicalize(&path).map_err(|e| crate::error::io_message(&e))?;
            let meta = std::fs::metadata(&font).map_err(|e| crate::error::io_message(&e))?;
            if !meta.is_file() {
                return Err("that is not a regular file".to_string());
            }
            let facts = fontinfo::read(&font);
            if ambiguous_otf && facts == Facts::default() {
                return Err("that file has no readable font tables".to_string());
            }
            Ok((font, meta, facts))
        }
    })
    .map_err(|e| e.msg)??;
    let key = key(&font, &meta, ask);
    if let Some(files) = cached(root, &key) {
        return Ok((facts, files));
    }
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(root).map_err(|e| crate::error::io_message(&e))?;
    let mut work = Work::new(root, WORK_TAG).map_err(|e| e.msg)?;
    let drawn: Vec<PathBuf> = (0..SAMPLES).map(|i| work.dir.join(format!("{}.png", i))).collect();
    run(&argv(program, &font, ask, &label, &drawn), &font, &mut work).map_err(|e| e.msg)?;
    if !drawn.iter().all(|f| is_png(f)) {
        return Err("ImageMagick did not draw the sample".to_string());
    }
    let mut files = Vec::new();
    for (i, from) in drawn.iter().enumerate() {
        let to = entry(root, &key, i);
        std::fs::rename(from, &to).map_err(|e| crate::error::io_message(&e))?;
        files.push(to);
    }
    prune(root);
    Ok((facts, files))
}

// On its own thread: a font on a hung mount or a slow render never holds the request loop.
pub fn spawn(ask: Ask, mime: Arc<Db>, aliases: Arc<Aliases>, tx: Sender<OpMsg>) {
    std::thread::spawn(move || {
        let line = answer(&ask, &mime, &aliases, &cache_root(), MAGICK, &mut jail);
        let _ = tx.send(OpMsg::Meta { line });
    });
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
