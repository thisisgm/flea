//! The listing's tag selection, bounded read tickets and the session's accepted-row feed.
use super::join::Chosen;
use crate::{list::pane::Held, state::Shell};
use flea::{
    jsondoc::Json,
    ui_logic_mn1::{Entry, Text},
    ui_logic_pl4::tags::{self, State},
};
use gpui::{Context, Window};
use std::{collections::VecDeque, rc::Rc};

#[derive(Default)]
pub struct Tags {
    pub(super) sequence: u64,
    pub(super) asking: Option<Asked>,
    pub(super) pending: [VecDeque<Change>; 2],
    pub(super) refresh: [Option<usize>; 2],
    seen: [Option<Rc<Held>>; 2],
    slots: [Option<(usize, bool)>; 2],
    /// Unpublished tags wait for Places to mount; that service owns the session union.
    unpublished: [bool; 7],
}

pub(super) struct Asked {
    pub id: u64,
    pub menu: u64,
    pub side: usize,
    pub listing: usize,
    pub identity: String,
}

pub(super) struct Change {
    pub tag: &'static str,
    pub listing: usize,
}

pub(super) fn entries(states: &[State; 7]) -> Vec<Entry> {
    let mut rows = flea::ui_logic_mn1::flyouts::tag_entries();
    for (row, state) in rows.iter_mut().zip(states) {
        row.tag_state = Some(*state);
    }
    if states.iter().any(|state| *state != State::Absent) {
        rows.push(Entry::separator());
        rows.push(Entry::leaf("__clear__", Text::Key(tags::CLEAR_LABEL_KEY)).with_glyph("x"));
    }
    rows
}

impl Shell {
    pub(crate) fn tag_entries_for(&self, side: usize, rows: &[usize]) -> Option<Vec<Entry>> {
        let held = &self.panes.get(side)?.as_ref()?.pane.held;
        let files = rows
            .iter()
            .map(|row| {
                held.row(*row)
                    .map(|facts| facts.tags.clone().unwrap_or_default())
            })
            .collect::<Option<Vec<_>>>()?;
        Some(entries(&tags::states_for(&files)))
    }

    pub(crate) fn open_tags(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &self.panes[self.focus_side] else {
            return;
        };
        if self.locked_folder(self.focus_side).is_some() || host.pane.held.total == 0 {
            return;
        }
        self.open_flyout("tags", window, cx);
    }

    pub(crate) fn ask_tag_states(&mut self, again: bool, cx: &mut Context<Self>) {
        let Some(capture) = self
            .menu
            .captured
            .clone()
            .filter(|capture| !capture.background)
        else {
            return;
        };
        if !again && self.tag_entries_for(capture.side, &capture.rows).is_some() {
            return;
        }
        self.menu.tags.sequence += 1;
        let id = self.menu.tags.sequence;
        self.menu.tags.asking = Some(Asked {
            id,
            menu: capture.id,
            side: capture.side,
            listing: capture.listing,
            identity: capture.identity.clone(),
        });
        let rows = capture
            .rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.menu_send(
            capture.side,
            format!(
                r#"{{"c":"tagstates","id":{id},"listing":{},"rows":[{rows}]}}"#,
                capture.listing
            ),
            cx,
        );
    }

    pub(crate) fn choose_tag(
        &mut self,
        chosen: &Chosen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if chosen.action != "tags" || chosen.captured.background || !chosen.captured.ready {
            return false;
        }
        let tag = if chosen.leaf == "__clear__" {
            ""
        } else {
            let Some(slot) = chosen
                .leaf
                .strip_prefix("tag")
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return false;
            };
            let Some(name) = slot.checked_sub(1).and_then(|at| tags::NAMES.get(at)) else {
                return false;
            };
            *name
        };
        if !chosen.claim() {
            return false;
        }
        let capture = &chosen.captured;
        let side = capture.side;
        let valid = self.panes[side].as_ref().is_some_and(|host| {
            !host.stopped && !host.pane.busy() && host.pane.held.listing == capture.listing
        });
        if valid {
            let rows = capture
                .rows
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let op = if tag.is_empty() { "clear" } else { "toggle" };
            self.menu.tags.pending[side].push_back(Change {
                tag,
                listing: capture.listing,
            });
            self.menu_send(
                side,
                format!(
                    r#"{{"c":"settags","listing":{},"rows":[{rows}],"op":"{op}","tag":"{tag}"}}"#,
                    capture.listing
                ),
                cx,
            );
        } else {
            self.menu_notice(side, self.locale.catalog.tr("menu.refusal.moved"), cx);
        }
        self.menu_send(
            side,
            format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, capture.id),
            cx,
        );
        self.retire_menu(window, cx);
        true
    }

    /// Reads only each pane's accepted held snapshot, once per change; producer replies cannot stamp stale rows.
    pub(crate) fn tags_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for side in 0..2 {
            self.refresh_tag_window(side, cx);
        }
        let mut used = self.menu.tags.unpublished;
        let mut changed = false;
        for (side, host) in self.panes.iter().enumerate() {
            let Some(host) = host else { continue };
            let held = &host.pane.held;
            if self.menu.tags.seen[side]
                .as_ref()
                .is_some_and(|seen| Rc::ptr_eq(seen, held))
            {
                continue;
            }
            changed = true;
            for names in held.rows.iter().filter_map(|row| row.tags.as_ref()) {
                for at in names.iter().filter_map(|name| tags::index_of(name)) {
                    used[at] = true;
                }
            }
            let was_latched = self.menu.tags.slots[side]
                .is_some_and(|(listing, latched)| listing == held.listing && latched);
            self.menu.tags.slots[side] = Some((
                held.listing,
                was_latched || held.rows.iter().any(|row| row.tags.is_some()),
            ));
            self.menu.tags.seen[side] = Some(Rc::clone(held));
        }
        if let Some(rail) = &self.rail {
            rail.places
                .update(cx, |producer, cx| producer.tags_seen(&used, cx));
            self.menu.tags.unpublished = [false; 7];
        } else {
            self.menu.tags.unpublished = used;
        }
        if changed {
            self.refresh_tag_rows(window, cx);
        }
    }

    fn refresh_tag_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(capture) = self
            .menu
            .captured
            .as_ref()
            .filter(|capture| capture.identity == self.menu_identity())
        else {
            return;
        };
        let side = capture.side;
        let Some(rows) = self.tag_entries_for(side, &capture.rows) else {
            return;
        };
        if self.menu.context.tag_submenu.as_ref() == Some(&rows) {
            return;
        }
        self.menu.context.tag_submenu = Some(rows);
        self.menu.tags.asking = None;
        if self.menu_opened() {
            let entries = self.menu_entries();
            let list = self.panes[side].as_ref().unwrap().list.borrow();
            self.menu
                .menu
                .borrow_mut()
                .refresh(entries, &self.menu_env(&list.typeface), window);
        } else {
            self.refresh_menu(window, cx);
        }
        cx.notify();
    }

    /// The folder-wide reserved slot survives an untagged held window until the listing changes.
    pub(crate) fn tag_slot(&self, side: usize, listing: usize) -> bool {
        self.menu.tags.slots[side].is_some_and(|(seen, latched)| seen == listing && latched)
    }

    pub(super) fn tag_preset(&self) -> &str {
        self.launch
            .settings
            .data
            .get("keys")
            .and_then(Json::as_str)
            .unwrap_or("default")
    }
}
