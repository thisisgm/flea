//! The chooser reuses list rows with a leading check slot and compact foreground metadata.
use super::{
    element::{paint_mark, Shown},
    ListView, Metrics, Palette,
};
use crate::grid::element::pictures::paint_in_slot;
use crate::row::marks::{ClipMark, Mark};
use gpui::{outline, point, px, size, App, BorderStyle, Bounds, Pixels, Window};

pub fn check_size(small: f32, hairline: f32) -> f32 {
    small + hairline + 4. * hairline
}

pub fn leading(view: &ListView, metrics: &Metrics) -> f32 {
    if view.picker == Some(true) {
        check_size(metrics.mark / super::MARK_RATIO as f32, metrics.hairline) + metrics.gap
    } else {
        0.
    }
}

pub fn checkbox(
    bounds: Bounds<Pixels>,
    checked: bool,
    palette: &Palette,
    window: &mut Window,
    cx: &mut App,
) {
    let ink = if checked {
        palette.accent_frame
    } else {
        palette.metadata_frame
    };
    let mut frame = outline(bounds, ink, BorderStyle::Solid);
    frame.border_widths = px(2.).into();
    frame.corner_radii = px(crate::compositor::corner(cx) / 2.).into();
    window.paint_quad(frame);
    if checked {
        let inset = px(4.);
        let inner = Bounds::new(
            bounds.origin + point(inset, inset),
            size(
                (bounds.size.width - inset * 2.).max(px(0.)),
                (bounds.size.height - inset * 2.).max(px(0.)),
            ),
        );
        if let Some((key, data)) = crate::g2::st1::glyphs::svg("check") {
            let _ = window.paint_svg(
                inner,
                key,
                Some(data),
                gpui::TransformationMatrix::unit(),
                palette.accent,
                cx,
            );
        }
    }
}

pub(super) fn paint_icon(
    shown: &Shown,
    view: &ListView,
    metrics: Metrics,
    palette: Palette,
    origin: (Pixels, Pixels, Option<&flea::protocol::RowFacts>),
    window: &mut Window,
    cx: &mut App,
) -> (usize, usize) {
    let (left, top, row) = origin;
    let offset = leading(view, &metrics);
    if offset > 0.
        && row.is_some_and(|row| {
            flea::ui_logic_pk2::directory(row.dir, row.mode, &row.icon) == view.picker_directory
        })
    {
        let side = offset - metrics.gap;
        checkbox(
            Bounds::new(
                point(left + px(metrics.pad), top + px((metrics.row - side) / 2.)),
                size(px(side), px(side)),
            ),
            shown.state.selected,
            &palette,
            window,
            cx,
        );
    }
    let dim = if shown.clip == Some(ClipMark::Cut) {
        super::CUT_DIM
    } else {
        1.
    };
    if let Some(picture) = &shown.picture {
        let side = if view.dual {
            metrics.mark
        } else {
            metrics.slot
        };
        let slot = Bounds::new(
            point(
                left + px(metrics.pad + offset + (metrics.slot - side) / 2.),
                top + px((metrics.row - side) / 2.),
            ),
            size(px(side), px(side)),
        );
        paint_in_slot(picture, slot, window);
        return (1, 1);
    }
    let at = point(
        left + px(metrics.pad + offset + (metrics.slot - metrics.mark) / 2.),
        top + px((metrics.row - metrics.mark) / 2.),
    );
    let mark = shown
        .prepared
        .as_ref()
        .map_or(Mark::File, |prepared| prepared.mark);
    let (key, data) = mark.svg();
    let ink = if shown.state.lifted() {
        palette.foreground
    } else {
        palette.metadata_frame
    };
    paint_mark(key, data, at, metrics.mark, ink.opacity(dim), window, cx);
    (1, 0)
}
