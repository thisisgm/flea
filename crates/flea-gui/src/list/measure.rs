use super::{columns, Advance, Layout, ListState, Metrics, Tokens};
use gpui::{Pixels, Window};

#[derive(Clone, Copy, Default)]
pub enum Density {
    Tight,
    #[default]
    Compact,
    Normal,
    Comfortable,
}
impl Density {
    pub fn parse(value: &str) -> Self {
        match value {
            "tight" => Self::Tight,
            "normal" => Self::Normal,
            "comfortable" => Self::Comfortable,
            _ => Self::Compact,
        }
    }
    fn ratio(self) -> f32 {
        match self {
            Self::Tight => 0.,
            Self::Compact => 0.5,
            Self::Normal => 1.,
            Self::Comfortable => 1.5,
        }
    }
}
impl ListState {
    /// The drawn width of a shown row's name, the run Tap.onName counts as the name.
    pub(crate) fn drawn_name_width(&self, row: usize) -> Option<f32> {
        let shown = self.visible.iter().find(|shown| shown.index == row)?;
        let width = f32::from(shown.prepared.as_ref()?.name.line().width);
        Some(width.min(self.drawn.layout.name.width))
    }

    /// Theme.fileRowHeight, the density row every Columns track draws; a contents search changes only the active pitch.
    pub fn columns_row(&self) -> f32 {
        let padding = (6. * f32::from(self.typeface.base) / 12.).round();
        Metrics::for_typeface(&self.typeface).row
            + 2. * ((padding * self.density.ratio()).round() - (padding * 0.5).round())
    }

    pub fn file_metrics(&self) -> Metrics {
        let mut metrics =
            Metrics::for_typeface(&self.typeface).for_dual(self.view.dual, self.typeface.body);
        metrics.row = self.columns_row();
        if self.view.contents {
            metrics.row = crate::g2::file_search::contents::BLOCK_H;
        }
        metrics
    }
    // Measured once per face; a language or font change drops it with the shaped rows.
    pub(super) fn advance(&mut self, window: &Window) -> Advance {
        if let Some(advance) = self.advance {
            return advance;
        }
        let text = window.text_system();
        let id = text.resolve_font(&self.typeface.font);
        let of = |size: Pixels| {
            text.advance(id, size, '0')
                .map_or(f32::from(size) * 0.6, |advance| f32::from(advance.width))
        };
        let advance = Advance {
            body: of(self.typeface.body),
            small: of(self.typeface.body_small),
            caption: of(self.typeface.caption),
        };
        self.advance = Some(advance);
        advance
    }

    /// Already resolved by normal frame preparation; observers never measure a font.
    pub(crate) fn observed_column_tokens(&self) -> Option<Tokens> {
        let advance = self.advance?;
        let metrics = self.file_metrics();
        Some(
            Tokens::measure(
                metrics.pad,
                metrics.slot,
                metrics.gap,
                advance.small,
                f32::from(self.typeface.body_small),
            )
            .for_dual(self.view.dual),
        )
    }

    /// The geometry and column tokens at this face.
    pub fn measure(&mut self, window: &Window) -> (Metrics, Tokens) {
        let metrics = self.file_metrics();
        let advance = self.advance(window);
        let tokens = Tokens::measure(
            metrics.pad,
            metrics.slot,
            metrics.gap,
            advance.small,
            f32::from(self.typeface.body_small),
        )
        .for_dual(self.view.dual);
        (metrics, tokens)
    }

    /// The drawn columns for a list this wide: the row is the list less its scroll lane.
    pub fn layout(&mut self, width: f32, window: &Window) -> (Metrics, Tokens, Layout) {
        let (metrics, mut tokens) = self.measure(window);
        tokens.flush_size = self.view.dual;
        if self.view.picker.is_some() {
            tokens.measured[super::columns::Column::Size.slot()] = 70.;
            tokens.measured[super::columns::Column::Date.slot()] = 80.;
        }
        if self.view.tags_shown {
            tokens.name_min += flea::ui_logic_rail_places::tags::ROW_SLOT + metrics.gap;
        }
        let row = (width - metrics.lane).max(0.);
        let mut layout = if self.view.place_recent.is_some() {
            super::recent::layout(
                row,
                &tokens,
                self.view.switched,
                self.view.stored,
                self.view.dual,
            )
        } else {
            columns::resolve(row, &tokens, self.view.switched, self.view.stored)
        };
        layout = super::git::reserve(layout, !self.view.git.label.is_empty(), metrics.gap);
        let leading = super::picker::leading(&self.view, &metrics);
        layout.name.x += leading;
        layout.name.width = (layout.name.width - leading).max(0.);
        (
            metrics,
            tokens,
            super::tags::reserve(layout, self.view.tags_shown, metrics.gap),
        )
    }
}
