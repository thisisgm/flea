//! Storage-bounded column reads: remote code is sent as text, never as a backend path.
use super::{
    docs::{Doc, Look},
    wire::Wire,
    *,
};
use crate::g2::doc_preview::code::CodeView;
use gpui::AppContext;
use std::io::Read as _;

enum Read {
    Text(String),
    Large,
    Failed,
}

fn read(path: &str, limit: u64) -> Read {
    let Some(file) = flea::backend::regfile::open_regular(std::path::Path::new(path)) else {
        return Read::Failed;
    };
    if file.metadata().is_ok_and(|meta| meta.len() > limit) {
        return Read::Large;
    }
    let mut bytes = Vec::new();
    if file.take(limit + 1).read_to_end(&mut bytes).is_err() {
        return Read::Failed;
    }
    if bytes.len() as u64 > limit {
        return Read::Large;
    }
    Read::Text(flea::textdecode::decode(&bytes).into_owned())
}

pub(super) fn code(
    kind: Kind,
    subject: &Subject,
    look: &Look,
    wire: &Wire,
    catalog: &Arc<flea::i18n::I18n>,
    cx: &mut Context<SelectionPreview>,
) -> Entity<CodeView> {
    let path = subject.path();
    let size = subject.row.size;
    let view = cx.new(|cx| {
        let mut view = look.code(kind == Kind::Code);
        view.max_bytes = look.text_limit;
        view.set_host(Some(Box::new(wire.clone())), cx);
        view.set_catalog(catalog.clone(), cx);
        view.set_active(true, cx);
        view.set_subject(&path, size, look.remote, None, cx);
        view
    });
    if look.remote && size <= look.text_limit {
        let limit = look.text_limit;
        let read_path = path.clone();
        let read = cx.background_spawn(async move { read(&read_path, limit) });
        view.update(cx, |_, cx| {
            cx.spawn(async move |this, cx| {
                let result = read.await;
                let _ = this.update(cx, |view, cx| match result {
                    Read::Text(text) => view.set_subject(&path, size, true, Some(text), cx),
                    Read::Large => view.set_subject(&path, limit + 1, true, None, cx),
                    Read::Failed => view.set_read_failed(cx),
                });
            })
        })
        .detach();
    }
    view
}

pub(super) fn read_document(
    doc: &Doc,
    path: &str,
    size: u64,
    limit: u64,
    cx: &mut Context<SelectionPreview>,
) {
    // Markdown owns its bounded read; code was started by code(), and other kinds read no text here.
    if !matches!(doc, Doc::Table(_) | Doc::Html(_)) {
        return;
    }
    let path = path.to_owned();
    let read_path = path.clone();
    let read = cx.background_spawn(async move { read(&read_path, limit) });
    match doc {
        Doc::Table(view) => {
            view.update(cx, |_, cx| {
                cx.spawn(async move |this, cx| {
                    let result = read.await;
                    let _ = this.update(cx, |view, cx| {
                        use flea::ui_logic_doc_preview::csv_feed::Answer;
                        match result {
                            Read::Text(text) => view.set_text(&path, &text, cx),
                            Read::Large => view.feed.answer = Answer::Large,
                            Read::Failed => view.feed.answer = Answer::Failed,
                        }
                        cx.notify();
                    });
                })
            })
            .detach();
        }
        Doc::Html(view) => {
            let generation = view.update(cx, |view, cx| view.set_path(&path, size, true, cx));
            view.update(cx, |_, cx| {
                cx.spawn(async move |this, cx| {
                    let result = read.await;
                    let _ = this.update(cx, |view, cx| match result {
                        Read::Text(text) => view.set_source_text(generation, Some(text), false, cx),
                        Read::Large => {
                            view.set_path(&path, limit + 1, true, cx);
                        }
                        Read::Failed => view.set_source_text(generation, None, true, cx),
                    });
                })
            })
            .detach();
        }
        _ => {}
    }
}
