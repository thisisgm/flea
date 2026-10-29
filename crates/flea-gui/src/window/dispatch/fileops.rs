//! The file operations family: duplicate, Move to Trash, the dd pair, Delete permanently and New Folder, from keys and menu rows.
use super::Order;
use crate::{g2::mn1::join::Chosen, state::Shell, window::input::Dispatch};
use flea::{
    json::escape,
    keys::{sequence::Seat, Context as Keys},
    ui_logic_fb1::status::Hint,
    ui_logic_op1::trash,
};
use gpui::{Context, Window};

// The status line's arm owner for the listing's dd pair; the Trash view's own pair is owner 1.
const ARM_OWNER: u64 = 2;

/// What the family holds between orders: the dd arm and the Delete permanently snapshot each side has out.
#[derive(Default)]
pub(crate) struct Held {
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
    pub(crate) fn armed_at_side(&self, side: usize) -> u64 {
        self.seat
            .filter(|seat| seat.focus == side as u64)
            .map_or(0, |_| self.arm.stamp)
    }
}

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
                "newFolder" => shell.fileops_new_folder(side, None, cx),
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

    /// Focus.js trashRefused: where gio has no Trash, a trash key refuses and names the key that still deletes.
    fn trash_refused(&mut self, side: usize, cx: &mut Context<Self>) -> bool {
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        if flea::ui_logic_pl1::mounts::trashable(&host.path, None) {
            return false;
        }
        let text = self.locale.catalog.tr("feedback.no_trash");
        self.status_messages(cx, |messages, now| {
            messages.say_placed(&text, Hint::Deletes, now)
        });
        cx.notify();
        true
    }

    // Ops.js trash: the marked rows, or the cursor while the filter draws it; none says why.
    fn fileops_trash(&mut self, side: usize, chosen: Option<&Chosen>, cx: &mut Context<Self>) {
        if self.trash_refused(side, cx) {
            return;
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
        let line = format!(
            "{{\"c\":\"trash\",\"rows\":[{}],\"menuId\":{menu},\"listing\":{listing}}}",
            numbers(&rows)
        );
        let anchor = self.deletion_capture(side, &rows);
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

    // Trash.js arm: the first press says what a second press inside ARM_MS does, the second trashes.
    fn fileops_arm(&mut self, side: usize, seat: Seat, cx: &mut Context<Self>) {
        if self.trash_refused(side, cx) {
            return;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let now = host.pane.now();
        let held = &mut self.operations.fileops;
        // A pair belongs to one listing, cursor, view and pane; a press anywhere else arms again.
        if held.seat != Some(seat) {
            held.arm.disarm();
        }
        held.seat = Some(seat);
        if held.arm.press(now) == trash::Press::Act {
            held.seat = None;
            self.status_messages(cx, |messages, now| messages.owner_stamp(0, now));
            return self.fileops_trash(side, None, cx);
        }
        let stamp = held.arm.stamp;
        let rows = self.fileops_targets(side).map_or(0, |(_, rows)| rows.len());
        let prompt = trash::trash_prompt(&self.locale.catalog, &self.fileops_preset(), rows);
        if let Some(host) = self.panes[side].as_mut() {
            host.status = prompt.clone().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.set_arm_owner(ARM_OWNER, stamp, now);
            messages.say_arm(&prompt, trash::ARM_MS, now);
        });
        cx.notify();
    }

    // Pane.qml act: Delete permanently reviews the rows on a snapshot of its own, then opens the permanent-delete card.
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
        self.operations.fileops.delete_anchor[side] = self
            .deletion_capture(side, &rows)
            .map(|anchor| (id, anchor));
        self.fileops_send(side, line, cx);
    }

    // Ops.js newFolder: no name, so the backend takes the first free one.
    fn fileops_new_folder(&mut self, side: usize, chosen: Option<&Chosen>, cx: &mut Context<Self>) {
        // Focus.js:271: Recent holds no folder, so the refusal lands before any mkdir.
        if self.panes[side].as_ref().is_some_and(|host| host.recent.on) {
            let text = self.locale.catalog.tr("places.recent.new_folder_refused");
            if let Some(host) = self.panes[side].as_mut() {
                host.status = text.into();
            }
            cx.notify();
            return;
        }
        let path = match chosen {
            Some(chosen) => chosen.captured.path.clone(),
            None => match self.panes[side].as_ref() {
                Some(host) => host.path.clone(),
                None => return,
            },
        };
        let line = format!("{{\"c\":\"mkdir\",\"path\":\"{}\"}}", escape(&path));
        self.fileops_send(side, line, cx);
    }

    // The rows an action takes, or the sentence that says why it takes none.
    fn fileops_targets(&self, side: usize) -> Result<(usize, Vec<usize>), String> {
        let catalog = &self.locale.catalog;
        let Some(host) = self.panes[side].as_ref() else {
            return Err(catalog.tr("filter.nothing_to_act_on"));
        };
        let held = &host.pane.held;
        let cursor = (held.total > 0).then_some(host.pane.cursor);
        flea::ui_logic_sr2::targets(
            &host.selected,
            host.filter.shown.as_deref(),
            cursor,
            |row| held.row(row).is_some(),
        )
        .map(|rows| (held.listing, rows))
        .map_err(|refusal| crate::g2::sr2::present::refusal(refusal, catalog))
    }

    pub(super) fn fileops_send(&mut self, side: usize, line: String, cx: &mut Context<Self>) {
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
            self.operations.fileops.deleting[side] = None;
            host.record_error(error, &self.locale);
            cx.notify();
        }
    }

    /// A stopped backend ends the side's Delete permanently and its pending rename, so the next Delete says why it cannot run.
    pub(crate) fn fileops_reset(&mut self, side: usize) {
        self.operations.fileops.deleting[side] = None;
        self.operations.fileops.arrival[side] = None;
    }

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
        self.status_messages(cx, |messages, now| {
            messages.say(text, error, "", Hint::None, now)
        });
        cx.notify();
    }

    pub(super) fn fileops_preset(&self) -> String {
        let keys = self.launch.settings.data.get("keys");
        keys.and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default")
            .to_owned()
    }
}

pub(super) fn join(base: &str, name: &str) -> String {
    format!("{}/{name}", base.trim_end_matches('/'))
}

fn numbers(rows: &[usize]) -> String {
    rows.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
