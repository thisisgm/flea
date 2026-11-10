//! Whether a file with no known type reads as text or a script, from its head read off the UI thread one file at a time, and only on a local mount.
pub(crate) use flea::ui_logic_preview_core::sniff::{read_head, Head};
use flea::{protocol::RowFacts, ui_logic_preview_core::kinds};
use gpui::App;
use std::{cell::RefCell, collections::HashMap};

/// Answers kept before the cache starts over.
const KEPT: usize = 256;

/// A file as listed: its path, size and modification time, so a changed file is read again.
pub(crate) type Key = (String, u64, i64);

type Done = Box<dyn FnOnce(&mut App)>;

#[derive(Default)]
struct Sniffer {
    answers: HashMap<Key, Head>,
    running: Option<(Key, Vec<Done>)>,
    waiting: Option<(Key, Vec<Done>)>,
}

thread_local! {
    static SNIFFER: RefCell<Sniffer> = RefCell::default();
}

/// Only a regular row the backend could name no type for, and never a font, which Quick Look opens itself.
pub(crate) fn candidate(row: &RowFacts) -> bool {
    !row.dir && row.kind == kinds::UNKNOWN_KIND && !kinds::is_font(&row.name.to_ascii_lowercase())
}

pub(crate) fn key(row: &RowFacts, path: &str) -> Key {
    (path.to_owned(), row.size, row.mtime)
}

/// A row with no known type: its key and its head's answer so far; None for any other row.
pub(crate) fn lookup(row: &RowFacts, path: &str) -> Option<(Key, Option<Head>)> {
    candidate(row).then(|| {
        let key = key(row, path);
        let head = answer(&key);
        (key, head)
    })
}

/// The answer for this file as listed, None until a read has given one.
pub(crate) fn answer(key: &Key) -> Option<Head> {
    SNIFFER.with(|sniffer| sniffer.borrow().answers.get(key).copied())
}

/// `done` runs on the UI thread after the background read, kept answer or not; never for an answer already kept or an ask a newer one replaced.
pub(crate) fn ask(key: Key, cx: &mut App, done: impl FnOnce(&mut App) + 'static) {
    let done: Done = Box::new(done);
    let start = SNIFFER.with(|sniffer| {
        let mut sniffer = sniffer.borrow_mut();
        let sniffer = &mut *sniffer;
        if sniffer.answers.contains_key(&key) {
            return None;
        }
        match (&mut sniffer.running, &mut sniffer.waiting) {
            (Some((running, list)), _) if *running == key => list.push(done),
            (Some(_), Some((waiting, list))) if *waiting == key => list.push(done),
            (Some(_), waiting) => *waiting = Some((key, vec![done])),
            (None, _) => {
                sniffer.running = Some((key.clone(), vec![done]));
                return Some(key);
            }
        }
        None
    });
    if let Some(key) = start {
        run(key, cx);
    }
}

fn run(key: Key, cx: &mut App) {
    let path = key.0.clone();
    let task = cx
        .background_executor()
        .spawn(async move { read_head(&path) });
    cx.spawn(async move |cx| {
        let text = task.await;
        cx.update(|cx| landed(key, text, cx));
    })
    .detach();
}

fn landed(key: Key, text: Option<Head>, cx: &mut App) {
    let (done, next) = SNIFFER.with(|sniffer| {
        let mut sniffer = sniffer.borrow_mut();
        if sniffer.answers.len() >= KEPT {
            sniffer.answers.clear();
        }
        // A read that failed keeps no answer, so the next ask for this file reads it again.
        if let Some(text) = text {
            sniffer.answers.insert(key, text);
        }
        let done = sniffer
            .running
            .take()
            .map(|(_, done)| done)
            .unwrap_or_default();
        sniffer.running = sniffer.waiting.take();
        (done, sniffer.running.as_ref().map(|(key, _)| key.clone()))
    });
    if let Some(next) = next {
        run(next, cx);
    }
    for done in done {
        done(cx);
    }
}

/// A preview showing a row with no known type draws again once its head is read, so it can mount it as text.
pub(crate) fn notify_when_read<T: 'static>(row: &RowFacts, path: &str, cx: &mut gpui::Context<T>) {
    let key = key(row, path);
    if candidate(row) && answer(&key).is_none() {
        let this = cx.entity().downgrade();
        ask(key, cx, move |cx| {
            let _ = this.update(cx, |_, cx| cx.notify());
        });
    }
}

impl crate::state::Shell {
    /// A head that reads as text reopens the card as text, only while the open's token says it still shows that row.
    pub(super) fn quick_look_sniff(
        &mut self,
        key: Key,
        token: u64,
        (side, index): (usize, usize),
        row: RowFacts,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let (this, handle, path) = (
            cx.entity().downgrade(),
            window.window_handle(),
            key.0.clone(),
        );
        let asked = key.clone();
        ask(key, cx, move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                this.update(cx, |shell, cx| {
                    let text = answer(&asked).is_some_and(|head| head != Head::Binary);
                    let current = text
                        && shell
                            .quick_look
                            .as_ref()
                            .is_some_and(|host| host.token == token);
                    if current && shell.quick_look_open(cx) {
                        shell.show_row(side, index, &row, path, window, cx);
                    }
                })
            });
        });
    }
}

#[cfg(test)]
#[path = "sniff_tests.rs"]
mod tests;
