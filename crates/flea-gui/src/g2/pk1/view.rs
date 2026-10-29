//! The chooser's own bands, title, answers and status, drawn to the picker boards; the body belongs to the listing owner.
use super::drawn::{Painted, Slot};
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
    drawn: &super::drawn::Drawn,
    slot: Slot,
) -> Stateful<Div> {
    let lit = default && available;
    let hover = drawn.clone();
    let down = drawn.clone();
    let up = drawn.clone();
    let outside = drawn.clone();
    div()
        .id(id)
        .relative()
        .children(drawn.probe(slot, [1.; 4]))
        .on_hover(move |hovered, _, _| hover.pointer(slot, Some(*hovered), None))
        .on_mouse_down(MouseButton::Left, move |_, _, _| {
            down.pointer(slot, None, Some(true))
        })
        .on_mouse_up(MouseButton::Left, move |_, _, _| {
            up.pointer(slot, None, Some(false))
        })
        .on_mouse_up_out(MouseButton::Left, move |_, _, _| {
            outside.pointer(slot, None, Some(false))
        })
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
        } else if self.find.holds_query() {
            // While a query stands, esc clears it, so the hint says so (picker-filter board).
            self.labels.hints_filtered.clone()
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
        if self.answered() {
            return;
        }
        self.browse.window = Some(window.window_handle());
        // The shared path entry answers its own keys; what it lets through is typed into it, never into the chooser.
        let path = self.browse.path_view(cx);
        if path.read(cx).holds_keyboard(window) {
            return;
        }
        if event.keystroke.key == "enter" {
            self.record_submission_focus(window, cx);
        }
        if self.parity_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        let key = event.keystroke.key.as_str();
        if let Some(filename) = self.browse.filename.clone() {
            if filename.read(cx).focus.is_focused(window) {
                let result = filename.update(cx, |field, cx| field.key(event, cx));
                match &result {
                    FieldKey::Edited(text) => self.save_name_edited(text.clone(), cx),
                    FieldKey::Enter => self.accept(cx),
                    FieldKey::Tab => window.focus(&self.focus, cx),
                    FieldKey::Moved | FieldKey::Ignored => {}
                }
                if result != FieldKey::Ignored {
                    cx.stop_propagation();
                    return;
                }
                if key != "escape" || filename.read(cx).entry.marked.is_some() {
                    return;
                }
            }
        }
        let typing = self.find.owner.typing();
        if typing {
            if self.find_key(event, window, cx) {
                cx.stop_propagation();
                return;
            }
            if self.find.owner.typing() {
                return;
            }
        }
        if key == "escape" {
            cx.stop_propagation();
            if self.find_escape(cx) {
                return;
            }
            if self.escape_listing(cx) == crate::g2::pk2::Escape::Cancel {
                self.cancel(cx);
            }
            return;
        }
        if self.find_key(event, window, cx) {
            cx.stop_propagation();
            return;
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
        self.submission_focus(window, cx);
        self.read_control_focus(window);
        self.chrome.probe.clear();
        let focused = self.focus.is_focused(window) || self.find.owner.typing();
        self.browse
            .listing
            .update(cx, |listing, _| listing.set_focused(focused));
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        self.browse
            .listing
            .update(cx, |listing, _| listing.set_selection_fill(palette.marked));
        let reviewing = self.accept_state().collision_review();
        let available = if reviewing {
            logic::replace_allowed(&self.accept_state())
        } else {
            self.can_accept()
        };
        self.drawn.render(Painted {
            cancel: self.labels.cancel.clone(),
            answer: self.answer_label(),
            answer_enabled: available,
            foreground: palette.foreground,
            themed: cx
                .try_global::<crate::palette::ParsedTheme>()
                .is_some_and(|loaded| loaded.0),
            sizes: (TEXT, STATUS_TEXT + 1.),
        });
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
            .relative()
            .children(self.drawn.probe(Slot::Title, [0., 0., 1., 0.]))
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
            .child(self.answer_leading(&palette, window, cx))
            .child(
                answer(
                    "picker-cancel",
                    self.labels.cancel.clone(),
                    false,
                    true,
                    &palette,
                    &self.drawn,
                    Slot::Cancel,
                )
                .rounded(px(crate::compositor::corner(cx)))
                .track_focus(&self.chrome.cancel)
                .when(self.chrome.cancel.is_focused(window), |button| {
                    button.border_color(palette.foreground)
                })
                .on_click(cx.listener(|picker, _: &ClickEvent, _, cx| picker.cancel(cx))),
            )
            .child(
                answer(
                    "picker-answer",
                    self.answer_label(),
                    self.answer_is_default(),
                    available,
                    &palette,
                    &self.drawn,
                    Slot::Answer,
                )
                .rounded(px(crate::compositor::corner(cx)))
                .track_focus(&self.chrome.answer)
                .when(self.chrome.answer.is_focused(window), |button| {
                    button.border_color(palette.foreground)
                })
                // A refused answer is drawn and its press does nothing, as DialogButton.qml says.
                .on_click(cx.listener(
                    move |picker, _: &ClickEvent, window, cx| {
                        picker.record_submission_focus(window, cx);
                        match (available, reviewing) {
                            (false, _) => {}
                            (true, true) => picker.replace(cx),
                            (true, false) => picker.accept(cx),
                        }
                    },
                )),
            );
        let footer = div()
            .relative()
            .children(self.drawn.probe(Slot::Footer, [1., 0., 0., 0.]))
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
                    .children(message.as_ref().map(|message| message.text.clone()))
                    .when(message.is_none(), |view| {
                        view.child(
                            div()
                                .debug_selector(|| "picker-selection-status".into())
                                .child(self.selection_status()),
                        )
                    }),
            )
            .child(div().flex_none().whitespace_nowrap().child(self.hints()));
        div()
            .relative()
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
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .children(self.body.clone())
                    .children(self.drawn.probe(Slot::Body, [0.; 4])),
            )
            .child(answers)
            .child(footer)
            .children(self.chrome.menu_view.clone().map(|menu| {
                div()
                    .absolute()
                    .size_full()
                    .debug_selector(|| "picker-filter-menu".into())
                    .child(menu)
            }))
            .children(self.drawn.probe(Slot::Window, [0.; 4]))
    }
}
