//! Rail marks painted from the monochrome atlas and theme roles.
use super::*;

pub(super) fn paint_mark(
    row: &LaidRow,
    palette: &Palette,
    digit: Option<&text::Prepared>,
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
            if let Some(colour) = tag.fill {
                window.paint_quad(fill(box_, colour));
            } else if let Some(digit) = digit {
                window.paint_quad(gpui::quad(
                    box_,
                    px(0.),
                    palette.background,
                    px(2.),
                    palette.foreground,
                    gpui::BorderStyle::Solid,
                ));
                digit.paint(
                    f32::from(box_.origin.x) + (metrics.tag - digit.width) / 2.,
                    f32::from(box_.origin.y),
                    metrics.tag,
                    palette.foreground,
                    window,
                );
            }
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
