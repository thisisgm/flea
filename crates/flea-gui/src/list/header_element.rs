//! The header Element implementation, with state and drag ownership in header.
use super::*;

impl Element for HeaderElement {
    type RequestLayoutState = ();
    type PrepaintState = HeaderFrame;

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
        let height = element_height(&self.state.borrow().typeface);
        let style = Style {
            size: size(relative(1.).into(), px(height).into()),
            flex_shrink: 0.,
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
    ) -> HeaderFrame {
        let mut state = self.state.borrow_mut();
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = state.palette(window, theme);
        let width = f32::from(bounds.size.width);
        let (metrics, tokens, mut layout) = state.layout(width, window);
        let drag = state.header.drag;
        if let Some(column) = drag.column {
            // The preview moves the header alone; the rows keep the stored layout until the one write on release.
            layout = layout.previewing(
                column,
                drag.preview,
                &tokens,
                (width - metrics.lane).max(0.),
            );
        }
        let key = TitleKey {
            order: state.view.order,
            recent: state.view.recent,
            font: state.typeface.font.clone().bold(),
            caption: state.typeface.caption,
            scale: window.scale_factor(),
        };
        let ListState { view, header, .. } = &mut *state;
        if header
            .titles
            .as_ref()
            .is_none_or(|titles| titles.key != key)
        {
            let text = window.text_system();
            let shape = |column: Option<Column>| {
                let title = words::painted(view, column);
                Title {
                    run: GlyphRun::new(text.layout_line(
                        &title,
                        key.caption,
                        &[title_run(title.len(), &key.font)],
                        None,
                    )),
                    text: title,
                    cut: None,
                }
            };
            header.painted_titles = titles(view);
            header.titles = Some(Titles {
                name: shape(None),
                columns: Column::ALL.map(|column| shape(Some(column))),
                key,
            });
            header.drawn.shaped += 1;
        }
        header.fit_titles(&layout, &metrics, view.dual, window);
        header.drawn = HeaderDrawn {
            bounds,
            layout,
            frames: header.drawn.frames + 1,
            shaped: header.drawn.shaped,
        };
        let active = !view.dual && !view.searching;
        HeaderFrame {
            palette,
            metrics,
            layout,
            line: drag
                .column
                .or(header.hot)
                .filter(|column| active && layout.shown(*column)),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        frame: &mut HeaderFrame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let HeaderFrame {
            palette,
            metrics,
            layout,
            line,
        } = *frame;
        let state = self.state.borrow();
        let Some(titles) = &state.header.titles else {
            return;
        };
        let left = bounds.origin.x;
        let top = bounds.origin.y;
        window.paint_quad(fill(bounds, palette.foreground.opacity(NORMAL)));
        let rule = Bounds::new(
            point(left, top + bounds.size.height - px(metrics.hairline)),
            size(bounds.size.width, px(metrics.hairline)),
        );
        window.paint_quad(fill(rule, palette.foreground.opacity(HAIRLINE)));
        // The board's padding sits above the titles, so their line box is the header less that padding.
        let text_top = top + px(metrics.header_top);
        let text_height = metrics.header - metrics.header_top;
        let dual = state.view.dual;
        window.paint_layer(bounds, |window| {
            for column in words::CELLS {
                let Some(cell) = HeaderState::title_cell(&layout, &metrics, column, dual) else {
                    continue;
                };
                let title = column.map_or(&titles.name, |column| &titles.columns[column.slot()]);
                let (run, _) = title.drawn();
                let x = if right_aligned(column) {
                    left + px(cell.right()) - run.line().width
                } else {
                    left + px(cell.x)
                };
                let mask = Bounds::new(
                    point(left + px(cell.x), top),
                    size(px(cell.width), bounds.size.height),
                );
                window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                    paint_line(
                        run,
                        x,
                        text_top,
                        text_height,
                        crate::window::glass::producers::text_ink(palette.metadata, window, cx),
                        window,
                    );
                });
            }
        });
        // ResizeHandle.qml's accent line: one hairline on the column's left edge while its zone is hovered or dragged.
        if let Some(cell) = line.and_then(|column| layout.cell(column)) {
            let edge = Bounds::new(
                point(left + px(cell.x), top),
                size(px(metrics.hairline), bounds.size.height),
            );
            window.paint_quad(fill(edge, palette.accent));
        }
    }
}
