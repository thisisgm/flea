//! Columns can expand across track dividers; only the hovered track's held name is read.
use super::{cut, Candidate};
use crate::columns::{
    element::{lifted_row, row_at},
    geometry::{cells_in, Slot},
    paint::TrackPaint,
};
use gpui::{point, px, size, Bounds, Pixels, Point};

pub(super) fn candidate(
    paint: &TrackPaint,
    at: Point<Pixels>,
    listing: usize,
    right: Pixels,
    identity: usize,
) -> Option<Candidate> {
    let drawn = paint.drawn.as_ref()?;
    let view = drawn.view();
    let focus = &drawn.focus;
    if !view.enabled || !drawn.bounds.contains(&at) || drawn.held.as_ref()?.listing != listing {
        return None;
    }
    let position = drawn.grow.position_at(
        f32::from(at.y - drawn.bounds.top()) + drawn.scroll,
        drawn.row_height,
    );
    let index = drawn.row_at(position)?;
    if focus.renaming.is_some() || focus.drop.is_some() {
        return None;
    }
    let prepared = paint.prepared(index)?;
    let row = row_at(&view.rows, drawn.held.as_deref(), index)?;
    let active = view.slot == Slot::Active;
    let cursor = active && focus.cursor == Some(index);
    let lifted = !cursor && lifted_row(view, drawn.held.as_deref(), index);
    let chevron = prepared.dir && (cursor || lifted);
    let cells = cells_in(
        f32::from(drawn.bounds.size.width),
        drawn.lane,
        active,
        chevron,
    );
    let top = drawn.bounds.top() + px(drawn.grow.top(position, drawn.row_height) - drawn.scroll);
    let x = drawn.bounds.left() + px(cells.name_x);
    let hit = Bounds::new(
        point(x, top),
        size(px(cells.name_right - cells.name_x), px(drawn.band)),
    );
    let full = crate::row::shown_name(row.name());
    let name = if !prepared.location_label.is_empty() {
        flea::ui_logic_file_search::base(&full).to_owned()
    } else {
        full.into_owned()
    };
    let run = prepared.name(chevron);
    // A chevron-only truncation has a different retained run even if the plain label still fits.
    let elided = prepared.label != name || (chevron && prepared.chevron.is_some());
    if !hit.contains(&at)
        || !cut(
            &name,
            if elided { "" } else { &name },
            run.line().width,
            hit.size.width,
        )
        || top < drawn.bounds.top()
        || top + px(drawn.band) > drawn.bounds.bottom()
    {
        return None;
    }
    let palette = paint.painted?;
    let wash = focus.plate(&palette, view.slot, index, lifted);
    let marked = active && focus.selected.binary_search(&index).is_ok();
    Some(Candidate {
        identity: (identity, listing, view.slot.index(), index),
        epoch: listing,
        name,
        font: paint.typeface.font.clone(),
        size: paint.typeface.body,
        row: Bounds::new(point(x, top), size((right - x).max(px(0.)), px(drawn.band))),
        hit,
        clip: drawn.bounds,
        text_x: x,
        height: super::line_height(&paint.typeface, drawn.band),
        pad: px(crate::list::Metrics::for_typeface(&paint.typeface).pad / 2.),
        ground: wash.map_or(palette.background, |wash| palette.background.blend(wash)),
        ink: palette.foreground,
        framed: !cursor
            && !marked
            && !lifted
            && !(focus.hover_wash && focus.hover == Some((view.slot, index))),
        hair: palette.hairline(),
    })
}
