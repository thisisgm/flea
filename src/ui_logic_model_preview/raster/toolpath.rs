//! The toolpath pass: each extrusion a line, the view's layer whole and the layers under it stepped to share the budget.
use super::{Raster, Style, EXTRUSION_MM, TOOLPATH_SEGMENTS};
use crate::ui_logic_model_preview::camera::{Basis, View};
use crate::ui_logic_model_preview::color::{mix, tone_index, Ink, Rgb};
use crate::ui_logic_model_preview::scene::Scene;
use crate::ui_logic_model_preview::vec::{cross, dot, norm};
use std::ops::Range;

// The stride that keeps `len` items within `budget`; a stride of one draws them all.
fn stride(len: usize, budget: usize) -> usize {
    if len <= budget {
        1
    } else {
        len.div_ceil(budget)
    }
}

impl Raster {
    // The layer at the view takes half the budget; the layers under it share what is left, each stepped by one stride.
    pub(super) fn toolpath(&mut self, scene: &Scene, view: &View, basis: &Basis, style: &Style) {
        if scene.segments.is_empty() {
            return;
        }
        let palette = style.palette;
        let lit = Ink::from_lit(palette.foreground);
        let dim = lit.dimmed(style.ground.unwrap_or(palette.ground));
        // The dimmed tone of each tenth, the one `dim.tone` mixes for a light at that step.
        let tones: [Rgb; 11] = std::array::from_fn(|k| mix(dim.shade, dim.lit, k as f32 / 10.));
        let width = (EXTRUSION_MM * view.scale).max(self.ss as f32);
        let light = view.light();
        let current = style.layer.unwrap_or(usize::MAX);
        let draw = |raster: &mut Self, range: Range<usize>, flat: bool, step: usize| {
            for s in scene.segments[range].iter().step_by(step) {
                if !s.extrude {
                    continue;
                }
                raster.stats.segments += 1;
                let color = if flat {
                    lit.lit
                } else {
                    let along = norm([s.b[0] - s.a[0], s.b[1] - s.a[1], 0.]);
                    let mut n = cross(along, [0., 0., 1.]);
                    if dot(n, basis.d) < 0. {
                        n = [-n[0], -n[1], -n[2]];
                    }
                    tone_index(dot(n, light)).map_or([f32::NAN; 3], |step| tones[step])
                };
                raster.line(
                    view.project(basis, s.a),
                    view.project(basis, s.b),
                    width,
                    color,
                    1.,
                    true,
                );
            }
        };
        if scene.layers.is_empty() {
            draw(
                self,
                0..scene.segments.len(),
                false,
                stride(scene.segments.len(), TOOLPATH_SEGMENTS),
            );
            return;
        }
        let clip = |r: &Range<u32>| {
            (r.start as usize).min(scene.segments.len())..(r.end as usize).min(scene.segments.len())
        };
        let shown = scene.layers.len().min(current.saturating_add(1));
        let len = |i: usize| clip(&scene.layers[i].segments).len();
        let here = if current < shown { len(current) } else { 0 };
        let under: usize = (0..shown).filter(|&i| i != current).map(len).sum();
        let here_stride = stride(here, TOOLPATH_SEGMENTS / 2);
        let under_stride = stride(under, TOOLPATH_SEGMENTS - here.div_ceil(here_stride));
        for (i, layer) in scene.layers.iter().enumerate().take(shown) {
            let (flat, step) = if i == current {
                (true, here_stride)
            } else {
                (false, under_stride)
            };
            draw(self, clip(&layer.segments), flat, step);
        }
    }
}
