//! Rail marks painted from the monochrome atlas and theme roles.
use super::*;

pub(super) fn paint_mark(
    row: &LaidRow,
    palette: &Palette,
    _digit: Option<&text::Prepared>,
    metrics: &RailMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    let x = row.rect.x + metrics.pad;
    let y = row.rect.y + (metrics.row - metrics.icon) / 2.;
    match &row.mark {
        RowMark::Tag(tag) => {
            let box_ = Bounds::new(
                point(
                    px(x + (metrics.icon - metrics.tag) / 2.),
                    px(y + (metrics.icon - metrics.tag) / 2.),
                ),
                size(px(metrics.tag), px(metrics.tag)),
            );
            let mut colour = tag.fill.unwrap_or(palette.foreground);
            colour.a = 1.;
            window.paint_quad(fill(box_, colour));
        }
        RowMark::Square(square) => {
            use flea::ui_logic_pl4::rail::Square;
            let colour = match square {
                Square::On => palette.executable,
                Square::Off => palette.muted_frame,
                Square::None => palette.foreground,
            };
            window.paint_quad(fill(
                Bounds::new(
                    point(
                        px(x + (metrics.icon - metrics.dot) / 2.),
                        px(y + (metrics.icon - metrics.dot) / 2.),
                    ),
                    size(px(metrics.dot), px(metrics.dot)),
                ),
                colour,
            ));
        }
        RowMark::None => {
            if let Some((key, data)) = glyph::svg(row.glyph) {
                let _ = window.paint_svg(
                    Bounds::new(
                        point(px(x), px(y)),
                        size(px(metrics.icon), px(metrics.icon)),
                    ),
                    key,
                    Some(data),
                    TransformationMatrix::unit(),
                    palette.foreground,
                    cx,
                );
            }
        }
    }
}

/// SidebarRow.qml ejectMark: muted at rest, foreground under the keyboard cursor or the pointer.
pub(super) fn paint_eject(
    row: &LaidRow,
    model: &RowsModel,
    palette: &Palette,
    hairline: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let keyboard = model.focused && !model.pointer_focus && model.cursor == Some(row.index);
    // The keyboard frame stands on the mark's own hit box, in the foreground.
    if let Some(hit) = row.eject_hit().filter(|_| keyboard && model.mark_focused) {
        window.paint_quad(gpui::quad(
            box_of(hit),
            px(0.),
            gpui::transparent_black(),
            px(hairline),
            palette.foreground,
            gpui::BorderStyle::Solid,
        ));
    }
    let ink = if keyboard || model.mark_hovered == Some(row.index) {
        palette.foreground
    } else {
        palette.muted_frame
    };
    let side = row.indicator_rect.height;
    if let Some((key, data)) = super::glyph::svg("eject") {
        let _ = window.paint_svg(
            Bounds::new(
                point(
                    px(row.indicator_rect.x + (row.indicator_rect.width - side) / 2.),
                    px(row.indicator_rect.y),
                ),
                size(px(side), px(side)),
            ),
            key,
            Some(data),
            gpui::TransformationMatrix::unit(),
            ink,
            cx,
        );
    }
}
