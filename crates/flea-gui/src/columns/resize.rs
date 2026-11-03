//! Resize state, persistence and width constraints shared by every Columns hairline.
use super::geometry::{Layout, Line};
use super::preview_host::{EDGE_MARK, EDGE_ZONE, START_DRAG_DISTANCE};

/// Hairlines resize their left track live, the third takes the difference, and release stores the share once.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edge {
    pub dragging: bool,
    /// The pointer is over the zone, which shows the line and mark at rest.
    pub hovered: bool,
    /// The hairline hovered or held.
    pub at: Line,
    pulled: bool,
    press_x: f32,
    from: f32,
    from_widths: Vec<f32>,
    /// dragLast: the live third width while a pulled drag holds it.
    pub last: Option<f32>,
    /// The live widths left of the third while a column hairline's pulled drag holds them.
    pub widths: Option<Vec<f32>>,
}

/// What the edge asks Settings to store: a share of 1 to 100, or 0 for the default third.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Store(pub u32);

/// The widths left of the third in ten-thousandths of their room, as dual.split keeps its share; equal widths keep none.
pub fn weights(widths: &[f32]) -> Vec<u32> {
    let room: f32 = widths.iter().sum();
    if room <= 0. || widths.iter().all(|&px| px == widths[0]) {
        return Vec::new();
    }
    widths
        .iter()
        .map(|px| ((10_000. * px / room).round() as u32).clamp(1, 10_000))
        .collect()
}

impl Edge {
    /// The zone's left and width, centred on the hairline just left of the third column.
    pub fn zone(layout: &Layout) -> (f32, f32) {
        Self::zone_at(layout, Line::Preview).unwrap_or_default()
    }

    /// Any hairline's zone, centred on its 1 px line, the last pixel of the track to its left.
    pub fn zone_at(layout: &Layout, line: Line) -> Option<(f32, f32)> {
        let x = layout.line_x(line)?;
        Some((x - 1. - (EDGE_ZONE / 2.).floor(), EDGE_ZONE))
    }

    fn x(&self, layout: &Layout) -> f32 {
        layout.line_x(self.at).unwrap_or_else(|| layout.edge())
    }

    /// ResizeHandle's accent line, 1 px on the hairline, while hovered or dragged.
    pub fn line(&self, layout: &Layout, height: f32) -> Option<(f32, f32, f32, f32)> {
        (self.hovered || self.dragging).then(|| (self.x(layout) - 1., 0., 1., height))
    }

    /// The move-horizontal mark, 16 px square, centred on the hairline and on the area's middle row.
    pub fn mark(&self, layout: &Layout, height: f32) -> Option<(f32, f32, f32)> {
        (self.hovered || self.dragging).then(|| {
            (
                self.x(layout) - 0.5 - EDGE_MARK / 2.,
                ((height - EDGE_MARK) / 2.).round(),
                EDGE_MARK,
            )
        })
    }

    pub fn press(&mut self, x: f32, layout: &Layout) {
        self.press_at(x, Line::Preview, layout);
    }

    pub fn press_at(&mut self, x: f32, line: Line, layout: &Layout) {
        self.at = line;
        self.press_x = x;
        self.dragging = true;
        self.pulled = false;
        self.from = layout.third_width;
        self.from_widths = layout.widths.clone();
    }

    /// A held move: nothing until the pointer passes the drag distance, then the clamped width follows it.
    pub fn moved(&mut self, x: f32, layout: &Layout) -> bool {
        if !self.dragging {
            return false;
        }
        let shift = (x - self.press_x).round();
        if !self.pulled && shift.abs() > START_DRAG_DISTANCE {
            self.pulled = true;
        }
        if !self.pulled {
            return false;
        }
        let (last, widths) = match self.at {
            Line::Preview => (layout.clamp_last(self.from - shift), None),
            Line::Column(at) => {
                let Some(&from) = self.from_widths.get(at) else {
                    return false;
                };
                let grown = (from + shift)
                    .min(from + self.from - layout.min_column)
                    .max(layout.min_column.min(from));
                let mut widths = self.from_widths.clone();
                widths[at] = grown;
                (self.from - (grown - from), Some(widths))
            }
        };
        let changed = self.last != Some(last) || self.widths != widths;
        self.last = Some(last);
        self.widths = widths;
        changed
    }

    /// Release, a lost grab and the edge hidden mid-drag all end the drag, storing the width the user saw.
    pub fn release(&mut self, width: f32) -> Option<Store> {
        self.dragging = false;
        self.last
            .take()
            .filter(|_| width > 0.)
            .map(|last| Store(((100. * last / width).round() as u32).clamp(1, 100)))
    }

    /// A double click restores the default third and stores 0.
    pub fn restore(&mut self) -> Store {
        self.dragging = false;
        self.last = None;
        self.widths = None;
        Store(0)
    }

    /// Reset takes preview room first, then spare room in other tracks without crossing their minima.
    pub fn restore_column(&mut self, at: usize, layout: &Layout) -> Option<(Store, Vec<f32>)> {
        self.dragging = false;
        self.last = None;
        self.widths = None;
        let from = *layout.widths.get(at)?;
        let default = (layout.width / layout.count as f32)
            .floor()
            .max(layout.min_column);
        let mut widths = layout.widths.clone();
        widths[at] = default;
        let mut needed = (default - from + layout.min_column - layout.third_width).max(0.);
        for (other, width) in widths.iter_mut().enumerate().rev() {
            if other != at && needed > 0. {
                let take = needed.min((*width - layout.min_column).max(0.));
                *width -= take;
                needed -= take;
            }
        }
        let third = layout.width - widths.iter().sum::<f32>();
        if widths.iter().all(|&px| px == default)
            && third == layout.width - default * widths.len() as f32
        {
            return Some((Store(0), Vec::new()));
        }
        let share = ((100. * third / layout.width.max(1.)).round() as u32).clamp(1, 100);
        Some((Store(share), widths))
    }
}

/// The preview edge as the area drives it: every drag step re-lays the tracks, and a release stores the share once.
impl super::Area {
    /// Settings View's previewWidth, the third column's share in percent, 0 for the default third.
    pub fn set_share(&mut self, share: f64) {
        self.share = share;
        self.relayout();
    }

    /// Settings View's preview.tracks, each column's weight left of the third, empty for equal columns.
    pub fn set_weights(&mut self, weights: Vec<f32>) {
        self.weights = weights;
        self.relayout();
    }

    /// The edge's stored share: a drag's release writes its percent once, a double click 0.
    pub fn store_edge(&mut self, store: Option<Store>) -> Option<u32> {
        let store = store?;
        self.share = f64::from(store.0);
        self.stored.push(store.0);
        self.relayout();
        Some(store.0)
    }

    /// Tracks right of a dragged line absorb whole-percent rounding without crossing their minima.
    fn store_tracks(&mut self, at: usize, mut widths: Vec<f32>, mut store: Store) -> Option<u32> {
        if store.0 > 0 {
            let reserved = widths.iter().take(at + 1).sum::<f32>()
                + widths.len().saturating_sub(at + 1) as f32 * self.layout.min_column;
            let max_share = ((100. * (self.layout.width - reserved) / self.layout.width.max(1.))
                .floor() as u32)
                .clamp(1, 100);
            store.0 = store.0.min(max_share);
            let third = self.layout.clamp_last(
                (f64::from(self.layout.width) * f64::from(store.0) / 100.).round() as f32,
            );
            let room: f32 = widths.iter().sum();
            let mut correction = self.layout.width - third - room;
            for next in widths.iter_mut().skip(at + 1) {
                let change = correction.max(self.layout.min_column - *next);
                *next += change;
                correction -= change;
                if correction == 0. {
                    break;
                }
            }
        }
        let weights = weights(&widths);
        self.weights = weights.iter().map(|&w| w as f32).collect();
        self.stored_tracks = Some(weights);
        self.store_edge(Some(store))
    }

    /// The pointer moved over the area: a column hairline is always hot, the preview edge only while the third shows.
    pub fn edge_hover(&mut self, x: f32) {
        let third = self.host.third_shown(self.settings);
        let hot = self.layout.lines().find(|&line| {
            (third || line != Line::Preview)
                && Edge::zone_at(&self.layout, line)
                    .is_some_and(|(left, width)| (left..left + width).contains(&x))
        });
        self.host.edge.hovered = hot.is_some();
        if let Some(line) = hot.filter(|_| !self.host.edge.dragging) {
            self.host.edge.at = line;
        }
    }

    pub fn edge_press(&mut self, x: f32) {
        let at = self.host.edge.at;
        self.host.edge.press_at(x, at, &self.layout);
    }

    pub fn edge_move(&mut self, x: f32) -> bool {
        let moved = self.host.edge.moved(x, &self.layout);
        if moved {
            self.relayout();
        }
        moved
    }

    /// Release and a lost grab both end the drag, storing the width the user saw.
    pub fn edge_release(&mut self) -> Option<u32> {
        let widths = self.host.edge.widths.take();
        let store = self.host.edge.release(self.layout.width);
        let stored = match (self.host.edge.at, widths, store) {
            (Line::Column(at), Some(widths), Some(store)) => self.store_tracks(at, widths, store),
            _ => self.store_edge(store),
        };
        self.relayout();
        stored
    }

    pub fn edge_restore(&mut self) -> u32 {
        if let Line::Column(at) = self.host.edge.at {
            if let Some((store, widths)) = self.host.edge.restore_column(at, &self.layout) {
                return self.store_tracks(at, widths, store).unwrap_or(0);
            }
        }
        let store = self.host.edge.restore();
        self.store_edge(Some(store)).unwrap_or(0)
    }
}
