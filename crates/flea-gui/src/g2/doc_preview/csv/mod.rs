//! The CSV table as a GPUI view: the column's fitted table and Quick Look's grid, fed raw lines by the code engine and
//! parsed off the UI thread; each frame lays out only the cells it shows, and a frame of cache hits allocates nothing.
mod cells;

pub use cells::{CellCache, Shaped};
use flea::i18n::I18n;
use flea::ui_logic_doc_preview::csv;
use flea::ui_logic_doc_preview::csv::Slot;
use flea::ui_logic_doc_preview::csv_feed::{self, Face, Feed, Geometry};
use gpui::{
    App, Bounds, Context, Font, FontFeatures, GlyphRun, Hsla, Pixels, ScrollWheelEvent, Task,
    Window,
};
use std::{cell::Cell, ops::Range, rc::Rc, sync::Arc};

/// The host's code engine: it picks the request's token, sends the line `line(token)` writes, and answers the token.
pub trait CodeHost {
    fn code(&self, line: &dyn Fn(u64) -> String) -> u64;
}

/// The theme roles the table draws with: cells in foreground, the header in the bright foreground, numbers muted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TablePalette {
    pub foreground: Hsla,
    pub bright: Hsla,
    pub muted: Hsla,
}

/// The theme's caption face and spacing the table sets its type and rows from.
#[derive(Clone, Debug, PartialEq)]
pub struct TableFace {
    pub font: Font,
    pub caption: Pixels,
    /// The caption's line height in logical pixels.
    pub line: f32,
    pub row_padding_x: f32,
    pub hairline: f32,
}

impl TableFace {
    /// The font each face draws in: the header bold, numbers in tabular figures.
    pub fn font(&self, face: Face) -> Font {
        match face {
            Face::Head => self.font.clone().bold(),
            Face::Figure => Font {
                features: FontFeatures(Arc::new(vec![("tnum".into(), 1)])),
                ..self.font.clone()
            },
            Face::Body => self.font.clone(),
        }
    }

    pub fn geometry(&self, look: bool) -> Geometry {
        Geometry::new(
            look,
            f64::from(self.caption),
            f64::from(self.line),
            f64::from(self.row_padding_x),
            f64::from(self.hairline),
        )
    }
}

/// One drawn cell: its text box from the frame's left, its alignment, its text and its shaped run.
pub struct DrawCell {
    pub x: f32,
    pub w: f32,
    pub right: bool,
    /// The text's left edge from the frame's left: the box's for a whole cell, the window's offset for a drawn part.
    pub tx: f32,
    /// The drawn text is the visible window of a longer cell, so no alignment shifts it inside its box.
    pub windowed: bool,
    pub text: Rc<str>,
    pub cut: bool,
    pub run: Rc<GlyphRun>,
}

/// One drawn body row: its top inside the frame, its record, its row number and its cells.
pub struct DrawRow {
    pub top: f32,
    pub record: usize,
    pub number: Option<DrawCell>,
    pub cells: Vec<DrawCell>,
}

pub struct CsvView {
    pub feed: Feed,
    slot: &'static str,
    path: String,
    host: Option<Box<dyn CodeHost>>,
    pub face: TableFace,
    pub palette: TablePalette,
    /// The grid's scroll, logical pixels: sideways in Quick Look only.
    pub scroll: (f64, f64),
    bars: crate::g2::quick_look_overlay::scroller::overlay::Overlay,
    pub frame: (f64, f64),
    pub cells: CellCache,
    pub head: Vec<DrawCell>,
    pub rows: Vec<DrawRow>,
    /// The column boxes, kept until a column lands or widens, or the frame's width changes.
    slots: Vec<Slot>,
    slots_for: Option<(usize, u64, u64)>,
    digits: String,
    landed: Option<csv::Table>,
    parse: Option<Task<()>>,
    advance: Option<(Pixels, f32)>,
    /// Frames painted, for the native run's idle count.
    pub painted: Cell<usize>,
}

impl CsvView {
    /// Quick Look's grid when `look`, the column's fitted table otherwise.
    pub fn new(look: bool, face: TableFace, palette: TablePalette) -> Self {
        let geometry = face.geometry(look);
        CsvView {
            feed: Feed::new(None, geometry),
            slot: if look { "look" } else { "column" },
            path: String::new(),
            host: None,
            face,
            palette,
            scroll: (0., 0.),
            bars: Default::default(),
            frame: (0., 0.),
            cells: CellCache::default(),
            head: Vec::new(),
            rows: Vec::new(),
            slots: Vec::new(),
            slots_for: None,
            digits: String::with_capacity(24),
            landed: None,
            parse: None,
            advance: None,
            painted: Cell::new(0),
        }
    }

    pub fn set_host(&mut self, host: Box<dyn CodeHost>) {
        self.host = Some(host);
    }

    fn fresh(&mut self, path: &str) {
        self.feed.reset(csv::delimiter(path));
        self.path = path.to_string();
        self.scroll = (0., 0.);
        self.bars = Default::default();
        self.cells.clear();
        self.head.clear();
        self.rows.clear();
        self.slots_for = None;
        self.landed = None;
        self.parse = None;
    }

    /// A file the engine reads: its first ask goes at the next layout, once the frame's rows are known.
    pub fn set_path(&mut self, path: &str, cx: &mut Context<Self>) {
        self.fresh(path);
        cx.notify();
    }

    /// The caller's own text, named for its delimiter, drawn plain with no engine.
    pub fn set_text(&mut self, name: &str, text: &str, cx: &mut Context<Self>) {
        self.fresh(name);
        self.path.clear();
        self.feed.answer_plain(text);
        self.parse_now(cx);
    }

    /// One code reply from the host; only the ask in flight takes it, and its parse runs off this thread.
    pub fn answered(&mut self, line: &str, cx: &mut Context<Self>) -> bool {
        let Some(reply) = csv_feed::parse_reply(line) else {
            return false;
        };
        if !self.feed.answered(&reply) {
            return false;
        }
        self.parse_now(cx);
        true
    }

    fn parse_now(&mut self, cx: &mut Context<Self>) {
        let job = self.feed.job();
        self.parse = Some(cx.spawn(async move |this, cx| {
            let table = cx
                .background_executor()
                .spawn(async move { job.run() })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.landed = Some(table);
                view.parse = None;
                cx.notify();
            });
        }));
    }

    /// The bar's count, "24 rows", once the engine has answered.
    pub fn count_text(&self, i18n: &I18n) -> Option<String> {
        self.feed
            .settled()
            .then(|| i18n.trn("document.csv_rows", self.feed.body_rows() as i64))
    }

    /// The table draws only once its first screen is whole, so no frame shows half a table.
    pub fn first_screen(&self) -> bool {
        self.feed.first_screen()
    }

    /// Scrolls by `rows` body rows, as j and k do.
    pub fn scroll_rows(&mut self, rows: i64, cx: &mut Context<Self>) {
        self.scroll.1 += rows as f64 * self.feed.geometry.row_height;
        cx.notify();
    }

    /// Scrolls the grid to show body row `index` at its top, as a far jump does.
    pub fn scroll_to_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.scroll.1 = index as f64 * self.feed.geometry.row_height;
        cx.notify();
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = crate::input::scroll::wheel_delta(&event.delta);
        self.scroll.1 -= f64::from(delta.y);
        if self.feed.geometry.look {
            self.scroll.0 -= f64::from(delta.x);
        }
        cx.notify();
    }

    /// Lays the table out before paint: lands a parse, measures new columns, asks what the frame needs, shapes cells.
    pub fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        self.feed.counters.frames += 1;
        self.frame = (f64::from(bounds.size.width), f64::from(bounds.size.height));
        if let Some(table) = self.landed.take() {
            self.feed.apply(table);
        }
        let g = self.feed.geometry;
        let body = (self.frame.1 - g.head_height).max(0.);
        let content = self.feed.body_rows() as f64 * g.row_height;
        self.scroll.1 = self.scroll.1.min(content - body).max(0.);
        self.measure(window);
        // Held cells left to widen by take the next frames, so no one frame measures every held row.
        if self.feed.widening {
            window.request_animation_frame();
        }
        self.lay_slots();
        self.scroll.0 = if g.look {
            let end = self.slots.last().map_or(0., |slot| slot.x + slot.w);
            self.scroll
                .0
                .min(end.max(self.frame.0) - self.frame.0)
                .max(0.)
        } else {
            0.
        };
        let top = (self.scroll.1 / g.row_height).floor() as usize + 2;
        self.feed.set_view(g.view_rows(body), top);
        if self.frame.1 > 0. {
            self.ask();
        }
        self.prepare(body, window);
    }

    fn ask(&mut self) {
        let (Some(host), Some(delim)) = (&self.host, self.feed.delim) else {
            return;
        };
        if self.path.is_empty() {
            return;
        }
        if let Some(ask) = self.feed.ask() {
            let (slot, path) = (self.slot, self.path.as_str());
            let token = host.code(&|token| csv_feed::ask_line(token, slot, path, delim, &ask));
            self.feed.sent(token);
        }
    }

    // One advance of "0" at the caption, which turns a box's width into the character cells a cell elides to.
    fn advance(&mut self, window: &Window) -> f32 {
        if let Some((size, advance)) = self.advance {
            if size == self.face.caption {
                return advance;
            }
        }
        let text = window.text_system();
        let id = text.resolve_font(&self.face.font);
        let advance = text
            .advance(id, self.face.caption, '0')
            .map_or(f32::from(self.face.caption) * 0.6, |a| f32::from(a.width));
        self.advance = Some((self.face.caption, advance.max(1.)));
        advance.max(1.)
    }

    // Measures the columns the scroll and the frame reach, so a column past the first screen costs only once it is reached.
    fn measure(&mut self, window: &Window) {
        let limit = self.feed.reach_target(self.scroll.0.max(0.), self.frame.0);
        if !self.feed.wants_measure(limit) {
            return;
        }
        let advance = f64::from(self.advance(window));
        let face = self.face.clone();
        let text = window.text_system().clone();
        // Never under the cells `prepare` elides by, so a box measured to a cell holds all of its characters.
        self.feed.measure(limit, &mut |value, f| {
            let run = text_run(value, face.font(f));
            let shaped = f64::from(text.layout_line(value, face.caption, &[run], None).width);
            shaped.max(value.chars().count() as f64 * advance)
        });
    }

    // The column boxes for this frame's width, laid out again only when a column lands or widens, or the width changes.
    fn lay_slots(&mut self) {
        let key = (
            self.feed.widths.len(),
            self.feed.revision,
            self.frame.0.to_bits(),
        );
        if self.slots_for != Some(key) {
            self.slots = self.feed.layout(self.frame.0);
            self.slots_for = Some(key);
        }
    }

    // The columns whose text boxes meet the frame after the sideways scroll, and one more ahead of them.
    fn columns_in_view(&self) -> Range<usize> {
        let (left, frame) = (self.scroll.0, self.frame.0);
        let ends = |slot: &Slot| {
            let (x, w) = self.feed.cell_box(slot);
            (x - left, x + w - left)
        };
        let from = self.slots.partition_point(|slot| ends(slot).1 <= 0.);
        let to = self.slots.partition_point(|slot| ends(slot).0 < frame);
        from..(to + 1).min(self.slots.len()).max(from)
    }
}

mod paint;
mod scroll;
use paint::text_run;
pub use paint::warm_faces;
