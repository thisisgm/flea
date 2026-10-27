//! Rows the file menu runs itself: Add to Shelf as one `flea shelf add` call, and a Tags leaf as one settags toggle.
use super::{delivery::string, join::Captured};
use crate::{state::Shell, window::navigate::Step};
use flea::{
    json::escape,
    jsondoc::Json,
    ui_logic_fb1::status::Hint,
    ui_logic_pl4::tags::{self, Op},
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

    // TagState.qml choose: one toggle over the captured rows; the answer is worded with the tag it named.
    pub(super) fn menu_tag(
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
        self.retire_menu(window, cx);
        let Some(name) = name else {
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
        self.menu.tagging.push((side, name));
        self.menu_send(
            side,
            format!(
                "{{\"c\":\"settags\",\"rows\":[{rows}],\"op\":\"toggle\",\"tag\":\"{}\"}}",
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if string(reply, "t") != "settagged" {
            return false;
        }
        // A settagged answer with no menu toggle waiting on this side belongs to another sender, so it passes on.
        let Some(at) = self.menu.tagging.iter().position(|(s, _)| *s == side) else {
            return false;
        };
        let (_, name) = self.menu.tagging.remove(at);
        let number = |key: &str| reply.get(key).and_then(Json::as_f64).unwrap_or(0.) as i64;
        let skipped = reply
            .get("skipped")
            .and_then(Json::as_array)
            .map_or(0, <[Json]>::len);
        let preset = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(Json::as_str)
            .unwrap_or("default")
            .to_owned();
        let catalog = &self.locale.catalog;
        let (op, ok, shown) = (
            Op::from_wire(string(reply, "op")),
            number("ok"),
            catalog.tr(tags::label_key(name)),
        );
        // A plain change carries the footer's own undo hint; a skip rides inside the line, as Tags.js doneLine words it.
        let (sentence, hint) = if ok > 0 && skipped == 0 {
            (tags::changed_line(catalog, op, ok, &shown), Hint::Undo)
        } else {
            (
                tags::done_line(catalog, &preset, op, ok, &shown, skipped),
                Hint::None,
            )
        };
        if !sentence.is_empty() {
            self.owner_notice(side, &sentence, false, hint, cx);
        }
        let _ = self.step(side, Step::Refresh(None), window, cx);
        true
    }

    /// A refused settags answers this side's oldest toggle, so a later answer keeps its own tag name.
    pub(crate) fn menu_tag_refused(&mut self, side: usize, operation: &str) {
        if operation == "settags" {
            if let Some(at) = self.menu.tagging.iter().position(|(s, _)| *s == side) {
                self.menu.tagging.remove(at);
            }
        }
    }

    fn owner_notice(
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
