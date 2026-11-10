//! CSV parsing and column measurement phases, including the next row a widening frame owes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Parse {
    Engine,
    ParsingEngine,
    Plain,
    ParsingPlain,
}

impl Parse {
    pub fn pending(self) -> Self {
        match self {
            Self::Engine | Self::ParsingEngine => Self::ParsingEngine,
            Self::Plain | Self::ParsingPlain => Self::ParsingPlain,
        }
    }

    pub fn finish(self) -> Self {
        match self {
            Self::Engine | Self::ParsingEngine => Self::Engine,
            Self::Plain | Self::ParsingPlain => Self::Plain,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Columns {
    Waiting,
    Widening(usize),
    Measured,
}
