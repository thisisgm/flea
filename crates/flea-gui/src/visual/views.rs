//! Capture readers use the mounted painter's last frame, including Columns and Grid.
use super::{geometry::Probe, view_name};
use crate::{
    columns::{
        element::lifted_row,
        geometry::{self, Slot},
    },
    state::Shell,
    window::startup::View,
};
use gpui::{point, px, size, Bounds, Pixels, Window};

impl Shell {
    pub(crate) fn visual_drawn_cursor(&self) -> Option<usize> {
        let host = self.panes[self.focus_side].as_ref()?;
        match host.drawn_view? {
            View::List | View::Dual => host.list.borrow().drawn_cursor,
            View::Columns => {
                let painter = host.columns.as_ref()?.painters.get(Slot::Active)?;
                let paint = painter.borrow();
                let drawn = paint.drawn.as_ref()?;
                if !drawn.view().enabled
                    || !std::rc::Rc::ptr_eq(drawn.held.as_ref()?, &host.pane.held)
                {
                    return None;
                }
                let cursor = drawn.focus.cursor?;
                paint.prepared(cursor).map(|_| cursor)
            }
            View::Grid => {
                let state = host.grid.as_ref()?.state.borrow();
                if !std::rc::Rc::ptr_eq(state.frame_held.as_ref()?, &host.pane.held) {
                    return None;
                }
                state
                    .drawn()
                    .iter()
                    .find(|row| row.cursor && row.name().is_some())
                    .map(|row| row.listing)
            }
        }
    }

    pub(crate) fn visual_view_geometry(&self, probe: &Probe, window: &Window) {
        for (side, host) in self
            .panes
            .iter()
            .enumerate()
            .filter_map(|(side, host)| host.as_ref().map(|host| (side, host)))
        {
            if side != 0 && self.view != View::Dual {
                continue;
            }
            match host.drawn_view {
                Some(View::Columns) => {
                    let Some(columns) = &host.columns else {
                        continue;
                    };
                    // Every track one frame builds shares its focus snapshot, so a painter holding an older one drew nothing this frame.
                    let Some(frame) = columns.painters.get(Slot::Active).and_then(|active| {
                        let paint = active.borrow();
                        paint
                            .drawn
                            .as_ref()
                            .map(|drawn| std::rc::Rc::clone(&drawn.focus))
                    }) else {
                        continue;
                    };
                    for slot in [
                        Slot::GreatGrandparent,
                        Slot::Grandparent,
                        Slot::Parent,
                        Slot::Active,
                        Slot::Child,
                    ] {
                        let Some(painter) = columns.painters.get(slot) else {
                            continue;
                        };
                        let paint = painter.borrow();
                        let Some(drawn) = paint
                            .drawn
                            .as_ref()
                            .filter(|drawn| std::rc::Rc::ptr_eq(&drawn.focus, &frame))
                        else {
                            continue;
                        };
                        let surface = format!("columns-{side}-{}", slot.index());
                        probe.push(&surface, "", "surface", drawn.bounds, "", window);
                        for index in paint.visible() {
                            let Some(prepared) = paint.prepared(index) else {
                                continue;
                            };
                            let Some(position) = drawn.position_of(index) else {
                                continue;
                            };
                            let top = drawn.bounds.origin.y
                                + px(drawn.grow.top(position, drawn.row_height)
                                    - drawn.scroll.max(0.));
                            let id = format!("{surface}-row-{index}");
                            let active = slot == Slot::Active;
                            let cursor = active && drawn.focus.cursor == Some(index);
                            let lifted =
                                !cursor && lifted_row(drawn.view(), drawn.held.as_deref(), index);
                            let chevron = prepared.dir && (cursor || lifted);
                            let cells = geometry::cells_in(
                                f32::from(drawn.bounds.size.width),
                                drawn.lane,
                                active,
                                chevron,
                            );
                            let rect = Bounds::new(
                                point(drawn.bounds.origin.x, top),
                                size(
                                    drawn.bounds.size.width,
                                    px(drawn.grow.height(position, drawn.row_height)),
                                ),
                            );
                            probe.push(&id, &surface, "row", rect, "", window);
                            let glyph = format!("{id}-glyph");
                            probe.push(
                                &glyph,
                                &id,
                                "glyph",
                                Bounds::new(
                                    point(drawn.bounds.origin.x + px(geometry::PAD_X), top),
                                    size(px(geometry::MARK_SLOT), px(drawn.row_height)),
                                ),
                                "",
                                window,
                            );
                            let (x, y, mark) = if paint.picture(index).is_some() {
                                geometry::picture_box(drawn.band)
                            } else {
                                geometry::mark_box(drawn.band)
                            };
                            probe.push(
                                &format!("{id}-ink"),
                                &glyph,
                                "ink",
                                Bounds::new(
                                    point(drawn.bounds.origin.x + px(x), top + px(y)),
                                    size(px(mark), px(mark)),
                                ),
                                "",
                                window,
                            );
                            let name = prepared.name_for(chevron, cursor);
                            let rect = crate::list::line_bounds(
                                name,
                                drawn.bounds.origin.x + px(cells.name_x),
                                top,
                                drawn.band,
                                window,
                            );
                            probe.push(
                                &format!("{id}-label"),
                                &id,
                                "label",
                                rect,
                                &prepared.label,
                                window,
                            );
                            if let (Some(run), Some(right)) = (&prepared.size, cells.size_right) {
                                let rect = crate::list::line_bounds(
                                    run,
                                    drawn.bounds.origin.x + px(right) - run.line().width,
                                    top,
                                    drawn.band,
                                    window,
                                );
                                probe.push(
                                    &format!("{id}-value"),
                                    &id,
                                    "value",
                                    rect,
                                    &prepared.size_label,
                                    window,
                                );
                            }
                        }
                    }
                }
                Some(View::Grid) => {
                    let Some(grid) = &host.grid else { continue };
                    let state = grid.state.borrow();
                    let Some(layout) = state.layout else { continue };
                    let surface = format!("{}-{side}", view_name(View::Grid));
                    probe.push(&surface, "", "surface", state.bounds, "", window);
                    let bounds = |rect: crate::grid::geometry::Rect| -> Bounds<Pixels> {
                        Bounds::new(
                            point(
                                state.bounds.origin.x + px(rect.x),
                                state.bounds.origin.y + px(rect.y),
                            ),
                            size(px(rect.width), px(rect.height)),
                        )
                    };
                    for drawn in state.drawn().iter().filter(|row| row.name().is_some()) {
                        let id = format!("{surface}-row-{}", drawn.listing);
                        probe.push(&id, &surface, "row", bounds(drawn.cell), "", window);
                        let glyph = format!("{id}-glyph");
                        probe.push(&glyph, &id, "glyph", bounds(drawn.slot), "", window);
                        probe.push(
                            &format!("{id}-ink"),
                            &glyph,
                            "ink",
                            bounds(drawn.picture.unwrap_or_else(|| layout.mark(drawn.slot))),
                            "",
                            window,
                        );
                        probe.push(
                            &format!("{id}-label"),
                            &id,
                            "label",
                            bounds(drawn.painted),
                            drawn.caption_text().unwrap_or_default(),
                            window,
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "views_tests.rs"]
mod tests;
