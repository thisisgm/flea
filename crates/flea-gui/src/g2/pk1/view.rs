//! The chooser's own bands, title, answers and status, drawn to the picker boards; the body belongs to the listing owner.
use super::drawn::{Painted, Slot};
use super::filename::FieldKey;
use super::picker::Picker;
use super::size::Size;
use crate::list::Palette;
use flea::ui_logic_pk1 as logic;
use gpui::{
    div, prelude::*, px, ClickEvent, Context, Div, KeyDownEvent, MouseButton, SharedString,
    Stateful, Window,
};

// The picker boards' sizes (picker-open, picker-save): title band, answer band, status bar, DialogButton.
const TITLE_HEIGHT: f32 = 34.;
const ANSWERS_HEIGHT: f32 = 59.;
pub(super) const STATUS_HEIGHT: f32 = 27.;
pub(super) const BUTTON_HEIGHT: f32 = 30.;
pub(super) const SIDE: f32 = 14.;
pub(super) const GAP: f32 = 9.;
pub(super) const BUTTON_SIDE: f32 = 9.;
// The bands' rules: foreground at 24%; a refused button draws its muted ink at 55%, as DialogButton.qml does; the default carries a 14% accent wash.
const RULE: f32 = 0.24;
const REFUSED: f32 = 0.55;
const WASH: f32 = 0.14;

// DialogButton's recipe: every button framed, the default an accent frame on the wash; a refused default loses both.
fn answer(
    id: &'static str,
    label: SharedString,
    default: bool,
    available: bool,
    (palette, size): (&Palette, Size),
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
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .when(!available, |button| {
            button.a11y_synthetic_children(|builder| builder.parent_node().set_disabled())
        })
        .debug_selector(move || id.into())
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
        .h(size.px(BUTTON_HEIGHT))
        .px(size.px(BUTTON_SIDE))
        .text_size(px(size.body))
        .text_color(if available {
            palette.foreground
        } else {
            palette.muted
        })
        .border_1()
        .border_color(if lit {
            palette.accent_frame
        } else {
            crate::presentation::frames::Frames::from_palette(palette, 0.).ink
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
        // New Folder's card holds the keyboard while it is open (0401-picker-new-folder).
        if self.chrome.folder.is_some() {
            self.folder_key(event, window, cx);
            return;
        }
        // The shared path entry answers its own keys; what it lets through is typed into it, never into the chooser.
        let path = self.browse.path_view(cx);
        if path.read(cx).holds_keyboard(window) {
            return;
        }
        if event.keystroke.key == "enter" {
            self.record_submission_focus(window, cx);
        }
        if self.rail_key(event, window, cx) || self.parity_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        let key = event.keystroke.key.as_str();
        if let Some(filename) = self.browse.filename.clone() {
            if filename.read(cx).focus.is_focused(window) {
                let result = filename.update(cx, |field, cx| field.key(event, cx));
                match &result {
                    FieldKey::Enter => self.accept(cx),
                    FieldKey::Tab => window.focus(&self.focus, cx),
                    FieldKey::Edited(_) | FieldKey::Moved | FieldKey::Ignored => {}
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
}

impl Render for Picker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The picker window's root, so the whole frame paints in the locale's direction.
        let direction =
            crate::locale_geometry::Direction::from_locale(self.locale.catalog.locale());
        window.set_layout_mirrored(direction.is_rtl());
        let face = crate::list::Typeface::for_window(window, cx);
        let size = Size::of(window, cx);
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
            sizes: (size.body, size.body_small),
        });
        let rule = palette.foreground.opacity(RULE);
        // Only a portal identity follows the title, in caption size after a muted dot, the way PickerChrome.qml draws it.
        let requester = self.labels.requester.clone().map(|requester| {
            div()
                .flex()
                .items_baseline()
                .min_w_0()
                .text_size(px(size.caption))
                .child(
                    div()
                        .flex_none()
                        .text_color(palette.metadata)
                        .child("\u{a0}\u{b7}\u{a0}"),
                )
                .child(div().min_w_0().text_ellipsis().child(requester))
        });
        // The title, answer and status bands sit on the surface, a step off the pane, as the boards and PickerChrome.qml draw them.
        let surface = theme.surface;
        let title = div()
            .debug_selector(|| "picker-title-band".into())
            .relative()
            .children(self.drawn.probe(Slot::Title, [0., 0., 1., 0.]))
            .flex_none()
            .flex()
            .items_center()
            .h(size.px(TITLE_HEIGHT))
            .px(size.px(SIDE))
            .bg(surface)
            .border_b_1()
            .border_color(rule)
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .min_w_0()
                    .whitespace_nowrap()
                    .text_size(px(size.body))
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .child(self.labels.title.clone()),
                    )
                    .children(requester),
            );
        // Where the Save As field would be cut, the caller's filter takes its own row above the answers.
        let filter_row = if self.filter_stacks(window, cx) {
            self.filter_trigger(&palette, window, cx).map(|trigger| {
                div()
                    .debug_selector(|| "picker-filter-row".into())
                    .flex_none()
                    .flex()
                    .items_center()
                    .h(size.px(ANSWERS_HEIGHT))
                    .px(size.px(SIDE))
                    .bg(surface)
                    .border_t_1()
                    .border_color(rule)
                    .child(trigger)
            })
        } else {
            None
        };
        // PickerSave.qml's status line, above Cancel and the answer, in the error ink.
        let save_line = self.save_status().map(|line| {
            div()
                .debug_selector(|| "picker-save-line".into())
                .flex_none()
                .px(size.px(SIDE))
                .pt(size.px(GAP))
                .bg(surface)
                .border_t_1()
                .border_color(rule)
                .text_size(px(size.caption))
                .text_color(palette.error_on_surface)
                .child(line)
        });
        let answers = div()
            .debug_selector(|| "picker-answers-band".into())
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap(size.px(GAP))
            .h(size.px(ANSWERS_HEIGHT))
            .px(size.px(SIDE))
            .bg(surface)
            .when(save_line.is_none(), |band| {
                band.border_t_1().border_color(rule)
            })
            .child(self.answer_leading(&palette, window, cx))
            .child(
                answer(
                    "picker-cancel",
                    self.labels.cancel.clone(),
                    false,
                    true,
                    (&palette, size),
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
                    (&palette, size),
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
        let footer = self.footer(&palette, theme, rule, window, cx);
        let folder_card = self.folder_card(window, cx);
        div()
            .relative()
            .size_full()
            .font_family(face.font.family)
            .flex()
            .flex_col()
            .bg(palette.background)
            .text_color(palette.foreground)
            .track_focus(&self.focus)
            .key_context("Picker")
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::row_pointer))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::header_release))
            .child(title)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .children(self.body.clone())
                    .children(self.drawn.probe(Slot::Body, [0.; 4])),
            )
            .children(filter_row)
            .children(save_line)
            .child(answers)
            .child(footer)
            .children(folder_card)
            .children(
                self.chrome
                    .menu_closing
                    .clone()
                    .map(|menu| div().absolute().size_full().child(menu)),
            )
            .children(self.chrome.menu_view.clone().map(|menu| {
                div()
                    .absolute()
                    .size_full()
                    .debug_selector(|| "picker-filter-menu".into())
                    .child(menu)
            }))
            .children(self.drawn.probe(Slot::Window, [0.; 4]))
            // The root holds the keyboard focus while the listing is browsed, so it is a node the listing's cursor
            // row reports through.
            .id("picker")
            .role(gpui::Role::Group)
    }
}
