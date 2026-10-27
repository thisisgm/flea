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

    pub fn file_metrics(&self) -> Metrics {
        let mut metrics =
            Metrics::for_typeface(&self.typeface).for_dual(self.view.dual, self.typeface.body);
        let padding = (6. * f32::from(self.typeface.base) / 12.).round();
        metrics.row += 2. * ((padding * self.density.ratio()).round() - (padding * 0.5).round());
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
        let metrics = Metrics::for_typeface(&self.typeface);
        Some(Tokens::measure(
            metrics.pad,
            metrics.slot,
            metrics.gap,
            advance.small,
            f32::from(self.typeface.body_small),
        ))
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
        );
        (metrics, tokens)
    }

    /// The drawn columns for a list this wide: the row is the list less its scroll lane.
    pub fn layout(&mut self, width: f32, window: &Window) -> (Metrics, Tokens, Layout) {
        let (metrics, mut tokens) = self.measure(window);
        if self.view.tags_shown {
            tokens.name_min += flea::ui_logic_pl4::tags::ROW_SLOT + metrics.gap;
        }
        let layout = columns::resolve(
            (width - metrics.lane).max(0.),
            &tokens,
            self.view.switched,
            self.view.stored,
        );
        (
            metrics,
            tokens,
            super::tags::reserve(layout, self.view.tags_shown, metrics.gap),
        )
    }
}
