//! Where the path surface meets the pointer and the keyboard: the chrome slot, a dual pane's strip and the popup.
use super::{
    field::FieldElement,
    menu::MenuElement,
    paint::{CrumbElement, Place},
    suggestions::Popup,
    view::PathView,
};
use crate::window::input::{clean_paste, modifiers, paste_key};
use gpui::{
    deferred, div, prelude::*, px, AnyElement, ClipboardItem, Context, Entity, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollWheelEvent, Window,
};

impl Render for PathView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        // Launches install the fonts before any window; a host without them keeps the face its caller set.
        if cx.try_global::<crate::presentation::Fonts>().is_some() {
            self.paint
                .borrow_mut()
                .set_typeface(crate::list::Typeface::for_window(window, cx));
        }
        let probe = self.probe.clone();
        let root = div().size_full().relative();
        if self.entry.editing {
            let field = div()
                .size_full()
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key_down))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::field_press))
                .on_mouse_down(MouseButton::Right, cx.listener(Self::field_menu))
                .on_mouse_move(cx.listener(Self::field_drag))
                .child(FieldElement::new(view.clone()))
                .children(self.text_menu.overlay(
                    &view,
                    px(crate::compositor::corner(cx)),
                    Self::text_menu_answer,
                ));
            let root = root.child(field);
            if !self.jump.shown() {
                self.paint.borrow_mut().drawn = 0;
                return super::probe::wrap(root, view, 0, probe);
            }
            if self.pointer.settling {
                cx.on_next_frame(window, |view, _, _| view.pointer.settling = false);
            }
            let rows = self.jump.entries.len();
            let width = self
                .paint
                .borrow()
                .area
                .map_or(0., |area| f32::from(area.size.width));
            let scale = self.paint.borrow().scale();
            let popup = Popup::measure(
                scale,
                f32::from(window.viewport_size().height),
                0.,
                width,
                rows,
            );
            let menu = div()
                .absolute()
                .left(px(0.))
                .top(px(popup.y))
                .w(px(popup.width))
                .h(px(popup.height))
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(Self::menu_press))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::menu_release))
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|view, _, _, _| view.pressed_row = None),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|_, _: &MouseDownEvent, window, _| window.prevent_default()),
                )
                .on_mouse_move(cx.listener(Self::menu_move))
                .on_scroll_wheel(cx.listener(Self::menu_wheel))
                .child(MenuElement::new(view.clone()));
            return super::probe::wrap(root.child(deferred(menu).with_priority(1)), view, 0, probe);
        }
        self.paint.borrow_mut().drawn = 0;
        if self.inputs.dual {
            return super::probe::wrap(
                root.child(CrumbElement::new(view.clone(), Place::Chrome)),
                view,
                0,
                probe,
            );
        }
        super::probe::wrap(
            root.child(strip(view.clone(), Place::Chrome, cx)),
            view,
            0,
            probe,
        )
    }
}

/// A dual pane's own strip, for the pane's 27 px band; empty and unbuilt while the view is single.
pub fn pane_strip(view: &Entity<PathView>, side: usize, cx: &mut gpui::App) -> AnyElement {
    if !view.read(cx).inputs.dual {
        return div().into_any_element();
    }
    view.update(cx, |_, cx| {
        strip(view.clone(), Place::Pane(side), cx).into_any_element()
    })
}

fn strip(view: Entity<PathView>, place: Place, cx: &mut Context<PathView>) -> impl IntoElement {
    let slot = place.slot();
    let origin = move |view: &PathView, x: gpui::Pixels| {
        let bounds = view.paint.borrow().strips[slot];
        let pad = super::paint::strip_pad(place != Place::Chrome, &view.paint.borrow().scale());
        bounds.map(|bounds| f32::from(x - bounds.origin.x) - pad)
    };
    div()
        .id(("path-strip", slot))
        .size_full()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                let Some(x) = origin(view, event.position.x) else {
                    return;
                };
                view.press_strip(
                    slot,
                    x,
                    event.click_count,
                    modifiers(event.modifiers),
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .on_mouse_move(cx.listener(move |view, event: &MouseMoveEvent, _, cx| {
            let x = origin(view, event.position.x);
            view.hover_strip(slot, x, cx);
        }))
        .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
            if !hovered {
                view.hover_strip(slot, None, cx);
            }
        }))
        .child(CrumbElement::new(view, place))
}

impl PathView {
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // The field's text menu holds the keyboard while it is open.
        if self.text_menu.opened() {
            let outcome = self.text_menu.key(event, window);
            self.text_menu_answer(outcome, window, cx);
            cx.stop_propagation();
            return;
        }
        let key = event.keystroke.key.as_str();
        let held = event.keystroke.modifiers;
        if self.entry.ime_key(key) {
            self.edited(cx);
            cx.stop_propagation();
            return;
        }
        if self.entry.marked.is_some() {
            cx.stop_propagation();
            return;
        }
        // A held Enter stands for the line as it was, so nothing typed after it changes what it opens; Escape still closes.
        if self.jump.enter_waiting && key != "escape" {
            cx.stop_propagation();
            return;
        }
        let at = self.entry.cursor();
        match key {
            "escape" => self.close(cx),
            "enter" => match self.press_enter() {
                Some(enter) => self.enter(enter, window, cx),
                None => self.commit(cx),
            },
            "tab" => self.tab(cx),
            "up" | "down" => {
                if self.jump.shown() && self.jump.step(if key == "down" { 1 } else { -1 }) {
                    self.reveal();
                    cx.notify();
                }
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
                cx.notify();
            }
            "backspace" | "delete" => {
                self.entry.erase(key == "delete");
                self.edited(cx);
            }
            _ if paste_key(event) => {
                let text = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .map(|text| clean_paste(&text))
                    .unwrap_or_default();
                if !text.is_empty() {
                    self.entry.replace(None, &text);
                    self.edited(cx);
                }
            }
            "a" | "c" | "x" if held.control && !held.alt && !held.shift => {
                if key == "a" {
                    self.entry.select_all();
                    cx.notify();
                } else if !self.entry.selected.is_empty() {
                    let text = self.entry.text[self.entry.selected.clone()].to_owned();
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    if key == "x" {
                        self.entry.replace(None, "");
                        self.edited(cx);
                    }
                }
            }
            // Printable keys go on to the input handler, which types them; every other key stops here.
            _ if event.keystroke.key_char.is_some() && !held.control && !held.platform => return,
            _ => {}
        }
        cx.stop_propagation();
    }

    fn field_index(&self, x: gpui::Pixels) -> usize {
        let origin = self.paint.borrow().field_origin;
        let line = self.line.borrow();
        line.index_for(x - origin + line.scroll)
    }

    fn field_press(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let at = self.field_index(event.position.x);
        match event.click_count {
            1 if event.modifiers.shift => self.entry.select_to(at),
            1 => self.entry.move_to(at),
            2 => {
                let text = &self.entry.text;
                let word = |c: char| c != '/' && !c.is_whitespace();
                let start = text[..at]
                    .rfind(|c: char| !word(c))
                    .map_or(0, |cut| cut + 1);
                let end = text[at..]
                    .find(|c: char| !word(c))
                    .map_or(text.len(), |cut| at + cut);
                self.entry.move_to(start);
                self.entry.select_to(end);
            }
            _ => self.entry.select_all(),
        }
        cx.notify();
    }

    // A right press in the field opens its text menu, the rename field's rows, over the line as it stands.
    fn field_menu(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.entry.marked.is_none() {
            let catalog = self.catalog.clone();
            self.text_menu
                .open(&self.entry, &catalog, event.position, window, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn text_menu_answer(
        &mut self,
        outcome: crate::g2::context_menus::menu::Outcome,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let crate::g2::context_menus::menu::Outcome::Fire { action, .. } = &outcome {
            if super::text_menu::apply(&mut self.entry, action, cx)
                == super::text_menu::Applied::Edited
            {
                self.edited(cx);
            }
        }
        self.text_menu.settle(&outcome);
        cx.notify();
    }

    fn field_drag(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button == Some(MouseButton::Left) {
            let at = self.field_index(event.position.x);
            self.entry.select_to(at);
            cx.notify();
        }
    }

    fn row_at(&self, x: gpui::Pixels, y: gpui::Pixels) -> Option<usize> {
        let popup = self.paint.borrow().popup?;
        popup.row_at(
            f32::from(x),
            f32::from(y),
            self.scroll,
            self.jump.entries.len(),
        )
    }

    // A row waits for release within its bounds, as MenuRow.qml's TapHandler does.
    fn menu_press(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.prevent_default();
        cx.stop_propagation();
        self.pressed_row = self
            .row_at(event.position.x, event.position.y)
            .map(|at| (at, self.jump.entries[at].path.clone()));
        if self.pressed_row.is_none() {
            self.close(cx);
        }
    }

    fn menu_release(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let Some((at, path)) = self.pressed_row.take() else {
            return;
        };
        if self.row_at(event.position.x, event.position.y) == Some(at)
            && self
                .jump
                .entries
                .get(at)
                .is_some_and(|row| row.path == path)
        {
            self.choose(path, cx);
        }
    }

    fn menu_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let at = (f32::from(event.position.x), f32::from(event.position.y));
        if !self.pointer.moved(at) {
            return;
        }
        if let Some(row) = self.row_at(event.position.x, event.position.y) {
            if self.jump.cursor != Some(row) {
                self.jump.cursor = Some(row);
                cx.notify();
            }
        }
    }

    // The popup steps its highlight like a menu: one row a notch, one row per row height of touchpad travel.
    fn menu_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let row = self.paint.borrow().scale().row;
        self.wheel -= f32::from(event.delta.pixel_delta(px(row)).y) / row;
        let mut moved = false;
        while self.wheel.abs() >= 1. {
            let step = self.wheel.signum();
            self.wheel -= step;
            moved |= self.jump.step(step as isize);
        }
        if moved {
            self.reveal();
            cx.notify();
        }
    }
}
