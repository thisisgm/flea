//! The blocks that draw a picture: a display figure, one image, a row of images and the remote box.

use super::{fence_base, grounded, one_line, plain_look, Ctx, FigureState, Geometry, Item};
use crate::ui_logic_pv2::markdown::markdown_figure::{centred, fallback_body, fit};
use crate::ui_logic_pv2::markdown::markdown_flow::{elide, Face, Ink, Look};
use crate::ui_logic_pv2::markdown::markdown_html_image::picture_size;
use crate::ui_logic_pv2::markdown::markdown_model::{Align, Figure, FigureKind, Image};

/// A display figure: nothing while it is asked, its picture fitted to the width once drawn, its bounded source on the code surface once failed.
pub(super) fn figure_block(figure: &Figure, width: f32, ctx: &mut Ctx) -> Geometry {
    let ordinal = ctx.counter.figure;
    ctx.counter.figure += 1;
    match ctx.assets.figure(ordinal) {
        FigureState::Waiting => Geometry {
            height: 0.0,
            items: vec![Item::Figure {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                ordinal,
            }],
            first_baseline: None,
        },
        FigureState::Drawn {
            width: natural_w,
            height: natural_h,
        } => {
            let (w, h) = fit(f64::from(natural_w), f64::from(natural_h), f64::from(width));
            let (w, h) = (w as f32, h as f32);
            let inset = if figure.kind == FigureKind::Math && h > 0.0 {
                ctx.metrics.gap
            } else {
                0.0
            };
            let x = if centred(figure.kind, true, false) {
                ((width - w) / 2.0).round()
            } else {
                0.0
            };
            Geometry {
                height: h + 2.0 * inset,
                items: vec![Item::Figure {
                    x,
                    y: inset,
                    width: w,
                    height: h,
                    ordinal,
                }],
                first_baseline: None,
            }
        }
        FigureState::Failed => {
            let base = fence_base(&ctx.metrics);
            let lines: Vec<Vec<(Look, String)>> = fallback_body(&figure.source)
                .split('\n')
                .map(|line| vec![(plain_look(&base, Ink::Foreground), line.to_string())])
                .collect();
            grounded(&lines, None, width, ctx)
        }
    }
}

fn sized(
    image_w: Option<u32>,
    image_h: Option<u32>,
    url: &str,
    width: f32,
    ctx: &Ctx,
) -> (f32, f32, bool) {
    let natural = ctx.assets.picture(url).unwrap_or((0.0, 0.0));
    let given = |v: Option<u32>| v.map_or(0.0, f64::from);
    let (w, h, stretch) = picture_size(
        given(image_w),
        given(image_h),
        f64::from(natural.0),
        f64::from(natural.1),
        f64::from(width),
    );
    (w as f32, h as f32, stretch)
}

/// A lone local picture at its fitted size, centred when its paragraph is.
pub(super) fn image_block(image: &Image, width: f32, ctx: &mut Ctx) -> Geometry {
    let (w, h, stretch) = sized(image.width, image.height, &image.url, width, ctx);
    let x = if image.align == Some(Align::Center) {
        ((width - w) / 2.0).round()
    } else {
        0.0
    };
    Geometry {
        height: h,
        items: vec![Item::Picture {
            x,
            y: 0.0,
            width: w,
            height: h,
            url: image.url.clone(),
            link: image.link.clone(),
            stretch,
        }],
        first_baseline: None,
    }
}

/// A row of badges wrapped at the width, the list's gap between neighbours and lines, each line centred or left; a picture rests on its line's bottom.
pub(super) fn images(items: &[Image], align: Option<Align>, width: f32, ctx: &mut Ctx) -> Geometry {
    let gap = ctx.metrics.block_gap();
    let sizes: Vec<(f32, f32, bool)> = items
        .iter()
        .map(|image| sized(image.width, image.height, &image.url, width, ctx))
        .collect();
    let mut out = Geometry::default();
    let mut y = 0.0;
    let mut from = 0;
    while from < items.len() {
        let mut to = from;
        let mut used = 0.0;
        let mut tall: f32 = 0.0;
        while to < items.len() {
            let w = sizes[to].0;
            if to > from && used + gap + w > width {
                break;
            }
            used += if to > from { gap } else { 0.0 } + w;
            tall = tall.max(sizes[to].1);
            to += 1;
        }
        let mut x = if align == Some(Align::Center) {
            ((width - used) / 2.0).round()
        } else {
            0.0
        };
        for (image, &(w, h, stretch)) in items[from..to].iter().zip(&sizes[from..to]) {
            out.items.push(Item::Picture {
                x,
                y: y + tall - h,
                width: w,
                height: h,
                url: image.url.clone(),
                link: image.link.clone(),
                stretch,
            });
            x += w + gap;
        }
        y += tall + gap;
        from = to;
    }
    out.height = (y - gap).max(0.0);
    out
}

/// The refused remote picture: a dashed box across the width around the picture glyph and the sentence naming its host.
pub(super) fn remote(host: &str, width: f32, ctx: &mut Ctx) -> Geometry {
    let metrics = ctx.metrics;
    let (ascent, descent) = ctx.measure.extent(Face::Regular, metrics.caption);
    let line = (ascent + descent).ceil();
    let height = metrics.mark.max(line) + 2.0 * metrics.gap;
    let text_x = metrics.gap + metrics.mark + metrics.gap;
    let sentence = ctx.remote.replace("%1", host);
    let shown = elide(
        &sentence,
        (width - text_x - metrics.gap).max(0.0),
        Face::Regular,
        metrics.caption,
        ctx.measure,
    );
    let mut base = metrics.base(metrics.caption, Face::Regular, Ink::Muted, None);
    base.line = line;
    let laid = one_line(&shown, &base, ctx);
    Geometry {
        height,
        items: vec![
            Item::Dashed {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            Item::Glyph {
                x: metrics.gap,
                y: ((height - metrics.mark) / 2.0).round(),
                size: metrics.mark,
            },
            Item::Text {
                x: text_x,
                y: ((height - line) / 2.0).round(),
                laid,
            },
        ],
        first_baseline: None,
    }
}
