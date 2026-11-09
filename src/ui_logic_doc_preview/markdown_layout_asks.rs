//! What a document asks its host for in ordinal order, the ordinals each block takes, and the sizes a run's text reads.

use super::{Assets, Block, Counter, FigureKind, FormulaBox, Sizes};

/// What a document asks its host for, in ordinal order: each figure and inline formula, and each fence's text and info.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Asks {
    /// Each figure's kind, its source and whether it is a display figure.
    pub figures: Vec<(FigureKind, String, bool)>,
    pub formulas: Vec<String>,
    pub fences: Vec<(String, String)>,
}

/// The asks of the blocks, appended in the order layout numbers them.
pub fn asks(blocks: &[Block], out: &mut Asks) {
    for block in blocks {
        match block {
            Block::Run { maths, .. } => out.formulas.extend(maths.iter().cloned()),
            Block::Fence { text, info } => out.fences.push((text.clone(), info.clone())),
            Block::Figure(figure) => {
                out.figures
                    .push((figure.kind, figure.source.clone(), figure.display))
            }
            Block::Quote(quote) => asks(quote.parts.as_deref().unwrap_or(&[]), out),
            Block::List(list) => {
                for parts in list.parts.iter().flatten().flatten() {
                    asks(parts, out);
                }
            }
            _ => {}
        }
    }
}

/// The ordinals one block takes.
pub fn count(block: &Block) -> Counter {
    let mut asked = Asks::default();
    asks(std::slice::from_ref(block), &mut asked);
    Counter {
        figure: asked.figures.len(),
        formula: asked.formulas.len(),
        fence: asked.fences.len(),
    }
}

/// Sizes for text that holds no picture or formula of its own.
pub struct NoSizes;

impl Sizes for NoSizes {
    fn picture(&self, _url: &str) -> Option<(f32, f32)> {
        None
    }

    fn formula(&self, _index: usize) -> Option<FormulaBox> {
        None
    }
}

/// The sizes a run's text reads: its pictures, and its formulas from the run's first ordinal.
pub struct RunSizes<'a> {
    pub assets: &'a dyn Assets,
    /// The run's first formula ordinal, None for text that draws formulas as chips.
    pub formulas: Option<usize>,
}

impl Sizes for RunSizes<'_> {
    fn picture(&self, url: &str) -> Option<(f32, f32)> {
        self.assets.picture(url)
    }

    fn formula(&self, index: usize) -> Option<FormulaBox> {
        self.assets.formula(self.formulas? + index)
    }
}
