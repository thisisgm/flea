//! The text menu a line editor opens on a right press: the shared menu card over the field, which keeps the keyboard and hands its keys here.
use crate::g2::context_menus::{
    element::MenuElement,
    lines::Env,
    menu::{Face, Menu, Outcome},
};
use crate::list::Typeface;
use crate::path::entry::Entry;
use flea::i18n::I18n;
use flea::ui_logic_context_menus::{fit::Rect, Entry as Row, Text};
use gpui::{
    anchored, deferred, div, point, prelude::*, px, AnyElement, App, ClipboardItem, Context,
    Entity, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point,
    ScrollWheelEvent, Window,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// Undo, then Cut, Copy and Paste, then Select All; Undo dims with nothing to undo, Paste with nothing to paste.
pub(crate) fn rows(entry: &Entry, paste: bool) -> Vec<Row> {
    let row = |id: &'static str, key: &'static str, glyph: &'static str, enabled: bool| Row {
        disabled: !enabled,
        ..Row::row(id, Text::Key(key), glyph)
    };
    let selected = entry.copyable().is_some();
    vec![
        row("edit:undo", "keys.action.undo", "undo", entry.can_undo()),
        Row::separator(),
        row("edit:cut", "menu.row.cut", "scissors", selected),
        row("edit:copy", "menu.row.copy", "copy", selected),
        row("edit:paste", "menu.row.paste", "clipboard", paste),
        Row::separator(),
        row(
            "edit:selectAll",
            "menu.row.select_all",
            "checkmark",
            !entry.text.is_empty(),
        ),
    ]
}

/// Whether the clipboard holds text a Paste row would enter.
pub(crate) fn can_paste(cx: &App) -> bool {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .is_some_and(|text| !text.is_empty())
}

/// What a chosen row did to the line: its text changed, only its selection moved, or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Applied {
    Edited,
    Moved,
    Nothing,
}

/// Runs one row on the line; a paste drops control characters, and a number line takes only digits.
pub(crate) fn apply(entry: &mut Entry, action: &str, cx: &mut App) -> Applied {
    match action {
        "edit:undo" if entry.can_undo() => {
            entry.undo();
            Applied::Edited
        }
        "edit:selectAll" => {
            entry.select_all();
            Applied::Moved
        }
        "edit:copy" | "edit:cut" => {
            let Some(text) = entry.copyable() else {
                return Applied::Nothing;
            };
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
            if action == "edit:copy" {
                return Applied::Nothing;
            }
            entry.replace(None, "");
            Applied::Edited
        }
        "edit:paste" => {
            let text: String = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap_or_default()
                .chars()
                .filter(|ch| !ch.is_control())
                .collect();
            if !text.is_empty() && entry.ime_replace(None, &text) {
                Applied::Edited
            } else {
                Applied::Nothing
            }
        }
        _ => Applied::Nothing,
    }
}

/// One field's menu card, opened over the window at the press.
#[derive(Default)]
pub(crate) struct TextMenu {
    pub menu: Rc<RefCell<Menu>>,
    catalog: Option<Arc<I18n>>,
    typeface: Option<Typeface>,
}

// The catalog and face a card opened with, which its pointer handlers measure rows in.
type Shared = Rc<(Arc<I18n>, Typeface)>;

/// The card's inset at the base text size, scaled with the face as the window's own menus scale it.
const INSET: f32 = 4.;
const BASE_TEXT: f32 = 12.;
/// The pixels a wheel line moves the highlight, as the Trash browser's menu reads a wheel.
const WHEEL_LINE: f32 = 37.;

fn env<'a>(catalog: &'a I18n, typeface: &'a Typeface) -> Env<'a> {
    Env {
        catalog,
        preset: "default",
        key_hints: false,
        typeface,
        inset: (INSET * f32::from(typeface.base) / BASE_TEXT).round(),
    }
}

impl TextMenu {
    pub fn opened(&self) -> bool {
        self.menu.borrow().opened()
    }

    /// Opens the card at the press over the line's rows, read now.
    pub fn open(
        &mut self,
        entry: &Entry,
        catalog: &Arc<I18n>,
        at: Point<Pixels>,
        window: &Window,
        cx: &App,
    ) {
        let typeface = Typeface::for_window(window, cx);
        let size = window.viewport_size();
        let work = Rect {
            x: 0.,
            y: 0.,
            w: size.width.into(),
            h: size.height.into(),
        };
        self.menu.borrow_mut().open(
            Face::Text,
            rows(entry, can_paste(cx)),
            (at.x.into(), at.y.into()),
            work,
            "",
            &env(catalog, &typeface),
            window,
        );
        self.catalog = Some(catalog.clone());
        self.typeface = Some(typeface);
    }

    pub fn close(&mut self) {
        self.menu.borrow_mut().close();
    }

    /// A key while the card is open: the menu's own keys step, choose or close, and every other key is held from the field.
    pub fn key(&mut self, event: &KeyDownEvent, window: &Window) -> Outcome {
        let (Some(catalog), Some(typeface)) = (&self.catalog, &self.typeface) else {
            return Outcome::Nothing;
        };
        let found = crate::window::input::Keyboard::default().lookup(
            "default",
            flea::keys::Context::Menu,
            event,
            window.modifiers(),
            window.layout_mirrored(),
        );
        let flea::keys::Match::Action(action) = found else {
            return Outcome::Nothing;
        };
        let env = env(catalog, typeface);
        self.menu.borrow_mut().act(action, None, "", &env, window)
    }

    /// Fire, a refusal or a close ends the card, as the window's own menus end theirs.
    pub fn settle(&mut self, outcome: &Outcome) {
        if matches!(
            outcome,
            Outcome::Fire { .. } | Outcome::Closed | Outcome::Refused(_)
        ) {
            self.close();
        }
    }

    /// The card over the whole window while it shows; a press chooses a row or closes it, and `answer` hands the outcome to the field's host.
    pub fn overlay<T: 'static>(
        &self,
        view: &Entity<T>,
        radius: Pixels,
        answer: Answer<T>,
    ) -> Option<AnyElement> {
        if !self.menu.borrow().visible() {
            return None;
        }
        let work = self.menu.borrow().work;
        let ground = div()
            .w(px(work.w))
            .h(px(work.h))
            .child(MenuElement::new(self.menu.clone(), radius));
        // A closing card fades out without taking the pointer.
        let ground = if !self.opened() {
            ground
        } else {
            let (Some(catalog), Some(typeface)) = (self.catalog.clone(), self.typeface.clone())
            else {
                return None;
            };
            let face = Rc::new((catalog, typeface));
            let press = |menu: Rc<RefCell<Menu>>, face: Shared, view: Entity<T>| {
                move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                    let outcome = menu.borrow_mut().click(
                        event.position.x.into(),
                        event.position.y.into(),
                        None,
                        "",
                        &env(&face.0, &face.1),
                        window,
                    );
                    view.update(cx, |host, cx| answer(host, outcome, window, cx));
                    cx.stop_propagation();
                }
            };
            let (hover, hover_face, hover_view) = (self.menu.clone(), face.clone(), view.clone());
            let wheel = self.menu.clone();
            let wheel_view = view.clone();
            ground
                .occlude()
                .debug_selector(|| "text-menu".into())
                .on_mouse_down(
                    MouseButton::Left,
                    press(self.menu.clone(), face.clone(), view.clone()),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    press(self.menu.clone(), face, view.clone()),
                )
                .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                    let outcome = hover.borrow_mut().hover(
                        event.position.x.into(),
                        event.position.y.into(),
                        &env(&hover_face.0, &hover_face.1),
                        window,
                    );
                    if outcome == Outcome::Redraw {
                        hover_view.update(cx, |_, cx| cx.notify());
                    }
                    cx.stop_propagation();
                })
                .on_scroll_wheel(move |event: &ScrollWheelEvent, _, cx| {
                    wheel.borrow_mut().touchpad(
                        event.position.x.into(),
                        event.position.y.into(),
                        -f32::from(event.delta.pixel_delta(px(WHEEL_LINE)).y),
                        false,
                    );
                    wheel_view.update(cx, |_, cx| cx.notify());
                    cx.stop_propagation();
                })
        };
        Some(
            deferred(anchored().position(point(px(0.), px(0.))).child(ground))
                .with_priority(2)
                .into_any_element(),
        )
    }
}

/// How a host takes a chosen row, a refusal or a close.
pub(crate) type Answer<T> = fn(&mut T, Outcome, &mut Window, &mut Context<T>);

#[path = "text_menu_slot.rs"]
mod slot;
pub(crate) use slot::Slot;

#[cfg(test)]
#[path = "text_menu_tests.rs"]
mod tests;
