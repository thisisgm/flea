//! The opener family: Enter, l, a double click and the menu's Open on a file hand it to flea --open, and its exit picks the next step.
use super::Order;
use crate::{
    g2::op3::lane,
    navigation::history::join,
    state::Shell,
    window::{input::ActionEvent, navigate::Step},
};
use flea::{
    keys::Context as Keys,
    ui_logic_op3::opener::{busy_text, failed_text, Opened, Opener},
    ui_logic_pv1::kinds::{quick_look_kind, State},
};
use gpui::{Context, EventEmitter, Task, Window};
#[path = "opener_subject.rs"]
mod subject;
mod targets;
use subject::{archive_path, file_row};

/// One pane's opener: the single-flight guard, the launch in flight and the launcher it calls off the UI thread.
pub(crate) struct Lane {
    guard: Opener,
    task: Option<Task<()>>,
    // Opener.qml's own terminal Process and RunCard.qml's text launch, each single-flight beside the open.
    terminal: Option<Task<()>>,
    text: Option<Task<()>>,
    pub(super) original: super::original::Pending,
    pub(crate) launch: fn(&[String]) -> lane::Exit,
    pending: Option<targets::Pending>,
    // The selection or archive wait's timeout, so a reply the backend never sends cannot hold the lane.
    wait: Option<Task<()>>,
}

impl Lane {
    /// A selection, gallery or archive wait holds this side's paths answer.
    pub(crate) fn waiting(&self) -> bool {
        self.pending.is_some()
    }
}

impl Default for Lane {
    fn default() -> Self {
        Self {
            guard: Opener::default(),
            task: None,
            terminal: None,
            text: None,
            original: super::original::Pending::default(),
            pending: None,
            wait: None,
            #[cfg(not(test))]
            launch: lane::run,
            // A test window never starts a real application; a test that drives the lane sets its own launcher.
            #[cfg(test)]
            launch: |_| (None, String::new()),
        }
    }
}

/// An answer of flea --open that only a card answers, the run card's ask or Open With, handed to the card host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Asked {
    pub side: usize,
    pub opened: Opened,
}

impl EventEmitter<Asked> for Shell {}

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    match order {
        Order::Action(event)
            if matches!(event.context, Keys::Listing | Keys::Rail)
                && event.action == "openTerminal" =>
        {
            let side = shell.focus_side;
            if let Some(host) = &shell.panes[side] {
                shell.open_terminal(side, host.pane.path().to_owned(), window, cx);
            }
            true
        }
        Order::Action(event)
            if event.context == Keys::Listing && matches!(event.action, "open" | "pageForward") =>
        {
            let side = shell.focus_side;
            if event.action == "open" && shell.open_selection(side, window, cx) {
                return true;
            }
            let Some((path, archive)) = file_row(shell, side) else {
                return false;
            };
            // Focus.js:139: pageForward on a file row is the preview key.
            if event.action == "pageForward" {
                let preview = ActionEvent {
                    action: "preview",
                    ..*event
                };
                shell.dispatch(Order::Action(preview), window, cx);
                return true;
            }
            shell.open_cursor_file(side, path, archive, window, cx);
            true
        }
        Order::Menu(chosen) if chosen.action == "open" && chosen.paths.len() > 1 => {
            if menu_claim(shell, chosen, cx) {
                shell.open_menu_selection(chosen, window, cx);
            }
            true
        }
        // PaneMenuActions.qml perform: the menu's Open on a file opens its one validated path on the menu's own pane.
        Order::Menu(chosen) if chosen.action == "open" && shell.menu.context.row_is_file => {
            if menu_claim(shell, chosen, cx) {
                shell.adopt_menu_target(&chosen.captured);
                shell.keyboard.context = Keys::Listing;
                // The cursor stands on the menu's row now, so the cursor's own file keeps Enter's path.
                shell.focus_side = chosen.captured.side;
                shell.open_menu_target(chosen, window, cx);
            }
            true
        }
        // Focus.js run: Nav.runCursor with the open verb, the same flea --open the menu's Open takes.
        Order::Menu(chosen) if chosen.action == "run" && shell.menu.context.row_is_file => {
            menu_claim(shell, chosen, cx) && {
                let side = chosen.captured.side;
                shell.adopt_menu_target(&chosen.captured);
                shell.keyboard.context = Keys::Listing;
                shell.focus_side = side;
                shell.open_menu_target(chosen, window, cx);
                true
            }
        }
        // Pane.qml runText: the menu's one file goes to flea --open-text.
        Order::Menu(chosen) if chosen.action == "openAsText" && chosen.paths.len() == 1 => {
            menu_claim(shell, chosen, cx) && {
                shell.open_text(chosen.captured.side, chosen.paths[0].clone(), window, cx);
                true
            }
        }
        // A locked menu's denied folder or a row menu's one row, else the folder the menu stands in.
        Order::Menu(chosen) if chosen.action == "openTerminal" => {
            menu_claim(shell, chosen, cx) && {
                let folder = match chosen.paths.as_slice() {
                    [one] => one.clone(),
                    _ => chosen.captured.path.clone(),
                };
                shell.open_terminal(chosen.captured.side, folder, window, cx);
                true
            }
        }
        _ => false,
    }
}

// A claimed row closes its producer snapshot; false when another family already took it.
fn menu_claim(
    shell: &mut Shell,
    chosen: &crate::g2::mn1::join::Chosen,
    cx: &mut Context<Shell>,
) -> bool {
    if !chosen.claim() {
        return false;
    }
    if !chosen.captured.background {
        let close = format!(
            "{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{}}}",
            chosen.captured.id
        );
        shell.menu_send(chosen.captured.side, close, cx);
    }
    true
}

impl Shell {
    // Enter on the cursor's own file: Nav.js:239 hands an archive to Quick Look, an image opens its gallery, any other file flea --open.
    pub(super) fn open_cursor_file(
        &mut self,
        side: usize,
        path: String,
        archive: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if archive {
            self.dispatch(
                Order::Action(ActionEvent::bare("preview", Keys::Listing)),
                window,
                cx,
            );
        } else if !self.open_gallery(side, path.clone(), window, cx) {
            self.open_file(side, path, window, cx);
        }
    }

    /// Opener.qml open: one flea --open per pane at a time; a second press while one is out says so and runs nothing.
    pub(crate) fn open_file(
        &mut self,
        side: usize,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_paths(side, vec![path], false, window, cx);
    }

    fn open_paths(
        &mut self,
        side: usize,
        paths: Vec<String>,
        selection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.opener[side].pending.is_some() {
            let busy = busy_text(&self.locale.catalog);
            self.opener_say(side, busy, false, cx);
            return;
        }
        let flea = crate::window::startup::binary()
            .to_string_lossy()
            .into_owned();
        let lane = &mut self.opener[side];
        let Some(mut argv) = lane.guard.start(&flea, &paths) else {
            let busy = busy_text(&self.locale.catalog);
            self.opener_say(side, busy, false, cx);
            return;
        };
        if selection {
            argv[1] = "--open-selection".into();
        }
        let launch = lane.launch;
        let exit = cx.background_executor().spawn(async move { launch(&argv) });
        lane.task = Some(cx.spawn_in(window, async move |this, cx| {
            let (status, stdout) = exit.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.opener_exited(side, status, &stdout, window, cx)
            });
        }));
    }

    // PaneWire.qml's Opener handlers: a folder navigates, a failure says so, and the two asks go to their cards.
    fn opener_exited(
        &mut self,
        side: usize,
        status: Option<i32>,
        stdout: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.opener[side].task = None;
        let Some(opened) = self.opener[side].guard.exited(status, stdout) else {
            return;
        };
        match opened {
            Opened::Done => {}
            Opened::Directory(path) => {
                let _ = self.step(side, Step::Open(path), window, cx);
            }
            Opened::Failed(_) => {
                let refusal = flea::json::field_str(stdout, "refusal");
                if let Some(key) = refusal.filter(|key| {
                    matches!(
                        key.as_str(),
                        "feedback.refusal.folder_with_files"
                            | "feedback.refusal.archives"
                            | "feedback.refusal.programs"
                            | "feedback.refusal.not_loaded"
                    )
                }) {
                    let sentence = self.locale.catalog.tr(&key);
                    self.opener_say(side, sentence, false, cx);
                    return;
                }
                let failed = failed_text(&self.locale.catalog);
                self.opener_say(side, failed, true, cx);
            }
            Opened::AskRun { path, text } => {
                cx.emit(Asked {
                    side,
                    opened: Opened::AskRun {
                        path: path.clone(),
                        text: text.clone(),
                    },
                });
                self.open_run_card(side, path, text, window.modifiers().shift, cx);
            }
            Opened::AskOpenWith(path) => {
                cx.emit(Asked {
                    side,
                    opened: Opened::AskOpenWith(path.clone()),
                });
                self.open_open_with(side, path, cx);
            }
        }
    }

    /// Opener.qml openTerminal: flea --terminal over the folder; a second ask while one is out says so.
    pub(crate) fn open_terminal(
        &mut self,
        side: usize,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.opener[side].terminal.is_some() {
            let busy = self.locale.catalog.tr("dialogs.file.terminal.busy");
            self.opener_say(side, busy, false, cx);
            return;
        }
        let flea = crate::window::startup::binary()
            .to_string_lossy()
            .into_owned();
        let argv = vec![flea, "--terminal".into(), path];
        let launch = self.opener[side].launch;
        let exit = cx.background_executor().spawn(async move { launch(&argv) });
        self.opener[side].terminal = Some(cx.spawn_in(window, async move |this, cx| {
            let (status, _) = exit.await;
            let _ = this.update(cx, |shell, cx| {
                shell.opener[side].terminal = None;
                if status != Some(0) {
                    let failed = shell.locale.catalog.tr("dialogs.file.terminal.failed");
                    shell.opener_say(side, failed, true, cx);
                }
            });
        }));
    }

    /// RunCard.qml openText: flea --open-text over one file; its exit reads as the run card's Open as Text.
    pub(crate) fn open_text(
        &mut self,
        side: usize,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use flea::ui_logic_op3::runfile::{exit_outcome, Action, Exit};
        if self.opener[side].text.is_some() {
            let busy = self.locale.catalog.tr("dialogs.file.run.busy");
            self.opener_say(side, busy, false, cx);
            return;
        }
        let flea = crate::window::startup::binary()
            .to_string_lossy()
            .into_owned();
        let argv = vec![flea, "--open-text".into(), path];
        let launch = self.opener[side].launch;
        let exit = cx.background_executor().spawn(async move { launch(&argv) });
        self.opener[side].text = Some(cx.spawn_in(window, async move |this, cx| {
            let (status, _) = exit.await;
            let _ = this.update(cx, |shell, cx| {
                shell.opener[side].text = None;
                if let Exit::Message(text) =
                    exit_outcome(&shell.locale.catalog, Action::Text, status)
                {
                    shell.opener_say(side, text, true, cx);
                }
            });
        }));
    }

    pub(crate) fn opener_say(
        &mut self,
        side: usize,
        sentence: String,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = &mut self.panes[side] {
            host.status = sentence.clone().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.say(
                &sentence,
                error,
                "",
                flea::ui_logic_fb1::status::Hint::None,
                now,
            )
        });
        cx.notify();
    }
}
