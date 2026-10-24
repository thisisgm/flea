//! Save As: the name field in the answer row, its stem selected whenever a name arrives or a review asks.
use crate::g2::pk2::view::SaveView;
use crate::list::Palette;
use crate::path::entry::Entry;
use flea::ui_logic_pk2::stem_end;
use gpui::{
    div, prelude::*, px, ClickEvent, Context, Entity, FocusHandle, IntoElement, KeyDownEvent,
    MouseButton, Render, SharedString, Window,
};

// picker-save board: the label 9 px before the field, a 30 px field on the button row, text 9 px in.
const GAP: f32 = 9.;
const HEIGHT: f32 = 30.;
const INSET: f32 = 9.;
const TEXT: f32 = 14.;
const LABEL: f32 = 12.;
// The selected run sits on a 24% accent wash with foreground text.
const WASH: f32 = 0.24;

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
}

impl Filename {
    pub fn new(seed: &str, label: String, save: Entity<SaveView>, cx: &mut Context<Self>) -> Self {
        let mut field = Self {
            entry: Entry::default(),
            focus: cx.focus_handle(),
            save,
            label: label.to_uppercase().into(),
        };
        field.entry.text = seed.to_owned();
        field.select_stem();
        field
    }

    pub fn text(&self) -> &str {
        &self.entry.text
    }

    // A name taken from a row replaces the field and opens with its stem selected.
    pub fn set(&mut self, name: &str, cx: &mut Context<Self>) {
        self.entry.text = name.to_owned();
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
            "a" if held.control && !held.alt && !held.shift => {
                self.entry.select_all();
                FieldKey::Moved
            }
            _ => match event.keystroke.key_char.as_deref() {
                Some(typed) if !held.control && !held.platform && !held.alt => {
                    self.entry.replace(None, typed);
                    FieldKey::Edited(self.entry.text.clone())
                }
                _ => FieldKey::Ignored,
            },
        };
        if let FieldKey::Edited(text) = &answer {
            let text = text.clone();
            self.save.update(cx, |save, _| save.field().edit(&text));
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
        let text = &self.entry.text;
        let selected = self.entry.selected.clone();
        let (before, run, after) = (
            text[..selected.start].to_owned(),
            text[selected.clone()].to_owned(),
            text[selected.end..].to_owned(),
        );
        let field = div()
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
            .child(before)
            .child(div().bg(palette.accent.opacity(WASH)).child(run))
            .child(after);
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
