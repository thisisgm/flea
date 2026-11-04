//! Ground observations follow GPUI element-state retention, including cached scene replay.
use super::{producers::Producer, Windows};
use crate::palette::Theme;
use gpui::{App, Bounds, ElementId, Hsla, Pixels, Window};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub(super) struct Sample {
    owner: Producer,
    theme: Theme,
    draw: u64,
    alpha: u8,
}

pub(super) type Samples = Vec<Weak<RefCell<Sample>>>;

pub(super) fn record(
    owner: Producer,
    bounds: Bounds<Pixels>,
    color: Hsla,
    theme: Theme,
    window: &mut Window,
    cx: &App,
) {
    let Some(draw) = window.current_draw_id() else {
        return;
    };
    let alpha = (color.a * 255.).round() as u8;
    let id = ElementId::Name(
        format!(
            "flea-ground-{:?}-{}-{bounds:?}",
            window.current_view(),
            owner as usize
        )
        .into(),
    );
    let sample = window.with_global_id(id, |id, window| {
        window.with_element_state(id, |sample: Option<Rc<RefCell<Sample>>>, _| {
            let sample = sample.unwrap_or_else(|| {
                Rc::new(RefCell::new(Sample {
                    owner,
                    theme,
                    draw,
                    alpha,
                }))
            });
            {
                let mut value = sample.borrow_mut();
                value.alpha = if value.draw == draw {
                    value.alpha.max(alpha)
                } else {
                    alpha
                };
                value.theme = theme;
                value.draw = draw;
            }
            (Rc::clone(&sample), sample)
        })
    });
    if let Some(book) = cx.try_global::<Windows>() {
        if let Some(mounted) = book
            .0
            .borrow_mut()
            .get_mut(&window.window_handle().window_id())
        {
            mounted.samples.retain(|entry| entry.strong_count() > 0);
            let weak = Rc::downgrade(&sample);
            if !mounted.samples.iter().any(|entry| entry.ptr_eq(&weak)) {
                mounted.samples.push(weak);
            }
        }
    }
}

pub(super) fn observed(window: &Window, cx: &App) -> ([Option<u8>; 9], bool) {
    let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
    let mut painted: [Option<u8>; 9] = [None; 9];
    let mut fresh = true;
    if let Some(book) = cx.try_global::<Windows>() {
        if let Some(mounted) = book
            .0
            .borrow_mut()
            .get_mut(&window.window_handle().window_id())
        {
            mounted.samples.retain(|entry| entry.strong_count() > 0);
            for sample in mounted.samples.iter().filter_map(Weak::upgrade) {
                let sample = sample.borrow();
                fresh &= sample.theme == theme;
                let entry = &mut painted[sample.owner as usize];
                *entry = Some(entry.map_or(sample.alpha, |previous| previous.max(sample.alpha)));
            }
        }
    }
    let ready = fresh
        && [
            Producer::Chrome,
            Producer::Status,
            Producer::Listing,
            Producer::Body,
        ]
        .iter()
        .all(|&owner| painted[owner as usize].is_some());
    (painted, ready)
}
