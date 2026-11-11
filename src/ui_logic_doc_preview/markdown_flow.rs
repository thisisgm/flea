//! A paragraph's pieces broken into drawn lines at a width: each run's place, the chip grounds and every line's height.

use super::markdown_model::Align;
use super::markdown_rich::{Piece, Style};

/// The task box's ticked and open glyphs, which draw in the symbol face.
pub const TASK_GLYPHS: [&str; 2] = ["\u{2610}", "\u{2611}"];

/// A face of the document's family; Symbol is the task box's face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Regular,
    Bold,
    Italic,
    BoldItalic,
    Symbol,
}

impl Face {
    pub fn of(bold: bool, italic: bool) -> Self {
        match (bold, italic) {
            (false, false) => Face::Regular,
            (true, false) => Face::Bold,
            (false, true) => Face::Italic,
            (true, true) => Face::BoldItalic,
        }
    }

    pub fn bold(self) -> bool {
        matches!(self, Face::Bold | Face::BoldItalic)
    }

    pub fn italic(self) -> bool {
        matches!(self, Face::Italic | Face::BoldItalic)
    }
}

/// The theme role a run draws in; the host maps each to its palette.
#[derive(Clone, Debug, PartialEq)]
pub enum Ink {
    Foreground,
    Bright,
    Muted,
    /// A code reply's role: 0 the foreground, 1 to 6 red, green, yellow, blue, magenta and cyan.
    Code(i64),
    /// A colour the markup names, "#rrggbb" or "#rrggbbaa".
    Hex(String),
}

/// How one run of text draws.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub face: Face,
    pub px: f32,
    pub ink: Ink,
    pub underline: bool,
    pub strike: bool,
    /// The run's baseline below the line's, negative for a superscript.
    pub shift: f32,
    /// A code chip's ground sits behind the run.
    pub chip: bool,
    pub link: Option<String>,
}

/// The host's text system: an advance and a face's extent at a size.
pub trait Measure {
    fn width(&mut self, text: &str, face: Face, px: f32) -> f32;
    /// A table cell uses the same complete shaped run that its painter draws.
    fn table_width(&mut self, text: &str, face: Face, px: f32) -> f32 {
        self.width(text, face, px)
    }
    /// The ascent and descent, both positive.
    fn extent(&mut self, face: Face, px: f32) -> (f32, f32);
}

/// An inline formula's drawn box: its width and how far it reaches above and below the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FormulaBox {
    pub width: f32,
    pub above: f32,
    pub below: f32,
}

/// What sizes a paragraph's pictures: a picture's natural size by address, an inline formula's box by its index.
pub trait Sizes {
    fn picture(&self, url: &str) -> Option<(f32, f32)>;
    fn formula(&self, index: usize) -> Option<FormulaBox>;
}

/// One drawn thing on a line, x from the line's left edge.
#[derive(Clone, Debug, PartialEq)]
pub enum Atom {
    Text {
        x: f32,
        width: f32,
        text: String,
        look: Look,
    },
    /// A picture whose bottom sits on the baseline.
    Picture {
        x: f32,
        width: f32,
        height: f32,
        url: String,
        link: Option<String>,
        stretch: bool,
    },
    /// An inline formula: its index in the paragraph, and its document ordinal once a block lays it out.
    Formula {
        x: f32,
        width: f32,
        above: f32,
        below: f32,
        index: usize,
    },
    /// A code chip's ground, top and height from the line's top.
    Ground {
        x: f32,
        width: f32,
        top: f32,
        height: f32,
    },
}

/// One drawn line: its top inside the paragraph, its height and its baseline from its top.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Text between this visual row and the next: a source break, soft-wrap space, or no separator.
    pub after: &'static str,
    pub top: f32,
    pub height: f32,
    pub baseline: f32,
    pub width: f32,
    pub atoms: Vec<Atom>,
}

/// A laid paragraph: its lines, its height and its widest line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    pub lines: Vec<Line>,
    pub height: f32,
    pub width: f32,
}

impl Laid {
    pub fn first_baseline(&self) -> Option<f32> {
        self.lines.first().map(|line| line.top + line.baseline)
    }

    /// Stacks another laid paragraph under this one, `gap` below it.
    pub fn append(&mut self, other: Laid, gap: f32) {
        if let Some(last) = self.lines.last_mut() {
            last.after = "\n";
        }
        let top = if self.lines.is_empty() {
            0.0
        } else {
            self.height + gap
        };
        for mut line in other.lines {
            line.top += top;
            self.lines.push(line);
        }
        self.height = top + other.height;
        self.width = self.width.max(other.width);
    }
}

/// The look a paragraph starts from and the sizes its line takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Base {
    pub px: f32,
    pub face: Face,
    pub ink: Ink,
    /// The line box, which the text centres in.
    pub line: f32,
    pub align: Option<Align>,
    pub chip_pad: f32,
    pub chip_tall: f32,
    /// A subscript's and superscript's size, and how far each moves off the baseline.
    pub script_px: f32,
    pub sub_shift: f32,
    pub sup_shift: f32,
    /// The space two cells of an HTML row keep between them.
    pub cell_gap: f32,
    /// Preformatted: every space is kept and a line end breaks the line.
    pub pre: bool,
}

/// The look a piece's style draws in over the paragraph's base ink and face.
pub fn look(style: &Style, base: &Base) -> Look {
    let face = Face::of(
        style.bold || base.face.bold(),
        style.italic || base.face.italic(),
    );
    let ink = match &style.colour {
        Some(hex) => Ink::Hex(hex.clone()),
        None => base.ink.clone(),
    };
    let scripted = style.sub || style.sup;
    let px = if scripted {
        base.script_px
    } else if style.small {
        (base.px * 5.0 / 6.0).round()
    } else {
        base.px
    };
    let shift = if style.sub {
        base.sub_shift
    } else if style.sup {
        base.sup_shift
    } else {
        0.0
    };
    Look {
        face,
        px,
        ink,
        underline: style.underline,
        strike: style.strike,
        shift,
        chip: style.code,
        link: style.link.clone(),
    }
}

#[path = "markdown_flow_tokens.rs"]
mod toks;

use toks::{tokens, Seg, Tok, Tokens};

#[path = "markdown_flow_lines.rs"]
mod lines;

pub use lines::{centred_baseline, SLACK};

/// The paragraph's pieces laid at a width: words wrap at spaces, and a word wider than the width breaks anywhere.
pub fn flow(
    pieces: &[Piece],
    base: &Base,
    width: f32,
    measure: &mut dyn Measure,
    sizes: &dyn Sizes,
) -> Laid {
    let toks = tokens(pieces, base, measure, sizes);
    lines::place(toks, base, width, measure)
}

/// Lines of runs that each name their own look, laid as preformatted text: a fence, a failed figure's source.
pub fn flow_lines(
    lines: &[Vec<(Look, String)>],
    base: &Base,
    width: f32,
    measure: &mut dyn Measure,
) -> Laid {
    let mut t = Tokens {
        out: Vec::new(),
        word: Vec::new(),
        measure,
        pre: true,
    };
    for (i, runs) in lines.iter().enumerate() {
        if i > 0 {
            t.flush();
            t.out.push(Tok::Break);
        }
        for (look, text) in runs {
            t.text(text, look);
        }
    }
    t.flush();
    let measure = t.measure;
    lines::place(t.out, base, width, measure)
}

/// The text cut to fit the width with an ellipsis at its end, whole when it fits.
pub fn elide(text: &str, width: f32, face: Face, px: f32, measure: &mut dyn Measure) -> String {
    if measure.width(text, face, px) <= width {
        return text.to_string();
    }
    let ellipsis = "\u{2026}";
    let room = width - measure.width(ellipsis, face, px);
    let mut used = 0.0;
    let mut out = String::new();
    for c in text.chars() {
        let w = measure.width(c.encode_utf8(&mut [0; 4]), face, px);
        if used + w > room {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push_str(ellipsis);
    out
}
