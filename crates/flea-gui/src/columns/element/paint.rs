//! Track painting shares the row drop caption and keeps its name clear of that caption.
use super::*;

impl TrackElement {
    pub(super) fn paint_track(
        &mut self,
        bounds: Bounds<Pixels>,
        palette: Palette,
        window: &mut Window,
        cx: &mut App,
    ) {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let paint = self.paint.borrow();
        let focus = &paint.drawn.as_ref().expect("prepainted track").focus;
        let view = &self.tracks[self.at];
        let held = self.held.as_deref();
        let active = view.slot == Slot::Active;
        let width = f32::from(bounds.size.width);
        let lane = crate::columns::geometry::lane(&paint.typeface);
        let left = bounds.origin.x;
        let grow = paint
            .drawn
            .as_ref()
            .map_or(Grow::default(), |drawn| drawn.grow);
        let top_of = |at: usize| {
            bounds.origin.y
                + px(grow.top(paint.first + at, self.row_height) - self.scroll_y.max(0.))
        };
        // The edited row's plate and edge take its band, so the refusal sits on the row's own ground.
        let row_box = |top: Pixels, w: f32, at: usize| {
            let height = grow.height(paint.first + at, self.row_height);
            Bounds::new(point(left, top), size(px(w), px(height)))
        };
        crate::window::glass::producers::paint(
            crate::window::glass::producers::Producer::Listing,
            bounds,
            palette.background,
            window,
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (at, (index, prepared, _)) in paint.visible.iter().enumerate() {
                if prepared.is_none() {
                    continue;
                }
                let top = top_of(at);
                let cursor = active && focus.cursor == Some(*index);
                let lifted = (!cursor && lifted_row(view, held, *index))
                    || focus.dropping(view.slot, *index);
                if let Some(plate) = focus.plate(&palette, view.slot, *index, lifted) {
                    window.paint_quad(fill(row_box(top, width, at), plate));
                }
                if focus.edge(view.slot, *index) {
                    window.paint_quad(fill(row_box(top, EDGE, at), palette.edge()));
                }
                if focus.menu_target == Some((view.slot, *index)) {
                    menu_target::paint(row_box(top, width, at), palette.edge(), window);
                }
                if let Some(prepared) = prepared {
                    let chevron = prepared.dir && (cursor || lifted);
                    if let Some((from, to)) = prepared.wash_span(chevron, cursor) {
                        let line = prepared.name(chevron).line();
                        let x = left + px(cells_in(width, lane, active, chevron).name_x);
                        let y = top + (px(self.band) - line.ascent - line.descent) / 2.;
                        window.paint_quad(fill(
                            Bounds::new(
                                point(x + from, y),
                                size(to - from, line.ascent + line.descent),
                            ),
                            palette.accent.opacity(crate::g2::sr1::run::RUN_WASH),
                        ));
                    }
                }
            }
            // One layer for every mark and glyph, so the scene orders the whole track once rather than once a line.
            let mut pictures_painted = 0;
            window.paint_layer(bounds, |window| {
                for (at, ((index, prepared, clip), picture)) in
                    paint.visible.iter().zip(&paint.pictures).enumerate()
                {
                    let Some(prepared) = prepared else { continue };
                    let top = top_of(at);
                    let caption = focus
                        .drop
                        .as_ref()
                        .filter(|(slot, row, _)| *slot == view.slot && *row == *index)
                        .and(paint.drop_line.as_ref())
                        .map(|line| &line.run);
                    let content = caption.map(|run| ContentMask {
                        bounds: Bounds::new(
                            point(left, top),
                            size(
                                (bounds.size.width - px(PAD_X + GAP) - run.line().width)
                                    .max(px(0.)),
                                px(self.row_height),
                            ),
                        ),
                    });
                    window.with_content_mask(
                        Some(content.unwrap_or(ContentMask { bounds })),
                        |window| {
                            let cursor = active && focus.cursor == Some(*index);
                            let lifted = (!cursor && lifted_row(view, held, *index))
                                || focus.dropping(view.slot, *index);
                            let chevron = prepared.dir && (cursor || lifted);
                            let mut inks = palette.row_ink(
                                *clip,
                                cursor || focus.dropping(view.slot, *index),
                                lifted,
                                view.dim,
                            );
                            let geometry = cells_in(width, lane, active, chevron);
                            // ColumnRow.qml: a thumbnail fills the whole mark slot, fitted whole, in place of the glyph.
                            if let Some(picture) = picture {
                                let (slot_x, slot_y, side) = picture_box(self.band);
                                let slot = Bounds::new(
                                    point(left + px(slot_x), top + px(slot_y)),
                                    size(px(side), px(side)),
                                );
                                paint_in_slot(picture, slot, window);
                                pictures_painted += 1;
                            } else {
                                let (mark_x, mark_y, mark) = mark_box(self.band);
                                let (key, data) = prepared.mark.svg();
                                let at = Bounds::new(
                                    point(left + px(mark_x), top + px(mark_y)),
                                    size(px(mark), px(mark)),
                                );
                                #[cfg(test)]
                                crate::bfx_cols_tests::record_mark(&key, at);
                                let _ = window.paint_svg(
                                    at,
                                    key,
                                    Some(data),
                                    TransformationMatrix::unit(),
                                    inks.mark,
                                    cx,
                                );
                            }
                            let name = prepared.name_for(chevron, cursor);
                            let name_x = left + px(geometry.name_x);
                            inks.name =
                                crate::window::glass::producers::text_ink(inks.name, window, cx);
                            paint_line(name, name_x, top, self.band, inks.name, window);
                            let mut name_width = f32::from(name.line().width);
                            if let Some(location) = prepared.location.as_ref() {
                                let gap = GAP;
                                let bright = cursor
                                    || lifted
                                    || self.focus.selected.contains(index)
                                    || self.focus.hover == Some((view.slot, *index));
                                paint_line(
                                    location,
                                    name_x + name.line().width + px(gap),
                                    top,
                                    self.band,
                                    if bright {
                                        inks.name
                                    } else {
                                        crate::window::glass::producers::text_ink(
                                            palette.metadata,
                                            window,
                                            cx,
                                        )
                                    },
                                    window,
                                );
                                name_width += gap + f32::from(location.line().width);
                            }
                            if let Some(Row::Listing(facts)) = row_at(&view.rows, held, *index) {
                                tags::paint(
                                    facts,
                                    point(left, top + px(self.band / 2.)),
                                    name_width,
                                    geometry,
                                    *clip,
                                    &theme,
                                    window,
                                );
                            }
                            if let (Some(size_run), Some(right)) =
                                (&prepared.size, geometry.size_right)
                            {
                                paint_line(
                                    size_run,
                                    left + px(right) - size_run.line().width,
                                    top,
                                    self.band,
                                    inks.name,
                                    window,
                                );
                            }
                            if let Some(x) = geometry.chevron_x {
                                let at = Bounds::new(
                                    point(left + px(x), top + px((self.band - CHEVRON) / 2.)),
                                    size(px(CHEVRON), px(CHEVRON)),
                                );
                                let chevron = if window.layout_mirrored() {
                                    CHEVRON_LEFT_SVG
                                } else {
                                    CHEVRON_SVG
                                };
                                paint_svg(chevron, at, inks.chevron, window, cx);
                            }
                            if *clip != Clip::None {
                                let x = geometry.clip_x();
                                let at = Bounds::new(
                                    point(left + px(x), top + px((self.band - CLIP) / 2. - 1.)),
                                    size(px(CLIP), px(CLIP)),
                                );
                                let glyph = if *clip == Clip::Cut {
                                    SCISSORS_SVG
                                } else {
                                    COPY_SVG
                                };
                                paint_svg(glyph, at, inks.clip, window, cx);
                            }
                        },
                    );
                    if let Some(caption) = caption {
                        paint_line(
                            caption,
                            bounds.right() - px(PAD_X) - caption.line().width,
                            top,
                            self.band,
                            palette.accent,
                            window,
                        );
                    }
                }
            });
            paint.painted_pictures.set(pictures_painted);
            if let Some(x) = view.divider_x() {
                let at = point(bounds.origin.x + px(x - view.x), bounds.origin.y);
                window.paint_quad(fill(
                    Bounds::new(at, size(px(DIVIDER), bounds.size.height)),
                    palette.hairline(),
                ));
            }
        });
    }
}
