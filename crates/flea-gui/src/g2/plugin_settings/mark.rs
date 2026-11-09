//! The boards' Settings metrics, a mark painted as an alpha mask, and CSS letter-spacing for capitals.
use super::glyphs::{self, DROPBOX_ASPECT, TICK};
use gpui::{
    div, prelude::*, px, App, Bounds, Div, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, Pixels, SharedString, Style, TransformationMatrix, Window,
};
use std::sync::Arc;

/// The boards' Settings row metrics, from boards/plugins-installed.html and its siblings.
pub mod metrics {
    pub const ROW: f32 = 37.;
    pub const PAD_X: f32 = 14.;
    pub const GAP: f32 = 9.;
    pub const BODY: f32 = 14.;
    pub const CAPTION: f32 = 12.;
    pub const SLOT: f32 = 19.;
    pub const MARK: f32 = 16.;
    pub const ROW_GLYPH: f32 = 18.;
    pub const CHEVRON: f32 = 19.;
    pub const BOX: f32 = 18.;
    pub const BOX_BORDER: f32 = 2.;
    pub const TICK: f32 = 12.;
    pub const HEADING_TOP: f32 = 19.;
    pub const FIRST_HEADING_TOP: f32 = 16.;
    pub const HEADING_BOTTOM: f32 = 7.8;
    pub const HEADING_GAP: f32 = 7.;
    pub const TRACKING: f32 = 1.68;
    pub const ACTION_GLYPH: f32 = 12.;
    pub const ACTION_GAP: f32 = 8.;
    pub const CHIP: f32 = 24.;
    pub const CHIP_PAD: f32 = 7.;
    pub const CHIP_GAP: f32 = 4.;
    pub const DESCRIPTION: [f32; 3] = [15., 14., 4.];
    pub const HINT: [f32; 4] = [4., 14., 2., 42.];
    pub const RULE_ABOVE: f32 = 7.;
    pub const RULE_BELOW: f32 = 8.;
    pub const DISABLED: f32 = 0.55;
    pub const CARD_WIDTH: f32 = 498.;
    pub const CARD_PAD: f32 = 16.;
    pub const CARD_GAP: f32 = 12.;
    pub const CARD_TITLE: f32 = 16.;
    pub const BUTTONS_ABOVE: f32 = 10.;
    pub const FIELD: f32 = 30.;
    pub const FIELD_PAD: f32 = 9.;
    pub const FIELD_LABEL_GAP: f32 = 5.;
    pub const FIELD_TRACKING: f32 = 1.2;
    pub const CARET: f32 = 16.;
}
use metrics as m;

/// A mark painted as an alpha mask in one ink: a static glyph or a plugin's own mark.svg.
pub struct Mark {
    key: SharedString,
    data: MarkData,
    size: f32,
    aspect: f32,
    ink: Hsla,
}

enum MarkData {
    Static(&'static [u8]),
    Owned(Arc<[u8]>),
}

impl Mark {
    pub fn named(name: &str, size: f32, ink: Hsla) -> Option<Self> {
        let (key, data) = glyphs::glyph(name)?;
        let aspect = if name == "dropbox" {
            DROPBOX_ASPECT
        } else {
            1.
        };
        Some(Self {
            key,
            data: MarkData::Static(data),
            size,
            aspect,
            ink,
        })
    }

    pub(super) fn tick(ink: Hsla) -> Self {
        Self {
            key: SharedString::new_static(TICK.0),
            data: MarkData::Static(TICK.1),
            size: m::TICK,
            aspect: 1.,
            ink,
        }
    }

    pub(super) fn owned(path: &str, data: Arc<[u8]>, size: f32, ink: Hsla) -> Self {
        Self {
            key: SharedString::from(format!("flea-st2-mark:{path}")),
            data: MarkData::Owned(data),
            size,
            aspect: 1.,
            ink,
        }
    }
}

impl IntoElement for Mark {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Mark {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: gpui::size(px(self.size).into(), px(self.size).into()),
            flex_shrink: 0.,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let height = bounds.size.height * self.aspect;
        let shape = Bounds::new(
            gpui::point(
                bounds.origin.x,
                bounds.origin.y + (bounds.size.height - height) / 2.,
            ),
            gpui::size(bounds.size.width, height),
        );
        let data = match &self.data {
            MarkData::Static(data) => *data,
            MarkData::Owned(data) => data,
        };
        let _ = window.paint_svg(
            shape,
            self.key.clone(),
            Some(data),
            TransformationMatrix::unit(),
            self.ink,
            cx,
        );
    }
}

// CSS letter-spacing: each character with the spacing after it, the last one included.
pub(super) fn tracked(text: &str, spacing: f32) -> Div {
    div()
        .flex()
        .flex_none()
        .pr(px(spacing))
        .gap(px(spacing))
        .children(text.chars().map(|c| SharedString::from(c.to_string())))
}

pub(super) fn value(text: &str, ink: Hsla) -> Div {
    div()
        .flex_none()
        .text_size(px(m::BODY))
        .text_color(ink)
        .child(SharedString::from(text.to_string()))
}

/// The heading's text and readback move together; its parent row and following rows retain their geometry.
pub(super) fn heading(
    text: &str,
    probe: &crate::g2::settings_pages::probe::Probe,
    id: &str,
) -> Div {
    const TEXT_TOP: f32 = 1.;
    probe.attach(
        tracked(&text.to_uppercase(), metrics::TRACKING)
            .relative()
            .top(px(TEXT_TOP))
            .font_weight(gpui::FontWeight::BOLD),
        format!("heading:{id}"),
    )
}
