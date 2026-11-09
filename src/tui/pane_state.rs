use super::model::{Model, Row};
use super::wire::{count, flag, text};
use crate::jsondoc::Json;
use flea::ui_logic_doc_preview::office::Group;
use std::path::{Component, Path, PathBuf};

// What the preview knows beyond its row: an archive's contents, a can't-show sentence, syntax runs and a half-block picture.
#[derive(Default)]
pub struct State {
    pub i18n: crate::i18n::I18n,
    pub text: Option<super::preview::words::Text>,
    pub refusal: Option<&'static str>,
    pub unshown_key: Option<&'static str>,
    pub media: Option<(usize, usize, usize, usize)>,
    pub facts: Vec<super::preview::words::Fact>,
    pub archive: Option<Archive>,
    pub unshown: Option<Unshown>,
    // None keeps the error alert; an Office subject names its own glyph and tone.
    pub unshown_mark: Option<Mark>,
    pub code: super::syntax::Code,
    pub still: super::still::Still,
    // The terminal's cell in pixels, as the input loop last read it, for fitting a picture into cells.
    pub cell: Option<(usize, usize)>,
    // Whether the open Quick Look already asked the compositor for its corner rounding.
    pub probed: bool,
    // The half-block colours the terminal takes, none at sixteen colours, and the ground a page is veiled with.
    pub colors: Option<super::chafa::ColorMode>,
    pub ground: String,
    // The Office subject: its shared request state and the held identity behind its converted PDF.
    pub office: super::office::Preview,
}

#[cfg(test)]
impl State {
    // Tests read English only; the fields are spelled out because Self::default() would still load the host's language.
    pub fn english() -> Self {
        Self {
            i18n: crate::i18n::test_catalogs::english(),
            text: None,
            refusal: None,
            unshown_key: None,
            media: None,
            facts: Vec::new(),
            archive: None,
            unshown: None,
            unshown_mark: None,
            code: Default::default(),
            still: Default::default(),
            cell: None,
            probed: false,
            colors: None,
            ground: String::new(),
            office: Default::default(),
        }
    }
}

// An archive's exact totals and the entries the backend named, each with its size when one is known.
pub struct Archive {
    pub entries: usize,
    pub unpacked: usize,
    pub rows: Vec<(Row, Option<usize>)>,
}

// The can't-show state: one sentence under the alert mark, and the Open With button when a handler can take the file.
pub struct Unshown {
    pub sentence: String,
    pub open_with: bool,
}

// The glyph and tone of a can't-show sentence: an error mark, or a calm glyph for a file that is merely unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    pub glyph: &'static str,
    pub error: bool,
}

// What the preview shows for a row, from the classification the listing already made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Image,
    Pdf,
    Audio,
    Video,
    Archive,
    Model3d,
    Office,
    Data,
}

pub fn kind(row: &Row) -> Kind {
    // A link is a PDF when its name or its target is, and its page is the target's.
    let named = |name: &str| Path::new(name).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    let pdf = row.is_pdf() || (!row.directory && !row.link.is_empty() && (named(&row.name) || named(&row.link)));
    let icon = row.icon.to_lowercase();
    if row.directory {
        Kind::Folder
    } else if crate::backend::model3d::supported(if row.link.is_empty() { &row.name } else { &row.link }).is_some() {
        Kind::Model3d
    } else if pdf {
        Kind::Pdf
    } else if Group::of(if row.link.is_empty() { &row.name } else { &row.link }).is_some() {
        Kind::Office
    } else if icon.starts_with("image") {
        Kind::Image
    } else if icon.starts_with("audio") {
        Kind::Audio
    } else if icon.starts_with("video") {
        Kind::Video
    } else if icon.contains("package") {
        Kind::Archive
    } else {
        Kind::Data
    }
}

// A picture row the preview decodes: one the listing can thumbnail, or a link, whose target the decoder opens.
pub fn pictured(row: &Row) -> bool {
    kind(row) == Kind::Image && (row.thumbnail || !row.link.is_empty())
}

// The file a row's preview reads: a link's target, joined lexically onto the listing, or the row itself.
pub fn content(m: &Model) -> Option<PathBuf> {
    let path = m.current_path()?;
    let row = m.rows.get(&m.cursor)?;
    if row.link.is_empty() {
        return Some(path);
    }
    let mut target = path.parent()?.to_path_buf();
    for part in Path::new(&row.link).components() {
        match part {
            Component::RootDir => target = PathBuf::from("/"),
            Component::ParentDir => { target.pop(); }
            Component::Normal(name) => target.push(name),
            Component::CurDir | Component::Prefix(_) => {}
        }
    }
    Some(target)
}

// A new preview forgets the last file's archive, sentence and syntax runs; its picture goes by key.
pub fn reset(state: &mut State) {
    state.text = None;
    state.refusal = None;
    state.unshown_key = None;
    state.media = None;
    state.facts.clear();
    state.archive = None;
    state.unshown = None;
    state.unshown_mark = None;
    state.code.forget();
    super::office::forget(state);
}

// The archive half of a meta reply: the totals and named entries, or the can't-show sentence when it failed.
pub fn meta(m: &mut Model, value: &Json) {
    m.preview_state.media = Some((count(value, "w"), count(value, "h"), count(value, "ms"), count(value, "rate")));
    super::preview::words::metadata(m);
    if flag(value, "afailed") {
        m.preview_state.unshown_key = Some("preview.archiveUnreadable");
        m.preview_state.unshown = Some(Unshown { sentence: m.preview_state.i18n.tr("preview.archiveUnreadable"), open_with: false });
        return;
    }
    let names = value.get("names").and_then(Json::as_array).unwrap_or(&[]);
    if count(value, "entries") == 0 && names.is_empty() {
        return;
    }
    let (entries, unpacked) = (count(value, "entries"), count(value, "unpacked"));
    let rows = names.iter().map(|entry| {
        let directory = flag(entry, "d");
        // A lone file's size is the archive's exact unpacked total; any other entry shows a size only when the reply names one.
        let size = if entry.get("s").is_some() { Some(count(entry, "s")) } else { (entries == 1 && !directory).then_some(unpacked) };
        let row = Row {
            name: text(entry, "n").into(),
            directory,
            size: size.unwrap_or(0),
            mode: if directory { 0o040755 } else { 0o100644 },
            link: String::new(),
            kind: String::new(),
            thumbnail: false,
            modified: 0, birth: None,
            icon: if directory { "folder".into() } else { String::new() },
        };
        (row, size)
    });
    m.preview_state.archive = Some(Archive { entries, unpacked, rows: rows.collect() });
}

// The line under an archive's facts: its exact count, then what it unpacks to.
pub fn archive_line(state: &State) -> Option<String> {
    let archive = state.archive.as_ref()?;
    let count = state.i18n.trn("preview.entries", archive.entries as i64);
    Some(state.i18n.tr_args("preview.unpacked", &[&count, &state.i18n.locale().format_size(archive.unpacked as u64)]))
}

#[cfg(test)]
#[path = "pane_state_tests.rs"]
mod tests;
