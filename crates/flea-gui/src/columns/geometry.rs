//! ui/js/Columns.js arithmetic: how many tracks a width draws, where each sits and which cells a row keeps.

pub const COUNT_4_AT: f32 = 1700.;
pub const COUNT_5_AT: f32 = 2300.;
pub const COUNT_NARROW_AT: f32 = 900.;
pub const COUNT_MIN: usize = 2;
pub const COUNT_MAX: usize = 5;
// The shipped View limit: a default install holds 3 columns from 900 px up.
pub const COUNT_DEFAULT: usize = 3;

// Board columns.html at body 14: row padding, the 23 mark slot, the 9 gap, the 70 size cell, the 12 chevron.
pub const PAD_X: f32 = 14.;
pub const MARK_SLOT: f32 = 23.;
// Theme.qml bodySmall at body 14, Omarchy's 0.917 step rounded.
const BODY_SMALL: f64 = 13.;
// v0.3.12 ColumnRow.qml: Glyph caps the slot at Theme.markSize, bodySmall x 1.45, the list row's mark.
pub const MARK: f32 = (BODY_SMALL * crate::list::MARK_RATIO) as f32;
pub const GAP: f32 = 9.;
pub const SIZE_W: f32 = 70.;
pub const CHEVRON: f32 = 12.;
pub const NAME_X: f32 = PAD_X + MARK_SLOT + GAP;
// Scroll.lane: every column keeps a lane as wide as its row padding clear at its right edge; LANE is body 14's.
pub const LANE: f32 = PAD_X;

/// ColumnPane.qml's Scroll.contentWidth lane, Theme.spacing.rowPaddingX at the face's size.
pub fn lane(face: &crate::list::Typeface) -> f32 {
    crate::list::Metrics::for_typeface(face).lane
}
pub const DIVIDER: f32 = 1.;

/// ColumnPane.qml's footer band at body 14: the peek tracks draw fixed-height rows, so their footer is fixed too.
pub const FOOTER: f32 = 7.;

/// The active track's scroll content: its rows, the refused rename's band and the footer.
pub fn active_content(
    grow: crate::list::pane::Grow,
    total: usize,
    row: f32,
    face: &crate::list::Typeface,
) -> f32 {
    let metrics = crate::list::Metrics::for_typeface(face);
    let footer = (metrics.nominal_row - metrics.slot) / 2.;
    grow.content(total, row) + footer
}

/// The row mark's square from the row's left and top: centred in its slot and on a row `band` high, as Glyph centres it.
pub fn mark_box(band: f32) -> (f32, f32, f32) {
    (PAD_X + (MARK_SLOT - MARK) / 2., (band - MARK) / 2., MARK)
}

/// A row thumbnail's square from the row's left and top: the mark slot, centred on the row and never taller than it.
pub fn picture_box(band: f32) -> (f32, f32, f32) {
    let side = MARK_SLOT.min(band);
    (PAD_X + (MARK_SLOT - side) / 2., (band - side) / 2., side)
}

/// cappedLimit: a stored number clamps to 2..5, and anything that is not a number reads as the shipped default.
pub fn capped_limit(limit: Option<f64>) -> usize {
    let Some(n) = limit.map(f64::floor) else {
        return COUNT_DEFAULT;
    };
    if n.is_nan() {
        COUNT_DEFAULT
    } else {
        n.clamp(COUNT_MIN as f64, COUNT_MAX as f64) as usize
    }
}

/// columnCountForWidth: one integer for a window of this width, so a resize re-lays tracks without a read.
pub fn count_for_width(width: f32, limit: Option<f64>) -> usize {
    let count = if width < COUNT_NARROW_AT {
        2
    } else if width < COUNT_4_AT {
        3
    } else if width < COUNT_5_AT {
        4
    } else {
        5
    };
    count.min(capped_limit(limit)).max(COUNT_MIN)
}

pub fn ancestors_for_count(count: usize) -> usize {
    if count < COUNT_MIN {
        0
    } else {
        count.min(COUNT_MAX) - 2
    }
}

/// ancestorParent: one step up, a trailing slash trimmed except at the root, Nav.parentOf's rule.
pub use crate::navigation::history::parent_of as ancestor_parent;

/// The nearest distinct ancestors of a path, oldest first, stopping where the climb stops.
pub fn ancestors(path: &str, count: usize) -> Vec<String> {
    let mut near: Vec<String> = Vec::new();
    if path.is_empty() {
        return near;
    }
    let mut current = if path.len() > 1 {
        path.strip_suffix('/').unwrap_or(path)
    } else {
        path
    };
    for _ in 0..count {
        let parent = ancestor_parent(current);
        if parent == current || near.iter().any(|seen| seen == parent) {
            break;
        }
        near.push(parent.to_owned());
        current = parent;
    }
    near.reverse();
    near
}

pub fn ancestor_shown(path: &str, depth: usize) -> bool {
    depth >= 1 && ancestors(path, depth).len() >= depth
}

pub fn neighbour_asks(path: &str, width: f32, limit: Option<f64>) -> Vec<String> {
    ancestors(path, ancestors_for_count(count_for_width(width, limit)))
}

/// keepAsks: the drawn set becomes the asks plus the child, and again names a column not drawn at the last refresh.
pub fn keep_asks(drawn: &mut Vec<String>, asks: &[String], child: &str) -> Vec<(String, bool)> {
    let mut shown = asks.to_vec();
    if !child.is_empty() && !asks.iter().any(|ask| ask == child) {
        shown.push(child.to_owned());
    }
    let plan = shown
        .iter()
        .map(|path| (path.clone(), !drawn.contains(path)))
        .collect();
    *drawn = shown;
    plan
}

pub fn folder_data_hold(cursor_is_dir: bool, answered: bool) -> bool {
    cursor_is_dir && !answered
}

pub fn show_folder_on_peek(child: &str, shown_child: &str, answered: bool) -> bool {
    answered && !child.is_empty() && child != shown_child
}

pub fn should_settle_hero(
    cursor_is_dir: bool,
    path: &str,
    child: &str,
    read_failed: bool,
    rows: usize,
) -> bool {
    cursor_is_dir && !read_failed && path == child && rows == 0
}

/// Which tracks a Columns view of this count draws, oldest ancestor first; the Child slot is the third column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    GreatGrandparent,
    Grandparent,
    Parent,
    Active,
    Child,
}

impl Slot {
    /// The slot's place left to right among all five, which per-slot state is indexed by.
    pub fn index(self) -> usize {
        match self {
            Self::GreatGrandparent => 0,
            Self::Grandparent => 1,
            Self::Parent => 2,
            Self::Active => 3,
            Self::Child => 4,
        }
    }

    /// The ancestor depth a slot names, 0 for the active track and the child.
    pub fn depth(self) -> usize {
        match self {
            Self::GreatGrandparent => 3,
            Self::Grandparent => 2,
            Self::Parent => 1,
            Self::Active | Self::Child => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub slot: Slot,
    pub x: f32,
    pub width: f32,
}

/// ColumnsArea's widths: equal tracks from the area width, the third taking the remainder or its stored share.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub width: f32,
    pub count: usize,
    pub shown_before: usize,
    pub column_width: f32,
    pub third_width: f32,
    pub min_column: f32,
    /// Each track left of the third, left to right; equal unless a hairline drag gave them weights.
    pub widths: Vec<f32>,
}

/// A hairline in Columns: the right edge of the track at this position left of the third, or the preview edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Line {
    #[default]
    Preview,
    Column(usize),
}

impl Layout {
    /// share is Settings View's previewWidth percent, 0 for the default third; drag is the live edge while held.
    pub fn new(width: f32, limit: Option<f64>, share: f64, drag: Option<f32>) -> Self {
        Self::weighted(width, limit, share, drag, &[])
    }

    /// weights share the room left of the third out per track; any other length than the tracks draw keeps them equal.
    pub fn weighted(
        width: f32,
        limit: Option<f64>,
        share: f64,
        drag: Option<f32>,
        weights: &[f32],
    ) -> Self {
        let width = width.max(0.).floor();
        let count = count_for_width(width, limit);
        let shown_before = count - 1;
        let default = (width / count as f32).floor().max(1.);
        let min_column = (COUNT_NARROW_AT / COUNT_DEFAULT as f32)
            .floor()
            .min(default);
        let stored = drag.unwrap_or_else(|| (f64::from(width) * share / 100.).round() as f32);
        let mut layout = Self {
            width,
            count,
            shown_before,
            column_width: default,
            third_width: 0.,
            min_column,
            widths: Vec::new(),
        };
        let mut room = shown_before as f32 * default;
        if stored > 0. {
            let last = layout.clamp_last(stored);
            layout.column_width = ((width - last) / shown_before as f32).floor().max(1.);
            room = width - last;
        }
        let sum: f32 = weights.iter().sum();
        let weighted = weights.len() == shown_before && sum > 0.;
        if !weighted {
            room = shown_before as f32 * layout.column_width;
        }
        layout.third_width = width - room;
        layout.widths = if weighted {
            let mut widths = vec![0.; shown_before];
            let (mut available, mut remaining) = (room, sum);
            while let Some(at) = weights.iter().enumerate().find_map(|(at, weight)| {
                (widths[at] == 0. && available * weight / remaining < min_column).then_some(at)
            }) {
                widths[at] = min_column;
                available -= min_column;
                remaining -= weights[at];
            }
            let mut left = available;
            let mut open = widths.iter().filter(|&&width| width == 0.).count();
            for (at, weight) in weights.iter().enumerate() {
                if widths[at] != 0. {
                    continue;
                }
                open -= 1;
                let px = (available * weight / remaining)
                    .round()
                    .clamp(min_column, left - open as f32 * min_column);
                widths[at] = if open == 0 { left } else { px };
                left -= widths[at];
            }
            widths
        } else {
            vec![layout.column_width; shown_before]
        };
        layout
    }

    /// The x of a track's left edge, the third's at the room's end.
    fn left_of(&self, at: usize) -> f32 {
        self.widths.iter().take(at).sum()
    }

    /// The x just past a hairline's track, where the next track starts; None for a column the count does not draw.
    pub fn line_x(&self, line: Line) -> Option<f32> {
        match line {
            Line::Preview => Some(self.edge()),
            Line::Column(at) if at + 1 < self.shown_before => Some(self.left_of(at + 1)),
            Line::Column(_) => None,
        }
    }

    /// Every hairline that drags, left to right, the preview edge last.
    pub fn lines(&self) -> impl Iterator<Item = Line> {
        (0..self.shown_before.saturating_sub(1))
            .map(Line::Column)
            .chain(std::iter::once(Line::Preview))
    }

    pub fn clamp_last(&self, px: f32) -> f32 {
        px.min(self.width - self.shown_before as f32 * self.min_column)
            .max(self.min_column)
    }

    pub fn active_x(&self) -> f32 {
        self.left_of(self.shown_before - 1)
    }

    /// The preview edge's x, the third column's left.
    pub fn edge(&self) -> f32 {
        self.left_of(self.shown_before)
    }

    /// Every drawn track left to right; an ancestor slot past the climb still keeps its width, blank.
    pub fn tracks(&self) -> impl Iterator<Item = Track> + '_ {
        static SLOTS: [Slot; 5] = [
            Slot::GreatGrandparent,
            Slot::Grandparent,
            Slot::Parent,
            Slot::Active,
            Slot::Child,
        ];
        SLOTS[5 - self.count..]
            .iter()
            .enumerate()
            .map(move |(at, &slot)| Track {
                slot,
                x: self.left_of(at),
                width: self.widths.get(at).copied().unwrap_or(self.third_width),
            })
    }

    pub fn track(&self, slot: Slot) -> Option<Track> {
        self.tracks().find(|track| track.slot == slot)
    }
}

/// One row's cells inside a track, ColumnRow.qml's anchors over the board's numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cells {
    pub mark_x: f32,
    pub name_x: f32,
    pub name_right: f32,
    pub size_right: Option<f32>,
    pub chevron_x: Option<f32>,
}

/// A row's cells at body 14's lane.
pub fn cells(track_width: f32, shows_size: bool, chevron: bool) -> Cells {
    cells_in(track_width, LANE, shows_size, chevron)
}

/// A row's cells in a track that keeps `lane` clear at its right edge.
pub fn cells_in(track_width: f32, lane: f32, shows_size: bool, chevron: bool) -> Cells {
    let content = (track_width - lane).max(0.);
    let chevron_w = if chevron { CHEVRON } else { 0. };
    let size_right = content - PAD_X - chevron_w - if shows_size { GAP } else { 0. };
    Cells {
        mark_x: PAD_X,
        name_x: NAME_X,
        name_right: size_right - if shows_size { SIZE_W + GAP } else { 0. },
        size_right: shows_size.then_some(size_right),
        chevron_x: chevron.then_some(content - PAD_X - CHEVRON),
    }
}

impl Cells {
    /// The clipboard slot's left edge, clear of the name and the metadata.
    pub fn clip_x(&self) -> f32 {
        self.name_right
            - super::paint::CLIP
            - if self.chevron_x.is_none() {
                CHEVRON
            } else {
                0.
            }
    }
}

/// nameBudgetPlain and nameBudgetChevron: whole body advances between the mark and the size cell.
pub fn name_budget(track_width: f32, shows_size: bool, chevron: bool, advance: f32) -> usize {
    name_budget_in(track_width, LANE, shows_size, chevron, advance)
}

/// name_budget in a track that keeps `lane` clear at its right edge.
pub fn name_budget_in(
    track_width: f32,
    lane: f32,
    shows_size: bool,
    chevron: bool,
    advance: f32,
) -> usize {
    if advance <= 0. {
        return 0;
    }
    let c = cells_in(track_width, lane, shows_size, chevron);
    ((c.name_right - c.name_x) / advance).floor().max(0.) as usize
}

/// ScrollOff.cursorY at context 0: the row shown whole with the least travel, clamped to the content.
pub fn contain_y(index: usize, total: usize, scroll: f32, height: f32, row: f32) -> f32 {
    let top = index as f32 * row;
    let rel = if top < scroll {
        top
    } else if top + row > scroll + height {
        top + row - height
    } else {
        scroll
    };
    rel.min(total as f32 * row - height).max(0.)
}
