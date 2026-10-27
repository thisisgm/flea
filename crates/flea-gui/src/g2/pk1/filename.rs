//! Save As: the name field in the answer row, its stem selected whenever a name arrives or a review asks.
use super::text::{Drawn, EntryHost, Input};
use crate::g2::pk2::view::SaveView;
use crate::list::Palette;
use crate::path::entry::Entry;
use flea::ui_logic_pk2::stem_end;
use gpui::{
    div, prelude::*, px, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, IntoElement,
    KeyDownEvent, MouseButton, Render, SharedString, Window,
};
use std::cell::RefCell;

// picker-save board: the label 9 px before the field, a 30 px field on the button row, text 9 px in.
const GAP: f32 = 9.;
const HEIGHT: f32 = 30.;
const INSET: f32 = 9.;
const TEXT: f32 = 14.;
const LABEL: f32 = 12.;

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
    pub save: Entity<SaveView>,
    label: SharedString,
    drawn: RefCell<Drawn>,
    pub(super) probe: crate::g2::st1::probe::Probe,
    pub(super) focused: bool,
}

impl Filename {
    pub fn new(seed: &str, label: String, save: Entity<SaveView>, cx: &mut Context<Self>) -> Self {
        let mut field = Self {
            entry: Entry::default(),
            drawn: RefCell::default(),
            focus: cx.focus_handle(),
            save,
            label: label.to_uppercase().into(),
            probe: Default::default(),
            focused: false,
        };
        field.entry.begin(seed);
        field.select_stem();
        field
    }

    pub fn text(&self) -> &str {
        &self.entry.text
    }

    // A name taken from a row replaces the field and opens with its stem selected.
    pub fn set(&mut self, name: &str, cx: &mut Context<Self>) {
        self.entry.begin(name);
        self.select_stem();
        self.save.update(cx, |save, _| save.field().edit(name));
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
        let field = self
            .probe
            .attach(div().relative(), "field")
            .id("picker-filename")
            .debug_selector(|| "pk1-filename".into())
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .h(px(HEIGHT))
            .px(px(INSET))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(TEXT))
            .text_color(palette.foreground)
            .border_1()
            .border_color(if focused {
                palette.accent_frame
            } else {
                palette.metadata_frame
            })
            .track_focus(&self.focus)
            .on_click(
                cx.listener(|field, _: &ClickEvent, window, cx| window.focus(&field.focus, cx)),
            )
            .child(Input::new(cx.entity(), face));
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(GAP))
            .min_w_0()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .text_size(px(LABEL))
                    .text_color(palette.metadata)
                    .child(self.label.clone()),
            )
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
        self.save.update(cx, |save, _| save.field().edit(&text));
        cx.emit(FieldKey::Edited(text));
        cx.notify();
    }
}
