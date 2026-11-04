use super::{contract::Spec, Pending};
use crate::{ipc::server::Answer, list::pane::Pane, state::Shell};
use flea::{
    ipc::{
        registry,
        wire::{Reply, Request},
    },
    jsondoc,
};
use gpui::{px, Context, Window};
use std::sync::mpsc::SyncSender;

impl Shell {
    fn visual_error(&self, key: &str) -> Reply {
        Reply::error(2, self.locale.catalog.tr(key))
    }

    pub(crate) fn visual_call(
        &mut self,
        request: Request,
        answer: SyncSender<Answer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Request, SyncSender<Answer>)> {
        let Request::Call {
            target,
            function,
            arguments,
        } = &request
        else {
            return Some((request, answer));
        };
        if target == "flea"
            && function == "visibleRowName"
            && self.view == crate::window::startup::View::Grid
        {
            return Some((request, answer));
        }
        if target != "flea"
            || ![
                "scene",
                "nativeGeometry",
                "pendingRequests",
                "previewReadiness",
                "sceneState",
                "presentedFrame",
                "listInFlight",
            ]
            .contains(&function.as_str())
        {
            return Some((request, answer));
        }
        let reply = match registry::validate(target, function, arguments) {
            Err(reply) => reply,
            Ok(_) => match function.as_str() {
                "scene" => match Spec::parse(&arguments[0], &arguments[1], &arguments[2]) {
                    Err(key) => self.visual_error(key),
                    // A receipt names a presented frame, so a window the compositor reports none for never earns one.
                    Ok(_) if self.presented_frames(window).is_none() => {
                        self.visual_error("visual.unpresented")
                    }
                    Ok(_) if self.visual.pending.is_some() || self.pending_requests() != 0 => {
                        self.visual_error("visual.busy")
                    }
                    Ok(spec) => {
                        self.apply_scene(spec, answer, window, cx);
                        return None;
                    }
                },
                "nativeGeometry" => {
                    if arguments[0] != std::process::id().to_string() {
                        self.visual_error("visual.window")
                    } else if let Some(receipt) = &self.visual.receipt {
                        // The geometry holds while the receipt frame is still the newest one presented.
                        let presented = self
                            .presented_frames(window)
                            .map(|frames| frames.presented.to_string());
                        let matches = receipt.get("scene").and_then(|value| value.as_str())
                            == Some(&arguments[1])
                            && receipt
                                .get("frame")
                                .and_then(|value| value.as_f64())
                                .map(|n| n.to_string())
                                == Some(arguments[2].clone())
                            && presented.as_ref() == Some(&arguments[2]);
                        if matches && !self.visual.probe.empty() {
                            Reply::value(
                                jsondoc::render(&self.visual.probe.json(receipt)).trim_end(),
                            )
                        } else {
                            self.visual_error("visual.snapshot")
                        }
                    } else {
                        self.visual_error("visual.snapshot")
                    }
                }
                "pendingRequests" => Reply::value(self.pending_requests().to_string()),
                "previewReadiness" => Reply::value("absent"),
                "sceneState" => {
                    Reply::value(jsondoc::render(&self.visual_state(window)).trim_end())
                }
                "presentedFrame" => match self.presented_frames(window) {
                    Some(frames) => Reply::value(frames.presented.to_string()),
                    None => self.visual_error("visual.unpresented"),
                },
                // Ipc.qml: the focused pane's listing in flight.
                "listInFlight" => Reply::value(
                    self.panes[self.focus_side]
                        .as_ref()
                        .is_some_and(|host| host.pane.listing_in_flight())
                        .to_string(),
                ),
                _ => unreachable!(),
            },
        };
        let _ = answer.try_send(Answer { reply, quit: false });
        None
    }

    pub(crate) fn apply_scene(
        &mut self,
        spec: Spec,
        answer: SyncSender<Answer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.visual.revision += 1;
        self.visual.receipt = None;
        self.visual.awaiting = None;
        // Registered here, outside a frame, because a Wayland window cannot take its first presented-frame callback while it draws.
        let weak = cx.entity().downgrade();
        self.visual.presented = Some(window.observe_frame_presented(cx, move |window, cx| {
            let _ = weak.update(cx, |shell, cx| {
                if shell.visual_presented(window) {
                    cx.notify();
                }
            });
        }));
        self.launch.paths = [spec.path.clone(), spec.path.clone()];
        self.launch.settings.view = spec.view;
        if self.view != spec.view || self.dual_pending {
            self.set_view(spec.view, window, cx);
        }
        self.focus_side = 0;
        let height = self.geometry(window).rows_height(
            spec.view == crate::window::startup::View::Dual,
            self.slots.strip_height,
        );
        for (side, host) in self.panes.iter_mut().enumerate() {
            let Some(host) = host else { continue };
            if host.path != spec.path {
                host.path.clone_from(&spec.path);
                host.pane = Pane::at_row_height(spec.path.clone(), height, host.pane.row_height());
                host.pane.use_executor(cx.background_executor().clone());
                host.answered = false;
                if let Some(backend) = &host.backend {
                    if let Err(error) =
                        backend.list(spec.path.clone(), None, host.pane.first_count())
                    {
                        host.record_error(error, &self.locale);
                    }
                }
            }
            // Only the focused side takes the scene's cursor and selection; render reapplies them before the build once rows land.
            let (cursor, selection) = if side == 0 {
                (spec.cursor, spec.selection.clone())
            } else {
                (0, Vec::new())
            };
            host.pane.cursor = cursor.min(host.pane.held.total.saturating_sub(1));
            host.pane.reveal_cursor();
            host.selected = selection;
            host.input.selection.sync(&host.selected);
            self.path_labels[side] = spec.path.clone().into();
        }
        // A later scene starts with no card open; the reset runs before any scene's own setup.
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        // The window holds the rounding every scene names.
        self.visual.scene_corner = Some(spec.corner as f32);
        self.apply_corner(spec.corner as f32, window, cx);
        // The operations scenes open their card on the overlay slot; copy_to needs no backend reply.
        if matches!(
            spec.name.as_str(),
            "g2.op1.copyto.r8"
                | "g2.op1.copyto.r0"
                | "g2.op1.copyto-empty.r8"
                | "g2.op1.copyto-empty.r0"
        ) {
            let empty = spec.name.as_str() == "g2.op1.copyto-empty.r8"
                || spec.name.as_str() == "g2.op1.copyto-empty.r0";
            self.open_overlay_copy_to(false, if empty { "" } else { &spec.path }, window, cx);
        }
        cx.set_reduce_motion(spec.reduced_motion);
        window.set_rem_size(px(spec.text_size as f32));
        self.visual.pending = Some(Pending { spec, answer });
        self.visual.deadline = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(super::FRAME_WAIT).await;
            let _ = cx.update(|_, cx| {
                this.update(cx, |shell, _| {
                    if let Some(pending) = shell.visual.pending.take() {
                        shell.visual.awaiting = None;
                        shell.visual.presented = None;
                        let _ = pending.answer.try_send(Answer {
                            reply: shell.visual_error("visual.timeout"),
                            quit: false,
                        });
                    }
                })
            });
        }));
        cx.notify();
    }
}
