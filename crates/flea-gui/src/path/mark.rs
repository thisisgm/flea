//! The toolbar's drawn marks: the SVG glyph, the focused ink and the press tween.
use super::chrome::Frame;
use gpui::{canvas, div, prelude::*, px, Div};
use std::{cell::RefCell, rc::Rc, time::Instant};

/// ui/js/Buttons.js:21 and :22: a pressed chrome mark scales to 0.96 over 150 ms, OutCubic (ui/ChromeButton.qml:50-59).
const PRESS_SCALE: f32 = 0.96;
const PRESS_MS: f32 = 150.;

/// The width paint_svg draws in device pixels: gpui's own edge snap, then the transform's scale.
#[cfg(test)]
fn drawn_width(window: &gpui::Window, bounds: gpui::Bounds<gpui::Pixels>, scale: f32) -> f32 {
    f32::from(window.pixel_snap(bounds.right()) - window.pixel_snap(bounds.left()))
        * window.scale_factor()
        * scale
}

/// ui/ChromeButton.qml:85-87: a focused, live mark lifts to the foreground, the same ink as its ring.
pub fn mark_ink(
    enabled: bool,
    focused: bool,
    resting: gpui::Hsla,
    foreground: gpui::Hsla,
) -> gpui::Hsla {
    if enabled && focused {
        foreground
    } else {
        resting
    }
}

/// ChromeModes.qml and ChromeButton.qml: current foreground, other modes muted frame, unavailable at 0.55.
pub fn resting_ink(enabled: bool, current: bool, palette: &crate::list::Palette) -> gpui::Hsla {
    if enabled {
        palette.mark(current)
    } else {
        palette.mark(false).opacity(0.55)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Press {
    pub down: bool,
    pub since: Instant,
    pub from: f32,
}

/// The mark's scale and whether its bounded, elapsed-time tween still needs frames.
pub fn press_scale(press: Option<Press>, now: Instant, reduced: bool) -> (f32, bool) {
    if reduced {
        return (1., false);
    }
    let Some(Press { down, since, from }) = press else {
        return (1., false);
    };
    let target = if down { PRESS_SCALE } else { 1. };
    let t = (now.saturating_duration_since(since).as_secs_f32() * 1000. / PRESS_MS).min(1.);
    let eased = 1. - (1. - t).powi(3);
    (from + (target - from) * eased, from != target && t < 1.)
}

/// ui/ChromeModes.qml: a hairline one pixel shorter than the mark.
pub fn divider(ink: gpui::Hsla, mark: f32) -> Div {
    div()
        .w(px(1.))
        .h(px(mark - 1.))
        .flex_none()
        .bg(ink.opacity(0.12))
}

/// Theme.chromeMarkSize at text size 14.
pub const MARK: f32 = 16.;

pub fn mark(name: &str, ink: gpui::Hsla, frame: Rc<RefCell<Frame>>, index: usize) -> Div {
    mark_sized(name, MARK, ink, frame, index)
}

pub fn mark_sized(
    name: &str,
    side: f32,
    ink: gpui::Hsla,
    frame: Rc<RefCell<Frame>>,
    index: usize,
) -> Div {
    let own = match name {
        "arrow-left" => Some(("flea-chrome:arrow-left".into(), br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M19 12H5 M12 19l-7-7 7-7"/></svg>"#.as_slice())),
        "filter" => Some(("flea-chrome:filter".into(), br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M3 4h18l-7 8v7l-4 2v-9z"/></svg>"#.as_slice())),
        _ => crate::g2::st1::glyphs::svg(name),
    };
    let slot = div().size(px(side)).flex_none();
    let Some((key, data)) = own else { return slot };
    let back = name == "arrow-left";
    slot.child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let (scale, moving) = press_scale(
                    frame.borrow().controls[index].press,
                    cx.background_executor().now(),
                    cx.reduce_motion(),
                );
                if moving {
                    window.request_animation_frame();
                }
                // paint_svg snaps bounds to whole device pixels, so the press scales the sprite instead (EY-486).
                let factor = window.scale_factor();
                let centre = bounds.center();
                let transform = gpui::TransformationMatrix::unit()
                    .translate(centre.scale(factor))
                    .scale(gpui::size(scale, scale))
                    .translate(centre.scale(-factor));
                #[cfg(test)]
                {
                    let size = gpui::size(bounds.size.width * scale, bounds.size.height * scale);
                    let mut frame = frame.borrow_mut();
                    frame.controls[index].mark_bounds = Some(gpui::Bounds::new(
                        centre - gpui::point(size.width / 2., size.height / 2.),
                        size,
                    ));
                    frame.controls[index].mark_ink = Some(ink);
                    frame.controls[index].mark_drawn =
                        Some(drawn_width(window, bounds, transform.rotation_scale[0][0]));
                }
                let (key, data) = match back && window.layout_mirrored() {
                    true => (
                        crate::locale_geometry::ARROW_RIGHT.0.into(),
                        crate::locale_geometry::ARROW_RIGHT.1,
                    ),
                    false => (key.clone(), data),
                };
                let _ = window.paint_svg(bounds, key, Some(data), transform, ink, cx);
            },
        )
        .size_full(),
    )
}
