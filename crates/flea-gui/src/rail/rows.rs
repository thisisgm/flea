//! The rail's rows as one custom Element: headings, marks, labels, trailing slots and the rename field, with the painted boxes published for IPC.
use crate::g2::pl4::content::{DrawnSection, RowMark};
use crate::rail::geometry::Rect;
#[path = "glyph.rs"]
pub(crate) mod glyph;
use crate::list::{Palette, Typeface};
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, TransformationMatrix, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

// ui/Theme.qml: the rail row is 29 px at body 14, its mark 15 px, the labels start at 14 + 15 + 9.
pub const PAD_X: f32 = 14.;
pub const GAP: f32 = 9.;
pub const ICON: f32 = 15.;
pub const PANEL_GAP: f32 = 14.;
pub const TOP: f32 = 14.;
// ui/RailHeading.qml: caption 12, tracking 1, lead 0.15 rounded up, bottom rowGap 9.
pub const HEADING_LEAD: f32 = 0.15;
pub const HEADING_TRACKING: f32 = 1.;
pub const BODY: f32 = 14.;
pub const CAPTION: f32 = 12.;
pub const ROW_GAP: f32 = 9.;
pub const DOT: f32 = 6.;
pub const SLOT: f32 = 12.;
// SidebarRow.qml: railIconSize is 15, the tag swatch is 14 and the dot is 6 inside a caption slot.
pub const TAG: f32 = 14.;
// f64, as Theme.qml computes, so flea.tokens reports the QML value; `as f32` yields the same f32 literal.
pub(crate) const RAIL_ROW_SCALE: f64 = 0.78;
pub(crate) const TEXT_LINE_HEIGHT_FACTOR: f64 = 1.8;
const OEM_BASE: f32 = 12.;

/// Preserve the body-14 recipe; larger text follows the two HIG rail anchors (rulings 170 and 173).
struct RailMetrics {
    row: f32,
    top: f32,
    heading: Option<f32>,
    pad: f32,
    gap: f32,
    panel_gap: f32,
    icon: f32,
    tag: f32,
    dot: f32,
    slot: f32,
}

impl RailMetrics {
    fn for_typeface(face: &Typeface) -> Self {
        let list = crate::list::Metrics::for_typeface(face);
        let base = f32::from(face.base);
        let compressed = base > BODY;
        let anchor = |at14: f32, at20: f32| {
            if base <= 20. {
                (at14 + (at20 - at14) * (base - BODY) / (20. - BODY)).round()
            } else {
                (at20 * base / 20.).round()
            }
        };
        let row = if compressed {
            anchor(29., 41.)
        } else {
            (list.nominal_row * RAIL_ROW_SCALE as f32).round()
        };
        let line = (f32::from(face.body_small) * TEXT_LINE_HEIGHT_FACTOR as f32).round();
        let board = |value: f32| (value * f32::from(face.base) / BODY).round();
        Self {
            row,
            top: if compressed { anchor(9., 8.) } else { list.pad },
            heading: compressed.then(|| anchor(33., 42.)),
            pad: if compressed {
                anchor(PAD_X, 14.)
            } else {
                list.pad
            },
            gap: if compressed {
                anchor(GAP, 9.)
            } else {
                list.gap
            },
            panel_gap: if compressed {
                anchor(11., 10.)
            } else {
                (PANEL_GAP * base / OEM_BASE).round()
            },
            icon: (row - (list.nominal_row - line)).max(1.),
            tag: board(TAG),
            dot: board(DOT),
            slot: f32::from(face.caption),
        }
    }
}

#[cfg(test)]
pub(crate) fn i18n_fit_widths(face: &Typeface) -> (f32, f32) {
    let metrics = RailMetrics::for_typeface(face);
    let width = crate::rail::geometry::DEFAULT_WIDTH;
    (
        width - 2. * metrics.pad - metrics.icon - metrics.gap,
        width - 2. * metrics.pad,
    )
}

/// One heading as painted: the caption's capitals, its box, its lead and its type size.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidHeader {
    pub text: String,
    pub rect: Rect,
    pub top_padding: f32,
    pub text_height: f32,
    pub font_size: f32,
    /// HEADING_TRACKING, or none in a caseless language.
    pub tracking: f32,
    /// The NETWORK heading's plus, in the rows' trailing slot, Sidebar.qml's addGlyph.
    pub add: Option<Rect>,
}

/// One rail row as painted, in window coordinates; IPC reads these and never recomputes a row.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidRow {
    pub index: usize,
    pub label: String,
    pub rect: Rect,
    pub label_rect: Rect,
    pub detail_rect: Rect,
    pub indicator_rect: Rect,
    pub indicator_visible: bool,
    /// The slot draws the eject mark rather than the mount square.
    pub eject: bool,
    pub label_width: f32,
    pub label_needs: f32,
    pub elided: bool,
    pub detail: String,
    pub detail_color: String,
    pub detail_width: f32,
    pub font_size: f32,
    pub glyph: &'static str,
    pub mark: RowMark,
    pub error: bool,
    pub mounted: Option<bool>,
    pub editor: Option<Rect>,
}

impl LaidRow {
    /// SidebarRow.qml's eject hit box: the WCAG floor, centred on the caption-sized ink in the slot.
    pub fn eject_hit(&self) -> Option<Rect> {
        if !self.eject {
            return None;
        }
        let side = crate::tabs::bar::HIT_MIN.max(self.indicator_rect.height);
        Some(Rect::new(
            self.indicator_rect.x + (self.indicator_rect.width - side) / 2.,
            self.indicator_rect.y + (self.indicator_rect.height - side) / 2.,
            side,
            side,
        ))
    }
}

/// Everything one paint put on the rail: the loaded boxes, never an expected rectangle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    #[cfg(test)]
    pub palette: Option<Palette>,
    pub painted: bool,
    pub width: f32,
    pub height: f32,
    pub content_height: f32,
    pub cursor_y: Option<f32>,
    pub row_gap: f32,
    pub headers: Vec<LaidHeader>,
    pub rows: Vec<LaidRow>,
}

impl Laid {
    pub fn row(&self, index: usize) -> Option<&LaidRow> {
        self.rows.iter().find(|row| row.index == index)
    }
}

/// The producer's rail cursor, the place it lights and the one row whose label is a field.
#[derive(Default)]
pub struct RowsModel {
    pub sections: Rc<Vec<DrawnSection>>,
    pub cursor: Option<usize>,
    pub lit: Option<usize>,
    pub focused: bool,
    /// The network row menu suppresses plates without taking the rail's cursor or focus.
    pub menu_open: bool,
    pub renaming: Option<usize>,
    pub editor: crate::path::entry::Entry,
    pub editor_scroll: f32,
    editor_line: Option<Rc<text::Prepared>>,
    pub reveal_cursor: bool,
    pub pointer_focus: bool,
    pub trash_count: u64,
    pub trash_count_shown: bool,
    /// places.driveSize: each DEVICES row prints its size before the trailing slot.
    pub drive_size_shown: bool,
    pub scroll: f32,
    cache: text::Cache,
    pub hovered: Option<usize>,
    /// The row whose eject mark's hit box holds the pointer.
    pub mark_hovered: Option<usize>,
    /// The row whose eject mark holds a left press that has not been released.
    pub mark_pressed: Option<usize>,
    /// The keyboard stands on the cursor row's eject mark, which draws its frame.
    pub mark_focused: bool,
    pub corner: f32,
    /// Presses on the NETWORK heading's plus; the window opens Add a Network Share once per new press.
    pub add_presses: u64,
    /// The row a held row drag would land on and its caption (move here, copy here or link here), RailDrop.qml's cue.
    pub drop_target: Option<(usize, gpui::SharedString)>,
    /// A Favorites row held by its reorder drag, SidebarRow.qml's DragHandler.
    pub reorder: Option<Reorder>,
}

/// The held favourite: its rail row, its store index, the press and active heights, and where the bar lands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reorder {
    pub row: usize,
    pub from: usize,
    pub count: usize,
    pub press_y: f32,
    pub start_y: Option<f32>,
    pub line: usize,
}

impl Reorder {
    /// The rail row the bar borders and whether it lies under that row, Sidebar.qml's boundary.
    pub fn boundary(&self) -> Option<(usize, bool)> {
        let first = self.row.checked_sub(self.from)?;
        self.start_y.map(|_| {
            (
                first + self.line.min(self.count.saturating_sub(1)),
                self.line >= self.count,
            )
        })
    }
}

pub struct RailElement {
    model: Rc<RefCell<RowsModel>>,
    laid: Rc<RefCell<Laid>>,
    typeface: Typeface,
    detail_face: Typeface,
    producer: gpui::Entity<crate::g2::pl4::producer::Producer>,
}

impl RailElement {
    pub fn new(
        model: Rc<RefCell<RowsModel>>,
        laid: Rc<RefCell<Laid>>,
        typeface: Typeface,
        producer: gpui::Entity<crate::g2::pl4::producer::Producer>,
    ) -> Self {
        let detail_face = Typeface {
            font: gpui::Font {
                features: gpui::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])),
                ..typeface.font.clone()
            },
            ..typeface.clone()
        };
        Self {
            model,
            laid,
            typeface,
            detail_face,
            producer,
        }
    }
}

impl RowsModel {
    pub fn new(sections: Rc<Vec<DrawnSection>>) -> Self {
        Self {
            sections,
            cursor: Some(0),
            ..Self::default()
        }
    }
    pub fn editor_x(&self, byte: usize) -> f32 {
        self.editor_line
            .as_ref()
            .map_or(0., |line| line.x_for(byte))
    }
    pub fn editor_index(&self, x: f32) -> usize {
        self.editor_line
            .as_ref()
            .map_or(0, |line| line.index_for(x))
    }

    /// Sidebar.qml's places flags: the Trash count and the drive sizes show only when set true.
    pub fn adopt_places(&mut self, state: &flea::jsondoc::Json) {
        let places = state.get("places");
        let flag = |key| {
            places
                .and_then(|p| p.get(key))
                .and_then(flea::jsondoc::Json::as_bool)
                == Some(true)
        };
        self.trash_count_shown = flag("trashCount");
        self.drive_size_shown = flag("driveSize");
    }

    pub fn shape_misses(&self) -> u64 {
        self.cache.misses
    }
}

impl IntoElement for RailElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

fn box_of(rect: Rect) -> Bounds<Pixels> {
    Bounds::new(
        point(px(rect.x), px(rect.y)),
        size(px(rect.width), px(rect.height)),
    )
}

#[path = "text.rs"]
pub(crate) mod text;

/// The rail stands on the theme surface, as Sidebar.qml and every HIG board draw it; an inherited ground keeps its own.
pub fn rail_palette(style: &gpui::TextStyle, theme: crate::palette::Theme) -> Palette {
    let mut palette = Palette::inherited(style, theme);
    if style.background_color.is_none() {
        palette.background = theme.surface;
    }
    palette
}

/// SidebarRow.qml: a broken favourite's label takes the error ink, every other label the foreground.
pub fn label_ink(error: bool, palette: &Palette) -> gpui::Hsla {
    if error {
        palette.error
    } else {
        palette.foreground
    }
}

/// RailHeading.qml: PLACES, FAVORITES, NETWORK and DEVICES draw in the muted caption, quieter than the rows.
pub fn heading_ink(palette: &Palette) -> gpui::Hsla {
    palette.caption(false)
}

pub struct Ready {
    palette: Palette,
    headers: Vec<Rc<text::Prepared>>,
    labels: Vec<Rc<text::Prepared>>,
    details: Vec<Rc<text::Prepared>>,
    digits: Vec<Option<Rc<text::Prepared>>>,
}

// A row has a trailing caption slot when SidebarRow.showsDot is true for its group and kind.
fn indicator_slot(
    group: flea::ui_logic_pl4::rail::Group,
    kind: flea::ui_logic_pl4::rail::Kind,
) -> bool {
    use flea::ui_logic_pl4::rail::{Group, Kind};
    (group == Group::Network && kind != Kind::PluginRow)
        || (group == Group::Device && matches!(kind, Kind::Volume | Kind::Phone))
}

#[path = "add.rs"]
mod add;
pub use add::add_hit;
#[path = "element.rs"]
mod element;
#[cfg(test)]
#[path = "ground_tests.rs"]
mod ground_tests;
#[cfg(test)]
#[path = "ink_tests.rs"]
mod ink_tests;
#[path = "layout.rs"]
mod layout;
#[path = "paint.rs"]
mod paint;
#[path = "rename.rs"]
mod rename;

#[cfg(test)]
#[path = "i50_rename_tests.rs"]
mod i50_rename_tests;
