//! The custom strip painter and its accessible tab-list root.
use super::*;
use crate::tabs::accessibility::Painted;

impl Element for StripElement {
    type RequestLayoutState = ();
    type PrepaintState = (Palette, Hitbox, Painted);

    // A stable identity keeps the tab list's accessible node across frames.
    fn id(&self) -> Option<ElementId> {
        Some("tab-strip".into())
    }
    fn a11y_role(&self) -> Option<gpui::Role> {
        Some(gpui::Role::TabList)
    }
    fn a11y_synthetic_children(
        &mut self,
        (_, _, painted): &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        if let Some(controls) = &self.controls {
            controls.children(&self.frame, painted, builder);
        }
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
    ) -> Self::PrepaintState {
        let theme = crate::window::glass::producers::chrome_theme(window, cx);
        self.labels.borrow_mut().prepare(&self.frame, window);
        // Every tab and New tab take the pointing hand, as the shipped strip's HoverHandlers did.
        let reach = px(self.frame.tab_width) * self.frame.labels.len() as f32 + px(HIT_MIN);
        let pointer = Bounds::new(
            point(bounds.origin.x + px(PAD_X), bounds.origin.y),
            size(reach, px(TAB_HEIGHT)),
        );
        (
            Palette::inherited(&window.text_style(), theme),
            window.insert_hitbox(pointer, HitboxBehavior::Normal),
            Painted::new(bounds, window.scale_factor()),
        )
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        (palette, pointer, painted): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(controls) = &self.controls {
            controls.actions(painted, window);
        }
        if let (Some(frames), Some(draw)) = (&self.feedback, window.current_draw_id()) {
            let frames = frames.borrow();
            frames.staged.set(Some(crate::tabs::held::Painted {
                draw,
                version: frames.version,
                holding: frames.holding,
                complete: !self.frame.labels.is_empty(),
            }));
        }
        window.set_cursor_style(CursorStyle::PointingHand, pointer);
        let palette = *palette;
        let labels = self.labels.borrow();
        let frame = &self.frame;
        let hair = palette.foreground.opacity(HAIR);
        let (left, top) = (bounds.origin.x, bounds.origin.y);
        let width = px(frame.tab_width);
        let count = frame.labels.len();
        // The strip is theme.surface; the current plate always paints the background.
        let current_ground =
            (frame.current < count && frame.ghost != Some(frame.current)).then(|| {
                Bounds::new(
                    point(
                        left + px(PAD_X) + width * frame.slots[frame.current] as f32,
                        top,
                    ),
                    size(width, px(TAB_HEIGHT)),
                )
            });
        crate::window::glass::tab_ground(bounds, current_ground, window, cx);
        let rule = Bounds::new(
            point(left, top + bounds.size.height - px(1.)),
            size(bounds.size.width, px(1.)),
        );
        window.paint_quad(fill(rule, hair));
        window.paint_layer(bounds, |window| {
            for (index, label) in labels.drawn.iter().enumerate().take(count) {
                let slot = frame.slots[index];
                let x = left + px(PAD_X) + width * slot as f32;
                let current = index == frame.current;
                let fade = if frame.ghost == Some(index) {
                    GHOST
                } else {
                    1.
                };
                let tab = Bounds::new(point(x, top), size(width, px(TAB_HEIGHT)));
                if current {
                    if crate::window::glass::producers::requested(window, cx) >= 1.
                        || frame.ghost != Some(index)
                    {
                        window.paint_quad(fill(
                            tab,
                            crate::window::glass::ground(palette.background, window, cx)
                                .opacity(fade),
                        ));
                    }
                    let edge = Bounds::new(
                        point(x, top + px(HEIGHT - ACCENT_EDGE)),
                        size(width, px(ACCENT_EDGE)),
                    );
                    window.paint_quad(fill(edge, palette.accent_frame.opacity(fade)));
                } else if slot + 1 < count {
                    let separator = Bounds::new(
                        point(x + width - px(1.), top + px(SEPARATOR_TOP)),
                        size(px(1.), px(SEPARATOR_HEIGHT)),
                    );
                    window.paint_quad(fill(separator, hair.opacity(fade)));
                }
                if frame.focus == Some(index) {
                    let inside = Bounds::new(
                        point(x + px(FRAME_INSET), top + px(FRAME_INSET)),
                        size(
                            width - px(2. * FRAME_INSET),
                            px(HEIGHT - 2. * FRAME_INSET - ACCENT_EDGE),
                        ),
                    );
                    frame_box(window, inside, palette.foreground);
                }
                let run = &labels.entries[*label].2;
                let line = run.line();
                let baseline = window.pixel_snap(
                    top + (px(TAB_HEIGHT) - line.ascent - line.descent) / 2. + line.ascent,
                );
                let ink = palette.caption(current);
                let _ = window.paint_glyph_run(
                    point(x + px(LABEL_X), baseline),
                    run,
                    ink.opacity(fade),
                );
                let mark_ink = palette.mark(current || frame.close_hover == Some(index));
                let mark = Bounds::new(
                    point(
                        x + width - px(HIT_MIN) + px((HIT_MIN - MARK) / 2.),
                        top + px((TAB_HEIGHT - MARK) / 2.),
                    ),
                    size(px(MARK), px(MARK)),
                );
                let _ = window.paint_svg(
                    mark,
                    "flea-mark:x".into(),
                    Some(X_MARK),
                    TransformationMatrix::unit(),
                    mark_ink.opacity(fade),
                    cx,
                );
            }
            let plus_x = left + px(PAD_X) + width * count as f32;
            let focused = frame.focus == Some(count);
            if focused {
                let inside = Bounds::new(
                    point(plus_x + px(FRAME_INSET), top + px(FRAME_INSET)),
                    size(
                        px(HIT_MIN - 2. * FRAME_INSET),
                        px(HEIGHT - 2. * FRAME_INSET - ACCENT_EDGE),
                    ),
                );
                frame_box(window, inside, palette.foreground);
            }
            let plus = Bounds::new(
                point(
                    plus_x + px((HIT_MIN - MARK) / 2.),
                    top + px((TAB_HEIGHT - MARK) / 2.),
                ),
                size(px(MARK), px(MARK)),
            );
            let ink = palette.mark(focused);
            let _ = window.paint_svg(
                plus,
                "flea-mark:plus".into(),
                Some(PLUS_MARK),
                TransformationMatrix::unit(),
                ink,
                cx,
            );
            // The accent bar borders the ghost's leading edge through the strip's height.
            if let Some(line) = frame.line {
                let at = left + px(PAD_X) + width * line as f32 - px(1.);
                window.paint_quad(fill(
                    Bounds::new(point(at, top), size(px(2.), bounds.size.height)),
                    palette.accent,
                ));
            }
        });
    }
}
