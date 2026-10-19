use super::limits::*;

use super::{number, text, Model, Result, IDENTITY};

// Sample input: solid part\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid part\n.
pub(super) fn stl(bytes: &[u8], model: &mut Model) -> Result<()> {
    if bytes.len() >= 84 {
        let count = super::zip::u32_at(bytes,80)? as usize;
        if count.checked_mul(50).and_then(|n| n.checked_add(84)) == Some(bytes.len()) {
            if count > super::MAX_TRIANGLES { return Err("STL triangle count exceeds its bound".into()) }
            for face in bytes[84..].chunks_exact(50) {
                let mut vertices = [0; 3];
                for (corner,index) in vertices.iter_mut().enumerate() {
                    let mut position = [0.; 3];
                    for (axis,value) in position.iter_mut().enumerate() {
                        let at = 12 + corner*12 + axis*4;
                        *value = f32::from_le_bytes(face[at..at+4].try_into().unwrap());
                    }
                    *index = model.vertex(position,None)?;
                }
                model.triangle(vertices,[None;3],None)?;
            }
            return Ok(());
        }
    }
    let mut lines = text(bytes)?.lines().map(str::trim).filter(|l| !l.is_empty()).peekable();
    while let Some(start) = lines.next() {
        if start.split_whitespace().next() != Some("solid") { return Err("invalid ASCII STL solid".into()) }
        let name = start.strip_prefix("solid").unwrap().trim();
        let first = model.triangles.len();
        let mut ended = false;
        while let Some(line) = lines.next() {
            if line.split_whitespace().next() == Some("endsolid") { ended = true; break }
            let mut normal = line.split_whitespace();
            if normal.next() != Some("facet") || normal.next() != Some("normal") { return Err("invalid STL facet".into()) }
            for _ in 0..3 { number(normal.next().ok_or("truncated STL normal")?)?; }
            if normal.next().is_some() || lines.next() != Some("outer loop") { return Err("invalid STL loop".into()) }
            let mut vertices = [0;3];
            for index in &mut vertices {
                let mut words = lines.next().ok_or("truncated STL vertex")?.split_whitespace();
                if words.next() != Some("vertex") { return Err("invalid STL vertex".into()) }
                let mut position = [0.;3];
                for value in &mut position { *value = number(words.next().ok_or("truncated STL coordinate")?)? as f32 }
                if words.next().is_some() { return Err("invalid STL vertex fields".into()) }
                *index = model.vertex(position,None)?;
            }
            if lines.next() != Some("endloop") || lines.next() != Some("endfacet") { return Err("truncated STL facet".into()) }
            model.triangle(vertices,[None;3],None)?;
        }
        if !ended { return Err("truncated ASCII STL solid".into()) }
        if model.triangles.len() > first { model.object(name,0,first,IDENTITY)? }
    }
    Ok(())
}

fn obj_index(token: &str, count: usize) -> Result<u32> {
    let raw = token.split('/').next().ok_or("missing OBJ vertex")?.parse::<i64>().map_err(|_| "invalid OBJ vertex index")?;
    let index = if raw < 0 { count as i64 + raw } else { raw-1 };
    if raw == 0 || index < 0 || index >= count as i64 { return Err("OBJ vertex index is out of range".into()) }
    Ok(index as u32)
}

// Sample input: v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n.
pub(super) fn obj(bytes: &[u8], model: &mut Model) -> Result<()> {
    let mut name = "Object 1".to_string();
    let mut first = 0;
    let mut material = None;
    for line in text(bytes)?.lines() {
        if line.len() > MAX_LINE_BYTES { return Err("OBJ line exceeds its bound".into()) }
        let mut words = line.split('#').next().unwrap_or("").split_whitespace();
        match words.next() {
            Some("v") => {
                let values: Vec<f64> = words.map(number).collect::<Result<_>>()?;
                if !matches!(values.len(),3|4|6|7) { return Err("invalid OBJ vertex fields".into()) }
                let w = if values.len() == 4 { values[3] } else { 1. };
                if w == 0. { return Err("invalid OBJ homogeneous coordinate".into()) }
                let color = if values.len() >= 6 {
                    if values[3..].iter().any(|v| *v < 0. || *v > 255.) { return Err("OBJ colour is out of range".into()) }
                    let factor = if values[3..].iter().any(|v| *v > 1.) { 1. } else { 255. };
                    Some([(values[3]*factor).round() as u8,(values[4]*factor).round() as u8,(values[5]*factor).round() as u8,
                        if values.len() == 7 { (values[6]*factor).round() as u8 } else { 255 }])
                } else { None };
                model.vertex([values[0] as f32/w as f32,values[1] as f32/w as f32,values[2] as f32/w as f32],color)?;
            }
            Some("f") => {
                let indices: Vec<u32> = words.take(MAX_POLYGON_VERTICES+1).map(|w| obj_index(w,model.vertices.len())).collect::<Result<_>>()?;
                polygon(model,&indices,material)?;
            }
            // Face groups do not divide physical topology; materials remain attached to triangles.
            Some("o") => {
                if model.triangles.len() > first { model.object(&name,0,first,IDENTITY)?; first = model.triangles.len() }
                name = words.collect::<Vec<_>>().join(" ");
            }
            Some("usemtl") => {
                let name = words.collect::<Vec<_>>().join(" ");
                let index = model.materials.iter().position(|m| m.name == name).unwrap_or(model.materials.len());
                if index >= super::MAX_OBJECTS { return Err("too many OBJ materials".into()) }
                if index == model.materials.len() { model.materials.push(super::Material { name, color: None }) }
                material = Some(index as u32);
            }
            // External mtllib and texture paths are never opened by a one-input preview.
            _ => {}
        }
    }
    if model.triangles.len() > first { model.object(&name,0,first,IDENTITY)? }
    Ok(())
}

// Clipping can leave a corner repeating its neighbour or a zero-width spike, and neither holds any area.
fn collapse(left: &mut Vec<usize>, same: impl Fn(usize,usize) -> bool) {
    while left.len() >= 3 {
        let n = left.len();
        let Some(at) = (0..n).find(|&at| { let prev = left[(at+n-1)%n]; same(prev,left[at]) || same(prev,left[(at+1)%n]) }) else { break };
        left.remove(at);
    }
}

// Ear clipping preserves concave OBJ and PLY faces without a dependency or unbounded recursion.
pub(super) fn polygon(model: &mut Model, indices: &[u32], material: Option<u32>) -> Result<()> {
    if indices.len() < 3 || indices.len() > MAX_POLYGON_VERTICES { return Err("polygon size exceeds its bound".into()) }
    if indices.iter().any(|&v| v as usize >= model.vertices.len()) { return Err("polygon vertex is out of range".into()) }
    // Only a repeat of the previous corner or of the first one at the close is dropped; a later revisit is a keyhole bridge or a pinch.
    let position = |i: u32| model.vertices[i as usize].position;
    let mut unique: Vec<u32> = Vec::new();
    for &index in indices {
        if unique.last().map_or(true,|&last| position(last) != position(index)) { unique.push(index) }
    }
    while unique.len() > 1 && position(unique[0]) == position(unique[unique.len()-1]) { unique.pop(); }
    if unique.len() < 3 { return Ok(()) }
    let indices = unique.as_slice();
    let corners: Vec<[f64;3]> = indices.iter().map(|&i| position(i).map(f64::from)).collect();
    let mut normal = [0.;3];
    for i in 0..corners.len() {
        let a = corners[i];
        let b = corners[(i+1)%corners.len()];
        normal[0] += (a[1]-b[1])*(a[2]+b[2]);
        normal[1] += (a[2]-b[2])*(a[0]+b[0]);
        normal[2] += (a[0]-b[0])*(a[1]+b[1]);
    }
    let axis = (0..3).max_by(|&a,&b| normal[a].abs().total_cmp(&normal[b].abs())).unwrap();
    let points: Vec<[f64;2]> = corners.iter().map(|p| [p[(axis+1)%3],p[(axis+2)%3]]).collect();
    let turn = |a: usize,b: usize,c: usize| (points[b][0]-points[a][0])*(points[c][1]-points[a][1]) - (points[b][1]-points[a][1])*(points[c][0]-points[a][0]);
    let same = |a: usize,b: usize| corners[a] == corners[b];
    if normal[axis] == 0. { return Ok(()) }
    let sign = normal[axis].signum();
    let mut left: Vec<usize> = (0..indices.len()).collect();
    loop {
        collapse(&mut left,same);
        if left.len() <= 3 { break }
        let mut found = None;
        for i in 0..left.len() {
            let a = left[(i+left.len()-1)%left.len()];
            let b = left[i];
            let c = left[(i+1)%left.len()];
            if turn(a,b,c)*sign <= 0. { continue }
            // A revisit of the tip blocks the ear only when one of its own edges runs into the ear's angle.
            let inside = |q: usize| turn(a,b,q)*sign > 0. && turn(b,c,q)*sign > 0.;
            let blocks = |at: usize| {
                let p = left[at];
                if p == a || p == b || p == c || same(p,a) || same(p,c) { return false }
                if same(p,b) { return inside(left[(at+left.len()-1)%left.len()]) || inside(left[(at+1)%left.len()]) }
                turn(a,b,p)*sign >= 0. && turn(b,c,p)*sign >= 0. && turn(c,a,p)*sign >= 0.
            };
            if (0..left.len()).any(blocks) { continue }
            found = Some((i,[indices[a],indices[b],indices[c]]));
            break;
        }
        let (i,triangle) = found.ok_or("polygon cannot be triangulated")?;
        model.triangle(triangle,[None;3],material)?;
        left.remove(i);
    }
    if left.len() < 3 { return Ok(()) }
    model.triangle([indices[left[0]],indices[left[1]],indices[left[2]]],[None;3],material)
}
