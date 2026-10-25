//! Scene receipts observe the fitted, shaped pieces the native path Element draws.
use super::{paint::strip_pad, view::PathView};
use crate::visual::Probe;
use gpui::{canvas, point, prelude::*, px, size, Bounds, Div, Entity, Pixels, Window};

pub(super) fn wrap(div: Div, path: Entity<PathView>, slot: usize, probe: Option<Probe>) -> Div {
    let Some(probe) = probe else { return div };
    div.child(
        canvas(
            move |_, window, cx| {
                path.read_with(cx, |path, _| record(path, slot, &probe, window));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full(),
    )
}

fn record(path: &PathView, slot: usize, probe: &Probe, window: &Window) {
    if slot == 0 && (path.editing() || path.inputs.dual) {
        return;
    }
    let paint = path.paint.borrow();
    let Some(bounds) = paint.strips[slot] else {
        return;
    };
    let parent = if slot == 0 {
        "chrome".into()
    } else {
        format!("path-{}", slot - 1)
    };
    let id = if slot == 0 {
        "chrome-path".into()
    } else {
        format!("{parent}-text")
    };
    let pad = strip_pad(slot != 0, &paint.scale());
    let height = bounds.size.height - px(if slot == 0 { 0. } else { 1. });
    let strip = &path.strips[slot];
    let mut text = String::new();
    let mut ink: Option<Bounds<Pixels>> = None;
    for (index, (crumb, run)) in strip.crumbs().iter().zip(&paint.runs[slot]).enumerate() {
        let line = run.line();
        let x = bounds.origin.x + px(pad + strip.extent(index).map_or(0., |(left, _)| left));
        let baseline = window
            .pixel_snap(bounds.origin.y + (height - line.ascent - line.descent) / 2. + line.ascent);
        let rect = Bounds::new(
            point(x, baseline - line.ascent),
            size(line.width, line.ascent + line.descent),
        )
        .intersect(&bounds);
        if rect.size.width <= px(0.) || rect.size.height <= px(0.) {
            continue;
        }
        probe.push(
            &format!("{id}-crumb-{index}"),
            &id,
            "label",
            rect,
            &crumb.text,
            window,
        );
        text.push_str(&crumb.text);
        ink = Some(match ink {
            None => rect,
            Some(kept) => {
                let origin = point(kept.left().min(rect.left()), kept.top().min(rect.top()));
                Bounds::new(
                    origin,
                    size(
                        kept.right().max(rect.right()) - origin.x,
                        kept.bottom().max(rect.bottom()) - origin.y,
                    ),
                )
            }
        });
    }
    if let Some(ink) = ink {
        probe.push(&id, &parent, "label", ink, &text, window);
    }
}
