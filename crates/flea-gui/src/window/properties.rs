//! The Properties card the file menu's row opens, on the captured pane's wire, held as the bulk rename card is.
use crate::{
    backend::Failure,
    g2::{
        context_menus::join::{identity, Captured, ProducerReply},
        open_with_properties::{
            keys::{key_of, Key},
            properties::Properties,
            shell::Probed,
            Effect,
        },
    },
    state::Shell,
    window::navigate::Step,
};
use flea::jsondoc::{self, Json};
use gpui::{div, prelude::*, AnyElement, Context, KeyDownEvent, MouseButton, Subscription, Window};

/// The open card, the side whose rows it inspects, and the reply subscription held while it shows.
pub(crate) struct Open {
    side: usize,
    pub(crate) card: Properties,
    pub(crate) probe: crate::ipc::card_geometry::CardGeometry,
    _reply: Subscription,
    initial: crate::g2::open_with_properties::initial::Initial,
}

#[cfg(test)]
impl Open {
    pub(crate) fn ready(&self) -> bool {
        self.card.ready()
    }

    pub(crate) fn card(&self) -> &Properties {
        &self.card
    }
}

impl Shell {
    /// The listing's Properties key, PaneMenuActions.qml open: the card asks for the rows the menu row would take, with no menu drawn.
    pub(crate) fn properties_listing(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let rows: Vec<usize> = if host.selected.is_empty() {
            vec![host.pane.cursor]
        } else {
            host.selected.clone()
        };
        let rows: Vec<usize> = rows
            .into_iter()
            .filter(|&row| host.pane.held.row(row).is_some())
            .collect();
        if rows.is_empty() {
            return true;
        }
        self.menu.sequence += 1;
        let captured = Captured {
            side,
            listing: host.pane.held.listing,
            rows,
            cursor: host.pane.cursor,
            path: host.path.clone(),
            identity: identity(host, side, false),
            id: self.menu.sequence,
            ready: false,
            kinds: Vec::new(),
            background: false,
            target: None,
        };
        self.open_properties_card(&captured, window, cx);
        true
    }

    /// A validated Properties choice or the listing key: the card takes its own snapshot of these rows, then asks their facts.
    pub(crate) fn open_properties_card(
        &mut self,
        captured: &Captured,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = captured.side;
        let reply = cx.subscribe_in(
            &cx.entity(),
            window,
            |shell, _, event: &ProducerReply, window, cx| {
                // {"t":"menuaction","op":"properties","id":8,"ok":true,"path":"/files/notes.txt","kind":"Plain text document","bytes":99}
                let Ok(reply) = jsondoc::parse(&event.line) else {
                    return;
                };
                if reply.get("t").and_then(Json::as_str) == Some("menuaction") {
                    shell.properties_reply(event.side, &reply, window, cx);
                }
            },
        );
        let (mut card, mut effects) = Properties::open(
            captured.id,
            captured.listing,
            &captured.rows,
            captured.cursor,
        );
        if captured.target.is_some() {
            effects = vec![Effect::Send(captured.snapshot_request())];
        }
        // Kind prints the MIME database's word the listing already holds for the row, as the preview column does.
        if let [row] = captured.rows[..] {
            let held = self.panes[side]
                .as_ref()
                .filter(|host| host.pane.held.listing == captured.listing)
                .and_then(|host| captured.row(host, row));
            if let Some(held) = held {
                card.set_extras(Vec::new(), &held.kind);
            }
        }
        self.properties = Some(Open {
            side,
            card,
            probe: Default::default(),
            _reply: reply,
            initial: crate::g2::open_with_properties::initial::Initial::at(
                cx.background_executor().now(),
            ),
        });
        self.properties_effects(effects, window, cx);
        cx.notify();
    }

    fn properties_reply(
        &mut self,
        side: usize,
        reply: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let catalog = self.locale.catalog.clone();
        let effects = match self.properties.as_mut() {
            Some(open) if open.side == side => open.card.reply(&catalog, reply),
            _ => return,
        };
        self.properties_effects(effects, window, cx);
        cx.notify();
    }

    /// The card's keys while it holds the keyboard; every other key is swallowed with it.
    pub(crate) fn properties_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.properties.is_none() {
            return false;
        }
        if let Some(key) = key_of(event).map(|key| key.read(window.layout_mirrored())) {
            let Some(key) = self
                .properties
                .as_ref()
                .and_then(|open| open.initial.key(key))
            else {
                cx.stop_propagation();
                return true;
            };
            let effects = self
                .properties
                .as_mut()
                .expect("an open card")
                .card
                .key(key);
            self.properties_effects(effects, window, cx);
        }
        cx.stop_propagation();
        true
    }

    fn properties_effects(
        &mut self,
        effects: Vec<Effect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(side) = self.properties.as_ref().map(|open| open.side) else {
            return;
        };
        for effect in effects {
            match effect {
                Effect::Send(line) => {
                    if let Some(host) = &mut self.panes[side] {
                        let result = host
                            .backend
                            .as_ref()
                            .ok_or_else(|| Failure {
                                key: "window.backend_stopped",
                                detail: String::new(),
                            })
                            .and_then(|backend| backend.line(line));
                        if let Err(error) = result {
                            host.record_error(error, &self.locale);
                        }
                    }
                }
                Effect::Close => self.properties = None,
                Effect::Message(text) => self.menu_notice(side, text, cx),
                Effect::Refresh => {
                    let _ = self.step(side, Step::Reload, window, cx);
                }
                _ => {}
            }
        }
        cx.notify();
    }

    /// The card over a scrim at the highest layer while it shows; a click on the scrim cancels it.
    pub(crate) fn properties_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.properties.as_mut()?;
        open.probe.clear();
        let card = open.initial.present(
            open.card.ready(),
            open.card.card(&self.locale.catalog),
            crate::g2::open_with_properties::initial::loading(
                &self.locale.catalog,
                "dialogs.file.properties.title",
                crate::g2::open_with_properties::properties::WIDTH,
            ),
            cx,
        )?;
        let open = self.properties.as_ref()?;
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        let room = f32::from(window.viewport_size().width);
        let press = cx.listener(|shell: &mut Shell, _: &usize, window, cx| {
            if let Some(open) = shell.properties.as_mut() {
                let effects = open.card.close();
                shell.properties_effects(effects, window, cx);
            }
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
            if shell.properties.is_some() {
                let effects = shell
                    .properties
                    .as_mut()
                    .expect("an open card")
                    .card
                    .key(Key::Escape);
                shell.properties_effects(effects, window, cx);
            }
        });
        Some(
            div()
                .id("op3-properties-scrim")
                .debug_selector(|| "op3-properties-scrim".into())
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
