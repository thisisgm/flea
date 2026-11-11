//! Save As: the name field in the answer row, its stem selected whenever a name arrives or a review asks.
use super::text::{Drawn, EntryHost, Input};
use crate::g2::picker_listing::view::SaveView;
use crate::g2::status_feedback::caption::Tracked;
use crate::list::Palette;
use crate::path::entry::Entry;
use flea::ui_logic_file_ops::card;
use flea::ui_logic_picker_listing::stem_end;
use gpui::{
    div, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle, IntoElement,
    KeyDownEvent, MouseButton, Render, SharedString, Window,
};
use std::cell::RefCell;

// picker-save board: the label 9 px before the field, a 30 px field on the button row, text 9 px in.
const GAP: f32 = 9.;
const HEIGHT: f32 = 30.;
pub(super) const WIDTH: f32 = 300.;
const INSET: f32 = 9.;
// The label's caption line, which the row centres; the field and label type are the body and caption tokens.
const LABEL_LINE: f32 = 16.;

// What one key did to the field, for the picker to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKey {
    Edited(String),
    Moved,
    Enter,
    Tab,
    Ignored,
}

pub struct Filename {
    pub entry: Entry,
    pub focus: FocusHandle,
    // The Save line's words, which only Save As carries; New Folder's card field has none.
    pub save: Option<Entity<SaveView>>,
    label: SharedString,
    // The board's caption tracking, 1.2 px, none for a caseless script.
    tracking: f32,
    drawn: RefCell<Drawn>,
    pub(super) probe: crate::g2::settings_pages::probe::Probe,
    pub(super) focused: bool,
    // 0401-picker-new-folder: the card's field, its name above it and the card's width; a refusal frames it in the error role.
    stacked: bool,
    pub(super) refused: bool,
    /// The field's Undo, Cut, Copy, Paste and Select All card while a right press holds it open.
    text_menu: crate::path::text_menu::TextMenu,
    catalog: Option<std::sync::Arc<flea::i18n::I18n>>,
}

impl Filename {
    pub fn new(
        seed: &str,
        label: String,
        tracking: f32,
        save: Entity<SaveView>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut field = Self::field(seed, label, tracking, Some(save), cx);
        field.select_stem();
        field
    }

    // New Folder's name field: the seed opens selected whole, as the window's new folder name does.
    pub fn folder_name(seed: &str, label: String, tracking: f32, cx: &mut Context<Self>) -> Self {
        let mut field = Self::field(seed, label, tracking, None, cx);
        field.stacked = true;
        field.entry.select_all();
        field
    }

    fn field(
        seed: &str,
        label: String,
        tracking: f32,
        save: Option<Entity<SaveView>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut field = Self {
            entry: Entry::default(),
            drawn: RefCell::default(),
            focus: cx.focus_handle(),
            save,
            label: label.to_uppercase().into(),
            tracking,
            probe: Default::default(),
            focused: false,
            stacked: false,
            refused: false,
            text_menu: Default::default(),
            catalog: None,
        };
        field.entry.begin(seed);
        field
    }

    /// The language the field's text menu names its rows in.
    pub fn with_catalog(mut self, catalog: std::sync::Arc<flea::i18n::I18n>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// A key for the field, which its open text menu takes first and holds as motion.
    pub fn menu_or_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FieldKey {
        if self.text_menu_key(event, window, cx) {
            return FieldKey::Moved;
        }
        self.key(event, cx)
    }

    /// The field's open text menu takes every key; true when it held this one.
    pub fn text_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.text_menu.opened() {
            return false;
        }
        let outcome = self.text_menu.key(event, window);
        self.text_menu_answer(outcome, window, cx);
        true
    }

    fn text_menu_answer(
        &mut self,
        outcome: crate::g2::context_menus::menu::Outcome,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::path::text_menu::{apply, Applied};
        if let crate::g2::context_menus::menu::Outcome::Fire { action, .. } = &outcome {
            if apply(&mut self.entry, action, cx) == Applied::Edited {
                self.changed(cx);
            }
        }
        self.text_menu.settle(&outcome);
        cx.notify();
    }

    // A right press in the field opens its text menu over the line as it stands, and the field takes the keyboard.
    fn text_menu_press(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        if let Some(catalog) = self.catalog.clone().filter(|_| self.entry.marked.is_none()) {
            self.text_menu
                .open(&self.entry, &catalog, event.position, window, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub fn text(&self) -> &str {
        &self.entry.text
    }

    // A name taken from a row replaces the field and opens with its stem selected.
    pub fn set(&mut self, name: &str, cx: &mut Context<Self>) {
        self.entry.begin(name);
        self.select_stem();
        if let Some(save) = &self.save {
            save.update(cx, |save, _| save.field().edit(name));
        }
        cx.notify();
    }

    pub fn select_stem(&mut self) {
        self.entry.selected = 0..stem_end(&self.entry.text);
        self.entry.reversed = false;
        self.entry.marked = None;
        self.entry.revision += 1;
    }

    pub fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> FieldKey {
        let key = event.keystroke.key.as_str();
        let held = event.keystroke.modifiers;
        if self.entry.ime_key(key) {
            self.changed(cx);
            return FieldKey::Moved;
        }
        if self.entry.marked.is_some() {
            return FieldKey::Ignored;
        }
        let at = self.entry.cursor();
        let answer = match key {
            "enter" => return FieldKey::Enter,
            "tab" => return FieldKey::Tab,
            "backspace" | "delete" => {
                self.entry.erase(key == "delete");
                FieldKey::Edited(self.entry.text.clone())
            }
            "left" | "right" | "home" | "end" => {
                let to = match key {
                    "left" if !held.shift && !self.entry.selected.is_empty() => {
                        self.entry.selected.start
                    }
                    "right" if !held.shift && !self.entry.selected.is_empty() => {
                        self.entry.selected.end
                    }
                    "left" => self.entry.previous(at),
                    "right" => self.entry.next(at),
                    "home" => 0,
                    _ => self.entry.text.len(),
                };
                if held.shift {
                    self.entry.select_to(to);
                } else {
                    self.entry.move_to(to);
                }
                FieldKey::Moved
            }
            "v" if held.control && !held.alt && !held.platform => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    if self.entry.ime_replace(None, &text) {
                        self.changed(cx);
                    }
                }
                return FieldKey::Moved;
            }
            "c" | "x" if held.control && !held.alt && !held.platform => {
                if let Some(text) = self.entry.copyable() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
                    if key == "x" {
                        self.entry.replace(None, "");
                        self.changed(cx);
                    }
                }
                return FieldKey::Moved;
            }
            "a" if held.control && !held.alt && !held.shift => {
                self.entry.select_all();
                FieldKey::Moved
            }
            _ => FieldKey::Ignored,
        };
        if matches!(answer, FieldKey::Edited(_)) {
            self.changed(cx);
        }
        cx.notify();
        answer
    }
}

impl Render for Filename {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let focused = self.focus.is_focused(window);
        self.focused = focused;
        self.probe.clear();
        let face = cx
            .try_global::<crate::list::Typeface>()
            .cloned()
            .unwrap_or_else(crate::list::Typeface::omarchy_default);
        let size = super::size::Size::of(window, cx);
        let stacked = self.stacked;
        let field = self
            .probe
            .attach(div().relative(), "field")
            .id(if stacked {
                "picker-folder-name"
            } else {
                "picker-filename"
            })
            .debug_selector(move || {
                if stacked {
                    "pk1-folder-name"
                } else {
                    "pk1-filename"
                }
                .into()
            })
            // picker-save's 300 px, giving way only where a narrow chooser leaves less; the card's field spans the card.
            .map(|field| {
                if stacked {
                    field.w_full().bg(palette.background)
                } else {
                    field.w(px(WIDTH))
                }
            })
            .flex_shrink_1()
            .min_w_0()
            .flex()
            .items_center()
            .h(size.px(HEIGHT))
            .px(size.px(INSET))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(size.body))
            .text_color(palette.foreground)
            .rounded(px(crate::compositor::corner(cx)))
            .border_1()
            .border_color(if self.refused {
                palette.error_on_surface
            } else if focused {
                palette.accent_frame
            } else {
                crate::presentation::frames::Frames::from_palette(&palette, 0.).ink
            })
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pointer_press))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::text_menu_press))
            .child(Input::new(cx.entity(), face.clone()))
            .children(self.text_menu.overlay(
                &cx.entity(),
                px(crate::compositor::corner(cx)),
                Self::text_menu_answer,
            ));
        if stacked {
            // DialogField: the name in muted capitals above the field, as the window's cards draw it.
            return div()
                .flex()
                .flex_col()
                .gap(size.px(card::FIELD_LABEL_GAP))
                .w_full()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(div().flex().child(Tracked {
                    text: self.label.clone(),
                    font: face.font,
                    size: px(size.caption),
                    line: size.px(card::FIELD_LABEL_LINE),
                    tracking: px(self.tracking),
                    color: palette.muted,
                }))
                .child(field);
        }
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(size.px(GAP))
            .flex_1()
            .min_w_0()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            // picker-save's caption: muted ink, letter-spaced as every board caption is.
            .child(div().flex_none().child(Tracked {
                text: self.label.clone(),
                font: face.font,
                size: px(size.caption),
                line: size.px(LABEL_LINE),
                tracking: px(self.tracking),
                color: palette.muted,
            }))
            .child(field)
    }
}

impl EventEmitter<FieldKey> for Filename {}

impl EntryHost for Filename {
    fn entry(&self) -> &Entry {
        &self.entry
    }
    fn entry_mut(&mut self) -> &mut Entry {
        &mut self.entry
    }
    fn drawn(&self) -> &RefCell<Drawn> {
        &self.drawn
    }
    fn focus(&self) -> &FocusHandle {
        &self.focus
    }
    fn active(&self, window: &Window, _: &App) -> bool {
        self.entry.editing && self.focus.is_focused(window)
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        let text = self.entry.text.clone();
        if let Some(save) = &self.save {
            save.update(cx, |save, _| save.field().edit(&text));
        }
        cx.emit(FieldKey::Edited(text));
        cx.notify();
    }
}
