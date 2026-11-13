//! A depth-buffered software rasterizer for the 3D view, the rendered thumbnail and the terminal preview alike; faces are flat-shaded in 10% tone steps, lines are coverage anti-aliased, and every buffer is reused frame to frame.
use super::camera::View;
use super::color::{from_bytes, mix, tone_index, Ink, Palette, Rgb};
use pixels::{pack, unpack};
use super::scene::{Edge, PlateRect, Scene, CELL_MM};
use super::vec::{dot, V3};
use std::collections::HashMap;

#[cfg(test)]
mod perf_tests;
mod pixels;
mod toolpath;

// The plate's hairlines and edge, drawn over the model at these strengths when the view looks up from under the plate.
const GRID_ALPHA: f32 = 0.4;
const GRID_UNDER: f32 = 0.18;
const EDGE_UNDER: f32 = 0.5;
// Wireframe: seen edges in the foreground at 0.9, hidden ones faint.
const WIRE_SEEN: f32 = 0.9;
const WIRE_HIDDEN: f32 = 0.14;
// A G-code extrusion line is this wide in millimetres, and never thinner than a pixel.
const EXTRUSION_MM: f32 = 0.45;
// Lines lying on a face win the depth test against it by this many millimetres.
const LINE_BIAS: f32 = 0.05;
/// A toolpath frame draws at most this many extrusions (the current layer takes half); the layers under a view are stepped to fit.
pub const TOOLPATH_SEGMENTS: usize = 10_000;
/// A file's first 4096 colours draw exactly; later ones share 4096 bins of 16 levels a channel, so the ink table stays bounded.
pub const EXACT_INKS: usize = 4096;
// A frame keeps at most this many line colours checked for full-cover writes; past them a line blends every sample.
const SOLIDS: usize = 16;
// Plate hairlines closer than this many pixels thin to every second, third or further cell, and never pass this many a side.
const GRID_MIN_PX: f32 = 2.;
const GRID_MAX_LINES: f32 = 1024.;

/// Per-scene shading data, built once off the UI thread; the role ink follows each frame's palette, a file's own colours never change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prepared {
    pub normals: Vec<V3>,
    /// The ink each triangle draws with; 0 is the foreground role.
    pub ink_of: Vec<u32>,
    /// The role ink for the palette it was built with, then one ink per colour or bin.
    pub inks: Vec<Ink>,
    /// Each ink's packed tone steps, so a face looks its sample up rather than mixing and packing it.
    pub tones: Vec<[u32; 11]>,
}

impl Prepared {
    pub fn new(scene: &Scene, palette: Palette) -> Self {
        let normals = (0..scene.triangles.len()).map(|t| scene.normal(t)).collect();
        let mut inks = vec![Ink::from_lit(palette.foreground)];
        let mut index: HashMap<[u8; 3], u32> = HashMap::new();
        let ink_of = (0..scene.triangles.len())
            .map(|t| match scene.colors.get(t).copied().flatten() {
                Some(c) => {
                    let key = if index.contains_key(&c) || index.len() < EXACT_INKS { c } else { c.map(|v| (v & 0xf0) | 0x08) };
                    *index.entry(key).or_insert_with(|| {
                        inks.push(Ink::from_lit(from_bytes(key)));
                        (inks.len() - 1) as u32
                    })
                }
                None => 0,
            })
            .collect();
        let tones = inks.iter().map(tone_table).collect();
        Self { normals, ink_of, inks, tones }
    }
}

// The packed sample of each tenth from `ink`'s shade to its lit end, the one `pack(ink.tone(light), 1.)` gives for a light at that step.
fn tone_table(ink: &Ink) -> [u32; 11] {
    std::array::from_fn(|k| pack(mix(ink.shade, ink.lit, k as f32 / 10.), 1.))
}

/// What a frame draws besides the faces.
#[derive(Clone, Copy, Debug)]
pub struct Style<'a> {
    pub palette: Palette,
    /// Feature edges and the silhouette instead of faces; `None` draws faces.
    pub wire: Option<&'a [Edge]>,
    /// The G-code layer the view stands at; layers above it are hidden.
    pub layer: Option<usize>,
    /// The build plate, absent for a thumbnail.
    pub plate: Option<PlateRect>,
    /// The view's ground; `None` leaves it transparent, as a thumbnail is.
    pub ground: Option<Rgb>,
}

/// What the last frame cost: faces filled, samples in its buffers and buffers that had to grow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub triangles: usize,
    /// Extrusions drawn, the toolpath's share of the budget `TOOLPATH_SEGMENTS`.
    pub segments: usize,
    pub samples: usize,
    pub allocations: usize,
    /// Line samples blended, the toolpath's and the plate's fill work.
    pub blends: usize,
    /// Pixels `frame_into` wrote into a buffer of its own, none when the sample store itself is handed over.
    pub resolved: usize,
    /// Line samples under full cover written as the line's colour with no blend, counted in `blends` too.
    pub solid: usize,
}

/// One frame's buffers at `samples` per pixel side, kept between frames so a moving view allocates nothing.
#[derive(Debug, Default)]
pub struct Raster {
    w: usize,
    h: usize,
    ss: usize,
    /// Four bytes a sample, a packed sample's little-endian bytes: blue, green, red and alpha, the order GPUI's atlas keeps.
    color: Vec<u8>,
    /// Whether the last frame was drawn on an opaque ground, so every sample's alpha is 255.
    opaque: bool,
    depth: Vec<f32>,
    proj: Vec<[f32; 3]>,
    front: Vec<bool>,
    /// This frame's line colours, each with the packed sample a full-cover write gives when that is the same whatever the sample held.
    solids: Vec<([u32; 3], Option<u32>)>,
    pub stats: Stats,
}

// Whether face `f` of the plate starting at triangle `first` faces the viewer; an open edge's missing face never does.
fn faces_front(front: &[bool], f: u32, first: usize) -> bool {
    f != u32::MAX && (f as usize).checked_sub(first).and_then(|k| front.get(k).copied()).unwrap_or(false)
}

// The spacing in millimetres between drawn hairlines across `side`, stepping `across` with lines running `along`: a whole number of cells at least `GRID_MIN_PX` apart on screen and at most `GRID_MAX_LINES` a side; `None` draws only the plate's edge.
fn grid_step(view: &View, basis: &super::camera::Basis, side: f32, across: V3, along: V3, ss: f32) -> Option<f32> {
    if !(side.is_finite() && side > 0.) {
        return None;
    }
    let screen = |v: V3| [view.scale * dot(v, basis.r), view.scale * dot(v, basis.u)];
    let (step, run) = (screen(across), screen(along));
    let length = (run[0] * run[0] + run[1] * run[1]).sqrt();
    let apart = if length > 1e-6 { (step[0] * run[1] - step[1] * run[0]).abs() / length } else { (step[0] * step[0] + step[1] * step[1]).sqrt() } * CELL_MM;
    let cells = (GRID_MIN_PX * ss / apart).max(side / CELL_MM / GRID_MAX_LINES).ceil().max(1.);
    (cells.is_finite() && apart.is_finite() && apart > 0.).then_some(cells * CELL_MM)
}

impl Raster {
    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }

    /// Sizes the buffers for a `w` by `h` frame at `ss` samples per side; the same size reuses every buffer.
    pub fn resize(&mut self, w: usize, h: usize, ss: usize) {
        let ss = ss.clamp(1, 4);
        let n = w * h * ss * ss;
        (self.w, self.h, self.ss) = (w, h, ss);
        self.stats = Stats { samples: n, ..Stats::default() };
        let grows = |cap: usize| usize::from(n > cap);
        self.stats.allocations += grows(self.color.capacity() / 4) + grows(self.depth.capacity());
        self.color.resize(n * 4, 0);
        self.depth.resize(n, f32::NEG_INFINITY);
    }

    /// Hands the buffers back, as a view at rest does once its still is presented.
    pub fn release(&mut self) {
        *self = Self::default();
    }

    /// Draws plate `plate` of `scene` through `view` into the sized buffers.
    pub fn render(&mut self, scene: &Scene, prepared: &Prepared, plate: usize, view: &View, style: &Style) {
        let view = self.sampled(view);
        self.clear(style);
        let basis = view.basis();
        let under = basis.d[2] < 0.;
        if let (Some(rect), false) = (style.plate, under) {
            self.plate(&view, rect, &style.palette, GRID_ALPHA, 1.);
        }
        let range = self.project(scene, prepared, plate, &view, &basis);
        if let Some(edges) = style.wire {
            self.wire(edges, &view, &basis, range.start, style.palette.foreground);
        } else {
            self.faces(scene, prepared, range, &view, style);
            self.toolpath(scene, &view, &basis, style);
        }
        if let (Some(rect), true) = (style.plate, under) {
            self.plate(&view, rect, &style.palette, GRID_UNDER, EDGE_UNDER);
        }
    }

    // The view in sample units.
    fn sampled(&self, view: &View) -> View {
        let ss = self.ss as f32;
        View { scale: view.scale * ss, cx: view.cx * ss, cy: view.cy * ss, ..*view }
    }

    fn clear(&mut self, style: &Style) {
        let ground = style.ground.map_or(0, |g| pack(g, 1.)).to_le_bytes();
        // Sixteen samples a store, which the compiler keeps in vector registers; a sample at a time is twice as slow.
        let block: [u8; 64] = std::array::from_fn(|i| ground[i % 4]);
        let (blocks, samples) = self.color.as_chunks_mut::<64>();
        blocks.iter_mut().for_each(|b| b.copy_from_slice(&block));
        for sample in samples.as_chunks_mut::<4>().0 {
            sample.copy_from_slice(&ground);
        }
        self.opaque = style.ground.is_some();
        self.solids.clear();
        self.depth.fill(f32::NEG_INFINITY);
    }

    // Projects every point and marks which of the plate's faces look at the viewer; answers the plate's triangles.
    fn project(&mut self, scene: &Scene, prepared: &Prepared, plate: usize, view: &View, basis: &super::camera::Basis) -> std::ops::Range<usize> {
        let grown = usize::from(scene.positions.len() > self.proj.capacity()) + usize::from(scene.triangles.len() > self.front.capacity());
        self.stats.allocations += grown;
        self.proj.clear();
        self.proj.extend(scene.positions.iter().map(|&p| view.project(basis, p)));
        let range = scene.plate_triangles(plate);
        self.front.clear();
        self.front.extend(range.clone().map(|t| prepared.normals.get(t).is_some_and(|&n| dot(n, basis.d) > 1e-4)));
        range
    }

    fn faces(&mut self, scene: &Scene, prepared: &Prepared, range: std::ops::Range<usize>, view: &View, style: &Style) {
        let light = view.light();
        let role = tone_table(&Ink::from_lit(style.palette.foreground));
        // A NaN light mixes a NaN tone, which packs to black.
        let unlit = pack([f32::NAN; 3], 1.);
        for (k, t) in range.enumerate() {
            if !self.front[k] {
                continue;
            }
            let tones = match prepared.ink_of.get(t).copied().unwrap_or(0) {
                0 => &role,
                i => prepared.tones.get(i as usize).unwrap_or(&role),
            };
            self.stats.triangles += 1;
            let color = tone_index(dot(prepared.normals[t], light)).map_or(unlit, |step| tones[step]);
            let [a, b, c] = scene.triangles[t].map(|i| self.proj[i as usize]);
            self.triangle(a, b, c, color);
        }
    }

    fn plate(&mut self, view: &View, rect: PlateRect, palette: &Palette, grid: f32, edge: f32) {
        let basis = view.basis();
        let ss = self.ss as f32;
        let seg = |raster: &mut Self, a: V3, b: V3, color: Rgb, alpha: f32| {
            let (p, q) = (view.project(&basis, a), view.project(&basis, b));
            raster.line(p, q, ss, color, alpha, false);
        };
        let (x1, y1) = (rect.x + rect.w, rect.y + rect.h);
        if let Some(step) = grid_step(view, &basis, rect.w, [1., 0., 0.], [0., 1., 0.], ss) {
            for k in 0..=(rect.w / step) as usize {
                let x = rect.x + k as f32 * step;
                seg(self, [x, rect.y, 0.], [x, y1, 0.], palette.muted, grid);
            }
        }
        if let Some(step) = grid_step(view, &basis, rect.h, [0., 1., 0.], [1., 0., 0.], ss) {
            for k in 0..=(rect.h / step) as usize {
                let y = rect.y + k as f32 * step;
                seg(self, [rect.x, y, 0.], [x1, y, 0.], palette.muted, grid);
            }
        }
        let c = rect.corners();
        for i in 0..4 {
            seg(self, c[i], c[(i + 1) % 4], palette.frame, edge);
        }
    }

    fn wire(&mut self, edges: &[Edge], view: &View, basis: &super::camera::Basis, first: usize, ink: Rgb) {
        let ss = self.ss as f32;
        for pass in [false, true] {
            for e in edges {
                let (f1, f2) = (faces_front(&self.front, e.f1, first), faces_front(&self.front, e.f2, first));
                let outline = e.f2 != u32::MAX && f1 != f2;
                if (e.feature || outline) && (f1 || f2) == pass {
                    let (p, q) = (view.project(basis, e.a), view.project(basis, e.b));
                    self.line(p, q, ss, ink, if pass { WIRE_SEEN } else { WIRE_HIDDEN }, false);
                }
            }
        }
    }

    // Edge functions over the triangle's box, sample centres at +0.5, the nearer depth winning.
    fn triangle(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: u32) {
        let (sw, sh) = (self.w * self.ss, self.h * self.ss);
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if area.abs() < 1e-8 || !area.is_finite() {
            return;
        }
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.) as usize;
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil().max(0.) as usize).min(sw);
        let y1 = (a[1].max(b[1]).max(c[1]).ceil().max(0.) as usize).min(sh);
        let inv = 1. / area;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            let row = y * sw;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let w0 = ((c[0] - b[0]) * (py - b[1]) - (c[1] - b[1]) * (px - b[0])) * inv;
                let w1 = ((a[0] - c[0]) * (py - c[1]) - (a[1] - c[1]) * (px - c[0])) * inv;
                let w2 = 1. - w0 - w1;
                if w0 < 0. || w1 < 0. || w2 < 0. {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let at = row + x;
                if z > self.depth[at] {
                    self.depth[at] = z;
                    self.set(at, color);
                }
            }
        }
    }

    // A coverage anti-aliased line walked along its major axis, only a few samples across.
    fn line(&mut self, p: [f32; 3], q: [f32; 3], width: f32, color: Rgb, alpha: f32, depth: bool) {
        let (sw, sh) = (self.w * self.ss, self.h * self.ss);
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        if !(dx.is_finite() && dy.is_finite()) {
            return;
        }
        let steep = dy.abs() > dx.abs();
        let (major, minor, (a0, a1), (b0, b1)) = if steep { (dy, dx, (p[1], q[1]), (p[0], q[0])) } else { (dx, dy, (p[0], q[0]), (p[1], q[1])) };
        let slope = if major.abs() > 1e-6 { minor / major } else { 0. };
        let half = width / 2. * (1. + slope * slope).sqrt();
        let (lo, hi) = (a0.min(a1), a0.max(a1));
        let limit = if steep { sh } else { sw } as f32;
        let across = if steep { sw } else { sh } as f32;
        let solid = if alpha == 1. { self.solid(color) } else { None };
        let mut m = lo.floor().max(0.);
        while m < hi.ceil().min(limit) {
            let centre = m + 0.5;
            let t = if major.abs() > 1e-6 { ((centre - a0) / (a1 - a0)).clamp(0., 1.) } else { 0. };
            let mid = b0 + (b1 - b0) * t;
            let z = p[2] + (q[2] - p[2]) * t;
            let mut n = (mid - half - 1.).floor().max(0.);
            while n <= (mid + half + 1.).ceil().min(across - 1.) {
                let cover = (half + 0.5 - (n + 0.5 - mid).abs()).clamp(0., 1.);
                if cover > 0. {
                    let (x, y) = if steep { (n as usize, m as usize) } else { (m as usize, n as usize) };
                    self.blend(y * sw + x, color, alpha * cover, depth.then_some(z), solid.filter(|_| cover == 1.));
                }
                n += 1.;
            }
            m += 1.;
        }
    }

    // `solid` is the sample a full cover of an opaque line leaves, written as it is.
    fn blend(&mut self, at: usize, color: Rgb, alpha: f32, depth: Option<f32>, solid: Option<u32>) {
        self.stats.blends += 1;
        if let Some(z) = depth {
            if z + LINE_BIAS < self.depth[at] {
                return;
            }
            if alpha >= 0.5 {
                self.depth[at] = z;
            }
        }
        if let Some(p) = solid {
            self.stats.solid += 1;
            self.set(at, p);
            return;
        }
        let (dst, da) = unpack(self.get(at));
        let out = if da == 0. { color } else { mix(dst, color, alpha) };
        self.set(at, pack(out, da + alpha * (1. - da)));
    }
}
