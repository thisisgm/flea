//! What Quick Look prepares ahead of an open: which rows read inline, the small pictures and formulas, and the one shared parse.

use super::markdown_model::{Block, FigureKind};

/// The one shared parse holds a text of at most this many bytes, and only a file this small reads inside the key.
pub const MAX_BYTES: u64 = 65536;
/// A picture at most this big is decoded ahead of the open.
pub const PICTURE_MAX_BYTES: u64 = 262_144;
/// A document is asked about at most this many pictures.
pub const PICTURE_LIMIT: usize = 8;
/// The size check of the pictures is dropped after this long, so a hung mount holds nothing past it.
pub const PICTURE_SIZE_DEADLINE_MS: u64 = 1500;
/// The most formulas a rested document typesets ahead.
pub const FORMULA_LIMIT: usize = 8;
/// The name endings a Markdown document carries, compared without case.
pub const MARKDOWN_EXTENSIONS: [&str; 3] = [".md", ".markdown", ".mkd"];

const S_IFMT: u32 = 0o170_000;
const S_IFREG: u32 = 0o100_000;
const FILE_SCHEME_LENGTH: usize = 7;

/// "notes.MD" is Markdown; ".md" alone and "notes.txt" are not.
pub fn is_markdown_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    MARKDOWN_EXTENSIONS
        .iter()
        .any(|ext| lower.len() > ext.len() && lower.ends_with(ext))
}

/// The cursor row as the preparation reads it: its name, whether it is a folder, its mode and its size.
#[derive(Clone, Copy, Debug)]
pub struct Row<'r> {
    pub name: &'r str,
    pub dir: bool,
    pub mode: u32,
    pub size: u64,
}

/// Whether the cursor row is a Markdown file by name, the one trigger Quick Look's warm and its build ahead of Space share.
pub fn is_markdown_row(row: Option<&Row>) -> bool {
    row.is_some_and(|row| !row.dir && is_markdown_name(row.name))
}

/// Only a regular local file of at most MAX_BYTES in a folder whose storage class has landed reads ahead or inside the key.
pub fn reads_inline(row: Option<&Row>, storage_class: &str, storage_known: bool) -> bool {
    let Some(row) = row else { return false };
    let removable = matches!(storage_class, "network" | "phone" | "usb");
    if row.dir || !storage_known || removable || !is_markdown_name(row.name) {
        return false;
    }
    row.mode & S_IFMT == S_IFREG && row.size > 0 && row.size <= MAX_BYTES
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

/// "file:///d/b%20c.png" answers "/d/b c.png"; a stray percent or an escape that is not UTF-8 answers "".
pub fn path_of_url(url: &str) -> String {
    let from = url
        .char_indices()
        .nth(FILE_SCHEME_LENGTH)
        .map_or(url.len(), |(at, _)| at);
    let bytes = &url.as_bytes()[from..];
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        match (
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            (Some(high), Some(low)) => out.push(high * 16 + low),
            _ => return String::new(),
        }
        i += 3;
    }
    String::from_utf8(out).unwrap_or_default()
}

/// "/d/img/a.png" is inside "/d"; "/mnt/nas/a.png" and "/d/../e/a.png" are not.
pub fn inside_dir(path: &str, dir: &str) -> bool {
    !dir.is_empty()
        && path.starts_with(&format!("{dir}/"))
        && !path.split('/').any(|part| part == "..")
}

/// The local pictures a parsed document draws at its top level under its own folder, each once, in reading order, at most limit.
pub fn picture_urls(blocks: &[Block], limit: usize, dir: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    let mut add = |url: &str| {
        if url.starts_with("file://")
            && urls.len() < limit
            && !urls.iter().any(|u| u == url)
            && inside_dir(&path_of_url(url), dir)
        {
            urls.push(url.to_string());
        }
    };
    for block in blocks {
        match block {
            Block::Image(image) => add(&image.url),
            Block::Images { items, .. } => items.iter().for_each(|image| add(&image.url)),
            _ => {}
        }
    }
    urls
}

/// The pictures the size check found at PICTURE_MAX_BYTES or less; one it did not print (a missing file) or a bigger one is left out.
pub fn small_pictures(urls: &[String], stat_text: &str) -> Vec<String> {
    let mut sizes = std::collections::HashMap::new();
    for line in stat_text.split('\n') {
        if let Some(tab) = line.find('\t').filter(|tab| *tab > 0) {
            sizes.insert(&line[tab + 1..], js_number(&line[..tab]));
        }
    }
    urls.iter()
        .filter(|url| {
            sizes
                .get(path_of_url(url).as_str())
                .is_some_and(|size| *size > 0.0 && *size <= PICTURE_MAX_BYTES as f64)
        })
        .cloned()
        .collect()
}

// A decimal size as Number() reads it: blank is 0 and anything else unreadable is NaN, which no test passes.
fn js_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse().unwrap_or(f64::NAN)
}

/// A formula a prepared document draws: its source and whether it is set as a display block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formula {
    pub source: String,
    pub display: bool,
}

/// The distinct formulas a document draws, in reading order and at most limit: inline ones and display figures, held ones included.
pub fn formulas_in(blocks: &[Block], limit: usize) -> Vec<Formula> {
    let mut out: Vec<Formula> = Vec::new();
    walk_formulas(blocks, limit, &mut out);
    out
}

fn add_formula(out: &mut Vec<Formula>, limit: usize, source: &str, display: bool) {
    if out.len() < limit
        && !out
            .iter()
            .any(|f| f.display == display && f.source == source)
    {
        out.push(Formula {
            source: source.to_string(),
            display,
        });
    }
}

fn walk_formulas(blocks: &[Block], limit: usize, out: &mut Vec<Formula>) {
    for block in blocks {
        match block {
            Block::Figure(figure) if figure.kind == FigureKind::Math => {
                add_formula(out, limit, &figure.source, true)
            }
            Block::Run { maths, .. } => maths
                .iter()
                .for_each(|source| add_formula(out, limit, source, false)),
            Block::Quote(quote) => {
                walk_formulas(quote.parts.as_deref().unwrap_or_default(), limit, out)
            }
            Block::List(list) => list
                .parts
                .iter()
                .flatten()
                .flatten()
                .for_each(|parts| walk_formulas(parts, limit, out)),
            _ => {}
        }
    }
}

/// A colour's channels from 0 to 1 as the "#rrggbb" a style attribute takes: 0.0627, 0.0745, 0.0745 answers "#101313".
pub fn hex_of(r: f32, g: f32, b: f32) -> String {
    let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", channel(r), channel(g), channel(b))
}

/// The inputs one parse ran on: the path, the text and the folder, chrome and ink it was parsed under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inputs {
    pub path: String,
    pub text: String,
    pub dir: String,
    pub chrome: String,
    pub ink: String,
}

/// The one parsed document Quick Look takes without parsing, keyed by its text so an edited file misses.
#[derive(Debug, Default)]
pub struct Shared {
    entry: Option<(Inputs, Vec<Block>)>,
}

impl Shared {
    /// Holds a parse; a text past MAX_BYTES is never held.
    pub fn store(&mut self, inputs: Inputs, blocks: Vec<Block>) {
        if inputs.text.len() as u64 <= MAX_BYTES {
            self.entry = Some((inputs, blocks));
        }
    }

    /// The blocks parsed from exactly these inputs, or None.
    pub fn take(&self, inputs: &Inputs) -> Option<&[Block]> {
        self.entry
            .as_ref()
            .filter(|(held, _)| held == inputs)
            .map(|(_, blocks)| blocks.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(path: &str, text: &str, dir: &str, chrome: &str, ink: &str) -> Inputs {
        Inputs {
            path: path.into(),
            text: text.into(),
            dir: dir.into(),
            chrome: chrome.into(),
            ink: ink.into(),
        }
    }

    // Ported from tests/js/mdprepared.js: the one held parse answers only the same file, text, folder, chrome and ink.
    #[test]
    fn the_held_parse_answers_only_the_same_request() {
        let text = "# hello\n";
        let blocks = vec![Block::run("hello")];
        let mut shared = Shared::default();
        let ask = |path, text, dir, chrome, ink| inputs(path, text, dir, chrome, ink);
        assert!(shared
            .take(&ask("/d/a.md", text, "/d/", "#101315", "#c0caf5"))
            .is_none());
        shared.store(
            ask("/d/a.md", text, "/d/", "#101315", "#c0caf5"),
            blocks.clone(),
        );
        let held = shared.take(&ask("/d/a.md", text, "/d/", "#101315", "#c0caf5"));
        assert_eq!(held, Some(blocks.as_slice()));
        let edited = format!("{text}more\n");
        for miss in [
            ask("/d/a.md", &edited, "/d/", "#101315", "#c0caf5"),
            ask("/d/b.md", text, "/d/", "#101315", "#c0caf5"),
            ask("/d/a.md", text, "/e/", "#101315", "#c0caf5"),
            ask("/d/a.md", text, "/d/", "#181825", "#c0caf5"),
            ask("/d/a.md", text, "/d/", "#101315", "#ffffff"),
        ] {
            assert!(shared.take(&miss).is_none(), "{} misses", miss.path);
        }
        let big = "x".repeat(MAX_BYTES as usize + 1);
        shared.store(ask("/d/big.md", &big, "/d/", "#101315", "#c0caf5"), blocks);
        assert!(shared
            .take(&ask("/d/big.md", &big, "/d/", "#101315", "#c0caf5"))
            .is_none());
    }
}
