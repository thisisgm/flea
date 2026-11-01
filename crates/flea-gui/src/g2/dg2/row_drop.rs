//! Folder-row feedback for another window's drag and the source line while its native lift is away.
use super::super::files::Hover;
use super::rail_drop::DRAG_LINE;
use super::*;
use crate::window::startup::View;
use flea::ui_logic_dg1::row_path;
use flea::ui_logic_dg2::{drop_label, line_to, line_to_folder, PasteAsk, Verb};
use gpui::{ExternalPaths, SharedString};

/// A List pane's folder row under a held drag, and the folder a release there lands in.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    side: usize,
    listing: usize,
    index: usize,
    name: String,
    dest: String,
}

/// What a held drag rests on in this window, the disk facts once read, and the caption the row draws.
pub(super) struct RowAim {
    row: Option<Row>,
    /// The paths another window's drag carries; None for this window's own lift.
    foreign: Option<Vec<String>>,
    facts: Option<Facts>,
    serial: u64,
    label: Option<SharedString>,
}

// The verb a decided drop asks op1 for, as Drag.label and Drag.line name it.
fn verb_of(landing: Landing) -> Option<Verb> {
    match landing {
        Landing::Ask(PasteAsk::Link { .. }) => Some(Verb::Link),
        Landing::Ask(PasteAsk::Transfer { op, .. }) if op == "move" => Some(Verb::Move),
        Landing::Ask(PasteAsk::Transfer { .. }) => Some(Verb::Copy),
        _ => None,
    }
}

// The uri-list another window's drag offered on entering, else its paths as file URIs.
fn foreign_urls(hover: Option<&Arrived>, paths: &[String]) -> Vec<String> {
    match hover {
        Some(hover) if !hover.urls.is_empty() => hover.urls.clone(),
        _ => paths.iter().map(|path| core::uri_for(path)).collect(),
    }
}

impl Shell {
    /// Source-window key changes update the shared native offer before any destination recomputes its cue.
    pub(crate) fn root_modifiers_changed(
        &mut self,
        event: &gpui::ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_card_modifiers(event, window, cx);
        let Some(drags) = cx.try_global::<Drags>() else {
            return;
        };
        for hover in drags.hovers.values() {
            if let Some(hover) = hover.borrow_mut().as_mut() {
                hover.ctrl = event.modifiers.control;
                hover.shift = event.modifiers.shift;
            }
        }
        self.refresh_row_drag_cue(window, cx);
        for handle in cx.windows() {
            if handle.window_id().as_u64() == window_key(window) {
                continue;
            }
            if let Some(handle) = handle.downcast::<Self>() {
                let _ = handle.update(cx, |shell, window, cx| {
                    shell.refresh_row_drag_cue(window, cx);
                });
            }
        }
    }

    fn refresh_row_drag_cue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cx
            .try_global::<Drags>()
            .is_some_and(|drags| drags.aims.contains_key(&window_key(window)))
        {
            self.aim_rail_drag(window.mouse_position(), window, cx);
        } else {
            self.say_row_aim(window, cx);
        }
    }

    /// Another window's file drag reaches this listener on every move over this window, wherever the pointer is.
    pub(crate) fn aim_rows_on_foreign_drag(
        div: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        gpui::InteractiveElement::on_drag_move::<ExternalPaths>(
            div,
            cx.listener(
                |shell, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                    let paths = event
                        .drag(cx)
                        .paths()
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect();
                    shell.aim_row_drag(event.event.position, Some(paths), window, cx);
                },
            ),
        )
    }

    // The folder row of a List pane under point, for another window's drag.
    fn row_under(&self, point: Point<gpui::Pixels>) -> Option<Row> {
        let side = (0..self.panes.len()).find(|side| {
            self.panes[*side]
                .as_ref()
                .and_then(|host| host.input.frame)
                .is_some_and(|frame| frame.bounds.contains(&point))
        })?;
        let host = self.panes[side].as_ref()?;
        if !matches!(host.drawn_view, Some(View::List | View::Dual)) || host.pane.busy() {
            return None;
        }
        let index = host.input.hit(point)?;
        let listing = host.pane.held.listing;
        let row = host.pane.held.row(index).filter(|row| row.dir)?;
        Some(Row {
            side,
            listing,
            index,
            name: row.name.clone(),
            dest: row_path(&host.path, &row.name),
        })
    }

    /// A held drag moved over this window: foreign holds the paths another window's drag carries, None for this window's own lift.
    pub(crate) fn aim_row_drag(
        &mut self,
        point: Point<gpui::Pixels>,
        foreign: Option<Vec<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = window_key(window);
        let lift = cx
            .try_global::<Drags>()
            .and_then(|drags| drags.lifts.get(&key));
        if foreign.is_none() && lift.is_none() {
            self.clear_row_aim(window, cx);
            return;
        }
        let first = match &foreign {
            None => lift.and_then(|lift| lift.paths.as_ref()?.first().cloned()),
            Some(paths) => {
                let hover = cx
                    .try_global::<Drags>()
                    .and_then(|drags| drags.hovers.get(&key))
                    .and_then(|hover| hover.borrow().clone());
                foreign_urls(hover.as_ref(), paths)
                    .iter()
                    .find_map(|url| core::file_path(url))
            }
        };
        let row = foreign.as_ref().and_then(|_| self.row_under(point));
        let drags = cx.default_global::<Drags>();
        if let Some(aim) = drags.rows.get(&key) {
            if aim.row == row && aim.foreign == foreign {
                self.say_row_aim(window, cx);
                return;
            }
        }
        drags.serial += 1;
        let serial = drags.serial;
        let dest = row.as_ref().map(|row| row.dest.clone());
        drags.rows.insert(
            key,
            RowAim {
                row,
                foreign,
                facts: None,
                serial,
                label: None,
            },
        );
        self.say_row_aim(window, cx);
        // As the rail does, the folder's device is read before the verb is said; a later aim supersedes this answer.
        let Some(dest) = dest else {
            return;
        };
        let facts = cx
            .background_executor()
            .spawn(async move { facts::read(first.as_deref(), &dest) });
        cx.spawn_in(window, async move |this, cx| {
            let facts = facts.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let key = window_key(window);
                let Some(aim) = cx.default_global::<Drags>().rows.get_mut(&key) else {
                    return;
                };
                if aim.serial == serial {
                    aim.facts = Some(facts);
                    shell.say_row_aim(window, cx);
                }
            });
        })
        .detach();
    }

    // FileDrag.showTarget and leaveTarget: the folder row's caption and the line from the keys held now; this window's own lift on no folder says the line without a name.
    fn say_row_aim(&mut self, window: &Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        let modifiers = window.modifiers();
        let catalog = self.locale.catalog.clone();
        let Some(drags) = cx.try_global::<Drags>() else {
            return;
        };
        let Some(aim) = drags.rows.get(&key) else {
            return;
        };
        let own = drags.lifts.get(&key);
        let hover = drags
            .hovers
            .get(&key)
            .and_then(|hover| hover.borrow().clone());
        let said = match (&aim.row, &aim.foreign) {
            (None, None) => own.map(|lift| {
                let line = line_to_folder(
                    &catalog,
                    lift.rows.len(),
                    modifiers.control,
                    modifiers.shift,
                );
                (None, line)
            }),
            (None, Some(_)) | (Some(_), None) => None,
            (Some(row), Some(paths)) => aim.facts.and_then(|facts| {
                let urls = foreign_urls(hover.as_ref(), paths);
                let held = Dropping::native(
                    hover.as_ref().and_then(|hover| hover.marker.as_deref()),
                    &urls,
                    hover.as_ref().and_then(|hover| hover.plain.as_deref()),
                    hover.as_ref().map_or(modifiers.control, |hover| hover.ctrl),
                    hover.as_ref().map_or(modifiers.shift, |hover| hover.shift),
                    hover.as_ref().and_then(|hover| hover.action),
                );
                let (landing, count) = (decide(&held, &row.dest, facts), urls.len());
                let verb = verb_of(landing)?;
                Some((
                    Some(SharedString::from(drop_label(&catalog, verb))),
                    line_to(&catalog, count, &row.name, verb),
                ))
            }),
        };
        let (label, line) = said.unwrap_or_default();
        if let Some(aim) = cx.default_global::<Drags>().rows.get_mut(&key) {
            aim.label = label;
        }
        self.say_drag_line(line, cx);
    }

    // FileDrag.say: a running transfer keeps the bar; an empty line always leaves.
    fn say_drag_line(&mut self, line: String, cx: &mut Context<Self>) {
        self.status_messages(cx, |messages, now| {
            if line.is_empty() || !messages.activities().iter().any(|entry| entry.running) {
                messages.set_activity(DRAG_LINE, &line, 0, false, false, now);
            }
        });
        cx.notify();
    }

    /// The window's offer record for its file client, and the channel that client tells when this window's lift ends elsewhere.
    pub(super) fn follow_row_drags(
        &mut self,
        key: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Hover, async_channel::Sender<()>) {
        let hover = cx
            .default_global::<Drags>()
            .hovers
            .entry(key)
            .or_default()
            .clone();
        let (ended, receive) = async_channel::unbounded();
        cx.spawn_in(window, async move |this, cx| {
            while receive.recv().await.is_ok() {
                let _ = this.update_in(cx, |shell, window, cx| shell.settle_row_drag(window, cx));
            }
        })
        .detach();
        (hover, ended)
    }

    /// No drag is in flight in this window: a lift whose native drag still runs elsewhere keeps the line without a name (FileDrag.leaveTarget), anything else clears.
    pub(crate) fn settle_row_drag(&mut self, window: &Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        let Some(drags) = cx.try_global::<Drags>() else {
            return;
        };
        let aim = drags.rows.get(&key);
        let away = aim.is_none_or(|aim| aim.foreign.is_none())
            && drags.lifts.contains_key(&key)
            && drags
                .held
                .get(&key)
                .is_some_and(|held| held.borrow().is_some());
        if !away {
            self.clear_row_aim(window, cx);
            return;
        }
        let reset = aim.is_none_or(|aim| aim.row.is_some());
        if reset {
            let drags = cx.global_mut::<Drags>();
            drags.serial += 1;
            let serial = drags.serial;
            drags.rows.insert(
                key,
                RowAim {
                    row: None,
                    foreign: None,
                    facts: None,
                    serial,
                    label: None,
                },
            );
        }
        let missing = self.status_messages(cx, |messages, _| {
            !messages
                .activities()
                .iter()
                .any(|entry| entry.owner == DRAG_LINE || entry.running)
        });
        if reset || missing {
            self.say_row_aim(window, cx);
        }
    }

    /// The drag left the panes or ended: no row stays lit and the drag's line leaves the status bar.
    pub(crate) fn clear_row_aim(&mut self, window: &Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        let held = cx
            .try_global::<Drags>()
            .is_some_and(|drags| drags.rows.contains_key(&key));
        if held {
            cx.global_mut::<Drags>().rows.remove(&key);
            self.say_drag_line(String::new(), cx);
        }
    }

    /// This window's own lift moved off every rail, row and tab target: the rail aim lets go and the line without a name stays.
    pub(super) fn miss_rail_aim(
        &mut self,
        point: Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_rail_aim(window, cx);
        self.aim_row_drag(point, None, window, cx);
    }
}

/// The List pane's own-lift cue takes precedence over another window's drag.
pub(crate) fn row_cue(
    window: &Window,
    cx: &App,
    side: usize,
    pane: &crate::list::pane::Pane,
) -> Option<(usize, SharedString)> {
    if let Some(cue) = super::rail_drop::row_cue(window, cx, side) {
        return Some(cue);
    }
    let aim = cx.try_global::<Drags>()?.rows.get(&window_key(window))?;
    let row = aim.row.as_ref()?;
    (row.side == side && row.listing == pane.held.listing)
        .then(|| Some((row.index, aim.label.clone()?)))
        .flatten()
}

#[cfg(test)]
#[path = "row_drop_tests.rs"]
mod tests;
