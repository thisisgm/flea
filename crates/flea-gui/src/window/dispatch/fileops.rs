//! The file operations family: duplicate, Move to Trash, the dd pair, Delete permanently and New Folder, from keys and menu rows.
use super::Order;
use crate::{g2::context_menus::join::Chosen, state::Shell, window::input::Dispatch};
use flea::ui_logic_status_feedback::status::Hint;
use flea::{
    json::escape,
    keys::{sequence::Seat, Context as Keys},
    ui_logic_file_ops::trash,
};
use gpui::{Context, Window};

// The status line's arm owner for the listing's dd pair; the Trash view's own pair is owner 1.
const ARM_OWNER: u64 = 2;

#[path = "fileops_route.rs"]
mod route;

/// What the family holds between orders: the dd arm and the Delete permanently snapshot each side has out.
#[derive(Default)]
pub(crate) struct Held {
    pub(super) creations: super::fileops_created::Targets,
    arm: trash::Arm,
    seat: Option<Seat>,
    pub(super) deleting: [Option<u64>; 2],
    /// The folder New Folder made on each side, renamed once its row lands.
    pub(super) arrival: [Option<String>; 2],
    pub(super) trash_anchors:
        [std::collections::VecDeque<Option<crate::navigation::anchor::Anchor>>; 2],
    pub(super) delete_anchor: [Option<(u64, crate::navigation::anchor::Anchor)>; 2],
}

impl Held {
    /// The dd arm's stamp, 0 while disarmed, as Pane.qml's trashArmedAt reads it.
    pub(crate) fn armed_at(&self) -> u64 {
        self.arm.stamp
    }

    /// Focus.js: any action but the second d ends the pair, including one a key-only handler takes.
    pub(crate) fn disarm(&mut self) {
        self.arm.disarm();
    }

    /// Focus.js: every listing key but the second d ends the dd arm, an unbound one too, before any handler can take it.
    pub(crate) fn end_for_listing_key(&mut self, listing: bool, dispatch: &Dispatch) {
        if listing
            && !matches!(
                dispatch,
                Dispatch::Composition | Dispatch::Action("trashArm")
            )
        {
            self.disarm();
        }
    }
}

impl Held {
    /// The dd arm's stamp for this side only, 0 when the seat sits elsewhere
    pub(crate) fn armed_at_side(&self, side: usize) -> u64 {
        self.seat
            .filter(|seat| seat.focus == side as u64)
            .map_or(0, |_| self.arm.stamp)
    }
}

/// Runs a listing action or a menu row for this family; false when another family owns it
pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    match order {
        Order::Action(event) if event.context == Keys::Listing => {
            // Focus.js: every other action ends an armed pair, so the arm never survives the key after it.
            if event.action != "trashArm" {
                shell.operations.fileops.arm.disarm();
            }
            let side = shell.focus_side;
            match event.action {
                "duplicate" => shell.fileops_duplicate(side, None, cx),
                "trash" => shell.fileops_trash(side, None, cx),
                "trashArm" => shell.fileops_arm(side, event.seat, cx),
                "deletePermanently" => shell.fileops_delete(side, None, cx),
                // The TUI's newFolder is this prompt too: one card, and the name decides the kind.
                "newFolder" | "newFile" => shell.fileops_new_file(side, window, cx),
                _ => return false,
            }
            true
        }
        Order::Menu(chosen) => shell.fileops_menu(chosen, window, cx),
        _ => false,
    }
}

impl Shell {
    // A menu row runs on the captured snapshot, so the backend checks the rows the menu showed.
    /// Runs the menu's trash, duplicate, delete or New Folder row once its snapshot still allows it
    fn fileops_menu(&mut self, chosen: &Chosen, _: &mut Window, cx: &mut Context<Self>) -> bool {
        let captured = &chosen.captured;
        let side = captured.side;
        let row_action = matches!(
            chosen.action.as_str(),
            "trash" | "duplicate" | "deletePermanently"
        );
        let wanted = match chosen.action.as_str() {
            _ if row_action => captured.ready && !captured.background,
            "newFolder" => captured.background,
            _ => false,
        };
        if !wanted || self.panes.get(side).and_then(Option::as_ref).is_none() || !chosen.claim() {
            return false;
        }
        match chosen.action.as_str() {
            "trash" => self.fileops_trash(side, Some(chosen), cx),
            "duplicate" => self.fileops_duplicate(side, Some(chosen), cx),
            "deletePermanently" => self.fileops_delete(side, Some(chosen), cx),
            _ => self.fileops_new_folder(side, Some(chosen), cx),
        }
        true
    }

    // Ops.js duplicate: the cursor row, or the menu's one row; no row is quiet.
    /// Duplicates the cursor row, or the menu's row, and sends the request
    fn fileops_duplicate(&mut self, side: usize, chosen: Option<&Chosen>, cx: &mut Context<Self>) {
        let (path, menu) = match chosen {
            Some(chosen) => match chosen.paths.first() {
                Some(path) => (path.clone(), chosen.captured.id),
                None => return,
            },
            None => {
                let Some(host) = self.panes[side].as_ref() else {
                    return;
                };
                let Some(row) = host.pane.held.row(host.pane.cursor) else {
                    return;
                };
                (join(&host.path, &row.name), 0)
            }
        };
        let line = format!(
            "{{\"c\":\"duplicate\",\"path\":\"{}\",\"menuId\":{menu}}}",
            escape(&path)
        );
        self.fileops_send(side, line, cx);
    }

    /// Whether a dd arm or a vim pair is live before this key ends it; Focus.js escapeCancelsArm reads it first.
    pub(crate) fn escape_arm_live(&self, seat: Seat) -> bool {
        let pending = self.keyboard.pending();
        self.operations.fileops.armed_at() > 0 || pending.is_some_and(|(_, held)| held == seat)
    }

    /// Focus.js escapeCancelsArm: with escape-up on, Escape ends a live dd arm or vim pair and stops, before it climbs.
    pub(crate) fn escape_cancels_arm(&mut self, arm_live: bool, cx: &mut Context<Self>) -> bool {
        if !arm_live || !self.escape_up_on() {
            return false;
        }
        self.operations.fileops.disarm();
        self.keyboard.disarm();
        self.status_messages(cx, |messages, now| messages.owner_stamp(0, now));
        cx.notify();
        cx.stop_propagation();
        true
    }

    // Ops.js trash: the marked rows, or the cursor while the filter draws it; none says why.
    /// Trashes the marked rows, or the cursor row, and keeps the anchor to restore them by
    fn fileops_trash(&mut self, side: usize, chosen: Option<&Chosen>, cx: &mut Context<Self>) {
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let base = chosen.map_or(host.path.as_str(), |chosen| chosen.captured.path.as_str());
        match self.fileops_route(side, base) {
            trash::Route::Refuse => return self.fileops_no_trash(side, cx),
            trash::Route::ConfirmPermanent => return self.fileops_delete(side, chosen, cx),
            trash::Route::Trash => {}
        }
        let (listing, rows, menu) = match chosen {
            Some(chosen) => (
                chosen.captured.listing,
                chosen.captured.rows.clone(),
                chosen.captured.id,
            ),
            None => match self.fileops_targets(side) {
                Ok((listing, rows)) => (listing, rows, 0),
                Err(text) => return self.fileops_say(side, &text, false, cx),
            },
        };
        let paths = chosen
            .filter(|chosen| chosen.captured.external(host))
            .map_or_else(String::new, |chosen| {
                format!(",\"paths\":{}", flea::json::string_array(&chosen.paths))
            });
        let line = format!(
            "{{\"c\":\"trash\",\"rows\":[{}],\"menuId\":{menu},\"listing\":{listing}{paths}}}",
            numbers(&rows)
        );
        let external = chosen.is_some_and(|chosen| chosen.captured.external(host));
        let anchor = if external {
            None
        } else {
            self.deletion_capture(side, &rows)
        };
        self.operations.fileops.trash_anchors[side].push_back(anchor);
        self.fileops_send(side, line, cx);
    }

    /// A row drag released on the rail's Trash (TrashDrop.drop): dd's request by path, with dd's cursor anchor while the rows are still listed.
    pub(crate) fn trash_dropped(
        &mut self,
        side: usize,
        line: String,
        rows: &[usize],
        current: bool,
        cx: &mut Context<Self>,
    ) {
        let anchor = current.then(|| self.deletion_capture(side, rows)).flatten();
        self.operations.fileops.trash_anchors[side].push_back(anchor);
        self.fileops_send(side, line, cx);
    }

    // Pane.qml act: Delete permanently reviews the rows on a snapshot of its own, then opens the permanent-delete card.
    /// Reviews Delete permanently's rows on a snapshot of its own, or says one is already finishing
    fn fileops_delete(&mut self, side: usize, chosen: Option<&Chosen>, cx: &mut Context<Self>) {
        if self.operations.fileops.deleting[side].is_some() {
            if let Some(chosen) = chosen.filter(|chosen| !chosen.captured.background) {
                let id = chosen.captured.id;
                let close = format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{id}}}");
                self.fileops_send(side, close, cx);
            }
            let text = self.locale.catalog.tr("jobs.delete.finishing");
            return self.fileops_say(side, &text, false, cx);
        }
        let line = match chosen {
            Some(chosen) => {
                let id = chosen.captured.id;
                self.operations.fileops.deleting[side] = Some(id);
                format!("{{\"c\":\"menuaction\",\"op\":\"prepareDelete\",\"id\":{id}}}")
            }
            None => {
                let (listing, rows) = match self.fileops_targets(side) {
                    Ok(targets) => targets,
                    Err(text) => return self.fileops_say(side, &text, false, cx),
                };
                let cursor = self.panes[side].as_ref().map_or(0, |host| host.pane.cursor);
                self.menu.sequence += 1;
                let id = self.menu.sequence;
                self.operations.fileops.deleting[side] = Some(id);
                format!(
                    "{{\"c\":\"menuaction\",\"op\":\"snapshot\",\"id\":{id},\"listing\":{listing},\"rows\":[{}],\"cursor\":{cursor}}}",
                    numbers(&rows)
                )
            }
        };
        let rows = match chosen {
            Some(chosen) => chosen.captured.rows.clone(),
            None => self
                .fileops_targets(side)
                .map_or_else(|_| Vec::new(), |(_, rows)| rows),
        };
        let id = self.operations.fileops.deleting[side].expect("accepted delete snapshot");
        let external = chosen.is_some_and(|chosen| {
            self.panes[side]
                .as_ref()
                .is_some_and(|host| chosen.captured.external(host))
        });
        self.operations.fileops.delete_anchor[side] = if external {
            None
        } else {
            self.deletion_capture(side, &rows)
                .map(|anchor| (id, anchor))
        };
        self.fileops_send(side, line, cx);
    }

    /// Ctrl+Shift+N's create card: the background menu's New File row, reached without the menu. The
    /// suggestion is a folder's, so Enter on an untouched name still makes one, as this key always has.
    pub(crate) fn fileops_new_file(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.recent_refuses(side, "places.recent.new_folder_refused", cx) {
            return;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        if host.backend.is_none() {
            return;
        }
        let folder = host.path.clone();
        let Some(view) = self.operation_overlay(side, window, cx) else {
            return;
        };
        let Some(id) = self.fileops_creation_requested(side, "newFile", &folder, cx) else {
            return;
        };
        let base = self.locale.catalog.tr("prompt.newFolder");
        view.update(cx, |overlay, cx| {
            overlay.new_file(id, &folder, Some(&base), cx)
        });
        self.show_operation(&view, cx);
        // A key-opened card takes the keyboard at once, remembering the listing so closing it gives the keys back.
        let shell_focus = self.focus.clone();
        window.focus(&shell_focus, cx);
        view.update(cx, |overlay, cx| overlay.acquire_mount_focus(window, cx));
        self.flush_operations(side, cx);
    }

    // The backend takes the first free name from the client catalog's suggestion.
    /// Asks the backend for a folder named from the catalog's base
    fn fileops_new_folder(&mut self, side: usize, chosen: Option<&Chosen>, cx: &mut Context<Self>) {
        // Focus.js:271: Recent holds no folder, so the refusal lands before any mkdir.
        if self.recent_refuses(side, "places.recent.new_folder_refused", cx) {
            return;
        }
        let path = match chosen {
            Some(chosen) => chosen.captured.path.clone(),
            None => match self.panes[side].as_ref() {
                Some(host) => host.path.clone(),
                None => return,
            },
        };
        let base = self.locale.catalog.tr("prompt.newFolder");
        let line = format!(
            "{{\"c\":\"mkdir\",\"path\":\"{}\",\"defaultBase\":\"{}\"}}",
            escape(&path),
            escape(&base)
        );
        self.fileops_send(side, line, cx);
    }

    // The rows an action takes, or the sentence that says why it takes none.
    /// Resolves an action's rows through the selection and filter, or the refusal sentence
    fn fileops_targets(&self, side: usize) -> Result<(usize, Vec<usize>), String> {
        let catalog = &self.locale.catalog;
        let Some(host) = self.panes[side].as_ref() else {
            return Err(catalog.tr("filter.nothing_to_act_on"));
        };
        let held = &host.pane.held;
        let cursor = (held.total > 0).then_some(host.pane.cursor);
        flea::ui_logic_listing_filter::targets(
            &host.selected,
            host.filter.shown.as_deref(),
            cursor,
            |row| held.row(row).is_some(),
        )
        .map(|rows| (held.listing, rows))
        .map_err(|refusal| crate::g2::listing_filter::present::refusal(refusal, catalog))
    }

    /// Puts a creation on the wire under its request token, recording a failure in the pane
    pub(super) fn fileops_send(&mut self, side: usize, line: String, cx: &mut Context<Self>) {
        let Some(line) = self.fileops_creation_request(side, line, cx) else {
            return;
        };
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let result = match &host.backend {
            Some(backend) => backend.operation(line),
            None => Err(crate::backend::Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            }),
        };
        if let Err(error) = result {
            self.operations.fileops.creations.clear(side);
            self.operations.fileops.deleting[side] = None;
            host.record_error(error, &self.locale);
            cx.notify();
        }
    }

    /// A stopped backend ends the side's Delete permanently and its pending rename, so the next Delete says why it cannot run.
    pub(crate) fn fileops_reset(&mut self, side: usize) {
        self.operations.fileops.creations.clear(side);
        self.operations.fileops.deleting[side] = None;
        self.operations.fileops.arrival[side] = None;
    }

    /// Says a sentence on the pane and on its status lane
    pub(crate) fn fileops_say(
        &mut self,
        side: usize,
        text: &str,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.panes[side].as_mut() {
            host.status = text.to_owned().into();
        }
        let catalog = self.locale.catalog.clone();
        self.status_messages(cx, |messages, now| {
            messages.say_receipt(&catalog, text, error, "", now)
        });
        cx.notify();
    }

    /// A receipt's undo tail draws in the hint lane (Status.js hintOf); an error keeps its sentence whole.
    pub(crate) fn split_undo(&self, text: &str, error: bool) -> (String, Hint) {
        if error {
            return (text.to_owned(), Hint::None);
        }
        let preset = self.fileops_preset();
        flea::ui_logic_status_feedback::status::split_undo(text, &self.locale.catalog, &preset)
    }

    /// The keys preset this launch runs under
    pub(super) fn fileops_preset(&self) -> String {
        let keys = self.launch.settings.data.get("keys");
        keys.and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default")
            .to_owned()
    }
}

/// Appends a name to a base path without doubling the slash
pub(super) fn join(base: &str, name: &str) -> String {
    format!("{}/{name}", base.trim_end_matches('/'))
}

/// The row numbers as the comma-joined array body a request wants
fn numbers(rows: &[usize]) -> String {
    rows.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
