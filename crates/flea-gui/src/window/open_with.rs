//! The Open With card in the window: flea --open's unclaimed ask snapshots its row and opens the card, as v0.3.12 Pane.showOpenWith does.
use crate::{
    g2::op3::{
        keys::{key_of, Key},
        openwith::OpenWith,
        shell::StandIn,
        Effect,
    },
    state::Shell,
};
use gpui::{div, prelude::*, AnyElement, Context, KeyDownEvent, MouseButton, Window};

/// The open card, the side that asked for it, and the menuaction id its snapshot and requests carry.
pub(crate) struct Open {
    pub(crate) side: usize,
    pub(crate) id: u64,
    pub(crate) card: OpenWith,
}

impl Shell {
    /// The opener's Open With ask: snapshot the row the path still names under a fresh menu id, then open the card on it.
    pub(crate) fn open_open_with(&mut self, side: usize, path: String, cx: &mut Context<Self>) {
        self.dismiss_cards(cx);
        let found = self.panes[side].as_ref().and_then(|host| {
            host.pane
                .held
                .rows
                .iter()
                .position(|row| {
                    crate::navigation::history::join(host.pane.path(), &row.name) == path
                })
                // The snapshot names listing rows, and the held window starts at its own row of the listing.
                .map(|at| {
                    (
                        host.pane.held.start + at,
                        host.pane.held.listing,
                        host.pane.held.rows[at].name.clone(),
                    )
                })
        });
        let Some((at, listing, name)) = found else {
            // The row left the listing while flea --open ran, and v0.3.12 Pane.qml showOpenWith names it.
            let unlisted = self.locale.catalog.tr("dialogs.file.open.unlisted");
            self.opener_say(side, unlisted, true, cx);
            return;
        };
        self.menu.sequence += 1;
        let id = self.menu.sequence;
        let snapshot = format!(
            "{{\"c\":\"menuaction\",\"op\":\"snapshot\",\"id\":{id},\"listing\":{listing},\"rows\":[{at}],\"cursor\":{at}}}"
        );
        // A snapshot the backend never took would leave a card whose applications never arrive.
        if !self.open_with_send(side, snapshot, cx) {
            return;
        }
        let (card, effects) = OpenWith::open(id, &name);
        self.open_with = Some(Open { side, id, card });
        self.open_with_effects(effects, cx);
        cx.notify();
    }

    /// One backend line the open card owns: its applications catalogue or its openWith answer, under its own id.
    pub(crate) fn open_with_line(
        &mut self,
        side: usize,
        line: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = self.open_with.as_ref() else {
            return false;
        };
        if open.side != side {
            return false;
        }
        let Ok(reply) = flea::jsondoc::parse(line) else {
            return false;
        };
        if reply.get("id").and_then(flea::jsondoc::Json::as_f64) != Some(open.id as f64)
            || !matches!(
                reply.get("op").and_then(flea::jsondoc::Json::as_str),
                Some("applications" | "openWith")
            )
        {
            return false;
        }
        let catalog = self.locale.catalog.clone();
        let Some(open) = self.open_with.as_mut() else {
            return false;
        };
        let effects = open.card.reply(&catalog, &reply);
        self.open_with_effects(effects, cx);
        cx.notify();
        true
    }

    /// Every key belongs to the card while it shows.
    pub(crate) fn open_with_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.open_with.is_none() {
            return false;
        }
        let Some(key) = key_of(event) else {
            cx.notify();
            return true;
        };
        let _ = window;
        let effects = self.open_with.as_mut().expect("an open card").card.key(key);
        self.open_with_effects(effects, cx);
        true
    }

    fn press_open_with(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(open) = self.open_with.as_mut() else {
            return;
        };
        let effects = open.card.press_at(at);
        self.open_with_effects(effects, cx);
    }

    fn open_with_effects(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        let Some(side) = self.open_with.as_ref().map(|open| open.side) else {
            return;
        };
        for effect in effects {
            match effect {
                Effect::Send(line) => {
                    self.open_with_send(side, line, cx);
                }
                Effect::Close => self.open_with = None,
                Effect::Message(text) => self.opener_say(side, text, false, cx),
                Effect::Launch(_) | Effect::EditNames { .. } | Effect::Refresh => {}
            }
        }
        cx.notify();
    }

    /// Whether the line reached the backend; a failure is recorded on the pane.
    fn open_with_send(&mut self, side: usize, line: String, cx: &mut Context<Self>) -> bool {
        let Some(host) = self.panes[side].as_mut() else {
            return false;
        };
        let result = host
            .backend
            .as_ref()
            .ok_or_else(|| crate::backend::Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            })
            .and_then(|backend| backend.operation(line));
        if let Err(error) = result {
            host.record_error(error, &self.locale);
            cx.notify();
            return false;
        }
        true
    }

    /// A new ask replaces the card that shows; an Open With card closes as Escape closes it, expiring its snapshot.
    pub(crate) fn dismiss_cards(&mut self, cx: &mut Context<Self>) {
        self.run_card = None;
        if let Some(open) = self.open_with.as_mut() {
            let effects = open.card.key(Key::Escape);
            self.open_with_effects(effects, cx);
        }
    }

    /// The card over one scrim on the window's highest layer; a press on the scrim cancels, a click on a button presses it as Enter does.
    pub(crate) fn open_with_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.open_with.as_ref()?;
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        let card = open.card.card(&self.locale.catalog);
        let room = f32::from(window.viewport_size().width);
        let press = cx.listener(|shell: &mut Shell, at: &usize, _, cx| {
            shell.press_open_with(*at, cx);
        });
        let drawn = StandIn.present_pressable(
            &card,
            &palette,
            &self.active_typeface(),
            self.corner,
            room,
            press,
        );
        let scrim = cx.listener(|shell: &mut Shell, _: &gpui::MouseDownEvent, _, cx| {
            if let Some(open) = shell.open_with.as_mut() {
                let effects = open.card.press_at(0);
                shell.open_with_effects(effects, cx);
            }
        });
        Some(
            div()
                .id("op3-openwith-scrim")
                .debug_selector(|| "op3-openwith-scrim".into())
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .bg(palette
                    .background
                    .opacity(flea::ui_logic_op1::card::scrim_alpha(0)))
                .on_mouse_down(MouseButton::Left, scrim)
                .child(drawn)
                .into_any_element(),
        )
    }
}
