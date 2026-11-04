//! Destination choices use mn1's bounded popup, leaving the Print card's geometry fixed.
use super::{host::OutputView, model::Card};
use crate::{
    g2::mn1::{
        element::MenuElement,
        lines::Env,
        menu::{Face, Menu, Outcome},
    },
    list::Typeface,
};
use flea::ui_logic_mn1::{fit::Rect, Entry, Text};
use gpui::{
    div, prelude::*, px, Bounds, Context, Div, KeyDownEvent, MouseButton, Pixels, ScrollDelta,
    Window,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
pub(super) struct PrinterMenu {
    pub bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub menu: Rc<RefCell<Menu>>,
    generation: Option<u64>,
}

impl PrinterMenu {
    pub fn opened(&self) -> bool {
        self.menu.borrow().opened()
    }
    pub fn close(&mut self) {
        self.menu.borrow_mut().close();
        self.generation = None;
    }
}

impl OutputView {
    pub(crate) fn restyle_printers(&mut self, face: &Typeface, window: &Window) {
        let entries = self.printer_entries();
        let env = Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: false,
            typeface: face,
            inset: 4.,
        };
        self.printer_menu
            .menu
            .borrow_mut()
            .refresh(entries, &env, window);
    }

    fn printer_entries(&self) -> Vec<Entry> {
        let Some(Card::Print(p)) = &self.model.card else {
            return vec![];
        };
        p.destinations
            .iter()
            .map(|d| Entry {
                id: d.name.clone().into(),
                action: d.name.clone().into(),
                label: Some(Text::Data(d.name.clone())),
                check: Some(d.name == p.printer),
                hint: d.default.then_some(Text::Key("dialogs.output.default")),
                ..Entry::default()
            })
            .collect()
    }

    pub(super) fn toggle_printers(&mut self, window: &Window) {
        if self.printer_menu.opened() {
            self.printer_menu.close();
            return;
        }
        let Some(Card::Print(p)) = &self.model.card else {
            return;
        };
        if !p.queried || p.pending.is_some() || p.destinations.is_empty() {
            return;
        }
        let (Some(bounds), Some(selection)) =
            (self.printer_menu.bounds.get(), &self.model.selection)
        else {
            return;
        };
        // The face the card's last render took from the window, so the menu follows the system font.
        let face = self
            .editor
            .face
            .clone()
            .unwrap_or_else(Typeface::omarchy_default);
        let env = Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: false,
            typeface: &face,
            inset: 4.,
        };
        let room = window.viewport_size();
        self.printer_menu.menu.borrow_mut().open(
            Face::Header,
            self.printer_entries(),
            (f32::from(bounds.origin.x), f32::from(bounds.bottom())),
            Rect {
                x: 0.,
                y: 0.,
                w: room.width.into(),
                h: room.height.into(),
            },
            "",
            &env,
            window,
        );
        self.printer_menu.generation = Some(selection.generation);
    }

    fn printer_outcome(&mut self, outcome: Outcome, cx: &mut Context<Self>) {
        if let Outcome::Fire { action, .. } = outcome {
            if self.model.editable()
                && self.model.selection.as_ref().map(|s| s.generation)
                    == self.printer_menu.generation
            {
                if let Some(Card::Print(p)) = &self.model.card {
                    if let Some(index) = p.destinations.iter().position(|d| d.name == action) {
                        self.model.query_printer(Some(index));
                    }
                }
            }
        }
        if !self.printer_menu.opened() {
            self.printer_menu.generation = None;
        }
        self.flush(cx);
    }

    pub(super) fn printer_key(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space") {
            return;
        }
        let action = match event.keystroke.key.as_str() {
            "escape" => "escape",
            "tab" if event.keystroke.modifiers.shift => "focusPrevious",
            "tab" | "down" | "j" => "cursorDown",
            "up" | "k" => "cursorUp",
            "enter" | "space" => "open",
            _ => return,
        };
        // The face the card's last render took from the window, so the menu follows the system font.
        let face = self
            .editor
            .face
            .clone()
            .unwrap_or_else(Typeface::omarchy_default);
        let env = Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: false,
            typeface: &face,
            inset: 4.,
        };
        let entries = self.printer_entries();
        let outcome =
            self.printer_menu
                .menu
                .borrow_mut()
                .act(action, Some(&entries), "", &env, window);
        self.printer_outcome(outcome, cx);
    }

    fn printer_pointer(
        &mut self,
        x: f32,
        y: f32,
        click: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // The face the card's last render took from the window, so the menu follows the system font.
        let face = self
            .editor
            .face
            .clone()
            .unwrap_or_else(Typeface::omarchy_default);
        let env = Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: false,
            typeface: &face,
            inset: 4.,
        };
        let outcome = if click {
            self.printer_menu.menu.borrow_mut().click(
                x,
                y,
                Some(&self.printer_entries()),
                "",
                &env,
                window,
            )
        } else {
            self.printer_menu
                .menu
                .borrow_mut()
                .hover(x, y, &env, window)
        };
        self.printer_outcome(outcome, cx);
    }
}

// A closing card fades out without taking the pointer.
pub(super) fn closing(view: &OutputView) -> Option<Div> {
    view.printer_menu.menu.borrow().visible().then(|| {
        div()
            .absolute()
            .size_full()
            .text_color(view.palette.foreground)
            .child(MenuElement::new(
                Rc::clone(&view.printer_menu.menu),
                px(view.palette.radius),
            ))
    })
}

pub(super) fn overlay(view: &OutputView, cx: &mut Context<OutputView>) -> gpui::Stateful<Div> {
    let mut layer = div()
        .id("op4-printer-popup")
        .absolute()
        .size_full()
        .occlude()
        .text_color(view.palette.foreground)
        .child(MenuElement::new(
            Rc::clone(&view.printer_menu.menu),
            px(view.palette.radius),
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, e: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                view.printer_pointer(e.position.x.into(), e.position.y.into(), true, window, cx);
            }),
        )
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_move(cx.listener(|view, e: &gpui::MouseMoveEvent, window, cx| {
            cx.stop_propagation();
            view.printer_pointer(e.position.x.into(), e.position.y.into(), false, window, cx);
        }))
        .on_scroll_wheel(cx.listener(|view, e: &gpui::ScrollWheelEvent, _, cx| {
            cx.stop_propagation();
            let x = e.position.x.into();
            let y = e.position.y.into();
            match e.delta {
                ScrollDelta::Lines(delta) => {
                    view.printer_menu
                        .menu
                        .borrow_mut()
                        .wheel(x, y, (-delta.y * 120.) as i32);
                }
                ScrollDelta::Pixels(delta) => {
                    view.printer_menu.menu.borrow_mut().touchpad(
                        x,
                        y,
                        -f32::from(delta.y),
                        e.touch_phase == gpui::TouchPhase::Started,
                    );
                }
            }
            cx.notify();
        }));
    // MenuElement reads the inherited roles, using the same captured palette as its parent card.
    layer.text_style().background_color = Some(view.palette.surface);
    layer
}
