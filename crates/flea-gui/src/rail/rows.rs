//! The rail's rows as one custom Element: headings, marks, labels, trailing slots and the rename field, with the painted boxes published for IPC.
use crate::g2::pl4::content::{DrawnSection, RowMark};
use crate::rail::geometry::Rect;
#[path = "glyph.rs"]
mod glyph;
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

/// Theme.qml rail formulas scale the approved body-14 geometry with the accepted Typeface.
struct RailMetrics {
    row: f32,
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
        let row = (list.nominal_row * RAIL_ROW_SCALE as f32).round();
        let line = (f32::from(face.body_small) * TEXT_LINE_HEIGHT_FACTOR as f32).round();
        let board = |value: f32| (value * f32::from(face.base) / BODY).round();
        Self {
            row,
            pad: list.pad,
            gap: list.gap,
            panel_gap: (PANEL_GAP * f32::from(face.base) / OEM_BASE).round(),
            icon: (row - (list.nominal_row - line)).max(1.),
            tag: board(TAG),
            dot: board(DOT),
            slot: f32::from(face.caption),
        }
    }
}

/// One heading as painted: the caption's capitals, its box, its lead and its type size.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidHeader {
    pub text: String,
    pub rect: Rect,
    pub top_padding: f32,
    pub font_size: f32,
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

/// Everything one paint put on the rail: the loaded boxes, never an expected rectangle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
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
    pub renaming: Option<usize>,
    pub editor: crate::path::entry::Entry,
    pub editor_scroll: f32,
    editor_line: Option<Rc<text::Prepared>>,
    pub reveal_cursor: bool,
    pub pointer_focus: bool,
    pub trash_count: u64,
    pub trash_count_shown: bool,
    pub scroll: f32,
    cache: text::Cache,
    pub hovered: Option<usize>,
    pub corner: f32,
    /// Presses on the NETWORK heading's plus; the window opens Add a Network Share once per new press.
    pub add_presses: u64,
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
mod text;

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
#[path = "element.rs"]
mod element;
#[path = "layout.rs"]
mod layout;
#[path = "paint.rs"]
mod paint;
