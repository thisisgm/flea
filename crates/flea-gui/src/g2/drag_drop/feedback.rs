//! The held folder target owns the existing row painters and one status activity until leave or release.
use super::*;
use flea::ui_logic_drag_drop::{drop_held, drop_label, line_to, Proposed, Seen, Verb};
use gpui::SharedString;

// Rail and file targets publish one activity for the same held drag.
const LINE: u64 = super::rail_drop::DRAG_LINE;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Target {
    pub(super) side: usize,
    pub(super) listing: usize,
    pub(super) folder: String,
    row: Option<usize>,
    slot: Option<crate::columns::geometry::Slot>,
    dest: String,
}

pub(super) struct Cue {
    pub(super) target: Target,
    serial: u64,
    count: usize,
    flags: Option<(bool, bool)>,
    /// A shelf drag's verb, fixed at its lift whatever keys are held (Drag.feedbackFor).
    fixed: Option<Verb>,
    source_keys: Option<(bool, bool)>,
    pub(super) own: bool,
    proposed: Proposed,
    /// None until the background read lands; a cue paints nothing before it, since unread facts would say copy.
    facts: Option<Facts>,
}

#[path = "feedback_keys.rs"]
mod keys;

impl Shell {
    fn cue_target(
        &self,
        point: Point<gpui::Pixels>,
        own: bool,
        window: &Window,
        cx: &App,
    ) -> Option<Target> {
        if self.input_blocked(cx) {
            return None;
        }
        if let Some((side, _, _, dest)) = self.file_crumb_hit(point, cx) {
            let host = self.panes[side].as_ref()?;
            return Some(Target {
                side,
                listing: host.pane.held.listing,
                folder: host.path.clone(),
                row: None,
                slot: None,
                dest,
            });
        }
        let (side, dest) = self.spring_target(point, own, window, cx)?;
        let host = self.panes[side].as_ref()?;
        let (row, slot) = if host.drawn_view == Some(crate::window::startup::View::Columns) {
            let hover = crate::window::columns::hover_at(host, point)?;
            (hover.index, Some(hover.slot))
        } else {
            let row = host.input.hit(point)?;
            if !folder_row(host.pane.held.row(row)?) {
                return None;
            }
            (row, None)
        };
        Some(Target {
            side,
            listing: host.pane.held.listing,
            folder: host.path.clone(),
            row: Some(row),
            slot,
            dest,
        })
    }

    pub(super) fn aim_file_cue(
        &mut self,
        point: Point<gpui::Pixels>,
        own: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.cue_target(point, own, window, cx) else {
            self.clear_file_cue(window, cx);
            return;
        };
        let key = window_key(window);
        let drags = cx.default_global::<Drags>();
        if drags
            .cue
            .as_ref()
            .is_some_and(|(owner, cue)| *owner == key && cue.target == target)
        {
            self.refresh_file_cue(window, cx);
            return;
        }
        let source = if own {
            drags.lifts.get(&key).and_then(|lift| {
                let paths = lift.paths.as_ref()?;
                if core::refusal(paths, &target.dest, true).is_some() {
                    return None;
                }
                Some((
                    paths.clone(),
                    Some((lift.marker.copy, lift.marker.shift)),
                    None,
                    Proposed::default(),
                ))
            })
        } else {
            drags.hovers.get(&key).and_then(|hover| {
                let hover = hover.borrow();
                let arrived = hover.as_ref()?;
                let marker = match arrived.marker.as_deref() {
                    Some(text) => Some(core::Marker::parse(text)?),
                    None => None,
                };
                let paths =
                    core::sources(&arrived.urls, arrived.plain.as_deref(), marker.is_some());
                if core::refusal(&paths, &target.dest, marker.is_some()).is_some() {
                    return None;
                }
                let proposed = Dropping::native(
                    arrived.marker.as_deref(),
                    &arrived.urls,
                    arrived.plain.as_deref(),
                    arrived.ctrl,
                    arrived.shift,
                    arrived.action,
                )
                .proposed;
                let fixed = arrived
                    .shelf
                    .as_deref()
                    .and_then(core::shelf)
                    .map(|(_, copy)| if copy { Verb::Copy } else { Verb::Move });
                Some((paths, marker.map(|m| (m.copy, m.shift)), fixed, proposed))
            })
        };
        let Some((sources, flags, fixed, proposed)) = source.filter(|source| !source.0.is_empty())
        else {
            self.clear_file_cue(window, cx);
            return;
        };
        let count = sources.len();
        let dest = target.dest.clone();
        drags.serial += 1;
        let serial = drags.serial;
        drags.cue = Some((
            key,
            Cue {
                target,
                serial,
                count,
                flags,
                fixed,
                source_keys: None,
                own,
                proposed,
                facts: None,
            },
        ));
        // The folder left behind gives up its line now; this one says its own once its facts are read.
        self.settle_file_feedback(window, cx);
        cx.refresh_windows();
        let facts = cx
            .background_executor()
            .spawn(async move { facts::read(&sources, &dest) });
        cx.spawn_in(window, async move |this, cx| {
            let facts = facts.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some((owner, cue)) = cx.default_global::<Drags>().cue.as_mut() {
                    if *owner == key && cue.serial == serial {
                        cue.facts = Some(facts);
                        shell.refresh_file_cue(window, cx);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn file_cue<'a>(&self, window: &Window, cx: &'a App) -> Option<&'a Cue> {
        let (key, cue) = cx.try_global::<Drags>()?.cue.as_ref()?;
        let host = self.panes[cue.target.side].as_ref()?;
        (cx.has_active_drag()
            && *key == window_key(window)
            && !self.input_blocked(cx)
            && host.path == cue.target.folder
            && host.pane.held.listing == cue.target.listing
            && !host.pane.busy()
            && cue.facts.is_some_and(|facts| facts.refused.is_none())
            && self
                .cue_target(window.mouse_position(), cue.own, window, cx)
                .is_some_and(|target| target == cue.target))
        .then_some(cue)
    }

    pub(crate) fn file_row_cue(
        &self,
        side: usize,
        window: &Window,
        cx: &App,
    ) -> Option<(usize, SharedString, Verb)> {
        let cue = self.file_cue(window, cx)?;
        let row = cue.target.row?;
        (cue.target.side == side
            && cue
                .target
                .slot
                .is_none_or(|slot| slot == crate::columns::geometry::Slot::Active))
        .then(|| {
            (
                row,
                drop_label(&self.locale.catalog, cue.verb(window)).into(),
                cue.verb(window),
            )
        })
    }

    pub(crate) fn file_column_cue(
        &self,
        side: usize,
        window: &Window,
        cx: &App,
    ) -> Option<(crate::columns::geometry::Slot, usize, SharedString)> {
        let cue = self.file_cue(window, cx)?;
        let row = cue.target.row?;
        (cue.target.side == side).then(|| {
            (
                cue.target
                    .slot
                    .unwrap_or(crate::columns::geometry::Slot::Active),
                row,
                drop_label(&self.locale.catalog, cue.verb(window)).into(),
            )
        })
    }

    pub(crate) fn file_grid_cue(
        &self,
        side: usize,
        window: &Window,
        cx: &App,
    ) -> Option<(usize, crate::grid::element::DropKind)> {
        use crate::grid::element::DropKind;
        self.file_row_cue(side, window, cx).map(|(row, _, verb)| {
            (
                row,
                match verb {
                    Verb::Move => DropKind::Move,
                    Verb::Copy => DropKind::Copy,
                    Verb::Link => DropKind::Link,
                },
            )
        })
    }

    pub(crate) fn refresh_file_cue(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(cue) = self.file_cue(window, cx) else {
            return;
        };
        let name = std::path::Path::new(&cue.target.dest)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&cue.target.dest);
        let line = line_to(&self.locale.catalog, cue.count, name, cue.verb(window));
        cx.default_global::<Drags>()
            .cue_lines
            .insert(window_key(window));
        self.publish_file_line(window, line, cx);
        cx.notify();
    }

    pub(super) fn clear_file_cue(&mut self, window: &Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        let drags = cx.default_global::<Drags>();
        if drags.cue.as_ref().is_some_and(|(owner, _)| *owner == key) {
            drags.cue = None;
        }
        self.settle_file_feedback(window, cx);
    }

    pub(super) fn settle_file_feedback(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !cx.has_active_drag()
            && cx
                .default_global::<Drags>()
                .cue
                .as_ref()
                .is_some_and(|(owner, _)| *owner == window_key(window))
        {
            cx.global_mut::<Drags>().cue = None;
        }
        if self.file_cue(window, cx).is_none()
            && cx
                .default_global::<Drags>()
                .cue_lines
                .remove(&window_key(window))
        {
            // A rail write retires the cached file line, so only its current owner clears it.
            if cx
                .default_global::<Drags>()
                .said
                .contains_key(&window_key(window))
            {
                self.publish_file_line(window, String::new(), cx);
            }
            cx.notify();
        }
    }
}

impl Shell {
    // A running transfer keeps the bar; an unpublished line stays uncached, and an empty line clears.
    fn publish_file_line(&mut self, window: &Window, line: String, cx: &mut Context<Self>) {
        let key = window_key(window);
        let shown = self.status_messages(cx, |messages, _| {
            messages
                .activities()
                .iter()
                .find(|entry| entry.owner == LINE)
                .map(|entry| entry.text.clone())
                .unwrap_or_default()
        });
        let drags = cx.default_global::<Drags>();
        if drags.said.get(&key).map(String::as_str).unwrap_or("") == line && shown == line {
            return;
        }
        let published = self.status_messages(cx, |messages, now| {
            if line.is_empty() || !messages.activities().iter().any(|entry| entry.running) {
                messages.set_activity(LINE, &line, 0, false, false, now);
                true
            } else {
                false
            }
        });
        if !published {
            return;
        }
        let drags = cx.default_global::<Drags>();
        if line.is_empty() {
            drags.said.remove(&key);
        } else {
            drags.said.insert(key, line);
        }
        cx.notify();
    }
}
#[cfg(test)]
#[path = "feedback_owner_tests.rs"]
mod owner_tests;
