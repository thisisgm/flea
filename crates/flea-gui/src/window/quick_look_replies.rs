//! The backend answers the open Quick Look card asked for: the code pages, the picture's meta and the Compare patch.
use super::{code_line, keys, Ask};
use crate::{
    backend::Event as BackendEvent,
    g2::pv1::{Body, Subject},
    state::Shell,
};
use flea::{
    jsondoc::{self, Json},
    ui_logic_pv1::{
        compare,
        kinds::State,
        meta::{self, CompareReply, CompareState, Meta},
        rect::Shape,
    },
};
use gpui::{Context, Window};

/// The one line that answers the ask, matched before the card's subject is read.
enum Reply {
    Compare(CompareReply),
    Code(Json),
    Meta(Box<Meta>),
    Menu(Json),
}

fn text<'a>(doc: &'a Json, key: &str) -> &'a str {
    doc.get(key).and_then(Json::as_str).unwrap_or("")
}

fn number(doc: &Json, key: &str) -> f64 {
    doc.get(key).and_then(Json::as_f64).unwrap_or(-1.)
}

impl Shell {
    /// A backend line the open card asked for; false for every other line.
    pub(crate) fn quick_look_line(
        &mut self,
        side: usize,
        event: &BackendEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if matches!(event, BackendEvent::Failed(_) | BackendEvent::Drained) {
            self.quick_look_code_lost(side, cx);
            self.quick_look_lost(side, cx);
            return false;
        }
        if self.quick_look_doc_line(side, event, window, cx) {
            return true;
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
        // Every other line is left for the pane before anything is read or copied.
        let reply = match &host.ask {
            Ask::Nothing => return false,
            Ask::Compare { .. } => {
                match meta::parse_compare(line).filter(|r| r.token as u64 == token) {
                    Some(reply) => Reply::Compare(reply),
                    None => return false,
                }
            }
            Ask::Meta { .. } => match meta::parse(line).filter(|m| m.token == Some(token as usize))
            {
                Some(meta) => Reply::Meta(Box::new(meta)),
                None => return false,
            },
            Ask::Code { .. } => match jsondoc::parse(line) {
                Ok(doc)
                    if text(&doc, "t") == "code"
                        && text(&doc, "slot") == "quicklook"
                        && number(&doc, "token") as u64 == token =>
                {
                    Reply::Code(doc)
                }
                _ => return false,
            },
            Ask::Resolve { .. } => match jsondoc::parse(line) {
                Ok(doc)
                    if text(&doc, "t") == "menuaction" && number(&doc, "id") as u64 == token =>
                {
                    Reply::Menu(doc)
                }
                _ => return false,
            },
        };
        let host = self.quick_look.as_mut().expect("read above");
        let ask = std::mem::replace(&mut host.ask, Ask::Nothing);
        host.token = 0;
        let catalog = self.locale.catalog.clone();
        match (ask, reply) {
            (
                Ask::Resolve {
                    listing,
                    rows,
                    mark,
                    kinds,
                },
                Reply::Menu(doc),
            ) => {
                let ask = (listing, rows, mark, kinds);
                self.compare_resolved(side, token, ask, &doc, window, cx);
                return true;
            }
            (Ask::Compare { a, b }, Reply::Compare(reply)) => {
                let (name_a, name_b) = (compare::leaf(&a), compare::leaf(&b));
                host.answer = match &reply.state {
                    CompareState::Diff(_) => "diff",
                    CompareState::Identical => "identical",
                    CompareState::Binary => "binary",
                    CompareState::Over => "over",
                    CompareState::Failed => "failed",
                }
                .into();
                let body = match reply.state {
                    CompareState::Diff(patch) => {
                        let rows = compare::rows(&patch);
                        let title =
                            compare::title(name_a, name_b, Some(compare::changed(&rows)), &catalog);
                        host.patch = patch;
                        Body::Diff {
                            title,
                            copy: catalog.tr("preview.compare.copy_patch"),
                            rows,
                        }
                    }
                    state => {
                        host.patch.clear();
                        Body::Refused {
                            title: compare::title(name_a, name_b, None, &catalog),
                            sentence: compare::sentence(&state, &catalog),
                        }
                    }
                };
                self.quick_look_land(cx, |subject| subject.body = body);
            }
            (Ask::Code { path }, Reply::Code(doc)) => {
                let total = number(&doc, "total").max(0.) as usize;
                if !matches!(text(&doc, "state"), "plain" | "styled") {
                    let (state, too_large) = match text(&doc, "state") {
                        "large" => (State::Text, true),
                        "binary" => (State::Unsupported, false),
                        _ => (State::Error, false),
                    };
                    let body = Body::cannot_show(&catalog, state, too_large, false, Vec::new())
                        .expect("a refused read has a sentence");
                    self.quick_look_land(cx, |subject| subject.body = body);
                    return true;
                }
                // Each page extends the card's own lines, so a long read never copies what it already holds.
                let count = catalog.trn("preview.lines", total as i64);
                let held = self.quick_look_land(cx, |subject| match &mut subject.body {
                    Body::Text {
                        lines,
                        count: shown,
                    } => {
                        lines.extend(keys::plain_lines(&doc));
                        *shown = count;
                        lines.len()
                    }
                    _ => total,
                });
                if held.is_some_and(|held| held < total) {
                    let next = super::token();
                    let host = self.quick_look.as_mut().expect("read above");
                    host.token = next;
                    let first = held.unwrap_or(0) + 1;
                    let line = code_line(next, &path, first);
                    host.ask = Ask::Code { path };
                    self.quick_look_send(side, line);
                }
            }
            (Ask::Meta { path, link }, Reply::Meta(meta)) => {
                let body = keys::picture(&meta, path, link.as_deref(), &catalog);
                let shape = Shape {
                    kind: State::Image,
                    known: meta.w > 0 && meta.h > 0,
                    w: meta.w.into(),
                    h: meta.h.into(),
                    orient: meta.orient.max(1),
                };
                self.quick_look_land(cx, |subject| {
                    subject.body = body;
                    subject.shape = shape;
                });
            }
            _ => unreachable!("each ask matches only its own reply"),
        }
        cx.notify();
        true
    }

    /// The answer changes the open card's subject in place; the first whole answer reveals it, a later page keeps its scroll and focus.
    fn quick_look_land<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Subject) -> R,
    ) -> Option<R> {
        let view = self.quick_look.as_ref()?.view.clone();
        view.update(cx, |view, cx| {
            let changed = view.subject.as_mut().map(change);
            if view.is_revealed() {
                cx.notify();
            } else {
                view.reveal(cx);
            }
            changed
        })
    }

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
        // A Compare still resolving its rows opened no card, so nothing is drawn for it.
        if matches!(
            std::mem::replace(&mut host.ask, Ask::Nothing),
            Ask::Resolve { .. }
        ) {
            return;
        }
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
