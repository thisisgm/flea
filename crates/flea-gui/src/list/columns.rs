//! Which list columns a pane of a given width draws and where, after ui/js/Columns.js: oversized stored widths shrink before a switched-on column drops.
use flea::jsondoc::Json;

/// The four optional columns; the name is the one column that never drops.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    Mode,
    Size,
    Date,
    Kind,
}

impl Column {
    /// Left to right, the order a row draws them in.
    pub const ALL: [Column; 4] = [Column::Mode, Column::Size, Column::Date, Column::Kind];
    /// Widest first, the order they drop in: kind repeats the mark and the extension, mode is the permanent column.
    pub const DROP_ORDER: [Column; 4] = [Column::Kind, Column::Date, Column::Size, Column::Mode];

    pub fn key(self) -> &'static str {
        match self {
            Column::Mode => "mode",
            Column::Size => "size",
            Column::Date => "date",
            Column::Kind => "kind",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.key() == key)
    }

    /// The column's place in ALL, which every per-column array uses.
    pub fn slot(self) -> usize {
        self as usize
    }
}

/// The columns the user switched on, ui.json "columns"; a fresh install shows the name, size and date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Switched([bool; 4]);

impl Default for Switched {
    fn default() -> Self {
        Self([false, true, true, false])
    }
}

impl Switched {
    /// ViewState.columns: an array names what is shown, and anything else keeps the default.
    pub fn from_settings(data: &Json) -> Self {
        let Some(names) = data.get("columns").and_then(Json::as_array) else {
            return Self::default();
        };
        let mut on = [false; 4];
        for column in names
            .iter()
            .filter_map(Json::as_str)
            .filter_map(Column::from_key)
        {
            on[column.slot()] = true;
        }
        Self(on)
    }

    pub fn with(mut self, column: Column, on: bool) -> Self {
        self.0[column.slot()] = on;
        self
    }

    pub fn on(self, column: Column) -> bool {
        self.0[column.slot()]
    }

    /// Columns.dualSet: a Dual pane never draws Mode or Kind, whatever the List shows.
    pub fn for_dual(self, dual: bool) -> Self {
        if dual {
            self.with(Column::Mode, false).with(Column::Kind, false)
        } else {
            self
        }
    }
}

/// The rails a dragged or fitted edge clamps to, src/uischema.rs's column width rule.
pub const MIN_WIDTH: u32 = 48;
pub const MAX_WIDTH: u32 = 480;

/// Columns.clampListWidth(): one whole pixel inside the rails, whatever was handed in.
pub fn clamp_width(px: f64) -> u32 {
    let n = px.round();
    if n.is_nan() || n < f64::from(MIN_WIDTH) {
        MIN_WIDTH
    } else {
        n.min(f64::from(MAX_WIDTH)) as u32
    }
}

/// Columns.autofitWidth(): the widest held cell, clamped; an empty window keeps the width it has.
pub fn autofit_width(widths: &[f32], fallback: f64) -> u32 {
    if widths.is_empty() {
        return clamp_width(fallback);
    }
    let best = widths
        .iter()
        .map(|width| width.round())
        .filter(|width| *width >= 0.)
        .fold(MIN_WIDTH as f32, f32::max);
    clamp_width(f64::from(best))
}

/// ui.json "columnWidths": a finite number at or above zero, rounded and clamped; any other value reads as the measured width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoredWidths([Option<u32>; 4]);

impl StoredWidths {
    pub fn from_settings(data: &Json) -> Self {
        let mut widths = [None; 4];
        if let Some(Json::Obj(pairs)) = data.get("columnWidths") {
            for column in Column::ALL {
                let stored = pairs
                    .iter()
                    .rev()
                    .find(|(key, _)| key == column.key())
                    .and_then(|(_, value)| value.as_f64());
                widths[column.slot()] = stored
                    .filter(|n| n.is_finite() && *n >= 0.)
                    .map(|n| clamp_width(n.round()));
            }
        }
        Self(widths)
    }

    pub fn get(self, column: Column) -> Option<u32> {
        self.0[column.slot()]
    }

    pub fn with(mut self, column: Column, width: u32) -> Self {
        self.0[column.slot()] = Some(clamp_width(f64::from(width)));
        self
    }
}

/// The resolved geometry Columns.js reads: padding, the mark slot, the gap, the name's floor and each column's measured width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub pad: f32,
    pub icon: f32,
    pub gap: f32,
    pub name_min: f32,
    pub measured: [f32; 4],
    /// Row.qml in Dual: Size sits flush against Modified, with no gap between them.
    pub flush_size: bool,
}

// Theme.qml's character counts: "rwxr-xr-x", "1000.0 kB", "2026-09-12 15:29", and the name floor of twenty.
const MODE_CHARS: f32 = 9.;
const SIZE_CHARS: f32 = 9.;
const DATE_CHARS: f32 = 16.;
const NAME_MIN_CHARS: f32 = 20.;
// Kind text varies too much for a character count: 120 px at the OEM's base small size of 12.
const KIND_BASE: f32 = 120.;
const KIND_BASE_SIZE: f32 = 12.;

impl Tokens {
    /// Every width from the small-body advance of "0", the unit the fixed columns are written in.
    pub fn measure(pad: f32, icon: f32, gap: f32, advance: f32, body_small: f32) -> Self {
        let chars = |count: f32| (count * advance).round();
        Self {
            pad,
            icon,
            gap,
            name_min: chars(NAME_MIN_CHARS),
            measured: [
                chars(MODE_CHARS),
                chars(SIZE_CHARS),
                chars(DATE_CHARS),
                (KIND_BASE * body_small / KIND_BASE_SIZE).round(),
            ],
            flush_size: false,
        }
    }

    /// Dual metadata cells abut; the mark-to-name gap remains in the leading slot.
    pub fn for_dual(mut self, dual: bool) -> Self {
        if dual {
            self.icon += self.gap;
            self.gap = 0.;
        }
        self
    }

    /// The width a column wants: the stored one, else the measured one.
    pub fn wanted(&self, column: Column, stored: StoredWidths) -> f32 {
        stored
            .get(column)
            .map_or(self.measured[column.slot()], |width| width as f32)
    }

    /// The name's slot at its floor: the padding either side, the mark and the gap after it.
    fn base(&self) -> f32 {
        2. * self.pad + self.icon + self.gap + self.name_min
    }
}

/// One drawn cell, in row coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cell {
    pub x: f32,
    pub width: f32,
}

impl Cell {
    pub fn right(self) -> f32 {
        self.x + self.width
    }
}

/// The drawn set for one row width: the name's cell and each column's, or None where it dropped or is switched off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layout {
    pub name: Cell,
    pub cells: [Option<Cell>; 4],
    /// Recent uses the Mode geometry for the fixed Location column.
    pub recent_location: bool,
}

impl Layout {
    pub fn cell(&self, column: Column) -> Option<Cell> {
        self.cells[column.slot()]
    }

    pub fn shown(&self, column: Column) -> bool {
        self.cell(column).is_some()
    }

    /// Columns.names(): the drawn set left to right, "name,size,date" on a fresh install.
    pub fn names(&self) -> String {
        let mut out = String::from("name");
        for column in Column::ALL.into_iter().filter(|column| self.shown(*column)) {
            out.push(',');
            out.push_str(if self.recent_location && column == Column::Mode {
                "location"
            } else {
                column.key()
            });
        }
        out
    }

    /// The same set with one column drawn at a drag's preview width, so the header follows the pointer and the rows do not.
    pub fn previewing(&self, column: Column, width: f32, tokens: &Tokens, row: f32) -> Self {
        let mut widths = [0.; 4];
        for each in Column::ALL {
            widths[each.slot()] = self.cell(each).map_or(0., |cell| cell.width);
        }
        if self.shown(column) {
            widths[column.slot()] = width;
        }
        let on = Column::ALL.map(|each| self.shown(each));
        Self {
            recent_location: self.recent_location,
            ..chain(row, tokens, on, widths)
        }
    }
}

/// Resolves the drawn set for a row `row` wide, the list less its scroll lane. Only switched-on columns take room;
/// each may shrink from its wanted width to the smaller of that and its measured width, and a column drops, in
/// DROP_ORDER, only when those floors and the name's floor cannot all fit. The stored widths are never rewritten.
pub fn resolve(row: f32, tokens: &Tokens, switched: Switched, stored: StoredWidths) -> Layout {
    let wanted = Column::ALL.map(|column| tokens.wanted(column, stored));
    let least = Column::ALL.map(|column| wanted[column.slot()].min(tokens.measured[column.slot()]));
    let mut on = Column::ALL.map(|column| switched.on(column));
    let need = |on: &[bool; 4], widths: &[f32; 4]| {
        tokens.base()
            + Column::ALL
                .iter()
                .filter(|column| on[column.slot()])
                .map(|column| widths[column.slot()] + tokens.gap)
                .sum::<f32>()
    };
    for column in Column::DROP_ORDER {
        if need(&on, &least) <= row {
            break;
        }
        on[column.slot()] = false;
    }
    let mut widths = wanted;
    let excess = (need(&on, &wanted) - row).ceil();
    if excess > 0. {
        let give = Column::ALL.map(|column| {
            if on[column.slot()] {
                wanted[column.slot()] - least[column.slot()]
            } else {
                0.
            }
        });
        let total: f32 = give.iter().sum();
        let mut left = excess.min(total);
        if total > 0. {
            for column in Column::ALL {
                let cut = (excess * give[column.slot()] / total).floor().min(left);
                widths[column.slot()] -= cut;
                left -= cut;
            }
        }
        // The pixels the proportional cut leaves go to the widest-first columns, so kind gives before mode.
        for column in Column::DROP_ORDER
            .into_iter()
            .filter(|column| on[column.slot()])
        {
            let cut = left
                .min(widths[column.slot()] - least[column.slot()])
                .max(0.);
            widths[column.slot()] -= cut;
            left -= cut;
        }
    }
    chain(row, tokens, on, widths)
}

// Lays the chain from the right edge: each drawn column takes its width and the gap before it, and the name keeps the rest.
pub(super) fn chain(row: f32, tokens: &Tokens, on: [bool; 4], widths: [f32; 4]) -> Layout {
    let mut cells = [None; 4];
    let mut right = row - tokens.pad;
    for column in Column::DROP_ORDER {
        if on[column.slot()] {
            let width = widths[column.slot()];
            cells[column.slot()] = Some(Cell {
                x: right - width,
                width,
            });
            let flush = tokens.flush_size && column == Column::Date && on[Column::Size.slot()];
            right -= width + if flush { 0. } else { tokens.gap };
        }
    }
    let x = tokens.pad + tokens.icon + tokens.gap;
    Layout {
        name: Cell {
            x,
            width: (right - x).max(0.),
        },
        cells,
        recent_location: false,
    }
}
