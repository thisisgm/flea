//! Addressed acknowledgments and the fork's native private-MIME drag transport.
use super::transfer;
use crate::g2::drag_drop::router::{Client, Native as Landing, Offered};
use crate::{ipc::server::Answer, state::Shell};
use flea::ipc::{registry, wire::Request};
use gpui::{point, px, Context, NativeDragAction, NativeDragActions, NativeDragPhase, Window};
use std::{cell::Cell, rc::Rc};

// The strip's half of the shared router: it claims only the private tab MIME, so file clients never see a tab.
struct TabClient {
    window: u64,
    send: async_channel::Sender<Native>,
    capacity: Rc<Cell<bool>>,
    zone: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    strip: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    catchers: super::catcher::Catchers,
}

impl TabClient {
    fn in_zone(&self, window: u64, at: gpui::Point<gpui::Pixels>) -> bool {
        window == self.window && self.zone.get().is_some_and(|zone| zone.contains(&at))
    }

    // WindowBody.qml takes a one-tab window's drop, TabBar.qml a shown strip's.
    fn place(&self) -> &'static str {
        if self.strip.get().is_some() {
            "strip"
        } else {
            "window"
        }
    }
}

impl Client for TabClient {
    fn offer(&self, offered: &Offered<'_>) -> Option<NativeDragActions> {
        if !transfer::has_tab_format(offered.types) {
            return None;
        }
        let at = point(px(offered.position.0), px(offered.position.1));
        // The fork asks only at Enter; admit the whole window and enforce strip geometry at drop.
        let receiving = offered.window == self.window && self.zone.get().is_some();
        let ours =
            self.catchers.handle(offered.window).is_some() || receiving && self.capacity.get();
        if !ours {
            return None;
        }
        if self.catchers.handle(offered.window).is_some() {
            super::trace::stage("catcher-enter", String::new);
        } else if self.in_zone(offered.window, at) {
            super::trace::stage(&format!("enter-{}", self.place()), || {
                format!("formats={} ok=true", offered.types.join(","))
            });
        }
        let _ = self.send.try_send(Native::Hover);
        Some(NativeDragActions {
            copy: false,
            move_: true,
            preferred: NativeDragAction::Move,
        })
    }

    fn land(&self, event: &Landing<'_>) -> Option<bool> {
        let desktop = self.catchers.handle(event.window);
        if event.window != self.window && desktop.is_none() {
            return None;
        }
        let bytes = event.get(transfer::TAB_MIME)?;
        let at = point(px(event.position.0), px(event.position.1));
        if desktop.is_none() && event.dropped && !self.in_zone(event.window, at) {
            return Some(false);
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return Some(false);
        };
        if transfer::parse(text).is_none() {
            return Some(false);
        }
        if let Some(handle) = desktop {
            let Some(global) = self.catchers.global(handle, at) else {
                return Some(false);
            };
            return Some(
                self.catchers.accepts(handle, text)
                    && (!event.dropped
                        || self
                            .send
                            .try_send(Native::Desktop(text.into(), global))
                            .is_ok()),
            );
        }
        if event.dropped {
            super::trace::stage(&format!("drop-{}", self.place()), || {
                format!("empty={} len={}", bytes.is_empty(), bytes.len())
            });
        }
        Some(!event.dropped || self.send.try_send(Native::Drop(text.into(), at)).is_ok())
    }

    fn source(&self, event: &gpui::NativeDragEvent) {
        let _ = self.send.try_send(Native::Source(*event));
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
            tearoff: [
                self.strip.catchers.count(),
                self.strip.lifts,
                self.strip.client_queries,
            ],
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
        super::trace::stage("taken-call", || {
            let result = if taken { "accepted" } else { "refused" };
            format!("token={token} result={result}")
        });
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
        // One process-wide router owns the fork's single-slot callbacks; this window registers its strip.
        let client = Rc::new(TabClient {
            window: window.window_handle().window_id().as_u64(),
            send,
            capacity: self.strip.capacity.clone(),
            zone: self.strip.drop_zone.clone(),
            strip: self.strip.bounds.clone(),
            catchers: self.strip.catchers.clone(),
        });
        self.strip.router = Some(crate::g2::drag_drop::router::shared(cx).register(client));
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
                                action,
                                ..
                            }) => {
                                super::trace::stage("drag-finished", || format!("action={}", super::trace::action(action)));
                                shell.strip.bar.out_finished(now);
                                shell.tabs_changed(window, cx);
                            }
                            Native::Source(gpui::NativeDragEvent {
                                phase: NativeDragPhase::Cancelled,
                                ..
                            }) => {
                                super::trace::stage("drag-finished", || "action=0".into());
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

    /// Escape ends a tab's live platform drag as Qt's QDrag does; the Cancelled event restores the strip.
    pub(crate) fn escape_tab_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.strip.bar.out_active && window.cancel_drag(cx)
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
