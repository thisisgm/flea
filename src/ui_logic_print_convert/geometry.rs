//! Composition coordinates in output pixels, fitted only after the backend geometry is known.
use super::composition::{resized_size, shared_size, Layout, Settings};
use super::output::MAX_ITEMS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Composition {
    pub width: f64,
    pub height: f64,
    pub tiles: Vec<Rect>,
}

impl Composition {
    pub fn new(sizes: &[(u32, u32)], settings: &Settings) -> Result<Self, &'static str> {
        if sizes.len() < 2 || sizes.len() > MAX_ITEMS {
            return Err("dialogs.output.error.image_count");
        }
        let cell =
            shared_size(settings.layout, sizes).map_err(|_| "dialogs.output.error.dimensions")?;
        if settings.layout == Layout::Grid && !(1..=MAX_ITEMS).contains(&settings.columns) {
            return Err("dialogs.output.error.columns");
        }
        let gap = f64::from(settings.spacing.pixels(settings.edge(cell)));
        let (cw, ch) = (f64::from(cell.0), f64::from(cell.1));
        let mut x = gap;
        let mut y = gap;
        let tiles = sizes
            .iter()
            .enumerate()
            .map(|(i, &(w, h))| {
                let (width, height) = resized_size(settings.layout, (w, h), cell);
                let (width, height) = (f64::from(width), f64::from(height));
                match settings.layout {
                    Layout::Row => {
                        let rect = Rect {
                            x,
                            y: gap,
                            width,
                            height: ch,
                        };
                        x += width + gap;
                        rect
                    }
                    Layout::Column => {
                        let rect = Rect {
                            x: gap,
                            y,
                            width: cw,
                            height,
                        };
                        y += height + gap;
                        rect
                    }
                    Layout::Grid => Rect {
                        x: gap
                            + (i % settings.columns) as f64 * (cw + gap)
                            + ((cw - width) / 2.).floor(),
                        y: gap
                            + (i / settings.columns) as f64 * (ch + gap)
                            + ((ch - height) / 2.).floor(),
                        width,
                        height,
                    },
                }
            })
            .collect();
        let (width, height) = match settings.layout {
            Layout::Row => (x, ch + 2. * gap),
            Layout::Column => (cw + 2. * gap, y),
            Layout::Grid => {
                let rows = sizes.len().div_ceil(settings.columns);
                (
                    settings.columns as f64 * cw + (settings.columns + 1) as f64 * gap,
                    rows as f64 * ch + (rows + 1) as f64 * gap,
                )
            }
        };
        Ok(Self {
            width,
            height,
            tiles,
        })
    }

    pub fn fitted(&self, width: f64, height: f64, padding: f64) -> Self {
        let scale = ((width - 2. * padding).max(0.) / self.width)
            .min((height - 2. * padding).max(0.) / self.height);
        let (w, h) = (self.width * scale, self.height * scale);
        let minimum = if scale > 0. { 1. } else { 0. };
        let (ox, oy) = (((width - w) / 2.).round(), ((height - h) / 2.).round());
        Self {
            width: w,
            height: h,
            tiles: self
                .tiles
                .iter()
                .map(|r| {
                    let (x, y) = ((r.x * scale).round(), (r.y * scale).round());
                    Rect {
                        x: ox + x,
                        y: oy + y,
                        width: (((r.x + r.width) * scale).round() - x).max(minimum),
                        height: (((r.y + r.height) * scale).round() - y).max(minimum),
                    }
                })
                .collect(),
        }
    }
}
