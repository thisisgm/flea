//! The run card in the window: flea --open's run ask opens it, its keys and clicks press its buttons, and a launch's exit is read on the asking pane.
use crate::{
    g2::open_with_properties::{
        keys::{key_of, Key},
        lane,
        runcard::RunCard,
        shell::Probed,
        Effect,
    },
    state::Shell,
    window::navigate::Step,
};
use flea::ui_logic_open_with_properties::runfile::{exit_outcome, Action, Button, Exit};
use gpui::{
    div, prelude::*, AnyElement, Context, KeyDownEvent, ModifiersChangedEvent, MouseButton, Window,
};

/// The open card, the side that asked for it, and the launcher a test can replace.
pub(crate) struct Open {
    pub(crate) side: usize,
    pub(crate) card: RunCard,
    pub(crate) launch: fn(&[String]) -> lane::Exit,
    /// The card's and its buttons' laid-out boxes on the latest frame, for runCardState.
    pub(crate) probe: crate::ipc::card_geometry::CardGeometry,
}

impl Open {
    fn new(side: usize, card: RunCard) -> Self {
        Self {
            side,
            card,
            probe: Default::default(),
            #[cfg(not(test))]
            launch: lane::run,
            // A test window never runs a program; a test that drives the card sets its own launcher.
            #[cfg(test)]
            launch: |_| (None, String::new()),
        }
    }
}

impl Shell {
    /// The opener's run ask: the card opens on the asking pane, and an ask that does not parse says so and runs nothing.
    /// `shift` is the window's Shift at open, so a Shift held before the ask returned labels the card it opens.
    pub(crate) fn open_run_card(
        &mut self,
        side: usize,
        path: String,
        text: String,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_cards(cx);
        let flea = crate::window::startup::binary()
            .to_string_lossy()
            .into_owned();
        // v0.3.12 RunFile.listedGlyph: the mark the listing draws for the row this path still names, read once at open.
        let listed = self.panes[side].as_ref().and_then(|host| {
            host.pane
                .held
                .rows
                .iter()
                .find(|row| crate::navigation::history::join(host.pane.path(), &row.name) == path)
                .map(|row| {
                    crate::row::marks::Mark::for_row(&row.icon, row.mode)
                        .name()
                        .to_string()
                })
        });
        match RunCard::open(&self.locale.catalog, &flea, &path, &text) {
            Ok(mut card) => {
                if let Some(listed) = listed {
                    card.set_listed(&listed);
                }
                card.shift = shift;
                self.run_card = Some(Open::new(side, card));
            }
            Err(message) => self.opener_say(side, message, true, cx),
        }
        cx.notify();
    }

    /// Every key belongs to the card while it shows; Shift held relabels Run in Terminal before Enter reads it.
    pub(crate) fn run_card_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mirrored = window.layout_mirrored();
        let Some(open) = self.run_card.as_mut() else {
            return false;
        };
        open.card.shift = event.keystroke.modifiers.shift;
        let Some(key) = key_of(event).map(|key| {
            let key = match key {
                Key::Text(ref text) if text == "h" => Key::Left,
                Key::Text(ref text) if text == "l" => Key::Right,
                key => key,
            };
            key.read(mirrored)
        }) else {
            cx.notify();
            return true;
        };
        let action = if key == Key::Enter {
            open.card.action_of(open.card.focus)
        } else {
            None
        };
        let effects = open.card.key(key);
        self.run_card_effects(effects, action, window, cx);
        true
    }

    /// Shift alone relabels the open card on both press and release, before any key or click chooses an action.
    pub(crate) fn run_card_modifiers(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cx.has_active_drag() {
            self.refresh_file_cue(window, cx);
        }
        let Some(open) = self.run_card.as_mut() else {
            return;
        };
        if open.card.shift != event.modifiers.shift {
            open.card.shift = event.modifiers.shift;
            cx.notify();
        }
    }

    /// A click reads Shift from its own press, never from the last key the card saw.
    fn press_run_card(
        &mut self,
        at: usize,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.run_card.as_mut() else {
            return;
        };
        open.card.shift = shift;
        let action = open
            .card
            .button_at(at)
            .and_then(|button| open.card.action_of(button));
        let effects = open.card.press_at(at);
        self.run_card_effects(effects, action, window, cx);
    }

    fn run_card_effects(
        &mut self,
        effects: Vec<Effect>,
        action: Option<Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(side) = self.run_card.as_ref().map(|open| open.side) else {
            return;
        };
        for effect in effects {
            match effect {
                Effect::Close => self.run_card = None,
                Effect::Launch(argv) => {
                    let Some(action) = action else { continue };
                    let Some(open) = self.run_card.as_ref() else {
                        continue;
                    };
                    let launch = open.launch;
                    let target = argv.get(2).cloned().unwrap_or_default();
                    let exit = cx.background_executor().spawn(async move { launch(&argv) });
                    // Each launch outlives the card it closed and reads its own exit, so a second launch never drops the first.
                    cx.spawn_in(window, async move |this, cx| {
                        let (status, _) = exit.await;
                        let _ = this.update_in(cx, |shell, window, cx| {
                            shell.run_card_exited(side, action, target, status, window, cx)
                        });
                    })
                    .detach();
                }
                Effect::Message(text) => self.opener_say(side, text, false, cx),
                Effect::Send(_) | Effect::EditNames { .. } | Effect::Refresh => {}
            }
        }
        cx.notify();
    }

    /// A launch that ended: silence, the folder it named, or the sentence its action and status name, on the asking pane.
    fn run_card_exited(
        &mut self,
        side: usize,
        action: Action,
        target: String,
        status: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match exit_outcome(&self.locale.catalog, action, status) {
            Exit::Quiet => {}
            Exit::Navigate => {
                let _ = self.step(side, Step::Open(target), window, cx);
            }
            Exit::Message(text) => self.opener_say(side, text, true, cx),
        }
        cx.notify();
    }

    /// The card over one scrim on the window's highest layer; a press on the scrim cancels, a click on a button presses it as Enter does.
    pub(crate) fn run_card_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.run_card.as_ref()?;
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        let card = open.card.card(&self.locale.catalog);
        let room = f32::from(window.viewport_size().width);
        let press = cx.listener(|shell: &mut Shell, at: &usize, window, cx| {
            shell.press_run_card(*at, window.modifiers().shift, window, cx);
        });
        let drawn = Probed(&open.probe).present_pressable(
            &card,
            &palette,
            &self.active_typeface(),
            self.corner,
            room,
            press,
        );
        let scrim = cx.listener(|shell: &mut Shell, _: &gpui::MouseDownEvent, window, cx| {
            if let Some(open) = shell.run_card.as_mut() {
                let effects = open.card.press(Button::Cancel);
                shell.run_card_effects(effects, None, window, cx);
            }
        });
        Some(
            div()
                .id("op3-runcard-scrim")
                .debug_selector(|| "op3-runcard-scrim".into())
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .bg(palette
                    .background
                    .opacity(flea::ui_logic_file_ops::card::scrim_alpha(0)))
                .on_mouse_down(MouseButton::Left, scrim)
                .child(drawn)
                .into_any_element(),
        )
    }
}
