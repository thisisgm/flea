//! An animated GIF's moving layer, fitted inside the frame's border; the still is fitted::fitted.
use super::{Frame, Kind, SelectionPreview};
use flea::ui_logic_pv1::{pixels::is_vector_name, thumbs};
use gpui::{
    canvas, div, prelude::*, px, AnyElement, Context, Corners, IntoElement, RenderImage, Task,
};
use std::{sync::Arc, time::Duration};

use super::render::FRAME_BORDER;

/// A GIF frame delay under 20 ms plays at 100 ms, the rule browsers and Qt's movie reader follow.
const SHORT_DELAY_MS: u64 = 20;
const DEFAULT_DELAY_MS: u64 = 100;

/// The preview's own clock for the drawn GIF: gpui's img only advances while its window holds keyboard focus, and v0.3.12 plays regardless.
pub(crate) struct GifClock {
    generation: u64,
    start: u64,
    _wake: Task<()>,
}

/// The frame shown `elapsed` ms into a looping animation, and the ms until the next one.
pub(crate) fn frame_at(delays: &[u64], elapsed: u64) -> (usize, u64) {
    let total: u64 = delays.iter().sum();
    if total == 0 {
        return (0, DEFAULT_DELAY_MS);
    }
    let mut left = elapsed % total;
    for (at, &delay) in delays.iter().enumerate() {
        if left < delay {
            return (at, delay - left);
        }
        left -= delay;
    }
    (0, delays[0])
}

impl SelectionPreview {
    /// The drawn GIF's frame for this render, waking the next render when that frame's delay runs out.
    pub(crate) fn gif_frame(&mut self, cx: &mut Context<Self>) -> usize {
        let Some((generation, image)) = self.drawn.as_ref().and_then(|frame| {
            let image = frame
                .animation
                .as_ref()
                .filter(|image| image.frame_count() > 1)?;
            Some((frame.generation, Arc::clone(image)))
        }) else {
            self.gif = None;
            return 0;
        };
        let now = self.now(cx);
        let start = match &self.gif {
            Some(clock) if clock.generation == generation => clock.start,
            _ => now,
        };
        let delays: Vec<u64> = (0..image.frame_count())
            .map(
                |at| match Duration::from(image.delay(at)).as_millis() as u64 {
                    short if short < SHORT_DELAY_MS => DEFAULT_DELAY_MS,
                    delay => delay,
                },
            )
            .collect();
        let (shown, wait) = frame_at(&delays, now.saturating_sub(start));
        let wake = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(wait))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        self.gif = Some(GifClock {
            generation,
            start,
            _wake: wake,
        });
        shown
    }
}

fn fitted(
    image: Arc<RenderImage>,
    at: usize,
    frame: &Frame,
    (width, height): (f32, f32),
    device: f32,
    selector: &'static str,
    painted: std::rc::Rc<std::cell::RefCell<super::frame_feedback::Frames>>,
) -> AnyElement {
    let generation = frame.generation;
    let size = image.size(0);
    let (w, h) = (size.width.0 as f64, size.height.0 as f64);
    let original = frame
        .facts
        .as_ref()
        .map_or((0., 0.), |m| (m.w as f64, m.h as f64));
    let limit = thumbs::preview_limit(
        frame.kind == Kind::Video,
        is_vector_name(&frame.subject.row.name),
        w,
        h,
        original.0,
        original.1,
    ) / f64::from(device);
    let fit = thumbs::fit_scale(
        (width - 2. * FRAME_BORDER).max(1.) as f64,
        (height - 2. * FRAME_BORDER).max(1.) as f64,
        w,
        h,
        limit,
    );
    let scale = fit as f32;
    let (drawn_w, drawn_h) =
        match crate::preview_quality::fitted_pixels(w, h, fit, limit, device.into()) {
            Some(own) => crate::preview_quality::own_size(own, device.into()),
            None => ((w as f32 * scale).round(), (h as f32 * scale).round()),
        };
    div()
        .debug_selector(move || selector.into())
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .debug_selector(move || format!("{selector}-image-{at}"))
                .w(px(drawn_w))
                .h(px(drawn_h))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let result = window.paint_image(
                                bounds,
                                bounds,
                                Corners::default(),
                                image,
                                at,
                                false,
                            );
                            painted.borrow_mut().animation =
                                Some(super::frame_feedback::AnimationPaint {
                                    generation,
                                    index: at,
                                    draw: window.current_draw_id(),
                                    error: result.err().map(|error| error.to_string()),
                                });
                        },
                    )
                    .size_full(),
                ),
        )
        .into_any_element()
}

/// An animated GIF's moving layer over its still, ready only with more than one frame of its own.
pub(crate) fn moving(
    frame: &Frame,
    at: usize,
    width: f32,
    height: f32,
    device: f32,
    painted: std::rc::Rc<std::cell::RefCell<super::frame_feedback::Frames>>,
) -> Option<AnyElement> {
    frame.animation.clone().and_then(|image| {
        (image.frame_count() > 1).then(|| {
            fitted(
                image,
                at,
                frame,
                (width, height),
                device,
                "column-preview-animation",
                painted,
            )
        })
    })
}
