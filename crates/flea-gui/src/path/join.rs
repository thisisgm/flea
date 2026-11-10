//! One window joins the path and toolbar producers to its pane navigation and action owners.
use super::{
    chrome::{ChromeView, Inputs, PanePath},
    history,
    rank::Jump,
    view::{Out, PathView},
};
use crate::{
    state::Shell,
    window::{dispatch::Order, input::ActionEvent, navigate::Step, startup::View},
};
use flea::ui_logic_status_feedback::status::Hint;
use gpui::{AppContext, Context, Entity, Subscription, Window};

#[derive(Default)]
pub struct Chrome {
    pub view: Option<Entity<ChromeView>>,
    pub panes: [Option<Entity<PanePath>>; 2],
    pub aliases: Vec<super::crumbs::Alias>,
    pub(crate) saved_labels: std::collections::HashMap<String, String>,
    /// The network service sets this when it subscribes to `NetworkPath`; until then a typed address is refused with a sentence.
    pub network: bool,
    /// The recent history file as the jump last read it.
    pub history: crate::window::path_host::Stamp,
    /// History reads not answered yet, each a frame still to come.
    pub waiting: usize,
    /// Each side's jump asks not answered yet; a lost backend owes none.
    pub jumps: [usize; 2],
    home: gpui::SharedString,
    subscriptions: Vec<Subscription>,
}

/// A typed network address keeps the originating pane when the network service takes it.
#[derive(Clone, Debug)]
pub struct NetworkPath {
    pub side: usize,
    pub uri: String,
}

impl gpui::EventEmitter<NetworkPath> for Shell {}

impl Shell {
    pub(crate) fn path_holds_keyboard(&self, window: &Window, cx: &gpui::App) -> bool {
        self.chrome.view.as_ref().is_some_and(|chrome| {
            chrome.read_with(cx, |chrome, cx| {
                chrome
                    .path
                    .read_with(cx, |path, _| path.holds_keyboard(window))
            })
        })
    }

    pub(crate) fn sync_chrome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let inputs = Inputs {
            view: self.view,
            // Back and Up close Recent onto the folder it opened over, so both stand at full ink there, as v0.3.12 draws them.
            back: !host.nav.history.back.is_empty() || host.recent.on,
            up: host.nav.can_go_up(&host.pane) || host.recent.on,
            filter: !host.pane.state.search,
            sort: !host.pane.state.search,
            live: !self.card_holds_keyboard(cx)
                && self.overlay.as_ref().is_none_or(|overlay| {
                    overlay.read_with(cx, |card, _| {
                        !(card.model.open()
                            || (card.model.jobs.visible() && !card.model.jobs.folded))
                    })
                }),
            corner: self.corner,
        };
        if self.chrome.view.is_none() {
            self.chrome.home = std::env::var("HOME").unwrap_or_default().into();
            let favourites = history::favourites(&self.launch.settings.data);
            let path = cx.new(|cx| {
                let mut view =
                    PathView::new(self.locale.catalog.clone(), Box::new(Jump::default()), cx);
                view.set_favourites(favourites);
                view
            });
            self.chrome.subscriptions.push(cx.subscribe_in(
                &path,
                window,
                |shell, _, out, window, cx| shell.path_out(out, window, cx),
            ));
            let view = cx.new(|cx| ChromeView::new(path, inputs, cx));
            view.read(cx).frame.borrow_mut().cover = self.settings.cover.clone();
            self.chrome.subscriptions.push(cx.subscribe_in(
                &view,
                window,
                |shell, _, action, window, cx| shell.chrome_action(action, window, cx),
            ));
            self.slots.chrome = Some(view.clone().into());
            self.chrome.view = Some(view);
        }
        let chrome = self.chrome.view.clone().expect("mounted chrome");
        self.sync_chrome_stops(window, cx);
        let path = chrome.read_with(cx, |view, _| view.path.clone());
        let host = self.panes[side].as_ref().expect("focused pane");
        let path_inputs = super::view::Inputs {
            git_label: host.git.paint.label.clone(),
            paths: self.path_labels.clone(),
            home: self.chrome.home.clone(),
            aliases: self.chrome.aliases.clone(),
            side,
            dual: self.view == View::Dual,
            failed: host.nav.path_failed(&host.pane),
            show_hidden: host.nav.folders.borrow().hidden,
            blocked: !inputs.live,
            address_path: self
                .launch
                .settings
                .data
                .get("addressBar")
                .and_then(flea::jsondoc::Json::as_str)
                == Some("path"),
            elide_start: false,
            first: host.pane.first_count(),
        };
        let face = self.active_typeface();
        path.update(cx, |view, cx| {
            // A font-set reaches the cached path view here, so the strip repaints in the new face.
            if view.paint.borrow_mut().set_typeface(face) {
                cx.notify();
            }
            if view.inputs.side != path_inputs.side || view.inputs.dual != path_inputs.dual {
                view.close(cx);
            }
            if view.corner != self.corner {
                view.corner = self.corner;
            }
            view.sync_frame(path_inputs);
        });
        chrome.update(cx, |view, _| view.inputs = inputs);
        for side in 0..2 {
            if self.view == View::Dual && self.chrome.panes[side].is_none() {
                let strip = cx.new(|_| PanePath {
                    path: path.clone(),
                    side,
                });
                self.slots.paths[side] = Some(strip.clone().into());
                self.chrome.panes[side] = Some(strip);
            }
        }
    }

    pub(crate) fn sync_chrome_probe(&self, cx: &mut Context<Self>) {
        let Some(chrome) = &self.chrome.view else {
            return;
        };
        let path = chrome.read_with(cx, |view, _| view.path.clone());
        let probe = self
            .visual
            .pending
            .as_ref()
            .map(|_| self.visual.probe.clone());
        path.update(cx, |view, _| view.probe = probe);
    }

    pub(crate) fn chrome_action(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search_action(action, window, cx) {
            return;
        }
        if self.settings_action(action, window, cx) {
            return;
        }
        if action == "pathBar" {
            self.sync_chrome(window, cx);
            if let Some(chrome) = &self.chrome.view {
                let path = chrome.read_with(cx, |view, _| view.path.clone());
                path.update(cx, |view, cx| view.open(window, cx));
            }
            return;
        } else if let Some(step) = match action {
            "back" => Some(Step::Back),
            "up" => Some(Step::Up),
            _ => None,
        } {
            let _ = self.step(self.focus_side, step, window, cx);
        } else {
            let Some(host) = &self.panes[self.focus_side] else {
                return;
            };
            let event = ActionEvent {
                action,
                context: self.keyboard.context,
                pair_armed: false,
                seat: flea::keys::sequence::Seat {
                    listing: host.pane.held.listing as u64,
                    cursor: host.pane.cursor,
                    view: self.view as u64,
                    focus: self.focus_side as u64,
                    ..Default::default()
                },
            };
            self.dispatch(Order::Action(event), window, cx);
        }
        window.focus(&self.focus, cx);
    }

    fn path_out(&mut self, out: &Out, window: &mut Window, cx: &mut Context<Self>) {
        match out {
            Out::Open { side, path } => {
                let _ = self.step(*side, Step::Open(path.clone()), window, cx);
            }
            Out::Focus(side) => self.focus_pane(*side, cx),
            Out::Network(uri) if self.chrome.network => cx.emit(NetworkPath {
                side: self.focus_side,
                uri: uri.clone(),
            }),
            Out::Network(_) => {
                let text = super::Said::NetworkUnavailable.text(&self.locale.catalog);
                if let Some(host) = self.panes[self.focus_side].as_mut() {
                    host.status = text.clone().into();
                }
                self.status_messages(cx, |messages, now| {
                    messages.say(&text, true, "", Hint::None, now);
                });
                cx.notify();
            }
            Out::Said(message) => {
                if let Some(host) = self.panes[self.focus_side].as_mut() {
                    host.status = message.clone().into();
                }
                self.status_messages(cx, |messages, now| {
                    messages.say(message, false, "", Hint::None, now);
                });
                cx.notify();
            }
            Out::Closed => {
                self.focus_pane(self.focus_side, cx);
                // Under a covering page (Trash) the keys go back to that page's list, never the hidden folder's.
                window.focus(&self.overlay_return_focus(cx), cx);
                self.check_history(cx);
            }
            Out::Send { side, line } => {
                if let Some(host) = self.panes.get_mut(*side).and_then(Option::as_mut) {
                    if let Some(backend) = &host.backend {
                        match backend.path_line(line.clone()) {
                            // Every jump line earns exactly one jumped reply, so the scene waits for it.
                            Ok(()) if super::is_jump_line(line) => self.chrome.jumps[*side] += 1,
                            Ok(()) => {}
                            Err(error) => host.record_error(error, &self.locale),
                        }
                    }
                }
            }
            Out::ReadHistory => self.read_history(cx),
        }
    }

    /// The favourites the jump ranks first follow each accepted ui state.
    pub(crate) fn path_favourites(&mut self, state: &flea::jsondoc::Json, cx: &mut Context<Self>) {
        self.relabel_path_aliases(state);
        let Some(chrome) = &self.chrome.view else {
            return;
        };
        let path = chrome.read_with(cx, |view, _| view.path.clone());
        let favourites = history::favourites(state);
        path.update(cx, |view, _| view.set_favourites(favourites));
    }

    /// The recent history, read and parsed on a worker; the jump's whole ask waits for it.
    fn read_history(&mut self, cx: &mut Context<Self>) {
        let read = cx
            .background_executor()
            .spawn(async move { crate::window::path_host::load(crate::window::path_host::file()) });
        self.chrome.waiting += 1;
        cx.spawn(async move |this, cx| {
            let (stamp, paths) = read.await;
            let _ = this.update(cx, |shell, cx| {
                shell.chrome.waiting = shell.chrome.waiting.saturating_sub(1);
                shell.chrome.history = stamp;
                if let Some(chrome) = &shell.chrome.view {
                    let path = chrome.read_with(cx, |view, _| view.path.clone());
                    path.update(cx, |view, cx| view.history_read(paths, cx));
                }
            });
        })
        .detach();
    }

    /// After each edit the file is stated on a worker, so the next open reads it again only when it changed.
    fn check_history(&mut self, cx: &mut Context<Self>) {
        let check = cx.background_executor().spawn(async move {
            crate::window::path_host::stamp(crate::window::path_host::file())
        });
        cx.spawn(async move |this, cx| {
            let stamp = check.await;
            let _ = this.update(cx, |shell, cx| {
                let Some(chrome) = &shell.chrome.view else {
                    return;
                };
                if stamp != shell.chrome.history {
                    let path = chrome.read_with(cx, |view, _| view.path.clone());
                    path.update(cx, |view, _| view.history_changed());
                }
            });
        })
        .detach();
    }

    /// The path producer takes its own reply; any other peeked answer comes back for the next peek consumer (the columns view shares the wire, ui/WindowBody.qml:232-237).
    pub(crate) fn path_answer(
        &mut self,
        side: usize,
        answer: super::Answer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<super::Answer> {
        if matches!(answer, super::Answer::Jumped(_)) {
            self.chrome.jumps[side] = self.chrome.jumps[side].saturating_sub(1);
        }
        let Some(chrome) = &self.chrome.view else {
            return Some(answer);
        };
        let path = chrome.read_with(cx, |view, _| view.path.clone());
        if !path.read_with(cx, |view, _| {
            view.inputs.side == side && view.claims(&answer)
        }) {
            return Some(answer);
        }
        path.update(cx, |view, cx| view.receive(answer, window, cx));
        None
    }
}
