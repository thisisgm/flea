//! The GPUI rail host: draws the places producer's view where the rail stands, feeds the pointer to Rail and arms its one timer.
use super::{geometry::EDGE, Handoff, Mode, Rail};
use gpui::{
    canvas, div, prelude::*, px, AnyView, App, Bounds, Context, EventEmitter, HoverListenerMode,
    Pixels, Task, Window,
};
use std::{cell::Cell, rc::Rc, time::Instant};

/// What the window's focus owner and the places producer hear from the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Focus(Handoff),
    Polling(bool),
}

/// The places producer's seam: builds its rows view, called once per life of the rows, never for a withdrawn rail.
pub type Build = Box<dyn Fn(&mut Window, &mut App) -> AnyView>;

pub struct Host {
    pub rail: Rail,
    width: f32,
    build: Build,
    content: Option<AnyView>,
    timer: Option<Task<()>>,
    polling: bool,
    /// The overlay body's window bounds as this frame laid it out, none while no overlay is drawn.
    cover: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Rows views the producer was asked to build.
    pub constructed: u32,
}

impl EventEmitter<Event> for Host {}

impl Host {
    pub fn new(mode: Mode, width: f32, build: Build, cx: &mut Context<Self>) -> Self {
        let rail = Rail::new(mode, cx.reduce_motion(), now(cx));
        let polling = rail.polling();
        Self {
            rail,
            width,
            build,
            content: None,
            timer: None,
            polling,
            cover: Rc::default(),
            constructed: 0,
        }
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.width = width;
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if let Some(handoff) = self.rail.set_mode(mode, now(cx)) {
            cx.emit(Event::Focus(handoff));
        }
        self.after(cx);
    }

    pub fn focus(&mut self, held: bool, cx: &mut Context<Self>) {
        self.rail.focus(held, now(cx));
        self.after(cx);
    }

    pub fn menu(&mut self, open: bool, cx: &mut Context<Self>) {
        self.rail.menu(open, now(cx));
        self.after(cx);
    }

    pub fn drag(&mut self, over: bool, cx: &mut Context<Self>) {
        self.rail.drag(over, now(cx));
        self.after(cx);
    }

    pub fn releasing(&mut self, active: bool, cx: &mut Context<Self>) {
        self.rail.releasing(active, now(cx));
        self.after(cx);
    }

    pub fn pointer_edge(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.rail.edge(hovered, now(cx));
        self.after(cx);
    }

    pub fn pointer_over(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.rail.over(hovered, now(cx));
        self.after(cx);
    }

    pub fn timer_armed(&self) -> bool {
        self.timer.is_some()
    }

    pub fn content(&self) -> Option<&AnyView> {
        self.content.as_ref()
    }

    /// Where the overlay rail covers the panes in the frame last laid out, so what paints over the shell can stay under it.
    pub fn cover(&self) -> Option<Bounds<Pixels>> {
        self.cover.get()
    }

    // Rows that are no longer alive go with their producer state; the poll gate follows; one timer covers the next due change.
    fn after(&mut self, cx: &mut Context<Self>) {
        let at = now(cx);
        if !self.rail.drawn(at) && self.rail.wants().over {
            self.rail.over(false, at);
        }
        if !self.rail.live() {
            self.content = None;
        }
        let polling = self.rail.polling();
        if polling != self.polling {
            self.polling = polling;
            cx.emit(Event::Polling(polling));
        }
        self.timer = self.rail.deadline(at).map(|delay| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update(cx, |host, cx| {
                    host.rail.tick(now(cx));
                    host.after(cx);
                });
            })
        });
        cx.notify();
    }
}

fn now(cx: &App) -> Instant {
    cx.background_executor().now()
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.rail.set_reduced(cx.reduce_motion());
        let at = now(cx);
        if self.rail.live() && self.content.is_none() {
            self.content = Some((self.build)(window, cx));
            self.constructed += 1;
        }
        let overlay = self.rail.mode() == Mode::Overlay;
        self.cover.set(None);
        let mut root = div().size_full().relative();
        if overlay {
            // No press handler: a click on the listing under the strip still lands on the listing.
            root = root.child(
                div()
                    .id("rail-edge")
                    .absolute()
                    .left_0()
                    .top_0()
                    .h_full()
                    .w(px(EDGE))
                    // A key press never unhovers a resting pointer, as a HoverHandler does not.
                    .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
                    .on_hover(cx.listener(|host, hovered: &bool, _, cx| {
                        host.pointer_edge(*hovered, cx);
                    })),
            );
        }
        if let Some(view) = self.content.clone().filter(|_| self.rail.drawn(at)) {
            let x = if overlay {
                (self.rail.slide(at) - 1.) * self.width
            } else {
                0.
            };
            let mut body = div()
                .id("rail-body")
                .absolute()
                .left(px(x))
                .top_0()
                .h_full()
                .w(px(self.width))
                .occlude()
                .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
                .on_hover(cx.listener(|host, hovered: &bool, _, cx| {
                    host.pointer_over(*hovered, cx);
                }))
                .child(view);
            if overlay {
                // Captured before the rows, so a row that stops its press still lets an overlay rail claim the keyboard.
                body = body.capture_any_mouse_down(cx.listener(|_, _, _, cx| {
                    cx.emit(Event::Focus(Handoff::Rail));
                }));
                let cover = Rc::clone(&self.cover);
                body = body.child(
                    canvas(move |bounds, _, _| cover.set(Some(bounds)), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                );
            }
            root = root.child(body);
        }
        if self.rail.animating(at) {
            window.request_animation_frame();
        }
        root
    }
}
