//! The custom rail Element prepares text and publishes its current geometry.
use super::*;

impl Element for RailElement {
    type RequestLayoutState = ();
    type PrepaintState = Ready;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Ready {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let metrics = RailMetrics::for_typeface(&self.typeface);
        let mut model = self.model.borrow_mut();
        let mut cache = std::mem::take(&mut model.cache);
        let mut laid = super::layout::layout(
            &model.sections,
            (f32::from(bounds.size.width), f32::from(bounds.size.height)),
            &self.typeface,
            &self.detail_face,
            &model,
            window,
            &mut cache,
        );
        if let Some(top) = laid.cursor_y.filter(|_| model.reveal_cursor) {
            if top < 0. {
                model.scroll = (model.scroll + top).max(0.);
            } else if top + metrics.row > laid.height {
                model.scroll += top + metrics.row - laid.height;
            }
        }
        model.reveal_cursor = false;
        model.scroll = model
            .scroll
            .min((laid.content_height - laid.height).max(0.));
        laid = super::layout::layout(
            &model.sections,
            (f32::from(bounds.size.width), f32::from(bounds.size.height)),
            &self.typeface,
            &self.detail_face,
            &model,
            window,
            &mut cache,
        );
        let (dx, dy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
        for header in &mut laid.headers {
            header.rect.x += dx;
            header.rect.y += dy;
        }
        for row in &mut laid.rows {
            row.rect.x += dx;
            row.rect.y += dy;
            row.label_rect.x += dx;
            row.label_rect.y += dy;
            row.detail_rect.x += dx;
            row.detail_rect.y += dy;
            row.indicator_rect.x += dx;
            row.indicator_rect.y += dy;
            if let Some(editor) = &mut row.editor {
                editor.x += dx;
                editor.y += dy;
            }
        }
        let headers = laid
            .headers
            .iter()
            .map(|h| {
                cache.get(
                    &h.text,
                    self.typeface.caption,
                    laid.width - 2. * metrics.pad,
                    HEADING_TRACKING,
                    &self.typeface,
                    window,
                )
            })
            .collect();
        let labels: Vec<Rc<text::Prepared>> = laid
            .rows
            .iter()
            .map(|row| {
                let text = if model.renaming == Some(row.index) {
                    &model.editor.text
                } else {
                    &row.label
                };
                cache.get(
                    text,
                    self.typeface.body,
                    if model.renaming == Some(row.index) {
                        f32::INFINITY
                    } else {
                        row.label_width
                    },
                    0.,
                    &self.typeface,
                    window,
                )
            })
            .collect();
        model.editor_line = model
            .renaming
            .and_then(|index| laid.rows.iter().position(|row| row.index == index))
            .map(|at| Rc::clone(&labels[at]));
        if let Some(rect) = model
            .renaming
            .and_then(|index| laid.row(index))
            .and_then(|row| row.editor)
        {
            let room = (rect.width - 8.).max(0.);
            let caret = model.editor_x(model.editor.cursor());
            if caret - model.editor_scroll > room {
                model.editor_scroll = caret - room;
            } else if caret < model.editor_scroll {
                model.editor_scroll = caret;
            }
        }
        let details = laid
            .rows
            .iter_mut()
            .map(|row| {
                row.detail_color = crate::ipc::rows::colour(palette.metadata);
                cache.get(
                    &row.detail,
                    self.detail_face.caption,
                    laid.width,
                    0.,
                    &self.detail_face,
                    window,
                )
            })
            .collect();
        let digit_face = Typeface {
            font: gpui::Font {
                weight: gpui::FontWeight::BOLD,
                ..self.typeface.font.clone()
            },
            ..self.typeface.clone()
        };
        let digits = laid
            .rows
            .iter()
            .map(|row| match row.mark {
                RowMark::Tag(tag) if tag.fill.is_none() => {
                    Some(cache.get(tag.digit, px(10.), TAG, 0., &digit_face, window))
                }
                _ => None,
            })
            .collect();
        cache.bound(((laid.height / metrics.row).ceil() as usize + 2) * 12);
        model.cache = cache;
        *self.laid.borrow_mut() = laid;
        Ready {
            palette,
            headers,
            labels,
            details,
            digits,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        ready: &mut Ready,
        window: &mut Window,
        cx: &mut App,
    ) {
        let palette = ready.palette;
        #[cfg(test)]
        {
            self.laid.borrow_mut().palette = Some(palette);
        }
        let metrics = RailMetrics::for_typeface(&self.typeface);
        let model = self.model.borrow();
        let laid = self.laid.borrow();
        if model.renaming.is_some() {
            if let Some(focus) = window.focused(cx) {
                window.handle_input(
                    &focus,
                    gpui::ElementInputHandler::new(bounds, self.producer.clone()),
                    cx,
                );
            }
        }
        window.paint_quad(fill(bounds, palette.background));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (header, text) in laid.headers.iter().zip(&ready.headers) {
                text.paint(
                    header.rect.x,
                    header.rect.y + header.top_padding,
                    header.rect.height - header.top_padding - metrics.gap,
                    palette.metadata,
                    window,
                );
            }
            for (at, row) in laid.rows.iter().enumerate() {
                if row.rect.y + metrics.row <= f32::from(bounds.top())
                    || row.rect.y >= f32::from(bounds.bottom())
                {
                    continue;
                }
                if model.hovered == Some(row.index) {
                    window.paint_quad(fill(
                        box_of(row.rect),
                        palette.foreground.opacity(crate::list::HOVER),
                    ));
                }
                let highlighted = if model.focused && !model.pointer_focus {
                    model.cursor
                } else {
                    model.lit
                };
                if highlighted == Some(row.index) {
                    let plate = if model.focused && !model.pointer_focus {
                        palette.accent.opacity(crate::list::CURSOR_WASH)
                    } else {
                        palette.foreground.opacity(crate::list::GREY_CURSOR)
                    };
                    window.paint_quad(fill(box_of(row.rect), plate));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(px(row.rect.x), px(row.rect.y)),
                            size(px(2.), px(row.rect.height)),
                        ),
                        palette.accent_frame,
                    ));
                }
                super::paint::paint_mark(
                    row,
                    &palette,
                    ready.digits[at].as_deref(),
                    &metrics,
                    window,
                    cx,
                );
                let label = &ready.labels[at];
                let rect = row.editor.unwrap_or(row.label_rect);
                if row.editor.is_some() {
                    window.paint_quad(gpui::quad(
                        box_of(rect),
                        px(model.corner / 2.),
                        palette.background,
                        px(1.),
                        palette.accent_frame,
                        gpui::BorderStyle::Solid,
                    ));
                }
                let origin = if row.editor.is_some() {
                    rect.x + 4. - model.editor_scroll
                } else {
                    rect.x
                };
                window.with_content_mask(
                    Some(ContentMask {
                        bounds: box_of(rect),
                    }),
                    |window| {
                        if row.editor.is_some() {
                            let from = origin + model.editor_x(model.editor.selected.start);
                            let to = origin + model.editor_x(model.editor.selected.end);
                            if !model.editor.selected.is_empty() {
                                window.paint_quad(fill(
                                    box_of(Rect::new(
                                        from,
                                        rect.y + 2.,
                                        to - from,
                                        rect.height - 4.,
                                    )),
                                    palette.accent.opacity(0.24),
                                ));
                            } else {
                                window.paint_quad(fill(
                                    box_of(Rect::new(from, rect.y + 3., 1., rect.height - 6.)),
                                    palette.foreground,
                                ));
                            }
                            if let Some(marked) = &model.editor.marked {
                                let from = origin + model.editor_x(marked.start);
                                let to = origin + model.editor_x(marked.end);
                                window.paint_quad(fill(
                                    box_of(Rect::new(
                                        from,
                                        rect.y + rect.height - 3.,
                                        to - from,
                                        1.,
                                    )),
                                    palette.foreground,
                                ));
                            }
                        }
                        label.paint(origin, rect.y, rect.height, palette.foreground, window);
                    },
                );
                ready.details[at].paint(
                    row.detail_rect.x,
                    row.detail_rect.y,
                    row.detail_rect.height,
                    palette.metadata,
                    window,
                );
                if row.indicator_visible {
                    let colour = if row.mounted == Some(false) {
                        palette.muted_frame.opacity(0.5)
                    } else {
                        palette.executable
                    };
                    window.paint_quad(fill(
                        Bounds::new(
                            point(
                                px(row.indicator_rect.x
                                    + (row.indicator_rect.width - metrics.dot) / 2.),
                                px(row.indicator_rect.y
                                    + (row.indicator_rect.height - metrics.dot) / 2.),
                            ),
                            size(px(metrics.dot), px(metrics.dot)),
                        ),
                        colour,
                    ));
                }
            }
        });
        // Divider.qml: the fixed hairline overlays the rail's inside right edge after its rows.
        let hairline = crate::list::Metrics::for_typeface(&self.typeface)
            .hairline
            .min(f32::from(bounds.size.width));
        if hairline > 0. {
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.right() - px(hairline), bounds.top()),
                    size(px(hairline), bounds.size.height),
                ),
                palette.foreground.opacity(crate::list::HAIRLINE),
            ));
        }
    }
}
