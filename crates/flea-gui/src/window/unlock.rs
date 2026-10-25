//! The unlock card's window owner: a locked rail row opens UnlockView over the panes, as ui/js/Unlock.js did in v0.3.12.
use crate::g2::pl1::host::Effect;
use crate::g2::pl1::seams::RailView;
use crate::g2::pl1::view::{HostEvent, UnlockView};
use crate::state::Shell;
use crate::window::navigate::Step;
use flea::ui_logic_pl4::rail::Group;
use gpui::{div, prelude::*, AnyElement, App, Context, Entity, Subscription, Window};

/// The card, the pane whose locked row opened it, and the card's effect subscription.
pub(crate) struct Unlock {
    pub(crate) view: Entity<UnlockView>,
    side: usize,
    _events: Subscription,
}

impl Shell {
    /// A locked row's typed request opens the card over that row; the card is built on first use.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_unlock(
        &mut self,
        side: usize,
        device: &str,
        label: &str,
        size: u64,
        fstype: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rail = self.unlock_rail(cx);
        let Some(join) = &mut self.rail else {
            return;
        };
        if join.unlock.is_none() {
            let (catalog, runner) = (self.locale.catalog.clone(), join.device_runner.clone());
            let view = cx.new(|cx| UnlockView::new(catalog, runner, cx));
            let events = cx.subscribe_in(&view, window, |shell, _, event: &HostEvent, window, cx| {
                shell.unlock_effect(&event.0, window, cx)
            });
            join.unlock = Some(Unlock {
                view,
                side,
                _events: events,
            });
        }
        let Some(unlock) = join.unlock.as_mut() else {
            return;
        };
        unlock.side = side.min(1);
        let view = unlock.view.clone();
        view.update(cx, |view, cx| {
            view.rail = rail;
            view.open_for(device, label, size, fstype, window, cx)
        });
        cx.notify();
    }

    /// The rail rebuilt: the card names a pending drive and lands the cursor on the unlocked row.
    pub(crate) fn unlock_rail_changed(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.unlock_view() else {
            return;
        };
        let rail = self.unlock_rail(cx);
        view.update(cx, |view, cx| view.rail_changed(rail, cx));
    }

    pub(crate) fn unlock_holds_keyboard(&self, cx: &App) -> bool {
        self.unlock_view()
            .is_some_and(|view| view.read(cx).host.opened())
    }

    /// The card centred over the window under its own focus, above the panes and the rail.
    pub(crate) fn unlock_layer(&self, cx: &App) -> Option<AnyElement> {
        let view = self.unlock_view().filter(|_| self.unlock_holds_keyboard(cx))?;
        Some(
            gpui::deferred(div().absolute().inset_0().child(view))
                .with_priority(2)
                .into_any_element(),
        )
    }

    fn unlock_view(&self) -> Option<Entity<UnlockView>> {
        self.rail
            .as_ref()
            .and_then(|join| join.unlock.as_ref())
            .map(|unlock| unlock.view.clone())
    }

    /// The rail as the unlock host reads it: places and network counts ahead of the source's device rows.
    fn unlock_rail(&self, cx: &App) -> Option<RailView> {
        let join = self.rail.as_ref()?;
        let producer = join.places.read(cx);
        let entries = producer.entries();
        let places = entries
            .iter()
            .take_while(|row| !matches!(row.group, Group::Network | Group::Device))
            .count();
        let network = entries
            .iter()
            .filter(|row| row.group == Group::Network)
            .count();
        Some(RailView {
            places,
            network,
            devices: join.devices.read(cx).devices.entries.clone(),
            cursor: producer.cursor(),
        })
    }

    fn unlock_effect(&mut self, effect: &Effect, window: &mut Window, cx: &mut Context<Self>) {
        let side = self
            .rail
            .as_ref()
            .and_then(|join| join.unlock.as_ref())
            .map_or(self.focus_side, |unlock| unlock.side);
        match effect {
            Effect::OpenInPane(path) | Effect::RailOpened(path) => {
                let _ = self.step(side, Step::OpenPlace(path.clone()), window, cx);
            }
            Effect::PollDevices => {
                if let Some(join) = &self.rail {
                    join.devices.update(cx, |source, cx| source.poll_now(cx));
                }
            }
            Effect::SetCursor(index) => {
                if let Some(join) = &self.rail {
                    join.places.update(cx, |producer, cx| producer.set_cursor(*index, cx));
                }
            }
            Effect::Say(note, error) => {
                let text = crate::g2::pl1::words::say(&self.locale.catalog, note);
                self.status_messages(cx, |messages, now| {
                    messages.say(&text, *error, "", flea::ui_logic_fb1::status::Hint::None, now)
                });
            }
            Effect::FocusPane => {
                window.focus(&self.focus, cx);
                self.keyboard.context = flea::keys::Context::Listing;
            }
            // The card's own children and name poll stay inside UnlockView.
            Effect::Card(_) | Effect::NameList(_) | Effect::NameKill | Effect::NameBound => {}
        }
        cx.notify();
    }
}
