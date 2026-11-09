//! Quick Look's font page (ui/FontPane.qml): the backend draws the sample at three sizes in the theme's colours, the
//! page shows all three with the facts line at once, and a refused font draws the unsupported sentence and no page.
use crate::i18n::I18n;
use std::sync::atomic::{AtomicU64, Ordering};

/// The board's sentence; the backend renders it, so it is the specimen text, not a label.
pub const SAMPLE_TEXT: &str = "The quick brown fox jumps over the lazy dog";
/// The board's three sizes in logical pixels before the text-size ratio.
pub const SAMPLE_SIZES: [u32; 3] = [22, 15, 12];
/// The page's sides, the least gap over and under it, and the gap between samples, before the ratio.
pub const SIDE: f64 = 40.;
pub const MIN_TOP: f64 = 24.;
pub const SPACING: f64 = 18.;

// One count for every font page, so an answer meant for an earlier page never carries a later page's id.
static LOOK_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn next_look_id() -> u64 {
    LOOK_SEQ.fetch_add(1, Ordering::Relaxed) + 1
}

/// TrueType, OpenType and collections, whatever the case; a bare extension is no font (the backend checks the MIME too).
pub fn is_font(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".ttf", ".otf", ".ttc"]
        .iter()
        .any(|ext| lower.len() > ext.len() && lower.ends_with(ext))
}

/// The pixels each sample is drawn with: the board size scaled by the text-size ratio, then by the window's pixel ratio.
pub fn physical(size_ratio: f64, pixel_ratio: f64) -> [u32; 3] {
    SAMPLE_SIZES.map(|px| ((f64::from(px) * size_ratio).round() * pixel_ratio).round() as u32)
}

/// The `fontlook` request.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub id: u64,
    pub path: String,
    pub fg: String,
    pub bg: String,
    pub sizes: [u32; 3],
    pub text: &'static str,
}

/// One `fontlooked` reply; `err` is diagnostic only and never drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reply {
    pub id: u64,
    pub err: Option<String>,
    pub family: String,
    pub style: String,
    pub glyphs: u64,
    pub files: Vec<String>,
}

/// The last good answer and the path it was asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub path: String,
    pub reply: Reply,
}

#[derive(Default)]
pub struct Page {
    pub active: bool,
    pub path: String,
    pub size: u64,
    pub look: Option<Look>,
    pub failed_path: Option<String>,
    pub pending: u64,
    pending_path: String,
    asked: Option<(String, String, String, [u32; 3])>,
    /// Asks sent, for the count tests.
    pub asks: usize,
}

impl Page {
    /// The page draws only its own answer, whole: until then Quick Look draws its loading state.
    pub fn drawn(&self) -> bool {
        self.look
            .as_ref()
            .is_some_and(|look| look.path == self.path)
    }

    pub fn failed(&self) -> bool {
        self.failed_path.as_deref() == Some(self.path.as_str())
    }

    pub fn refusal(&self) -> Option<super::refusal::Refusal> {
        self.failed()
            .then_some(super::refusal::Refusal::FontUnsupported)
    }

    /// One ask per change of path, colours or sizes; a theme change asks again and keeps the old images until the new three land.
    pub fn ask(&mut self, fg: &str, bg: &str, sizes: [u32; 3]) -> Option<Ask> {
        if !self.active || self.path.is_empty() {
            return None;
        }
        let same = self
            .asked
            .as_ref()
            .is_some_and(|(path, f, b, s)| *path == self.path && f == fg && b == bg && *s == sizes);
        if same {
            return None;
        }
        self.asked = Some((self.path.clone(), fg.into(), bg.into(), sizes));
        self.pending = next_look_id();
        self.pending_path = self.path.clone();
        self.asks += 1;
        Some(Ask {
            id: self.pending,
            path: self.path.clone(),
            fg: fg.into(),
            bg: bg.into(),
            sizes,
            text: SAMPLE_TEXT,
        })
    }

    /// One answer: read against its own request, and dropped unless it is the newest ask of this page.
    pub fn answered(&mut self, reply: Reply) -> bool {
        if reply.id != self.pending {
            return false;
        }
        if reply.err.is_some() || reply.files.len() != SAMPLE_SIZES.len() {
            self.failed_path = Some(self.pending_path.clone());
            self.look = None;
        } else {
            self.failed_path = None;
            self.look = Some(Look {
                path: self.pending_path.clone(),
                reply,
            });
        }
        true
    }

    /// The page's samples could not be decoded: the font reads as refused, never half drawn.
    pub fn samples_failed(&mut self, path: &str) {
        if self.look.as_ref().is_some_and(|look| look.path == path) {
            self.failed_path = Some(path.to_string());
            self.look = None;
        }
    }

    /// "Inter · Bold · 312.0 kB · 2,548 glyphs"; a table the font lacks drops its part, and the size is always known.
    pub fn facts_line(&self, i18n: &I18n) -> String {
        self.look
            .as_ref()
            .filter(|_| self.drawn())
            .map_or_else(String::new, |look| {
                facts_line(
                    i18n,
                    &look.reply.family,
                    &look.reply.style,
                    self.size,
                    look.reply.glyphs,
                )
            })
    }
}

pub fn facts_line(i18n: &I18n, family: &str, style: &str, bytes: u64, glyphs: u64) -> String {
    let size = i18n.locale().format_size(bytes);
    let mut parts: Vec<String> = [family, style]
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect();
    parts.push(size);
    if glyphs > 0 {
        parts.push(i18n.trn("document.font_glyphs", glyphs as i64));
    }
    join_facts(i18n, parts)
}

/// Facts joined pairwise through the catalog's " · " template, so a language can reorder or respace them.
pub fn join_facts(i18n: &I18n, parts: Vec<String>) -> String {
    parts
        .into_iter()
        .reduce(|line, part| i18n.tr_args("document.facts_pair", &[&line, &part]))
        .unwrap_or_default()
}

/// The page's block in a pane below the bar: its left edge and width, its top (centred, never closer than the least gap), and the gap.
pub fn layout(pane: (f64, f64), block: f64, ratio: f64) -> (f64, f64, f64, f64) {
    let x = (SIDE * ratio).round();
    let top = (MIN_TOP * ratio)
        .round()
        .max(((pane.1 - block) / 2.).round());
    (x, pane.0 - 2. * x, top, (SPACING * ratio).round())
}

#[cfg(test)]
#[path = "font_tests.rs"]
mod tests;
