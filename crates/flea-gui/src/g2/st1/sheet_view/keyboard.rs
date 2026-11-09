//! The sheet's drawn keyboard: where it fits, what the held modifiers and looked-up chords make of it.
//!
//! Caps draw in three looks (caps.rs): faint does nothing under the held set, bound does, lit is held or looked up.
//! A window too small for the keyboard shows a small card asking for a larger window.
use super::super::geometry::CLAMP;
use super::super::input::Host;
use super::{SheetView, FIELD_H, KEY_ROW, PAD, SHEET_H, STACK_GAP, TITLE_H};
use crate::window::input::modifiers as mask;
use flea::keys::physical::{ROWS, WIDTH};
use flea::keys::{ALT, CONTROL, SUPER};
use gpui::{
    prelude::*, Context, Div, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, Stateful, Window,
};

#[path = "caps.rs"]
mod caps;
#[path = "corner.rs"]
mod corner;
#[path = "heads.rs"]
mod heads;
#[path = "held.rs"]
mod held;

/// The key unit's bounds: the keyboard shrinks to fit a narrow window and leaves it below the smallest unit.
const MAX_UNIT: f32 = 40.;
const MIN_UNIT: f32 = 28.;
/// The function row's extra distance from the main block.
const ROW_GAP: f32 = 10.;
/// The gap between caps; the band ends at its last cap, so the card's padding is equal on every side.
const CAP_GAP: f32 = 4.;
/// The shortest list a keyboard leaves below it: eight rows, so the sheet stays a list first.
const LIST_ROWS: f32 = 8.;
/// The card that asks for a larger window: as wide as its hint reads well, the title over four rows of room.
const SMALL_W: f32 = 420.;
const SMALL_H: f32 = 2. + 2. * PAD + TITLE_H + 1. + STACK_GAP + 4. * KEY_ROW;
/// Keys the field edits with any modifier while it holds text, as it does without a keyboard.
const FIELD_KEYS: [&str; 6] = ["backspace", "delete", "left", "right", "home", "end"];
/// The clipboard and select-all keys the field takes with Ctrl alone.
const FIELD_CTRL_KEYS: [&str; 4] = ["a", "c", "x", "v"];

/// The drawn keyboard's height at this key unit: its rows and the function row's gap.
fn keyboard_height(unit: f32) -> f32 {
    f32::from(ROWS) * unit + ROW_GAP - CAP_GAP
}

/// The drawn keyboard's width at this key unit.
fn keyboard_width(unit: f32) -> f32 {
    WIDTH * unit - CAP_GAP
}

/// The card's chrome around the keyboard and the list, as the sheet stacks it.
fn chrome(searching: bool) -> f32 {
    let field = if searching { FIELD_H + STACK_GAP } else { 0. };
    2. + 2. * PAD + TITLE_H + 1. + 3. * STACK_GAP + field
}

impl SheetView {
    /// The key unit this window fits, or none when it is too small for a keyboard above a usable list.
    pub(super) fn unit(&self, window: &Window) -> Option<f32> {
        self.sheet.keyboard.as_ref()?;
        let size = window.viewport_size();
        let room_w = f32::from(size.width) - 2. * CLAMP;
        let room_h = f32::from(size.height) - 2. * CLAMP;
        let wide = (room_w - 2. * PAD - 2. + CAP_GAP) / WIDTH;
        // A short window shrinks the keys too, so the list below keeps its eight rows.
        let tall =
            (room_h - chrome(true) - LIST_ROWS * KEY_ROW - ROW_GAP + CAP_GAP) / f32::from(ROWS);
        let unit = wide.min(tall).min(MAX_UNIT).floor();
        (unit >= MIN_UNIT).then_some(unit)
    }

    /// A keyboard is known but the window is too small to draw it, so the sheet only asks for a larger one.
    pub(super) fn too_small(&self, window: &Window) -> bool {
        self.sheet.keyboard.is_some() && self.unit(window).is_none()
    }

    /// The card around a keyboard: as wide as the keyboard, as tall as the window lets the list grow, centered; or the
    /// small card that asks for a larger window.
    pub(super) fn keyboard_card(&self, window: &Window) -> Option<(f32, f32, f32, f32)> {
        let size = window.viewport_size();
        let (width, height) = (f32::from(size.width), f32::from(size.height));
        let (card_w, card_h) = match self.unit(window) {
            Some(unit) => (
                (keyboard_width(unit) + 2. * PAD + 2.).ceil(),
                (height - 2. * CLAMP).min(SHEET_H + keyboard_height(unit) + STACK_GAP),
            ),
            None if self.too_small(window) => (
                (width - 2. * CLAMP).min(SMALL_W),
                (height - 2. * CLAMP).min(SMALL_H),
            ),
            None => return None,
        };
        Some((
            ((width - card_w) / 2.).floor(),
            ((height - card_h) / 2.).floor(),
            card_w,
            card_h,
        ))
    }

    /// The card this window draws: around a keyboard, asking for a larger window, or the sheet's own without one.
    pub(super) fn card(&self, window: &Window) -> (f32, f32, f32, f32) {
        self.keyboard_card(window)
            .unwrap_or_else(|| super::fit::for_sheet(&self.sheet, &self.typeface, window))
    }

    /// The keyboard's own listeners on the sheet's ground: held modifiers, and a looked-up key let go.
    pub(super) fn listen(ground: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        ground
            .on_modifiers_changed(cx.listener(Self::modifiers_changed))
            .on_key_up(cx.listener(Self::key_up))
    }

    /// The list's height under the keyboard, for the held rows each column can show.
    fn list_room(&self, window: &Window) -> Option<f32> {
        let unit = self.unit(window)?;
        let (_, _, _, card_h) = self.keyboard_card(window)?;
        Some(card_h - chrome(false) - keyboard_height(unit))
    }

    /// The search is open: Ctrl+F opened it, or the field holds text.
    pub(super) fn searching(&self) -> bool {
        self.sheet.typing || !self.sheet.query.is_empty() || !self.editor.entry.text.is_empty()
    }

    /// Where the field waits at rest for the first typed key: below the keyboard when one is drawn.
    pub(super) fn field_top(&self, window: &Window) -> f32 {
        let band = self
            .unit(window)
            .map_or(0., |unit| keyboard_height(unit) + STACK_GAP);
        PAD + TITLE_H + 1. + 2. * STACK_GAP + band
    }

    /// Leaves the search for the keyboard: the field empties and keys look themselves up again.
    fn stop_typing(&mut self) {
        self.sheet.typing = false;
        // An input method's uncommitted text goes with it, so the field is empty and the keys look up again.
        self.editor.clear();
        self.sheet.query_changed(String::new());
        self.sync_editor();
    }

    /// The held modifiers changed: the keyboard and the held list follow them, except while the search types.
    pub(super) fn modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut held = mask(event.modifiers);
        // While searching, Shift alone is a capital letter, not a layer to show.
        if held == flea::keys::SHIFT && self.searching() {
            held = 0;
        }
        if self.sheet.keyboard.is_some() && held != self.sheet.held {
            self.sheet.hold(held);
            cx.notify();
        }
    }

    /// Over a keyboard every key looks itself up while it is held, never runs and never types; Ctrl+F opens the search,
    /// and Escape or Ctrl+F again returns to the keyboard. While searching, keys type and edit as they do without a
    /// keyboard, and a Ctrl, Alt or Super chord off the field's own editing keys is still looked up.
    pub(in crate::g2::st1) fn chord_down(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.sheet.keyboard.is_none() {
            return false;
        }
        let keystroke = &event.keystroke;
        let key = keystroke.key.as_str();
        // Too small for the keyboard the sheet shows only its hint: Escape closes it and every other key waits.
        if self.unit(window).is_none() {
            return key != "escape";
        }
        // A held key's own repeats stay with it; any other key ends its look-up.
        if let Some(pressed) = self.sheet.pressed.clone() {
            if pressed == key {
                return true;
            }
            self.sheet.release(&pressed);
            cx.notify();
        }
        // The window's state keeps the Shift a symbol key drops; the event's carries a press the window has not seen yet.
        let held = mask(window.modifiers()) | mask(keystroke.modifiers);
        let chorded = held & (CONTROL | ALT | SUPER) != 0;
        let ctrl_alone = held == CONTROL;
        let searching = self.searching();
        if ctrl_alone && key == "f" {
            // Holding Ctrl+F opens or closes the search once, not again on every repeat.
            if event.is_held {
                return true;
            }
            if searching {
                self.stop_typing();
            } else {
                // The search opens on every binding, narrowed as words are typed.
                self.sheet.typing = true;
                self.sheet.chord = None;
                self.sheet.query_changed(String::new());
            }
            cx.notify();
            return true;
        }
        if key == "escape" {
            // A held Escape leaves the search once; its repeats don't go on to close the sheet.
            if event.is_held {
                return true;
            }
            if !searching {
                return false;
            }
            self.stop_typing();
            cx.notify();
            return true;
        }
        let editing = FIELD_KEYS.contains(&key) || (ctrl_alone && FIELD_CTRL_KEYS.contains(&key));
        if searching && (!chorded || editing) {
            return false;
        }
        // A key the keyboard does not draw does nothing here: it neither types nor runs.
        if self.sheet.look_up(key, keystroke.us_key.as_deref(), held) && held == 0 {
            self.sheet.pressed = Some(key.into());
        }
        cx.notify();
        true
    }

    /// Letting go of the key that looked itself up returns the sheet to what the held modifiers show.
    pub(in crate::g2::st1) fn key_up(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sheet.release(&event.keystroke.key) {
            cx.notify();
        }
    }
}
