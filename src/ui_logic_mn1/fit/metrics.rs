//! The menu board tokens at one live body size, shared by fitting, painting, input and readers.
use super::{rows, Cut, Entry, Rect, RowParts};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    ratio: f32,
    pub row: i32,
    pub sep: i32,
    pub pad: i32,
    pub menu_w: f32,
    pub pad_x: f32,
    pub gap: f32,
    pub slot: f32,
    pub chevron: f32,
    pub clear: i32,
    pub cue_give: i32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::at(14.)
    }
}

impl Metrics {
    pub fn at(body: f32) -> Self {
        let scale = |value: f32| (value * body / 14.).round();
        let row = scale(super::ROW as f32) as i32;
        let clear = (row as f32 * 0.7).round() as i32;
        Self {
            ratio: body / 14.,
            row,
            sep: scale(super::SEP as f32) as i32,
            pad: scale(super::PAD as f32) as i32,
            menu_w: scale(super::MENU_W),
            pad_x: scale(super::PAD_X),
            gap: scale(super::GAP),
            slot: scale(super::SLOT),
            chevron: scale(super::CHEVRON),
            clear,
            cue_give: clear / 2,
        }
    }

    pub fn of(&self, pixels: f32) -> f32 {
        (pixels * self.ratio).round()
    }

    pub fn height(&self, entry: &Entry) -> i32 {
        if entry.separator {
            self.sep
        } else {
            self.row
        }
    }

    pub fn content_height(&self, items: &[Entry]) -> i32 {
        items.iter().map(|entry| self.height(entry)).sum()
    }

    pub fn row_top(&self, items: &[Entry], index: usize) -> i32 {
        items[..index.min(items.len())]
            .iter()
            .map(|entry| self.height(entry))
            .sum()
    }

    pub fn row_width(&self, parts: RowParts) -> f32 {
        let mut width = 2. + self.pad_x + self.slot + self.gap + parts.label + self.pad_x;
        if parts.word > 0. {
            width += self.gap + parts.word;
        }
        if parts.key > 0. {
            width += self.gap + parts.key;
        }
        if parts.chevron || parts.word > 0. {
            width += self.gap + self.chevron;
        }
        width
    }

    pub fn fit_width(&self, widest: f32, work_width: f32) -> f32 {
        self.menu_w.max(widest.ceil()).min(work_width).max(0.)
    }

    pub fn flyout_width(&self, parent: f32, widest: f32, work_width: f32) -> f32 {
        parent
            .max(self.menu_w)
            .max(widest.ceil())
            .min(work_width)
            .max(0.)
    }

    pub fn place(
        &self,
        point: (f32, f32),
        width: f32,
        content: i32,
        work: Rect,
        inset: f32,
    ) -> Rect {
        let h = ((content + 2 * self.pad) as f32)
            .min(work.h - 2. * inset)
            .max(0.);
        let x = work.x + rows::clamp(point.0 - work.x, width, work.w);
        let y = work.y
            + inset
            + rows::clamp(point.1 - work.y - inset, h, (work.h - 2. * inset).max(0.));
        Rect { x, y, w: width, h }
    }

    pub fn place_flyout(
        &self,
        frame: Rect,
        width: f32,
        content: i32,
        row_screen_top: f32,
        work: Rect,
        inset: f32,
    ) -> Rect {
        self.place_flyout_insets(frame, width, content, row_screen_top, work, (inset, inset))
    }

    pub fn place_flyout_insets(
        &self,
        frame: Rect,
        width: f32,
        content: i32,
        row_screen_top: f32,
        work: Rect,
        insets: (f32, f32),
    ) -> Rect {
        let (horizontal, vertical) = insets;
        let h = ((content + 2 * self.pad) as f32)
            .min(work.h - 2. * vertical)
            .max(0.);
        let right = work.x + work.w;
        let x = if frame.x + frame.w + width <= right {
            frame.x + frame.w
        } else if frame.x - width >= work.x {
            frame.x - width
        } else {
            (right - horizontal - width).max(work.x)
        };
        let y = (row_screen_top - self.pad as f32)
            .min(work.y + work.h - vertical - h)
            .max(work.y + vertical);
        Rect { x, y, w: width, h }
    }

    pub fn reveal(&self, items: &[Entry], index: usize, content_y: i32, view_h: i32) -> i32 {
        let top = self.row_top(items, index);
        let end = self.content_height(items) - view_h;
        if end <= 0 {
            return 0;
        }
        if top < content_y + self.clear {
            return (top - self.clear).max(0);
        }
        if top + self.row > content_y + view_h - self.clear {
            return end.min(top + self.row - view_h + self.clear);
        }
        content_y
    }

    pub fn edge_cuts(&self, items: &[Entry], content_y: i32, view_h: i32) -> (Cut, Cut) {
        let (mut top, mut bottom, mut y) = (None, None, 0);
        for (index, item) in items.iter().enumerate() {
            let h = self.height(item);
            if y < content_y && content_y < y + h {
                top = Some((index, y + h - content_y));
            }
            if y < content_y + view_h && content_y + view_h < y + h {
                bottom = Some((index, content_y + view_h - y));
            }
            y += h;
        }
        (top, bottom)
    }

    fn half(&self, items: &[Entry], cut: Cut) -> bool {
        cut.is_some_and(|(index, shown)| {
            !items[index].separator
                && (self.row / 2 - 1..self.row - self.row / 2 + 2).contains(&shown)
        })
    }

    pub fn cue_ok(&self, items: &[Entry], content_y: i32, view_h: i32) -> bool {
        let (top, bottom) = self.edge_cuts(items, content_y, view_h);
        (content_y <= 0 || self.half(items, top))
            && (content_y + view_h >= self.content_height(items) || self.half(items, bottom))
    }

    pub fn keeps_clear(
        &self,
        items: &[Entry],
        keep: Option<usize>,
        content_y: i32,
        view_h: i32,
    ) -> bool {
        let Some(index) = keep else { return true };
        let top = self.row_top(items, index) - content_y;
        let reach = self.clear - self.cue_give;
        (content_y == 0 || top >= reach)
            && (content_y + view_h >= self.content_height(items)
                || top + self.row <= view_h - reach)
    }

    fn balance(&self, items: &[Entry], content_y: i32, view_h: i32) -> i32 {
        let (top, bottom) = self.edge_cuts(items, content_y, view_h);
        [top, bottom]
            .iter()
            .flatten()
            .map(|(_, shown)| (2 * shown - self.row).abs())
            .sum()
    }

    pub fn menu_cut(
        &self,
        items: &[Entry],
        content_y: i32,
        view_h: i32,
        keep: Option<usize>,
    ) -> (i32, i32) {
        let end = self.content_height(items) - view_h;
        if end <= 0 {
            return (0, view_h);
        }
        let fits = |y: i32, h: i32| self.cue_ok(items, y, h) && self.keeps_clear(items, keep, y, h);
        if fits(content_y, view_h) {
            return (content_y, view_h);
        }
        let best = (0..=end).filter(|y| fits(*y, view_h)).min_by_key(|y| {
            (
                (y - content_y).abs() / self.row,
                self.balance(items, *y, view_h),
                (y - content_y).abs(),
            )
        });
        if let Some(y) = best {
            return (y, view_h);
        }
        // Rows overflowing by less than half a row cannot scroll to a half row, so the view stops at one instead.
        let trim = |from: i32| {
            (from..view_h)
                .rev()
                .filter(|h| fits(content_y, *h))
                .min_by_key(|h| (self.balance(items, content_y, *h), -h))
        };
        // An edge just under a separator reaches the half of the row above it only past that separator too.
        if let Some(h) =
            trim(view_h - self.row + 1).or_else(|| trim(view_h - self.row - self.sep + 1))
        {
            return (content_y, h);
        }
        // A view a pixel or two short of two half rows plus whole ones moves within a row to the scroll whose edge rows sit nearest half.
        let rows_cut = |y: i32| {
            let (top, bottom) = self.edge_cuts(items, y, view_h);
            let row = |cut: Cut| cut.is_some_and(|(index, _)| !items[index].separator);
            (y <= 0 || row(top)) && (y + view_h >= self.content_height(items) || row(bottom))
        };
        let worst = |y: i32| {
            let (top, bottom) = self.edge_cuts(items, y, view_h);
            [top, bottom]
                .iter()
                .flatten()
                .map(|(_, shown)| (2 * shown - self.row).abs())
                .max()
        };
        (0..=end)
            .filter(|y| rows_cut(*y) && self.keeps_clear(items, keep, *y, view_h))
            .min_by_key(|y| {
                (
                    (y - content_y).abs() / self.row,
                    worst(*y),
                    self.balance(items, *y, view_h),
                    (y - content_y).abs(),
                )
            })
            .map_or((content_y, view_h), |y| (y, view_h))
    }

    pub fn scroll_for(
        &self,
        items: &[Entry],
        cursor: usize,
        content_y: i32,
        view_h: i32,
    ) -> (i32, i32) {
        self.menu_cut(
            items,
            self.reveal(items, cursor, content_y, view_h),
            view_h,
            Some(cursor),
        )
    }

    pub fn fades(&self, items: &[Entry], content_y: i32, view_h: i32) -> (bool, bool) {
        (
            content_y > 0,
            content_y + view_h < self.content_height(items),
        )
    }
}
