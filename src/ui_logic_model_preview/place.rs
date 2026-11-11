//! Places a loaded scene at its own origin, and drops a toolpath's travel so the renderer's strides count what it draws.
use super::scene::Scene;
use super::vec::V3;

impl Scene {
    /// Moves x and y so the middle of the points and moves sits at the origin, and keeps the shift in `offset` so the bed follows.
    pub fn recentre(&mut self) {
        let mut lo = [f32::INFINITY; 2];
        let mut hi = [f32::NEG_INFINITY; 2];
        let mut see = |p: &V3| {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        };
        self.positions.iter().for_each(&mut see);
        for s in &self.segments {
            see(&s.a);
            see(&s.b);
        }
        if lo[0] > hi[0] {
            return;
        }
        let c = [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.];
        if c == [0., 0.] {
            return;
        }
        for p in self.positions.iter_mut() {
            p[0] -= c[0];
            p[1] -= c[1];
        }
        for s in self.segments.iter_mut() {
            for p in [&mut s.a, &mut s.b] {
                p[0] -= c[0];
                p[1] -= c[1];
            }
        }
        self.offset = [self.offset[0] + c[0], self.offset[1] + c[1]];
    }

    /// Drops a toolpath's travel moves and renumbers its layers over the extrusions left, so strides count what the renderer draws; a file with no extrusion keeps its moves to frame.
    pub fn keep_extrusions(&mut self) {
        if !self.is_toolpath() || !self.segments.iter().any(|s| s.extrude) {
            return;
        }
        if self.layers.is_empty() {
            self.segments.retain(|s| s.extrude);
            return;
        }
        let mut write = 0usize;
        for layer in self.layers.iter_mut() {
            let start = write;
            for i in layer.segments.start as usize..layer.segments.end as usize {
                if self.segments[i].extrude {
                    self.segments[write] = self.segments[i];
                    write += 1;
                }
            }
            layer.segments = start as u32..write as u32;
        }
        self.segments.truncate(write);
    }
}
