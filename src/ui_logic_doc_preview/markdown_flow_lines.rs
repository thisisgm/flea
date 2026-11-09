//! Greedy line placement: words fill a line to the width, the line box centres the text, and pictures grow their line.

use super::{Atom, Base, FormulaBox, Laid, Line, Measure, Seg, Tok};
use crate::ui_logic_doc_preview::markdown::markdown_fit::picture_fit;
use crate::ui_logic_doc_preview::markdown::Align;

// A width this far past the limit still fits, so a measure's rounding never wraps a word that fits.
pub const SLACK: f32 = 0.01;

struct Placer<'m> {
    base: Base,
    width: f32,
    measure: &'m mut dyn Measure,
    laid: Laid,
    atoms: Vec<Atom>,
    x: f32,
    pending: Option<Seg>,
    above: f32,
    below: f32,
    box_baseline: f32,
}

impl Placer<'_> {
    fn empty(&self) -> bool {
        self.atoms.is_empty()
    }

    fn start(&mut self) {
        self.x = 0.0;
        self.pending = None;
        self.above = self.box_baseline;
        self.below = self.base.line - self.box_baseline;
    }

    // Closes the line: aligns it, sets its baseline and height, and lays the chip grounds behind its text.
    fn end(&mut self, after: &'static str) {
        let used = self.atoms.iter().map(atom_end).fold(0.0, f32::max);
        let shift = match self.base.align {
            Some(Align::Center) => ((self.width - used) / 2.0).round().max(0.0),
            Some(Align::Right) => (self.width - used).max(0.0),
            None => 0.0,
        };
        let mut atoms = std::mem::take(&mut self.atoms);
        for atom in &mut atoms {
            match atom {
                Atom::Text { x, .. }
                | Atom::Picture { x, .. }
                | Atom::Formula { x, .. }
                | Atom::Ground { x, .. } => *x += shift,
            }
        }
        let baseline = self.above;
        let grounds = self.grounds(&atoms, baseline);
        let mut line_atoms = grounds;
        line_atoms.extend(atoms);
        let top = self.laid.height;
        let height = self.above + self.below;
        self.laid.lines.push(Line {
            after,
            top,
            height,
            baseline,
            width: used,
            atoms: line_atoms,
        });
        self.laid.height = top + height;
        self.laid.width = self.laid.width.max(used);
        self.start();
    }

    // Each run of chip text on the line gets one ground, centred on the text and chip_tall high.
    fn grounds(&mut self, atoms: &[Atom], baseline: f32) -> Vec<Atom> {
        let tall = self.base.chip_tall;
        let mut out = Vec::new();
        let mut open: Option<(f32, f32, f32)> = None;
        for atom in atoms {
            let chip = match atom {
                Atom::Text { x, width, look, .. } if look.chip => Some((*x, *width, look)),
                _ => None,
            };
            let Some((x, width, look)) = chip else {
                close(&mut open, &mut out, tall);
                continue;
            };
            match &mut open {
                Some((_, end, _)) if x <= *end + SLACK => *end = x + width,
                _ => {
                    close(&mut open, &mut out, tall);
                    let (ascent, descent) = self.measure.extent(look.face, look.px);
                    let middle = baseline - (ascent - descent) / 2.0;
                    open = Some((x, x + width, (middle - tall / 2.0).round()));
                }
            }
        }
        close(&mut open, &mut out, tall);
        out
    }

    // A run of one look extends the text before it; text and an empty chip pad never join, in either order.
    fn put(&mut self, seg: Seg) {
        let x = self.x;
        self.x += seg.width;
        if let Some(Atom::Text {
            width,
            text,
            look,
            x: at,
        }) = self.atoms.last_mut()
        {
            if !text.is_empty()
                && !seg.text.is_empty()
                && *look == seg.look
                && (*at + *width - x).abs() < SLACK
            {
                text.push_str(&seg.text);
                *width += seg.width;
                return;
            }
        }
        self.atoms.push(Atom::Text {
            x,
            width: seg.width,
            text: seg.text,
            look: seg.look,
        });
    }

    fn space(&mut self) {
        if let Some(space) = self.pending.take() {
            self.put(space);
        }
    }

    fn fits(&self, width: f32) -> bool {
        let space = self.pending.as_ref().map_or(0.0, |s| s.width);
        self.empty() || self.x + space + width <= self.width + SLACK
    }

    fn word(&mut self, segs: Vec<Seg>) {
        let width: f32 = segs.iter().map(|s| s.width).sum();
        if !self.fits(width) {
            self.end(if self.pending.is_some() { " " } else { "" });
        }
        self.space();
        if self.x + width <= self.width + SLACK {
            for seg in segs {
                self.put(seg);
            }
            return;
        }
        // A word wider than the line breaks between characters.
        for seg in segs {
            if seg.text.is_empty() {
                self.put(seg);
                continue;
            }
            let mut piece = String::new();
            let mut piece_width = 0.0;
            for c in seg.text.chars() {
                let w = self
                    .measure
                    .width(c.encode_utf8(&mut [0; 4]), seg.look.face, seg.look.px);
                if self.x + piece_width + w > self.width + SLACK
                    && (self.x > 0.0 || piece_width > 0.0)
                {
                    let text = std::mem::take(&mut piece);
                    self.put(Seg {
                        text,
                        look: seg.look.clone(),
                        width: piece_width,
                    });
                    piece_width = 0.0;
                    self.end("");
                }
                piece.push(c);
                piece_width += w;
            }
            if !piece.is_empty() {
                self.put(Seg {
                    text: piece,
                    look: seg.look,
                    width: piece_width,
                });
            }
        }
    }

    fn picture(
        &mut self,
        width: f32,
        height: f32,
        url: String,
        link: Option<String>,
        stretch: bool,
    ) {
        let (width, height) = picture_fit(width, height, self.width);
        if !self.fits(width) {
            self.end(if self.pending.is_some() { " " } else { "" });
        }
        self.space();
        self.above = self.above.max(height);
        self.atoms.push(Atom::Picture {
            x: self.x,
            width,
            height,
            url,
            link,
            stretch,
        });
        self.x += width;
    }

    fn formula(&mut self, index: usize, at: FormulaBox) {
        if !self.fits(at.width) {
            self.end(if self.pending.is_some() { " " } else { "" });
        }
        self.space();
        self.above = self.above.max(at.above);
        self.below = self.below.max(at.below);
        self.atoms.push(Atom::Formula {
            x: self.x,
            width: at.width,
            above: at.above,
            below: at.below,
            index,
        });
        self.x += at.width;
    }
}

fn close(open: &mut Option<(f32, f32, f32)>, out: &mut Vec<Atom>, height: f32) {
    if let Some((start, end, top)) = open.take() {
        out.push(Atom::Ground {
            x: start,
            width: end - start,
            top,
            height,
        });
    }
}

fn atom_end(atom: &Atom) -> f32 {
    match atom {
        Atom::Text { x, width, .. }
        | Atom::Picture { x, width, .. }
        | Atom::Formula { x, width, .. }
        | Atom::Ground { x, width, .. } => x + width,
    }
}

/// The baseline a line box of `line` puts a face of this ascent and descent on: the text centred in the box.
pub fn centred_baseline(line: f32, ascent: f32, descent: f32) -> f32 {
    ((line - (ascent + descent)) / 2.0 + ascent).round()
}

pub(super) fn place(toks: Vec<Tok>, base: &Base, width: f32, measure: &mut dyn Measure) -> Laid {
    let face = base.face;
    let (ascent, descent) = measure.extent(face, base.px);
    let box_baseline = centred_baseline(base.line, ascent, descent);
    let mut p = Placer {
        base: base.clone(),
        width: width.max(0.0),
        measure,
        laid: Laid::default(),
        atoms: Vec::new(),
        x: 0.0,
        pending: None,
        above: 0.0,
        below: 0.0,
        box_baseline,
    };
    p.start();
    if toks.is_empty() {
        return p.laid;
    }
    let mut drawn = false;
    for tok in toks {
        match tok {
            Tok::Word(segs) => {
                p.word(segs);
                drawn = true;
            }
            Tok::Space(seg) => {
                if !p.empty() && p.pending.is_none() {
                    p.pending = Some(seg);
                }
            }
            Tok::Picture {
                width,
                height,
                url,
                link,
                stretch,
            } => {
                p.picture(width, height, url, link, stretch);
                drawn = true;
            }
            Tok::Formula { index, at } => {
                p.formula(index, at);
                drawn = true;
            }
            Tok::Break => {
                p.end("\n");
                drawn = false;
            }
            Tok::Gap(gap) => {
                p.pending = None;
                p.x += gap;
            }
        }
    }
    if drawn || !p.empty() {
        p.end("");
    }
    p.laid
}
