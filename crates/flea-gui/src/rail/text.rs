//! Bounded rail text preparation; paint only consumes retained glyph runs.
use crate::list::Typeface;
use gpui::{point, px, size, Bounds, Font, GlyphRun, Hsla, Pixels, TextRun, Window};
use std::{collections::VecDeque, rc::Rc};

pub struct Prepared {
    pub runs: Vec<(f32, GlyphRun)>,
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
}

impl Prepared {
    pub fn x_for(&self, byte: usize) -> f32 {
        self.runs
            .first()
            .map_or(0., |(_, run)| f32::from(run.line().x_for_index(byte)))
    }
    pub fn index_for(&self, x: f32) -> usize {
        self.runs
            .first()
            .map_or(0, |(_, run)| run.line().closest_index_for_x(px(x)))
    }

    pub fn paint(&self, x: f32, y: f32, height: f32, ink: Hsla, window: &mut Window) {
        let baseline = y + (height - self.height) / 2. + self.ascent;
        // A tracked label is one run per letter, so it mirrors as one unit to keep its letter order.
        let drawn = self
            .runs
            .last()
            .map_or(0., |(offset, run)| offset + f32::from(run.line().width));
        let label = Bounds::new(point(px(x), px(y)), size(px(drawn), px(height)));
        window.with_unmirrored(label, |window| {
            for (offset, run) in &self.runs {
                let _ = window.paint_glyph_run(point(px(x + offset), px(baseline)), run, ink);
            }
        });
    }
}

struct Entry {
    text: String,
    font: Font,
    size: Pixels,
    width: f32,
    tracking: f32,
    scale: f32,
    prepared: Rc<Prepared>,
}

#[derive(Default)]
pub struct Cache {
    entries: VecDeque<Entry>,
    pub misses: u64,
}

fn shape(text: &str, size: Pixels, font: &Font, window: &Window) -> GlyphRun {
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(window.text_system().layout_line(text, size, &[run], None))
}

impl Cache {
    pub fn get(
        &mut self,
        text: &str,
        size: Pixels,
        width: f32,
        tracking: f32,
        face: &Typeface,
        window: &Window,
    ) -> Rc<Prepared> {
        let scale = window.scale_factor();
        if let Some(at) = self.entries.iter().position(|entry| {
            entry.text == text
                && entry.font == face.font
                && entry.size == size
                && entry.width == width
                && entry.tracking == tracking
                && entry.scale == scale
        }) {
            let entry = self.entries.remove(at).expect("the located cache entry");
            let prepared = Rc::clone(&entry.prepared);
            self.entries.push_back(entry);
            return prepared;
        }
        self.misses += 1;
        let full = shape(text, size, &face.font, window);
        let mut visible = text.to_owned();
        if f32::from(full.line().width) > width {
            let ellipsis = shape("…", size, &face.font, window);
            let room = (width - f32::from(ellipsis.line().width)).max(0.);
            let mut cut = 0;
            for (byte, ch) in text.char_indices() {
                let next = byte + ch.len_utf8();
                if f32::from(shape(&text[..next], size, &face.font, window).line().width) > room {
                    break;
                }
                cut = next;
            }
            visible = format!("{}…", &text[..cut]);
        }
        let line = full.line();
        let mut prepared = Prepared {
            width: f32::from(line.width),
            height: f32::from(line.ascent + line.descent),
            ascent: f32::from(line.ascent),
            runs: Vec::new(),
        };
        if tracking > 0.
            && visible
                .chars()
                .all(|c| c.is_ascii() || matches!(c as u32, 0xC0..=0x24F | 0x370..=0x52F))
        {
            let mut x = 0.;
            for ch in visible.chars() {
                let run = shape(&ch.to_string(), size, &face.font, window);
                let advance = f32::from(run.line().width);
                prepared.runs.push((x, run));
                x += advance + tracking;
            }
            prepared.width = (x - tracking).max(0.);
        } else {
            prepared
                .runs
                .push((0., shape(&visible, size, &face.font, window)));
        }
        let prepared = Rc::new(prepared);
        self.entries.push_back(Entry {
            text: text.to_owned(),
            font: face.font.clone(),
            size,
            width,
            tracking,
            scale,
            prepared: Rc::clone(&prepared),
        });
        prepared
    }

    pub fn bound(&mut self, capacity: usize) {
        while self.entries.len() > capacity {
            self.entries.pop_front();
        }
    }
}
