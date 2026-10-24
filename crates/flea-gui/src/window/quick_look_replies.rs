//! The backend answers the open Quick Look card asked for: the code pages, the picture's meta and the Compare patch.
use super::{code_line, keys, Ask};
use crate::{backend::Event as BackendEvent, g2::pv1::Body, state::Shell};
use flea::ui_logic_pv1::{
    compare,
    kinds::State,
    meta::{self, CompareState},
    rect::Shape,
};
use gpui::Context;

impl Shell {
    /// A backend line the open card asked for; false for every other line.
    pub(crate) fn quick_look_line(
        &mut self,
        side: usize,
        event: &BackendEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if matches!(event, BackendEvent::Failed(_) | BackendEvent::Drained) {
            self.quick_look_lost(side, cx);
            return false;
        }
        let BackendEvent::Producer(line) = event else {
            return false;
        };
        let Some(host) = self
            .quick_look
            .as_ref()
            .filter(|h| h.side == side && h.token != 0)
        else {
            return false;
        };
        let token = host.token;
        let catalog = self.locale.catalog.clone();
        let Some(mut subject) = host.view.read(cx).subject().cloned() else {
            return false;
        };
        match host.ask.clone() {
            Ask::Compare { .. } => {
                let Some(reply) = meta::parse_compare(line).filter(|r| r.token as u64 == token)
                else {
                    return false;
                };
                let (a, b) = match &host.ask {
                    Ask::Compare { a, b } => (a.clone(), b.clone()),
                    _ => unreachable!(),
                };
                let (name_a, name_b) = (compare::leaf(&a), compare::leaf(&b));
                let answer = match &reply.state {
                    CompareState::Diff(_) => "diff",
                    CompareState::Identical => "identical",
                    CompareState::Binary => "binary",
                    CompareState::Over => "over",
                    CompareState::Failed => "failed",
                };
                subject.body = match &reply.state {
                    CompareState::Diff(patch) => {
                        let rows = compare::rows(patch);
                        let title =
                            compare::title(name_a, name_b, Some(compare::changed(&rows)), &catalog);
                        Body::Diff {
                            title,
                            copy: catalog.tr("preview.compare.copy_patch"),
                            rows,
                        }
                    }
                    state => Body::Refused {
                        title: compare::title(name_a, name_b, None, &catalog),
                        sentence: compare::sentence(state, &catalog),
                    },
                };
                let host = self.quick_look.as_mut().expect("read above");
                host.answer = answer.into();
                host.patch = match reply.state {
                    CompareState::Diff(patch) => patch,
                    _ => String::new(),
                };
                host.token = 0;
                host.ask = Ask::Nothing;
            }
            Ask::Code { path, mut lines } => {
                let Ok(doc) = flea::jsondoc::parse(line) else {
                    return false;
                };
                let text = |key| {
                    doc.get(key)
                        .and_then(flea::jsondoc::Json::as_str)
                        .unwrap_or("")
                };
                let number = |key| {
                    doc.get(key)
                        .and_then(flea::jsondoc::Json::as_f64)
                        .unwrap_or(-1.)
                };
                if text("t") != "code"
                    || text("slot") != "quicklook"
                    || number("token") as u64 != token
                {
                    return false;
                }
                let total = number("total").max(0.) as usize;
                let failed = !matches!(text("state"), "plain" | "styled");
                if failed {
                    let (state, too_large) = match text("state") {
                        "large" => (State::Text, true),
                        "binary" => (State::Unsupported, false),
                        _ => (State::Error, false),
                    };
                    subject.body = Body::cannot_show(&catalog, state, too_large, false, Vec::new())
                        .expect("a refused read has a sentence");
                    let host = self.quick_look.as_mut().expect("read above");
                    host.token = 0;
                    host.ask = Ask::Nothing;
                } else {
                    lines.extend(keys::plain_lines(&doc));
                    let next = (lines.len() < total).then(super::token);
                    subject.body = Body::Text {
                        lines: lines.clone(),
                        count: catalog.trn("preview.lines", total as i64),
                    };
                    let first = lines.len() + 1;
                    let host = self.quick_look.as_mut().expect("read above");
                    match next {
                        Some(token) => {
                            host.token = token;
                            host.ask = Ask::Code {
                                path: path.clone(),
                                lines,
                            };
                            self.quick_look_send(side, code_line(token, &path, first));
                        }
                        None => {
                            host.token = 0;
                            host.ask = Ask::Nothing;
                        }
                    }
                }
            }
            Ask::Meta { path, link } => {
                let Some(meta) = meta::parse(line).filter(|m| m.token == Some(token as usize))
                else {
                    return false;
                };
                subject.body = keys::picture(&meta, path, link.as_deref(), &catalog);
                subject.shape = Shape {
                    kind: State::Image,
                    known: meta.w > 0 && meta.h > 0,
                    w: meta.w.into(),
                    h: meta.h.into(),
                    orient: meta.orient.max(1),
                };
                let host = self.quick_look.as_mut().expect("read above");
                host.token = 0;
                host.ask = Ask::Nothing;
            }
            Ask::Nothing => return false,
        }
        let view = self.quick_look.as_ref().expect("read above").view.clone();
        // A later page keeps the card's scroll and focus; the first whole answer reveals it.
        view.update(cx, |view, cx| {
            view.subject = Some(subject);
            if view.is_revealed() {
                cx.notify();
            } else {
                view.reveal(cx);
            }
        });
        cx.notify();
        true
    }
}

impl Shell {
    /// The pane's backend went away with an answer still owed: the card says the file could not be read.
    fn quick_look_lost(&mut self, side: usize, cx: &mut Context<Self>) {
        let Some(host) = self
            .quick_look
            .as_mut()
            .filter(|h| h.side == side && h.token != 0)
        else {
            return;
        };
        host.token = 0;
        host.ask = Ask::Nothing;
        let catalog = self.locale.catalog.clone();
        host.view.update(cx, |view, cx| {
            if let Some(subject) = view.subject.as_mut() {
                if let Some(body) =
                    Body::cannot_show(&catalog, State::Error, false, true, Vec::new())
                {
                    subject.body = body;
                }
            }
            view.reveal(cx);
        });
    }
}
