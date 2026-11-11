//! The Trash menus use mn1's fitted, shaped custom Element and return choices to their original selection.
use super::seams::At;
use crate::{
    g2::context_menus::{
        element::MenuElement,
        lines::Env,
        menu::{Face, Outcome},
    },
    list::Typeface,
    state::Shell,
};
use flea::ui_logic_context_menus::{fit::Rect, Entry, Text};
use flea::ui_logic_trash_browser::host::Item;
use gpui::{
    div, prelude::*, px, AnyElement, Context, MouseButton, MouseDownEvent, MouseMoveEvent,
    ScrollWheelEvent, Window,
};

fn menu_env<'a>(
    catalog: &'a flea::i18n::I18n,
    settings: &'a flea::jsondoc::Json,
    typeface: &'a Typeface,
) -> Env<'a> {
    Env {
        catalog,
        preset: settings
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default"),
        key_hints: settings
            .get("keyHints")
            .and_then(flea::jsondoc::Json::as_bool)
            == Some(true),
        typeface,
        inset: 4.,
    }
}

impl Shell {
    pub(crate) fn restyle_trash_menu(&self, face: &Typeface, window: &Window) {
        let Some(host) = &self.trash else { return };
        let entries = host
            .menu
            .borrow()
            .card
            .as_ref()
            .map(|card| card.entries.clone());
        if let Some(entries) = entries {
            let env = menu_env(&self.locale.catalog, &self.launch.settings.data, face);
            host.menu.borrow_mut().refresh(entries, &env, window);
        }
    }

    pub(super) fn open_trash_menu(
        &mut self,
        items: Vec<Item>,
        at: At,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let geometry = self.geometry(window);
        let typeface = self.active_typeface();
        let Some(host) = &mut self.trash else {
            return;
        };
        let pane = host.pane.read(cx);
        host.menu_selection = pane.view().selection.clone();
        let entries = items
            .into_iter()
            .map(|item| match item {
                Item::Separator => Entry::separator(),
                Item::Entry(item) => Entry {
                    action: item.action.into(),
                    disabled: item.disabled,
                    // MenuRow.qml: an unavailable row takes the muted ink before the destructive one (EY-198).
                    danger: item.danger && !item.disabled,
                    ..Entry::row(item.id, Text::Key(item.label_key), item.glyph)
                },
            })
            .collect();
        let env = menu_env(&self.locale.catalog, &self.launch.settings.data, &typeface);
        let size = window.viewport_size();
        let rect = geometry.panes[host.side];
        let list_bounds = pane.paint.borrow().bounds;
        let origin = list_bounds.map_or((rect.x, rect.y), |bounds| {
            (
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y) - pane.board().top(),
            )
        });
        let anchor = (origin.0 + at.x, origin.1 + at.y);
        host.menu.borrow_mut().open(
            Face::Trash,
            entries,
            anchor,
            Rect {
                x: 0.,
                y: 0.,
                w: size.width.into(),
                h: size.height.into(),
            },
            "",
            &env,
            window,
        );
        cx.notify();
    }

    fn trash_menu_live(&self, cx: &gpui::App) -> bool {
        self.trash.as_ref().is_some_and(|host| {
            let pane = host.pane.read(cx);
            !pane.view().busy() && host.menu_selection == pane.view().selection
        })
    }

    fn trash_menu_outcome(
        &mut self,
        outcome: Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = match outcome {
            Outcome::Fire { action, .. } => Some(action),
            Outcome::Closed => None,
            _ => {
                cx.notify();
                return;
            }
        };
        let live = self.trash_menu_live(cx);
        let Some(host) = &self.trash else {
            return;
        };
        let pane = host.pane.clone();
        if !live && action.is_some() {
            pane.update(cx, |pane, cx| pane.menu_action(None, cx));
            self.trash_say(
                &self.locale.catalog.tr("trash.error.selection_changed"),
                true,
                cx,
            );
        } else {
            pane.update(cx, |pane, cx| pane.menu_action(action.as_deref(), cx));
        }
        let focus = pane.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.flush_trash(window, cx);
    }

    pub(super) fn trash_menu_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &self.trash else {
            return;
        };
        let typeface = self.active_typeface();
        let env = menu_env(&self.locale.catalog, &self.launch.settings.data, &typeface);
        let outcome = host.menu.borrow_mut().act(action, None, "", &env, window);
        self.trash_menu_outcome(outcome, window, cx);
    }

    pub(crate) fn trash_menu_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let host = self
            .trash
            .as_ref()
            .filter(|host| host.menu.borrow().visible())?;
        // A closing card fades out without taking the pointer.
        if !host.menu.borrow().opened() {
            return Some(
                div()
                    .absolute()
                    .inset_0()
                    .child(MenuElement::new(host.menu.clone(), px(self.corner)))
                    .into_any_element(),
            );
        }
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_mouse_move(cx.listener(|shell, event: &MouseMoveEvent, window, cx| {
                    let Some(host) = &shell.trash else {
                        return;
                    };
                    let typeface = shell.active_typeface();
                    let env = menu_env(
                        &shell.locale.catalog,
                        &shell.launch.settings.data,
                        &typeface,
                    );
                    host.menu.borrow_mut().hover(
                        event.position.x.into(),
                        event.position.y.into(),
                        &env,
                        window,
                    );
                    cx.notify();
                    cx.stop_propagation();
                }))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::trash_menu_click))
                .on_mouse_down(MouseButton::Right, cx.listener(Self::trash_menu_click))
                .on_scroll_wheel(cx.listener(|shell, event: &ScrollWheelEvent, _, cx| {
                    if let Some(host) = &shell.trash {
                        host.menu.borrow_mut().touchpad(
                            event.position.x.into(),
                            event.position.y.into(),
                            -f32::from(event.delta.pixel_delta(px(37.)).y),
                            false,
                        );
                    }
                    cx.stop_propagation();
                    cx.notify();
                }))
                .child(MenuElement::new(host.menu.clone(), px(self.corner)))
                .into_any_element(),
        )
    }

    fn trash_menu_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &self.trash else {
            return;
        };
        let typeface = self.active_typeface();
        let env = menu_env(&self.locale.catalog, &self.launch.settings.data, &typeface);
        let outcome = host.menu.borrow_mut().click(
            event.position.x.into(),
            event.position.y.into(),
            None,
            "",
            &env,
            window,
        );
        self.trash_menu_outcome(outcome, window, cx);
        cx.stop_propagation();
    }
}
