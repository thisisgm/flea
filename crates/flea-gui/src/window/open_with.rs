//! The Open With card in the window: flea --open's unclaimed ask snapshots its row and opens the card, as v0.3.12 Pane.showOpenWith does.
use crate::{
    g2::op3::{
        keys::{key_of, Key},
        openwith::OpenWith,
        shell::{Block, Control, DialogShell, OpenWithShell},
        Effect,
    },
    state::Shell,
};
use gpui::{div, prelude::*, AnyElement, Context, KeyDownEvent, MouseButton, Window};

#[path = "open_with_initial.rs"]
mod initial;

/// The open card, the side that asked for it, and the menuaction id its snapshot and requests carry.
pub(crate) struct Open {
    pub(crate) side: usize,
    pub(crate) id: u64,
    pub(crate) card: OpenWith,
    /// The parts the card painted last, which openWithState reports.
    pub(crate) frame: crate::g2::op1::measured::Frame,
    pub(crate) initial: crate::g2::op3::initial::Initial,
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
        self.open_with_row(side, listing, at, &name, cx);
    }

    /// The row menu's Another Application leaf: the card opens on the one row the menu captured, as the opener's ask does.
    pub(crate) fn open_with_captured(
        &mut self,
        captured: &crate::g2::mn1::join::Captured,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(row) = &captured.target {
            self.dismiss_cards(cx);
            self.menu.sequence += 1;
            let mut target = captured.clone();
            target.id = self.menu.sequence;
            if self.open_with_send(target.side, target.snapshot_request(), cx) {
                self.open_with = Some(Open {
                    side: target.side,
                    id: target.id,
                    card: OpenWith::open(target.id, &row.name),
                    frame: Default::default(),
                    initial: crate::g2::op3::initial::Initial::at(cx.background_executor().now()),
                });
            }
            return true;
        }
        let [at] = captured.rows[..] else {
            return false;
        };
        let name = self.panes[captured.side]
            .as_ref()
            .filter(|host| host.pane.held.listing == captured.listing)
            .and_then(|host| host.pane.held.row(at))
            .map(|row| row.name.clone());
        let Some(name) = name else {
            return false;
        };
        self.dismiss_cards(cx);
        self.open_with_row(captured.side, captured.listing, at, &name, cx);
        true
    }

    /// Snapshots one listing row under a fresh menu id and opens the card on it; the card asks its catalogue once the snapshot is held.
    fn open_with_row(
        &mut self,
        side: usize,
        listing: usize,
        at: usize,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        self.menu.sequence += 1;
        let id = self.menu.sequence;
        let snapshot = format!(
            "{{\"c\":\"menuaction\",\"op\":\"snapshot\",\"id\":{id},\"listing\":{listing},\"rows\":[{at}],\"cursor\":{at}}}"
        );
        // A snapshot the backend never took would leave a card whose applications never arrive.
        if !self.open_with_send(side, snapshot, cx) {
            return;
        }
        self.open_with = Some(Open {
            side,
            id,
            card: OpenWith::open(id, name),
            frame: Default::default(),
            initial: crate::g2::op3::initial::Initial::at(cx.background_executor().now()),
        });
        cx.notify();
    }

    /// A flyout application: the backend launches it on the menu's snapshot `id`, as v0.3.12 sends openWith with the chosen id.
    pub(crate) fn open_with_application(
        &mut self,
        side: usize,
        id: u64,
        application: &str,
        cx: &mut Context<Self>,
    ) {
        let launch = format!(
            "{{\"c\":\"menuaction\",\"op\":\"openWith\",\"id\":{id},\"application\":\"{}\"}}",
            flea::json::escape(application)
        );
        self.open_with_launch[side] = Some(id);
        self.menu_send(side, launch, cx);
    }

    /// Sample input: {"t":"menuaction","id":7,"op":"openWith","ok":false,"error":"Neovim could not start."}, with "cancelled":true when a newer menu took the slot; a failure says why, then the snapshot is freed.
    fn open_with_launched(&mut self, side: usize, line: &str, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.open_with_launch[side] else {
            return false;
        };
        let Ok(reply) = flea::jsondoc::parse(line) else {
            return false;
        };
        let field = |key| reply.get(key).and_then(flea::jsondoc::Json::as_bool);
        if reply.get("op").and_then(flea::jsondoc::Json::as_str) != Some("openWith")
            || reply.get("id").and_then(flea::jsondoc::Json::as_f64) != Some(id as f64)
        {
            return false;
        }
        self.open_with_launch[side] = None;
        if field("ok") != Some(true) && field("cancelled") != Some(true) {
            let error = reply
                .get("error")
                .and_then(flea::jsondoc::Json::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| self.locale.catalog.tr("dialogs.file.openwith.open_failed"));
            self.menu_notice(side, error, cx);
        }
        self.menu_send(
            side,
            format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{id}}}"),
            cx,
        );
        true
    }

    /// One backend line the open card owns: its snapshot's answer, its applications catalogue or its openWith answer, under its own id.
    pub(crate) fn open_with_line(
        &mut self,
        side: usize,
        line: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.open_with_launched(side, line, cx) {
            return true;
        }
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
                Some("snapshot" | "applications" | "openWith")
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

    /// Every key belongs to the card while it shows; the search's typed text reaches it through the platform.
    pub(crate) fn open_with_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.open_with.is_none() {
            return false;
        }
        if self
            .open_with
            .as_ref()
            .is_some_and(|open| open.initial.published())
            && self.dialog_key(event, cx)
        {
            return true;
        }
        cx.stop_propagation();
        let Some(key) = key_of(event).map(|key| key.read(window.layout_mirrored())) else {
            cx.notify();
            return true;
        };
        let open = self.open_with.as_mut().expect("an open card");
        let Some(key) = open.initial.key(key) else {
            return true;
        };
        let effects = open.card.key(key);
        self.open_with_effects(effects, cx);
        true
    }

    fn activate_open_with(&mut self, control: Control, cx: &mut Context<Self>) {
        let Some(open) = self.open_with.as_mut() else {
            return;
        };
        let effects = open.card.activate(control);
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
        self.sync_dialog_editor();
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
        self.cancel_plugin_input(cx);
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
        self.sync_dialog_editor();
        let mut card = self.initial_open_with_card(cx)?;
        let open = self.open_with.as_ref()?;
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        self.rename.ime.palette = Some(crate::g2::op1::palette::CardPalette::from_list(
            &palette,
            self.corner,
        ));
        self.rename.ime.error_ink = false;
        if self.rename.ime.owner == Some(crate::g2::op1::dialog_editor::Slot::OpenWithSearch) {
            if let Some(Block::Field(search)) = card.body.first_mut() {
                search.input = Some(cx.entity());
            }
        }
        self.load_open_with_icons(&card, window, cx);
        // Parts the card no longer draws, such as the list behind a no-match note, leave the record.
        open.frame.clear();
        let room = f32::from(window.viewport_size().width);
        let weak = cx.entity().downgrade();
        let wire = move |control, element: gpui::Stateful<gpui::Div>| {
            let weak = weak.clone();
            if control == crate::g2::op3::shell::Control::Applications {
                return element.on_scroll_wheel(move |event, _, cx| {
                    let _ = weak.update(cx, |shell, cx| {
                        let row = crate::g2::op3::shell::APP_ROW
                            * f32::from(shell.active_typeface().body)
                            / crate::g2::op3::shell::BOARD_BODY;
                        if let Some(open) = shell.open_with.as_mut() {
                            open.card.scroll(event, row);
                            cx.notify();
                        }
                    });
                    cx.stop_propagation();
                });
            }
            let hover = weak.clone();
            element
                .on_mouse_move(move |_, _, cx| {
                    if let crate::g2::op3::shell::Control::Application(at) = control {
                        let _ = hover.update(cx, |shell, cx| {
                            if shell
                                .open_with
                                .as_mut()
                                .is_some_and(|open| open.card.hover(at))
                            {
                                cx.notify();
                            }
                        });
                        cx.stop_propagation();
                    }
                })
                .on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |shell, cx| shell.activate_open_with(control, cx));
                    cx.stop_propagation();
                })
        };
        let drawn = OpenWithShell {
            wire: &wire,
            surface: theme.surface,
            measure: &open.frame,
        }
        .present(&card, &palette, &self.active_typeface(), self.corner, room);
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
