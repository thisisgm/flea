//! Short windows scroll only the form body; title and buttons retain their positions.
use super::drawing::{Drawn, Rect};

fn intersect(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect {
        x,
        y,
        w: (a.x + a.w).min(b.x + b.w).max(x) - x,
        h: (a.y + a.h).min(b.y + b.h).max(y) - y,
    }
}
fn horizontal(mut rect: Rect, width: f32) -> Rect {
    let full = width - 34.;
    let half = (full - 12.) / 2.;
    if rect.x >= 255.5 {
        rect.x += 17. + half + 12. - 255.5;
    }
    for (old, new) in [
        (465., full),
        (445., full - 20.),
        (226.5, half),
        (206.5, half - 20.),
    ] {
        if rect.w == old {
            rect.w = new.max(0.);
            break;
        }
    }
    rect
}
impl Drawn {
    /// The body's clip as painted, in card coordinates: between the pinned title and the buttons.
    pub fn body(&self) -> Option<Rect> {
        self.card.map(|card| Rect {
            x: 17.,
            y: 61.,
            w: (card.w - 34.).max(0.),
            h: (self.body_height - 108.).max(0.),
        })
    }
    pub(super) fn fit_body(
        &mut self,
        start: (usize, usize, usize),
        scroll: f32,
        width: f32,
        height: f32,
    ) {
        let clip = Rect {
            x: 17.,
            y: 61.,
            w: (width - 34.).max(0.),
            h: (height - 108.).max(0.),
        };
        for text in &mut self.text[start.0..] {
            text.rect = horizontal(text.rect, width);
            text.rect.y -= scroll;
            let own = text.clip.map(|r| {
                let mut r = horizontal(r, width);
                r.y -= scroll;
                r
            });
            text.clip = Some(own.map_or(clip, |r| intersect(r, clip)));
        }
        for paint in &mut self.boxes[start.1..] {
            paint.rect = horizontal(paint.rect, width);
            paint.rect.y -= scroll;
            paint.clip = Some(clip);
        }
        for (_, rect) in &mut self.hits[start.2..] {
            *rect = horizontal(*rect, width);
            rect.y -= scroll;
            *rect = intersect(*rect, clip);
        }
    }
}
