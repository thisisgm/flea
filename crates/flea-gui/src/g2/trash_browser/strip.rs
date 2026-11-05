//! The Trash strip, the column header and the empty-state line, laid out by the boards' numbers.
use super::board::{Glyph, DISABLED, HOVER, PRESS};
use super::controls::track;
use super::glyph::GlyphBox;
use super::list::TrashList;
use super::pane::TrashPane;
use flea::ui_logic_trash_browser::words;
use gpui::{
    div, prelude::*, px, AnyElement, Context, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, ScrollWheelEvent, SharedString, Window,
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
    /// Empty Trash… holds Trash's keys only while it is live; strip_focus alone can outlast the button's availability.
    pub fn on_empty(&self) -> bool {
        self.host.view.strip_focus && self.empty_available()
    }

    /// Empty Trash… draws its focus ring only while it is live and the window's keyboard is on it.
    pub fn empty_focused(&self) -> bool {
        self.on_empty() && !self.keys_away
    }

    /// trash, trash-strip-hover, trash-strip-focus and trash-emptying: the label stays foreground in every state.
    pub fn button_look(&self) -> ButtonLook {
        let ink = self.paint.borrow().ink;
        let available = self.empty_available();
        let focused = self.empty_focused();
        let wash = match (available, self.button) {
            (true, b) if b.pressed && b.hover => Some(ink.foreground.opacity(PRESS)),
            (true, b) if b.hover && !b.pressed => Some(ink.foreground.opacity(HOVER)),
            _ => None,
        };
        ButtonLook {
            frame: if focused {
                ink.foreground
            } else {
                ink.muted_frame
            },
            wash,
            label: ink.foreground,
            opacity: if available { 1. } else { DISABLED },
        }
    }

    fn chrome_button(
        &self,
        glyph: Glyph,
        enabled: bool,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ink = self.paint.borrow().ink;
        let board = self.board();
        let mark = GlyphBox {
            glyph,
            side: board.cbtn_glyph,
            ink: if enabled {
                ink.foreground
            } else {
                ink.muted_frame
            },
        };
        let button = div()
            .id(id)
            .debug_selector(|| id.into())
            .w(px(board.cbtn_w))
            .h(px(board.strip_h))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(mark)
            .child(track(&self.controls, id))
            .opacity(if enabled { 1. } else { DISABLED });
        if enabled {
            button
                .on_click(cx.listener(|pane, _, _, cx| pane.back_pressed(cx)))
                .into_any_element()
        } else {
            button.into_any_element()
        }
    }

    fn strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let ink = self.paint.borrow().ink;
        let catalog = &self.catalog;
        let look = self.button_look();
        let board = self.board();
        let paint = self.paint.borrow();
        let count: SharedString = words::strip_count(catalog, paint.total, paint.bytes).into();
        let label: SharedString = catalog.tr("trash.empty_action").into();
        let mut button = div()
            .id("trash-empty")
            .debug_selector(|| "trash-empty".into())
            .h(px(board.button_h))
            .px(px(board.button_pad))
            .flex()
            .items_center()
            .flex_none()
            .text_size(board.caption)
            .text_color(look.label)
            .border_1()
            .border_color(look.frame)
            .rounded(ink.small_radius())
            .opacity(look.opacity)
            .child(label)
            .child(track(&self.controls, "trash-empty"))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|pane, _: &MouseUpEvent, _, cx| {
                    pane.button.pressed = false;
                    cx.notify();
                }),
            )
            .on_hover(cx.listener(|pane, hovered: &bool, _, cx| {
                pane.button.hover = *hovered;
                cx.notify();
            }));
        if let Some(wash) = look.wash {
            button = button.bg(wash);
        }
        if self.empty_available() {
            button = button
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|pane, _: &MouseDownEvent, _, cx| {
                        pane.button.pressed = true;
                        cx.notify();
                    }),
                )
                .on_click(cx.listener(|pane, _, _, cx| {
                    pane.button.pressed = false;
                    pane.empty_pressed(cx);
                }));
        }
        div()
            .h(px(board.strip_h))
            .flex_none()
            .flex()
            .items_center()
            .px(px(board.pad_x))
            .gap(px(board.gap))
            .bg(ink.surface)
            .border_b_1()
            .border_color(ink.hair())
            .text_size(board.caption)
            .text_color(ink.foreground)
            .child(self.chrome_button(Glyph::Back, true, "trash-back", cx))
            .child(self.chrome_button(Glyph::Up, false, "trash-up", cx))
            .child(
                div()
                    .flex_none()
                    .child(SharedString::from(catalog.tr("trash.title"))),
            )
            .child(div().flex_1().min_w(px(0.)).overflow_hidden().child(count))
            .child(button)
            .into_any_element()
    }

    fn header(&self) -> AnyElement {
        let ink = self.paint.borrow().ink;
        let board = self.board();
        let tr = |key: &str| SharedString::from(self.catalog.tr(key));
        let deleted = SharedString::from(crate::navigation::sort::title(
            &self.catalog.tr("trash.header.deleted"),
            true,
        ));
        div()
            .debug_selector(|| "trash-header".into())
            .h(px(board.header_h))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(board.gap))
            .px(px(board.pad_x))
            .text_size(board.caption)
            .text_color(ink.foreground)
            .border_b_1()
            .border_color(ink.hair())
            // Name stands above the marks, as every folder's header sets it.
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .child(tr("trash.header.name")),
            )
            .child(
                div()
                    .w(px(board.location_w))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(tr("trash.header.location")),
            )
            .child(
                div()
                    .w(px(board.deleted_w))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(deleted),
            )
            .into_any_element()
    }

    /// The header's bare labels for the IPC describe, before the sort mark.
    pub fn header_labels(&self) -> [String; 3] {
        [
            "trash.header.name",
            "trash.header.location",
            "trash.header.deleted",
        ]
        .map(|key| self.catalog.tr(key))
    }

    /// TrashView.qml's centred line while nothing lists: an error, Reading Trash…, or Trash is empty.
    pub fn empty_line(&self) -> Option<(String, bool)> {
        let view = &self.host.view;
        let paint = self.paint.borrow();
        if paint.total > 0 || !view.opened {
            return None;
        }
        Some(match &paint.error_text {
            Some(text) => (text.clone(), true),
            None if view.busy() => (self.catalog.tr("trash.reading"), false),
            None => (self.catalog.tr("trash.is_empty"), false),
        })
    }

    /// A pointer move over the rows; a move to the point the keyboard quieted is not a move, so the wash stays off.
    pub fn pointer_moved(
        &mut self,
        at: gpui::Point<Pixels>,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        if !self.pointer.moved(at) {
            return;
        }
        self.hover(index, cx);
    }

    fn row_at(&self, y: Pixels) -> Option<usize> {
        let bounds = self.paint.borrow().bounds?;
        let y = y - bounds.origin.y;
        if y < px(0.) || y >= bounds.size.height {
            return None;
        }
        let at = ((f32::from(y) + self.host.view.scroll_y) / self.board().row_h).floor();
        (at >= 0. && (at as usize) < self.host.view.total).then_some(at as usize)
    }
}

impl Render for TrashPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.evict(window, cx);
        // Every Trash label draws in the system font the list follows, never a family fixed at mount.
        let face = crate::list::Typeface::for_window(window, cx);
        self.set_face(&face, cx);
        self.sync_keys_away(window, cx);
        let ink = self.paint.borrow().ink;
        if self.presentation_loading {
            self.loading_at
                .get_or_insert(cx.background_executor().now());
        }
        let empty = if self.presentation_loading {
            Some((self.catalog.tr("trash.reading"), false))
        } else {
            self.empty_line()
        }
        .map(|(text, error)| {
            div()
                .debug_selector(|| "trash-empty-line".into())
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(self.board().body)
                .text_color(if error { ink.error } else { ink.muted })
                .child(SharedString::from(text))
        });
        let list = div()
            .debug_selector(|| "trash-list-area".into())
            .relative()
            .flex_1()
            .min_h(px(0.))
            .when(!self.presentation_loading, |list| {
                list.child(TrashList(self.paint.clone()))
            })
            .on_mouse_move(cx.listener(move |pane, event: &MouseMoveEvent, _, cx| {
                if pane.bar_move(
                    event.position,
                    event.pressed_button == Some(MouseButton::Left),
                    cx,
                ) {
                    return;
                }
                let index = pane.row_at(event.position.y);
                pane.pointer_moved(event.position, index, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |pane, event: &MouseDownEvent, window, cx| {
                    window.focus(&pane.focus, cx);
                    if pane.bar_press(event.position, cx) {
                        cx.stop_propagation();
                        return;
                    }
                    if let Some(index) = pane.row_at(event.position.y) {
                        pane.click_row(index, event.modifiers.control, cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|pane, _: &MouseUpEvent, _, cx| pane.bar_release(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|pane, _: &MouseUpEvent, _, cx| pane.bar_release(cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |pane, event: &MouseDownEvent, _, cx| {
                    let index = pane.row_at(event.position.y);
                    let bounds = pane.paint.borrow().bounds;
                    let (x, y) = bounds.map_or((0., 0.), |bounds| {
                        (
                            f32::from(bounds.origin.x),
                            f32::from(bounds.origin.y) - pane.board().top(),
                        )
                    });
                    pane.right_click(
                        index,
                        f32::from(event.position.x) - x,
                        f32::from(event.position.y) - y,
                        cx,
                    );
                }),
            )
            .on_scroll_wheel(cx.listener(|pane, event: &ScrollWheelEvent, _, cx| {
                let row = px(pane.board().row_h);
                pane.wheel(-f32::from(event.delta.pixel_delta(row).y), cx);
                cx.stop_propagation();
            }))
            .when(!self.presentation_loading, |list| {
                list.child(self.scrollbar(window, cx))
            });
        div()
            .debug_selector(|| "trash-view".into())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(ink.background)
            .font_family(self.paint.borrow().font.family.clone())
            .track_focus(&self.focus)
            .child(self.strip(cx))
            .child(self.header())
            .child(list)
            .children(empty)
            .into_any_element()
    }
}
