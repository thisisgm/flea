//! Rail row grounds and marks painted from the monochrome atlas and theme roles.
use super::*;

pub(super) fn paint_ground(
    row: &LaidRow,
    model: &RowsModel,
    palette: &Palette,
    window: &mut Window,
) {
    if is_cue(model, row.index) {
        paint_cue(row, palette, window);
        return;
    }
    if model.menu_open {
        return;
    }
    if model.hovered == Some(row.index) {
        window.paint_quad(fill(
            box_of(row.rect),
            palette.foreground.opacity(crate::list::HOVER),
        ));
    }
    let keyboard = model.focused && !model.pointer_focus;
    let highlighted = if keyboard { model.cursor } else { model.lit };
    // SidebarRow.qml: the row behind a focused eject mark keeps the grey cursor ground.
    let wash = keyboard && !model.mark_focused;
    if highlighted == Some(row.index) {
        let plate = if wash {
            palette.accent.opacity(crate::list::CURSOR_WASH)
        } else {
            palette.foreground.opacity(crate::list::GREY_CURSOR)
        };
        window.paint_quad(fill(box_of(row.rect), plate));
        // HIG rail-favorites and SidebarRow.qml: the keyboard cursor keeps its accent edge, eject focus included.
        if keyboard {
            window.paint_quad(fill(
                Bounds::new(
                    point(px(row.rect.x), px(row.rect.y)),
                    size(px(2.), px(row.rect.height)),
                ),
                palette.accent_frame,
            ));
        }
    }
}

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
            // GM's tags ruling: always the tag's own solid colour, never the text ink or a digit.
            let mut colour = tag.fill.unwrap_or(palette.tags[tag.index]);
            colour.a = 1.;
            window.paint_quad(fill(box_, colour));
        }
        RowMark::Square(square) => {
            use flea::ui_logic_rail_places::rail::Square;
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
                        size(
                            px(metrics.icon * if row.glyph == "dropbox" { 1.18 } else { 1. }),
                            px(metrics.icon),
                        ),
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

/// The row a held drag would land on (RailDrop.qml's cue).
pub(super) fn is_cue(model: &RowsModel, index: usize) -> bool {
    model
        .drop_target
        .as_ref()
        .is_some_and(|(target, _)| *target == index)
}

/// The trailing caption's ink: the drop cue's label in the accent, any other detail muted (SidebarRow.qml detailText).
pub(super) fn detail_ink(model: &RowsModel, index: usize, palette: &Palette) -> gpui::Hsla {
    if is_cue(model, index) {
        palette.accent
    } else {
        palette.caption(false)
    }
}

/// drag-cut-move: the drop target takes the cursor's accent wash and edge, one destination lit at a time.
pub(super) fn paint_cue(row: &LaidRow, palette: &Palette, window: &mut Window) {
    window.paint_quad(fill(
        box_of(row.rect),
        palette.accent.opacity(crate::list::CURSOR_WASH),
    ));
    window.paint_quad(fill(
        Bounds::new(
            point(px(row.rect.x), px(row.rect.y)),
            size(px(2.), px(row.rect.height)),
        ),
        palette.accent_frame,
    ));
}

/// Theme.accentEdge, the reorder bar's thickness in hairlines.
pub(super) const ACCENT_EDGE: f32 = 3.;

/// Buttons.js DISABLED_OPACITY over every ink a rail row draws.
pub(super) fn ghost(palette: Palette) -> Palette {
    let fade = |mut ink: gpui::Hsla| {
        ink.a *= crate::g2::quick_look_overlay::look::DISABLED;
        ink
    };
    Palette {
        foreground: fade(palette.foreground),
        error: fade(palette.error),
        metadata: fade(palette.metadata),
        muted: fade(palette.muted),
        muted_frame: fade(palette.muted_frame),
        executable: fade(palette.executable),
        accent: fade(palette.accent),
        accent_frame: fade(palette.accent_frame),
        ..palette
    }
}
