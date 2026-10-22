//! The model a 3D view draws, in millimetres with Z up, plus its plate, bounds, feature edges and live decimation.
use super::vec::{cross, dot, norm, sub, V3};
use std::collections::HashMap;
use std::ops::Range;

/// A mesh over this many triangles is decimated for the live view; the facts keep the original count.
pub const LIVE_TRIANGLES: usize = 2_000_000;
/// The CPU stand-in for the GPU path decimates to this many, the count whose moving frame at the card size its worker measured nearest one 120 Hz frame.
pub const CPU_LIVE_TRIANGLES: usize = 100_000;
/// Positions and moves are refused past the parser's 1e9 mm, which bounds every span at 2e9 mm.
pub const MAX_COORD_MM: f32 = 1e9;
/// A printer's plate may not pass 10 m a side; a slicer's larger value is ignored.
pub const MAX_BED_MM: f32 = 10_000.;
/// Faces meeting at more than this many degrees make a feature edge in the wireframe.
pub const FEATURE_DEGREES: f32 = 25.;
/// The plate's hairline cell.
pub const CELL_MM: f32 = 10.;
/// An STL names no printer, so its plate is its footprint and this many cells around it.
pub const FOOTPRINT_CELLS: f32 = 2.;

/// One toolpath move; travel is kept so a layer's approach stays in its range, and is never drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub a: V3,
    pub b: V3,
    pub extrude: bool,
}

/// A contiguous run of segments printed at one height.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub segments: Range<u32>,
    pub z: f32,
}

/// The render input: positions already transformed, triangles with an optional colour each, plates as triangle ranges.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub positions: Vec<V3>,
    pub triangles: Vec<[u32; 3]>,
    /// One per triangle, or empty when the file carries no colour at all.
    pub colors: Vec<Option<[u8; 3]>>,
    /// Triangle ranges per plate; empty means one plate holding every triangle.
    pub plates: Vec<Range<u32>>,
    pub segments: Vec<Segment>,
    pub layers: Vec<Layer>,
    /// The printer's plate in millimetres when the file names one.
    pub bed: Option<[f32; 2]>,
    /// The x and y that `recentre` took off every position and move; the bed sits at minus this.
    pub offset: [f32; 2],
}

impl Scene {
    pub fn is_toolpath(&self) -> bool {
        self.triangles.is_empty() && !self.segments.is_empty()
    }

    pub fn plate_count(&self) -> usize {
        self.plates.len().max(1)
    }

    /// The triangles plate `plate` holds, every triangle when the file has no plates.
    pub fn plate_triangles(&self, plate: usize) -> Range<usize> {
        match self.plates.get(plate) {
            Some(range) => range.start as usize..(range.end as usize).min(self.triangles.len()),
            None if self.plates.is_empty() => 0..self.triangles.len(),
            None => 0..0,
        }
    }

    /// Every point the view frames: the plate's triangle corners, or the extrusion ends (all ends for a travel-only file).
    pub fn for_each_point(&self, plate: usize, mut each: impl FnMut(V3)) {
        if self.is_toolpath() {
            let extrudes = self.segments.iter().any(|s| s.extrude);
            for s in self.segments.iter().filter(|s| s.extrude || !extrudes) {
                each(s.a);
                each(s.b);
            }
            return;
        }
        for t in &self.triangles[self.plate_triangles(plate)] {
            for &i in t {
                if let Some(&p) = self.positions.get(i as usize) {
                    each(p);
                }
            }
        }
    }

    /// The lowest and highest corner of what the view frames, `None` for an empty plate.
    pub fn bounds(&self, plate: usize) -> Option<(V3, V3)> {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        self.for_each_point(plate, |p| {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        });
        (lo[0] <= hi[0]).then_some((lo, hi))
    }

    /// Whether every index, range, colour and coordinate is in bounds, so nothing drawn from the scene can panic or stall; answers the first fault.
    pub fn check(&self) -> Result<(), &'static str> {
        let near = |p: &V3| p.iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD_MM);
        if !self.positions.iter().all(near) {
            return Err("a coordinate past the parser's 1e9 mm");
        }
        let n = self.positions.len();
        if !self.triangles.iter().all(|t| t.iter().all(|&i| (i as usize) < n)) {
            return Err("an index past the positions");
        }
        if !self.colors.is_empty() && self.colors.len() != self.triangles.len() {
            return Err("colours unlike the triangles");
        }
        if !self.plates.iter().all(|r| r.start <= r.end && r.end as usize <= self.triangles.len()) {
            return Err("a plate past the triangles");
        }
        if !self.segments.iter().all(|s| near(&s.a) && near(&s.b)) {
            return Err("a move past the parser's 1e9 mm");
        }
        if !self.layers.iter().all(|l| l.segments.start <= l.segments.end && l.segments.end as usize <= self.segments.len() && l.z.is_finite()) {
            return Err("a layer past the moves");
        }
        if !self.layers.windows(2).all(|w| w[0].segments.end <= w[1].segments.start) {
            return Err("layers out of order");
        }
        match self.bed {
            Some(b) if !b.iter().all(|&v| v > 0. && v <= MAX_BED_MM) => Err("a plate past 10 m"),
            _ => Ok(()),
        }
    }

    /// The flat-shading normal of triangle `t` from its final positions.
    pub fn normal(&self, t: usize) -> V3 {
        let [a, b, c] = self.triangles[t].map(|i| self.positions[i as usize]);
        norm(cross(sub(b, a), sub(c, a)))
    }
}

/// The plate's rectangle on z = 0: origin and size in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlateRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl PlateRect {
    pub fn corners(&self) -> [V3; 4] {
        let (x0, y0, x1, y1) = (self.x, self.y, self.x + self.w, self.y + self.h);
        [[x0, y0, 0.], [x1, y0, 0.], [x1, y1, 0.], [x0, y1, 0.]]
    }

    pub fn centre(&self) -> [f32; 2] {
        [self.x + self.w / 2., self.y + self.h / 2.]
    }
}

/// Where the plate sits: the printer's whole plate when the file names one, else the footprint and two cells around it; a plate whose objects lie outside the printer's rectangle (a later plate laid beside the first) centres it on them. The bed sits at minus `offset`, the scene's recentring.
pub fn plate_rect(lo: V3, hi: V3, bed: Option<[f32; 2]>, offset: [f32; 2]) -> PlateRect {
    let margin = FOOTPRINT_CELLS * CELL_MM;
    match bed {
        Some([w, h]) => {
            let (x0, y0) = (-offset[0], -offset[1]);
            let inside = lo[0] >= x0 - 0.5 && lo[1] >= y0 - 0.5 && hi[0] <= x0 + w + 0.5 && hi[1] <= y0 + h + 0.5;
            if inside {
                PlateRect { x: x0, y: y0, w, h }
            } else {
                let (cx, cy) = ((lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.);
                PlateRect { x: cx - w / 2., y: cy - h / 2., w, h }
            }
        }
        None => PlateRect { x: lo[0] - margin, y: lo[1] - margin, w: hi[0] - lo[0] + 2. * margin, h: hi[1] - lo[1] + 2. * margin },
    }
}

/// The printer's plate from slicer metadata: PrusaSlicer `bed_shape`, Bambu and Orca `printable_area`, Cura's machine size, sample input ("bed_shape", "0x0,250x0,250x210,0x210") answers [250, 210]; a side past 10 m answers None.
pub fn bed_from_metadata<'a>(metadata: impl IntoIterator<Item = (&'a str, &'a str)>) -> Option<[f32; 2]> {
    let (mut width, mut depth) = (None, None);
    for (key, value) in metadata {
        match key {
            "bed_shape" | "printable_area" => {
                if let Some(size) = polygon_size(value) {
                    return Some(size);
                }
            }
            "machine_width" => width = value.trim().parse::<f32>().ok(),
            "machine_depth" => depth = value.trim().parse::<f32>().ok(),
            _ => {}
        }
    }
    match (width, depth) {
        (Some(w), Some(d)) if w > 0. && d > 0. && w <= MAX_BED_MM && d <= MAX_BED_MM => Some([w, d]),
        _ => None,
    }
}

// Sample input: "0x0,256x0,256x256,0x256" answers [256, 256]; anything malformed answers None.
fn polygon_size(text: &str) -> Option<[f32; 2]> {
    let mut lo = [f32::INFINITY; 2];
    let mut hi = [f32::NEG_INFINITY; 2];
    let mut count = 0;
    for corner in text.split(',') {
        let (x, y) = corner.trim().split_once('x')?;
        let p = [x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?];
        for i in 0..2 {
            if !p[i].is_finite() {
                return None;
            }
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
        count += 1;
    }
    let size = [hi[0] - lo[0], hi[1] - lo[1]];
    (count >= 3 && size.iter().all(|&v| v > 0. && v <= MAX_BED_MM)).then_some(size)
}

/// One mesh edge for the wireframe: its two ends, the faces on either side and whether it is a feature edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub a: V3,
    pub b: V3,
    pub f1: u32,
    /// The second face, `u32::MAX` for an open edge.
    pub f2: u32,
    pub feature: bool,
}

// One welded edge so far: its first ends, the first and second face, and how many faces share it.
type Sides = (u32, u32, u32, u32, u32);

/// Every edge of the plate's triangles after welding identical positions, so an STL's unshared corners still meet; an edge is a feature where its faces meet at more than 25 degrees or it does not join exactly two faces.
pub fn edges(scene: &Scene, plate: usize) -> Vec<Edge> {
    let range = scene.plate_triangles(plate);
    let mut weld: HashMap<[u32; 3], u32> = HashMap::new();
    let mut ids = Vec::with_capacity(scene.positions.len());
    for p in &scene.positions {
        let key = p.map(|c| if c == 0. { 0 } else { c.to_bits() });
        let next = weld.len() as u32;
        ids.push(*weld.entry(key).or_insert(next));
    }
    let mut faces: HashMap<(u32, u32), Sides> = HashMap::new();
    for t in range.clone() {
        let tri = scene.triangles[t];
        for e in 0..3 {
            let (i, j) = (tri[e], tri[(e + 1) % 3]);
            let (wi, wj) = (ids[i as usize], ids[j as usize]);
            let key = if wi < wj { (wi, wj) } else { (wj, wi) };
            let entry = faces.entry(key).or_insert((i, j, t as u32, u32::MAX, 0));
            if entry.4 == 1 {
                entry.3 = t as u32;
            }
            entry.4 += 1;
        }
    }
    let limit = (FEATURE_DEGREES.to_radians()).cos();
    let mut out: Vec<Edge> = faces
        .into_values()
        .map(|(i, j, f1, f2, count)| {
            let feature = count != 2 || dot(scene.normal(f1 as usize), scene.normal(f2 as usize)) < limit;
            Edge {
                a: scene.positions[i as usize],
                b: scene.positions[j as usize],
                f1,
                f2: if count == 2 { f2 } else { u32::MAX },
                feature,
            }
        })
        .collect();
    out.sort_by_key(|e| (e.f1, e.f2));
    out
}

/// The plate's triangles clustered on a grid until no more than `target` remain, colours kept per surviving triangle; the source scene is left whole for its facts; a plate already under the target comes back as it is.
pub fn decimate(scene: &Scene, plate: usize, target: usize) -> Scene {
    if scene.is_toolpath() {
        return scene.clone();
    }
    let range = scene.plate_triangles(plate);
    if range.len() <= target {
        return plate_only(scene, plate);
    }
    let Some((lo, hi)) = scene.bounds(plate) else {
        return plate_only(scene, plate);
    };
    let span = (0..3).map(|i| hi[i] - lo[i]).fold(0f32, f32::max).max(f32::EPSILON);
    let mut cells = ((target as f32).sqrt() * 1.5).max(2.);
    loop {
        let out = cluster(scene, range.clone(), lo, span / cells);
        if out.triangles.len() <= target || cells <= 2. {
            return Scene { bed: scene.bed, offset: scene.offset, ..out };
        }
        cells = (cells * 0.7).floor().max(2.);
    }
}

// The plate's triangles over only the positions they use, identical positions welded so an STL's corners are projected once.
fn plate_only(scene: &Scene, plate: usize) -> Scene {
    let range = scene.plate_triangles(plate);
    let mut weld: HashMap<[u32; 3], u32> = HashMap::new();
    let mut remap = vec![u32::MAX; scene.positions.len()];
    let mut positions = Vec::new();
    let triangles = scene.triangles[range.clone()]
        .iter()
        .map(|t| {
            t.map(|i| {
                let i = i as usize;
                if remap[i] == u32::MAX {
                    let p = scene.positions[i];
                    remap[i] = *weld.entry(p.map(|c| if c == 0. { 0 } else { c.to_bits() })).or_insert_with(|| {
                        positions.push(p);
                        (positions.len() - 1) as u32
                    });
                }
                remap[i]
            })
        })
        .collect();
    Scene {
        positions,
        triangles,
        colors: if scene.colors.is_empty() { Vec::new() } else { scene.colors[range].to_vec() },
        plates: Vec::new(),
        segments: Vec::new(),
        layers: Vec::new(),
        bed: scene.bed,
        offset: scene.offset,
    }
}

// Each vertex moves to its cell's first vertex; triangles that collapse or repeat are dropped.
fn cluster(scene: &Scene, range: Range<usize>, lo: V3, cell: f32) -> Scene {
    let mut ids: HashMap<[i32; 3], u32> = HashMap::new();
    let mut positions = Vec::new();
    let mut remap = vec![u32::MAX; scene.positions.len()];
    let mut triangles = Vec::new();
    let mut colors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in range {
        let tri = scene.triangles[t].map(|i| {
            let i = i as usize;
            if remap[i] == u32::MAX {
                let p = scene.positions[i];
                let key = [0, 1, 2].map(|k| ((p[k] - lo[k]) / cell).floor() as i32);
                remap[i] = *ids.entry(key).or_insert_with(|| {
                    positions.push(p);
                    (positions.len() - 1) as u32
                });
            }
            remap[i]
        });
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
            continue;
        }
        let mut key = tri;
        key.sort_unstable();
        if !seen.insert(key) {
            continue;
        }
        triangles.push(tri);
        if !scene.colors.is_empty() {
            colors.push(scene.colors[t]);
        }
    }
    Scene { positions, triangles, colors, ..Scene::default() }
}
