//! previewFigures: each top-level figure block's asset state, as IpcPreviewState.qml reads its figureInfo.
use super::super::MarkdownView;
use flea::ui_logic_pv2::markdown::{markdown_layout::FigureState, Block};

impl MarkdownView {
    /// `index=state` per figure block, joined by commas; "" until the document is ready, so a case keeps polling.
    pub(crate) fn figures_state(&self) -> String {
        if !self.active || !self.doc.content_ready() {
            return String::new();
        }
        let doc = &self.doc;
        doc.blocks()
            .iter()
            .enumerate()
            .filter(|(_, block)| matches!(block, Block::Figure(_)))
            .map(|(index, _)| {
                let ordinal = doc.counter(index).figure;
                let state = match doc.assets.figures.get(&ordinal) {
                    Some(FigureState::Drawn { .. }) => "ready",
                    Some(FigureState::Failed) => "failed",
                    Some(FigureState::Waiting) => "working",
                    None if doc.figure_tickets.values().any(|at| *at == ordinal) => "working",
                    None => "deferred",
                };
                format!("{index}={state}")
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}
