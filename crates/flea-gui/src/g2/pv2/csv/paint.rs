//! The table prepares only its visible cells and paints its fitted rows beneath the document lanes.
use super::cells::{HEADER, NUMBER};
use super::*;
use flea::ui_logic_pv2::csv_feed::{body_face, elide, RULE_OPACITY};
use gpui::{canvas, div, fill, point, prelude::*, px, size, ContentMask, Render, TextRun};
use std::fmt::Write as _;

/// Float slack on a box's character cells: f32 glyph advances (7.2000003 for 7.2) leave a box sized to its widest cell a hair short of it.
const FIT_SLACK_CELLS: f64 = 1e-4;

impl CsvView {
    pub(super) fn prepare(&mut self, body: f64, window: &mut Window) {
        self.head.clear();
        if !self.feed.first_screen() || self.feed.table.header.is_none() {
            self.rows.clear();
            return;
        }
        let g = self.feed.geometry;
        let advance = f64::from(self.advance(window));
        let columns = self.columns_in_view();
        let (shift, down) = (-self.scroll.0, self.scroll.1);
        let frame_w = self.frame.0;
        let first = (self.scroll.1 / g.row_height).floor() as usize;
        let last =
            (((self.scroll.1 + body) / g.row_height).ceil() as usize).min(self.feed.body_rows());
        let capacity = (last.saturating_sub(first) + 1) * (columns.len() + 1) * 3;
        let scale = window.scale_factor();
        let text = window.text_system().clone();
        let CsvView {
            feed,
            face,
            cells,
            head,
            rows,
            slots,
            digits,
            ..
        } = self;
        cells.begin(&face.font.family, f32::from(face.caption), scale, capacity);
        let mut shape = |at: (usize, usize), value: &str, f: Face, x: f64, w: f64, right: bool| {
            let fit = (w / advance + FIT_SLACK_CELLS).floor().max(0.) as usize;
            // The count without allocating: one_line swaps each newline for one space, and only a miss shapes.
            let count = value.chars().count();
            // The box's visible slice in characters, exact for the monospace faces the widths assume.
            let bx = x + shift;
            let vis_l = (-bx).max(0.);
            let vis_r = (frame_w - bx).min(w).max(0.);
            let start = ((vis_l / advance).floor().max(0.) as usize).min(count);
            let end = ((vis_r / advance).ceil().max(0.) as usize).clamp(start, count);
            // A clipped box draws only its visible window; a box the frame holds draws whole, as elided.
            let windowed = (start > 0 || vis_r < w) && (start > 0 || end < count);
            let key = if windowed {
                (at.0, at.1, start + 1, end + 1)
            } else {
                (at.0, at.1, 0, 0)
            };
            let Shaped {
                text: drawn,
                cut,
                run,
            } = cells.get_or_shape(key, value, fit, f, || {
                let line = csv::one_line(value);
                if windowed {
                    let window = line
                        .chars()
                        .skip(start)
                        .take(end - start)
                        .collect::<String>();
                    let run = [text_run(&window, face.font(f))];
                    let laid = text.layout_line(&window, face.caption, &run, None);
                    (window, true, GlyphRun::new(laid))
                } else {
                    let (line, cut) = elide(&line, fit);
                    let run = [text_run(&line, face.font(f))];
                    let laid = text.layout_line(&line, face.caption, &run, None);
                    (line, cut, GlyphRun::new(laid))
                }
            });
            DrawCell {
                x: (x + shift) as f32,
                w: w as f32,
                right,
                tx: (bx + if windowed { start as f64 * advance } else { 0. }) as f32,
                windowed,
                text: drawn,
                cut,
                run,
            }
        };
        let header = feed.table.header.as_deref().unwrap_or_default();
        for c in columns.clone() {
            let (x, w) = feed.cell_box(&slots[c]);
            let value = header.get(c).map(String::as_str).unwrap_or("");
            head.push(shape((HEADER, c), value, Face::Head, x, w, slots[c].right));
        }
        let mut n = 0;
        for index in first..last.max(first) {
            let record = index + 2;
            let Some(row) = feed.table.at_record(record) else {
                continue;
            };
            let number = g.look.then(|| {
                digits.clear();
                let _ = write!(digits, "{}", csv::row_number(record));
                let w = g.number_width - g.pad_x;
                shape((record, NUMBER), digits, Face::Figure, 0., w, true)
            });
            if n == rows.len() {
                rows.push(DrawRow {
                    top: 0.,
                    record,
                    number: None,
                    cells: Vec::new(),
                });
            }
            let draw = &mut rows[n];
            draw.top = (g.head_height + index as f64 * g.row_height - down) as f32;
            draw.record = record;
            draw.number = number;
            draw.cells.clear();
            for c in columns.clone() {
                let (x, w) = feed.cell_box(&slots[c]);
                let value = row.cells.get(c).map(String::as_str).unwrap_or("");
                let right = slots[c].right;
                draw.cells
                    .push(shape((record, c), value, body_face(value), x, w, right));
            }
            n += 1;
        }
        rows.truncate(n);
    }

    /// Every text a frame draws, the header first, for the readers that check what is on screen.
    pub fn drawn_texts(&self) -> impl Iterator<Item = &str> + '_ {
        self.head
            .iter()
            .chain(
                self.rows
                    .iter()
                    .flat_map(|row| row.number.iter().chain(&row.cells)),
            )
            .map(|c| &*c.text)
    }

    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        self.painted.set(self.painted.get() + 1);
        if self.head.is_empty() {
            return;
        }
        let g = self.feed.geometry;
        let palette = self.palette;
        let origin = bounds.origin;
        let head_h = px(g.head_height as f32);
        let body = Bounds::new(
            origin + point(px(0.), head_h),
            size(bounds.size.width, (bounds.size.height - head_h).max(px(0.))),
        );
        window.with_content_mask(Some(ContentMask { bounds: body }), |window| {
            for row in &self.rows {
                let top = origin.y + px(row.top + g.body_pad_y as f32);
                if let Some(number) = &row.number {
                    cell(window, origin.x, top, number, palette.muted);
                }
                for c in &row.cells {
                    cell(window, origin.x, top, c, palette.foreground);
                }
            }
        });
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let top = origin.y + px(g.head_pad_y as f32);
            for c in &self.head {
                cell(window, origin.x, top, c, palette.bright);
            }
            let hairline = px(self.face.hairline);
            let rule = Bounds::new(
                origin + point(px(0.), head_h - hairline),
                size(bounds.size.width, hairline),
            );
            window.paint_quad(fill(rule, palette.foreground.opacity(RULE_OPACITY)));
        });
    }
}

pub(super) fn text_run(value: &str, font: Font) -> TextRun {
    TextRun {
        len: value.len(),
        font,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

// One cell inside its box, clipped to it, its baseline one ascent under the row's text top.
fn cell(window: &mut Window, left: Pixels, top: Pixels, c: &DrawCell, color: Hsla) {
    let line = c.run.line();
    let x = left
        + px(c.tx)
        + if c.right && !c.windowed {
            (px(c.w) - line.width).max(px(0.))
        } else {
            px(0.)
        };
    let clip = Bounds::new(
        point(left + px(c.x), top),
        size(px(c.w), line.ascent + line.descent),
    );
    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
        let _ = window.paint_glyph_run(
            point(x, window.pixel_snap(top + line.ascent)),
            &c.run,
            color,
        );
    });
}

/// Resolves the table's three faces and shapes a digit row in each, so a first table never waits on a font load.
pub fn warm_faces(face: &TableFace, window: &Window) -> usize {
    let text = window.text_system();
    [Face::Head, Face::Body, Face::Figure]
        .into_iter()
        .map(|f| {
            let font = face.font(f);
            let _ = text.resolve_font(&font);
            let _ = text.layout_line(
                "0123456789",
                face.caption,
                &[text_run("0123456789", font)],
                None,
            );
        })
        .count()
}

impl Render for CsvView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (laid, painted) = (cx.entity(), cx.entity());
        let bars = crate::g2::pv1::scroller::overlay::surface(
            cx.entity(),
            self.palette.foreground,
            crate::compositor::corner(cx),
        );
        div()
            .relative()
            .size_full()
            .on_scroll_wheel(cx.listener(|view, event, _, cx| view.wheel(event, cx)))
            .child(
                canvas(
                    move |bounds, window, cx: &mut App| {
                        laid.update(cx, |view, _| view.layout(bounds, window))
                    },
                    move |bounds, _, window, cx| painted.read(cx).paint(bounds, window),
                )
                .size_full(),
            )
            .child(bars)
    }
}
