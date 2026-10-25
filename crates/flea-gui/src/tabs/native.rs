//! Addressed acknowledgments and the fork's native private-MIME drag transport.
use super::transfer;
use crate::{ipc::server::Answer, state::Shell};
use flea::ipc::{registry, wire::Request};
use gpui::{Context, NativeDragAction, NativeDragActions, NativeDragPhase, Window};

fn refused() -> NativeDragActions {
    NativeDragActions {
        copy: false,
        move_: false,
        preferred: NativeDragAction::Move,
    }
}

impl Shell {
    pub(crate) fn tab_adapter<'a>(
        &'a self,
        home_label: &'a str,
    ) -> crate::ipc::tabs::MountedTabsAdapter<'a> {
        let side = self.focus_side;
        let bounds = self.strip.bounds.get();
        crate::ipc::tabs::MountedTabsAdapter {
            frame: self.strip.snapshot.as_ref().map(|snapshot| &snapshot.frame),
            identities: self
                .strip
                .snapshot
                .as_ref()
                .map_or(&[], |snapshot| snapshot.identities.as_slice()),
            bounds,
            model: crate::ipc::tabs::TabsAdapter {
                state: &self.strip.tabs[side],
                bar: &self.strip.bar,
                path: self.panes[side]
                    .as_ref()
                    .map_or("", |host| host.pane.path()),
                home: &self.strip.home,
                home_label,
                visible: self.has_tabs() && bounds.is_some(),
                top: bounds.map_or(0., |bounds| f32::from(bounds.origin.y)),
            },
        }
    }

    pub(crate) fn tab_ack_call(
        &mut self,
        request: &Request,
        answer: &std::sync::mpsc::SyncSender<Answer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(token) = crate::ipc::tabs::taken(request) else {
            return false;
        };
        if let Err(reply) = registry::validate("fleatab", "taken", std::slice::from_ref(&token)) {
            let _ = answer.try_send(Answer { reply, quit: false });
            return true;
        }
        let now = self.tab_now();
        let taken = self.with_tabs(|bar, panes| bar.take(panes, &token, now));
        let _ = answer.try_send(Answer {
            reply: crate::ipc::tabs::answer(taken),
            quit: false,
        });
        if taken {
            self.tabs_changed(window, cx);
        }
        true
    }

    pub(crate) fn follow_tab_drags(&mut self, window: &Window, cx: &mut Context<Self>) {
        let (send, receive) = async_channel::unbounded();
        let handle = window.window_handle();
        let offered = send.clone();
        let capacity = self.strip.capacity.clone();
        let zone = self.strip.drop_zone.clone();
        let catchers = self.strip.catchers.clone();
        cx.on_native_drag_offer(move |offer| {
            if transfer::has_tab_format(&offer.offered_types) && catchers.contains(offer.window)
                || offer.window == handle
                    && zone
                        .get()
                        .is_some_and(|zone| zone.contains(&offer.position))
                    && capacity.get()
                    && transfer::has_tab_format(&offer.offered_types)
            {
                let _ = offered.try_send(Native::Hover);
                NativeDragActions {
                    copy: false,
                    move_: true,
                    preferred: NativeDragAction::Move,
                }
            } else {
                refused()
            }
        });
        let drops = send.clone();
        let zone = self.strip.drop_zone.clone();
        let catchers = self.strip.catchers.clone();
        cx.on_native_drop(move |event| {
            let desktop = catchers.contains(event.window);
            if !desktop
                && (event.window != handle
                    || !zone
                        .get()
                        .is_some_and(|zone| zone.contains(&event.position)))
            {
                return false;
            }
            let Some(bytes) = event.data.get(transfer::TAB_MIME) else {
                return false;
            };
            let Ok(text) = std::str::from_utf8(&bytes) else {
                return false;
            };
            if transfer::parse(text).is_none() {
                return false;
            }
            if desktop {
                let Some(global) = catchers.global(event.window, event.position) else {
                    return false;
                };
                return catchers.accepts(event.window, text)
                    && (!event.dropped
                        || drops.try_send(Native::Desktop(text.into(), global)).is_ok());
            }
            if event.dropped {
                drops
                    .try_send(Native::Drop(text.into(), event.position))
                    .is_ok()
            } else {
                true
            }
        });
        cx.on_native_drag(move |event| {
            let _ = send.try_send(Native::Source(event));
        });
        self.strip.native = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = receive.recv().await {
                let _ = cx.update(|window, cx| {
                    this.update(cx, |shell, cx| {
                        let now = shell.tab_now();
                        if matches!(&event, Native::Source(event) if event.source_id != shell.strip.drag_serial) { return; }
                        match event {
                            Native::Desktop(payload, global) => {
                                if let Some(tear) = shell.take_desktop_tab(&payload, global) {
                                    shell.spawn_tab_window(tear, window, cx);
                                }
                            }
                            Native::Drop(payload, position) => {
                                if !shell.strip.drop_zone.get().is_some_and(|zone| zone.contains(&position)) { return; }
                                let x = f64::from(position.x)
                                    - shell
                                        .strip
                                        .bounds
                                        .get()
                                        .map_or(0., |bounds| f64::from(bounds.origin.x))
                                    - f64::from(super::bar::PAD_X);
                                shell.with_tabs(|bar, panes| {
                                    bar.strip_drop(panes, &payload, x, now);
                                });
                                shell.tabs_changed(window, cx);
                            }
                            Native::Source(gpui::NativeDragEvent {
                                phase: NativeDragPhase::Finished,
                                ..
                            }) => {
                                shell.strip.bar.out_finished(now);
                                shell.tabs_changed(window, cx);
                            }
                            Native::Source(gpui::NativeDragEvent {
                                phase: NativeDragPhase::Cancelled,
                                ..
                            }) => {
                                shell.strip.bar.cancel_out(now);
                                shell.tabs_changed(window, cx);
                            }
                            Native::Hover | Native::Source(_) => {}
                        }
                    })
                });
            }
        }));
        let catchers = self.strip.catchers.clone();
        cx.on_release(move |_, cx| catchers.close(cx)).detach();
    }

    /// The tab menu's explicit move uses the same identity and acknowledgment as a native lift.
    pub fn detach_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.tab_now();
        self.with_tabs(|bar, panes| {
            bar.tab_lift_began(panes, index, now);
            bar.tab_lift_moved(panes, 0., true);
        });
        if !self.strip.bar.out_active {
            self.strip.bar.drag_cancelled();
            return;
        }
        let Some(tear) = self.strip.bar.tear_off_at(now) else {
            return;
        };
        self.spawn_tab_window(tear, window, cx);
    }

    fn spawn_tab_window(
        &mut self,
        tear: transfer::TearOff,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command =
            transfer::tear_off_command(&crate::window::startup::binary().to_string_lossy(), &tear);
        let side = self
            .strip
            .bar
            .lifts
            .iter()
            .find(|lift| lift.token == tear.token)
            .map_or(self.focus_side, |lift| lift.side);
        let token = tear.token;
        let (started, launched) = async_channel::bounded(1);
        let _reaper = std::thread::Builder::new()
            .name("flea-tab-window".into())
            .spawn(move || {
                let mut command = command;
                match command.spawn() {
                    Ok(mut child) => {
                        let _ = started.try_send(true);
                        let _ = child.wait();
                    }
                    Err(_) => {
                        let _ = started.try_send(false);
                    }
                }
            });
        cx.spawn_in(window, async move |this, cx| {
            let opened = launched.recv().await.unwrap_or(false);
            if !opened {
                let _ = cx.update(|window, cx| {
                    this.update(cx, |shell, cx| {
                        shell.tab_window_failed(side, &token, window, cx)
                    })
                });
            }
        })
        .detach();
        self.tabs_changed(window, cx);
    }

    pub(crate) fn tab_window_failed(
        &mut self,
        side: usize,
        token: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.strip.bar.out_token == token {
            self.strip.bar.cancel_out(self.tab_now());
            self.strip.bar.clear_ack();
        }
        self.strip.bar.lifts.retain(|lift| lift.token != token);
        self.with_tabs(|_, panes| {
            panes.visit(side, &mut |_, pane| {
                pane.say(super::state::Say::NotReceived);
            });
        });
        self.tabs_changed(window, cx);
    }
}

enum Native {
    Hover,
    Drop(String, gpui::Point<gpui::Pixels>),
    Desktop(String, gpui::Point<gpui::Pixels>),
    Source(gpui::NativeDragEvent),
}
