//! The Source view: the file's lines numbered and coloured by the backend's code engine, asked a screen at a time, or drawn plain without one.
use super::worker::TextMeasure;
use super::{
    CodeAnswer, CodeHost, CodeSource, CodeState, MarkdownView, Status, Words, CODE_LINE_CAP,
};
use flea::ui_logic_doc_preview::code::pairs;
use flea::ui_logic_doc_preview::markdown::{
    markdown_code_rows::{gutter_columns, plain_row, row_count, row_lines, Piece},
    markdown_flow::{Face, Measure},
};
use gpui::Context;
use std::{collections::HashMap, sync::Arc};

/// Rows kept laid out past the screen's own; past this the cache starts over.
const ROWS_KEPT: usize = 512;

/// The backend's whole answer for the file, None until the lines in view are answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Answer {
    #[default]
    None,
    Styled,
    Plain,
    Binary,
    Large,
    Failed,
}

/// The Source view's geometry at the view's width: a line's height, the digit advance and the gutter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    pub width: f32,
    pub height: f32,
    pub body: f32,
    pub caption: f32,
    pub line: f32,
    pub advance: f32,
    pub notice: f32,
    pub wrap: bool,
}

pub struct Source {
    measure: TextMeasure,
    text: Option<Arc<str>>,
    starts: Arc<Vec<usize>>,
    /// Ask with the text the view holds, never the path, as on network and phone storage.
    pub truncate: bool,
    pub answer: Answer,
    pub total: usize,
    held: HashMap<usize, Vec<(i64, String)>>,
    token: u64,
    asking: Option<(usize, usize)>,
    /// The first row in view, the notice being row 0 when there is one, and how far into it the view starts.
    pub top: usize,
    pub offset: f32,
    pub frame: Frame,
    notice: Option<String>,
    laid: HashMap<usize, Arc<Vec<Vec<Piece>>>>,
    /// Characters of the rows the last frame laid out, so a suite sees that Rendered lays none.
    pub laid_chars: usize,
}

#[path = "source_position.rs"]
mod position;

impl Source {
    pub fn new(measure: TextMeasure) -> Self {
        Source {
            measure,
            text: None,
            starts: Arc::new(Vec::new()),
            truncate: false,
            answer: Answer::None,
            total: 0,
            held: HashMap::new(),
            token: 0,
            asking: None,
            top: 0,
            offset: 0.,
            frame: Frame::default(),
            notice: None,
            laid: HashMap::new(),
            laid_chars: 0,
        }
    }

    pub fn measure(&mut self) -> &mut TextMeasure {
        &mut self.measure
    }

    /// A new document: nothing held, nothing asked and the view at its top.
    pub fn reset(&mut self) {
        self.text = None;
        self.starts = Arc::new(Vec::new());
        self.top = 0;
        self.offset = 0.;
        self.laid_chars = 0;
        self.reset_asks();
    }

    /// Forgets every answer, for a new engine or a new text.
    pub fn reset_asks(&mut self) {
        self.answer = Answer::None;
        self.total = 0;
        self.held.clear();
        self.token = 0;
        self.asking = None;
        self.laid.clear();
    }

    pub fn set_text(&mut self, text: Arc<str>, starts: Arc<Vec<usize>>) {
        self.text = Some(text);
        self.starts = starts;
        if self.answer == Answer::Plain {
            self.reset_asks();
        }
    }

    fn settled(&self) -> bool {
        matches!(self.answer, Answer::Styled | Answer::Plain)
    }

    /// The sentence a refused Source shows; a binary file shows its mark and caption only.
    pub fn refusal<'a>(&self, words: &'a Words) -> Option<&'a str> {
        match self.answer {
            Answer::Large => Some(&words.too_large),
            Answer::Failed => Some(&words.unreadable),
            _ => None,
        }
    }

    /// The runs of line `n` (1-based), None until answered.
    pub fn row(&self, n: usize) -> Option<Vec<(i64, String)>> {
        if let Some(runs) = self.held.get(&n) {
            return Some(runs.clone());
        }
        let text = self
            .text
            .as_ref()
            .filter(|_| self.answer == Answer::Plain && self.held.is_empty())?;
        (n <= self.total)
            .then(|| plain_row(text, &self.starts, n))
            .flatten()
    }

    fn has(&self, n: usize) -> bool {
        self.held.contains_key(&n)
            || (self.answer == Answer::Plain && self.held.is_empty() && n <= self.total)
    }

    /// Rows in the view's list: the notice when there is one, then every line.
    pub fn rows(&self) -> usize {
        usize::from(self.notice.is_some()) + self.total
    }

    /// The line a row draws, None for the notice.
    pub fn line_of(&self, row: usize) -> Option<usize> {
        match self.notice {
            Some(_) if row == 0 => None,
            Some(_) => Some(row),
            None => Some(row + 1),
        }
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Sets the frame the rows are laid in; a new width or wrap lays them again with the top line kept at the top.
    pub fn set_frame(&mut self, frame: Frame, notice: Option<&str>) {
        if frame.width != self.frame.width
            || frame.wrap != self.frame.wrap
            || frame.body != self.frame.body
        {
            self.laid.clear();
            self.offset = 0.;
        }
        self.frame = frame;
        self.notice = notice.map(str::to_string);
    }

    /// The pieces of each drawn line of a line row, laid once per width and wrap.
    pub fn lines(&mut self, n: usize) -> Option<Arc<Vec<Vec<Piece>>>> {
        if let Some(lines) = self.laid.get(&n) {
            return Some(Arc::clone(lines));
        }
        let runs = self.row(n)?;
        let filled = pairs(
            &runs
                .iter()
                .map(|(role, text)| (*role, text.as_str()))
                .collect::<Vec<_>>(),
        );
        let f = self.frame;
        let text_x = self.gutter() + 2. * f.advance;
        let lines = Arc::new(row_lines(
            &filled,
            (f.width - text_x).max(0.),
            f.wrap,
            f.body,
            &mut self.measure,
        ));
        if self.laid.len() >= ROWS_KEPT {
            self.laid.clear();
        }
        self.laid.insert(n, Arc::clone(&lines));
        Some(lines)
    }

    pub fn gutter(&self) -> f32 {
        gutter_columns(self.total) as f32 * self.frame.advance
    }

    /// A row's height: the notice and its gap, one line, or every line a wrapped row breaks into.
    pub fn height(&mut self, row: usize) -> f32 {
        match self.line_of(row) {
            None => self.frame.notice,
            Some(_) if !self.frame.wrap => self.frame.line,
            Some(n) => self.lines(n).map_or(1, |lines| lines.len().max(1)) as f32 * self.frame.line,
        }
    }

    /// Moves the view by `dy` pixels, never past the first row's top or the last row's bottom.
    pub fn scroll_by(&mut self, dy: f32) {
        self.offset += dy;
        while self.offset < 0. && self.top > 0 {
            self.top -= 1;
            self.offset += self.height(self.top);
        }
        self.offset = self.offset.max(0.);
        while self.top + 1 < self.rows() && self.offset >= self.height(self.top) {
            self.offset -= self.height(self.top);
            self.top += 1;
        }
        // The last screen ends at the last row's bottom.
        let (mut row, mut room) = (self.rows(), self.frame.height);
        while row > 0 && room > 0. {
            row -= 1;
            room -= self.height(row);
        }
        let floor = if room > 0. { (0, 0.) } else { (row, -room) };
        if (self.top, self.offset) > floor {
            (self.top, self.offset) = floor;
        }
    }

    /// The lines the view needs held: those in it and a screen either side, never past the file once its length is known.
    pub fn needed(&self) -> (usize, usize) {
        let rows = (self.frame.height / self.frame.line.max(1.)).ceil().max(1.) as usize;
        let top = self.line_of(self.top).unwrap_or(1).max(1);
        let mut to = top + 2 * rows - 1;
        if self.settled() {
            to = to.min(self.total);
        }
        (top.saturating_sub(rows).max(1), to)
    }

    fn covered(&self, (from, to): (usize, usize)) -> bool {
        (from..=to).all(|n| self.has(n))
    }

    /// The lines in view are held, so the view may show.
    pub fn first_screen(&self) -> bool {
        self.settled() && self.covered(self.needed())
    }

    /// The backend's reply, true when it was this view's.
    pub fn answered(&mut self, answer: &CodeAnswer) -> bool {
        if self.token == 0 || answer.token != self.token {
            return false;
        }
        self.token = 0;
        self.answer = match answer.state {
            CodeState::Styled => Answer::Styled,
            CodeState::Plain => Answer::Plain,
            CodeState::Binary => Answer::Binary,
            CodeState::Large => Answer::Large,
            CodeState::Failed => Answer::Failed,
        };
        self.total = answer.total;
        for (i, line) in answer.lines.iter().enumerate() {
            self.laid.remove(&(answer.first + i));
            self.held.insert(answer.first + i, line.clone());
        }
        true
    }

    // Asks the engine for the lines in view not yet held, one ask in flight per window.
    fn ask(&mut self, host: &dyn CodeHost, slot: &str, path: Option<&str>) {
        if self.frame.height <= 0.
            || matches!(self.answer, Answer::Binary | Answer::Large | Answer::Failed)
        {
            return;
        }
        let source = if self.truncate {
            let Some(text) = &self.text else { return };
            if text.is_empty() {
                self.answer = Answer::Plain;
                return;
            }
            let name = path
                .and_then(|p| p.rsplit('/').next())
                .unwrap_or_default()
                .to_string();
            CodeSource::Text {
                text: Arc::clone(text),
                name,
                lang: String::new(),
            }
        } else {
            match path {
                Some(path) if !path.is_empty() => CodeSource::Path(path.to_string()),
                _ => return,
            }
        };
        let want = self.needed();
        if want.1 < want.0
            || (self.settled() && self.covered(want))
            || (self.token != 0 && self.asking == Some(want))
        {
            return;
        }
        self.asking = Some(want);
        let mut from = want.0;
        while from < want.1 && self.has(from) {
            from += 1;
        }
        self.token = host.ask_code(slot, source, from, CODE_LINE_CAP.min(want.1 - from + 1));
    }
}

impl MarkdownView {
    /// Keeps the Source view's lines asked while it shows; without an engine the text read is drawn plain.
    pub(super) fn source_pump(&mut self, _cx: &mut Context<Self>) {
        if !self.active
            || self.shown_view() != super::View::Source
            || matches!(self.doc.status, Status::TooLarge | Status::Unreadable)
        {
            return;
        }
        let path = self.path.as_ref().map(|p| p.to_string_lossy().into_owned());
        match &self.code {
            Some(host) => self.source.ask(host.as_ref(), &self.slot, path.as_deref()),
            None => {
                if let (Some(text), Answer::None) = (&self.source.text, self.source.answer) {
                    self.source.total = row_count(text, &self.source.starts);
                    self.source.answer = Answer::Plain;
                }
            }
        }
    }

    /// What Source shows in place of its lines: a too large or unreadable read, or the engine's refusal.
    pub fn source_refused(&self) -> Option<Answer> {
        match self.doc.status {
            Status::TooLarge => Some(Answer::Large),
            Status::Unreadable => Some(Answer::Failed),
            _ => matches!(
                self.source.answer,
                Answer::Binary | Answer::Large | Answer::Failed
            )
            .then_some(self.source.answer),
        }
    }

    /// Source's frame at the view's size; the notice row is the caption's line and the block gap under it.
    pub(super) fn source_frame(&mut self, width: f32, height: f32) {
        let t = self.tokens;
        let measure = self.source.measure();
        let (ascent, descent) = measure.extent(Face::Regular, t.body);
        // A board's row never draws under the face's extent, so a large text size keeps its lines apart.
        let face_line = (ascent + descent).round();
        let (cap_ascent, cap_descent) = measure.extent(Face::Regular, t.caption);
        let advance = measure.width("0", Face::Regular, t.body);
        let frame = Frame {
            width: (width - 2. * t.inset_x()).max(0.),
            height: (height - 2. * t.source_inset_y()).max(0.),
            body: t.body,
            caption: t.caption,
            line: t.source_line.map_or_else(
                || super::super::line_pitch(t.body).max((ascent + descent).ceil()),
                |row| row.max(face_line),
            ),
            advance,
            notice: (cap_ascent + cap_descent).round() + self.tokens.metrics().block_gap(),
            wrap: self.wrap,
        };
        let notice = self.doc.deep().then(|| self.words.deep.clone());
        self.source.set_frame(frame, notice.as_deref());
    }
}
