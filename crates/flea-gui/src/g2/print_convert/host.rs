//! The card entity; backend and document effects leave through the window's existing services.
use super::{
    controls::Control,
    model::{Action, Card, Focus, Model},
    preview::Labels,
    seam::{Effect, Thumbnail},
    view,
};
use crate::g2::file_ops::palette::CardPalette;
use flea::{
    i18n::I18n,
    ui_logic_print_convert::{geometry::Composition, selection::Selection},
};
use gpui::{div, prelude::*, px, Context, EventEmitter, FocusHandle, MouseButton, Task, Window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Instant,
};

pub struct OutputEvent(pub Effect);

pub struct OutputView {
    pub model: Model,
    pub(super) editor: super::editor::Editor,
    pub catalog: Arc<I18n>,
    pub palette: CardPalette,
    pub thumbnails: Vec<Thumbnail>,
    pub hovered: Option<Control>,
    pub pressed: Option<Control>,
    pub motion: Option<(Control, bool, Instant)>,
    pub dragging: Option<usize>,
    pub labels: Rc<RefCell<Labels>>,
    pub cards_below: usize,
    pub preview_bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    pub pointer_position: Option<gpui::Point<gpui::Pixels>>,
    pub(super) printer_menu: super::printer_menu::PrinterMenu,
    pub(super) body_scroll: super::body_scroll::BodyScroll,
    released: Vec<Arc<gpui::RenderImage>>,
    pub(super) focus: FocusHandle,
    check: Option<Task<()>>,
    pub(super) select_all: bool,
    /// The card and title boxes the last paint drew, for convertCardRect and convertTitleCentre.
    pub observed: super::observations::Observations,
}

impl EventEmitter<OutputEvent> for OutputView {}

impl OutputView {
    pub fn new(catalog: Arc<I18n>, palette: CardPalette, cx: &mut Context<Self>) -> Self {
        Self {
            model: Model::default(),
            editor: Default::default(),
            catalog,
            palette,
            thumbnails: vec![],
            hovered: None,
            pressed: None,
            motion: None,
            dragging: None,
            labels: Default::default(),
            cards_below: 0,
            focus: cx.focus_handle(),
            check: None,
            select_all: false,
            preview_bounds: Default::default(),
            pointer_position: None,
            printer_menu: Default::default(),
            body_scroll: Default::default(),
            released: vec![],
            observed: Default::default(),
        }
    }

    pub fn open(
        &mut self,
        action: Action,
        selection: Selection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.model.open(action, selection, &self.catalog) {
            self.flush(cx);
            return false;
        }
        self.editor.close();
        self.released
            .extend(self.thumbnails.drain(..).map(|t| t.image));
        self.hovered = None;
        self.pressed = None;
        self.dragging = None;
        self.motion = None;
        self.select_all = false;
        self.pointer_position = None;
        self.printer_menu.close();
        self.body_scroll.reset();
        window.focus(&self.focus, cx);
        self.flush(cx);
        true
    }

    /// pv1 lands the complete captured set once; an obsolete or partial set cannot replace a good preview.
    pub fn set_thumbnails(
        &mut self,
        generation: u64,
        thumbnails: Vec<Thumbnail>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selection) = &self.model.selection else {
            return false;
        };
        if self.model.card.is_none()
            || generation != selection.generation
            || thumbnails.len() != selection.files.len()
            || thumbnails.len() > 256
            || thumbnails
                .iter()
                .any(|t| t.size.0 == 0 || t.size.1 == 0 || t.size.0 > 50_000 || t.size.1 > 50_000)
            || selection
                .files
                .iter()
                .any(|f| thumbnails.iter().filter(|t| t.path == f.path).count() != 1)
        {
            return false;
        }
        self.released.extend(
            self.thumbnails
                .iter()
                .filter(|old| {
                    !thumbnails
                        .iter()
                        .any(|new| Arc::ptr_eq(&old.image, &new.image))
                })
                .map(|t| t.image.clone()),
        );
        self.thumbnails = thumbnails;
        cx.notify();
        true
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.printer_menu.close();
        self.catalog = catalog;
        self.model.localize(&self.catalog);
        self.labels = Default::default();
        self.flush(cx);
    }

    pub fn line(&mut self, line: &str, cx: &mut Context<Self>) -> bool {
        let focus = self.model.focus;
        let taken = self.model.line(line, &self.catalog);
        if taken {
            if self.model.focus != focus {
                self.body_scroll.request_reveal();
            }
            self.flush(cx);
        }
        taken
    }

    pub fn flush(&mut self, cx: &mut Context<Self>) {
        self.sync_editor();
        if !self.model.editable() {
            self.printer_menu.close();
        }
        if self.model.card.is_none() {
            self.released
                .extend(self.thumbnails.drain(..).map(|t| t.image));
            self.check = None;
            self.labels = Default::default();
        }
        for effect in std::mem::take(&mut self.model.effects) {
            match effect {
                Effect::CheckOutputs { ticket, paths } => {
                    let pdf_pages = matches!(&self.model.card, Some(Card::Convert(c)) if c.kind == flea::ui_logic_print_convert::output::Kind::Pdf);
                    self.check = Some(cx.spawn(async move |this, cx| {
                        let result = cx
                            .background_executor()
                            .spawn(async move { super::check::paths(paths, pdf_pages) })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            this.model.checked_paths(ticket, result, &this.catalog);
                            this.flush(cx);
                        });
                    }));
                }
                other => cx.emit(OutputEvent(other)),
            }
        }
        cx.notify();
    }

    pub fn activate(&mut self, control: Control, window: &Window, cx: &mut Context<Self>) {
        match control {
            Control::Cancel => {
                self.model.cancel();
            }
            Control::Submit => self.model.submit(&self.catalog),
            _ if !self.model.editable() => return,
            Control::Field(focus) => {
                self.select_all = false;
                self.model.focus = Some(focus);
                if focus == Focus::Printer {
                    self.toggle_printers(window);
                }
            }
            Control::Choice(Focus::Formats, i) => self.model.choose_format(i, &self.catalog),
            Control::Choice(Focus::Printer, i) => self.model.query_printer(Some(i)),
            Control::Choice(focus, i) => self.model.setting(focus, i, &self.catalog),
            Control::Check(Focus::Check) => self.model.toggle_check(&self.catalog),
            Control::Check(focus) => {
                if let Some(Card::Print(p)) = &mut self.model.card {
                    if focus == Focus::Duplex && p.capabilities.duplex {
                        p.two_sided = !p.two_sided;
                    }
                    if focus == Focus::Grayscale && p.capabilities.color {
                        p.grayscale = !p.grayscale;
                    }
                    self.model.focus = Some(focus);
                }
            }
            Control::Image(i) => {
                if let Some(Card::Combine(c)) = &mut self.model.card {
                    c.focused = Some(i);
                    self.model.focus = Some(Focus::Preview);
                }
            }
        }
        self.body_scroll.request_reveal();
        self.flush(cx);
    }

    pub fn image_at(&self, x: f64, y: f64, width: f64) -> Option<usize> {
        let (Some(Card::Combine(c)), Some(s)) = (&self.model.card, &self.model.selection) else {
            return None;
        };
        let sizes = s
            .files
            .iter()
            .map(|f| {
                self.thumbnails
                    .iter()
                    .find(|t| t.path == f.path)
                    .map(|t| t.size)
            })
            .collect::<Option<Vec<_>>>()?;
        let geometry = Composition::new(&sizes, &c.settings)
            .ok()?
            .fitted(width, 278., 14.);
        geometry
            .tiles
            .iter()
            .position(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
    }

    pub fn pointer_image(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        release: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.model.editable() {
            return;
        }
        let hit = self.preview_bounds.get().and_then(|bounds| {
            self.image_at(
                f64::from(f32::from(at.x - bounds.origin.x)),
                f64::from(f32::from(at.y - bounds.origin.y)),
                f64::from(f32::from(bounds.size.width)),
            )
        });
        if release {
            if let (Some(from), Some(to)) = (self.dragging.take(), hit) {
                self.model.move_image(from, to, &self.catalog);
            }
        } else if let Some(i) = hit {
            self.dragging = Some(i);
            if let Some(Card::Combine(c)) = &mut self.model.card {
                c.focused = Some(i);
            }
            self.model.focus = Some(Focus::Preview);
        }
        self.flush(cx);
    }
}

impl Render for OutputView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for image in self.released.drain(..) {
            let _ = window.drop_image(image);
        }
        self.sync_editor();
        self.editor.face = Some(crate::list::Typeface::for_window(window, cx));
        let room = window.viewport_size();
        self.body_scroll.prepare(&self.model);
        let mut root = div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|view, key, window, cx| view.key(key, window, cx)));
        if self.model.card.is_none() {
            return root;
        }
        root = root.child(
            div()
                .absolute()
                .size_full()
                .bg(self
                    .palette
                    .background
                    .opacity(flea::ui_logic_file_ops::card::scrim_alpha(self.cards_below)))
                .debug_selector(|| "op4-scrim".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, _, cx| {
                        view.model.close();
                        view.flush(cx);
                    }),
                )
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
        );
        if let Some(card) = view::card(self, (f32::from(room.width), f32::from(room.height)), cx) {
            root = root.child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        card.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
                    ),
            );
        }
        if self.printer_menu.opened() {
            root = root.child(super::printer_menu::overlay(self, cx));
        } else {
            root = root.children(super::printer_menu::closing(self));
        }
        root.text_size(px(14.))
    }
}
