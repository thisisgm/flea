//! The chooser's own bands, title, answers and status, drawn to the picker boards; the body belongs to the listing owner.
use super::filename::FieldKey;
use super::picker::Picker;
use crate::list::Palette;
use flea::ui_logic_pk1 as logic;
use gpui::{
    div, prelude::*, px, ClickEvent, Context, Div, KeyDownEvent, MouseButton, SharedString,
    Stateful, Window,
};

// The picker boards' sizes (picker-open, picker-save): title band, answer band, status bar, DialogButton.
const TITLE_HEIGHT: f32 = 34.;
const ANSWERS_HEIGHT: f32 = 59.;
const STATUS_HEIGHT: f32 = 27.;
const BUTTON_HEIGHT: f32 = 30.;
const SIDE: f32 = 14.;
const GAP: f32 = 9.;
const BUTTON_SIDE: f32 = 9.;
const TEXT: f32 = 14.;
const STATUS_TEXT: f32 = 12.;
// The bands' rules: foreground at 24%; a refused button draws at 55%; the default carries a 14% accent wash.
const RULE: f32 = 0.24;
const REFUSED: f32 = 0.55;
const WASH: f32 = 0.14;

// DialogButton's recipe: every button framed, the default an accent frame on the wash; a refused default loses both.
fn answer(
    id: &'static str,
    label: SharedString,
    default: bool,
    available: bool,
    palette: &Palette,
) -> Stateful<Div> {
    let lit = default && available;
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .h(px(BUTTON_HEIGHT))
        .px(px(BUTTON_SIDE))
        .text_size(px(TEXT))
        .text_color(palette.foreground)
        .border_1()
        .border_color(if lit {
            palette.accent_frame
        } else {
            palette.metadata_frame
        })
        .when(lit, |button| button.bg(palette.accent.opacity(WASH)))
        .when(!available, |button| button.opacity(REFUSED))
        .child(label)
}

impl Picker {
    // The labels the answer row draws now: Replace stands in for the answer while the asked review stands.
    pub fn answer_label(&self) -> SharedString {
        if self.accept_state().collision_review() {
            self.labels.replace.clone()
        } else {
            self.labels
                .accept(&self.locale.catalog, &self.req, self.marks.len())
        }
    }

    // Only a plain answer carries the default's accent frame and wash.
    pub fn answer_is_default(&self) -> bool {
        !self.accept_state().collision_review()
    }

    pub fn hints(&self) -> SharedString {
        if self.backend_unavailable() {
            self.labels.hints_unavailable.clone()
        } else {
            self.labels.hints.clone()
        }
    }

    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.browse.window = Some(window.window_handle());
        // The shared path entry answers its own keys; what it lets through is typed into it, never into the chooser.
        let path = self.browse.path_view(cx);
        if path.read(cx).holds_keyboard(window) {
            return;
        }
        let key = event.keystroke.key.as_str();
        // The save field takes the keyboard from the list, and Escape still refuses from there.
        if key == "escape" {
            cx.stop_propagation();
            if self.escape_listing(cx) == crate::g2::pk2::Escape::Cancel {
                self.cancel(cx);
            }
            return;
        }
        if let Some(filename) = self.browse.filename.clone() {
            if filename.read(cx).focus.is_focused(window) {
                cx.stop_propagation();
                match filename.update(cx, |field, cx| field.key(event, cx)) {
                    FieldKey::Edited(text) => self.save_name_edited(text, cx),
                    FieldKey::Enter => self.accept(cx),
                    FieldKey::Tab => window.focus(&self.focus, cx),
                    FieldKey::Moved | FieldKey::Ignored => {}
                }
                return;
            }
        }
        let held = event.keystroke.modifiers;
        let cursor = self.listing.cursor;
        match key {
            "up" => self.move_cursor(cursor.saturating_sub(1), cx),
            "down" => self.move_cursor(cursor + 1, cx),
            "home" => self.move_cursor(0, cx),
            "end" => self.move_cursor(self.listing.total.saturating_sub(1), cx),
            "enter" => self.activate_row(cursor, cx),
            "tab" if self.browse.filename.is_some() => {
                self.browse.focus_filename = true;
                cx.notify();
            }
            // keys.toml's pathBar rows, ":" and Ctrl+L, open the shared entry on the drawn folder.
            "l" if held.control && !held.alt && !held.shift => {
                path.update(cx, |view, cx| view.open(window, cx))
            }
            _ if event.keystroke.key_char.as_deref() == Some(":") => {
                path.update(cx, |view, cx| view.open(window, cx))
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    // Focus and stem selection are applied with the window: a name that arrived, a review that asked, the default field.
    fn place_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browse.window = Some(window.window_handle());
        let Some(filename) = self.browse.filename.clone() else {
            return;
        };
        let review = self.stem_requests != self.browse.stems_seen;
        if !self.browse.focus_filename && !review {
            return;
        }
        self.browse.focus_filename = false;
        self.browse.stems_seen = self.stem_requests;
        let focus = filename.update(cx, |field, cx| {
            if review {
                field.select_stem();
                cx.notify();
            }
            field.focus.clone()
        });
        window.focus(&focus, cx);
    }
}

impl Render for Picker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.place_focus(window, cx);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let reviewing = self.accept_state().collision_review();
        let available = if reviewing {
            logic::replace_allowed(&self.accept_state())
        } else {
            self.can_accept()
        };
        let message = self.message.clone();
        let rule = palette.foreground.opacity(RULE);
        // Only a portal identity follows the title, in caption size after a muted dot, the way PickerChrome.qml draws it.
        let requester = self.labels.requester.clone().map(|requester| {
            div()
                .flex()
                .items_baseline()
                .min_w_0()
                .text_size(px(STATUS_TEXT))
                .child(
                    div()
                        .flex_none()
                        .text_color(palette.metadata)
                        .child("\u{a0}\u{b7}\u{a0}"),
                )
                .child(div().min_w_0().text_ellipsis().child(requester))
        });
        let title = div()
            .flex_none()
            .flex()
            .items_center()
            .h(px(TITLE_HEIGHT))
            .px(px(SIDE))
            .border_b_1()
            .border_color(rule)
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .min_w_0()
                    .whitespace_nowrap()
                    .text_size(px(TEXT))
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .child(self.labels.title.clone()),
                    )
                    .children(requester),
            );
        let answers = div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap(px(GAP))
            .h(px(ANSWERS_HEIGHT))
            .px(px(SIDE))
            .border_t_1()
            .border_color(rule)
            .child(div().flex_1().children(self.filter.clone()))
            .child(
                answer(
                    "picker-cancel",
                    self.labels.cancel.clone(),
                    false,
                    true,
                    &palette,
                )
                .on_click(cx.listener(|picker, _: &ClickEvent, _, cx| picker.cancel(cx))),
            )
            .child(
                answer(
                    "picker-answer",
                    self.answer_label(),
                    self.answer_is_default(),
                    available,
                    &palette,
                )
                // A refused answer is drawn and its press does nothing, as DialogButton.qml says.
                .on_click(cx.listener(
                    move |picker, _: &ClickEvent, _, cx| match (available, reviewing) {
                        (false, _) => {}
                        (true, true) => picker.replace(cx),
                        (true, false) => picker.accept(cx),
                    },
                )),
            );
        let footer = div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .h(px(STATUS_HEIGHT))
            .px(px(SIDE))
            .text_size(px(STATUS_TEXT))
            .text_color(palette.foreground)
            .border_t_1()
            .border_color(rule)
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .children(message.map(|message| message.text)),
            )
            .child(div().flex_none().whitespace_nowrap().child(self.hints()));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.background)
            .text_color(palette.foreground)
            .track_focus(&self.focus)
            .key_context("Picker")
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|picker, _, window, cx| window.focus(&picker.focus, cx)),
            )
            .child(title)
            .child(div().flex_1().overflow_hidden().children(self.body.clone()))
            .child(answers)
            .child(footer)
    }
}
