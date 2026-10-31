//! The existing filter menu card handles both keyboard and pointer choices.
use super::chrome::{ChipMenu, MENU_INSET};
use crate::g2::mn1::{lines::Env, menu::Outcome};
use gpui::{Context, EventEmitter, KeyDownEvent, Window};

#[derive(Clone, Debug)]
pub struct FilterChosen(pub i64);
impl EventEmitter<FilterChosen> for ChipMenu {}

impl ChipMenu {
    // PickerFilter.qml opens with the caller's active filter already under the cursor.
    pub fn select_chip(
        &mut self,
        chips: &[flea::ui_logic_pk2::request::Chip],
        index: i64,
        window: &Window,
    ) {
        let env = Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: true,
            typeface: &self.typeface,
            inset: MENU_INSET,
        };
        for _ in 0..chips
            .iter()
            .position(|chip| chip.index == index)
            .unwrap_or(0)
        {
            self.menu
                .borrow_mut()
                .act("cursorDown", None, "chips", &env, window);
        }
    }

    fn chosen(outcome: Outcome) -> Option<i64> {
        match outcome {
            Outcome::Fire { action, .. } => action.strip_prefix("chip:")?.parse().ok(),
            _ => None,
        }
    }

    pub fn key(&mut self, event: &KeyDownEvent, window: &Window) -> Option<i64> {
        let action = match event.keystroke.key.as_str() {
            "down" => "cursorDown",
            "up" => "cursorUp",
            "enter" | "space" => "open",
            "escape" => "escape",
            _ => return None,
        };
        let env = Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: true,
            typeface: &self.typeface,
            inset: MENU_INSET,
        };
        Self::chosen(
            self.menu
                .borrow_mut()
                .act(action, None, "chips", &env, window),
        )
    }

    pub(super) fn press(&mut self, x: f32, y: f32, window: &Window, cx: &mut Context<Self>) {
        let env = Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: true,
            typeface: &self.typeface,
            inset: MENU_INSET,
        };
        let chosen = Self::chosen(
            self.menu
                .borrow_mut()
                .click(x, y, None, "chips", &env, window),
        );
        if let Some(index) = chosen {
            cx.emit(FilterChosen(index));
        }
        cx.notify();
    }
}
