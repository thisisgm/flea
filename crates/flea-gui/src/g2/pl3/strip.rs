//! The Trash strip, the column header and the empty-state line, laid out by the boards' numbers.
use super::board::{
    Glyph, BODY, BUTTON_H, BUTTON_PAD, CAPTION, CBTN_GLYPH, CBTN_W, DELETED_W, DISABLED, GAP, HEADER_H, HOVER, LOCATION_W,
    PAD_X, PRESS, ROW_H, SLOT, STRIP_H,
};
use super::glyph::GlyphBox;
use super::list::TrashList;
use super::pane::TrashPane;
use flea::ui_logic_pl3::words;
use gpui::{
    div, prelude::*, px, AnyElement, Context, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    ScrollWheelEvent, SharedString, Window,
};

/// Empty Trash…'s look in one state: frame, wash, label ink and opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonLook {
    pub frame: Hsla,
    pub wash: Option<Hsla>,
    pub label: Hsla,
    pub opacity: f32,
}

impl TrashPane {
    /// trash, trash-strip-hover, trash-strip-focus and trash-emptying: the label stays foreground in every state.
    pub fn button_look(&self) -> ButtonLook {
        let ink = self.paint.borrow().ink;
        let available = self.empty_available();
        let focused = self.host.view.strip_focus && available;
        let wash = match (available, self.button) {
            (true, b) if b.pressed && b.hover => Some(ink.foreground.opacity(PRESS)),
            (true, b) if b.hover && !b.pressed => Some(ink.foreground.opacity(HOVER)),
            _ => None,
        };
        ButtonLook {
            frame: if focused { ink.foreground } else { ink.muted_frame },
            wash,
            label: ink.foreground,
            opacity: if available { 1. } else { DISABLED },
        }
    }

    fn chrome_button(&self, glyph: Glyph, enabled: bool, id: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let ink = self.paint.borrow().ink;
        let mark = GlyphBox { glyph, side: CBTN_GLYPH, ink: if enabled { ink.foreground } else { ink.muted_frame } };
        let button = div().id(id).debug_selector(|| id.into()).w(px(CBTN_W)).h(px(STRIP_H)).flex_none().flex().items_center().justify_center()
            .child(mark).opacity(if enabled { 1. } else { DISABLED });
        if enabled {
            button.on_click(cx.listener(|pane, _, _, cx| pane.back_pressed(cx))).into_any_element()
        } else {
            button.into_any_element()
        }
    }

    fn strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let ink = self.paint.borrow().ink;
        let catalog = &self.catalog;
        let view = &self.host.view;
        let look = self.button_look();
        let count: SharedString = words::strip_count(catalog, view.total, view.bytes).into();
        let label: SharedString = catalog.tr("trash.empty_action").into();
        let mut button = div().id("trash-empty").debug_selector(|| "trash-empty".into()).h(px(BUTTON_H)).px(px(BUTTON_PAD)).flex().items_center().flex_none()
            .text_size(px(CAPTION)).text_color(look.label).border_1().border_color(look.frame)
            .rounded(ink.small_radius()).opacity(look.opacity).child(label)
            .on_mouse_up_out(MouseButton::Left, cx.listener(|pane, _: &MouseUpEvent, _, cx| {
                pane.button.pressed = false;
                cx.notify();
            }))
            .on_hover(cx.listener(|pane, hovered: &bool, _, cx| {
                pane.button.hover = *hovered;
                cx.notify();
            }));
        if let Some(wash) = look.wash {
            button = button.bg(wash);
        }
        if self.empty_available() {
            button = button
                .on_mouse_down(MouseButton::Left, cx.listener(|pane, _: &MouseDownEvent, _, cx| {
                    pane.button.pressed = true;
                    cx.notify();
                }))
                .on_click(cx.listener(|pane, _, _, cx| {
                    pane.button.pressed = false;
                    pane.empty_pressed(cx);
                }));
        }
        div().h(px(STRIP_H)).flex_none().flex().items_center().px(px(PAD_X)).gap(px(GAP)).bg(ink.surface)
            .border_b_1().border_color(ink.hair()).text_size(px(CAPTION)).text_color(ink.foreground)
            .child(self.chrome_button(Glyph::Back, true, "trash-back", cx))
            .child(self.chrome_button(Glyph::Up, false, "trash-up", cx))
            .child(div().flex_none().child(SharedString::from(catalog.tr("trash.title"))))
            .child(div().flex_1().min_w(px(0.)).overflow_hidden().child(count))
            .child(button)
            .into_any_element()
    }

    fn header(&self) -> AnyElement {
        let ink = self.paint.borrow().ink;
        let tr = |key: &str| SharedString::from(self.catalog.tr(key));
        let deleted = SharedString::from(crate::navigation::sort::title(&self.catalog.tr("trash.header.deleted"), true));
        div().h(px(HEADER_H)).flex_none().flex().items_center().gap(px(GAP)).px(px(PAD_X)).text_size(px(CAPTION))
            .text_color(ink.foreground).border_b_1().border_color(ink.hair())
            .child(div().w(px(SLOT)).flex_none())
            .child(div().flex_1().min_w(px(0.)).overflow_hidden().child(tr("trash.header.name")))
            .child(div().w(px(LOCATION_W)).flex_none().flex().justify_end().child(tr("trash.header.location")))
            .child(div().w(px(DELETED_W)).flex_none().flex().justify_end().child(deleted))
            .into_any_element()
    }

    /// The header's bare labels for the IPC describe, before the sort mark.
    pub fn header_labels(&self) -> [String; 3] {
        ["trash.header.name", "trash.header.location", "trash.header.deleted"].map(|key| self.catalog.tr(key))
    }

    /// TrashView.qml's centred line while nothing lists: an error, Reading Trash…, or Trash is empty.
    pub fn empty_line(&self) -> Option<(String, bool)> {
        let view = &self.host.view;
        if view.total > 0 || !view.opened {
            return None;
        }
        Some(match &view.error_text {
            Some(text) => (text.clone(), true),
            None if view.busy() => (self.catalog.tr("trash.reading"), false),
            None => (self.catalog.tr("trash.is_empty"), false),
        })
    }

    fn row_at(&self, y: Pixels) -> Option<usize> {
        let at = ((f32::from(y) + self.host.view.scroll_y) / ROW_H).floor();
        (at >= 0. && (at as usize) < self.host.view.total).then_some(at as usize)
    }
}

impl Render for TrashPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.evict(window, cx);
        let ink = self.paint.borrow().ink;
        let empty = self.empty_line().map(|(text, error)| {
            div().absolute().size_full().flex().items_center().justify_center().text_size(px(BODY))
                .text_color(if error { ink.error } else { ink.muted }).child(SharedString::from(text))
        });
        let top = px(STRIP_H + HEADER_H);
        let list = div().relative().flex_1().min_h(px(0.)).child(TrashList(self.paint.clone()))
            .on_mouse_move(cx.listener(move |pane, event: &MouseMoveEvent, _, cx| {
                let index = pane.row_at(event.position.y - top);
                pane.hover(index, cx);
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |pane, event: &MouseDownEvent, _, cx| {
                if let Some(index) = pane.row_at(event.position.y - top) {
                    pane.click_row(index, event.modifiers.control, cx);
                }
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |pane, event: &MouseDownEvent, _, cx| {
                let index = pane.row_at(event.position.y - top);
                pane.right_click(index, f32::from(event.position.x), f32::from(event.position.y), cx);
            }))
            .on_scroll_wheel(cx.listener(|pane, event: &ScrollWheelEvent, _, cx| {
                pane.wheel(-f32::from(event.delta.pixel_delta(px(ROW_H)).y), cx);
            }))
            .children(empty);
        div().size_full().flex().flex_col().bg(ink.background).font_family(self.paint.borrow().font.family.clone())
            .track_focus(&self.focus).child(self.strip(cx)).child(self.header()).child(list)
    }
}
