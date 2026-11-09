//! A paragraph's pieces as tokens: words of measured runs, spaces, pictures, formulas, breaks and the gaps chips pad with.

use super::super::markdown_html_image::picture_size;
use super::{look, Base, Face, FormulaBox, Look, Measure, Piece, Sizes, Style, TASK_GLYPHS};

pub(super) struct Seg {
    pub(super) text: String,
    pub(super) look: Look,
    pub(super) width: f32,
}

pub(super) enum Tok {
    Word(Vec<Seg>),
    Space(Seg),
    Picture {
        width: f32,
        height: f32,
        url: String,
        link: Option<String>,
        stretch: bool,
    },
    Formula {
        index: usize,
        at: FormulaBox,
    },
    Break,
    Gap(f32),
}

pub(super) struct Tokens<'m> {
    pub(super) out: Vec<Tok>,
    pub(super) word: Vec<Seg>,
    pub(super) measure: &'m mut dyn Measure,
    pub(super) pre: bool,
}

impl Tokens<'_> {
    pub(super) fn flush(&mut self) {
        if !self.word.is_empty() {
            self.out.push(Tok::Word(std::mem::take(&mut self.word)));
        }
    }

    fn seg(&mut self, text: String, look: &Look) -> Seg {
        let width = self.measure.width(&text, look.face, look.px);
        Seg {
            text,
            look: look.clone(),
            width,
        }
    }

    fn pad(&mut self, look: &Look, width: f32) {
        self.word.push(Seg {
            text: String::new(),
            look: look.clone(),
            width,
        });
    }

    // Words glue across pieces; a space ends one and is a break opportunity, kept as its own word in preformatted text.
    pub(super) fn text(&mut self, text: &str, look: &Look) {
        let mut word = String::new();
        for c in text.chars() {
            if c == ' ' || c == '\n' {
                if !word.is_empty() {
                    let seg = self.seg(std::mem::take(&mut word), look);
                    self.word.push(seg);
                }
                self.flush();
                if self.pre && c == '\n' {
                    self.out.push(Tok::Break);
                    continue;
                }
                let space = self.seg(" ".to_string(), look);
                self.out.push(if self.pre {
                    Tok::Word(vec![space])
                } else {
                    Tok::Space(space)
                });
            } else {
                word.push(c);
            }
        }
        if !word.is_empty() {
            let seg = self.seg(word, look);
            self.word.push(seg);
        }
    }
}

fn chip_of(piece: Option<&Piece>) -> bool {
    match piece {
        Some(Piece::Text(_, style)) => style.code,
        Some(Piece::Formula { .. }) => true,
        _ => false,
    }
}

pub(super) fn tokens(
    pieces: &[Piece],
    base: &Base,
    measure: &mut dyn Measure,
    sizes: &dyn Sizes,
) -> Vec<Tok> {
    let mut t = Tokens {
        out: Vec::new(),
        word: Vec::new(),
        measure,
        pre: base.pre,
    };
    let mut chip_open = false;
    for (i, piece) in pieces.iter().enumerate() {
        let formula = match piece {
            Piece::Formula { index, .. } => sizes.formula(*index).map(|at| (*index, at)),
            _ => None,
        };
        let chip = chip_of(Some(piece)) && formula.is_none();
        let text = match piece {
            Piece::Text(text, style) => Some((text.as_str(), style.clone())),
            Piece::Formula { text, style, .. } if formula.is_none() => {
                Some((text.as_str(), style.clone()))
            }
            _ => None,
        };
        if let Some((text, mut style)) = text {
            style.code |= chip;
            let look = look(&style, base);
            if chip && !chip_open {
                t.pad(&look, base.chip_pad);
            }
            t.text(text, &look);
            let next_chip = chip_of(pieces.get(i + 1))
                && !matches!(pieces.get(i + 1), Some(Piece::Formula { index, .. }) if sizes.formula(*index).is_some());
            if chip && !next_chip {
                t.pad(&look, base.chip_pad);
            }
            chip_open = chip && next_chip;
            continue;
        }
        chip_open = false;
        match piece {
            Piece::TaskBox(done) => {
                let symbol = Look {
                    face: Face::Symbol,
                    ..look(&Style::default(), base)
                };
                let seg = t.seg(TASK_GLYPHS[usize::from(*done)].to_string(), &symbol);
                t.word.push(seg);
            }
            Piece::Image(image) => {
                t.flush();
                let natural = sizes.picture(&image.url).unwrap_or((0.0, 0.0));
                let given = |v: Option<u32>| v.map_or(0.0, f64::from);
                let (w, h, stretch) = picture_size(
                    given(image.width),
                    given(image.height),
                    f64::from(natural.0),
                    f64::from(natural.1),
                    f64::INFINITY,
                );
                t.out.push(Tok::Picture {
                    width: w as f32,
                    height: h as f32,
                    url: image.url.clone(),
                    link: image.link.clone(),
                    stretch,
                });
            }
            Piece::Formula { .. } => {
                t.flush();
                if let Some((index, at)) = formula {
                    t.out.push(Tok::Formula { index, at });
                }
            }
            Piece::Break => {
                t.flush();
                t.out.push(Tok::Break);
            }
            Piece::CellGap => {
                t.flush();
                t.out.push(Tok::Gap(base.cell_gap));
            }
            Piece::Text(..) => {}
        }
    }
    t.flush();
    t.out
}
