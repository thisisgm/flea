use super::{xml, Model, Result, Transform, IDENTITY};
use std::collections::BTreeMap;

pub(super) fn scale(unit: &str) -> Result<f64> {
    match unit { "micron"|"micrometer" => Ok(0.001), "millimeter"|"" => Ok(1.), "centimeter" => Ok(10.), "meter" => Ok(1000.), "inch" => Ok(25.4), "foot" => Ok(304.8), _ => Err("unsupported 3D unit".into()) }
}

fn color(doc: &xml::Document, node: usize) -> Result<Option<super::Color>> {
    let Some(c) = doc.child(node,"color") else { return Ok(None) };
    let mut color = [255;4];
    for (i,key) in ["r","g","b","a"].iter().enumerate() {
        if i == 3 && doc.child(c,key).is_none() { continue }
        let value = doc.value(c,key)?;
        if !(0. ..=1.).contains(&value) { return Err("AMF colour is out of range".into()) }
        color[i] = (value*255.).round() as u8;
    }
    Ok(Some(color))
}

pub(super) fn transformed(point: super::Point, matrix: Transform) -> super::Point {
    std::array::from_fn(|axis| (matrix[axis]*f64::from(point[0]) + matrix[3+axis]*f64::from(point[1]) + matrix[6+axis]*f64::from(point[2]) + matrix[9+axis]) as f32)
}

pub(super) fn compose(outer: Transform, inner: Transform) -> Transform {
    let mut out = [0.;12];
    for col in 0..3 { for row in 0..3 { out[col*3+row] = (0..3).map(|k| outer[k*3+row]*inner[col*3+k]).sum() } }
    for row in 0..3 { out[9+row] = outer[9+row] + (0..3).map(|k| outer[k*3+row]*inner[9+k]).sum::<f64>() }
    out
}

fn instance_transform(doc: &xml::Document, i: usize, unit: f64) -> Result<Transform> {
    let mut transform = IDENTITY;
    for (axis,key) in ["rx","ry","rz"].iter().enumerate() {
        let angle = if doc.child(i,key).is_some() { doc.value(i,key)?.to_radians() } else { 0. };
        let (s,c) = angle.sin_cos();
        let mut rotation = IDENTITY;
        let (a,b) = ((axis+1)%3,(axis+2)%3);
        rotation[a*3+a] = c; rotation[b*3+b] = c;
        rotation[a*3+b] = s; rotation[b*3+a] = -s;
        transform = compose(rotation,transform);
    }
    for (axis,key) in ["deltax","deltay","deltaz"].iter().enumerate() {
        if doc.child(i,key).is_some() { transform[9+axis] = doc.value(i,key)?*unit }
    }
    Ok(transform)
}

// Sample input: <amf unit="millimeter"><object id="1"><mesh><vertices/><volume/></mesh></object></amf>.
pub(super) fn parse(bytes: &[u8], model: &mut Model) -> Result<()> {
    let archive;
    let bytes = if bytes.starts_with(b"PK") {
        archive = super::zip::unpack(bytes)?;
        archive.iter().find(|(n,_)| n.to_ascii_lowercase().ends_with(".amf")).map(|(_,v)| v.as_slice()).ok_or("AMF archive has no model")?
    } else { bytes };
    let doc = xml::parse(super::text(bytes)?)?;
    if doc.nodes[doc.root].name != "amf" { return Err("AMF root is missing".into()) }
    let unit = scale(doc.nodes[doc.root].attr("unit").unwrap_or("millimeter"))?;
    let mut materials = BTreeMap::new();
    for i in doc.children(doc.root,"material") {
        let id = doc.nodes[i].index("id")?;
        let name = doc.children(i,"metadata").find(|&j| doc.nodes[j].attr("type") == Some("name")).map(|j| doc.nodes[j].text.clone()).unwrap_or_else(|| format!("Material {id}"));
        if model.materials.len() >= super::MAX_OBJECTS { return Err("too many AMF materials".into()) }
        if materials.insert(id,model.materials.len() as u32).is_some() { return Err("duplicate AMF material".into()) }
        model.materials.push(super::Material { name, color: color(&doc,i)? });
    }
    let mut objects = BTreeMap::new();
    for i in doc.children(doc.root,"object") {
        if objects.insert(doc.nodes[i].index("id")?,i).is_some() { return Err("duplicate AMF object".into()) }
    }
    let mut instances = Vec::new();
    for c in doc.children(doc.root,"constellation") {
        for i in doc.children(c,"instance") {
            if instances.len() >= super::MAX_OBJECTS { return Err("too many AMF instances".into()) }
            instances.push((doc.nodes[i].index("objectid")?,instance_transform(&doc,i,unit)?));
        }
    }
    if instances.is_empty() { instances.extend(objects.keys().map(|&id| (id,IDENTITY))) }
    for (id,transform) in instances {
        let i = *objects.get(&id).ok_or("missing AMF instance object")?;
        let object_color = color(&doc,i)?;
        let mesh = doc.child(i,"mesh").ok_or("AMF object has no mesh")?;
        let vertices = doc.child(mesh,"vertices").ok_or("AMF vertices are missing")?;
        let mut indices = Vec::new();
        for v in doc.children(vertices,"vertex") {
            let p = doc.child(v,"coordinates").ok_or("AMF vertex has no coordinates")?;
            let position = [doc.value(p,"x")?*unit,doc.value(p,"y")?*unit,doc.value(p,"z")?*unit].map(|v| v as f32);
            indices.push(model.vertex(transformed(position,transform),color(&doc,v)?)?);
        }
        let first = model.triangles.len();
        for volume in doc.children(mesh,"volume") {
            let material = doc.nodes[volume].attr("materialid").map(|s| s.parse::<u32>().map_err(|_| "invalid AMF material id")).transpose()?
                .map(|id| materials.get(&id).copied().ok_or("missing AMF material")).transpose()?;
            let volume_color = color(&doc,volume)?.or(object_color).or_else(|| material.and_then(|m| model.materials[m as usize].color));
            for t in doc.children(volume,"triangle") {
                let mut face = [0;3];
                for (axis,key) in ["v1","v2","v3"].iter().enumerate() {
                    let n = doc.value(t,key)?;
                    if n < 0. || n.fract() != 0. { return Err("invalid AMF triangle index".into()) }
                    face[axis] = *indices.get(n as usize).ok_or("AMF triangle index is out of range")?;
                }
                let triangle_color = color(&doc,t)?;
                let colors = face.map(|v| triangle_color.or(model.vertices[v as usize].color).or(volume_color));
                model.triangle(face,colors,material)?;
            }
        }
        let name = doc.children(i,"metadata").find(|&j| doc.nodes[j].attr("type") == Some("name")).map(|j| doc.nodes[j].text.as_str()).unwrap_or("Object");
        model.object(name,0,first,transform)?;
    }
    Ok(())
}
