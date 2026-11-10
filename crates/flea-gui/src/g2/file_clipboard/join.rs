//! The clipboard joined to the live Shell: key verbs and menu picks call the Clipboard directly, clip and paths replies reach their one owner, a paste goes through op1's collision host holding its captured entry until the transfer spends it or ends, and List, Columns and Grid rows draw the marks.
#[path = "join_marks.rs"]
mod view_marks;

use super::{
    keys::Verb,
    marks::ViewMarks,
    seams::{Captured, Foreign, MenuAction, Owner, Publish, Sink, Wire},
    Clipboard, Landing, Take,
};
use crate::{state::Shell, window::pane::PaneHost};
use flea::i18n::I18n;
use flea::ui_logic_file_clipboard::{self as core, PasteAsk};
use flea::ui_logic_file_ops::collide::{text, texts, Request};
use flea::ui_logic_status_feedback::status::Hint;
use gpui::{ClipboardItem, Context, Window};
use std::rc::Rc;

/// The window's clipboard state beside the Shell: the one Clipboard, each side's captured paste and the marks the views draw.
#[derive(Default)]
pub struct Joined {
    pub board: Clipboard,
    /// The entry a side's paste captured, held until op1 sends the transfer or the ask ends.
    pub captured: [Option<Captured>; 2],
    marks: Option<(u64, Rc<ViewMarks>)>,
    drawn: [Option<(u64, String, bool)>; 2],
}

pub(crate) use super::transport::PaneWire;

#[derive(Default)]
struct Asks(Vec<(PasteAsk, Captured)>);

impl Sink for Asks {
    fn paste(&mut self, ask: PasteAsk, captured: Captured) {
        self.0.push((ask, captured));
    }
}

// The backend's clipSet stays the one claim until the fork's validate-only authority is published, so the file owner publishes nothing and only Copy As text reaches the system clipboard.
#[derive(Default)]
struct Texts(Vec<String>);

impl Owner for Texts {
    fn publish(&mut self, _item: Publish) -> bool {
        false
    }

    fn publish_text(&mut self, text: String) -> bool {
        self.0.push(text);
        true
    }

    fn clear_if_owned(&mut self, _token: &str, _cut_paths: &[String]) -> bool {
        false
    }

    fn drain_foreign(&mut self) -> Vec<Foreign> {
        Vec::new()
    }
}

// The rows a verb acts on: the selection, else the cursor row; an empty listing has none.
pub(super) fn picks(host: &PaneHost) -> Vec<usize> {
    if host.pane.held.total == 0 {
        Vec::new()
    } else if host.selected.is_empty() {
        vec![host.pane.cursor]
    } else {
        host.selected.clone()
    }
}

// Sample input: a cut transfer of ["/a/f"] into "/b" reads {"c":"transfer","op":"move","paths":["/a/f"],"dest":"/b"}, as Clipboard.paste asks.
pub fn request(ask: &PasteAsk) -> Option<(Request, bool)> {
    fn refs(paths: &[String]) -> Vec<&str> {
        paths.iter().map(String::as_str).collect()
    }
    Some(match ask {
        PasteAsk::Transfer {
            op,
            paths,
            dest,
            spends_cut,
        } => (
            Request::new()
                .with("c", &text("transfer"))
                .with("op", &text(op))
                .with("paths", &texts(&refs(paths)))
                .with("dest", &text(dest)),
            *spends_cut,
        ),
        PasteAsk::Link { kind, paths, dest } => (
            Request::new()
                .with("c", &text("link"))
                .with("op", &text(kind.op()))
                .with("paths", &texts(&refs(paths)))
                .with("dest", &text(dest)),
            false,
        ),
        PasteAsk::Image { .. } => return None,
    })
}

type Step<'a, R> = dyn FnOnce(
        &mut Clipboard,
        &I18n,
        u64,
        Take<'_>,
        Landing<'_>,
        &dyn Wire,
        &mut dyn Sink,
        &mut dyn Owner,
    ) -> (R, Vec<core::Said>)
    + 'a;

impl Shell {
    // One clipboard step on a side: its facts in, then said lines to the footer, pastes to op1 and Copy As text to the system clipboard.
    pub(super) fn clipboard_run<R: Default>(
        &mut self,
        side: usize,
        step: Box<Step<'_, R>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> R {
        let Some(host) = self.panes[side].as_ref() else {
            return R::default();
        };
        let backend = side as u64;
        let busy = host.pane.busy();
        let take = Take {
            path: &host.path,
            listing: host.pane.held.listing as u64,
            paths_busy: host.nav.gate.paths_pending || self.carry_pending(side, window, cx),
            list_in_flight: busy,
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() as i64);
        let landing = Landing {
            path: &host.path,
            list_in_flight: busy,
            recent: host.recent.on || flea::ui_logic_picker_portal::is_recent(host.pane.place()),
            stamp: core::stamp_at(now).unwrap_or(core::Stamp {
                year: 1970,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
            }),
        };
        let wire = PaneWire(host.backend.as_ref());
        let mut asks = Asks::default();
        let mut texts = Texts::default();
        let board = &mut self.clipboard.board;
        if wire.0.is_some() {
            board.watch_unknown(backend, &wire);
        }
        let (result, said) = step(
            board,
            &self.locale.catalog,
            backend,
            take,
            landing,
            &wire,
            &mut asks,
            &mut texts,
        );
        let pending = board.paths_pending(backend) || self.carry_pending(side, window, cx);
        if let Some(host) = self.panes[side].as_mut() {
            host.nav.gate.clip_pending = pending;
        }
        if let Some(menu_side) = self.menu.captured.as_ref().map(|capture| capture.side) {
            let (available, failed, _) = self.clipboard_menu_facts(menu_side);
            if (
                self.menu.context.clipboard_available,
                self.menu.context.clipboard_image_available,
                self.menu.context.clipboard_watch_failed,
            ) != (available, self.clipboard.board.image_available(), failed)
            {
                self.menu.context.clipboard_available = available;
                self.menu.context.clipboard_image_available =
                    self.clipboard.board.image_available();
                self.menu.context.clipboard_watch_failed = failed;
                if let Some(host) = self.panes[menu_side]
                    .as_ref()
                    .filter(|_| self.menu_opened())
                {
                    let entries = self.menu_entries();
                    let list = host.list.borrow();
                    self.menu.menu.borrow_mut().refresh(
                        entries,
                        &self.menu_env(&list.typeface),
                        window,
                    );
                } else {
                    self.refresh_menu(window, cx);
                }
            }
        }
        for line in said {
            let hint = if line.paste_hint {
                Hint::Paste
            } else {
                Hint::None
            };
            if let Some(host) = self.panes[side].as_mut() {
                host.status = line.text.clone().into();
            }
            self.status_messages(cx, |messages, now| {
                messages.say(&line.text, line.error, "", hint, now);
            });
        }
        for text in texts.0 {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        for (ask, captured) in asks.0 {
            let Some((request, cut)) = request(&ask) else {
                self.clipboard_image(side, &ask, cx);
                continue;
            };
            if self.ask_operation(side, &request, &[], cut, window, cx) {
                self.clipboard.captured[side] = Some(captured);
            }
        }
        cx.notify();
        result
    }

    /// A key verb: copy, cut, paste, forced move and the link pastes call the Clipboard here, never through an emitted action.
    pub(crate) fn clipboard_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if action == "carry" {
            return self.carry_lift(window, cx);
        }
        let Some(verb) = Verb::parse(action) else {
            return false;
        };
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let rows = picks(host);
        self.clipboard_run(
            side,
            Box::new(
                move |board, i18n, backend, take, landing, wire, sink, owner| {
                    let said = board.act(
                        i18n, backend, verb, None, rows, take, landing, wire, sink, owner,
                    );
                    ((), said.unwrap_or_default())
                },
            ),
            window,
            cx,
        );
        true
    }

    /// A rows-menu pick the Clipboard owns: Copy, Cut, Paste, Copy As and Paste As over the validated snapshot.
    pub(crate) fn clipboard_menu(
        &mut self,
        capture: &crate::g2::context_menus::join::Captured,
        action: &str,
        leaf: &str,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let chosen = if leaf.is_empty() {
            action.to_owned()
        } else {
            format!("{action}:{leaf}")
        };
        let verb = Verb::parse(action).filter(|_| leaf.is_empty());
        if verb.is_none() && MenuAction::parse(&chosen) == MenuAction::Unhandled {
            return false;
        }
        self.retire_menu(window, cx);
        let side = capture.side;
        let Some(host) = self.panes[side].as_ref() else {
            return true;
        };
        let rows = capture.rows.clone();
        let cursor_path = capture.cursor_path(host);
        let base = capture.path.clone();
        let snapshot = paths.to_vec();
        self.clipboard_run(
            side,
            Box::new(
                move |board, i18n, backend, take, landing, wire, sink, owner| {
                    let take = Take {
                        path: &base,
                        ..take
                    };
                    let landing = Landing {
                        path: &base,
                        ..landing
                    };
                    let said = match verb {
                        Some(verb) => {
                            let paths = (!snapshot.is_empty()).then_some(snapshot);
                            board
                                .act(
                                    i18n, backend, verb, paths, rows, take, landing, wire, sink,
                                    owner,
                                )
                                .unwrap_or_default()
                        }
                        None => board.menu_action(
                            i18n,
                            backend,
                            &chosen,
                            &snapshot,
                            &cursor_path,
                            rows,
                            take,
                            landing,
                            wire,
                            sink,
                            owner,
                        ),
                    };
                    ((), said)
                },
            ),
            window,
            cx,
        );
        true
    }

    /// op1 published a frame: a sent cut transfer spends exactly the captured cut, and an ask that ended without one releases its capture.
    pub(crate) fn clipboard_operations(
        &mut self,
        side: usize,
        spent: bool,
        idle: bool,
        cx: &mut Context<Self>,
    ) {
        if spent {
            if let Some(captured) = self.clipboard.captured[side].take() {
                let Some(host) = self.panes[side].as_ref() else {
                    return;
                };
                let wire = PaneWire(host.backend.as_ref());
                let mut asks = Asks::default();
                let mut texts = Texts::default();
                let said = self.clipboard.board.spent(
                    side as u64,
                    &captured,
                    true,
                    &wire,
                    &mut asks,
                    &mut texts,
                );
                drop(said);
                cx.notify();
            }
        } else if idle {
            self.clipboard.captured[side] = None;
        }
    }

    /// The menu's clipboard facts: Paste shows while files are held or the watch is down, Paste As only where links are possible; a pane never watched (a test shell) reads as up.
    pub(crate) fn clipboard_menu_facts(&self, side: usize) -> (bool, bool, bool) {
        let backend = side as u64;
        let board = &self.clipboard.board;
        let can_link = self.surfaces[side]
            .filesystem
            .facts
            .as_ref()
            .is_none_or(|facts| !matches!(facts.name.as_str(), "vfat" | "exfat"));
        (board.available(), board.watch_failed(backend), can_link)
    }
}
