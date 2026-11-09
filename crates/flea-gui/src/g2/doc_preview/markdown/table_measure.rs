//! Complete table-cell runs keep ligatures and joined glyphs at their painted advances.
use super::super::style::Faces;
use flea::ui_logic_doc_preview::markdown::markdown_flow::Face;
use gpui::{px, TextRun, TextSystem, WindowTextSystem};
use std::{collections::HashMap, sync::Arc};

const RUNS_KEPT: usize = 4096;
const BYTES_KEPT: usize = 1_048_576;

pub(super) struct Runs {
    widths: HashMap<(Face, u32), HashMap<Box<str>, f32>>,
    count: usize,
    bytes: usize,
    system: Arc<TextSystem>,
    text: WindowTextSystem,
}

impl Runs {
    pub(super) fn new(text: Arc<TextSystem>) -> Self {
        Self {
            widths: HashMap::new(),
            count: 0,
            bytes: 0,
            text: WindowTextSystem::new(Arc::clone(&text)),
            system: text,
        }
    }
    pub(super) fn width(
        &mut self,
        text: &str,
        face: Face,
        size: f32,
        faces: &Faces,
    ) -> (f32, bool) {
        if let Some(width) = self
            .widths
            .get(&(face, size.to_bits()))
            .and_then(|group| group.get(text))
        {
            return (*width, false);
        }
        if self.count >= RUNS_KEPT || self.bytes.saturating_add(text.len()) > BYTES_KEPT {
            self.widths.clear();
            self.count = 0;
            self.bytes = 0;
            self.text = WindowTextSystem::new(Arc::clone(&self.system));
        }
        let run = TextRun {
            len: text.len(),
            font: faces.font(face),
            color: crate::palette::Theme::default().foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let width = f32::from(self.text.layout_line(text, px(size), &[run], None).width);
        self.widths
            .entry((face, size.to_bits()))
            .or_default()
            .insert(text.into(), width);
        self.bytes += text.len();
        self.count += 1;
        (width, true)
    }
}
