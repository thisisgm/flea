//! The 3D viewport: advances the view in prepaint, has a worker rasterize each changed frame into pixels GPUI takes as they are, presents it when it lands, and turns pointer input into orbit, pan and zoom.
use super::{live::Live, seam};
use crate::list::Palette as Roles;
use gpui::{
    relative, size, App, Bounds, Corners, CursorStyle, DispatchPhase, Element, ElementId,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, RenderImage, ScrollDelta,
    ScrollWheelEvent, Style, Window,
};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};

/// The picture on screen, the ones waiting to leave the sprite atlas, and counts of both for the gates.
#[derive(Default)]
pub struct Presented {
    pub image: Option<Arc<RenderImage>>,
    retired: Vec<Arc<RenderImage>>,
    pub shown: usize,
    pub dropped: usize,
}

impl Presented {
    /// Takes the picture off screen; the next paint frees its atlas sprite.
    pub fn retire(&mut self) {
        if let Some(old) = self.image.take() {
            self.retired.push(old);
        }
    }

    /// Puts a landed frame on screen, retiring the one before it.
    pub fn present(&mut self, image: Arc<RenderImage>) {
        self.retire();
        self.image = Some(image);
        self.shown += 1;
    }

    pub fn retired(&self) -> usize {
        self.retired.len()
    }
}

pub struct Viewport {
    pub live: Rc<RefCell<Live>>,
    pub presented: Rc<RefCell<Presented>>,
    pub host: seam::Host,
    pub epoch: Instant,
}

fn seconds(epoch: Instant, cx: &App) -> f64 {
    cx.background_executor()
        .now()
        .saturating_duration_since(epoch)
        .as_secs_f64()
}

impl IntoElement for Viewport {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Viewport {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

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
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        let host = seam::Host {
            focused: self.host.focused && window.is_window_active(),
            ..self.host
        };
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = seam::palette(&Roles::inherited(&window.text_style(), theme), host.ground);
        let now = seconds(self.epoch, cx);
        let size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let mut live = self.live.borrow_mut();
        let frame = live.frame(now, &host, size, window.scale_factor(), palette);
        if let Some(job) = frame.job {
            let work = cx.background_executor().spawn(async move {
                let mut drawn = job.draw();
                let picture = drawn.picture();
                (drawn, picture)
            });
            let (live, presented) = (self.live.clone(), self.presented.clone());
            window
                .spawn(cx, async move |cx| {
                    let (drawn, image) = work.await;
                    let mut live = live.borrow_mut();
                    match (live.finish(drawn), image) {
                        (true, Some(image)) => presented.borrow_mut().present(image),
                        (false, Some(image)) => live.recycle(image),
                        (_, None) => {}
                    }
                    drop(live);
                    let _ = cx.update(|window, _| window.refresh());
                })
                .detach();
        }
        if frame.moving {
            live.counters.requests += 1;
            window.request_animation_frame();
        }
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        _: &mut App,
    ) {
        let mut presented = self.presented.borrow_mut();
        while let Some(old) = presented.retired.pop() {
            let _ = window.drop_image(old.clone());
            presented.dropped += 1;
            self.live.borrow_mut().recycle(old);
        }
        let live = self.live.borrow();
        if live.model.is_none() {
            return;
        }
        if let (Some(image), false) = (&presented.image, live.thumbnail) {
            let _ = window.paint_image(bounds, bounds, Corners::default(), image.clone(), 0, false);
        }
        let grip = if live.dragging.is_some() {
            CursorStyle::ClosedHand
        } else {
            CursorStyle::OpenHand
        };
        window.set_cursor_style(grip, hitbox);
        drop(live);
        listen(self, bounds, hitbox.clone(), window);
    }
}

// Pointer input: a left drag orbits, shift or middle drag pans, the wheel zooms at the pointer and a double-click frames again.
fn listen(viewport: &Viewport, bounds: Bounds<Pixels>, hitbox: Hitbox, window: &mut Window) {
    let (live, epoch) = (viewport.live.clone(), viewport.epoch);
    let local = move |p: gpui::Point<Pixels>| {
        (
            f32::from(p.x - bounds.origin.x),
            f32::from(p.y - bounds.origin.y),
        )
    };
    let (down, hit) = (live.clone(), hitbox.clone());
    window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || !hit.is_hovered(window) {
            return;
        }
        let pan = match e.button {
            MouseButton::Left => e.modifiers.shift,
            MouseButton::Middle => true,
            _ => return,
        };
        let now = seconds(epoch, cx);
        let mut live = down.borrow_mut();
        live.thumbnail = false;
        let (x, y) = local(e.position);
        let Some(viewer) = live.viewer.as_mut() else {
            return;
        };
        if e.click_count == 2 && !pan {
            viewer.double_click(now);
        } else {
            viewer.press(now);
            live.dragging = Some((x, y, pan));
        }
        window.refresh();
    });
    let moved = live.clone();
    window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        let mut live = moved.borrow_mut();
        let Some((x0, y0, pan)) = live.dragging else {
            return;
        };
        let (x, y) = local(e.position);
        live.dragging = Some((x, y, pan));
        if let Some(viewer) = live.viewer.as_mut() {
            viewer.drag(x - x0, y - y0, pan, seconds(epoch, cx));
        }
        window.refresh();
    });
    let up = live.clone();
    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        let mut live = up.borrow_mut();
        let Some((_, _, pan)) = live.dragging.take() else {
            return;
        };
        if let Some(viewer) = live.viewer.as_mut() {
            viewer.release(pan, seconds(epoch, cx));
        }
        window.refresh();
    });
    window.on_mouse_event(move |e: &ScrollWheelEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
            return;
        }
        let (delta, lines) = match e.delta {
            ScrollDelta::Pixels(p) => (-f32::from(p.y), false),
            ScrollDelta::Lines(l) => (-l.y, true),
        };
        let (x, y) = local(e.position);
        let mut live = live.borrow_mut();
        live.thumbnail = false;
        if let Some(viewer) = live.viewer.as_mut() {
            viewer.wheel(x, y, delta, lines, seconds(epoch, cx));
            window.refresh();
        }
    });
}
