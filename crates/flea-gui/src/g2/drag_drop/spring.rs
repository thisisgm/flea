//! One cancellable dwell per destination window, shared by every listing view and the rail.
use super::*;
use gpui::{prelude::*, Div, ExternalPaths};
use std::time::Duration;

// FINDER-BEHAVIOR's spring-loaded folder dwell, about 0.3 s over a row (HIG-RECHECK-1024 item 7).
pub(in crate::g2::drag_drop) const OPEN_MS: u64 = 300;

pub(super) struct Rest {
    point: Point<gpui::Pixels>,
    side: usize,
    folder: String,
    dest: String,
    _task: gpui::Task<()>,
}

impl Shell {
    pub(crate) fn file_drag_events(div: Div, cx: &mut Context<Self>) -> Div {
        div.on_drag_move::<Seed>(cx.listener(
            |shell, event: &gpui::DragMoveEvent<Seed>, window, cx| {
                shell.file_drag_moved(event.event.position, true, window, cx);
            },
        ))
        .on_drag_move::<ExternalPaths>(cx.listener(
            |shell, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                shell.file_drag_moved(event.event.position, false, window, cx);
            },
        ))
        .capture_any_mouse_up(cx.listener(|shell, _, window, cx| {
            shell.clear_file_rest(window, cx);
            shell.clear_file_cue(window, cx);
            shell.show_crumb_drop(None, cx);
        }))
        .on_mouse_exit(cx.listener(|shell, _, window, cx| {
            shell.clear_file_rest(window, cx);
            shell.clear_file_cue(window, cx);
            shell.show_crumb_drop(None, cx);
        }))
        .on_drop::<Seed>(cx.listener(|shell, _: &Seed, window, cx| {
            if let Some((side, _)) = shell.file_crumb_destination(window.mouse_position(), cx) {
                shell.land_own_drag(side, window, cx);
            }
        }))
        .on_modifiers_changed(cx.listener(
            |shell, _: &gpui::ModifiersChangedEvent, window, cx| {
                shell.refresh_file_cue(window, cx);
            },
        ))
    }

    pub(super) fn spring_target(
        &self,
        point: Point<gpui::Pixels>,
        own: bool,
        window: &Window,
        cx: &App,
    ) -> Option<(usize, String)> {
        if self.input_blocked(cx) {
            return None;
        }
        if let Some(target) = self.file_rail_folder(point, cx) {
            return Some(target);
        }
        self.panes.iter().enumerate().find_map(|(side, host)| {
            let host = host.as_ref()?;
            if host.pane.busy() || host.drawn_view.is_none() {
                return None;
            }
            let carried = cx
                .try_global::<Drags>()
                .and_then(|drags| drags.lifts.get(&window_key(window)))
                .filter(|lift| own && lift.side == side && lift.listing == host.pane.held.listing)
                .map_or(&[][..], |lift| lift.rows.as_slice());
            if host.drawn_view == Some(crate::window::startup::View::Columns) {
                return crate::window::columns::spring_spot(host, point, carried)
                    .map(|dest| (side, dest));
            }
            if !host.input.frame?.bounds.contains(&point) {
                return None;
            }
            let dest = pane_spot(host, point, carried);
            (dest != host.path).then_some((side, dest))
        })
    }

    fn file_drag_moved(
        &mut self,
        point: Point<gpui::Pixels>,
        own: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = window_key(window);
        if own && !cx.default_global::<Drags>().lifts.contains_key(&key) {
            self.clear_file_rest(window, cx);
            self.clear_file_cue(window, cx);
            return;
        }
        self.aim_file_cue(point, own, window, cx);
        self.show_crumb_drop(
            self.file_crumb_hit(point, cx)
                .map(|(_, slot, index, _)| (slot, index)),
            cx,
        );
        let enabled = self
            .launch
            .settings
            .data
            .get("openFoldersWhileDragging")
            .and_then(|v| v.as_bool())
            == Some(true);
        if !enabled {
            self.clear_file_rest(window, cx);
            return;
        }
        let Some((side, dest)) = self.spring_target(point, own, window, cx) else {
            self.clear_file_rest(window, cx);
            return;
        };
        let Some(host) = self.panes[side]
            .as_ref()
            .filter(|host| !host.pane.busy() && host.path != dest)
        else {
            self.clear_file_rest(window, cx);
            return;
        };
        let path = host.path.clone();
        // Animated frames replay a move at the same point; only a changed point or folder restarts the dwell.
        if cx
            .default_global::<Drags>()
            .rests
            .get(&key)
            .is_some_and(|rest| {
                rest.point == point && rest.side == side && rest.folder == path && rest.dest == dest
            })
        {
            return;
        }
        cx.global_mut::<Drags>().rests.clear();
        let requested = dest.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(OPEN_MS))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let enabled = shell
                    .launch
                    .settings
                    .data
                    .get("openFoldersWhileDragging")
                    .and_then(|v| v.as_bool())
                    == Some(true);
                let current = shell.panes[side]
                    .as_ref()
                    .is_some_and(|host| host.path == path && !host.pane.busy());
                if enabled
                    && current
                    && cx.has_active_drag()
                    && window.mouse_position() == point
                    && shell.spring_target(point, own, window, cx) == Some((side, dest.clone()))
                {
                    let _ = shell.step(
                        side,
                        crate::window::navigate::Step::OpenPlace(dest),
                        window,
                        cx,
                    );
                }
            });
        });
        cx.default_global::<Drags>().rests.insert(
            key,
            Rest {
                point,
                side,
                folder: host_path(self, side),
                dest: requested,
                _task: task,
            },
        );
    }

    /// A folder that sprang open under a still pointer: the cue its old listing named re-aims at the row now under the pointer.
    pub(crate) fn reaim_file_cue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !cx.has_active_drag() {
            return;
        }
        let key = window_key(window);
        let stale = cx
            .try_global::<Drags>()
            .and_then(|drags| drags.cue.as_ref())
            .filter(|(owner, _)| *owner == key)
            .and_then(|(_, cue)| {
                let aimed = &cue.target;
                let host = self.panes[aimed.side].as_ref()?;
                let moved = host.path != aimed.folder || host.pane.held.listing != aimed.listing;
                (!host.pane.busy() && moved).then_some(cue.own)
            });
        if let Some(own) = stale {
            self.aim_file_cue(window.mouse_position(), own, window, cx);
        }
    }

    pub(super) fn clear_file_rest(&mut self, window: &Window, cx: &mut App) {
        cx.default_global::<Drags>()
            .rests
            .remove(&window_key(window));
    }

    pub(super) fn file_rail_folder(
        &self,
        point: Point<gpui::Pixels>,
        cx: &App,
    ) -> Option<(usize, String)> {
        use flea::ui_logic_rail_places::rail::Kind;
        let join = self.rail.as_ref()?;
        let rail = &join.host.read(cx).rail;
        if !rail.shown() || !rail.drawn(cx.background_executor().now()) {
            return None;
        }
        let producer = join.places.read(cx);
        let entry = producer.entry(producer.row_at(point)?)?;
        (!matches!(
            entry.kind,
            Kind::Trash | Kind::Recent | Kind::Tag | Kind::Phone
        ) && !entry.invalid_record()
            && entry.mounted != Some(false)
            && entry.path.starts_with('/'))
        .then_some((producer.side, entry.path))
    }
}

fn host_path(shell: &Shell, side: usize) -> String {
    shell.panes[side]
        .as_ref()
        .expect("the dwell has an owner")
        .path
        .clone()
}
