//! The bulk rename card for a multi-selection: Pane.qml openBulkRename, its paths round trip, the card's keys and effects, and its done line.
use super::Shell;
use crate::g2::open_with_properties::{
    bulkrename::BulkRename, keys::key_of, lane, shell::StandIn, Effect,
};
use flea::ui_logic_file_ops::lines;
use gpui::{div, prelude::*, AnyElement, Context, KeyDownEvent, MouseButton, Window};

/// The open card and the side whose selection it renames.
pub(crate) struct Open {
    pub side: usize,
    pub card: BulkRename,
    /// The request is out, so the card's done line and refresh belong to this side.
    pub sent: bool,
    pub _editor: Option<gpui::Task<()>>,
}

impl Shell {
    /// Several marked rows: their absolute paths are asked of the backend, as v0.3.12 asked them.
    pub(super) fn bulk_rename_ask(&mut self, side: usize, cx: &mut Context<Self>) -> bool {
        let catalog = self.locale.catalog.clone();
        let Some(host) = self.panes[side].as_mut() else {
            return false;
        };
        if self.rename.paths.is_some() || host.nav.gate.paths_pending {
            let said = lines::paths_busy(&catalog);
            self.rename_say(said, false, cx);
            return true;
        }
        let Some(action) = host.pane.row_action(host.selected.clone()) else {
            let said = lines::no_target(&catalog, host.pane.cursor as i64, false);
            self.rename_say(said, false, cx);
            return true;
        };
        match host
            .backend
            .as_ref()
            .ok_or_else(stopped)
            .and_then(|backend| backend.rows("paths", action))
        {
            Ok(()) => self.rename.paths = Some(side),
            Err(error) => {
                host.record_error(error, &self.locale);
                cx.notify();
            }
        }
        true
    }

    /// The backend's answer: two or more paths open the card, fewer say there is nothing to rename.
    pub(super) fn bulk_rename_open(
        &mut self,
        side: usize,
        paths: Vec<String>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if paths.len() <= 1 {
            let said = lines::no_target(&self.locale.catalog, -1, false);
            self.rename_say(said, false, cx);
            return;
        }
        self.rename.ime.close();
        self.rename.edit = None;
        self.rename.bulk = Some(Open {
            side,
            card: BulkRename::open(paths),
            sent: false,
            _editor: None,
        });
        cx.notify();
    }

    pub(super) fn bulk_rename_key(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_key(event, cx) {
            return;
        }
        cx.stop_propagation();
        let Some(key) = key_of(event).map(|key| key.read(window.layout_mirrored())) else {
            return;
        };
        let Some(open) = self.rename.bulk.as_mut() else {
            return;
        };
        let effects = open.card.key(key);
        self.bulk_rename_effects(effects, cx);
    }

    fn bulk_rename_effects(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        let Some(side) = self.rename.bulk.as_ref().map(|open| open.side) else {
            return;
        };
        for effect in effects {
            match effect {
                Effect::Send(line) => {
                    let Some(host) = self.panes[side].as_mut() else {
                        continue;
                    };
                    match host
                        .backend
                        .as_ref()
                        .ok_or_else(stopped)
                        .and_then(|backend| backend.operation(line))
                    {
                        Ok(()) => self.rename.bulk.as_mut().expect("an open card").sent = true,
                        Err(error) => host.record_error(error, &self.locale),
                    }
                }
                Effect::Close => {
                    // A sent request keeps its side until the done line, with the card gone.
                    if self.rename.bulk.as_ref().is_some_and(|open| open.sent) {
                        let open = self.rename.bulk.take().expect("an open card");
                        self.rename.sent[open.side] = true;
                    } else {
                        self.rename.bulk = None;
                    }
                }
                Effect::EditNames { folder, names } => {
                    let editor = lane::session_editor();
                    let base = lane::scratch_base();
                    let task = cx.spawn(async move |shell, cx| {
                        let outcome = cx
                            .background_executor()
                            .spawn(async move {
                                lane::edit_names(&base, &folder, editor.as_deref(), &names)
                            })
                            .await;
                        let _ = shell.update(cx, |shell, cx| {
                            let catalog = shell.locale.catalog.clone();
                            if let Some(open) = shell.rename.bulk.as_mut() {
                                open.card.edited(&catalog, outcome);
                                cx.notify();
                            }
                        });
                    });
                    if let Some(open) = self.rename.bulk.as_mut() {
                        open._editor = Some(task);
                    }
                }
                Effect::Message(text) => self.rename_say(text, false, cx),
                Effect::Launch(_) | Effect::Refresh => {}
            }
        }
        self.sync_dialog_editor();
        cx.notify();
    }

    /// PaneWire.qml onBulkRenamed: one line for the whole batch, the marks cleared and the folder read again.
    pub(super) fn bulk_rename_line(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.rename.sent[side] {
            return false;
        }
        // Sample input: {"t":"bulkrenamed","ok":5,"unchanged":1}
        let Ok(doc) = flea::jsondoc::parse(line) else {
            return false;
        };
        if doc.get("t").and_then(flea::jsondoc::Json::as_str) != Some("bulkrenamed") {
            return false;
        }
        self.rename.sent[side] = false;
        let ok = doc
            .get("ok")
            .and_then(flea::jsondoc::Json::as_f64)
            .map_or(0, |ok| ok.max(0.) as u64);
        let preset = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default")
            .to_owned();
        let said = lines::bulk_done_line(&self.locale.catalog, &preset, ok);
        if let Some(host) = self.panes[side].as_mut() {
            host.selected.clear();
            host.input.selection.changed(&host.selected);
        }
        self.rename_say(said, false, cx);
        let _ = self.step(
            side,
            super::super::navigate::Step::Refresh(None),
            window,
            cx,
        );
        true
    }

    /// The card over one scrim on the window's highest layer; a press on the scrim cancels, a click on a button presses it as Enter does.
    pub(crate) fn bulk_rename_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.sync_dialog_editor();
        let open = self.rename.bulk.as_ref()?;

        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        let mut card = open.card.card(&self.locale.catalog);
        self.rename.ime.palette = Some(crate::g2::file_ops::palette::CardPalette::from_list(
            &palette,
            self.corner,
        ));
        self.rename.ime.error_ink = false;
        for block in &mut card.body {
            use crate::g2::open_with_properties::shell::Block;
            match block {
                Block::Fields(a, b) => {
                    if a.focused
                        && self.rename.ime.owner
                            == Some(crate::g2::file_ops::dialog_editor::Slot::BulkFind)
                    {
                        a.input = Some(cx.entity());
                    }
                    if b.focused
                        && self.rename.ime.owner
                            == Some(crate::g2::file_ops::dialog_editor::Slot::BulkReplace)
                    {
                        b.input = Some(cx.entity());
                    }
                }
                Block::Names { new, .. } => {
                    if let Some(crate::g2::file_ops::dialog_editor::Slot::BulkLine(at)) =
                        self.rename.ime.owner
                    {
                        if let Some(line) = new.get_mut(at) {
                            self.rename.ime.error_ink =
                                line.ink == crate::g2::open_with_properties::shell::Ink::Error;
                            line.input = Some(cx.entity());
                        }
                    }
                }
                _ => {}
            }
        }
        let room = f32::from(window.viewport_size().width);
        let press = cx.listener(|shell: &mut Shell, at: &usize, _, cx| {
            if let Some(open) = shell.rename.bulk.as_mut() {
                let effects = open.card.press_button(*at);
                shell.bulk_rename_effects(effects, cx);
            }
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
            if let Some(open) = shell.rename.bulk.as_mut() {
                let effects = open
                    .card
                    .press(flea::ui_logic_open_with_properties::bulkrename::Stop::Cancel);
                shell.bulk_rename_effects(effects, cx);
            }
        });
        Some(
            div()
                .id("op3-bulkrename-scrim")
                .debug_selector(|| "op3-bulkrename-scrim".into())
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .bg(palette
                    .background
                    .opacity(flea::ui_logic_file_ops::card::scrim_alpha(0)))
                .on_mouse_down(MouseButton::Left, scrim)
                .child(
                    div()
                        .size_full()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(drawn),
                )
                .into_any_element(),
        )
    }
}

/// The pane has no backend to ask, as every other request reports it.
fn stopped() -> crate::backend::Failure {
    crate::backend::Failure {
        key: "window.backend_stopped",
        detail: String::new(),
    }
}
