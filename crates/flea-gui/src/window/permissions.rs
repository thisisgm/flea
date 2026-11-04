//! Permissions owns one captured pane and its backend, with the other file cards' focus and effect lifecycle.
use crate::{
    backend::Event,
    g2::{
        mn1::join::{Captured, Chosen},
        op3::{
            permissions::{
                host::{Changed, View},
                Permissions,
            },
            Effect,
        },
    },
    state::Shell,
    window::{dispatch::Order, navigate::Step},
};
use flea::{
    jsondoc::{self, Json},
    ui_logic_op3::permissions::INSPECT_STRIDE,
};
use gpui::{App, AppContext, Context, Entity, FocusHandle, Subscription, Window};

#[derive(Default)]
pub(crate) struct Join {
    pub view: Option<Entity<View>>,
    owner: usize,
    generation: u64,
    origin: Option<FocusHandle>,
    events: Option<Subscription>,
    kind: Option<(usize, u64, String)>,
}

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Menu(chosen) = order else {
        return false;
    };
    if chosen.action != "permissions" {
        return false;
    }
    if !chosen.claim() {
        return true;
    }
    shell.open_permissions(chosen, window, cx);
    true
}

impl Shell {
    pub(crate) fn permissions_showing(&self, cx: &App) -> bool {
        self.permissions
            .view
            .as_ref()
            .is_some_and(|view| view.read(cx).opened)
    }

    /// The open card's permissionsState, or None while no card is open.
    pub(crate) fn permissions_state(&self, cx: &App) -> Option<Json> {
        let view = self.permissions.view.as_ref()?.read(cx);
        view.opened.then(|| {
            view.model
                .painted_state(&view.catalog, view.painted.as_ref(), &view.probe)
        })
    }

    fn open_permissions(&mut self, chosen: &Chosen, window: &mut Window, cx: &mut Context<Self>) {
        let capture = &chosen.captured;
        let side = capture.side;
        let available = capture.ready
            && !capture.background
            && !chosen.paths.is_empty()
            && !self.card_holds_keyboard(cx)
            && self
                .panes
                .get(side)
                .and_then(Option::as_ref)
                .is_some_and(|host| {
                    !host.stopped
                        && host.backend.is_some()
                        && (capture.target.is_some() || host.path == capture.path)
                        && host.pane.held.listing == capture.listing
                });
        self.release_permissions_snapshot(capture, window, cx);
        if !available {
            self.menu_notice(side, self.locale.catalog.tr("menu.refusal.unavailable"), cx);
            return;
        }
        self.permissions.generation += 1;
        let id = self.permissions.generation;
        // A wide batch reserves all of its inspect-id blocks before a later open can reuse them.
        self.permissions.generation +=
            (chosen.paths.len().saturating_sub(1) / INSPECT_STRIDE as usize) as u64;
        let kind = self
            .permissions
            .kind
            .as_ref()
            .filter(|(owner, menu, _)| *owner == side && *menu == capture.id)
            .map_or("", |(_, _, kind)| kind.as_str());
        let (model, effects) = Permissions::open(id, chosen.paths.clone(), kind);
        self.permissions.origin = Some(self.focus.clone());
        self.permissions.owner = side;
        let catalog = self.locale.catalog.clone();
        let corner = self.corner;
        let view = cx.new(|cx| View::new(model, catalog, corner, cx));
        self.permissions.events =
            Some(
                cx.subscribe_in(&view, window, |shell, _, change: &Changed, window, cx| {
                    shell.permissions_effect(change.0.clone(), window, cx);
                }),
            );
        self.permissions.view = Some(view.clone());
        view.update(cx, |view, cx| view.focus(window, cx));
        for effect in effects {
            self.permissions_effect(effect, window, cx);
        }
        cx.notify();
    }

    fn release_permissions_snapshot(
        &mut self,
        capture: &Captured,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only item menus take backend snapshots; background menus stay local.
        let held = self
            .menu
            .captured
            .as_ref()
            .is_some_and(|live| live.side == capture.side && live.id == capture.id);
        if held {
            self.retire_menu(window, cx);
        } else if !capture.background {
            self.menu_send(
                capture.side,
                format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, capture.id),
                cx,
            );
        }
    }

    pub(crate) fn permissions_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Event::Producer(line) | Event::Operations(line) = event {
            let Ok(reply) = jsondoc::parse(line) else {
                return false;
            };
            let text = |key| reply.get(key).and_then(Json::as_str);
            if text("t") == Some("menuaction") && text("op") == Some("applications") {
                if let Some(capture) = self.menu.captured.as_ref().filter(|c| {
                    c.side == side && reply.get("id").and_then(Json::as_f64) == Some(c.id as f64)
                }) {
                    self.permissions.kind =
                        Some((side, capture.id, text("kind").unwrap_or("").to_string()));
                }
            }
            if text("t") != Some("permissions") || side != self.permissions.owner {
                return false;
            }
            if let Some(view) = self.permissions.view.clone().filter(|v| v.read(cx).opened) {
                view.update(cx, |view, cx| view.reply(&reply, cx));
                return true;
            }
        }
        if side == self.permissions.owner && matches!(event, Event::Failed(_) | Event::Drained) {
            self.permissions_stopped(cx);
        }
        let _ = window;
        false
    }

    fn permissions_stopped(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = &self.permissions.view {
            view.update(cx, |view, cx| view.stopped(cx));
        }
    }

    fn permissions_effect(&mut self, effect: Effect, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.permissions.owner;
        match effect {
            Effect::Send(line) => {
                let sent = self.panes[side]
                    .as_ref()
                    .and_then(|host| host.backend.as_ref())
                    .is_some_and(|backend| backend.operation(line).is_ok());
                if !sent {
                    self.permissions_stopped(cx);
                }
            }
            Effect::Close => {
                if let Some(origin) = self.permissions.origin.take() {
                    window.focus(&origin, cx);
                }
                self.keyboard.context = flea::keys::Context::Listing;
            }
            Effect::Message(text) => {
                if let Some(host) = &mut self.panes[side] {
                    host.status = text.clone().into();
                }
                self.status_messages(cx, |messages, now| {
                    messages.say(
                        &text,
                        false,
                        "",
                        flea::ui_logic_fb1::status::Hint::None,
                        now,
                    )
                });
            }
            // WindowBody.qml refresh("", true): a mode change keeps the cursor's row and the marks.
            Effect::Refresh => {
                let _ = self.step(side, Step::Reread, window, cx);
            }
            Effect::Launch(_) | Effect::EditNames { .. } => {
                unreachable!("Permissions emits only backend and presentation effects")
            }
        }
        cx.notify();
    }
}
