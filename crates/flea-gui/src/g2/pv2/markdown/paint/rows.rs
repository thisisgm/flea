//! Source's frame: the shown rows with their numbers and colours, or a refusal's mark and caption.
use super::super::source::Answer;
use super::super::{FirstFrame, MarkdownView, Status, View};
use super::build::centred;
use super::{Build, Op};
use flea::ui_logic_pv2::markdown::markdown_flow::{elide, Face, Ink, Measure};
use gpui::{SharedString, Window};

/// The refusal mark's dimming.
const MARK_DIM: f32 = 0.7;
// LanguageMarks' Markdown mark in the Omarchy cut: the frame, the M and the down arrow.
const MARKDOWN_MARK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M2 5h20v14H2z M5.5 15V9l3 3.5 3-3.5v6 M17 9v6 M14.5 12.5 17 15l2.5-2.5"/></svg>"#;

impl MarkdownView {
    // Source's rows, or its refusal: a binary or too large file's mark and caption, an unreadable one's sentence.
    pub(super) fn prepare_source(&mut self, window: &Window) -> Option<FirstFrame> {
        let t = self.tokens;
        let (w, h) = self.frame;
        let view = self.shown_view();
        let refused = match (view, self.doc.status) {
            (View::Source, _) => self.source_refused(),
            (_, Status::TooLarge) => Some(Answer::Large),
            (_, Status::Unreadable) => Some(Answer::Failed),
            _ => None,
        };
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
            hairline: t.hairline,
            height: h,
            block: 0,
            table: 0,
            first: FirstFrame::default(),
            window,
        };
        let muted = self.palette.muted;
        match refused {
            Some(Answer::Failed) | Some(Answer::Large) if view == View::Rendered => {
                let sentence = if refused == Some(Answer::Large) {
                    &self.words.too_large
                } else {
                    &self.words.unreadable
                };
                centred(&mut build, sentence, t.body, (w, h), muted);
                return Some(build.first);
            }
            Some(Answer::Failed) => {
                centred(&mut build, &self.words.unreadable, t.body, (w, h), muted);
                return Some(build.first);
            }
            Some(answer) => {
                let caption = if answer == Answer::Binary {
                    &self.words.binary
                } else {
                    &self.words.large
                };
                let run = build
                    .glyphs
                    .get(caption, Face::Regular, t.caption, build.faces, window);
                let line = run.line();
                let text_h = f32::from(line.ascent + line.descent);
                let column = t.state_mark + t.gap + text_h;
                let y = ((h - column) / 2.).round();
                let mark = ((w - t.state_mark) / 2., y, t.state_mark, t.state_mark);
                let key = SharedString::new_static("flea-mark:markdown");
                build.ops.push(Op::Mark {
                    rect: mark,
                    key,
                    data: MARKDOWN_MARK,
                    color: muted.opacity(MARK_DIM),
                });
                let x = ((w - f32::from(line.width)) / 2.).round();
                let baseline = y + t.state_mark + t.gap + f32::from(line.ascent);
                build.ops.push(Op::Text {
                    selected: None,
                    x,
                    baseline,
                    run,
                    color: muted,
                });
                return Some(build.first);
            }
            None => {}
        }
        if !self.doc.shown {
            self.counts.held_frames += usize::from(self.doc.status != Status::Idle);
            return None;
        }
        let src = &mut self.source;
        let f = src.frame;
        let (x0, y0) = (t.inset_x(), t.source_inset_y());
        let clip = (x0, y0, f.width, f.height);
        let at = build.ops.len();
        build.ops.push(Op::Clip { rect: clip, len: 0 });
        let (ascent, descent) = src.measure().extent(Face::Regular, f.body);
        let (cap_ascent, cap_descent) = src.measure().extent(Face::Regular, f.caption);
        let body_base = (f.line - ascent - descent) / 2. + ascent;
        let number_base = (f.line - cap_ascent - cap_descent) / 2. + cap_ascent - 1.;
        let (gutter, text_x) = (src.gutter(), src.gutter() + 2. * f.advance);
        let mut y = y0 - src.offset;
        let mut row = src.top;
        let mut chars = 0;
        while row < src.rows() && y < y0 + f.height {
            match src.line_of(row) {
                None => {
                    let notice = src.notice().unwrap_or_default().to_string();
                    let text = elide(&notice, f.width, Face::Regular, f.caption, src.measure());
                    build.text(&text, Face::Regular, f.caption, x0, y + cap_ascent, muted);
                }
                Some(n) => {
                    let number = n.to_string();
                    let run =
                        build
                            .glyphs
                            .get(&number, Face::Regular, f.caption, build.faces, window);
                    let x = x0 + gutter - f32::from(run.line().width);
                    build.ops.push(Op::Text {
                        selected: None,
                        x,
                        baseline: y + number_base,
                        run,
                        color: muted,
                    });
                    let lines = src.lines(n).unwrap_or_default();
                    if lines.iter().all(Vec::is_empty) {
                        let run = build
                            .glyphs
                            .get("", Face::Regular, f.body, build.faces, window);
                        super::super::selection::span(
                            build.selection,
                            n - 1,
                            0,
                            x0 + text_x,
                            y + ascent,
                            &run,
                        );
                    }
                    for (k, line) in lines.iter().enumerate() {
                        for piece in line {
                            chars += piece.text.chars().count();
                            let color = build.palette.ink(&Ink::Code(piece.role));
                            let baseline = y + k as f32 * f.line + body_base;
                            build.text(
                                &piece.text,
                                Face::Regular,
                                f.body,
                                x0 + text_x + piece.x,
                                baseline,
                                color,
                            );
                            if piece.len > 0 {
                                build.select_last(n - 1, piece.start);
                                if let Some(span) = build.selection.spans.last_mut() {
                                    span.len = piece.len;
                                }
                            }
                        }
                    }
                }
            }
            y += src.height(row);
            row += 1;
        }
        src.laid_chars = chars;
        let len = build.ops.len() - at - 1;
        build.ops[at] = Op::Clip { rect: clip, len };
        Some(build.first)
    }
}
