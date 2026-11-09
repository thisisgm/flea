//! The one path an action id runs on outside a key press or a click: a built-in from the key path's dispatch point, a plugin action through the menu's own row path.
use crate::{
    ipc::server::Answer,
    pane::requests,
    state::Shell,
    window::input::{Dispatch, KeyRun},
};
use flea::{
    ipc::wire::Reply,
    keys::actions::{self, Id, UNAVAILABLE, UNKNOWN},
    ui_logic_custom_actions::plugins::{self, Menu},
    ui_logic_plugin_settings::{contributions, store, wire::Plugin},
};
use gpui::{Context, Window};
use std::sync::mpsc::SyncSender;
use std::time::{Duration, Instant};

// Inside the server's four-second answer bound, with room left for the menu's snapshot and validation.
const READY_WAIT: Duration = Duration::from_millis(1500);
const READY_POLL: Duration = Duration::from_millis(20);

/// One caller's answer; dropped unanswered, it refuses with the sentence it was built with. A key binding has no one to answer.
pub(crate) struct Waiting {
    sender: Option<SyncSender<Answer>>,
    fallback: String,
}

impl Waiting {
    pub(crate) fn new(sender: SyncSender<Answer>, fallback: String) -> Self {
        Self {
            sender: Some(sender),
            fallback,
        }
    }

    // Key bindings (PLUGIN-KEYS) call this; until they join only tests do.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn none() -> Self {
        Self {
            sender: None,
            fallback: String::new(),
        }
    }

    pub(crate) fn answer(mut self, reply: Reply) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(Answer { reply, quit: false });
        }
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let reply = Reply::error(UNAVAILABLE, std::mem::take(&mut self.fallback));
            let _ = sender.try_send(Answer { reply, quit: false });
        }
    }
}

pub(crate) fn ran() -> Reply {
    Reply {
        code: 0,
        stdout: String::new(),
        stderr: String::new(),
    }
}

fn unknown(id: &str) -> Reply {
    Reply::error(
        UNKNOWN,
        format!("flea: no action has the id {id}; flea --actions lists the ids"),
    )
}

impl Shell {
    fn refusal(&self, key: &str) -> String {
        format!("flea: {}", self.locale.catalog.tr(key))
    }

    pub(crate) fn action_refused(&self, key: &str) -> Reply {
        Reply::error(UNAVAILABLE, self.refusal(key))
    }

    pub(crate) fn action_fallback(&self) -> String {
        self.refusal("menu.refusal.moved")
    }

    /// Runs one action id and answers `waiting` with RAN, UNKNOWN or UNAVAILABLE, at once or when the menu row settles.
    pub(crate) fn run_action(
        &mut self,
        id: &str,
        waiting: Waiting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match actions::parse(id) {
            None => waiting.answer(unknown(id)),
            Some(Id::Builtin(action)) => {
                let reply = self.run_builtin_action(action, window, cx);
                waiting.answer(reply);
            }
            Some(Id::Plugin { plugin, action }) => {
                let (plugin, action) = (plugin.to_owned(), action.to_owned());
                self.run_plugin_action(id.to_owned(), plugin, action, waiting, window, cx);
            }
        }
    }

    /// A plugin's actions are callable while it is installed, switched on and not failed, the menu's own test before it starts one.
    fn plugin_switched_on(&self, plugin: &Plugin) -> bool {
        plugin.enabled
            && plugin.failure.is_none()
            && store::enabled(
                &self.launch.settings.data,
                &plugin.manifest.id,
                plugin.enabled,
            )
    }

    /// Built-ins in key table order, then each switched-on plugin's `plugin:action` ids.
    pub(crate) fn action_ids(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<String> {
        self.follow_plugins(window, cx);
        let mut ids: Vec<String> = actions::builtin_ids()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut contributed: Vec<String> = self
            .plugins
            .host
            .as_ref()
            .map(|host| {
                host.read(cx)
                    .plugins
                    .iter()
                    .filter(|p| self.plugin_switched_on(p))
                    .flat_map(|p| contributions::menu_actions(&p.manifest).unwrap_or_default())
                    .map(|c| plugins::leaf(&c))
                    .collect()
            })
            .unwrap_or_default();
        contributed.sort();
        contributed.dedup();
        ids.extend(contributed);
        ids
    }

    /// Whatever holds the keyboard would take the key first, so an outside action waits until it closes.
    fn keyboard_taken(&self, window: &Window, cx: &gpui::App) -> bool {
        self.wheel_blocked(cx)
            || self
                .rename
                .edit
                .as_ref()
                .is_some_and(|edit| edit.entry.editing)
            || self.path_holds_keyboard(window, cx)
    }

    /// A row already captured or delivering belongs to its own caller, so a second one is refused, never swapped in.
    fn menu_taken(&self, window: &Window, cx: &gpui::App) -> bool {
        self.keyboard_taken(window, cx)
            || self.menu.captured.is_some()
            || self.menu.activating
            || self.menu.reading
    }

    pub(crate) fn run_builtin_action(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        if self.keyboard_taken(window, cx) {
            return self.action_refused("feedback.action.unavailable");
        }
        let Some(host) = &self.panes[self.focus_side] else {
            return self.action_refused("feedback.action.unavailable");
        };
        let seat = flea::keys::sequence::Seat {
            listing: host.pane.held.listing as u64,
            cursor: host.pane.cursor,
            view: self.view as u64,
            focus: self.focus_side as u64,
            ..Default::default()
        };
        if let Some(preview) = host
            .preview
            .as_ref()
            .filter(|p| p.read(cx).focus.is_focused(window))
        {
            self.keyboard.context = preview.read(cx).key_context();
        }
        if !actions::available_in(action, self.keyboard.context) {
            return self.action_refused("feedback.action.unavailable");
        }
        let listing = self.keyboard.context == flea::keys::Context::Listing;
        // A key here would be held for the re-read and run later, which an outside caller cannot be told, so it is refused.
        let busy = self.panes[self.focus_side]
            .as_ref()
            .is_some_and(|host| host.pane.busy());
        if listing && requests::swallows(busy, action) {
            return self.action_refused("pane.busy");
        }
        let arm_live = self.escape_arm_live(seat);
        // An outside action ends a half-typed pair the way any other key does.
        self.keyboard.disarm();
        self.operations
            .fileops
            .end_for_listing_key(listing, &Dispatch::Action(action));
        let run = self.run_key_action(action, seat, false, arm_live, window, cx);
        cx.notify();
        match run {
            KeyRun::Ran | KeyRun::Kept => ran(),
            KeyRun::Busy => self.action_refused("pane.busy"),
            KeyRun::Unclaimed => self.action_refused("feedback.action.unavailable"),
        }
    }

    pub(crate) fn run_plugin_action(
        &mut self,
        id: String,
        plugin: String,
        action: String,
        waiting: Waiting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.follow_plugins(window, cx);
        let Some(host) = self.plugins.host.clone() else {
            return waiting.answer(unknown(&id));
        };
        let contribution = host
            .read(cx)
            .plugins
            .iter()
            .find(|p| p.manifest.id == plugin && self.plugin_switched_on(p))
            .and_then(|p| contributions::menu_actions(&p.manifest).ok())
            .and_then(|all| all.into_iter().find(|c| c.action == action));
        let Some(contribution) = contribution else {
            return waiting.answer(unknown(&id));
        };
        if self.menu_taken(window, cx) {
            return waiting.answer(self.action_refused("menu.refusal.unavailable"));
        }
        let side = self.focus_side;
        let Some(target) = self.panes[side]
            .as_ref()
            .map(|pane| crate::g2::context_menus::join::identity(pane, side, false))
        else {
            return waiting.answer(self.action_refused("menu.refusal.unavailable"));
        };
        // The menu shows a row only once its plugin answered ready and the kinds database loaded, so both come first.
        host.update(cx, |host, cx| host.prepare("menu-action", cx));
        self.load_menu_mimes(window, cx);
        let deadline = Instant::now() + READY_WAIT;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let ready = this.update(cx, |shell, cx| {
                    let host = host.read(cx);
                    let started = host
                        .plugins
                        .iter()
                        .any(|p| p.manifest.id == plugin && host.healthy(p));
                    started && shell.menu_mimes_loaded()
                });
                match ready {
                    Ok(true) => break,
                    Ok(false) if Instant::now() < deadline => {
                        cx.background_executor().timer(READY_POLL).await
                    }
                    Ok(false) => break,
                    Err(_) => return,
                }
            }
            let _ = this.update_in(cx, |shell, window, cx| {
                // The same menu identity must survive startup; a moved target is refused before capture.
                if shell.focus_side != side
                    || !shell.panes[side].as_ref().is_some_and(|pane| {
                        crate::g2::context_menus::join::identity(pane, side, false) == target
                    })
                {
                    return waiting.answer(shell.action_refused("menu.refusal.moved"));
                }
                shell.open_action_menu(id, &contribution, waiting, window, cx)
            });
        })
        .detach();
    }

    /// The sheet's route into a menu row: a capture of the focused pane, no drawn menu, the row chosen and validated as a click would.
    fn open_action_menu(
        &mut self,
        id: String,
        contribution: &plugins::Contribution,
        waiting: Waiting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_taken(window, cx) {
            return waiting.answer(self.action_refused("menu.refusal.unavailable"));
        }
        let side = self.focus_side;
        // A cursor row outside the held window is still the target, so the file menu refuses rather than fall to the folder.
        let has_row = self.panes[side]
            .as_ref()
            .is_some_and(|host| host.pane.held.total > 0);
        let file = contribution.menus.contains(&Menu::File);
        if (file && has_row) || !contribution.menus.contains(&Menu::Background) {
            self.menu_keyboard_open(window, cx);
        } else {
            self.open_menu(side, None, (0., 0.), window, cx);
        }
        if self.menu.captured.is_none() {
            return waiting.answer(self.action_refused("menu.refusal.unavailable"));
        }
        self.menu.from_sheet = true;
        self.menu.presentation.cancel();
        self.menu.menu.borrow_mut().close();
        self.menu.pending = Some((id, String::new()));
        self.menu.external = Some(waiting);
        self.activate_menu(window, cx);
    }
}
