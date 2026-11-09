//! The window's archive and flash host: menu choices open the cards, every effect reaches the origin backend, replies return by request.
use super::{
    cards::Hit,
    compress::{self, Compress},
    draw::StandIn,
    draw_flash::{CancelTap, ClickTap, DownTap, FlashFrame, FlashTaps, HoverTap, StepTap, UpTap},
    extract::Extract,
    flash::Flash,
    keys, Effect,
};
use crate::{backend::Failure, g2::mn1::join::Chosen, state::Shell};
use flea::{
    jsondoc::{self, Json},
    ui_logic_fb1::status::Hint,
    ui_logic_mn1::build::Context as MenuContext,
    ui_logic_op2::{archive, cards as logic, wire},
};
use gpui::{
    div, prelude::*, AnyElement, ClickEvent, Context, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseUpEvent, Task, Window,
};
use std::{process::Command, rc::Rc, time::Instant};

mod chosen;
mod cover;
mod layer;
mod reply;

/// The status activity owner a handed flash takes, apart from op1's per-side owners 0 and 1.
const HANDED_OWNER: u64 = 2;

/// What one backend reported it can write and read; asked once per backend, before the first menu needs it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Formats {
    pub(crate) asked: bool,
    pub(crate) known: bool,
    pub(crate) archive: Vec<String>,
    pub(crate) caps: archive::Caps,
}

/// One archive card or the flash view at a time, each bound to the backend of the pane that asked.
pub(crate) struct Archive {
    pub(crate) formats: [Formats; 2],
    pub(crate) compress: Option<Compress>,
    pub(crate) extract: Extract,
    pub(crate) flash: Flash,
    /// The side whose backend the open password card sends to, with the menu session it holds open.
    pub(crate) card_side: usize,
    pub(crate) card_menu: Option<u64>,
    pub(crate) flash_side: usize,
    /// Extracts this window asked for, by side, archive and destination; only these may open the password card.
    pub(crate) extracts: Vec<(usize, String, String)>,
    pub(crate) handed: Option<u64>,
    /// The image chooser, `flea flash choose` by default; a test swaps it for a fixture command.
    pub(crate) chooser: fn() -> Command,
    pub(crate) picking: Option<Task<()>>,
    /// Where the open card's password eye last painted, for the pointer and archiveCardsState.
    pub(crate) eye: super::draw::EyeProbe,
    /// Where Compress's confirm eye last painted, for the pointer.
    pub(crate) confirm_eye: super::draw::EyeProbe,
    /// Which step action the flash pane last painted and where, for flashControlCentre.
    pub(crate) flash_action: super::draw_flash::ActionProbe,
    epoch: Option<Instant>,
}

impl Default for Archive {
    fn default() -> Self {
        Self {
            formats: Default::default(),
            compress: None,
            extract: Extract::closed(),
            flash: Flash::closed(),
            card_side: 0,
            card_menu: None,
            flash_side: 0,
            extracts: Vec::new(),
            handed: None,
            chooser: chooser_command,
            picking: None,
            eye: Default::default(),
            confirm_eye: Default::default(),
            flash_action: Default::default(),
            epoch: None,
        }
    }
}

/// Flea's own picker over the download directory with disk image filters first, as v0.3.12's chooser row ran it.
fn chooser_command() -> Command {
    let mut command = Command::new(crate::window::startup::binary());
    command
        .args(["flash", "choose"])
        .env("FLEA_FRONTEND", "gpui")
        .stdin(std::process::Stdio::null());
    command
}

impl Archive {
    pub(crate) fn card_open(&self) -> bool {
        self.compress.as_ref().is_some_and(Compress::opened) || self.extract.opened()
    }

    pub(crate) fn showing(&self) -> bool {
        self.card_open() || self.flash.shown()
    }
}

/// The last line of a failed chooser's stderr, since the renderer banner can come first, or its exit status.
fn failure_reason(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .rfind(|line| !line.trim().is_empty())
        .map_or_else(|| output.status.to_string(), str::to_owned)
}

fn join(base: &str, leaf: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{leaf}")
    } else {
        format!("{base}/{leaf}")
    }
}

fn parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(at) => &path[..at],
        None => "",
    }
}

fn text<'a>(reply: &'a Json, key: &str) -> &'a str {
    reply.get(key).and_then(Json::as_str).unwrap_or("")
}

/// The backend's own failure sentence with its first letter raised, as Errors.sentence words an archive failure.
fn capitalised(message: &str) -> String {
    let mut chars = message.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

impl Shell {
    fn archive_now(&mut self, cx: &Context<Self>) -> u64 {
        let now = cx.background_executor().now();
        let epoch = *self.operations.archive.epoch.get_or_insert(now);
        now.saturating_duration_since(epoch).as_millis() as u64
    }

    /// The menu rows this host needs filled: the probed formats, the archive row and whether it extracts.
    pub(crate) fn archive_menu_context(&mut self, side: usize) {
        let mut context = std::mem::take(&mut self.menu.context);
        self.archive_context(side, &mut context);
        self.menu.context = context;
    }

    /// The same archive facts on a context the caller holds, as the keyboard sheet reads the cursor row's menu.
    pub(crate) fn archive_context(&self, side: usize, context: &mut MenuContext) {
        let formats = &self.operations.archive.formats[side];
        let name = context.row_name.clone();
        context.row_is_archive = context.row_is_file && archive::is_archive(&name);
        context.archive_name = name.clone();
        context.archive_formats = formats.archive.clone();
        // Issue 397: several marked archives extract only when each one does.
        let names = if context.archive_names.is_empty() {
            std::slice::from_ref(&name)
        } else {
            &context.archive_names[..]
        };
        context.can_extract =
            formats.known && names.iter().all(|n| archive::can_extract(n, formats.caps));
    }

    /// A started backend is asked its formats once, before the first menu needs them, as v0.3.12's backend start asked.
    pub(crate) fn archive_ask_formats(&mut self, side: usize, cx: &mut Context<Self>) {
        if self.operations.archive.formats[side].asked {
            return;
        }
        self.operations.archive.formats[side].asked = true;
        self.archive_send(side, "{\"c\":\"formats\",\"id\":1}".into(), cx);
    }

    fn archive_menu_close(&mut self, side: usize, menu: u64, cx: &mut Context<Self>) {
        self.archive_send(
            side,
            format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{menu}}}"),
            cx,
        );
    }

    fn archive_send(&mut self, side: usize, line: String, cx: &mut Context<Self>) -> bool {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return false;
        };
        let result = host
            .backend
            .as_ref()
            .ok_or_else(|| Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            })
            .and_then(|backend| backend.line(line));
        match result {
            Ok(()) => true,
            Err(error) => {
                host.record_error(error, &self.locale);
                cx.notify();
                false
            }
        }
    }

    fn archive_say(&mut self, text: &str, error: bool, cx: &mut Context<Self>) {
        self.status_messages(cx, |messages, now| {
            messages.say(text, error, "", Hint::None, now)
        });
    }

    /// A handed flash run that ended or came back clears its status-line activity.
    fn archive_release_handed(&mut self, cx: &mut Context<Self>) {
        if self.operations.archive.handed.take().is_some() {
            self.status_messages(cx, |messages, now| {
                messages.set_activity(HANDED_OWNER, "", 0, false, false, now)
            });
        }
    }

    /// A dead backend ends the flash its side was running, with the failure ending or its status line.
    pub(crate) fn archive_flash_failed(&mut self, side: usize, cx: &mut Context<Self>) {
        if self.operations.archive.flash_side != side {
            return;
        }
        let catalog = self.locale.catalog.clone();
        for effect in self.operations.archive.flash.on_failed(&catalog) {
            match effect {
                Effect::CloseJob(_) => self.archive_release_handed(cx),
                Effect::Say { text, error } => self.archive_say(&text, error, cx),
                _ => {}
            }
        }
    }

    /// Every effect a card returns, consumed against the backend and the status line of the side it belongs to.
    pub(crate) fn archive_effects(
        &mut self,
        side: usize,
        effects: Vec<Effect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for effect in effects {
            match effect {
                Effect::Send(line) => {
                    if !self.archive_send(side, line, cx) {
                        let stopped = archive::backend_stopped_line(&self.locale.catalog, None);
                        self.archive_say(&stopped, true, cx);
                        self.archive_flash_failed(side, cx);
                    }
                }
                Effect::Close => {}
                Effect::Say { text, error } => self.archive_say(&text, error, cx),
                Effect::HandTransfer(block) => {
                    self.operations.archive.handed = Some(block.id);
                    let line: String = std::iter::once(block.head.clone())
                        .chain(block.byte.iter().map(|(part, _)| part.clone()))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    let id = block.id;
                    self.status_messages(cx, |messages, now| {
                        messages.set_activity(HANDED_OWNER, &line, id, false, false, now)
                    });
                }
                Effect::CloseJob(_) | Effect::TakeBack => self.archive_release_handed(cx),
                Effect::PickImage => self.archive_pick_image(side, window, cx),
            }
        }
        // A card that closed releases the menu session it held, whether it sent or not.
        if !self.operations.archive.card_open() {
            self.operations.archive.compress = None;
            if let Some(menu) = self.operations.archive.card_menu.take() {
                let side = self.operations.archive.card_side;
                self.archive_menu_close(side, menu, cx);
            }
        }
        self.sync_dialog_editor();
        cx.notify();
    }

    fn archive_pick_image(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.operations.archive.picking.is_some() {
            return;
        }
        let mut command = (self.operations.archive.chooser)();
        let binary = command.get_program().to_string_lossy().into_owned();
        let run = cx
            .background_executor()
            .spawn(async move { command.output() });
        self.operations.archive.picking = Some(cx.spawn_in(window, async move |this, cx| {
            let output = run.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.operations.archive.picking = None;
                // An answer the pane no longer waits for is dropped, whichever way the chooser ended.
                let flash = &shell.operations.archive.flash;
                if !flash.shown() || flash.running() || shell.operations.archive.flash_side != side
                {
                    return;
                }
                // A chooser that never started or failed says so; only a cancel, a clean exit naming nothing, stays silent.
                let reason = match &output {
                    Ok(output) if output.status.success() => None,
                    Ok(output) => Some(failure_reason(output)),
                    Err(err) => Some(err.to_string()),
                };
                if let Some(reason) = reason {
                    let line = shell
                        .locale
                        .catalog
                        .tr_args("archive.flash.chooser_failed", &[&binary, &reason]);
                    shell.archive_say(&line, true, cx);
                    return;
                }
                let chosen = output
                    .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
                    .unwrap_or_default();
                if chosen.is_empty() {
                    return;
                }
                let effects = shell.operations.archive.flash.open_with_image(&chosen);
                shell.archive_effects(side, effects, window, cx);
            });
        }));
    }
}
