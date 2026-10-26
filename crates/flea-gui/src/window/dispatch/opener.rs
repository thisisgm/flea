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

/// One pane's opener: the single-flight guard, the launch in flight and the launcher it calls off the UI thread.
pub(crate) struct Lane {
    guard: Opener,
    task: Option<Task<()>>,
    // Opener.qml's own terminal Process and RunCard.qml's text launch, each single-flight beside the open.
    terminal: Option<Task<()>>,
    text: Option<Task<()>>,
    pub(super) original: super::original::Pending,
    pub(crate) launch: fn(&[String]) -> lane::Exit,
}

impl Default for Lane {
    fn default() -> Self {
        Self {
            guard: Opener::default(),
            task: None,
            terminal: None,
            text: None,
            original: super::original::Pending::default(),
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
            let Some((path, archive)) = file_row(shell, side) else {
                return false;
            };
            // Focus.js:139: pageForward on a file row is the preview key, and Nav.js:239 hands an archive to Quick Look.
            if event.action == "pageForward" || archive {
                let preview = ActionEvent {
                    action: "preview",
                    ..*event
                };
                shell.dispatch(Order::Action(preview), window, cx);
                return true;
            }
            shell.open_file(side, path, window, cx);
            true
        }
        // PaneMenuActions.qml perform: the menu's Open on a file runs the listing's own open on the row it stands on.
        Order::Menu(chosen) if chosen.action == "open" && shell.menu.context.row_is_file => {
            if !chosen.claim() {
                return true;
            }
            let side = chosen.captured.side;
            if !chosen.captured.background {
                let close = format!(
                    "{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{}}}",
                    chosen.captured.id
                );
                shell.menu_send(side, close, cx);
            }
            shell.keyboard.context = Keys::Listing;
            // Nav.js openCursor on the menu's own pane: open_menu put its cursor on the clicked row and focused it, so Enter's path runs there.
            shell.focus_side = side;
            shell.dispatch(
                Order::Action(ActionEvent::bare("open", Keys::Listing)),
                window,
                cx,
            );
            true
        }
        // Focus.js run: Nav.runCursor with the open verb, the same flea --open the menu's Open takes.
        Order::Menu(chosen) if chosen.action == "run" && shell.menu.context.row_is_file => {
            menu_claim(shell, chosen, cx) && {
                let side = chosen.captured.side;
                shell.keyboard.context = Keys::Listing;
                shell.focus_side = side;
                shell.dispatch(
                    Order::Action(ActionEvent::bare("open", Keys::Listing)),
                    window,
                    cx,
                );
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
        // Pane.qml openTerminal: the folder the menu stands in, or the denied folder a locked menu delivers as its one path.
        Order::Menu(chosen) if chosen.action == "openTerminal" => {
            menu_claim(shell, chosen, cx) && {
                let folder = match chosen.paths.as_slice() {
                    [locked] if chosen.captured.background => locked.clone(),
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

// The cursor row's path and whether Quick Look owns it, when the pane is idle and the row is a loaded file.
fn file_row(shell: &Shell, side: usize) -> Option<(String, bool)> {
    let host = shell.panes[side].as_ref()?;
    if host.pane.busy() || host.stopped {
        return None;
    }
    let row = host.pane.held.row(host.pane.cursor)?;
    if row.dir {
        return None;
    }
    // Nav.js:239: an AppImage is a program first, so it asks on the run card instead.
    let archive = quick_look_kind(&row.icon, &row.name, false) == State::Archive
        && !row.name.to_ascii_lowercase().ends_with(".appimage");
    Some((join(host.pane.path(), &row.name), archive))
}

impl Shell {
    /// Opener.qml open: one flea --open per pane at a time; a second press while one is out says so and runs nothing.
    pub(crate) fn open_file(
        &mut self,
        side: usize,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let flea = crate::window::startup::binary()
            .to_string_lossy()
            .into_owned();
        let lane = &mut self.opener[side];
        let Some(argv) = lane.guard.start(&flea, &[path]) else {
            let busy = busy_text(&self.locale.catalog);
            self.opener_say(side, busy, false, cx);
            return;
        };
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
                let failed = failed_text(&self.locale.catalog);
                self.opener_say(side, failed, true, cx);
            }
            opened @ (Opened::AskRun { .. } | Opened::AskOpenWith(_)) => {
                cx.emit(Asked { side, opened });
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
