use super::{Facts, Model, Point};

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1]*b[2] - a[2]*b[1], a[2]*b[0] - a[0]*b[2], a[0]*b[1] - a[1]*b[0]]
}

fn position(p: Point) -> [f64; 3] { p.map(f64::from) }

// Exact coordinate keys weld STL's repeated vertices, including the two spellings of zero.
fn key(p: Point) -> [u32; 3] { p.map(|v| if v == 0. { 0 } else { v.to_bits() }) }

pub(super) fn measure(model: &Model, plate: Option<usize>) -> Facts {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut include = |p: Point| {
        for axis in 0..3 {
            min[axis] = min[axis].min(f64::from(p[axis]));
            max[axis] = max[axis].max(f64::from(p[axis]));
        }
    };
    let mut facts = Facts { plates: model.plates.len(), ..Facts::default() };
    let mut surface = 0.;
    let mut volume = 0.;
    let mut open = 0;
    for object in &model.objects {
        if plate.is_some_and(|p| p != object.plate as usize) { continue }
        facts.objects += 1;
        let mut signed_volume = 0.;
        let mut edges = Vec::with_capacity(object.triangle_count as usize * 3);
        let first = object.first_triangle as usize;
        for triangle in &model.triangles[first..first + object.triangle_count as usize] {
            let points = triangle.vertices.map(|v| model.vertices[v as usize].position);
            for p in points { include(p) }
            let [a,b,c] = points.map(position);
            let ab = std::array::from_fn(|i| b[i] - a[i]);
            let ac = std::array::from_fn(|i| c[i] - a[i]);
            let normal = cross(ab, ac);
            surface += normal.iter().map(|v| v*v).sum::<f64>().sqrt() / 2.;
            let bc = cross(b, c);
            signed_volume += a.iter().zip(bc).map(|(a,b)| a*b).sum::<f64>() / 6.;
            let keys = points.map(key);
            for (a,b) in [(0,1), (1,2), (2,0)] {
                edges.push(if keys[a] < keys[b] { (keys[a], keys[b]) } else { (keys[b], keys[a]) });
            }
            facts.triangles += 1;
        }
        edges.sort_unstable();
        let mut at = 0;
        while at < edges.len() {
            let mut end = at + 1;
            while end < edges.len() && edges[end] == edges[at] { end += 1 }
            if end - at != 2 || edges[at].0 == edges[at].1 { open += 1 }
            at = end;
        }
        volume += signed_volume.abs();
    }
    let has_extrusion = model.segments.iter().any(|s| s.extrusion_mm > 0.);
    for segment in &model.segments {
        if has_extrusion && segment.extrusion_mm <= 0. { continue }
        include(segment.start);
        include(segment.end);
    }
    facts.dimensions_mm = std::array::from_fn(|i| if min[i].is_finite() { max[i] - min[i] } else { 0. });
    if facts.triangles > 0 {
        facts.surface_cm2 = Some(surface / 100.);
        facts.open_edges = Some(open);
        facts.volume_cm3 = (open == 0).then_some(volume / 1000.);
    }
    facts
}
