//! The share browser is a flat viewport of menu rows, without an invented size or date column.
use crate::{
    g2::{
        context_menus::{
            glyphs,
            lines::{self, Built, Env},
        },
        file_ops::palette::CardPalette,
    },
    list::Typeface,
};
use flea::{
    i18n::I18n,
    ui_logic_context_menus::{Entry, Text},
    ui_logic_network_shares::browser::Browser,
};
use gpui::{
    div, fill, point, prelude::*, px, relative, size, App, Bounds, Context, Element, ElementId,
    Entity, EventEmitter, FocusHandle, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Style, Window,
};
use std::{cell::RefCell, sync::Arc};

pub struct Activated(pub String, pub String, pub u64);
pub struct Closed(pub u64);
pub struct ShareView {
    motion: crate::reveal::Reveal,
    pub model: Browser,
    pub palette: CardPalette,
    catalog: Arc<I18n>,
    focus: FocusHandle,
    cache: RefCell<Option<Cache>>,
    visible: usize,
    bounds: Option<Bounds<Pixels>>,
    slot_bounds: Option<Bounds<Pixels>>,
    hovered: Option<usize>,
    pressed: Option<usize>,
}
struct Cache {
    generation: u64,
    start: usize,
    count: usize,
    lines: Built,
}
impl EventEmitter<Activated> for ShareView {}
impl EventEmitter<Closed> for ShareView {}
impl ShareView {
    pub fn new(catalog: Arc<I18n>, palette: CardPalette, cx: &mut Context<Self>) -> Self {
        Self {
            motion: Default::default(),
            model: Browser::default(),
            palette,
            catalog,
            focus: cx.focus_handle(),
            cache: RefCell::default(),
            visible: 1,
            bounds: None,
            slot_bounds: None,
            hovered: None,
            pressed: None,
        }
    }
    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.catalog = catalog;
        self.cache.borrow_mut().take();
        cx.notify();
    }
    pub fn set_palette(&mut self, palette: CardPalette, cx: &mut Context<Self>) {
        self.palette = palette;
        self.cache.borrow_mut().take();
        cx.notify();
    }
    pub fn show(
        &mut self,
        uri: String,
        label: String,
        names: Vec<String>,
        origin: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.model.show(uri, label, names, origin);
        self.bounds = None;
        self.slot_bounds = None;
        self.hovered = None;
        self.pressed = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.model.open {
            self.model.close();
            cx.emit(Closed(self.model.origin));
            cx.notify();
        }
    }
    pub(crate) fn visible(&self) -> bool {
        self.model.open || self.motion.visible()
    }
    pub(crate) fn painted_bounds(&self) -> Option<Bounds<Pixels>> {
        self.slot_bounds.filter(|_| self.model.open)
    }
    pub(crate) fn inside(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
    }
    // G1 dispatch passes the generated action name and retains ownership of the keymap.
    pub fn action(&mut self, action: &str, cx: &mut Context<Self>) -> bool {
        if !self.model.open {
            return false;
        }
        match action {
            "escape" => self.close(cx),
            "cursorDown" => self.model.step(1, self.visible),
            "cursorUp" => self.model.step(-1, self.visible),
            "open" | "pageForward" => self.activate(cx),
            _ => return false,
        }
        cx.notify();
        true
    }
    fn row_at(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        if !self.model.open {
            return None;
        }
        let bounds = self.bounds?;
        if !bounds.contains(&position) {
            return None;
        }
        let row = self.model.start + (f32::from(position.y - bounds.origin.y) / 37.) as usize;
        (row < self.model.names.len()).then_some(row)
    }
    fn activate(&mut self, cx: &mut Context<Self>) {
        if let Some((uri, label, origin)) = self.model.activate() {
            self.model.close();
            cx.emit(Activated(uri, label, origin));
            cx.emit(Closed(origin));
            cx.notify();
        }
    }
}
impl gpui::Render for ShareView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let motion = self.motion.sample(
            self.model.open,
            cx.background_executor().now(),
            cx.reduce_motion(),
        );
        if motion.animating {
            window.request_animation_frame();
        }
        let root = div()
            .id("g2-pl2-shares")
            .relative()
            .size_full()
            .track_focus(&self.focus);
        if !motion.visible {
            return root;
        }
        let view = cx.entity();
        root.child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, _, cx| {
                    view.update(cx, |view, _| view.slot_bounds = Some(bounds));
                },
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .debug_selector(|| "share-motion".into())
                .size_full()
                .relative()
                .top(px(motion.rise))
                .opacity(motion.opacity)
                .child(Rows(cx.entity())),
        )
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(|this, e: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                window.focus(&this.focus, cx);
                this.pressed = this.row_at(e.position);
                cx.notify();
            }),
        )
        .on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(|this, e: &gpui::MouseUpEvent, _, cx| {
                cx.stop_propagation();
                let row = this.row_at(e.position);
                if let Some(row) = row.filter(|row| Some(*row) == this.pressed.take()) {
                    this.model.cursor = row;
                    this.activate(cx);
                }
                this.pressed = None;
                cx.notify();
            }),
        )
        .on_mouse_up_out(
            gpui::MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.pressed = None;
                cx.notify();
            }),
        )
        .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, _, cx| {
            let row = this.row_at(e.position);
            if row != this.hovered {
                this.hovered = row;
                cx.notify();
            }
        }))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
    }
}

struct Rows(Entity<ShareView>);
impl IntoElement for Rows {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Rows {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (
            window.request_layout(
                Style {
                    size: size(relative(1.).into(), relative(1.).into()),
                    ..Style::default()
                },
                None,
                cx,
            ),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.update(cx, |view, _| {
            let count = (f32::from(bounds.size.height) / 37.).ceil().max(1.) as usize;
            view.visible = count;
            view.bounds = Some(bounds);
            let mut cache = view.cache.borrow_mut();
            if cache.as_ref().is_some_and(|c| {
                c.generation == view.model.generation
                    && c.start == view.model.start
                    && c.count == count
            }) {
                return;
            }
            let rows = view
                .model
                .names
                .iter()
                .enumerate()
                .skip(view.model.start)
                .take(count)
                .map(|(index, name)| {
                    Entry::row(index.to_string(), Text::Data(name.clone()), "server")
                })
                .collect::<Vec<_>>();
            let mut typeface = Typeface::omarchy_default();
            typeface.font = window.text_style().font();
            let env = Env {
                catalog: &view.catalog,
                preset: "default",
                key_hints: false,
                typeface: &typeface,
                inset: 0.,
            };
            *cache = Some(Cache {
                generation: view.model.generation,
                start: view.model.start,
                count,
                lines: lines::build(
                    &rows,
                    &env,
                    window,
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.width),
                ),
            });
        });
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.0.read(cx);
        let p = view.palette;
        let cache = view.cache.borrow();
        window.paint_quad(fill(bounds, p.background));
        let Some(cache) = cache.as_ref() else {
            return;
        };
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for (index, row) in cache.lines.lines.iter().enumerate() {
                let top = bounds.origin.y + px(index as f32 * 37.);
                let picked = view.model.cursor == cache.start + index;
                if picked || view.hovered == Some(cache.start + index) {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.origin.x, top),
                            size(bounds.size.width, px(37.)),
                        ),
                        if picked {
                            p.accent.opacity(crate::list::CURSOR_WASH)
                        } else {
                            p.foreground.opacity(0.08)
                        },
                    ));
                }
                let (key, data) = glyphs::mark("server");
                let _ = window.paint_svg(
                    Bounds::new(
                        point(bounds.origin.x + px(14.), top + px(9.5)),
                        size(px(18.), px(18.)),
                    ),
                    key,
                    Some(data),
                    gpui::TransformationMatrix::unit(),
                    if picked { p.accent } else { p.muted_frame },
                    cx,
                );
                if let Some(label) = &row.label {
                    let line = label.run.line();
                    let baseline = top + (px(37.) - line.ascent - line.descent) / 2. + line.ascent;
                    let _ = window.paint_glyph_run(
                        point(bounds.origin.x + px(41.), baseline),
                        &label.run,
                        if picked { p.accent } else { p.foreground },
                    );
                }
            }
        });
    }
}
