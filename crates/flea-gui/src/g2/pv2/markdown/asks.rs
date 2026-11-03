//! What the shown blocks ask their host for, how each answer lands, and the hold that keeps the first screen until its blocks are whole.
use super::assets::{Art, Decoded};
use super::doc::{FenceAsk, PictureState};
use super::style::hex;
use super::{CodeAnswer, CodeSource, CodeState, FigureDone, MarkdownView, CODE_LINE_CAP};
use flea::ui_logic_pv2::markdown::markdown_flow::Measure;
use flea::ui_logic_pv2::markdown::{
    markdown_figure::{self as figure, Theme},
    markdown_fit::formula_box,
    markdown_flow::{Atom, Face, FormulaBox},
    markdown_layout::{FigureState, Geometry, Item},
    markdown_prepared::{inside_dir, path_of_url},
    FigureKind,
};
use gpui::Context;
use std::{path::PathBuf, sync::Arc};

mod hold;
pub use hold::FIGURE_HOLD_MS;

// Every picture address laid items draw, inline, alone or inside a table.
fn picture_urls(items: &[Item], out: &mut Vec<String>) {
    for item in items {
        match item {
            Item::Picture { url, .. } => out.push(url.clone()),
            Item::Text { laid, .. } => {
                for atom in laid.lines.iter().flat_map(|line| &line.atoms) {
                    if let Atom::Picture { url, .. } = atom {
                        out.push(url.clone());
                    }
                }
            }
            Item::Table { items, .. } => picture_urls(items, out),
            _ => {}
        }
    }
}

impl MarkdownView {
    /// The theme a figure of a kind is asked under: the palette's inks, the body face and, for a diagram, its label advances.
    pub fn figure_theme(&mut self, kind: FigureKind) -> Theme {
        let body = self.tokens.body;
        let measure = self.source.measure();
        let x_height = measure.x_height(Face::Regular, body);
        let mut table = |face: Face| {
            let widths: Vec<f64> = (figure::ADVANCE_FIRST..=figure::ADVANCE_LAST)
                .map(|code| {
                    f64::from(measure.width((code as char).encode_utf8(&mut [0; 4]), face, body))
                })
                .collect();
            figure::advance_table(f64::from(body), &|c| {
                widths[(c as u8 - figure::ADVANCE_FIRST) as usize]
            })
        };
        let (advances, bold) = if kind == FigureKind::Mermaid {
            (table(Face::Regular), table(Face::Bold))
        } else {
            (Vec::new(), Vec::new())
        };
        let palette = &self.palette;
        Theme {
            bg: hex(palette.background),
            fg: hex(palette.foreground),
            accent: hex(palette.accent_frame),
            font: self.faces.family.clone(),
            body_px: body.round() as u32,
            ex_px: figure::ex_px(f64::from(x_height)),
            advances,
            bold_advances: bold,
            muted: hex(palette.figure_muted),
            surface: "none".into(),
        }
    }

    /// A parsed document with figures starts the helper at once, so it is warm when the first is asked.
    pub(super) fn warm(&mut self, cx: &mut Context<Self>) {
        let figures: Vec<(FigureKind, String)> = self
            .doc
            .parsed
            .asks
            .figures
            .iter()
            .map(|(kind, source, _)| (*kind, source.clone()))
            .collect();
        if self.preparing || figures.is_empty() || self.shown_view() != super::View::Rendered {
            return;
        }
        let (math, mermaid) = (
            self.figure_theme(FigureKind::Math),
            self.figure_theme(FigureKind::Mermaid),
        );
        let themes = move |kind: FigureKind| {
            Some(if kind == FigureKind::Mermaid {
                mermaid.clone()
            } else {
                math.clone()
            })
        };
        self.figures
            .update(cx, |host, cx| host.warm(&figures, &themes, cx));
    }

    pub(super) fn note_pictures(&mut self, i: usize, geometry: &Geometry) {
        let mut urls = Vec::new();
        picture_urls(&geometry.items, &mut urls);
        let dir = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        for url in urls {
            let blocks = self.doc.picture_blocks.entry(url.clone()).or_default();
            if !blocks.contains(&i) {
                blocks.push(i);
            }
            if self.doc.pictures.contains_key(&url) {
                continue;
            }
            let path = path_of_url(&url);
            // Only a file inside the document's folder is read; the parser already refused the rest.
            if path.is_empty() || !inside_dir(&path, &dir) {
                self.doc.pictures.insert(url, PictureState::Failed);
                continue;
            }
            if self.preparing
                && self.doc.pictures.len()
                    >= flea::ui_logic_pv2::markdown::markdown_prepared::PICTURE_LIMIT
            {
                continue;
            }
            self.doc.pictures.insert(url.clone(), PictureState::Asked);
            if self.preparing {
                self.decoder.picture_limited(
                    self.doc.seq,
                    url,
                    PathBuf::from(path),
                    self.scale,
                    flea::ui_logic_pv2::markdown::markdown_prepared::PICTURE_MAX_BYTES,
                );
            } else {
                self.decoder
                    .picture(self.doc.seq, url, PathBuf::from(path), self.scale);
            }
        }
    }

    // The figures, formulas and fence colours one shown block asks for, each once.
    pub(super) fn ask_for(&mut self, i: usize, cx: &mut Context<Self>) {
        if !self.preparing {
            for ordinal in self.doc.ordinals(i, |c| c.figure) {
                let asked = self.doc.figure_tickets.values().any(|o| *o == ordinal)
                    || self.doc.assets.figures.contains_key(&ordinal);
                let Some((kind, source, _)) = self.doc.parsed.asks.figures.get(ordinal).cloned()
                else {
                    continue;
                };
                if asked {
                    continue;
                }
                let theme = self.figure_theme(kind);
                let ticket = self
                    .figures
                    .update(cx, |host, cx| host.ask(kind, &source, true, &theme, cx));
                self.doc.figure_tickets.insert(ticket, ordinal);
            }
        }
        for ordinal in self.doc.ordinals(i, |c| c.formula) {
            let Some(source) = self.doc.parsed.asks.formulas.get(ordinal).cloned() else {
                continue;
            };
            if self.preparing
                && !self.doc.formula_sources.contains_key(&source)
                && self.doc.formula_sources.len()
                    >= flea::ui_logic_pv2::markdown::markdown_prepared::FORMULA_LIMIT
            {
                continue;
            }
            if let Some(ticket) = self.doc.formula_sources.get(&source).copied() {
                let ordinals = self.doc.formula_tickets.entry(ticket).or_default();
                if !ordinals.contains(&ordinal)
                    && !self.doc.formulas_settled.contains(&ordinal)
                    && !self.doc.assets.formulas.contains_key(&ordinal)
                {
                    ordinals.push(ordinal);
                }
                continue;
            }
            let theme = self.figure_theme(FigureKind::Math);
            let ticket = self.figures.update(cx, |host, cx| {
                host.ask(FigureKind::Math, &source, false, &theme, cx)
            });
            self.doc.formula_sources.insert(source, ticket);
            self.doc
                .formula_tickets
                .entry(ticket)
                .or_default()
                .push(ordinal);
        }
        if self.preparing {
            return;
        }
        for ordinal in self.doc.ordinals(i, |c| c.fence) {
            self.ask_fence(ordinal);
        }
    }

    fn ask_fence(&mut self, ordinal: usize) {
        let Some(code) = &self.code else { return };
        let Some((text, info)) = self.doc.parsed.asks.fences.get(ordinal) else {
            return;
        };
        if info.trim().is_empty() {
            return;
        }
        let total = text.split('\n').count();
        let fence = self.doc.fences.entry(ordinal).or_insert(FenceAsk {
            next: 1,
            ..FenceAsk::default()
        });
        if fence.done || fence.token != 0 || fence.next > total {
            fence.done |= fence.next > total;
            return;
        }
        let source = CodeSource::Text {
            text: text.as_str().into(),
            name: String::new(),
            lang: info.trim().to_string(),
        };
        let slot = format!("{}:fence:{ordinal}", self.slot);
        fence.token = code.ask_code(
            &slot,
            source,
            fence.next,
            CODE_LINE_CAP.min(total - fence.next + 1),
        );
    }

    /// The backend's code reply, for a fence or for Source.
    pub fn code_answered(&mut self, answer: CodeAnswer, cx: &mut Context<Self>) {
        let fence = self
            .doc
            .fences
            .iter()
            .find(|(_, f)| f.token != 0 && f.token == answer.token)
            .map(|(o, _)| *o);
        let Some(ordinal) = fence else {
            if self.source.answered(&answer) {
                self.source_pump(cx);
                self.release_hold(cx);
                cx.notify();
            }
            return;
        };
        let state = self.doc.fences.get_mut(&ordinal).expect("found above");
        state.token = 0;
        if !matches!(answer.state, CodeState::Styled | CodeState::Plain) || answer.lines.is_empty()
        {
            state.done = true;
        } else {
            state.next += answer.lines.len();
            state.lines.extend(answer.lines);
            let lines = Arc::new(state.lines.clone());
            let block = self.doc.block_of(ordinal, |c| c.fence);
            self.doc.touch(block.as_slice()).code.insert(ordinal, lines);
            self.ask_fence(ordinal);
        }
        self.pump(cx);
        cx.notify();
    }

    pub(super) fn figure_done(&mut self, done: &FigureDone, cx: &mut Context<Self>) {
        let seq = self.doc.seq;
        if let Some(ordinal) = self.doc.figure_tickets.remove(&done.ticket) {
            if done.svg.is_empty() {
                let block = self.doc.block_of(ordinal, |c| c.figure);
                self.doc
                    .touch(block.as_slice())
                    .figures
                    .insert(ordinal, FigureState::Failed);
            } else {
                self.decoder
                    .svg(seq, Art::Figure(ordinal), Arc::clone(&done.svg), self.scale);
                self.doc.figure_tickets.insert(done.ticket, ordinal);
            }
        } else if let Some(ordinals) = self.doc.formula_tickets.get(&done.ticket).cloned() {
            let placed = (!done.svg.is_empty())
                .then(|| formula_box(&done.svg))
                .flatten();
            for ordinal in ordinals {
                match placed {
                    Some((w, above, below)) => {
                        self.doc
                            .formula_boxes
                            .insert(ordinal, (w as f32, above as f32, below as f32));
                        self.decoder.svg(
                            seq,
                            Art::Formula(ordinal),
                            Arc::clone(&done.svg),
                            self.scale,
                        );
                    }
                    None => {
                        self.doc.formulas_settled.insert(ordinal);
                    }
                }
            }
            if placed.is_none() {
                self.doc.formula_tickets.remove(&done.ticket);
            }
        } else {
            return;
        }
        self.pump(cx);
        cx.notify();
    }

    pub(super) fn decoded(&mut self, done: Decoded, cx: &mut Context<Self>) {
        match done {
            Decoded::Picture { seq, url, image } if seq == self.doc.seq => {
                let blocks = self
                    .doc
                    .picture_blocks
                    .get(&url)
                    .cloned()
                    .unwrap_or_default();
                match image {
                    Some((image, natural)) => {
                        self.doc
                            .pictures
                            .insert(url.clone(), PictureState::Drawn(image));
                        self.doc.touch(&blocks).pictures.insert(url, natural);
                    }
                    None => {
                        self.doc.pictures.insert(url, PictureState::Failed);
                    }
                }
            }
            Decoded::Svg { seq, art, image } if seq == self.doc.seq => match (art, image) {
                (Art::Figure(ordinal), image) => {
                    self.doc.figure_tickets.retain(|_, o| *o != ordinal);
                    let block = self.doc.block_of(ordinal, |c| c.figure);
                    let state = match image {
                        Some((image, (width, height))) => {
                            self.doc.art.insert(art, image);
                            FigureState::Drawn { width, height }
                        }
                        None => FigureState::Failed,
                    };
                    self.doc
                        .touch(block.as_slice())
                        .figures
                        .insert(ordinal, state);
                }
                (Art::Formula(ordinal), image) => {
                    self.doc.formulas_settled.insert(ordinal);
                    for ordinals in self.doc.formula_tickets.values_mut() {
                        ordinals.retain(|o| *o != ordinal);
                    }
                    let boxed = self.doc.formula_boxes.get(&ordinal).copied();
                    if let (Some(image), Some((width, above, below))) = (image, boxed) {
                        self.doc.art.insert(art, image.0);
                        let block = self.doc.block_of(ordinal, |c| c.formula);
                        self.doc.touch(block.as_slice()).formulas.insert(
                            ordinal,
                            FormulaBox {
                                width,
                                above,
                                below,
                            },
                        );
                    }
                }
            },
            Decoded::Picture {
                image: Some((image, _)),
                ..
            }
            | Decoded::Svg {
                image: Some((image, _)),
                ..
            } => {
                self.doc.released.push(image);
                return;
            }
            _ => return,
        }
        self.pump(cx);
        cx.notify();
    }
}
