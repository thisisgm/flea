//! The preview column's content inside the third-column host: the frame, the name and the facts under it.
use super::{
    bar::mark,
    glyph::Glyph,
    look::{self, Look},
    RasterState,
};
use flea::ui_logic_pv1::{facts::Fact, pixels::is_vector_name};
use gpui::{div, img, prelude::*, px, AnyElement, ImageSource, ObjectFit, SharedString};
use std::path::PathBuf;

/// What the frame holds.
pub enum Frame {
    /// cols-preview-image, cols-preview-raw: the thumbnail fit to the 173 px frame.
    Picture { path: PathBuf, w: f64, h: f64 },
    /// multi-facts: one muted-frame mark per kind, front-most first.
    Stack(Vec<Glyph>),
    /// preview-column: a document runs to the column bottom; its content is the document unit's slot.
    Document(Option<AnyElement>),
}

pub enum Facts {
    /// The facts table: 71 px muted labels, values in the foreground, 18 px rows 3 px apart.
    Table(Vec<Fact>),
    /// A tall kind's one line, the kind apart so it alone elides.
    Line { kind: String, tail: String },
}

pub struct Column {
    /// The host column's width, which the picture also fits.
    pub width: f32,
    /// The raster the host holds for this picture (#336), keyed by the device size it asked for.
    pub raster: RasterState,
    pub name: String,
    pub frame: Frame,
    pub facts: Facts,
}

fn caption(content: &str, ink: gpui::Hsla) -> gpui::Div {
    div()
        .text_size(px(look::CAPTION))
        .line_height(px(look::FACT_LINE))
        .text_color(ink)
        .whitespace_nowrap()
        .child(SharedString::from(content.to_owned()))
}

/// Inset 14 px in the host's bounds: the frame, the 20 px name and the facts, 9 px apart.
pub fn column(view: Column, look: &Look) -> AnyElement {
    let framed = |d: gpui::Div| {
        d.relative()
            .overflow_hidden()
            .border_1()
            .border_color(look.muted_frame)
            .rounded(look.corner)
            .debug_selector(|| "pv1-column-frame".into())
    };
    let frame = match view.frame {
        Frame::Document(slot) => framed(div().flex_1().min_h_0()).children(slot),
        Frame::Picture { path, w, h } => {
            let room_w = f64::from(view.width - 2. * look::COLUMN_INSET - 2.);
            // The host's raster draws when it has one; a vector without one draws nothing, never img(path).
            let vector = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_vector_name);
            let fit =
                super::body::fit_painted(room_w, f64::from(look::COLUMN_FRAME - 2.), w, h, vector);
            let (pw, ph) = fit.map_or((0., 0.), |f| (f.w as f32, f.h as f32));
            let source = view
                .raster
                .image
                .clone()
                .map(ImageSource::from)
                .or_else(|| (!vector).then(|| ImageSource::from(path.clone())));
            let mut picture = div()
                .w(px(pw))
                .h(px(ph))
                .flex_none()
                .debug_selector(|| "pv1-column-picture".into());
            if let Some(source) = source {
                picture = picture.child(img(source).size_full().object_fit(ObjectFit::Fill));
            }
            framed(
                div()
                    .h(px(look::COLUMN_FRAME))
                    .flex_none()
                    .flex()
                    .justify_center(),
            )
            .child(picture)
        }
        Frame::Stack(marks) => {
            let steps = marks.len().saturating_sub(1) as f32;
            let stack = div()
                .relative()
                .w(px(look::MARK + steps * look::STACK_STEP_X))
                .h(px(look::MARK + steps * look::STACK_STEP_Y))
                .debug_selector(|| "pv1-column-stack".into())
                .children(marks.into_iter().enumerate().rev().map(|(i, glyph)| {
                    div()
                        .absolute()
                        .left(px((steps - i as f32) * look::STACK_STEP_X))
                        .top(px(i as f32 * look::STACK_STEP_Y))
                        .opacity(1. - i as f32 * look::STACK_FADE)
                        .child(mark(glyph, look::MARK, look.muted_frame))
                }));
            framed(
                div()
                    .h(px(look::COLUMN_FRAME))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center(),
            )
            .child(stack)
        }
    };
    let name = div()
        .h(px(look::COLUMN_NAME))
        .flex_none()
        .text_size(px(look::BODY))
        .line_height(px(look::COLUMN_NAME))
        .text_color(look.foreground)
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis_middle()
        .child(SharedString::from(view.name));
    let facts = match view.facts {
        Facts::Table(rows) => div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(look::FACT_ROW_GAP))
            .debug_selector(|| "pv1-column-facts".into())
            .children(rows.into_iter().map(|fact| {
                div()
                    .h(px(look::FACT_ROW))
                    .pt(px(1.))
                    .flex()
                    .child(
                        caption(&fact.label, look.muted)
                            .w(px(look::FACT_LABEL_W))
                            .flex_none(),
                    )
                    .child(
                        caption(&fact.value, look.foreground)
                            .min_w_0()
                            .overflow_hidden(),
                    )
            })),
        Facts::Line { kind, tail } => div()
            .h(px(look::FACT_ROW))
            .flex_none()
            .flex()
            .debug_selector(|| "pv1-column-facts".into())
            .child(
                caption(&kind, look.foreground)
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis(),
            )
            .child(caption(&tail, look.foreground).flex_none()),
    };
    div()
        .absolute()
        .inset(px(look::COLUMN_INSET))
        .flex()
        .flex_col()
        .gap(px(look::GAP))
        .child(frame)
        .child(name)
        .child(facts)
        .into_any_element()
}
