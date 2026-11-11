//! The backend answers the open Quick Look card asked for: the code pages, the picture's meta and the Compare patch.
use super::{code_line, keys, Ask};
use crate::{
    backend::Event as BackendEvent,
    g2::quick_look_overlay::{Body, Subject},
    state::Shell,
};
use flea::{
    jsondoc::{self, Json},
    ui_logic_preview_core::{
        compare,
        kinds::{self, State},
        meta::{self, CompareReply, CompareState, Meta},
        rect::Shape,
    },
};
use gpui::{Context, Window};
use std::path::PathBuf;

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
    // Sample input: {"t":"code","token":7,"slot":"quicklook","state":"plain","first":1,"total":2,"lines":[[0,"a"],[]]}; a meta, compare or menuaction line carries the ask's token the same way (menuaction as "id").
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
            let picture = view.subject.as_ref().is_some_and(|subject| {
                matches!(
                    subject.body,
                    Body::Image { .. } | Body::Link { .. } | Body::Raw { .. }
                )
            });
            if view.is_revealed() || picture {
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

    /// The picture's own size from its header, landed while the meta reply is still out (EY-521).
    pub(super) fn quick_look_header(
        &mut self,
        token: u64,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = path else { return };
        let read = cx
            .background_executor()
            .spawn(async move { flea::backend::imagesize::header(&path) });
        cx.spawn(async move |this, cx| {
            let Some(found) = read.await else { return };
            let _ =
                this.update(cx, |shell, cx| {
                    if !shell.quick_look.as_ref().is_some_and(|host| {
                        host.token == token && matches!(host.ask, Ask::Meta { .. })
                    }) {
                        return;
                    }
                    let (w, h) = keys::upright(found.width, found.height, found.orientation.into());
                    if w <= 0. || h <= 0. {
                        return;
                    }
                    shell.quick_look_land(cx, |subject| {
                        if let Body::Image { w: sw, h: sh, .. } = &mut subject.body {
                            if *sw <= 0. {
                                (*sw, *sh) = (w, h);
                                subject.shape = Shape {
                                    kind: State::Image,
                                    known: true,
                                    w: found.width.into(),
                                    h: found.height.into(),
                                    orient: found.orientation.max(1).into(),
                                };
                            }
                        }
                    });
                });
        })
        .detach();
    }
}

/// PreviewImage.qml draws the picture as soon as it decodes, so the card reads the file's own header beside a meta ask.
pub(super) fn header_path(ask: &Ask, body: &Body) -> Option<PathBuf> {
    match (ask, body) {
        (
            Ask::Meta {
                link: None,
                path: asked,
            },
            Body::Image { path, .. },
        ) if !kinds::is_raw(asked) => Some(path.clone()),
        _ => None,
    }
}
