//! Rows the file menu runs itself: Add to Shelf as one `flea shelf add` call, a Tags leaf as one settags toggle, and an Open With leaf.
use super::{delivery::string, join::Captured};
use crate::state::Shell;
use flea::{
    json::escape, jsondoc::Json, ui_logic_fb1::status::Hint, ui_logic_mn1::rows::OPEN_WITH_OTHER,
    ui_logic_pl4::tags,
};
use gpui::{Context, Window};
use std::path::Path;

/// Runs one shelf add over its paths: Ok, or the shelf's own last stderr line ("" when it said nothing).
pub type ShelfAdd = fn(&[String]) -> Result<(), String>;

// PaneMenuActions.qml shelver: the same `flea shelf add` a drop onto the card makes.
fn shelf_add(paths: &[String]) -> Result<(), String> {
    let output = std::process::Command::new(crate::window::startup::binary())
        .args(["shelf", "add"])
        .args(paths)
        .output()
        .map_err(|_| String::new())?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let last = said.trim().lines().last().unwrap_or("");
    Err(last.strip_prefix("flea: ").unwrap_or(last).to_owned())
}

impl Shell {
    // PaneMenuActions.qml targets: a menu without a selection means the cursor row under the listing's base.
    fn menu_targets(&self, capture: &Captured, paths: &[String]) -> Vec<String> {
        if !paths.is_empty() {
            return paths.to_vec();
        }
        self.panes[capture.side]
            .as_ref()
            .and_then(|host| host.pane.held.row(capture.cursor))
            .map(|row| {
                Path::new(&capture.path)
                    .join(&row.name)
                    .to_string_lossy()
                    .into_owned()
            })
            .into_iter()
            .collect()
    }

    pub(super) fn menu_shelve(
        &mut self,
        capture: &Captured,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = capture.side;
        let paths = self.menu_targets(capture, paths);
        self.retire_menu(window, cx);
        if paths.is_empty() {
            let sentence = self.locale.catalog.tr("menu.shelf.nothing");
            self.owner_notice(side, &sentence, true, Hint::None, cx);
            return;
        }
        let add = self.menu.shelve.unwrap_or(shelf_add);
        let count = paths.len();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { add(&paths) })
                .await;
            let _ = this.update(cx, |shell, cx| {
                let catalog = &shell.locale.catalog;
                let (sentence, error) = match result {
                    Ok(()) if count == 1 => (catalog.tr("menu.shelf.added_one"), false),
                    Ok(()) => (
                        catalog.tr_args("menu.shelf.added_many", &[&count.to_string()]),
                        false,
                    ),
                    Err(said) if !said.is_empty() => (said, true),
                    Err(_) => (catalog.tr("menu.shelf.refused"), true),
                };
                shell.owner_notice(side, &sentence, error, Hint::None, cx);
            });
        })
        .detach();
    }

    // PaneMenuActions.qml validateActivation: a flyout application launches through the registry op the card submits, and the tail row opens the card.
    pub(super) fn menu_open_with(
        &mut self,
        capture: &Captured,
        leaf: &str,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = capture.side;
        if leaf == OPEN_WITH_OTHER {
            let path = self.menu_targets(capture, paths).into_iter().next();
            self.retire_menu(window, cx);
            if let Some(path) = path {
                self.open_open_with(side, path, cx);
            }
            return;
        }
        // The launch reads the menu's snapshot, so the snapshot stays open until its answer closes it.
        self.menu_send(
            side,
            format!(
                "{{\"c\":\"menuaction\",\"op\":\"openWith\",\"id\":{},\"application\":\"{}\"}}",
                capture.id,
                escape(leaf)
            ),
            cx,
        );
        self.menu.launching.push((side, capture.id));
        self.menu.captured = None;
        self.retire_menu(window, cx);
        self.free_menu_gate(side, window, cx);
    }

    /// The answer to a flyout launch: close its snapshot, and say why when the application did not open.
    pub(super) fn menu_launched(
        &mut self,
        side: usize,
        id: u64,
        reply: &Json,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(at) = self
            .menu
            .launching
            .iter()
            .position(|&launch| launch == (side, id))
        else {
            return false;
        };
        self.menu.launching.remove(at);
        self.menu_send(
            side,
            format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{id}}}"),
            cx,
        );
        let flag = |key| reply.get(key).and_then(Json::as_bool) == Some(true);
        if !flag("ok") && !flag("cancelled") {
            let said = string(reply, "error");
            let sentence = if said.is_empty() {
                self.locale.catalog.tr("dialogs.file.openwith.open_failed")
            } else {
                said.to_owned()
            };
            self.owner_notice(side, &sentence, true, Hint::None, cx);
        }
        true
    }

    // Only a validated choice completes the current menu's activation; a legacy Order::Menu write cannot release it.
    pub(super) fn menu_tag(
        &mut self,
        capture: &Captured,
        leaf: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu.activating = false;
        self.send_menu_tag(capture, leaf, window, cx);
        if self
            .menu
            .captured
            .as_ref()
            .is_some_and(|current| current.id == capture.id)
        {
            self.menu.pending = self.menu.tags.choices.pop_front();
            self.activate_menu(window, cx);
        }
    }

    // TagState.qml choose: one toggle over the captured rows; the answer is worded with the tag it named.
    pub(super) fn send_menu_tag(
        &mut self,
        capture: &Captured,
        leaf: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = capture.side;
        let name = leaf
            .strip_prefix("tag")
            .and_then(|slot| slot.parse::<usize>().ok())
            .and_then(|slot| tags::NAMES.get(slot.wrapping_sub(1)));
        let name = if leaf == "__clear__" {
            Some("")
        } else {
            name.copied()
        };
        let Some(name) = name else {
            self.retire_menu(window, cx);
            let sentence = self.locale.catalog.tr("menu.refusal.unavailable");
            self.owner_notice(side, &sentence, true, Hint::None, cx);
            return;
        };
        let rows = if capture.rows.is_empty() {
            vec![capture.cursor]
        } else {
            capture.rows.clone()
        };
        let rows = rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let op = if name.is_empty() { "clear" } else { "toggle" };
        self.menu.tags.pending[side].push_back(super::tags::Change {
            tag: name,
            listing: capture.listing,
        });
        self.menu_send(
            side,
            format!(
                "{{\"c\":\"settags\",\"listing\":{},\"rows\":[{rows}],\"op\":\"{op}\",\"tag\":\"{}\"}}",
                capture.listing,
                escape(name)
            ),
            cx,
        );
    }

    /// TagState.qml setTaggedDone: a settagged answer lands this side's oldest toggle, says it and redraws the rows.
    pub(super) fn menu_tag_line(
        &mut self,
        side: usize,
        reply: &Json,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if string(reply, "t") != "settagged" {
            return false;
        }
        if self.menu.tags.pending[side].is_empty() {
            return false;
        }
        self.tag_write_done(side, reply, cx);
        true
    }

    pub(crate) fn owner_notice(
        &mut self,
        side: usize,
        sentence: &str,
        error: bool,
        hint: Hint,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = &mut self.panes[side] {
            host.status = sentence.to_owned().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.say(sentence, error, "", hint, now);
        });
        cx.notify();
    }
}
