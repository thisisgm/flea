//! The code view's layout and paint: the lines in view shaped once into the cache, then painted from their glyph runs,
//! with the binary and too-large frames drawn as the kind's mark over one caption.
use super::shape::{self, Shaped, Shaping};
use super::CodeView;
use gpui::{
    canvas, div, point, prelude::*, px, size, Bounds, ContentMask, Context, GlyphRun, Hsla, Pixels,
    Render, TransformationMatrix, Window,
};
use std::rc::Rc;

// Lines kept shaped past the view, so a short scroll back reshapes nothing.
const CACHE_SCREENS: usize = 3;
// The board draws the gutter's caption one pixel above the body line's centre.
const NUMBER_RISE: f32 = 1.;
const FRAME_MARK_OPACITY: f32 = 0.7;

/// What a frame is built from beyond the view's revision: the first line, the offset into it, the frame, the row
/// metrics and the shaping, as bits so an unchanged frame compares equal.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameKey {
    revision: u64,
    top: usize,
    offset: u32,
    frame: (u32, u32),
    row: (u32, u32),
    shaping: Shaping,
}

/// What one frame paints: each row in view with its top, and the refusal frame's caption when one shows.
#[derive(Default)]
pub struct Frame {
    pub rows: Vec<(f32, Rc<Shaped>)>,
    pub caption: Option<GlyphRun>,
}

impl CodeView {
    /// A settled file with no lines and no refusal: the frame stands the kind's mark in for it.
    pub fn blank(&self) -> bool {
        self.refusal().is_none() && self.lines.answer.settled() && self.lines.total == 0
    }

    /// F and B: one page by whole rows, snapping a partial row under the top padding.
    pub fn page(&mut self, direction: i64, fallback: f32, cx: &mut Context<Self>) {
        let viewport = if self.frame.1 > 0. {
            self.frame.1
        } else {
            fallback
        };
        let step = flea::ui_logic_doc_preview::code_view::page_step(viewport, self.metrics.row);
        let partial = self.offset % self.metrics.row.max(1.);
        self.scroll_by(direction as f32 * step - partial, cx);
    }

    fn advance(&mut self, window: &Window) -> Pixels {
        if let Some(advance) = self.advance {
            return advance;
        }
        let text = window.text_system();
        let id = text.resolve_font(&self.typeface.font);
        if self.slot == "look" {
            // The board pitch is a minimum; taller body glyphs still determine their row (EY-170).
            let run = shape::glyphs(window, "0", &self.typeface.font, self.typeface.body);
            self.metrics.row = f32::from(run.line().ascent + run.line().descent)
                .ceil()
                .max(self.metrics.row)
                .max(1.);
        }
        let advance = text
            .advance(id, self.typeface.body, '0')
            .map_or(self.typeface.body * 0.6, |a| a.width);
        self.advance = Some(advance);
        advance
    }

    /// Lays the view out in its bounds and shapes the lines entering it; asks for the lines the new window needs.
    pub fn layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Rc<Frame> {
        // Quick Look keeps its top pad under the last line too, as v0.3.12's CodeLines margins do (EY-428).
        let inset = self.scroll_inset();
        let frame = (
            f32::from(bounds.size.width),
            (f32::from(bounds.size.height) - self.metrics.top - inset).max(0.),
        );
        let moved = frame != self.frame;
        self.frame = frame;
        self.center_match(cx);
        let advance = self.advance(window);
        self.fit_scroll(Bounds::new(
            bounds.origin + point(px(inset), px(self.metrics.top)),
            size(
                (bounds.size.width - px(2. * inset)).max(px(0.)),
                px(self.frame.1),
            ),
        ));
        if moved {
            self.flush(cx);
        }
        let shaping = Shaping {
            font: self.typeface.font.clone(),
            body: self.typeface.body,
            caption: self.typeface.caption,
            cells: self.text_cells(advance),
            wrap: self.wrap,
            numbered: self.numbered,
            scale: window.scale_factor(),
        };
        let key = FrameKey {
            revision: self.revision,
            top: self.top,
            offset: self.offset.to_bits(),
            frame: (frame.0.to_bits(), frame.1.to_bits()),
            row: (self.metrics.row.to_bits(), self.metrics.top.to_bits()),
            shaping,
        };
        if let Some((laid, out)) = &self.laid {
            if *laid == key {
                let out = Rc::clone(out);
                self.selection_frame(bounds, &out);
                return out;
            }
        }
        let out = Rc::new(self.build(&key.shaping, window));
        self.selection_frame(bounds, &out);
        self.laid = Some((key, Rc::clone(&out)));
        out
    }

    // Builds the frame: the refusal's caption, or each row in view shaped once into the row cache.
    fn build(&mut self, shaping: &Shaping, window: &Window) -> Frame {
        let mut out = Frame::default();
        self.built.set(self.built.get() + 1);
        if let Some(refusal) = self.refusal() {
            if let (Some(key), Some(catalog)) = (refusal.frame_key(), self.catalog.as_ref()) {
                out.caption = Some(shape::glyphs(
                    window,
                    &catalog.tr(key),
                    &self.typeface.font,
                    self.typeface.caption,
                ));
            }
            return out;
        }
        let row = self.metrics.row.max(1.);
        let rows = (self.frame.1 / row).ceil() as usize + 1;
        self.cache.begin(shaping, rows * CACHE_SCREENS);
        self.bound_prepared(rows * CACHE_SCREENS);
        let mut y = -self.offset;
        let mut line = self.top;
        while y < self.frame.1 && line < self.lines.shown() {
            let Some(prepared) = self.prepared(line) else {
                y += row;
                line += 1;
                continue;
            };
            let count = if self.wrap { prepared.rows.len() } else { 1 };
            let first = ((-y / row).floor().max(0.) as usize).min(count);
            let last = if self.slot == "column" {
                ((self.frame.1 - y) / row).floor()
            } else {
                ((self.frame.1 - y) / row).ceil()
            };
            let last = (last.max(0.) as usize).min(count);
            for r in first..last {
                let shaped = self.cache.rows.get_or_make((line, r), || {
                    Rc::new(shape::shape_row(window, shaping, &prepared, r, line + 1))
                });
                out.rows.push((y + r as f32 * row, shaped));
            }
            y += count as f32 * row;
            line += 1;
        }
        out
    }

    fn paint(&self, bounds: Bounds<Pixels>, frame: &Frame, window: &mut Window, cx: &gpui::App) {
        self.painted.set(self.painted.get() + 1);
        self.painted_syntax.borrow_mut().clear();
        let Some(advance) = self.advance else { return };
        let origin = bounds.origin + point(px(0.), px(self.metrics.top));
        if self.refusal().is_some() {
            self.paint_frame(bounds, frame, window, cx);
            return;
        }
        let row = px(self.metrics.row);
        let (gutter, text_left) = (px(self.gutter_right(advance)), px(self.text_left(advance)));
        let clip = Bounds::new(origin, size(bounds.size.width, px(self.frame.1)));
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            for (top, shaped) in &frame.rows {
                let top = origin.y + px(*top);
                self.paint_match(shaped, point(origin.x + text_left, top), row, window);
                self.painted_rows.set(self.painted_rows.get() + 1);
                if let Some(number) = &shaped.number {
                    let x = origin.x + gutter - number.line().width;
                    let baseline = baseline(number, top, row, window) - px(NUMBER_RISE);
                    let _ = window.paint_glyph_run(point(x, baseline), number, self.palette.gutter);
                }
                for piece in &shaped.pieces {
                    let ink: Hsla = self.palette.inks[piece.ink];
                    let y = baseline(&piece.run, top, row, window);
                    if window
                        .paint_glyph_run(point(origin.x + text_left + piece.x, y), &piece.run, ink)
                        .is_ok()
                    {
                        if let Some(prepared) = self
                            .prepared
                            .borrow()
                            .1
                            .peek(&shaped.line.saturating_sub(1))
                        {
                            self.painted_syntax
                                .borrow_mut()
                                .push(super::capture::Syntax {
                                    text: Rc::clone(prepared),
                                    span: piece.start..piece.start + piece.len,
                                    ink,
                                });
                        }
                    }
                    if let Some(part) = self.selection.part(shaped.line, piece.start, piece.len) {
                        let accent = cx
                            .try_global::<crate::palette::Theme>()
                            .copied()
                            .unwrap_or_default()
                            .accent;
                        crate::g2::doc_preview::selection::paint(
                            &piece.run,
                            point(origin.x + text_left + piece.x, y),
                            Bounds::new(point(origin.x, top), size(bounds.size.width, row)),
                            part,
                            None,
                            (accent, self.palette.foreground),
                            window,
                        );
                    }
                }
            }
        });
    }

    fn paint_frame(
        &self,
        bounds: Bounds<Pixels>,
        frame: &Frame,
        window: &mut Window,
        cx: &gpui::App,
    ) {
        if let Some(caption) = &frame.caption {
            let m = &self.metrics;
            refusal_frame(
                window,
                cx,
                bounds,
                (m.top, m.state_mark, m.gap),
                &self.mark,
                caption,
                self.palette.muted,
            );
        }
    }

    /// Where the frame's mark sits, for the probes and the board check; None when no frame shows.
    pub fn frame_text(&self, i18n: &flea::i18n::I18n) -> Option<String> {
        self.refusal()
            .and_then(|refusal| refusal.frame_key())
            .map(|key| i18n.tr(key))
    }
}

/// The binary and too-large frame: the kind's mark over the caption, centred in the bounds under `top`.
pub fn refusal_frame(
    window: &mut Window,
    cx: &gpui::App,
    bounds: Bounds<Pixels>,
    (top, mark, gap): (f32, f32, f32),
    (key, data): &super::Mark,
    caption: &GlyphRun,
    ink: Hsla,
) {
    let (mark, gap, line) = (px(mark), px(gap), caption.line());
    let height = mark + gap + line.ascent + line.descent;
    let y = bounds.origin.y + px(top) + ((bounds.size.height - px(top) - height) / 2.).round();
    let centre = bounds.origin.x + bounds.size.width / 2.;
    let mark_bounds = Bounds::new(point(centre - mark / 2., y), size(mark, mark));
    let _ = window.paint_svg(
        mark_bounds,
        key.clone(),
        Some(*data),
        TransformationMatrix::unit(),
        ink.opacity(FRAME_MARK_OPACITY),
        cx,
    );
    let _ = window.paint_glyph_run(
        point(centre - line.width / 2., y + mark + gap + line.ascent),
        caption,
        ink,
    );
}

// The baseline that centres a shaped line in a row of the body line's height.
fn baseline(run: &GlyphRun, top: Pixels, row: Pixels, window: &Window) -> Pixels {
    let line = run.line();
    window.pixel_snap(top + (row - line.ascent - line.descent) / 2. + line.ascent)
}

impl Render for CodeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (laid, painted) = (cx.entity(), cx.entity());
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let mut look =
            crate::g2::quick_look_overlay::look::Look::of_text(&window.text_style(), theme);
        look.foreground = self.palette.foreground;
        let inset = px(self.scroll_inset());
        if let Some(view) = self.scroll.view {
            self.fit_scroll(view.bounds);
        }
        div()
            .id("pv2-code-view")
            .relative()
            .size_full()
            .on_scroll_wheel(cx.listener(|view, event, _, cx| view.wheel(event, cx)))
            .on_mouse_move(
                cx.listener(|view, event, window, cx| view.text_motion(event, window, cx)),
            )
            .on_mouse_down_out(cx.listener(|view, _: &gpui::MouseDownEvent, _, cx| {
                view.selection.clear();
                cx.notify();
            }))
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if !hovered && view.scroll.lane.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                    view.text_press(event, window, cx);
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.selection.release(cx);
                    if view.scroll.release() {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.selection.release(cx);
                    if view.scroll.release() {
                        cx.notify();
                    }
                }),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        laid.update(cx, |view, cx| view.layout(bounds, window, cx))
                    },
                    move |bounds, frame, window, cx| {
                        painted.read(cx).paint(bounds, &frame, window, cx)
                    },
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top(px(self.metrics.top))
                    .bottom(inset)
                    .left(inset)
                    .right(inset)
                    .children(self.scroll.bars(&look)),
            )
    }
}
