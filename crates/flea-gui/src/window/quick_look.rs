//! Quick Look over the window: pv1's card, built on first open, its subject read through the pane's backend.
use crate::{
    g2::pv1::{Body, Event, QuickLook, Subject},
    state::Shell,
};
use flea::keys::Context as Keys;
use flea::{
    json::escape,
    protocol::RowFacts,
    ui_logic_pv1::{
        kinds::{self, State},
        rect::Shape,
    },
};
use gpui::{AppContext, ClipboardItem, Context, Entity, Subscription, Task, Window};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[path = "quick_look_compare.rs"]
mod compare_open;
#[path = "quick_look_docs.rs"]
mod docs;
#[path = "quick_look_interim.rs"]
mod interim;
#[path = "quick_look_keys.rs"]
mod keys;
#[path = "quick_look_mount.rs"]
mod mount;
#[path = "quick_look_readers.rs"]
pub(crate) mod readers;
#[path = "quick_look_replies.rs"]
mod replies;
#[path = "quick_look_warm.rs"]
pub(crate) mod warm;
#[path = "quick_look_wire.rs"]
pub(crate) mod wire;

// Its own token range, so a reply the column asked for never reads as one of these.
static NEXT: AtomicU64 = AtomicU64::new(1 << 40);
fn token() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The most lines one code ask returns (src/backend/codereq.rs MAX_COUNT).
const PAGE: usize = 400;

/// What the open card still waits for from the backend.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ask {
    Nothing,
    Code {
        path: String,
    },
    Meta {
        path: String,
        link: Option<String>,
    },
    Compare {
        a: String,
        b: String,
    },
    /// Compare on rows outside the held window: the backend's snapshot of them, then its validated paths (Ops.js menuKinds).
    Resolve {
        listing: usize,
        rows: Vec<usize>,
        mark: Option<RowFacts>,
        kinds: Option<Vec<u32>>,
    },
}

pub(crate) struct Host {
    pub view: Entity<QuickLook>,
    pub side: usize,
    pub token: u64,
    pub ask: Ask,
    /// The cursor row the card was opened on, so a Follow knows where it started.
    pub row: Option<usize>,
    /// Compare's backend answer and patch, "" until it lands.
    pub answer: String,
    pub patch: String,
    pub prior: Keys,
    /// The mounted view's outbox and the token of its meta ask, 0 once answered.
    pub wire: wire::Wire,
    pub meta: u64,
    /// The decoder media plays through; None uses the sandboxed helper.
    pub backend: Option<std::rc::Rc<dyn crate::g2::pv3::engine::Backend>>,
    /// Preview.qml markdownSource lasts until the card closes, including moves through other kinds.
    markdown_view: crate::g2::pv2::markdown::View,
    cap: Option<Task<()>>,
    _events: Subscription,
    _drain: Task<()>,
}

impl Shell {
    pub(crate) fn quick_look_open(&self, cx: &gpui::App) -> bool {
        self.quick_look
            .as_ref()
            .is_some_and(|host| cx.read_entity(&host.view, |view, _| view.is_open()))
    }

    /// Builds the card on the first open; later opens reuse it.
    fn quick_look_host(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> &mut Host {
        if self.quick_look.is_none() {
            let catalog = self.locale.catalog.clone();
            let view = cx.new(|_| QuickLook::new(catalog));
            let events = cx.subscribe_in(&view, window, |shell, _, event: &Event, window, cx| {
                shell.quick_look_event(*event, window, cx)
            });
            // A mounted view asks as it lays out and scrolls; each ask wakes this drain, which sends it.
            let (wire, woken) = wire::Wire::new();
            let drain = cx.spawn(async move |this, cx| {
                while woken.recv().await.is_ok() {
                    if this
                        .update(cx, |shell, _| shell.quick_look_drain())
                        .is_err()
                    {
                        break;
                    }
                }
            });
            self.quick_look = Some(Host {
                view,
                side,
                token: 0,
                ask: Ask::Nothing,
                row: None,
                answer: String::new(),
                patch: String::new(),
                prior: Keys::Listing,
                wire,
                meta: 0,
                backend: None,
                markdown_view: Default::default(),
                cap: None,
                _events: events,
                _drain: drain,
            });
        }
        self.restyle_quick_look(window, cx);
        self.quick_look_release();
        let languages = self
            .launch
            .settings
            .data
            .get("langMarks")
            .and_then(flea::jsondoc::Json::as_bool)
            != Some(false);
        let host = self.quick_look.as_mut().expect("built above");
        docs::remember_markdown(host, cx);
        host.view
            .update(cx, |view, cx| view.set_languages(languages, cx));
        host.side = side;
        host
    }

    /// PreviewKeys.open: Space on the cursor file opens the card; false on a folder or with no loaded row.
    pub(crate) fn open_quick_look(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.panes[side]
            .as_ref()
            .filter(|p| !p.pane.busy() && !p.stopped)
        else {
            return false;
        };
        let index = pane.pane.cursor;
        let Some(row) = pane
            .pane
            .held
            .row(index)
            .filter(|row| !row.dir && !row.name.is_empty())
            .cloned()
        else {
            return false;
        };
        let path = crate::navigation::history::join(pane.pane.path(), &row.name);
        self.show_row(side, index, &row, path, window, cx);
        true
    }

    /// One row's subject by its kind, held back until the backend answers what it still needs.
    pub(super) fn show_row(
        &mut self,
        side: usize,
        index: usize,
        row: &RowFacts,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = kinds::quick_look_kind(&row.icon, &row.name, false);
        let subject = |state, body| Subject {
            name: row.name.clone(),
            icon: row.icon.clone(),
            mode: row.mode,
            state,
            shape: Shape::unknown(state),
            body,
        };
        // a name with no known type opens as text when it reads as text (.bash_aliases, .jc141rc).
        let sniff = (state == State::Unsupported && crate::window::sniff::candidate(row))
            .then(|| crate::window::sniff::key(row, &path));
        let text = sniff.as_ref().and_then(crate::window::sniff::answer);
        let mount =
            mount::Mount::of(row).or_else(|| (text == Some(true)).then_some(mount::Mount::Text));
        if let Some(mount) = mount {
            if self.show_doc(side, index, row, &path, mount, window, cx) {
                return;
            }
        }
        if state == State::Image {
            self.show_picture(side, index, row, path, window, cx);
            return;
        }
        let token = token();
        let (subject, revealed, line, ask) = match state {
            State::Text => {
                let state = if kinds::is_code(&row.name) {
                    State::Code
                } else {
                    State::Text
                };
                let body = Body::Text {
                    lines: Vec::new(),
                    count: String::new(),
                };
                let line = code_line(token, &path, 1);
                (subject(state, body), false, Some(line), Ask::Code { path })
            }
            _ => {
                let body = self.quick_look_unsupported(row, &path, token, window, cx);
                (subject(State::Unsupported, body), true, None, Ask::Nothing)
            }
        };
        let early = replies::header_path(&ask, &subject.body);
        let host = self.quick_look_host(side, window, cx);
        host.token = token;
        host.row = Some(index);
        host.ask = ask;
        self.show(subject, revealed, line, window, cx);
        self.quick_look_header(token, early, cx);
        // Unread yet, the card opens as it is and reopens as text if its head reads as text.
        if let Some(key) = sniff.filter(|_| text.is_none()) {
            self.quick_look_sniff(key, token, (side, index), row.clone(), window, cx);
        }
    }

    /// Opens or replaces the card's subject and sends its one ask; the card takes the keyboard.
    fn show(
        &mut self,
        subject: Subject,
        revealed: bool,
        line: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.quick_look.as_mut() else {
            return;
        };
        let opening = !host.view.read(cx).is_open();
        if opening {
            host.prior = self.keyboard.context;
        }
        let context = keys::context(&subject);
        let side = host.side;
        host.view
            .update(cx, |view, cx| view.open(subject, revealed, cx));
        // PreviewSwap's cap: a card still waiting on its answer shows what it has, never a ground with nothing on it.
        let view = host.view.clone();
        let generation = view.read(cx).observations.borrow().shown_seq;
        host.cap = (!revealed).then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(crate::pane::swap::CAP_MS))
                    .await;
                let _ = this.update(cx, |_, cx| {
                    let current = view.read(cx).observations.borrow().shown_seq == generation;
                    if current && view.read(cx).is_open() && !view.read(cx).is_revealed() {
                        view.update(cx, |view, cx| view.reveal_cap(cx));
                    }
                });
            })
        });
        self.keyboard.context = context;
        self.keyboard.disarm();
        window.focus(&self.focus, cx);
        if let Some(line) = line {
            self.quick_look_send(side, line);
        }
        cx.notify();
    }

    /// A Compare snapshot still resolving is closed on its own side before its ask is replaced or the card closes.
    fn quick_look_release(&mut self) {
        let Some(host) = self.quick_look.as_mut().filter(|h| h.token != 0) else {
            return;
        };
        if matches!(host.ask, Ask::Resolve { .. }) {
            let (side, token) = (host.side, host.token);
            host.token = 0;
            host.ask = Ask::Nothing;
            self.quick_look_send(side, compare_open::close_line(token));
        }
    }

    fn quick_look_send(&mut self, side: usize, line: String) {
        let locale = self.locale.clone();
        let Some(pane) = self.panes[side].as_mut().filter(|p| !p.stopped) else {
            return;
        };
        if let Some(backend) = pane.backend.as_ref() {
            if let Err(error) = backend.line(line) {
                pane.stopped = true;
                pane.record_error(error, &locale);
            }
        }
    }

    fn quick_look_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quick_look_release();
        let Some(host) = self.quick_look.as_mut() else {
            return;
        };
        host.markdown_view = Default::default();
        if host.token == 0
            && host.ask == Ask::Nothing
            && host.row.is_none()
            && host.answer.is_empty()
        {
            return;
        }
        host.token = 0;
        host.ask = Ask::Nothing;
        host.row = None;
        host.answer.clear();
        host.patch.clear();
        let (prior, side) = (host.prior, host.side);
        // A card opened from the focused column hands the keyboard back to it.
        let column = matches!(prior, Keys::Preview | Keys::Pdf | Keys::Media)
            .then(|| self.selection_preview(side))
            .flatten();
        match column {
            Some(ref column) => window.focus(&column.read(cx).focus.clone(), cx),
            None => window.focus(&self.focus, cx),
        }
        self.keyboard.context = if column.is_some() {
            prior
        } else {
            Keys::Listing
        };
        self.keyboard.disarm();
        cx.notify();
    }

    fn quick_look_event(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::Closed if !self.quick_look_open(cx) => self.quick_look_closed(window, cx),
            // CompareView copyPatch: the whole patch, headers included, goes to the clipboard.
            Event::CopyPatch => {
                if let Some(host) = self.quick_look.as_ref().filter(|h| !h.patch.is_empty()) {
                    cx.write_to_clipboard(ClipboardItem::new_string(host.patch.clone()));
                }
            }
            Event::OpenWith => self.quick_look_open_with(cx),
            // Follow runs at dispatch; a queued close cannot retire a reopened subject.
            Event::Follow(_) | Event::Closed => {}
        }
    }

    /// The card's Open With opens the chooser the row menu's Open With… opens, for the file the card shows.
    fn quick_look_open_with(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.quick_look.as_ref() else {
            return;
        };
        let (side, view) = (host.side, host.view.clone());
        let Some(name) = view.read(cx).subject().map(|s| s.name.clone()) else {
            return;
        };
        let Some(pane) = self.panes[side].as_ref() else {
            return;
        };
        let path = crate::navigation::history::join(pane.pane.path(), &name);
        self.open_open_with(side, path, cx);
    }
}

fn code_line(token: u64, path: &str, first: usize) -> String {
    format!(
        r#"{{"c":"code","slot":"quicklook","token":{token},"path":"{}","first":{first},"count":{PAGE},"raw":true}}"#,
        escape(path)
    )
}
