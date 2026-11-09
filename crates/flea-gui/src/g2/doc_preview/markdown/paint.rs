//! The Markdown view's frame: before paint the shown blocks or Source rows become a list of quads, glyph runs and pictures, and paint draws that list.
use super::doc::Doc;
use super::style::{Faces, Palette};
use super::{FirstFrame, MarkdownView, Status, View};
use flea::ui_logic_doc_preview::markdown::{markdown_flow::Face, Block};
use gpui::{
    canvas, div, fill, point, prelude::*, px, size, App, Bounds, ContentMask, Context, Corners,
    GlyphRun, Hsla, MouseButton, Pixels, Render, RenderImage, SharedString, TextRun,
    TransformationMatrix, Window,
};
use std::{collections::HashMap, rc::Rc, sync::Arc};

mod build;
mod input;
mod rows;

/// Shaped runs kept between frames; past this only the last frame's are kept.
const GLYPHS_KEPT: usize = 4096;

struct Shaped {
    run: Rc<GlyphRun>,
    frame: u64,
}

/// Shaped runs by face, size and text, so a line already seen is not shaped again.
#[derive(Default)]
pub struct Glyphs {
    runs: HashMap<(Face, u32), HashMap<Box<str>, Shaped>>,
    count: usize,
    frame: u64,
    /// Runs shaped this frame.
    pub shaped: usize,
}

impl Glyphs {
    fn start(&mut self) {
        self.frame += 1;
        self.shaped = 0;
    }

    fn get(
        &mut self,
        text: &str,
        face: Face,
        size: f32,
        faces: &Faces,
        window: &Window,
    ) -> Rc<GlyphRun> {
        let frame = self.frame;
        let group = self.runs.entry((face, size.to_bits())).or_default();
        if let Some(found) = group.get_mut(text) {
            found.frame = frame;
            return Rc::clone(&found.run);
        }
        let run = TextRun {
            len: text.len(),
            font: faces.font(face),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = Rc::new(GlyphRun::new(window.text_system().layout_line(
            text,
            px(size),
            &[run],
            None,
        )));
        group.insert(
            text.into(),
            Shaped {
                run: Rc::clone(&shaped),
                frame,
            },
        );
        self.count += 1;
        self.shaped += 1;
        shaped
    }

    fn end(&mut self) {
        if self.count > GLYPHS_KEPT {
            let frame = self.frame;
            for group in self.runs.values_mut() {
                group.retain(|_, shaped| shaped.frame == frame);
            }
            self.runs.retain(|_, group| !group.is_empty());
            self.count = self.runs.values().map(HashMap::len).sum();
        }
    }
}

/// One thing the frame draws, in logical pixels from the view's top left.
pub enum Op {
    Quad {
        rect: (f32, f32, f32, f32),
        color: Hsla,
    },
    Text {
        selected: Option<usize>,
        x: f32,
        baseline: f32,
        run: Rc<GlyphRun>,
        color: Hsla,
    },
    Mark {
        rect: (f32, f32, f32, f32),
        key: SharedString,
        data: &'static [u8],
        color: Hsla,
    },
    Image {
        rect: (f32, f32, f32, f32),
        image: Arc<RenderImage>,
    },
    /// The next `len` ops draw inside `rect`.
    Clip {
        rect: (f32, f32, f32, f32),
        len: usize,
    },
}

/// A chunked table's chunks scroll together under their key; a whole table scrolls alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TableId {
    Key(usize),
    Block(usize, usize),
}

/// What a point of the frame reaches: a link, or a table wider than its block.
pub enum Hit {
    Link {
        rect: (f32, f32, f32, f32),
        url: String,
    },
    Table {
        rect: (f32, f32, f32, f32),
        id: TableId,
        most: f32,
        /// The first body row across the whole content width, where the sideways shift puts it.
        row: Option<(f32, f32, f32, f32)>,
    },
}

fn inside(rect: (f32, f32, f32, f32), (x, y): (f32, f32)) -> bool {
    x >= rect.0 && y >= rect.1 && x < rect.0 + rect.2 && y < rect.1 + rect.3
}

/// A touchpad stroke's taker until it ends: a table it went to sideways, or the document.
#[derive(Default)]
pub struct Pointer {
    held: Option<(TableId, f32)>,
    document: bool,
}

// Gathers one frame's ops from laid blocks.
struct Build<'a> {
    ops: &'a mut Vec<Op>,
    selection: &'a mut crate::g2::doc_preview::selection::Selection,
    text_id: usize,
    hits: &'a mut Vec<Hit>,
    glyphs: &'a mut Glyphs,
    palette: &'a Palette,
    faces: &'a Faces,
    doc: &'a Doc,
    tables: &'a HashMap<TableId, f32>,
    hairline: f32,
    height: f32,
    block: usize,
    table: usize,
    first: FirstFrame,
    window: &'a Window,
}

impl MarkdownView {
    /// Lays the frame out before paint: the view's size, the blocks or rows it shows and what each draws; hands back the pictures paint releases.
    pub(super) fn prepare(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Arc<RenderImage>> {
        let frame = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        self.bounds = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
        let resized = frame != self.frame || window.scale_factor() != self.scale;
        self.frame = frame;
        self.scale = window.scale_factor();
        if resized {
            self.pump(cx);
        }
        // Source asks for the lines its frame shows now, whatever moved it since the last frame.
        if self.shown_view() == View::Source {
            self.source_frame(frame.0, frame.1);
            self.source_pump(cx);
            self.release_hold(cx);
        }
        self.selection_start_frame(cx);
        self.ops.clear();
        self.hits.clear();
        self.glyphs.start();
        self.counts.frames += 1;
        let drawn = if self.shown_view() == View::Source
            || self.doc.status == Status::TooLarge
            || self.doc.status == Status::Unreadable
        {
            self.prepare_source(window)
        } else if !self.doc.shown {
            self.counts.held_frames += usize::from(self.doc.status != Status::Idle);
            None
        } else {
            Some(self.prepare_rendered(window))
        };
        self.glyphs.end();
        self.counts.shaped_frame = self.glyphs.shaped;
        self.counts.shaped_max = self.counts.shaped_max.max(self.glyphs.shaped);
        self.counts.items_frame = self.ops.len();
        self.counts.items_max = self.counts.items_max.max(self.ops.len());
        if let (Some(first), None) = (drawn, &self.counts.first) {
            self.counts.first = Some(first);
        }
        std::mem::take(&mut self.doc.released)
    }

    fn prepare_rendered(&mut self, window: &Window) -> FirstFrame {
        let (x0, top) = (self.tokens.inset_x(), self.tokens.inset_y() - self.scroll);
        let visible = self.visible();
        let mut build = Build {
            ops: &mut self.ops,
            selection: &mut self.selection,
            text_id: 0,
            hits: &mut self.hits,
            glyphs: &mut self.glyphs,
            palette: &self.palette,
            faces: &self.faces,
            doc: &self.doc,
            tables: &self.tables,
            hairline: self.tokens.hairline,
            height: self.frame.1,
            block: 0,
            table: 0,
            first: FirstFrame::default(),
            window,
        };
        for i in visible {
            let (Some(laid), Some(at)) = (
                build.doc.laid.get(i).and_then(Option::as_ref),
                build.doc.tops.get(i),
            ) else {
                continue;
            };
            build.block = i;
            build.table = 0;
            build.text_id = 0;
            build.first.tables +=
                usize::from(matches!(build.doc.blocks().get(i), Some(Block::Table(_))));
            build.items(&laid.geometry.items, x0, top + at);
        }
        build.first
    }

    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &App) {
        let origin = bounds.origin;
        let rect = |(x, y, w, h): (f32, f32, f32, f32)| {
            Bounds::new(origin + point(px(x), px(y)), size(px(w), px(h)))
        };
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let mut i = 0;
            while i < self.ops.len() {
                if let Op::Clip { rect: clip, len } = &self.ops[i] {
                    let (from, to) = (i + 1, (i + 1 + len).min(self.ops.len()));
                    window.with_content_mask(
                        Some(ContentMask {
                            bounds: rect(*clip),
                        }),
                        |window| {
                            for op in &self.ops[from..to] {
                                draw(self, op, &rect, window, cx);
                            }
                        },
                    );
                    i = to;
                } else {
                    draw(self, &self.ops[i], &rect, window, cx);
                    i += 1;
                }
            }
        });
    }
}

fn draw(
    view: &MarkdownView,
    op: &Op,
    rect: &impl Fn((f32, f32, f32, f32)) -> Bounds<Pixels>,
    window: &mut Window,
    cx: &App,
) {
    match op {
        Op::Quad { rect: r, color } => window.paint_quad(fill(rect(*r), *color)),
        Op::Text {
            selected,
            x,
            baseline,
            run,
            color,
        } => {
            let at = rect((*x, *baseline, 0., 0.)).origin;
            let at = point(at.x, window.pixel_snap(at.y));
            let _ = window.paint_glyph_run(at, run, *color);
            if let Some(index) = selected {
                let accent = cx
                    .try_global::<crate::palette::Theme>()
                    .copied()
                    .unwrap_or_default()
                    .accent;
                view.selection_run(*index, run, at, accent, window);
            }
        }
        Op::Mark {
            rect: r,
            key,
            data,
            color,
        } => {
            let _ = window.paint_svg(
                rect(*r),
                key.clone(),
                Some(data),
                TransformationMatrix::unit(),
                *color,
                cx,
            );
        }
        Op::Image { rect: r, image } => {
            let bounds = rect(*r);
            let _ = window.paint_image(
                bounds,
                bounds,
                Corners::default(),
                Arc::clone(image),
                0,
                false,
            );
        }
        Op::Clip { .. } => {}
    }
}

impl Render for MarkdownView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (laid, painted) = (cx.entity(), cx.entity());
        div()
            .id("pv2-markdown-view")
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event, window, cx| view.text_press(event, window, cx)),
            )
            .on_mouse_move(
                cx.listener(|view, event, window, cx| view.text_motion(event, window, cx)),
            )
            .on_mouse_down_out(cx.listener(|view, _: &gpui::MouseDownEvent, _, cx| {
                view.selection.clear();
                cx.notify();
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.text_release(cx);
                }),
            )
            .on_scroll_wheel(cx.listener(|view, event, _, cx| view.wheel(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, event: &gpui::MouseUpEvent, _, cx| {
                    if view.text_release(cx) && !event.modifiers.shift && event.click_count == 1 {
                        view.click(event, cx);
                    }
                }),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        laid.update(cx, |view, cx| view.prepare(bounds, window, cx))
                    },
                    move |bounds, released, window, cx| {
                        for image in released {
                            let _ = window.drop_image(image);
                        }
                        painted.read(cx).paint(bounds, window, cx);
                    },
                )
                .size_full(),
            )
    }
}
