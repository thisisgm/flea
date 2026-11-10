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
        self.prepaint_rows(bounds, window, cx)
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
        let grounds = rail_palette(
            &window.text_style(),
            cx.try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default(),
        );
        #[cfg(test)]
        {
            self.laid.borrow_mut().palette = Some(palette);
        }
        let metrics = RailMetrics::for_typeface(&self.typeface);
        let model = self.model.borrow();
        let hover_wash = crate::list::hover_wash::HoverWash::on(cx);
        let laid = self.laid.borrow();
        self.attach_rename(&model, &laid, bounds, window, cx);
        crate::window::glass::producers::paint(
            crate::window::glass::producers::Producer::Rail,
            bounds,
            palette.background,
            window,
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (header, text) in laid.headers.iter().zip(&ready.headers) {
                text.paint(
                    header.rect.x,
                    header.rect.y + header.top_padding,
                    header.text_height,
                    heading_ink(&palette),
                    window,
                );
                if let Some((add, (key, data))) = header.add.zip(super::glyph::svg("plus")) {
                    let _ = window.paint_svg(
                        Bounds::new(
                            point(px(add.x), px(add.y)),
                            size(px(add.width), px(add.height)),
                        ),
                        key,
                        Some(data),
                        TransformationMatrix::unit(),
                        palette.muted_frame,
                        cx,
                    );
                    self.listen_add(add, window);
                }
            }
            let hairline = crate::list::Metrics::for_typeface(&self.typeface).hairline;
            for (at, row) in laid.rows.iter().enumerate() {
                if row.rect.y + metrics.row <= f32::from(bounds.top())
                    || row.rect.y >= f32::from(bounds.bottom())
                {
                    continue;
                }
                // SidebarRow.qml held: the favourite under its reorder drag ghosts at the disabled opacity.
                let held = model
                    .reorder
                    .is_some_and(|held| held.start_y.is_some() && held.row == row.index);
                let palette = if held {
                    super::paint::ghost(palette)
                } else {
                    palette
                };
                let grounds = if held {
                    super::paint::ghost(grounds)
                } else {
                    grounds
                };
                super::paint::paint_ground(row, &model, &grounds, hairline, hover_wash, window);
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
                let paint_label = |window: &mut Window| {
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
                            label.paint(
                                origin,
                                rect.y,
                                rect.height,
                                super::label_ink(row.error, &palette),
                                window,
                            );
                        },
                    )
                };
                // The editor's name, wash and caret keep their order in a mirrored window.
                if row.editor.is_some() {
                    window.with_unmirrored(box_of(rect), paint_label);
                } else {
                    paint_label(window);
                }
                ready.details[at].paint(
                    row.detail_rect.x,
                    row.detail_rect.y,
                    row.detail_rect.height,
                    super::paint::detail_ink(&model, row.index, &palette),
                    window,
                );
                if row.eject {
                    let hairline = crate::list::Metrics::for_typeface(&self.typeface).hairline;
                    super::paint::paint_eject(row, &model, &palette, hairline, window, cx);
                } else if row.indicator_visible {
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
            // Sidebar.qml's reorder bar: Theme.accentEdge hairlines over the boundary, one in the row above.
            if let Some((at, below)) = model.reorder.and_then(|held| held.boundary()) {
                if let Some(row) = laid.row(at) {
                    let hairline = crate::list::Metrics::for_typeface(&self.typeface).hairline;
                    let y = row.rect.y + if below { row.rect.height } else { 0. } - hairline;
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.left(), px(y)),
                            size(bounds.size.width, px(super::paint::ACCENT_EDGE * hairline)),
                        ),
                        palette.accent,
                    ));
                }
            }
        });
        // Divider.qml: the fixed hairline overlays the rail's inside right edge after its rows.
        let hairline = crate::list::Metrics::for_typeface(&self.typeface)
            .hairline
            .min(f32::from(bounds.size.width));
        if hairline > 0. {
            // The hair's left edge snaps to a device pixel so a fractional scale never smears it over two.
            let scale = window.scale_factor();
            let right = f32::from(bounds.right());
            let left = ((right - hairline) * scale).round() / scale;
            window.paint_quad(fill(
                Bounds::new(
                    point(px(left), bounds.top()),
                    size(px(right - left), bounds.size.height),
                ),
                palette.foreground.opacity(crate::list::HAIRLINE),
            ));
        }
    }
}
