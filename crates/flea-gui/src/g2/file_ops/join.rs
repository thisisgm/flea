//! Pane-bound operation sessions keep each card's replies and commands on its originating backend.
use super::{host::Overlay, palette::CardPalette};
#[path = "chosen.rs"]
mod chosen;
use crate::{palette::Theme, state::Shell};
use flea::ui_logic_file_ops::{
    collide::Request,
    confirm::Source,
    trash::{Review, Scope},
};
use gpui::{AppContext, Context, Entity, Subscription, Window};

pub struct OperationReply {
    pub side: usize,
    pub line: String,
    pub taken: bool,
}

pub struct OperationUpdate {
    pub side: usize,
    pub said: Vec<super::model::Said>,
    pub sticky: String,
    pub spent_cut: bool,
}

impl gpui::EventEmitter<OperationReply> for Shell {}
impl gpui::EventEmitter<OperationUpdate> for Shell {}

#[derive(Default)]
pub(crate) struct Sessions {
    pub(crate) fileops: crate::window::dispatch::FileOps,
    pub(super) focus_owner: Option<Entity<Overlay>>,
    views: [Option<Entity<Overlay>>; 2],
    observers: [Option<Subscription>; 2],
    // The sticky line each side last published, so a frame with nothing new publishes nothing.
    sticky: [String; 2],
    pub(crate) origins: [Option<crate::g2::context_menus::join::Chosen>; 2],
    chosen_subscription: Option<Subscription>,
    status_subscription: Option<Subscription>,
    pub(crate) archive: crate::g2::archive_flash::host::Archive,
}

impl Shell {
    pub fn operation_view(&self, side: usize) -> Option<Entity<Overlay>> {
        self.operations.views.get(side)?.clone()
    }

    pub(crate) fn operation_action(&mut self, action: &str, cx: &mut Context<Self>) -> bool {
        if !matches!(action, "undo" | "redo") {
            return false;
        }
        let Some(host) = self.panes[self.focus_side].as_mut() else {
            return false;
        };
        if let Some(backend) = &host.backend {
            if let Err(error) = backend.operation(format!("{{\"c\":\"{action}\"}}")) {
                host.record_error(error, &self.locale);
                cx.notify();
            }
        }
        true
    }

    pub(crate) fn operation_overlay(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Overlay>> {
        self.panes.get(side)?.as_ref()?;
        if let Some(view) = &self.operations.views[side] {
            return Some(view.clone());
        }
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = CardPalette::from_theme(&window.text_style(), theme, self.corner);
        let catalog = self.locale.catalog.clone();
        let preset = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default")
            .to_owned();
        let view = cx.new(|_| {
            let mut overlay = Overlay::new(catalog, palette, &preset);
            overlay.owner = Some(side);
            overlay
        });
        self.operations.observers[side] =
            Some(cx.observe_in(&view, window, move |shell, _, _, cx| {
                shell.flush_operations(side, cx);
            }));
        self.operations.views[side] = Some(view.clone());
        Some(view)
    }

    pub(crate) fn show_operation(&mut self, view: &Entity<Overlay>, cx: &mut Context<Self>) {
        let active = Entity::read(view, cx);
        if !active.modal(cx)
            && !active.model.jobs.visible()
            && !active.model.wire.transfer.running
            && !active.model.emptying.running
        {
            return;
        }
        let occupied = self
            .overlay
            .as_ref()
            .is_some_and(|shown| shown != view && Entity::read(shown, cx).modal(cx));
        if !occupied {
            self.overlay = Some(view.clone());
        }
        cx.notify();
    }

    /// Paste, drop and menu producers pass their captured request through the existing collision host.
    pub fn ask_operation(
        &mut self,
        side: usize,
        request: &Request,
        probe: &[&str],
        cut: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.operation_overlay(side, window, cx) else {
            return false;
        };
        let listing = self.panes[side]
            .as_ref()
            .map_or(0, |host| host.pane.held.listing as u64);
        let accepted = view.update(cx, |overlay, cx| {
            overlay.model.held_listing = listing;
            let accepted = overlay.model.ask(&overlay.catalog, request, probe, cut);
            overlay.schedule(cx);
            cx.notify();
            accepted
        });
        self.show_operation(&view, cx);
        self.flush_operations(side, cx);
        accepted
    }

    pub fn confirm_operation(
        &mut self,
        side: usize,
        review: Review,
        scope: Scope,
        source: Source,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.operation_overlay(side, window, cx) else {
            return;
        };
        view.update(cx, |overlay, cx| {
            overlay.confirm_delete(review, scope, source, cx)
        });
        self.show_operation(&view, cx);
        self.flush_operations(side, cx);
    }

    pub fn copy_to_operation(
        &mut self,
        side: usize,
        menu: u64,
        moving: bool,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.operation_overlay(side, window, cx) else {
            return;
        };
        view.update(cx, |overlay, cx| overlay.copy_to(menu, moving, path, cx));
        self.show_operation(&view, cx);
        self.flush_operations(side, cx);
    }

    pub(crate) fn operation_line(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.operation_overlay(side, window, cx) else {
            return;
        };
        let (taken, landed) = view.update(cx, |overlay, cx| {
            let taken = overlay.line(line, cx);
            (taken, overlay.model.new_file_landed.take())
        });
        cx.emit(OperationReply {
            side,
            line: line.into(),
            taken,
        });
        if let Some(path) = landed {
            self.new_file_created(side, path, window, cx);
        }
        self.show_operation(&view, cx);
        self.flush_operations(side, cx);
    }

    pub(crate) fn operation_failed(
        &mut self,
        side: usize,
        place: &str,
        input: &str,
        message: &str,
        cx: &mut Context<Self>,
    ) {
        if place == "backend" {
            self.archive_failed(side, cx);
        }
        let related = flea::ui_logic_file_ops::collide::dropped_pending(place, input)
            || (place == "stale" && input == "transfer")
            || matches!(
                place,
                "collisions" | "mergecollisions" | "transfer" | "undo" | "redo"
            );
        let notice = (related && !message.is_empty()).then(|| super::model::Said {
            text: message.into(),
            error: true,
            detail: String::new(),
            behind: false,
        });
        if let Some(view) = self.operations.views[side].clone() {
            view.update(cx, |overlay, cx| {
                overlay.model.failed(place, input);
                if place == "backend" {
                    disconnect(overlay, cx);
                }
                overlay.model.said.extend(notice);
                cx.notify();
            });
            self.flush_operations(side, cx);
        } else if let Some(notice) = notice {
            if let Some(host) = self.panes[side].as_mut() {
                host.status = notice.text.clone().into();
                host.operation_line = None;
            }
            cx.emit(OperationUpdate {
                side,
                said: vec![notice],
                sticky: String::new(),
                spent_cut: false,
            });
            cx.notify();
        }
    }

    pub(crate) fn flush_operations(&mut self, side: usize, cx: &mut Context<Self>) {
        let Some(view) = self.operations.views[side].clone() else {
            return;
        };
        let pending = view.update(cx, |overlay, cx| {
            let (said, sticky) =
                overlay.publication_if_pending(cx, &self.operations.sticky[side])?;
            Some((
                std::mem::take(&mut overlay.model.outbox),
                said,
                sticky,
                std::mem::take(&mut overlay.model.spent_cut),
            ))
        });
        // An ask with no collision pending and no open card is idle; its spent cut, taken above, rides apart.
        let idle = {
            let model = &Entity::read(&view, cx).model;
            model.collide.pending.is_none() && !model.open()
        };
        let Some((outbox, mut said, mut sticky, spent_cut)) = pending else {
            self.sync_transfer_status(side, cx);
            self.clipboard_operations(side, false, idle, cx);
            if self.settle_overlay_mount(cx) {
                cx.notify();
            }
            return;
        };
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let before = host.status.clone();
        let mut changed = false;
        let mut sent = true;
        for line in outbox {
            let result = host
                .backend
                .as_ref()
                .ok_or(crate::backend::Failure {
                    key: "window.backend_stopped",
                    detail: String::new(),
                })
                .and_then(|backend| backend.operation(line));
            if let Err(error) = result {
                sent = false;
                changed = true;
                said = view.update(cx, |overlay, cx| {
                    disconnect(overlay, cx);
                    std::mem::take(&mut overlay.model.said)
                });
                host.record_error(error, &self.locale);
                sticky.clear();
                break;
            }
        }
        if let Some(message) = said.last() {
            host.status = message.text.clone().into();
            host.operation_line = Some(message.text.clone());
        } else if !sticky.is_empty() {
            host.status = sticky.clone().into();
            host.operation_line = None;
        }
        changed |= host.status != before;
        let spent_cut = spent_cut && sent;
        self.sync_transfer_status(side, cx);
        self.clipboard_operations(side, spent_cut, idle, cx);
        // An empty frame publishes nothing, so the shell's cached view stays untouched.
        if !said.is_empty() || spent_cut || self.operations.sticky[side] != sticky {
            self.operations.sticky[side].clone_from(&sticky);
            cx.emit(OperationUpdate {
                side,
                said,
                sticky,
                spent_cut,
            });
        }
        changed |= self.settle_overlay_mount(cx);
        if changed {
            cx.notify();
        }
    }

    // A queued card that becomes modal takes the overlay mount; the caller redraws only when it changed.
    fn settle_overlay_mount(&mut self, cx: &mut Context<Self>) -> bool {
        if self
            .overlay
            .as_ref()
            .is_some_and(|view| Entity::read(view, cx).modal(cx))
        {
            return false;
        }
        let next = self
            .operations
            .views
            .iter()
            .flatten()
            .find(|view| Entity::read(view, cx).modal(cx));
        let Some(next) = next else {
            return false;
        };
        if self.overlay.as_ref() == Some(next) {
            return false;
        }
        self.overlay = Some(next.clone());
        true
    }
}

fn disconnect(overlay: &mut Overlay, cx: &mut Context<Overlay>) {
    overlay.model.failed("backend", "");
    overlay.model.dialog = None;
    overlay.model.confirm = None;
    overlay.model.trash_held = None;
    overlay.model.menu_held = None;
    overlay.model.wire.transfer.running = false;
    overlay.model.pending_transfer_job = None;
    overlay.model.transfer_cancelling = false;
    overlay.model.wire.held.clear();
    overlay.model.wire.held_since = None;
    overlay.model.emptying.running = false;
    overlay.model.sticky.clear();
    overlay.model.said.clear();
    overlay.model.said.push(super::model::Said {
        text: overlay.catalog.tr("window.backend_stopped"),
        error: true,
        detail: String::new(),
        behind: false,
    });
    overlay.model.outbox.clear();
    for job in &mut overlay.model.jobs.rows {
        job.undo = false;
        if flea::ui_logic_file_ops::jobs::unfinished(job) {
            job.state = "failed".into();
            job.reason = overlay.catalog.tr("window.backend_stopped");
        }
    }
    overlay.schedule(cx);
}
