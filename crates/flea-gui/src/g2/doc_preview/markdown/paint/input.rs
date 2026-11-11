//! The frame's input: the wheel and keys scroll the document, or a wide table under the wheel, and a click opens an allowed link.
use super::super::{MarkdownView, View};
use super::{inside, Hit, Pointer};
use flea::ui_logic_doc_preview::markdown::{
    markdown_keys::{across, sideways},
    markdown_layout::Metrics,
    markdown_resolve::is_external_link,
};
use gpui::{Context, MouseUpEvent, Pixels, ScrollWheelEvent, TouchPhase};

impl MarkdownView {
    fn local(&self, at: gpui::Point<Pixels>) -> (f32, f32) {
        (
            f32::from(at.x) - self.bounds.0,
            f32::from(at.y) - self.bounds.1,
        )
    }

    pub(super) fn wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = crate::input::scroll::wheel_delta(&event.delta);
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        if self.shown_view() == View::Source {
            self.source.scroll_by(-dy);
            self.source_pump(cx);
            cx.notify();
            return;
        }
        let at = self.local(event.position);
        let table = self.hits.iter().find_map(|hit| match hit {
            Hit::Table { rect, id, most, .. } if *most > 0. && inside(*rect, at) => {
                Some((*id, *most))
            }
            _ => None,
        });
        let (shift, control) = (event.modifiers.shift, event.modifiers.control);
        let touchpad = matches!(event.delta, gpui::ScrollDelta::Pixels(_));
        let taker = if !touchpad {
            table.filter(|_| sideways(dx, dy, shift, control))
        } else {
            match event.touch_phase {
                TouchPhase::Started => {
                    self.pointer = Pointer::default();
                    None
                }
                phase => {
                    if self.pointer.held.is_none()
                        && !self.pointer.document
                        && phase != TouchPhase::Ended
                    {
                        match table.filter(|_| sideways(dx, dy, shift, control)) {
                            Some(found) => self.pointer.held = Some(found),
                            None => self.pointer.document = dx != 0. || dy != 0.,
                        }
                    }
                    let held = self.pointer.held;
                    if phase == TouchPhase::Ended {
                        self.pointer = Pointer::default();
                    }
                    held
                }
            }
        };
        match taker {
            Some((id, most)) => {
                let x = self.tables.entry(id).or_insert(0.);
                *x = (*x - across(dx, dy, shift)).clamp(0., most);
                cx.notify();
            }
            None => self.scroll_to(self.scroll - dy, cx),
        }
    }

    pub(super) fn click(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let at = self.local(event.position);
        self.open_link_at(at, cx);
    }

    /// Opens the link under a point of the frame in the default application, true when one opened.
    /// Only a link that names an allowed scheme opens.
    pub fn open_link_at(&mut self, at: (f32, f32), cx: &mut Context<Self>) -> bool {
        let url = self.hits.iter().find_map(|hit| match hit {
            Hit::Link { rect, url } if inside(*rect, at) => Some(url.clone()),
            _ => None,
        });
        match url.filter(|url| is_external_link(url)) {
            Some(url) => {
                cx.open_url(&url);
                true
            }
            None => false,
        }
    }

    /// The last frame's links: each one's rect in the frame and its address.
    pub fn links(&self) -> Vec<((f32, f32, f32, f32), String)> {
        self.hits
            .iter()
            .filter_map(|hit| match hit {
                Hit::Link { rect, url } => Some((*rect, url.clone())),
                Hit::Table { .. } => None,
            })
            .collect()
    }

    /// IpcPreviewState.previewTable: the first sideways table's shift, its rect and its first body row's, in window pixels, or `{}`.
    pub fn table_state(&self) -> String {
        use flea::jsondoc::Json;
        let (ox, oy) = self.bounds;
        let rect = |(x, y, w, h): (f32, f32, f32, f32)| {
            let r = |n: f32| n.round();
            Json::Str(format!("{} {} {} {}", r(ox + x), r(oy + y), r(w), r(h)))
        };
        let table = (self.shown_view() == View::Rendered)
            .then(|| {
                self.hits.iter().find_map(|hit| match hit {
                    Hit::Table {
                        rect,
                        id,
                        most,
                        row,
                    } if *most > 0. => Some((rect, id, most, row)),
                    _ => None,
                })
            })
            .flatten();
        let fields = table.map_or_else(Vec::new, |(view, id, most, row)| {
            let shift = self.tables.get(id).copied().unwrap_or(0.).clamp(0., *most);
            vec![
                ("scrollX".into(), Json::Num(shift.round().to_string())),
                ("view".into(), rect(*view)),
                ("row".into(), row.map_or(Json::Str(String::new()), rect)),
            ]
        });
        flea::jsondoc::render(&Json::Obj(fields))
            .trim_end()
            .to_owned()
    }

    fn scroll_to(&mut self, top: f32, cx: &mut Context<Self>) {
        let most = (self.content_height() - self.frame.1).max(0.);
        let top = top.clamp(0., most);
        if top != self.scroll {
            self.scroll = top;
            self.pump(cx);
            cx.notify();
        }
    }

    pub(in crate::g2::doc_preview::markdown) fn scroll_rows(
        &mut self,
        rows: i64,
        cx: &mut Context<Self>,
    ) {
        self.scroll_by(rows as f32 * Metrics::line(self.tokens.body), cx);
    }

    /// Moves the shown view, rendered or Source, by `pixels`, never past either end.
    pub fn scroll_by(&mut self, pixels: f32, cx: &mut Context<Self>) {
        if self.shown_view() == View::Source {
            self.source.scroll_by(pixels);
            self.source_pump(cx);
            cx.notify();
        } else {
            self.scroll_to(self.scroll + pixels, cx);
        }
    }
}
