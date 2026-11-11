//! The open menu's keys, and what each menu outcome does to the window.
use super::{lines::Env, menu::Outcome};
use crate::{state::Shell, window::input::Dispatch};
use flea::jsondoc::Json;
use gpui::{Context, KeyDownEvent, Window};

impl Shell {
    pub(crate) fn menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let held = event.keystroke.modifiers;
        let tab = matches!(event.keystroke.key.as_str(), "tab" | "backtab");
        if self.menu.presentation.at.is_some() {
            if event.keystroke.key == "escape" {
                self.retire_menu(window, cx);
            }
            cx.stop_propagation();
            return true;
        }
        if self.menu_opened() {
            let preset = self
                .launch
                .settings
                .data
                .get("keys")
                .and_then(Json::as_str)
                .unwrap_or("default");
            // Tab and Shift+Tab walk the rows as Down and Up do; a modified Tab does nothing, and the menu keeps the keyboard.
            let action = if tab && (held.control || held.alt || held.platform) {
                Dispatch::Unbound
            } else if tab {
                Dispatch::Action(if held.shift || event.keystroke.key == "backtab" {
                    "focusPrevious"
                } else {
                    "focusNext"
                })
            } else {
                self.keyboard.resolve(
                    preset,
                    event,
                    window.modifiers(),
                    window.layout_mirrored(),
                    Default::default(),
                )
            };
            if let Dispatch::Action(action) = action {
                let side = self
                    .menu
                    .captured
                    .as_ref()
                    .map_or(self.focus_side, |c| c.side);
                let list = self.panes[side]
                    .as_ref()
                    .expect("the menu's pane")
                    .list
                    .borrow();
                let env = self.menu_env(&list.typeface);
                let live = self.menu_entries();
                let outcome = if matches!(action, "copyAs" | "pasteAs" | "tags") {
                    self.menu_submenu(action, &env, window)
                } else {
                    self.menu.menu.borrow_mut().act(
                        action,
                        Some(&live),
                        &self.menu_identity(),
                        &env,
                        window,
                    )
                };
                drop(list);
                self.menu_outcome(outcome, window, cx);
            }
            cx.stop_propagation();
            return true;
        }
        false
    }

    /// c, P or T over the open menu: the drawn row's flyout, a lone one over a hidden row, or a refusal, read against the captured listing.
    pub(crate) fn menu_submenu(&self, action: &str, env: &Env, window: &Window) -> Outcome {
        let mut menu = self.menu.menu.borrow_mut();
        menu.history = self.menu.context.recent;
        menu.open_submenu_for(action, self.menu.context.clipboard_available, env, window)
    }

    pub(crate) fn menu_outcome(
        &mut self,
        outcome: Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_outcome_with_detail(outcome, None, window, cx);
    }

    pub(crate) fn menu_outcome_with_detail(
        &mut self,
        outcome: Outcome,
        detail: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rename_menu_outcome(&outcome, window, cx)
            || self.tab_menu_outcome(&outcome, window, cx)
            || self.rail_menu_outcome(&outcome, window, cx)
        {
            return;
        }
        match outcome {
            Outcome::Closed => self.retire_menu(window, cx),
            Outcome::Refused(key) => {
                if let Some(waiting) = self.menu.external.take() {
                    waiting.answer(self.action_refused(key));
                }
                if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
                    let side = self
                        .menu
                        .captured
                        .as_ref()
                        .map_or(self.focus_side, |capture| capture.side);
                    self.menu_notice(side, detail.to_owned(), cx);
                } else {
                    self.menu_refuse(key, cx);
                }
                self.retire_menu(window, cx);
            }
            Outcome::Fire { action, leaf } => {
                if action != "tags" {
                    self.keyboard.context = flea::keys::Context::Listing;
                }
                if self.menu.activating || (self.menu.reading && self.menu.pending.is_some()) {
                    self.menu.tags.choices.push_back((action, leaf));
                    return;
                }
                self.menu.pending = Some((action, leaf));
                self.activate_menu(window, cx);
            }
            Outcome::Nothing | Outcome::Redraw => {}
        }
        cx.notify();
    }
}
