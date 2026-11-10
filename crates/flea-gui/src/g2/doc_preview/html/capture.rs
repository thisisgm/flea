//! The frame recipe recorded by its painter and the currently mounted HTML viewport's lane.
use super::HtmlView;
use crate::{
    g2::quick_look_overlay::scroller::capture::Lane,
    window::selection_preview::{docs::Doc, Frame, Mounted},
};
use gpui::{canvas, prelude::*, px, AnyElement, App, Entity, Hsla};

#[derive(Clone, Copy)]
pub(crate) struct FrameStyle {
    pub hairline: f32,
    pub ring: Hsla,
    pub canvas: Hsla,
}

pub(crate) fn frame(
    view: Option<Entity<HtmlView>>,
    hairline: f32,
    ring: Hsla,
    canvas_ink: Hsla,
) -> Option<AnyElement> {
    let view = view?;
    let style = FrameStyle {
        hairline,
        ring,
        canvas: canvas_ink,
    };
    Some(
        canvas(
            |_, _, _| (),
            move |bounds, _, _, cx| {
                let view = view.read(cx);
                view.frame_style.set(Some(style));
                // The absolute child fills the padding box inside the owner's painted border.
                view.frame_bounds.set(Some(bounds.dilate(px(hairline))));
            },
        )
        .absolute()
        .size_full()
        .into_any_element(),
    )
}

/// The column's passive HTML frame child; other mounted kinds create no capture.
pub(crate) fn column(column: &Frame, hairline: f32, ring: Hsla) -> Option<AnyElement> {
    let view = match &column.mounted {
        Mounted::Doc(Doc::Html(view)) => Some(view.clone()),
        _ => None,
    };
    frame(view, hairline, ring, Hsla::transparent_black())
}

impl HtmlView {
    /// The live image submission and retained document facts, without changing the preview.
    pub fn capture_state(&self, window: &gpui::Window, cx: &App) -> flea::jsondoc::Json {
        use flea::jsondoc::Json;
        let (submitted, drawn) = self.captured_image.get();
        let text = |v: String| Json::Str(v);
        let number = |v: f32| Json::Num(v.to_string());
        let rect = self.bounds.map_or_else(String::new, |b| {
            crate::ipc::rows::rect_of(
                b.origin.x.into(),
                b.origin.y.into(),
                b.size.width.into(),
                b.size.height.into(),
            )
        });
        crate::visual::object([
            (
                "visible",
                Json::Bool(self.doc.active && self.bounds.is_some()),
            ),
            ("ready", Json::Bool(self.first_screen(cx))),
            (
                "view",
                text(
                    if self.doc.source_view() {
                        "source"
                    } else {
                        "rendered"
                    }
                    .into(),
                ),
            ),
            (
                "state",
                text(
                    self.catalog
                        .as_ref()
                        .map_or_else(String::new, |catalog| self.status(catalog, cx)),
                ),
            ),
            ("file", text(self.doc.image_file.clone())),
            ("drawn", Json::Bool(drawn)),
            ("clip", Json::Bool(submitted)),
            ("accelerated", Json::Bool(drawn)),
            (
                "failed",
                Json::Bool(
                    self.doc.read_failed
                        || self.doc.picture == flea::ui_logic_doc_preview::html::Picture::Error,
                ),
            ),
            ("large", Json::Bool(self.doc.over_size())),
            ("pending", Json::Bool(self.doc.pending)),
            ("frame", text(rect)),
            (
                "window",
                crate::visual::object([
                    ("width", number(window.viewport_size().width.into())),
                    ("height", number(window.viewport_size().height.into())),
                ]),
            ),
        ])
    }

    pub(crate) fn viewport_metrics(&self, cx: &App) -> (f32, f32, f32) {
        if self.doc.source_view() {
            self.source
                .as_ref()
                .map_or((0., 0., 0.), |source| source.read(cx).viewport_metrics())
        } else {
            (self.scroll_y, self.frame.1, self.page_size().1 as f32)
        }
    }

    pub(crate) fn vertical_lane(&self, cx: &App) -> Option<Lane> {
        if self.doc.source_view() {
            self.source.as_ref()?.read(cx).vertical_lane()
        } else {
            self.bars.vertical_lane()
        }
    }
}
