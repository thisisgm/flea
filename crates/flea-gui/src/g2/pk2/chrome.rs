//! Path bar, rail, grid, chips on picker facts; decides nothing.
use crate::g2::mn1::{
    element::MenuElement,
    lines::Env,
    menu::{Face, Menu},
};
use crate::grid::{
    element::{GridElement, GridState, Overlay, Plain},
    geometry::Metrics,
    FocusSnapshot, GridView,
};
use crate::list::{pane::Held, Typeface};
use crate::path::{
    rank::Jump,
    view::{Inputs, PathView},
};
use crate::rail::{
    host::{Build, Host},
    Mode,
};
use flea::i18n::I18n;
use flea::ui_logic_mn1::{fit::Rect, Entry, Text};
use flea::ui_logic_pk2::request::Chip;
use gpui::{div, prelude::*, px, AnyView, App, Context, Entity, IntoElement, Render, Window};
use std::{cell::RefCell, rc::Rc, sync::Arc};

// The chip menu card keeps this far inside its window.
pub(super) const MENU_INSET: f32 = 4.;

/// The work area is the opening window, so the card fits.
fn work_area(window: &Window) -> Rect {
    let viewport = window.viewport_size();
    Rect {
        x: 0.,
        y: 0.,
        w: f32::from(viewport.width),
        h: f32::from(viewport.height),
    }
}

/// Path bar with picker paths; ranking waits, so Unranked.
pub struct PathBar {
    path: Entity<PathView>,
}

impl PathBar {
    pub fn new(i18n: &I18n, cx: &mut Context<Self>) -> Self {
        let catalog = Arc::new(i18n.clone());
        let path = cx.new(|cx| PathView::new(catalog, Box::new(Jump::default()), cx));
        Self { path }
    }

    pub fn sync(&mut self, inputs: Inputs, cx: &mut Context<Self>) {
        self.path.update(cx, |path, cx| path.sync(inputs, cx));
    }

    pub fn path(&self) -> Entity<PathView> {
        self.path.clone()
    }
}

impl Render for PathBar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .debug_selector(|| "pk2-path".into())
            .child(self.path.clone())
    }
}

/// The places rail: the G1.11 Host, filled once live.
pub struct RailPanel {
    host: Entity<Host>,
}

impl RailPanel {
    pub fn new(mode: Mode, width: f32, cx: &mut Context<Self>) -> Self {
        let build: Build =
            Box::new(|_: &mut Window, cx: &mut App| AnyView::from(cx.new(|_| Places)));
        let host = cx.new(|cx| Host::new(mode, width, build, cx));
        Self { host }
    }

    pub fn host(&self) -> Entity<Host> {
        self.host.clone()
    }
}

impl Render for RailPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .debug_selector(|| "pk2-rail".into())
            .child(self.host.clone())
    }
}

/// The places list's slot; pl4 fills it.
struct Places;

impl Render for Places {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().debug_selector(|| "pk2-places".into())
    }
}

/// Tile grid over held rows; no thumbs, selection, drop.
pub struct TileGrid {
    held: Rc<Held>,
    state: Rc<RefCell<GridState>>,
    size: (f32, f32),
}

impl TileGrid {
    pub fn new(size: (f32, f32)) -> Self {
        Self {
            held: Rc::new(Held {
                listing: 0,
                total: 0,
                start: 0,
                rows: Vec::new(),
            }),
            state: Rc::new(RefCell::new(GridState::omarchy_default())),
            size,
        }
    }

    /// The listing's held window, shared; the grid never copies.
    pub fn show(&mut self, held: Rc<Held>) {
        self.held = held;
    }

    // Test hooks into what the frame drew.
    #[cfg(test)]
    pub fn shaped(&self) -> (u64, u64, usize) {
        self.state.borrow().shaped()
    }

    #[cfg(test)]
    pub fn frames(&self) -> u64 {
        self.state.borrow().frames
    }
}

impl Render for TileGrid {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let view = GridView::new(Metrics::board(), self.size);
        let element = GridElement::new(
            self.held.clone(),
            None,
            view,
            Rc::new(FocusSnapshot::default()),
            Rc::new(Plain),
            Overlay::default(),
            self.state.clone(),
        );
        div()
            .size_full()
            .debug_selector(|| "pk2-tiles".into())
            .child(element)
    }
}

/// Chip menu: mn1 card, All files; file face, no chip face.
pub struct ChipMenu {
    pub(super) menu: Rc<RefCell<Menu>>,
    pub(super) catalog: I18n,
    pub(super) typeface: Typeface,
}

impl ChipMenu {
    pub fn new(catalog: I18n, typeface: Typeface) -> Self {
        Self {
            menu: Rc::new(RefCell::new(Menu::default())),
            catalog,
            typeface,
        }
    }

    /// Opens the card at a point; each chip's index names it.
    pub fn open(&mut self, chips: &[Chip], at: (f32, f32), window: &Window) {
        let entries: Vec<Entry> = chips
            .iter()
            .map(|chip| {
                let action = format!("chip:{}", chip.index);
                let mut entry = Entry::leaf(action.clone(), Text::Data(chip.label.clone()));
                entry.action = action.into();
                entry
            })
            .collect();
        let env = Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: true,
            typeface: &self.typeface,
            inset: MENU_INSET,
        };
        let work = work_area(window);
        self.menu
            .borrow_mut()
            .open(Face::File, entries, at, work, "chips", &env, window);
    }

    pub fn opened(&self) -> bool {
        self.menu.borrow().opened()
    }

    pub fn visible(&self) -> bool {
        self.menu.borrow().visible()
    }

    #[cfg(test)]
    pub fn card_opacity(&self) -> f32 {
        self.menu.borrow().fades.card_frame.opacity
    }

    // Test hook: the card's frame, held inside its window.
    #[cfg(test)]
    pub fn card_frame(&self) -> Option<Rect> {
        self.menu.borrow().card.as_ref().map(|card| card.frame)
    }

    // Test hook: the card's entries, in order.
    #[cfg(test)]
    pub fn card_labels(&self) -> Vec<String> {
        use flea::ui_logic_mn1::Text;
        self.menu
            .borrow()
            .card
            .iter()
            .flat_map(|card| card.entries.iter())
            .map(|entry| match &entry.label {
                Some(Text::Data(name)) => name.clone(),
                _ => String::new(),
            })
            .collect()
    }
}

impl Render for ChipMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if cx.try_global::<crate::presentation::Fonts>().is_some() {
            let face = Typeface::for_window(window, cx);
            if self.typeface != face {
                self.typeface = face;
                let entries = self
                    .menu
                    .borrow()
                    .card
                    .as_ref()
                    .map(|card| card.entries.clone());
                if let Some(entries) = entries {
                    let env = Env {
                        catalog: &self.catalog,
                        preset: "default",
                        key_hints: true,
                        typeface: &self.typeface,
                        inset: MENU_INSET,
                    };
                    self.menu.borrow_mut().refresh(entries, &env, window);
                }
            }
        }
        let card = MenuElement::new(self.menu.clone(), px(crate::compositor::corner(cx)));
        // A closing card fades out without taking the pointer.
        if !self.opened() {
            return div().size_full().child(card);
        }
        div()
            .size_full()
            .debug_selector(|| "pk2-chip-menu".into())
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|menu, event: &gpui::MouseDownEvent, window, cx| {
                    menu.press(
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .child(card)
    }
}
