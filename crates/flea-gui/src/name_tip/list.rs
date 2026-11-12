//! Read only the one visible List name under the pointer, using the painter's retained facts.
use super::{cut, Candidate};
use crate::list::{ListState, CURSOR_WASH, GREY_CURSOR, HOVER};
use gpui::{point, px, size, Bounds, Pixels, Point};

pub(crate) fn list_candidate(
    state: &ListState,
    at: Point<Pixels>,
    listing: usize,
    right: Pixels,
    identity: usize,
    other_pane: bool,
) -> Option<Candidate> {
    let drawn = &state.drawn;
    if drawn.listing != listing || !drawn.mask.contains(&at) {
        return None;
    }
    let metrics = drawn.metrics;
    let right = right.min(drawn.bounds.right() - px(metrics.lane));
    let position = drawn.grow.position_at(
        f32::from(at.y - drawn.bounds.top()) + drawn.scroll_y,
        metrics.row,
    );
    let shown = state.visible().iter().find(|row| row.view == position)?;
    if shown.state.renaming || shown.state.drop_target {
        return None;
    }
    let facts = state.drawn_rows.row(shown.index)?;
    let prepared = shown.prepared.as_ref()?;
    let mut display = crate::list::outline::display(facts);
    if state.view.searching || state.view.recent || state.view.place_recent.is_some() {
        display.to_mut().name = facts.name.rsplit('/').next()?.to_owned();
    }
    let name = crate::row::decorated_name(&display);
    let top = drawn.bounds.top() + px(drawn.grow.top(position, metrics.row) - drawn.scroll_y);
    let x = drawn.bounds.left()
        + px(drawn.layout.name.x + crate::list::outline::indent(facts, &metrics));
    let mut room = drawn.bounds.left() + px(drawn.layout.name.right()) - x;
    if shown.clip.is_some() {
        room -= px(metrics.gap + metrics.clip);
    }
    let hit = Bounds::new(point(x, top), size(room.max(px(0.)), px(metrics.row)));
    if !hit.contains(&at)
        || !cut(&name, &prepared.label, prepared.name.line().width, room)
        || top < drawn.mask.top()
        || top + px(metrics.row) > drawn.mask.bottom()
    {
        return None;
    }
    let palette = drawn.palette?;
    let marks = shown.state;
    let wash = if marks.cursor {
        Some(if state.drawn_focused {
            palette.accent.opacity(CURSOR_WASH)
        } else {
            palette.foreground.opacity(GREY_CURSOR)
        })
    } else if marks.selected {
        Some(if other_pane && !state.drawn_focused {
            palette.foreground.opacity(GREY_CURSOR)
        } else {
            palette.marked
        })
    } else if marks.hover_lit {
        Some(palette.foreground.opacity(HOVER))
    } else {
        None
    };
    Some(Candidate {
        identity: (identity, listing, 0, shown.index),
        epoch: listing,
        name,
        font: state.typeface.font.clone(),
        size: state.typeface.body,
        row: Bounds::new(
            point(x, top),
            size((right - x).max(px(0.)), px(metrics.row)),
        ),
        hit,
        clip: drawn.mask,
        text_x: x,
        height: super::line_height(&state.typeface, metrics.row),
        pad: px(metrics.pad / 2.),
        ground: wash.map_or(palette.background, |wash| palette.background.blend(wash)),
        ink: palette.name(prepared.ink, marks.lifted()),
        framed: !marks.cursor && !marks.selected && !marks.hover_lit,
        hair: palette.foreground.opacity(crate::list::HAIRLINE),
    })
}
